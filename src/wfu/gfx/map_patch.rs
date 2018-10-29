extern crate zip;

use std::io::Read;
use wfu::gfx::color_table::ColorTable;
use wfu::gfx::render_spec::DisplaySpec;
use wfu::gfx::render_spec::RenderSpec;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct MapPatch {
    pub min_x: i32,
    pub min_y: i32,
    pub min_z: i16,
    pub max_x: i32,
    pub max_y: i32,
    pub max_z: i16,
    pub elements: Vec<RenderSpec>,
}

pub fn parse_patch(name: &str) -> Option<(i32, i32)> {
    match name.split('_').collect::<Vec<&str>>()[..] {
        [a, b] => i32::from_str_radix(a, 10)
            .ok()
            .and_then(|x| i32::from_str_radix(b, 10).ok().map(|y| (x, y))),
        _ => None,
    }
}

impl<R: Read> Decoder<R> for MapPatch {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let min_x: i32 = cur.decode();
        let min_y: i32 = cur.decode();
        let min_z: i16 = cur.decode();
        let max_x: i32 = cur.decode();
        let max_y: i32 = cur.decode();
        let max_z: i16 = cur.decode();
        let count: u16 = cur.decode();
        let mut group_keys: Vec<i32> = Vec::with_capacity(count as usize);
        let mut layer_indexes: Vec<u8> = Vec::with_capacity(count as usize);
        let mut group_ids: Vec<i32> = Vec::with_capacity(count as usize);

        for _ in 0..count as usize {
            group_keys.push(cur.decode());
            layer_indexes.push(cur.decode());
            group_ids.push(cur.decode());
        }
        let colors: ColorTable = cur.decode();
        let map_x: i32 = cur.decode();
        let map_y: i32 = cur.decode();
        let rects: u16 = cur.decode();
        let mut elements: Vec<RenderSpec> = Vec::with_capacity((rects * 2) as usize);

        for _ in 0..rects {
            let rect_min_x = map_x + cur.decode::<u8>() as i32;
            let rect_max_x = map_x + cur.decode::<u8>() as i32;
            let rect_min_y = map_y + cur.decode::<u8>() as i32;
            let rect_max_y = map_y + cur.decode::<u8>() as i32;

            for cell_x in rect_min_x..rect_max_x {
                for cell_y in rect_min_y..rect_max_y {
                    let count: u8 = cur.decode();
                    for _ in 0..count {
                        let display: DisplaySpec = cur.decode();
                        let group_idx = cur.decode::<u16>() as usize;
                        let group_key = group_keys[group_idx];
                        let layer_idx = layer_indexes[group_idx];
                        let group_id = group_ids[group_idx];
                        let color_idx: u16 = cur.decode();
                        let element_colors = colors[color_idx as usize].to_owned();
                        let element = RenderSpec {
                            display,
                            cell_x,
                            cell_y,
                            group_key,
                            layer_idx,
                            group_id,
                            colors: element_colors,
                        };
                        elements.push(element);
                    }
                }
            }
        }
        MapPatch {
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
            elements,
        }
    }
}
