use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use vakfu::app::{Cli, LoadedWorld, build_app, egui_has_primary_context, parse_args};
use vakfu::camera::camera_system;
use vakfu::editor::{
    EditorState, editor_ghost_system, editor_input_system, editor_overlay_system, editor_ui_system,
};

const HELP: &str = "\
vakfu editor

USAGE:
    editor --path <PATH>

FLAGS:
    -h, --help       Prints help information

OPTIONS:
    --path <PATH>    Path to Ankama Games' Wakfu game directory

The map is chosen from the editor toolbar.
";

fn main() -> anyhow::Result<()> {
    let Cli::Run(args) = parse_args(HELP)? else {
        return Ok(());
    };
    let world = LoadedWorld::load(args.maps_path, true)?;

    let mut app = build_app("vakfu editor", world);
    app.init_resource::<EditorState>()
        .add_systems(
            EguiPrimaryContextPass,
            editor_ui_system.run_if(egui_has_primary_context),
        )
        .add_systems(
            Update,
            (
                editor_input_system,
                editor_ghost_system,
                editor_overlay_system,
            )
                .chain()
                .after(camera_system),
        );
    app.run();

    Ok(())
}
