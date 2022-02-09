use std::fs::File;
use std::path::PathBuf;

use anyhow::Result;
use assets::jar::JarAssetIo;
use assets::tgam::TgamLoader;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use map::element::ElementLibrary;
use map::Map;
use pico_args::Arguments;
use systems::camera::CameraController;

mod assets;
mod map;
mod systems;

fn main() -> Result<()> {
    let mut pargs = Arguments::from_env();

    let game_path: PathBuf = pargs.value_from_str("--path")?;
    let map: i32 = pargs.value_from_str("--map")?;

    let maps_path = game_path.join("contents").join("maps");
    let gfx_path = maps_path.join("gfx.jar");
    let map_path = maps_path.join("gfx").join(format!("{}.jar", map));
    let lib_path = maps_path.join("data.jar");

    let map = Map::load(File::open(map_path)?)?;
    let lib = ElementLibrary::load(File::open(lib_path)?)?;

    App::new()
        .add_plugins_with(DefaultPlugins, |group| {
            group.add_before::<bevy::asset::AssetPlugin, _>(JarAssetIo::plugin(gfx_path))
        })
        .add_plugin(LogDiagnosticsPlugin::default())
        .add_plugin(FrameTimeDiagnosticsPlugin::default())
        .init_asset_loader::<TgamLoader>()
        .insert_resource(CameraController::default())
        .insert_resource(lib)
        .insert_resource(map)
        .add_startup_system(systems::setup::setup_system)
        .add_system(systems::camera::camera_controller_system)
        .add_system(systems::camera::camera_system)
        .add_system(systems::render::map_chunk_view_system)
        .add_system(systems::render::animation_system)
        .run();

    Ok(())
}
