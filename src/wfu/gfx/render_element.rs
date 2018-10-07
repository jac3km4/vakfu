extern crate zip;

use std::io::{Read, Seek};
use wfu::gfx::color_table::ColorTable;
use wfu::gfx::world::world_element::WorldElement;
use wfu::io::decoder::{Decoder, DecoderCursor};
use wfu::util::{iso_to_screen_x, iso_to_screen_y};

pub struct RenderElementPatch {
    min_x: i32,
    min_y: i32,
    min_z: i16,
    max_x: i32,
    max_y: i32,
    max_z: i16,
    pub elements: Vec<RenderElement>,
}

impl RenderElementPatch {
    pub fn load<R: Seek + Read>(
        archive: &mut zip::ZipArchive<R>,
        x: i32,
        y: i32,
    ) -> Option<RenderElementPatch> {
        match archive.by_name(format!("{}_{}", x, y).as_ref()) {
            Ok(entry) => Some(
                DecoderCursor::new(entry)
                    .decode::<RenderElementPatch>()
                    .sorted(),
            ),
            _ => None,
        }
    }

    pub fn sorted(self) -> RenderElementPatch {
        use std::num::Wrapping;

        let mut indexes = self
            .elements
            .iter()
            .enumerate()
            .map(|tuple| (Wrapping(tuple.1.hashcode()) << 13usize).0 + tuple.0 as i64)
            .collect::<Vec<_>>();

        indexes.sort();

        let mut output: Vec<RenderElement> = Vec::with_capacity(self.elements.len());

        let mut insert_idx = 0;
        for code in indexes {
            let idx = code & 0x3FFFi64;
            let mut element = self.elements[idx as usize].clone();
            element.z = insert_idx;
            if code < 0 {
                output.push(element)
            } else {
                output.insert(insert_idx as usize, element);
                insert_idx += 1;
            }
        }
        RenderElementPatch {
            elements: output,
            ..self
        }
    }
}

impl<R: Read> Decoder<R> for RenderElementPatch {
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
        let mut elements: Vec<RenderElement> = Vec::with_capacity((rects * 2) as usize);

        for _ in 0..rects {
            let rect_min_x = map_x + cur.decode::<u8>() as i32;
            let rect_max_x = map_x + cur.decode::<u8>() as i32;
            let rect_min_y = map_y + cur.decode::<u8>() as i32;
            let rect_max_y = map_y + cur.decode::<u8>() as i32;

            for cell_x in rect_min_x..rect_max_x {
                for cell_y in rect_min_y..rect_max_y {
                    let count: u8 = cur.decode();
                    for _ in 0..count {
                        let display: DisplayElement = cur.decode();
                        let group_idx = cur.decode::<u16>() as usize;
                        let group_key = group_keys[group_idx];
                        let layer_idx = layer_indexes[group_idx];
                        let group_id = group_ids[group_idx];
                        let color_idx: u16 = cur.decode();
                        let element_colors = colors[color_idx as usize].to_owned();
                        let element = RenderElement {
                            display,
                            cell_x,
                            cell_y,
                            group_key,
                            layer_idx,
                            group_id,
                            colors: element_colors,
                            z: 0,
                        };
                        elements.push(element);
                    }
                }
            }
        }
        RenderElementPatch {
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

#[derive(Clone)]
pub struct RenderElement {
    pub display: DisplayElement,
    pub cell_x: i32,
    pub cell_y: i32,
    group_key: i32,
    layer_idx: u8,
    group_id: i32,
    pub colors: Vec<f32>,
    pub z: i32,
}

impl RenderElement {
    pub fn get_x(&self, world_element: &WorldElement) -> i32 {
        iso_to_screen_x(self.cell_x, self.cell_y) - world_element.origin_x as i32
    }

    pub fn get_y(&self, world_element: &WorldElement) -> i32 {
        iso_to_screen_y(
            self.cell_x,
            self.cell_y,
            self.display.cell_z as i32 - self.display.height as i32,
        ) + world_element.origin_y as i32
    }

    pub fn hashcode(&self) -> i64 {
        (self.cell_y as i64 + 8192i64 & 0x3FFFi64) << 34i64
            | (self.cell_x as i64 + 8192i64 & 0x3FFFi64) << 19i64
            | (self.display.altitude_order as i64 & 0x1FFFi64) << 6i64
    }
}

#[derive(Clone)]
pub struct DisplayElement {
    cell_z: i16,
    height: u8,
    altitude_order: u8,
    occluder: bool,
    tpe: u8,
    pub element_id: i32,
}

impl<R: Read> Decoder<R> for DisplayElement {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let cell_z: i16 = cur.decode();
        let height: u8 = cur.decode();
        let altitude_order: u8 = cur.decode();
        let byte: u8 = cur.decode();
        let occluder = byte & 0b0000_0001 == 0b0000_0001;
        let tpe = (byte & 0b0000_1110) >> 1;
        let element_id: i32 = cur.decode();
        DisplayElement {
            cell_z,
            height,
            altitude_order,
            occluder,
            tpe,
            element_id,
        }
    }
}
