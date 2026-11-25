//! Shared application bootstrap for the viewer and editor binaries.
//!
//! Both binaries parse the same command line arguments, locate the same
//! archives and run the same core systems. Keeping that here stops the two
//! bootstraps drifting apart; the editor previously scheduled rendering in
//! `Update` instead of `PostUpdate`.

use std::fs;
use std::fs::File;
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

use crate::assets::{JarAssetSource, MapSpriteLibrary, TgamLoader};
use crate::camera::{
    CameraController, MainCamera, OverlayCamera, OVERLAY_LAYER, WORLD_LAYER,
    camera_controller_system, camera_system,
};
use crate::premult_alpha;
use crate::render::{
    ChangeMapEvent, MapCatalog, MapRenderer, PendingMapLoad, animation_system, lighting_system,
    load_map_system, load_renderer, rendering_system,
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
    /// The requested map id, if any.
    pub map_id: Option<i32>,
}

/// Parses the shared `--path` / `--map` arguments.
///
/// Prints `help` when requested or on error, and rejects unexpected trailing
/// arguments.
pub fn parse_args(help: &str) -> anyhow::Result<Cli> {
    let mut pargs = Arguments::from_env();

    if pargs.contains(["-h", "--help"]) {
        print!("{help}");
        return Ok(Cli::Help);
    }

    let Some(game_path) = pargs.opt_value_from_str::<_, PathBuf>("--path")? else {
        eprint!("{help}");
        bail!("missing required option '--path'");
    };
    let map_id = pargs.opt_value_from_str("--map")?;

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
        map_id,
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
}

impl LoadedWorld {
    /// Loads the sprite library, asset archive and initial map.
    ///
    /// `editor_mode` only changes how the camera controller behaves; the
    /// editor-specific systems are added by the editor binary.
    pub fn load(
        maps_path: PathBuf,
        requested_map: Option<i32>,
        editor_mode: bool,
    ) -> anyhow::Result<Self> {
        let available_maps = get_map_list(&maps_path).map_err(|err| {
            anyhow::anyhow!(
                "could not read maps directory {}: {err}",
                maps_path.join("gfx").display()
            )
        })?;
        if available_maps.is_empty() {
            bail!("no map files found in {}", maps_path.join("gfx").display());
        }
        let map_id = match requested_map {
            Some(id) if available_maps.contains(&id) => id,
            Some(id) => bail!("map {id} not found in {}", maps_path.join("gfx").display()),
            None => available_maps[0],
        };

        let lib_path = maps_path.join("data.jar");
        let sprites = Arc::new(MapSpriteLibrary::load(File::open(&lib_path).map_err(
            |err| {
                anyhow::anyhow!(
                    "failed to open sprite library {}: {err}",
                    lib_path.display()
                )
            },
        )?)?);

        let gfx_path = maps_path.join("gfx.jar");
        let asset_source = JarAssetSource::new(&gfx_path).map_err(|err| {
            anyhow::anyhow!("failed to open asset archive {}: {err}", gfx_path.display())
        })?;

        let renderer = load_renderer(&maps_path, map_id, &sprites)?;
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
        })
    }
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
        .init_asset_loader::<TgamLoader>()
        .add_message::<ChangeMapEvent>()
        .insert_resource(self.catalog.clone())
        .insert_resource(self.camera_controller.clone())
        .init_resource::<PendingMapLoad>()
        .init_resource::<MapViewSettings>()
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (load_map_system, camera_controller_system, camera_system).chain(),
        )
        .add_systems(
            PostUpdate,
            (lighting_system, rendering_system, animation_system)
                .chain()
                .after(CameraUpdateSystems)
                .before(TransformSystems::Propagate),
        )
        .add_systems(PostUpdate, sync_overlay_camera.before(CameraUpdateSystems));
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
fn sync_overlay_camera(
    main: Query<'_, '_, &Transform, (With<MainCamera>, Without<OverlayCamera>)>,
    mut overlay: Query<'_, '_, (&mut Transform, &mut GlobalTransform), With<OverlayCamera>>,
) {
    let Ok(main_transform) = main.single() else {
        return;
    };
    let Ok((mut transform, mut global_transform)) = overlay.single_mut() else {
        return;
    };
    transform.set_if_neq(*main_transform);
    global_transform.set_if_neq(GlobalTransform::from(*main_transform));
}

/// Run condition: the primary egui context exists.
pub fn egui_has_primary_context(
    query: Query<'_, '_, &bevy_egui::EguiContext, With<PrimaryEguiContext>>,
) -> bool {
    !query.is_empty()
}
