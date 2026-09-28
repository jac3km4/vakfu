use bevy::ecs::event::EventWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, Res, ResMut};
use bevy_egui::{EguiContexts, egui};

use crate::render::{ChangeMapEvent, MapCatalog};

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
    catalog: Res<'_, MapCatalog>,
    mut change_map: EventWriter<'_, ChangeMapEvent>,
    mut search_filter: Local<'_, String>,
) {
    egui::Window::new("Settings")
        .movable(false)
        .show(contexts.ctx_mut(), |ui| {
            ui.horizontal(|ui| {
                ui.label("Map:");

                let current_idx = catalog
                    .available_maps
                    .iter()
                    .position(|&id| id == catalog.current_map_id);

                let has_prev = current_idx.is_some_and(|idx| idx > 0);
                if ui.add_enabled(has_prev, egui::Button::new("◀")).clicked()
                    && let Some(idx) = current_idx
                {
                    change_map.write(ChangeMapEvent(catalog.available_maps[idx - 1]));
                }

                egui::ComboBox::from_id_salt("map_selector")
                    .selected_text(catalog.current_map_id.to_string())
                    .width(100.0)
                    .show_ui(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label("🔍");
                            ui.text_edit_singleline(&mut *search_filter);
                        });
                        ui.separator();
                        egui::ScrollArea::vertical()
                            .max_height(200.0)
                            .show(ui, |ui| {
                                for &map_id in &catalog.available_maps {
                                    let map_str = map_id.to_string();
                                    if !search_filter.is_empty()
                                        && !map_str.contains(&**search_filter)
                                    {
                                        continue;
                                    }
                                    let is_selected = catalog.current_map_id == map_id;
                                    if ui.selectable_label(is_selected, &map_str).clicked() {
                                        change_map.write(ChangeMapEvent(map_id));
                                    }
                                }
                            });
                    });

                let has_next =
                    current_idx.is_some_and(|idx| idx + 1 < catalog.available_maps.len());
                if ui.add_enabled(has_next, egui::Button::new("▶")).clicked()
                    && let Some(idx) = current_idx
                {
                    change_map.write(ChangeMapEvent(catalog.available_maps[idx + 1]));
                }
            });

            ui.separator();

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
