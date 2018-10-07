extern crate time;

pub struct Timer {
    last_time_to_s: f64,
    last_time: f64,
    nb_frames: i32,
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            last_time_to_s: 0f64,
            last_time: 0f64,
            nb_frames: 0,
        }
    }

    pub fn tick(&mut self) -> f64 {
        let current_time = time::get_time();
        let seconds = current_time.sec as f64 + (current_time.nsec as f64 / 1000000000.0f64);
        self.nb_frames += 1;
        let delta = seconds - self.last_time;
        self.last_time = seconds;

        if seconds - self.last_time_to_s >= 1.0f64 {
            println!("{} ms/frame", 1000.0f64 / self.nb_frames as f64);
            self.nb_frames = 0;
            self.last_time_to_s = seconds;
        }
        return delta;
    }
}
