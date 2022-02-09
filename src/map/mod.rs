use std::io::{Read, Seek};

use anyhow::{anyhow, Result};
use byte::BytesExt;
use glam::{IVec2, Vec2};

use crate::map::chunk::MapChunk;

pub mod animation;
pub mod chunk;
#[allow(unused)]
pub mod element;
pub mod sprite;

const CELL_WIDTH: f32 = 86.;
const CELL_HEIGHT: f32 = 43.;
const ELEVATION_UNIT: f32 = 10.;

#[derive(Debug)]
pub struct Map {
    chunks: Vec<MapChunk>,
}

impl Map {
    pub fn load<R: Read + Seek>(input: R) -> Result<Map> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut chunks = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file
                .name()
                .trim_matches(|c| char::is_numeric(c) || c == '-')
                == "_"
            {
                let mut buffer = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut buffer)?;
                let chunk = buffer
                    .read(&mut 0)
                    .map_err(|err| anyhow!("Read error: {:?}", err))?;
                chunks.push(chunk);
            }
        }
        Ok(Map { chunks })
    }

    #[inline]
    pub fn chunks(&self) -> &[MapChunk] {
        &self.chunks
    }
}

#[inline]
pub fn iso_to_screen(vec: IVec2, height: i32) -> Vec2 {
    let x = (vec.x - vec.y) as f32 * CELL_WIDTH / 2.;
    let y = -(vec.x + vec.y) as f32 * CELL_HEIGHT / 2. + height as f32 * ELEVATION_UNIT;
    Vec2::new(x, y)
}
