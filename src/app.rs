//! Shared application bootstrap for the viewer and editor binaries.
//!
//! Both binaries parse the same command line arguments, locate the same
//! archives and run the same core systems. Keeping that here stops the two
//! bootstraps drifting apart; the editor previously scheduled rendering in
//! `Update` instead of `PostUpdate`.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use bevy::asset::io::AssetSourceBuilder;
use bevy::camera::CameraUpdateSystems;
use bevy::camera::visibility::RenderLayers;
use bevy::prelude::*;
use bevy::transform::TransformSystems;
use bevy_egui::{EguiContext, EguiGlobalSettings, EguiPlugin, PrimaryEguiContext};
use pico_args::Arguments;

use crate::assets::{
    DayLightTable, GroupVisibility, JarAssetSource, MapSpriteLibrary, TgamLoader, TgamMaskLibrary,
    open_archive,
};
use crate::camera::{
    CameraController, MainCamera, OverlayCamera, OVERLAY_LAYER, WORLD_LAYER,
    camera_controller_system, camera_system,
};
use crate::premult_alpha;
use crate::render::{
    ChangeMapEvent, GroupVisibilityState, MapCatalog, MapRenderer, PendingMapLoad,
    animation_system, day_light_system, group_fade_system, lighting_system, load_map_system,
    load_renderer, rendering_system, sea_system,
};
use crate::settings::MapViewSettings;

/// The outcome of parsing the shared command line arguments.
pub enum Cli {
    /// `--help` was requested; the help text has already been printed.
    Help,
    /// A runnable configuration.
    Run(CliArgs),
}

/// Command line arguments shared by both binaries.
pub struct CliArgs {
    /// Path to the `contents/maps` directory of a Wakfu installation.
    pub maps_path: PathBuf,
}

/// Parses the shared `--path` argument.
///
/// The map is chosen from the in-app selector, so there is deliberately no
/// `--map` option. Prints `help` when requested or on error, and rejects
/// unexpected trailing arguments.
pub fn parse_args(help: &str) -> anyhow::Result<Cli> {
    parse_args_from(help, std::env::args_os())
}

/// [`parse_args`] over an explicit argument vector, so the accepted command
/// line can be tested without touching the process environment.
///
/// `args` includes the executable name, exactly like [`std::env::args_os`].
fn parse_args_from<I, T>(help: &str, args: I) -> anyhow::Result<Cli>
where
    I: IntoIterator<Item = T>,
    T: Into<std::ffi::OsString>,
{
    // `Arguments::from_vec` expects the executable to already be removed.
    let args: Vec<std::ffi::OsString> = args.into_iter().map(Into::into).skip(1).collect();
    let mut pargs = Arguments::from_vec(args);

    if pargs.contains(["-h", "--help"]) {
        print!("{help}");
        return Ok(Cli::Help);
    }

    let Some(game_path) = pargs.opt_value_from_str::<_, PathBuf>("--path")? else {
        eprint!("{help}");
        bail!("missing required option '--path'");
    };

    let remaining = pargs.finish();
    if !remaining.is_empty() {
        let args: Vec<String> = remaining
            .into_iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        eprint!("{help}");
        bail!("unexpected arguments: {}", args.join(", "));
    }

    Ok(Cli::Run(CliArgs {
        maps_path: game_path.join("contents").join("maps"),
    }))
}

/// Lists the map ids available under `maps_path/gfx`.
pub fn get_map_list(maps_path: &Path) -> anyhow::Result<Vec<i32>> {
    let mut map_ids: Vec<i32> = fs::read_dir(maps_path.join("gfx"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.extension() == Some("jar".as_ref()))
        .filter_map(|entry| entry.file_stem()?.to_str()?.parse::<i32>().ok())
        .collect();
    map_ids.sort_unstable();
    map_ids.dedup();

    Ok(map_ids)
}

/// The map data and resources both binaries need to start up.
pub struct LoadedWorld {
    pub catalog: MapCatalog,
    pub renderer: MapRenderer,
    pub camera_controller: CameraController,
    pub asset_source: JarAssetSource,
    pub group_visibility: Option<GroupVisibility>,
    pub masks: TgamMaskLibrary,
    pub day_light: DayLightTable,
}

impl LoadedWorld {
    /// Loads the sprite library, day/night definitions, asset archive and the
    /// first available map.
    ///
    /// The startup map can be changed from the in-app map selector, so no map
    /// id is taken from the command line. `editor_mode` only changes how the
    /// camera controller behaves; the editor-specific systems are added by the
    /// editor binary.
    pub fn load(maps_path: PathBuf, editor_mode: bool) -> anyhow::Result<Self> {
        let available_maps = get_map_list(&maps_path).map_err(|err| {
            anyhow::anyhow!(
                "could not read maps directory {}: {err}",
                maps_path.join("gfx").display()
            )
        })?;
        if available_maps.is_empty() {
            bail!("no map files found in {}", maps_path.join("gfx").display());
        }
        let map_id = available_maps[0];

        let lib_path = maps_path.join("data.jar");
        let mut maps_archive = open_archive(&lib_path).map_err(|err| {
            anyhow::anyhow!(
                "failed to open map data archive {}: {err}",
                lib_path.display()
            )
        })?;
        let sprites = Arc::new(MapSpriteLibrary::load_archive(&mut maps_archive)?);
        let day_light = load_day_light(&maps_path)?;

        let gfx_path = maps_path.join("gfx.jar");
        let asset_source = JarAssetSource::new(&gfx_path).map_err(|err| {
            anyhow::anyhow!("failed to open asset archive {}: {err}", gfx_path.display())
        })?;

        let masks = TgamMaskLibrary::default();
        let (renderer, group_visibility) = load_renderer(&maps_path, map_id, &sprites, &masks)?;
        let center = renderer.center();
        let camera_controller = CameraController {
            target_position: Vec3::new(center.x, center.y, 0.0),
            current_position: Vec3::new(center.x, center.y, 0.0),
            editor_mode,
            ..CameraController::default()
        };
        let catalog = MapCatalog {
            maps_path,
            sprites,
            available_maps,
            current_map_id: map_id,
        };

        Ok(Self {
            catalog,
            renderer,
            camera_controller,
            asset_source,
            group_visibility,
            masks,
            day_light,
        })
    }
}

/// Loads the day/night sun colours the client uses.
///
/// `defaultDayLightFile` points at `contents/data/data.jar!/dayLight.xml`; note
/// that this is the game data jar, not `contents/maps/data.jar` (which holds
/// `elements.lib`). A loose `dayLight.xml` beside `config.properties` takes
/// precedence, and a missing archive is tolerated.
fn load_day_light(maps_path: &Path) -> anyhow::Result<DayLightTable> {
    let loose = maps_path
        .parent()
        .and_then(Path::parent)
        .map(|root| root.join("dayLight.xml"));
    let data_path = maps_path
        .parent()
        .map(|contents| contents.join("data").join("data.jar"));
    let mut archive = match &data_path {
        Some(path) if path.is_file() => Some(open_archive(path).map_err(|err| {
            anyhow::anyhow!("failed to open data archive {}: {err}", path.display())
        })?),
        _ => None,
    };

    let table = DayLightTable::load(archive.as_mut(), loose.as_deref())
        .map_err(|err| anyhow::anyhow!("failed to load day-light definitions: {err}"))?;
    if table.is_empty() {
        warn!("no dayLight.xml found in contents/data/data.jar; using the default sun colour");
    }
    Ok(table)
}

/// Shared plugins, resources and systems for both binaries.
///
/// Binary-specific systems (the settings UI, editor tools) are added by the
/// caller. The initial [`MapRenderer`] is inserted separately by [`build_app`]
/// because it is not worth cloning.
pub struct VakfuCorePlugin {
    pub window_title: String,
    pub catalog: MapCatalog,
    pub camera_controller: CameraController,
    pub asset_source: JarAssetSource,
    pub group_visibility: Option<GroupVisibility>,
    pub masks: TgamMaskLibrary,
    pub day_light: DayLightTable,
}

impl Plugin for VakfuCorePlugin {
    fn build(&self, app: &mut App) {
        let asset_source = self.asset_source.clone();
        app.register_asset_source(
            "gfx",
            AssetSourceBuilder::new(move || Box::new(asset_source.clone())),
        )
        .add_plugins(DefaultPlugins.set(WindowPlugin {
            primary_window: Some(Window {
                title: self.window_title.clone(),
                ..default()
            }),
            ..default()
        }))
        .add_plugins(premult_alpha::PremultAlphaPlugin)
        .add_plugins(EguiPlugin::default())
        // The editor spawns a second camera for its overlay layer; without this
        // bevy_egui could attach the primary context to either camera.
        .insert_resource(EguiGlobalSettings {
            auto_create_primary_context: false,
            ..default()
        })
        .register_asset_loader(TgamLoader::new(self.masks.clone()))
        .insert_resource(self.masks.clone())
        .insert_resource(self.day_light.clone())
        .add_message::<ChangeMapEvent>()
        .insert_resource(self.catalog.clone())
        .insert_resource(self.camera_controller.clone())
        .init_resource::<PendingMapLoad>()
        .init_resource::<MapViewSettings>()
        .insert_resource(GroupVisibilityState(self.group_visibility.clone()))
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                load_map_system,
                camera_controller_system,
                camera_system,
                sync_overlay_camera,
            )
                .chain(),
        )
        .add_systems(
            PostUpdate,
            (
                day_light_system,
                group_fade_system,
                lighting_system,
                rendering_system,
                animation_system,
                sea_system,
            )
                .chain()
                .after(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        );
    }
}

/// Builds an [`App`] with the shared core plugin and the initial map renderer.
pub fn build_app(window_title: &str, world: LoadedWorld) -> App {
    let mut app = App::new();
    app.insert_resource(world.renderer)
        .add_plugins(VakfuCorePlugin {
            window_title: window_title.to_owned(),
            catalog: world.catalog,
            camera_controller: world.camera_controller,
            asset_source: world.asset_source,
            group_visibility: world.group_visibility,
            masks: world.masks,
            day_light: world.day_light,
        });
    app
}

/// Spawns the world camera and the editor overlay camera at the controller's
/// initial position.
fn setup(mut commands: Commands<'_, '_>, camera_controller: Res<'_, CameraController>) {
    let transform = Transform::from_translation(camera_controller.current_position);
    commands.spawn((
        Camera2d,
        CompositingSpace::Linear,
        MainCamera,
        RenderLayers::layer(WORLD_LAYER),
        transform,
    ));
    // Drawn after the world camera with its own sorted pass, so overlay depth
    // is independent of map sprite Z. It shares the main camera's transform.
    // Egui renders into this pass too, keeping the UI above the overlays.
    commands.spawn((
        Camera2d,
        CompositingSpace::Linear,
        OverlayCamera,
        PrimaryEguiContext,
        EguiContext::default(),
        Camera {
            order: 1,
            clear_color: ClearColorConfig::None,
            ..default()
        },
        RenderLayers::layer(OVERLAY_LAYER),
        transform,
    ));
}

/// Keeps the overlay camera aligned with the world camera every frame.
///
/// Only the local `Transform` is copied; Bevy's `TransformSystems::Propagate`
/// updates the `GlobalTransform` in `PostUpdate` like any other entity.
fn sync_overlay_camera(
    main: Query<'_, '_, &Transform, (With<MainCamera>, Without<OverlayCamera>)>,
    mut overlay: Query<'_, '_, &mut Transform, With<OverlayCamera>>,
) {
    let Ok(main_transform) = main.single() else {
        return;
    };
    let Ok(mut transform) = overlay.single_mut() else {
        return;
    };
    transform.set_if_neq(*main_transform);
}

/// Run condition: the primary egui context exists.
pub fn egui_has_primary_context(
    query: Query<'_, '_, &bevy_egui::EguiContext, With<PrimaryEguiContext>>,
) -> bool {
    !query.is_empty()
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELP: &str = "usage: vakfu --path <PATH>\n";

    #[test]
    fn parses_the_maps_path() {
        let Ok(Cli::Run(args)) = parse_args_from(HELP, ["vakfu", "--path", "/game"]) else {
            panic!("expected a runnable configuration");
        };
        assert_eq!(args.maps_path, PathBuf::from("/game/contents/maps"));
    }

    #[test]
    fn requires_the_path_option() {
        assert!(parse_args_from(HELP, ["vakfu"]).is_err());
    }

    #[test]
    fn rejects_the_removed_map_option() {
        // Maps are picked in the UI now, so `--map` is an unknown argument.
        assert!(parse_args_from(HELP, ["vakfu", "--path", "/game", "--map", "5"]).is_err());
    }

    #[test]
    fn help_short_circuits_parsing() {
        assert!(matches!(
            parse_args_from(HELP, ["vakfu", "--help"]),
            Ok(Cli::Help)
        ));
    }
}
