use bevy::{app::AppExit, prelude::*};
use bevy_egui::{egui, EguiContext};

use crate::systems::settings::Settings;

use super::navigation::{start_other_vakfu, NavigationInfo};

pub fn ui_system(
    mut egui_context: ResMut<EguiContext>,
    mut settings: ResMut<Settings>,
    navigation: Res<NavigationInfo>,
    mut exit: EventWriter<AppExit>,
) {
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

    egui::Window::new("Navigation").show(egui_context.ctx_mut(), |ui| {
        ui.horizontal(|ui| {
            if navigation.current_index > 0 {
                let previous_value = navigation
                    .map_ids
                    .get(navigation.current_index - 1)
                    .unwrap();
                if ui
                    .button(format!("Previous ({})", previous_value))
                    .clicked()
                {
                    start_other_vakfu(previous_value, &navigation.game_path);
                    exit.send(AppExit);
                };
            }
            if navigation.current_index < navigation.map_ids.len() - 1 {
                let next_value = navigation
                    .map_ids
                    .get(navigation.current_index + 1)
                    .unwrap();
                if ui.button(format!("Next ({})", next_value)).clicked() {
                    start_other_vakfu(next_value, &navigation.game_path);
                    exit.send(AppExit);
                };
            }
        });
        ui.separator();
        ui.label("Open specific map");
        let map_list = egui::ScrollArea::vertical()
            .max_height(200.0)
            .auto_shrink([false; 2]);
        map_list.show(ui, |ui| {
            for map_id in &navigation.map_ids {
                if ui.button(format!("{}", map_id)).clicked() {
                    start_other_vakfu(map_id, &navigation.game_path);
                    exit.send(AppExit);
                };
            }
        });
    });

    if settings.as_ref() != &copy {
        settings.updated = true;
    }
}
