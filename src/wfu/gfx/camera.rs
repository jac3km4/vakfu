extern crate cgmath;
extern crate winit;

use self::cgmath::{Matrix2, Matrix4, Vector2};
use wfu::util::input_state::InputState;
use winit::VirtualKeyCode;

pub struct Camera<F> {
    zoom_factor: f32,
    translation: Vector2<f32>,
    accel: Matrix2<f32>,
    ease: F,
}

const ACCELERATION_FACTOR: f32 = 12f32;

const MOVEMENT_SPEED: f32 = 0.4f32;
const ZOOM_SPEED: f32 = 0.9f32;

const BOUND_LEEWAY: f32 = 1024f32;

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
            zoom_factor: 2.0f32,
            ease,
            accel: Matrix2::new(0.0f32, 0.0f32, 0.0f32, 0.0f32),
            translation: Vector2 { x: 0f32, y: 0f32 },
        }
    }

    pub fn get_matrix(&self, screen_w: u32, screen_h: u32) -> Matrix4<f32> {
        let x_factor = self.zoom_factor / screen_w as f32;
        let y_factor = self.zoom_factor / screen_h as f32;

        Matrix4::from_nonuniform_scale(x_factor, y_factor, 1.0f32)
            * Matrix4::from_translation(self.translation.extend(0f32))
    }

    pub fn get_bounds(&self, screen_w: u32, screen_h: u32) -> Matrix2<f32> {
        let translation_x = -self.translation.x;
        let translation_y = self.translation.y;

        let half_w = (screen_w as f32 / self.zoom_factor) + BOUND_LEEWAY;
        let half_h = (screen_h as f32 / self.zoom_factor) + BOUND_LEEWAY;

        let left = translation_x - half_w;
        let top = translation_y - half_h;
        let right = left + 2f32 * half_w;
        let bottom = top + 2f32 * half_h;

        Matrix2 {
            x: Vector2 { x: left, y: right },
            y: Vector2 { x: top, y: bottom },
        }
    }

    pub fn update(&mut self, delta: i64, input: &InputState) {
        if input.is_pressed(&VirtualKeyCode::Up) {
            self.accel.x.y = MOVEMENT_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::Down) {
            self.accel.y.y = MOVEMENT_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::Left) {
            self.accel.x.x = MOVEMENT_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::Right) {
            self.accel.y.x = MOVEMENT_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::Subtract) {
            self.zoom_factor *= ZOOM_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::Add) {
            self.zoom_factor *= 1f32 / ZOOM_SPEED
        }
        if input.is_pressed(&VirtualKeyCode::O) {
            self.zoom_factor = 2f32
        }

        let ease = &self.ease;
        self.accel =
            Matrix2::from_cols(self.accel.x.map(|v| ease(v)), self.accel.y.map(|v| ease(v)));
        self.translation += self.accel.x * (ACCELERATION_FACTOR * delta as f32 / self.zoom_factor);
        self.translation -= self.accel.y * (ACCELERATION_FACTOR * delta as f32 / self.zoom_factor);
    }
}
