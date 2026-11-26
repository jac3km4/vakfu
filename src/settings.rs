use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, Res, ResMut};
use bevy_egui::{EguiContexts, egui};

use crate::assets::LightingState;
use crate::render::{ChangeMapEvent, MapCatalog};

#[derive(Debug, Clone, PartialEq, Resource)]
pub struct MapViewSettings {
    pub layer_filter_on: bool,
    pub layer: u8,
    pub group_filter_on: bool,
    pub group: i32,
    /// Fade groups using `groups.lib` and the group under the camera centre.
    ///
    /// Disable this (or enable the group filter) to reveal every element at
    /// once, including indoor rooms and roofs the camera is not inside.
    pub auto_group_visibility: bool,
    /// Visibility-mask bits requested from the map, mirroring
    /// `DisplayedScreenWorld.getLODMask`: `1` = minimum, `3` = average and
    /// `7` = maximum detail. An element is shown when all of its own mask bits
    /// are present.
    pub visibility_mask: u8,
    pub enable_light: bool,
    /// Derive the shadow and night-light intensities from the sun colour,
    /// matching `SunLightModifier.update` / `setLightIntensityFactor`.
    pub auto_daylight: bool,
    pub lighting: LightingState,
}

impl Default for MapViewSettings {
    fn default() -> Self {
        Self {
            layer_filter_on: false,
            layer: 0,
            group_filter_on: false,
            group: 0,
            auto_group_visibility: true,
            visibility_mask: 7,
            enable_light: true,
            auto_daylight: false,
            lighting: LightingState::default(),
        }
    }
}

/// Draws the shared map selector: previous, searchable combo box and next.
pub fn map_selector_ui(
    ui: &mut egui::Ui,
    id_salt: &str,
    catalog: &MapCatalog,
    search_filter: &mut String,
    change_map: &mut MessageWriter<'_, ChangeMapEvent>,
) {
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

    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(catalog.current_map_id.to_string())
        .width(100.0)
        .show_ui(ui, |ui| {
            ui.horizontal(|ui| {
                ui.label("🔍");
                ui.text_edit_singleline(search_filter);
            });
            ui.separator();
            egui::ScrollArea::vertical()
                .max_height(200.0)
                .show(ui, |ui| {
                    for &map_id in &catalog.available_maps {
                        let map_str = map_id.to_string();
                        if !search_filter.is_empty() && !map_str.contains(&**search_filter) {
                            continue;
                        }
                        let is_selected = catalog.current_map_id == map_id;
                        if ui.selectable_label(is_selected, &map_str).clicked() {
                            change_map.write(ChangeMapEvent(map_id));
                        }
                    }
                });
        });

    let has_next = current_idx.is_some_and(|idx| idx + 1 < catalog.available_maps.len());
    if ui.add_enabled(has_next, egui::Button::new("▶")).clicked()
        && let Some(idx) = current_idx
    {
        change_map.write(ChangeMapEvent(catalog.available_maps[idx + 1]));
    }
}

/// Draws the shared time-of-day lighting controls.
///
/// Callers apply [`MapViewSettings::auto_daylight`] before calling this, and
/// gate the whole block on `enable_light` where appropriate.
pub fn lighting_controls_ui(ui: &mut egui::Ui, settings: &mut MapViewSettings) {
    ui.checkbox(&mut settings.auto_daylight, "Auto daylight");
    ui.add_enabled_ui(!settings.auto_daylight, |ui| {
        ui.add(
            egui::Slider::new(&mut settings.lighting.shadow_intensity, 0.0..=1.0).text("Shadows"),
        );
        ui.add(
            egui::Slider::new(&mut settings.lighting.night_light_intensity, 0.0..=1.0)
                .text("Night lights"),
        );
    });
    ui.horizontal(|ui| {
        ui.label("Sun colour");
        ui.color_edit_button_rgb(&mut settings.lighting.sun_color);
    });
}

pub fn settings_ui_system(
    mut contexts: EguiContexts<'_, '_>,
    mut settings: ResMut<'_, MapViewSettings>,
    catalog: Res<'_, MapCatalog>,
    mut change_map: MessageWriter<'_, ChangeMapEvent>,
    mut search_filter: Local<'_, String>,
) -> bevy::prelude::Result {
    let mut edited_settings = settings.clone();
    if edited_settings.auto_daylight {
        edited_settings.lighting.apply_auto_daylight();
    }
    egui::Window::new("Settings")
        .movable(false)
        .show(contexts.ctx_mut()?, |ui| {
            ui.horizontal(|ui| {
                ui.label("Map:");
                map_selector_ui(
                    ui,
                    "map_selector",
                    &catalog,
                    &mut search_filter,
                    &mut change_map,
                );
            });

            ui.separator();

            ui.horizontal(|ui| {
                ui.checkbox(&mut edited_settings.layer_filter_on, "Layer filter");
                ui.add_enabled(
                    edited_settings.layer_filter_on,
                    egui::Slider::new(&mut edited_settings.layer, 0..=127),
                );
            });
            ui.horizontal(|ui| {
                ui.checkbox(&mut edited_settings.group_filter_on, "Group filter");
                ui.add_enabled(
                    edited_settings.group_filter_on,
                    egui::Slider::new(&mut edited_settings.group, -1..=1),
                );
            });
            ui.horizontal(|ui| {
                ui.label("LOD:");
                ui.selectable_value(&mut edited_settings.visibility_mask, 7, "Max");
                ui.selectable_value(&mut edited_settings.visibility_mask, 3, "Avg");
                ui.selectable_value(&mut edited_settings.visibility_mask, 1, "Min");
            });
            ui.checkbox(&mut edited_settings.enable_light, "Enable light");
            ui.add_enabled_ui(edited_settings.enable_light, |ui| {
                lighting_controls_ui(ui, &mut edited_settings);
            });
        });
    settings.set_if_neq(edited_settings);
    Ok(())
}
