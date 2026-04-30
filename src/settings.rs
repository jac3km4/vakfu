use bevy::ecs::resource::Resource;
use bevy::ecs::system::ResMut;
use bevy_egui::{EguiContexts, egui};

#[derive(Debug, Resource)]
pub struct MapViewSettings {
    pub layer_filter_on: bool,
    pub layer: u8,
    pub group_filter_on: bool,
    pub group: i32,
    pub enable_light: bool,
}

impl Default for MapViewSettings {
    fn default() -> Self {
        Self {
            layer_filter_on: false,
            layer: 0,
            group_filter_on: false,
            group: 0,
            enable_light: true,
        }
    }
}

pub fn settings_ui_system(
    mut contexts: EguiContexts<'_, '_>,
    mut settings: ResMut<'_, MapViewSettings>,
) {
    egui::Window::new("Settings")
        .movable(false)
        .show(contexts.ctx_mut(), |ui| {
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
            ui.checkbox(&mut settings.enable_light, "Enable light");
        });
}
