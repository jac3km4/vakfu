use bevy::prelude::*;
use bevy_egui::{egui, EguiContext};

use crate::systems::settings::Settings;

pub fn ui_system(mut egui_context: ResMut<EguiContext>, mut settings: ResMut<Settings>) {
    let copy = settings.clone();

    egui::Window::new("Settings").show(egui_context.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.layer_filter_on, "Layer filter");
            ui.add_enabled(
                settings.layer_filter_on,
                egui::Slider::new(&mut settings.layer, 0..=127),
            );
        });
        ui.horizontal(|ui| {
            ui.checkbox(&mut settings.group_filter_on, "Group filter");
            ui.add_enabled(
                settings.group_filter_on,
                egui::Slider::new(&mut settings.group, -1..=1),
            );
        });
    });

    if settings.as_ref() != &copy {
        settings.updated = true;
    }
}
