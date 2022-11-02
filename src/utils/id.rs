use std::str::FromStr;
use std::{fs::read_dir, path::PathBuf};

use anyhow::Result;

pub fn get_maps_id(map_dir: PathBuf) -> Result<Vec<i32>> {
    let maps_availables = read_dir(map_dir)?;
    let mut maps_id = Vec::new();
    const JAR_SUFFIX: &str = ".jar";
    for raw_path in maps_availables {
        match raw_path {
            Ok(path) => {
                let file_name = path.file_name();
                match file_name.to_str() {
                    Some(file_name) => {
                        if file_name.ends_with(JAR_SUFFIX) {
                            match file_name.strip_suffix(JAR_SUFFIX) {
                                Some(map_id) => match i32::from_str(map_id) {
                                    Ok(map_id) => {
                                        maps_id.push(map_id);
                                    }
                                    Err(_e) => {}
                                },
                                None => {}
                            }
                        }
                    }
                    None => {}
                };
            }
            Err(_e) => continue,
        };
    }
    maps_id.sort();
    return Ok(maps_id);
}
