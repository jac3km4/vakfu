use bevy::prelude::*;
use bevy_egui::EguiPrimaryContextPass;
use vakfu::app::{Cli, LoadedWorld, build_app, egui_has_primary_context, parse_args};
use vakfu::settings::settings_ui_system;

const HELP: &str = "\
vakfu

USAGE:
    vakfu --path <PATH>

FLAGS:
    -h, --help       Prints help information

OPTIONS:
    --path <PATH>    Path to Ankama Games' Wakfu game directory

The map is chosen from the in-app selector.
";

fn main() -> anyhow::Result<()> {
    let Cli::Run(args) = parse_args(HELP)? else {
        return Ok(());
    };
    let world = LoadedWorld::load(args.maps_path, false)?;

    let mut app = build_app("vakfu", world);
    app.add_systems(
        EguiPrimaryContextPass,
        settings_ui_system.run_if(egui_has_primary_context),
    );
    app.run();

    Ok(())
}
