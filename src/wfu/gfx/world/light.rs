extern crate zip;

use std::io::{Read, Seek};
use std::mem::swap;
use wfu::io::decoder::{Decoder, DecoderCursor};
use std::collections::HashMap;

#[derive(Clone)]
pub struct LightDef {
    pub ambianceLight: Vec<f32>,
    pub shadows: Vec<f32>,
    pub lights: Vec<f32>,
    pub allowOutdoorLighting: bool,
    pub hasShadows: bool,
    pub merged: Vec<f32>,
    pub nightLight: Vec<f32>,
}

pub struct LightCell {
    pub cellX: i32,
    pub cellY: i32,
    pub layerColors: Option<HashMap<i32, LightDef>>,
}

impl LightCell {
    pub fn get_noLight(&self) -> LightDef {
        return LightDef {
            allowOutdoorLighting: false,
            ambianceLight: vec![1f32, 1f32, 1f32],
            shadows: vec![1f32, 1f32, 1f32],
            lights: vec![1f32, 1f32, 1f32],
            hasShadows: false,
            merged: vec![1f32],
            nightLight: vec![0f32, 0f32, 0f32],
        };
    }
}

pub struct LightMap {
    pub lightmaps: HashMap<i32, LightCell>,
}

impl LightMap {
    pub fn get_noCell(&self, mapx:i32, mapy:i32) -> LightCell {
        return LightCell {
            cellX: mapx,
            cellY: mapy,
            layerColors: None
        };
    }
}

impl<R: Read> Decoder<R> for LightCell {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let x: i16 = cur.decode();
        let y: i16 = cur.decode();
        let numCells: i16 = 18 * 18;
        let mut layer: Vec<LightDef> = Vec::with_capacity(numCells as usize);
        let defcount: i16 = cur.decode();

        for i in 0..(defcount & 0xFFFF) {
            let outdoor: bool = cur.decode();
            let ambiance: i32 = cur.decode();
            let shadow: i32 = cur.decode();
            let light: i32 = cur.decode();

            let night = if light as u32 == 0xFF808080 { 
                vec![0f32]
            } else {
                vec![0f32, 0f32, 0f32]
            };

            let def = LightDef {
                allowOutdoorLighting: outdoor,
                ambianceLight: vec! [
                    (ambiance & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 8 & 0xFF) as f32 / 255f32 * 2f32,
                    (ambiance >> 16 & 0xFF) as f32 / 255f32 * 2f32,
                ],
                shadows: vec! [
                    (shadow & 0xFF) as f32 / 255f32,
                    (shadow >> 8 & 0xFF) as f32 / 255f32,
                    (shadow >> 16 & 0xFF) as f32 / 255f32,
                ],
                lights: vec! [
                    (light & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 8 & 0xFF) as f32 / 255f32 - 0.5f32,
                    (light >> 16 & 0xFF) as f32 / 255f32 - 0.5f32,
                ],
                hasShadows: shadow as u32 != 0xFF808080,
                merged: vec![0f32],
                nightLight: night,
            };

            layer.push(def);
        }

        let layerCount :i16 = cur.decode();
        let lcount :i16 = cur.decode();
        let mut layerColor = HashMap::new();
         
        for _ in 0..lcount {
            let k: u16 = cur.decode();
            let idx: u16 = cur.decode();

            if idx < layer.len() as u16 {
                layerColor.insert(k as i32, layer[idx as usize].clone())
            } else {
                layerColor.insert(k as i32, 
                    LightDef {
                        allowOutdoorLighting: false,
                        ambianceLight: vec![1f32, 1f32, 1f32],
                        shadows: vec![1f32, 1f32, 1f32],
                        lights: vec![1f32, 1f32, 1f32],
                        hasShadows: false,
                        merged: vec![1f32],
                        nightLight: vec![0f32, 0f32, 0f32],
                    })
            };
        }

        LightCell {
            cellX: x as i32 * 18,
            cellY: y as i32 * 18,
            layerColors: Some(layerColor),
        }
    }
}
