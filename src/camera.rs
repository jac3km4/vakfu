use bevy::ecs::system::SystemParam;
use bevy::input::ButtonState;
use bevy::input::mouse::{MouseButtonInput, MouseScrollUnit, MouseWheel};
use bevy::prelude::*;
use bevy_egui::{EguiContext, PrimaryEguiContext, egui};

/// Render layer containing every map sprite.
pub const WORLD_LAYER: usize = 0;
/// Render layer containing editor ghosts, grids and selection outlines.
///
/// A dedicated camera draws this layer after the world camera, so overlay
/// depth never has to be interleaved with map sprite Z.
pub const OVERLAY_LAYER: usize = 1;

/// Marks the camera that follows [`CameraController`] and draws the map.
#[derive(Component)]
pub struct MainCamera;

/// Marks the camera that draws the editor overlay layer on top of the map.
#[derive(Component)]
pub struct OverlayCamera;


#[derive(Clone, Resource)]
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

    pub editor_mode: bool,
    pub canvas_rect: Option<Rect>,
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
            editor_mode: false,
            canvas_rect: None,
        }
    }
}

pub fn is_pointer_over_ui(
    ctx: &mut egui::Context,
    canvas_rect: Option<Rect>,
    cursor_pos: Option<Vec2>,
) -> bool {
    if ctx.is_pointer_over_egui() || ctx.egui_is_using_pointer() || ctx.any_popup_open() {
        return true;
    }
    if let Some(rect) = canvas_rect {
        let pos = cursor_pos.or_else(|| {
            ctx.input(|i| i.pointer.latest_pos())
                .map(|p| Vec2::new(p.x, p.y))
        });
        if let Some(pos) = pos {
            return !rect.contains(pos);
        }
    }
    false
}

#[derive(SystemParam)]
pub struct CameraInput<'w, 's> {
    time: Res<'w, Time>,
    keys: Res<'w, ButtonInput<KeyCode>>,
    windows: Query<'w, 's, &'static Window>,
    mouse_button_input_events: MessageReader<'w, 's, MouseButtonInput>,
    cursor_moved_events: MessageReader<'w, 's, CursorMoved>,
    mouse_wheel_events: MessageReader<'w, 's, MouseWheel>,
    egui_contexts: Query<'w, 's, &'static mut EguiContext, With<PrimaryEguiContext>>,
}

pub fn camera_controller_system(
    input: CameraInput<'_, '_>,
    mut state: ResMut<'_, CameraController>,
) {
    let CameraInput {
        time,
        keys,
        windows,
        mut mouse_button_input_events,
        mut cursor_moved_events,
        mut mouse_wheel_events,
        mut egui_contexts,
    } = input;
    let cursor_pos = windows.single().ok().and_then(Window::cursor_position);
    let canvas_rect = state.canvas_rect;
    let (is_over_egui, wants_keyboard) = egui_contexts
        .iter_mut()
        .fold((false, false), |(over, kbd), mut context| {
            let ctx = context.get_mut();
            (
                over || is_pointer_over_ui(ctx, canvas_rect, cursor_pos),
                kbd || ctx.egui_wants_keyboard_input(),
            )
        });

    let space_held = keys.pressed(KeyCode::Space);

    for event in mouse_button_input_events.read() {
        let is_pan_button = if state.editor_mode {
            event.button == MouseButton::Middle
                || event.button == MouseButton::Right
                || (event.button == MouseButton::Left && space_held)
        } else {
            event.button == MouseButton::Left
        };

        match event.state {
            ButtonState::Pressed if is_pan_button && !is_over_egui => {
                state.dragging = true;
            }
            ButtonState::Released
                if (!state.editor_mode && event.button == MouseButton::Left)
                    || (state.editor_mode
                        && matches!(
                            event.button,
                            MouseButton::Middle | MouseButton::Right | MouseButton::Left
                        )) =>
            {
                state.dragging = false;
                state.last_cursor_position = None;
            }
            _ => {}
        }
    }

    for event in cursor_moved_events.read() {
        let current_position = event.position;
        if state.dragging {
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
        } else {
            state.last_cursor_position = None;
        }
    }

    if state.editor_mode && !wants_keyboard {
        let mut pan = Vec2::ZERO;
        if keys.pressed(KeyCode::KeyW) || keys.pressed(KeyCode::ArrowUp) {
            pan.y += 1.0;
        }
        if keys.pressed(KeyCode::KeyS) || keys.pressed(KeyCode::ArrowDown) {
            pan.y -= 1.0;
        }
        if keys.pressed(KeyCode::KeyA) || keys.pressed(KeyCode::ArrowLeft) {
            pan.x -= 1.0;
        }
        if keys.pressed(KeyCode::KeyD) || keys.pressed(KeyCode::ArrowRight) {
            pan.x += 1.0;
        }
        if pan != Vec2::ZERO {
            let speed = 600.0 * state.current_zoom * time.delta_secs();
            state.target_position += (pan.normalize() * speed).extend(0.0);
        }
    }

    let min_zoom = if state.editor_mode { 0.25 } else { 1.0 };
    for event in mouse_wheel_events.read() {
        if !is_over_egui {
            let scroll_amount = match event.unit {
                MouseScrollUnit::Line => event.y,
                MouseScrollUnit::Pixel => event.y / 16.0,
            };
            let zoom_delta = scroll_amount * state.zoom_sensitivity;
            state.target_zoom -= zoom_delta;
            state.target_zoom = state.target_zoom.clamp(min_zoom, 20.0);
        }
    }
}

pub fn camera_system(
    time: Res<'_, Time>,
    mut cameras: Query<'_, '_, &mut Transform, With<MainCamera>>,
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

    let dt = time.delta_secs();
    let pos_factor = 1.0 - (1.0 - controller.position_smoothness.clamp(0.0, 0.999)).powf(dt * 60.0);
    let zoom_factor = 1.0 - (1.0 - controller.zoom_smoothness.clamp(0.0, 0.999)).powf(dt * 60.0);

    controller.current_position = smooth_position(
        controller.current_position,
        controller.target_position,
        pos_factor,
    );

    controller.current_zoom =
        smooth_zoom(controller.current_zoom, controller.target_zoom, zoom_factor);

    let mut next_transform = *transform;
    next_transform.translation.x = controller.current_position.x;
    next_transform.translation.y = controller.current_position.y;
    next_transform.scale = Vec3::splat(controller.current_zoom);
    transform.set_if_neq(next_transform);

    Ok(())
}

fn smooth_position(current: Vec3, target: Vec3, factor: f32) -> Vec3 {
    let next = current.lerp(target, factor.clamp(0.0, 1.0));
    if next.distance_squared(target) <= 0.01 * 0.01 {
        target
    } else {
        next
    }
}

fn smooth_zoom(current: f32, target: f32, factor: f32) -> f32 {
    let next = current + (target - current) * factor.clamp(0.0, 1.0);
    if (next - target).abs() <= 0.0001 {
        target
    } else {
        next
    }
}
