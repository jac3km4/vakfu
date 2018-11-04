extern crate winit;

use std::collections::HashSet;
use winit::DeviceEvent;
use winit::Event;
use winit::VirtualKeyCode;
use winit::WindowEvent;

pub struct InputState {
    pressed: HashSet<VirtualKeyCode>,
    focused: bool,
    close_requested: bool,
    light_enabled: bool,
}

impl InputState {
    pub fn new() -> InputState {
        InputState {
            pressed: HashSet::new(),
            focused: true,
            close_requested: false,
            light_enabled: true,
        }
    }

    pub fn is_pressed(&self, code: &VirtualKeyCode) -> bool {
        self.pressed.contains(code)
    }

    pub fn should_close(&self) -> bool {
        self.close_requested
    }

    pub fn is_light_enabled(&self) -> bool {
        self.light_enabled
    }

    pub fn update(&mut self, event: Event) {
        match event {
            Event::WindowEvent {
                event: WindowEvent::CloseRequested,
                ..
            } => self.close_requested = true,
            Event::WindowEvent {
                event: WindowEvent::Focused(focus),
                ..
            } => {
                self.focused = focus;
                if !focus {
                    self.pressed = HashSet::new()
                }
            }
            Event::DeviceEvent {
                event: DeviceEvent::Key(input),
                device_id,
            } => match input.state {
                winit::ElementState::Pressed => {
                    input.virtual_keycode.map(|code| match code {
                        VirtualKeyCode::Escape => self.close_requested = true,
                        _ => {
                            self.pressed.insert(code);
                        }
                    });
                }
                winit::ElementState::Released => {
                    input.virtual_keycode.map(|code| {
                        self.pressed.remove(&code);
                        match code {
                            VirtualKeyCode::L => self.light_enabled = !self.light_enabled,
                            _ => (),
                        };
                    });
                }
            },
            _ => (),
        }
    }
}
