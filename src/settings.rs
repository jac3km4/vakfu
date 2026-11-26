use bevy::ecs::change_detection::DetectChangesMut;
use bevy::ecs::message::MessageWriter;
use bevy::ecs::resource::Resource;
use bevy::ecs::system::{Local, Res, ResMut, SystemParam};
use bevy_egui::{EguiContexts, egui};

use crate::assets::{DayLightTable, LightingState};
use crate::render::{ChangeMapEvent, GroupVisibilityState, MapCatalog, MapRenderer};

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
    /// Game-calendar season selecting a `dayLight.xml` colour curve.
    pub season: u8,
    /// Position within the day, in `0..=100`, interpolating the sun colour.
    pub day_percentage: f32,
    /// Effective lighting. `day_light_system` refreshes the sun colour and the
    /// shadow/night-light intensities derived from it whenever [`Self::season`]
    /// or [`Self::day_percentage`] changes.
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
            season: 0,
            day_percentage: 50.0,
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
/// The sun colour, shadow intensity and night-light intensity all follow the
/// `dayLight.xml` curve, so the only controls left are the season and the day
/// percentage. Callers gate the whole block on `enable_light`.
pub fn lighting_controls_ui(
    ui: &mut egui::Ui,
    settings: &mut MapViewSettings,
    table: &DayLightTable,
) {
    if table.is_empty() {
        ui.weak("No dayLight.xml in contents/data/data.jar");
        return;
    }

    let seasons: Vec<u8> = table.season_ids().collect();
    if let Some(&first) = seasons.first()
        && !seasons.contains(&settings.season)
    {
        settings.season = first;
    }
    ui.horizontal(|ui| {
        ui.label("Season");
        if seasons.len() > 1 {
            egui::ComboBox::from_id_salt("day_light_season")
                .selected_text(format!("Season {}", settings.season))
                .width(120.0)
                .show_ui(ui, |ui| {
                    for &season in &seasons {
                        ui.selectable_value(
                            &mut settings.season,
                            season,
                            format!("Season {season}"),
                        );
                    }
                });
        } else {
            ui.weak(format!("Season {}", settings.season));
        }
    });
    ui.add(
        egui::Slider::new(&mut settings.day_percentage, 0.0..=100.0)
            .text("Day")
            .suffix("%"),
    );
}

/// Draws a one-of-many map filter: an enable checkbox plus a combo box over the
/// values the current map actually contains.
///
/// Sharing this between the layer and group filters keeps the settings window
/// predictable, and the combo box cannot select a value that would blank the map.
fn map_filter_ui<T>(
    ui: &mut egui::Ui,
    label: &str,
    enabled: &mut bool,
    selected: &mut T,
    available: &[T],
    prefix: &str,
    id_salt: &str,
) where
    T: Copy + PartialEq + std::fmt::Display,
{
    ui.checkbox(enabled, label);
    if !*enabled {
        return;
    }
    // Keep the selection on a value the current map actually uses.
    if !available.contains(selected)
        && let Some(&first) = available.first()
    {
        *selected = first;
    }
    egui::ComboBox::from_id_salt(id_salt)
        .selected_text(format!("{prefix}{selected}"))
        .show_ui(ui, |ui| {
            for &value in available {
                ui.selectable_value(selected, value, format!("{prefix}{value}"));
            }
        });
}

#[derive(SystemParam)]
pub struct SettingsUiParams<'w, 's> {
    contexts: EguiContexts<'w, 's>,
    settings: ResMut<'w, MapViewSettings>,
    catalog: Res<'w, MapCatalog>,
    renderer: Res<'w, MapRenderer>,
    groups: Res<'w, GroupVisibilityState>,
    day_light: Res<'w, DayLightTable>,
    change_map: MessageWriter<'w, ChangeMapEvent>,
    search_filter: Local<'s, String>,
}

pub fn settings_ui_system(params: SettingsUiParams<'_, '_>) -> bevy::prelude::Result {
    let SettingsUiParams {
        mut contexts,
        mut settings,
        catalog,
        renderer,
        groups,
        day_light,
        mut change_map,
        mut search_filter,
    } = params;
    let mut edited_settings = settings.clone();
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
                map_filter_ui(
                    ui,
                    "Layer filter",
                    &mut edited_settings.layer_filter_on,
                    &mut edited_settings.layer,
                    renderer.available_layers(),
                    "L",
                    "layer_filter",
                );
            });
            ui.horizontal(|ui| {
                map_filter_ui(
                    ui,
                    "Group filter",
                    &mut edited_settings.group_filter_on,
                    &mut edited_settings.group,
                    renderer.available_groups(),
                    "G",
                    "group_filter",
                );
            });
            ui.horizontal(|ui| {
                let has_groups = groups.0.is_some();
                ui.add_enabled(
                    has_groups && !edited_settings.group_filter_on,
                    egui::Checkbox::new(
                        &mut edited_settings.auto_group_visibility,
                        "Auto group fade",
                    ),
                )
                .on_disabled_hover_text(if has_groups {
                    "Disabled while a single group is isolated"
                } else {
                    "This map has no groups.lib metadata"
                });
                if has_groups {
                    ui.weak(format!("camera: G{}", renderer.active_camera_group()));
                }
            });
            ui.horizontal(|ui| {
                ui.label("LOD:");
                ui.selectable_value(&mut edited_settings.visibility_mask, 7, "Max");
                ui.selectable_value(&mut edited_settings.visibility_mask, 3, "Avg");
                ui.selectable_value(&mut edited_settings.visibility_mask, 1, "Min");
            });
            ui.checkbox(&mut edited_settings.enable_light, "Enable light");
            ui.add_enabled_ui(edited_settings.enable_light, |ui| {
                lighting_controls_ui(ui, &mut edited_settings, &day_light);
            });
        });
    settings.set_if_neq(edited_settings);
    Ok(())
}
