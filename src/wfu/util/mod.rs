pub mod indexed;
pub mod input_state;
pub mod resources;
pub mod timer;

pub fn first_greater_power_of_two(value: i32) -> i32 {
    if value < 2 {
        return value;
    }
    let mut v = value - 1;
    v = v | v >> 1;
    v = v | v >> 2;
    v = v | v >> 4;
    v = v | v >> 8;
    v = v | v >> 16;
    v + 1
}

pub fn iso_to_screen_x(iso_local_x: i32, iso_local_y: i32) -> f32 {
    (iso_local_x - iso_local_y) as f32 * 43f32 // 43 * 86 * 0.5f
}

pub fn iso_to_screen_y(iso_local_x: i32, iso_local_y: i32, iso_altitude: i32) -> f32 {
    (-(iso_local_x + iso_local_y) as f32 * 21.5f32) + iso_altitude as f32 * 10f32 //21.5
}

pub fn screen_to_iso_x(x: f32, y: f32) -> i32 {
    (x / 86f32 - y / 43f32) as i32
}

pub fn screen_to_iso_y(x: f32, y: f32) -> i32 {
    -(x / 86f32 + y / 43f32) as i32
}
