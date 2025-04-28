use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};

use anyhow::bail;
use assets::{JarAssetSource, Map, MapSpriteLibrary, TgamLoader};
use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;
use bevy::window::PrimaryWindow;
use bevy_egui::{EguiContextPass, EguiPlugin};
use camera::{CameraController, camera_controller_system, camera_system};
use pico_args::Arguments;
use render::{MapRenderer, animation_system, rendering_system};
use settings::{MapViewSettings, settings_ui_system};

#[allow(unused)]
mod assets;
mod camera;
mod render;
mod settings;
mod util;

fn main() -> anyhow::Result<()> {
    let mut pargs = Arguments::from_env();

    let game_path: PathBuf = pargs.value_from_str("--path")?;
    let maps_path = game_path.join("contents").join("maps");

    let map_id = match pargs.value_from_str::<&str, i32>("--map") {
        Ok(id) => id,
        Err(err) => match get_map_list(&maps_path) {
            Ok(map_ids) => bail!(
                "no map specified ({err}), you can try one of the following:\n{}",
                map_ids.join(", ")
            ),
            Err(_) => bail!("no map specified ({err}) and could not find anyh"),
        },
    };

    let asset_source = JarAssetSource::new(maps_path.join("gfx.jar"))?;
    let renderer = load_renderer(&maps_path, map_id)?;

    App::new()
        .register_asset_source(
            "gfx",
            AssetSourceBuilder::default().with_reader(move || Box::new(asset_source.clone())),
        )
        .add_plugins(DefaultPlugins)
        .add_plugins(EguiPlugin {
            enable_multipass_for_primary_context: true,
        })
        .init_asset_loader::<TgamLoader>()
        .insert_resource(renderer)
        .insert_resource(CameraController::default())
        .init_resource::<MapViewSettings>()
        .add_systems(Startup, setup)
        .add_systems(
            EguiContextPass,
            settings_ui_system.run_if(egui_has_primary_context),
        )
        .add_systems(
            Update,
            (
                (camera_controller_system, camera_system, rendering_system).chain(),
                animation_system,
            ),
        )
        .run();

    Ok(())
}

fn load_renderer(maps_path: &Path, map_id: i32) -> anyhow::Result<MapRenderer> {
    let map_path = maps_path.join("gfx").join(format!("{}.jar", map_id));
    let lib_path = maps_path.join("data.jar");

    let map = Map::load(File::open(map_path)?)?;
    let sprites = MapSpriteLibrary::load(File::open(lib_path)?)?;
    Ok(MapRenderer::new(&map, &sprites))
}

fn setup(mut commands: Commands<'_, '_>) {
    commands.spawn(Camera2d);
}

fn egui_has_primary_context(
    query: Query<'_, '_, &bevy_egui::EguiContext, With<PrimaryWindow>>,
) -> bool {
    !query.is_empty()
}

fn get_map_list(maps_path: &Path) -> Result<Vec<String>> {
    let mut map_ids: Vec<String> = fs::read_dir(maps_path.join("gfx"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|entry| entry.extension() == Some("jar".as_ref()))
        .filter_map(|entry| Some(entry.file_stem()?.to_str()?.to_owned()))
        .collect();
    map_ids.sort();

    Ok(map_ids)
}
