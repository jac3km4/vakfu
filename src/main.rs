use std::fs;
use std::fs::File;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::bail;
use assets::{JarAssetSource, MapSpriteLibrary, TgamLoader};
use bevy::asset::io::AssetSourceBuilder;
use bevy::prelude::*;
use bevy_egui::{EguiPlugin, EguiPrimaryContextPass, PrimaryEguiContext};
use camera::{CameraController, camera_controller_system, camera_system};
use pico_args::Arguments;
use render::{
    ChangeMapEvent, MapCatalog, animation_system, load_map_system, load_renderer, rendering_system,
};
use settings::{MapViewSettings, settings_ui_system};

#[allow(unused)]
mod assets;
mod camera;
mod premult_alpha;
mod render;
mod settings;
mod util;

fn main() -> anyhow::Result<()> {
    let mut pargs = Arguments::from_env();

    let game_path: PathBuf = pargs.value_from_str("--path")?;
    let maps_path = game_path.join("contents").join("maps");

    let available_maps = match get_map_list(&maps_path) {
        Ok(ids) if !ids.is_empty() => ids,
        Ok(_) => bail!("no map files found in {}", maps_path.join("gfx").display()),
        Err(err) => bail!(
            "could not read maps directory {}: {err}",
            maps_path.join("gfx").display()
        ),
    };

    let map_id = match pargs.opt_value_from_str::<&str, i32>("--map")? {
        Some(id) => {
            if !available_maps.contains(&id) {
                bail!("map {id} not found in {}", maps_path.join("gfx").display());
            }
            id
        }
        None => available_maps[0],
    };

    let lib_path = maps_path.join("data.jar");
    let sprites = Arc::new(MapSpriteLibrary::load(File::open(lib_path)?)?);
    let asset_source = JarAssetSource::new(maps_path.join("gfx.jar"))?;
    let renderer = load_renderer(&maps_path, map_id, &sprites)?;

    let initial_center = renderer.center();
    let camera_controller = CameraController {
        target_position: Vec3::new(initial_center.x, initial_center.y, 0.0),
        current_position: Vec3::new(initial_center.x, initial_center.y, 0.0),
        ..CameraController::default()
    };

    let map_catalog = MapCatalog {
        maps_path,
        sprites,
        available_maps,
        current_map_id: map_id,
    };

    App::new()
        .register_asset_source(
            "gfx",
            AssetSourceBuilder::new(move || Box::new(asset_source.clone())),
        )
        .add_plugins(DefaultPlugins)
        .add_plugins(premult_alpha::PremultAlphaPlugin)
        .add_plugins(EguiPlugin::default())
        .init_asset_loader::<TgamLoader>()
        .add_message::<ChangeMapEvent>()
        .insert_resource(map_catalog)
        .insert_resource(renderer)
        .insert_resource(camera_controller)
        .init_resource::<MapViewSettings>()
        .add_systems(Startup, setup)
        .add_systems(
            EguiPrimaryContextPass,
            settings_ui_system.run_if(egui_has_primary_context),
        )
        .add_systems(
            Update,
            (
                (
                    load_map_system,
                    camera_controller_system,
                    camera_system,
                    rendering_system,
                )
                    .chain(),
                animation_system,
            ),
        )
        .run();

    Ok(())
}

fn setup(mut commands: Commands<'_, '_>, camera_controller: Res<'_, CameraController>) {
    commands.spawn((
        Camera2d,
        CompositingSpace::Linear,
        Transform::from_translation(camera_controller.current_position),
    ));
}

fn egui_has_primary_context(
    query: Query<'_, '_, &bevy_egui::EguiContext, With<PrimaryEguiContext>>,
) -> bool {
    !query.is_empty()
}

fn get_map_list(maps_path: &Path) -> anyhow::Result<Vec<i32>> {
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
