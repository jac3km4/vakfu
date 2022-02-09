use bevy::prelude::Color;
use glam::{IVec2, Vec2};

use crate::map::iso_to_screen;

#[derive(Debug)]
pub struct MapSprite {
    pub cell_x: i32,
    pub cell_y: i32,
    pub cell_z: i16,
    pub height: u8,
    pub altitude_order: u8,
    pub tag: u8,
    pub element_id: i32,
    pub group_key: i32,
    pub group_id: i32,
    pub layer: u8,
    pub colors: Vec<f32>,
}

impl MapSprite {
    #[inline]
    pub fn screen_position(&self) -> Vec2 {
        let height = self.cell_z as i32 - self.height as i32;
        iso_to_screen(IVec2::new(self.cell_x, self.cell_y), height)
    }

    pub fn color(&self) -> Color {
        if self.colors.len() == 3 {
            Color::rgb_linear(self.colors[0], self.colors[1], self.colors[2])
        } else {
            Color::rgb_linear(0.5, 0.5, 0.5)
        }
    }

    #[inline]
    pub fn hashcode(&self) -> i64 {
        (self.altitude_order as i64 & 0x1FFFi64) << 6i64
            | ((self.cell_x as i64 + 8192i64) & 0x3FFFi64) << 19i64
            | ((self.cell_y as i64 + 8192i64) & 0x3FFFi64) << 34i64
    }
}
