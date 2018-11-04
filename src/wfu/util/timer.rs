extern crate chrono;
use self::chrono::prelude::*;

pub struct Timer {
    last_frame: i64,
    last_time: i64,
    nb_frames: i32,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            last_frame: 0i64,
            last_time: 0i64,
            nb_frames: 0,
        }
    }

    pub fn time_as_millis(&self) -> i64 {
        self.last_time
    }

    pub fn tick(&mut self) -> i64 {
        let ts = Local::now().timestamp_millis();
        self.nb_frames += 1;
        let delta = ts - self.last_time;
        self.last_time = ts;

        if ts - self.last_frame >= 1000i64 {
            debug!("{} ms/frame", 1000.0f64 / self.nb_frames as f64);
            self.nb_frames = 0;
            self.last_frame = ts;
        }
        return delta;
    }
}
