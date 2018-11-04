extern crate zip;

use std::collections::HashMap;
use std::io::{Read, Seek};
use std::rc::Rc;
use wfu::gfx::map_patch::parse_patch;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct LightDef {
    pub ambiance_light: [f32; 3],
    pub shadows: [f32; 3],
    pub lights: [f32; 3],
    pub allow_outdoor_lighting: bool,
    pub has_shadows: bool,
    pub night_light: [f32; 3],
}

impl LightDef {
    const DEFAULT: LightDef = LightDef {
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
    pub layer_colors: HashMap<i32, Rc<LightDef>>,
}

pub struct LightMap {
    pub light_maps: HashMap<i32, LightCell>,
}

impl LightMap {
    pub fn load<S: Read + Seek>(reader: S) -> LightMap {
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        let mut light_maps: HashMap<i32, LightCell> = HashMap::new();
        for i in 0..archive.len() {
            let entry = archive.by_index(i).unwrap();
            match parse_patch(&entry.name()) {
                Some(_) => {
                    let cell: LightCell = DecoderCursor::new(entry).decode();
                    let mapx = cell.cell_x / 18;
                    let mapy = cell.cell_y / 18;
                    let hash = mapx << 16 | (mapy & 0xFFFF);
                    light_maps.insert(hash, cell);
                }
                _ => (),
            }
        }
        return LightMap { light_maps };
    }

    pub fn get_color(&self, cell_x: i32, cell_y: i32, layer_idx: i32) -> Rc<LightDef> {
        let map_x = (cell_x as f32 / 18f32).floor() as i32;
        let map_y = (cell_y as f32 / 18f32).floor() as i32;
        let key = map_x << 16 | (map_y & 0xFFFF);

        let default = LightCell {
            cell_x,
            cell_y,
            layer_colors: HashMap::new(),
        };
        let cell = self.light_maps.get(&key).unwrap_or(&default);

        let hash = (cell_x - cell.cell_x) + ((cell_y - cell.cell_y) + (layer_idx * 18)) * 18;
        cell.layer_colors
            .get(&hash)
            .unwrap_or(&Rc::new(LightDef::DEFAULT))
            .clone()
    }
}

impl<R: Read> Decoder<R> for LightCell {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let x: i16 = cur.decode();
        let y: i16 = cur.decode();

        let mut layer: Vec<Rc<LightDef>> = Vec::with_capacity(18usize * 18usize);
        let defcount: u16 = cur.decode();

        for _ in 0..defcount {
            let outdoor: bool = cur.decode();
            let ambiance: i32 = cur.decode();
            let shadow: i32 = cur.decode();
            let light: i32 = cur.decode();
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
                has_shadows: shadow as u32 != 0xFF808080,
                night_light: night,
            };
            layer.push(Rc::new(def));
        }
        let layer_count: i16 = cur.decode();
        let lcount: i16 = cur.decode();
        let mut layer_colors = HashMap::new();
        let default_layer = Rc::new(LightDef::DEFAULT);

        for _ in 0..lcount {
            let k: u16 = cur.decode();
            let idx: u16 = cur.decode();
            if idx < layer.len() as u16 {
                layer_colors.insert(k as i32, layer[idx as usize].clone())
            } else {
                layer_colors.insert(k as i32, default_layer.clone())
            };
        }
        LightCell {
            cell_x: x as i32 * 18,
            cell_y: y as i32 * 18,
            layer_colors,
        }
    }
}
