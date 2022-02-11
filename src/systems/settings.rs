use bevy::prelude::*;

use super::render::{SpriteProperties, VisibilityFlags};

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Settings {
    pub layer_filter_on: bool,
    pub layer: u8,
    pub group_filter_on: bool,
    pub group: i32,
    pub updated: bool,
}

pub fn settings_system(
    mut settings: ResMut<Settings>,
    mut query: Query<(&SpriteProperties, &mut VisibilityFlags)>,
) {
    if settings.updated {
        for (props, mut visibility) in query.iter_mut() {
            let group_range = settings.group * 1000..(settings.group + 1) * 1000;

            let is_layer_active = !settings.layer_filter_on || props.layer == settings.layer;
            let is_group_active =
                !settings.group_filter_on || group_range.contains(&props.group_key);
            visibility.is_active = is_layer_active && is_group_active;
        }

        settings.updated = false;
    }
}
