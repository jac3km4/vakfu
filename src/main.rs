use std::fs::File;
use std::path::PathBuf;

use anyhow::{anyhow, Result};
use assets::jar::JarAssetIo;
use assets::tgam::TgamLoader;
use bevy::diagnostic::{FrameTimeDiagnosticsPlugin, LogDiagnosticsPlugin};
use bevy::prelude::*;
use bevy_egui::EguiPlugin;
use map::element::ElementLibrary;
use map::Map;
use pico_args::{Arguments, Error};
use systems::camera::{camera_controller_system, camera_system, CameraController};
use systems::navigation::NavigationInfo;
use systems::render::{animation_system, map_chunk_view_system, visibility_system};
use systems::settings::{settings_system, Settings};
use systems::setup::setup_system;
use systems::ui::ui_system;
use utils::id::get_map_ids;

mod assets;
mod map;
mod systems;
mod utils;

fn main() -> Result<()> {
    let mut pargs = Arguments::from_env();

    let game_path: PathBuf = pargs.value_from_str("--path")?;

    let maps_path = game_path.join("contents").join("maps");
    let gfx_path = maps_path.join("gfx.jar");

    let base_map_gfx_path = maps_path.join("gfx");

    match pargs.value_from_str::<&str, i32>("--map") {
        Ok(map_arg) => {
            let map_path = base_map_gfx_path.join(format!("{}.jar", map_arg));
            let lib_path = maps_path.join("data.jar");

            let map = Map::load(File::open(map_path)?)?;
            let lib = ElementLibrary::load(File::open(lib_path)?)?;

            let map_ids = get_map_ids(&base_map_gfx_path)?;
            let map_index = map_ids.iter().position(|id| id == &map_arg);

            App::new()
                .insert_resource(WindowDescriptor {
                    title: format!("vakfu (Map id : {})", map_arg),
                    ..default()
                })
                .add_plugins_with(DefaultPlugins, |group| {
                    group.add_before::<bevy::asset::AssetPlugin, _>(JarAssetIo::plugin(gfx_path))
                })
                .add_plugin(EguiPlugin)
                .add_plugin(LogDiagnosticsPlugin::default())
                .add_plugin(FrameTimeDiagnosticsPlugin::default())
                .init_asset_loader::<TgamLoader>()
                .insert_resource(Settings::default())
                .insert_resource(NavigationInfo {
                    map_ids,
                    current_index: map_index.unwrap(),
                    game_path: game_path.clone(),
                })
                .insert_resource(CameraController::default())
                .insert_resource(lib)
                .insert_resource(map)
                .add_startup_system(setup_system)
                .add_system(settings_system.label("settings"))
                .add_system(ui_system.label("ui"))
                .add_system(camera_controller_system.label("camera_control"))
                .add_system(camera_system.label("camera").after("camera_control"))
                .add_system(map_chunk_view_system.label("chunk_view").after("camera"))
                .add_system(
                    visibility_system
                        .label("visibility")
                        .after("chunk_view")
                        .after("settings"),
                )
                .add_system(animation_system.label("animation").after("visibility"))
                .run();

            Ok(())
        }
        Err(Error::MissingOption(_) | Error::OptionWithoutAValue(_)) => {
            match get_map_ids(&base_map_gfx_path) {
                Ok(map_ids) => Err(anyhow!(
                    "Map isn't specified, following map ids are available :\n{:?}",
                    map_ids
                )),
                Err(_e) => Err(anyhow!(
                    "Map isn't specified, but no map found in game_path specified."
                )),
            }
        }
        Err(_e) => Err(anyhow!(_e)),
    }
}
