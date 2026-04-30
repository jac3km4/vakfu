use std::collections::HashMap;
use std::io::{Read, Seek};
use std::sync::Arc;

use byte::ctx::LittleEndian;
use byte::{BytesExt, TryRead};

use crate::assets::AssetError;

pub struct LightDef {
    pub ambiance_light: [f32; 3],
    pub shadows: [f32; 3],
    pub lights: [f32; 3],
    pub allow_outdoor_lighting: bool,
    pub has_shadows: bool,
    pub night_light: [f32; 3],
}

impl LightDef {
    pub const DEFAULT: LightDef = LightDef {
        allow_outdoor_lighting: false,
        ambiance_light: [1f32, 1f32, 1f32],
        shadows: [1f32, 1f32, 1f32],
        lights: [1f32, 1f32, 1f32],
        has_shadows: false,
        night_light: [0f32, 0f32, 0f32],
    };
}

#[derive(Clone)]
pub struct LightCell {
    pub cell_x: i32,
    pub cell_y: i32,
    pub layer_colors: HashMap<i32, Arc<LightDef>>,
}

impl<'a> TryRead<'a, LittleEndian> for LightCell {
    fn try_read(bytes: &'a [u8], ctx: LittleEndian) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        let x = bytes.read::<i16>(offset, ctx)?;
        let y = bytes.read::<i16>(offset, ctx)?;

        let defcount = bytes.read::<u16>(offset, ctx)?;
        let mut layer: Vec<Arc<LightDef>> = Vec::with_capacity(18 * 18);

        for _ in 0..defcount {
            let outdoor = bytes.read::<u8>(offset, ctx)? != 0;
            let ambiance = bytes.read::<i32>(offset, ctx)?;
            let shadow = bytes.read::<i32>(offset, ctx)?;
            let light = bytes.read::<i32>(offset, ctx)?;
            let night = [0f32, 0f32, 0f32];

            let def = LightDef {
                allow_outdoor_lighting: outdoor,
                ambiance_light: [
                    (ambiance & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 8 & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 16 & 0xFF) as f32 / 255f32 * 2f32,
                ],
                shadows: [
                    (shadow & 0xFF) as f32 / 255f32,
                    (shadow >> 8 & 0xFF) as f32 / 255f32,
                    (shadow >> 16 & 0xFF) as f32 / 255f32,
                ],
                lights: [
                    (light & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 8 & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 16 & 0xFF) as f32 / 255f32 - 0.5f32,
                ],
                has_shadows: shadow as u32 != 0xFF80_8080,
                night_light: night,
            };
            layer.push(Arc::new(def));
        }

        let _layer_count = bytes.read::<i16>(offset, ctx)?;
        let lcount = bytes.read::<i16>(offset, ctx)?;

        let mut layer_colors = HashMap::new();
        let default_layer = Arc::new(LightDef::DEFAULT);

        for _ in 0..lcount {
            let k = bytes.read::<u16>(offset, ctx)?;
            let idx = bytes.read::<u16>(offset, ctx)?;
            if (idx as usize) < layer.len() {
                layer_colors.insert(k as i32, layer[idx as usize].clone());
            } else {
                layer_colors.insert(k as i32, default_layer.clone());
            }
        }

        Ok((
            LightCell {
                cell_x: x as i32 * 18,
                cell_y: y as i32 * 18,
                layer_colors,
            },
            *offset,
        ))
    }
}

#[derive(Default)]
pub struct LightMap {
    pub light_maps: HashMap<i32, LightCell>,
}

impl LightMap {
    pub fn load<R: Read + Seek>(input: R) -> Result<LightMap, AssetError> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut light_maps = HashMap::new();

        for i in 0..archive.len() {
            let mut entry = archive.by_index(i)?;
            if entry
                .name()
                .trim_matches(|c| char::is_numeric(c) || c == '-')
                == "_"
            {
                let mut buffer = Vec::with_capacity(entry.size() as usize);
                entry.read_to_end(&mut buffer)?;

                let (cell, _) = LightCell::try_read(&buffer, LittleEndian)?;
                let mapx = cell.cell_x / 18;
                let mapy = cell.cell_y / 18;
                let hash = mapx << 16 | (mapy & 0xFFFF);
                light_maps.insert(hash, cell);
            }
        }
        Ok(LightMap { light_maps })
    }

    pub fn get_color(&self, cell_x: i32, cell_y: i32, layer_idx: i32) -> Arc<LightDef> {
        let map_x = (cell_x as f32 / 18f32).floor() as i32;
        let map_y = (cell_y as f32 / 18f32).floor() as i32;
        let key = map_x << 16 | map_y;

        if let Some(cell) = self.light_maps.get(&key) {
            let hash = (cell_x - cell.cell_x) + ((cell_y - cell.cell_y) + (layer_idx * 18)) * 18;
            if let Some(color) = cell.layer_colors.get(&hash) {
                color.clone()
            } else {
                Arc::new(LightDef::DEFAULT)
            }
        } else {
            Arc::new(LightDef::DEFAULT)
        }
    }
}
