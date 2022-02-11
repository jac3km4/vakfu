use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::input::ElementState;
use bevy::prelude::*;

#[derive(Default, Component)]
pub struct CameraController {
    cursor_position: Vec2,
    last_cursor_position: Vec2,
    drag_start_position: Option<Vec2>,
    zoom: Option<f32>,
}

pub fn camera_controller_system(
    mut mouse_button_input_events: EventReader<MouseButtonInput>,
    mut cursor_moved_events: EventReader<CursorMoved>,
    mut mouse_wheel_events: EventReader<MouseWheel>,
    mut state: ResMut<CameraController>,
) {
    if let Some(event) = cursor_moved_events.iter().last() {
        state.last_cursor_position = state.cursor_position;
        state.cursor_position = event.position;
    }

    for event in mouse_button_input_events.iter() {
        match event {
            MouseButtonInput {
                button: MouseButton::Middle,
                state: ElementState::Pressed,
            } => {
                state.drag_start_position = Some(state.cursor_position);
            }
            MouseButtonInput {
                button: MouseButton::Middle,
                state: ElementState::Released,
            } => {
                state.drag_start_position = None;
            }
            _ => {}
        }
    }

    if let Some(event) = mouse_wheel_events.iter().next() {
        state.zoom = Some(event.y);
    } else {
        state.zoom = None;
    }
}

pub fn camera_system(
    mut cameras: Query<&mut Transform, With<Camera>>,
    controller_state: Res<CameraController>,
) {
    let mut cam = cameras.single_mut();

    if controller_state.drag_start_position.is_some() {
        let offset = controller_state.cursor_position - controller_state.last_cursor_position;
        let scale = cam.scale;
        cam.translation -= offset.extend(0.) * scale;
    }

    if let Some(delta) = controller_state.zoom {
        let factor = if delta < 0. { 1.5 } else { 0.75 };
        cam.scale = (cam.scale * factor).max(Vec3::ONE);
    }
}
