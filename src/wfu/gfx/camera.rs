extern crate cgmath;
extern crate winit;

use self::cgmath::{Matrix2, Matrix4, Vector2};
use winit::DeviceEvent;
use winit::VirtualKeyCode;

pub struct Camera<F> {
    zoom_factor: f32,
    translation: Vector2<f32>,
    accel: Matrix2<f32>,
    ease: F,
}

const DEFAULT_WIDTH: f32 = 1024f32;
const DEFAULT_HEIGHT: f32 = 768f32;

const SCREEN_WIDTH_RATIO: f32 = 9.765625E-4f32;
const SCREEN_HEIGHT_RATIO: f32 = 0.0017361111f32;

const ACCELERATION_FACTOR: f32 = 16f32;

const MOVEMENT_SPEED: f32 = 0.4f32;
const ZOOM_SPEED: f32 = 0.8f32;

pub fn with_ease_in_out_quad() -> Camera<impl Fn(f32) -> f32> {
    Camera::new(|t| {
        if t < 0.5f32 {
            2f32 * t * t
        } else {
            -1f32 + 2f32 * (2f32 - t) * t
        }
    })
}

impl<F: Fn(f32) -> f32> Camera<F> {
    pub fn new(ease: F) -> Camera<F> {
        Camera {
            zoom_factor: 3.0f32,
            ease,
            accel: Matrix2::new(0.0f32, 0.0f32, 0.0f32, 0.0f32),
            translation: Vector2 { x: 0f32, y: 0f32 },
        }
    }

    pub fn get_matrix(&self, screen_w: u32, screen_h: u32) -> Matrix4<f32> {
        let x_factor = self.zoom_factor / (screen_w as f32 / DEFAULT_WIDTH);
        let y_factor = self.zoom_factor / (screen_h as f32 / DEFAULT_HEIGHT);

        Matrix4::from_nonuniform_scale(x_factor, y_factor, 1.0f32)
            * Matrix4::from_translation(self.translation.extend(0f32))
    }

    pub fn get_abs_center(&self) -> Vector2<f32> {
        let translation_x = -self.translation.x;
        let translation_y = self.translation.y;

        let resolution_factor = DEFAULT_WIDTH.max(DEFAULT_HEIGHT);

        let x = translation_x * resolution_factor * SCREEN_WIDTH_RATIO;
        let y = translation_y * resolution_factor * SCREEN_HEIGHT_RATIO;

        Vector2 { x, y }
    }

    pub fn update(&mut self, delta: f64) {
        let ease = &self.ease;
        self.accel =
            Matrix2::from_cols(self.accel.x.map(|v| ease(v)), self.accel.y.map(|v| ease(v)));
        self.translation += self.accel.x * (ACCELERATION_FACTOR * delta as f32 / self.zoom_factor);
        self.translation -= self.accel.y * (ACCELERATION_FACTOR * delta as f32 / self.zoom_factor);
    }

    pub fn handle(&mut self, event: DeviceEvent) -> () {
        match event {
            DeviceEvent::Key(input) => {
                input.virtual_keycode.map(|code| match code {
                    VirtualKeyCode::Up => self.accel.x.y = MOVEMENT_SPEED,
                    VirtualKeyCode::Down => self.accel.y.y = MOVEMENT_SPEED,
                    VirtualKeyCode::Left => self.accel.x.x = MOVEMENT_SPEED,
                    VirtualKeyCode::Right => self.accel.y.x = MOVEMENT_SPEED,
                    VirtualKeyCode::Subtract => self.zoom_factor *= ZOOM_SPEED,
                    VirtualKeyCode::Add => self.zoom_factor *= 1f32 / ZOOM_SPEED,
                    _ => (),
                });
                ()
            }
            _ => (),
        }
    }
}
