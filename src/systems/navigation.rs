use std::{env, path::PathBuf, process::Command};

#[derive(Debug)]
pub struct NavigationInfo {
    pub map_ids: Vec<i32>,
    pub current_index: usize,
    pub game_path: PathBuf,
}

pub fn start_other_vakfu(value: &i32, game_path: &PathBuf) {
    let args: Vec<_> = env::args().collect();
    Command::new(args.get(0).unwrap())
        .arg("--path")
        .arg(game_path)
        .arg("--map")
        .arg(value.to_string())
        .spawn()
        .ok();
}
