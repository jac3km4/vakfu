use std::fs::read_dir;
use std::path::Path;

use anyhow::Result;
use itertools::Itertools;

pub fn get_map_ids(map_dir: impl AsRef<Path>) -> Result<Vec<i32>> {
    let mut map_ids: Vec<i32> = read_dir(map_dir)?
        .map_ok(|entry| entry.path())
        .filter_ok(|entry| entry.extension() == Some("jar".as_ref()))
        .filter_map_ok(|entry| Some(entry.file_stem()?.to_str()?.parse()))
        .flatten_ok()
        .try_collect()?;
    map_ids.sort();
    Ok(map_ids)
}
