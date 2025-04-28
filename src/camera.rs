use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButtonInput, MouseWheel};
use bevy::prelude::*;

#[derive(Resource)]
pub struct CameraController {
    pub dragging: bool,
    pub last_cursor_position: Option<Vec2>,

    pub target_position: Vec3,
    pub current_position: Vec3,

    pub target_zoom: f32,
    pub current_zoom: f32,

    pub position_smoothness: f32,
    pub zoom_smoothness: f32,

    pub drag_sensitivity: f32,
    pub zoom_sensitivity: f32,
}

impl Default for CameraController {
    fn default() -> Self {
        Self {
            dragging: false,
            last_cursor_position: None,
            target_position: Vec3::ZERO,
            current_position: Vec3::ZERO,
            target_zoom: 1.0,
            current_zoom: 1.0,
            position_smoothness: 0.25,
            zoom_smoothness: 0.25,
            drag_sensitivity: 1.,
            zoom_sensitivity: 0.25,
        }
    }
}

pub fn camera_controller_system(
    mut mouse_button_input_events: EventReader<'_, '_, MouseButtonInput>,
    mut cursor_moved_events: EventReader<'_, '_, CursorMoved>,
    mut mouse_wheel_events: EventReader<'_, '_, MouseWheel>,
    mut state: ResMut<'_, CameraController>,
) {
    for event in mouse_button_input_events.read() {
        if event.button == MouseButton::Left {
            state.dragging = event.state == ButtonState::Pressed;
            if !state.dragging {
                state.last_cursor_position = None;
            }
        }
    }

    if state.dragging {
        for event in cursor_moved_events.read() {
            let current_position = event.position;

            if let Some(last_position) = state.last_cursor_position {
                let delta = current_position - last_position;
                // invert x for natural movement direction
                let world_delta = Vec3::new(
                    -delta.x * state.drag_sensitivity * state.current_zoom,
                    delta.y * state.drag_sensitivity * state.current_zoom,
                    0.0,
                );
                state.target_position += world_delta;
            }

            state.last_cursor_position = Some(current_position);
        }
    }

    for event in mouse_wheel_events.read() {
        let zoom_delta = event.y * state.zoom_sensitivity;
        state.target_zoom -= zoom_delta;
        state.target_zoom = state.target_zoom.clamp(1.0, 20.0);
    }
}

pub fn camera_system(
    mut cameras: Query<'_, '_, &mut Transform, With<Camera>>,
    mut controller: ResMut<'_, CameraController>,
) -> Result {
    if controller.current_position == controller.target_position
        && controller.current_zoom == controller.target_zoom
    {
        return Ok(());
    }

    let Ok(mut transform) = cameras.single_mut() else {
        return Ok(());
    };

    if controller.current_position == Vec3::ZERO && controller.target_position == Vec3::ZERO {
        controller.current_position = transform.translation;
        controller.target_position = transform.translation;
    }

    controller.current_position = controller
        .current_position
        .lerp(controller.target_position, controller.position_smoothness);

    controller.current_zoom = lerp(
        controller.current_zoom,
        controller.target_zoom,
        controller.zoom_smoothness,
    );

    transform.translation.x = controller.current_position.x;
    transform.translation.y = controller.current_position.y;
    transform.scale = Vec3::splat(controller.current_zoom);

    Ok(())
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}
