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
    layers_disabled: u8,
}

impl InputState {
    pub fn new() -> InputState {
        InputState {
            pressed: HashSet::new(),
            focused: true,
            close_requested: false,
            light_enabled: true,
            layers_disabled: 0,
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

    pub fn set_disabled_layers(&mut self, layer: u8) {
        self.layers_disabled = self.layers_disabled ^ (1 << layer);
        info!("Disabled layers: {:#010b}", self.layers_disabled);
    }

    pub fn disabled_layers(&self) -> u8 {
        self.layers_disabled
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
                            if self.focused {
                                self.pressed.insert(code)
                            } else {
                                false
                            };
                        }
                    });
                }
                winit::ElementState::Released => {
                    input.virtual_keycode.map(|code| {
                        self.pressed.remove(&code);
                        match code {
                            VirtualKeyCode::L => self.light_enabled = !self.light_enabled,
                            VirtualKeyCode::Key1 => self.set_disabled_layers(0),
                            VirtualKeyCode::Key2 => self.set_disabled_layers(1),
                            VirtualKeyCode::Key3 => self.set_disabled_layers(2),
                            VirtualKeyCode::Key4 => self.set_disabled_layers(3),
                            VirtualKeyCode::Key5 => self.set_disabled_layers(4),
                            VirtualKeyCode::Key6 => self.set_disabled_layers(5),
                            VirtualKeyCode::Key7 => self.set_disabled_layers(6),
                            VirtualKeyCode::Key8 => self.set_disabled_layers(7),
                            VirtualKeyCode::Key0 => self.layers_disabled = 0,
                            _ => (),
                        };
                    });
                }
            },
            _ => (),
        }
    }
}
