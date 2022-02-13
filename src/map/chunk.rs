use std::borrow::Cow;

use bevy::prelude::Color;
use byte::ctx::Bytes;
use byte::{BytesExt, TryRead};

use super::sprite::MapSprite;

#[derive(Debug)]
pub struct MapChunk {
    pub map_x: i32,
    pub map_y: i32,
    pub min_x: i32,
    pub min_y: i32,
    pub min_z: i16,
    pub max_x: i32,
    pub max_y: i32,
    pub max_z: i16,
    pub sprites: Vec<MapSprite>,
}

impl<'a> TryRead<'a> for MapChunk {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let min_x: i32 = bytes.read(offset)?;
        let min_y: i32 = bytes.read(offset)?;
        let min_z: i16 = bytes.read(offset)?;
        let max_x: i32 = bytes.read(offset)?;
        let max_y: i32 = bytes.read(offset)?;
        let max_z: i16 = bytes.read(offset)?;
        let count: u16 = bytes.read(offset)?;
        let mut group_keys: Vec<i32> = Vec::with_capacity(count as usize);
        let mut layer_indexes: Vec<u8> = Vec::with_capacity(count as usize);
        let mut group_ids: Vec<i32> = Vec::with_capacity(count as usize);

        for _ in 0..count as usize {
            group_keys.push(bytes.read(offset)?);
            layer_indexes.push(bytes.read(offset)?);
            group_ids.push(bytes.read(offset)?);
        }
        let colors: Colors = bytes.read(offset)?;
        let map_x: i32 = bytes.read(offset)?;
        let map_y: i32 = bytes.read(offset)?;
        let rects: u16 = bytes.read(offset)?;
        let mut sprites: Vec<MapSprite> = Vec::with_capacity(rects as usize * 2);

        for _ in 0..rects {
            let rect_min_x = map_x + bytes.read::<u8>(offset)? as i32;
            let rect_max_x = map_x + bytes.read::<u8>(offset)? as i32;
            let rect_min_y = map_y + bytes.read::<u8>(offset)? as i32;
            let rect_max_y = map_y + bytes.read::<u8>(offset)? as i32;

            for cell_x in rect_min_x..rect_max_x {
                for cell_y in rect_min_y..rect_max_y {
                    let count: u8 = bytes.read(offset)?;
                    for _ in 0..count {
                        let cell_z = bytes.read(offset)?;
                        let height = bytes.read(offset)?;
                        let altitude_order = bytes.read(offset)?;
                        let tag = bytes.read(offset)?;
                        let element_id = bytes.read(offset)?;
                        let group_idx: u16 = bytes.read(offset)?;
                        let group_key = group_keys[group_idx as usize];
                        let group_id = group_ids[group_idx as usize];
                        let layer = layer_indexes[group_idx as usize];
                        let color_idx: u16 = bytes.read(offset)?;
                        let color = colors.get(color_idx);
                        let element = MapSprite {
                            cell_x,
                            cell_y,
                            cell_z,
                            height,
                            altitude_order,
                            tag,
                            element_id,
                            group_key,
                            group_id,
                            layer,
                            color,
                        };
                        sprites.push(element);
                    }
                }
            }
        }
        let chunk = MapChunk {
            map_x,
            map_y,
            min_x,
            min_y,
            min_z,
            max_x,
            max_y,
            max_z,
            sprites,
        };
        Ok((chunk, *offset))
    }
}

struct Colors<'a> {
    table: Vec<Cow<'a, [u8]>>,
}

impl<'a> Colors<'a> {
    fn get(&self, idx: u16) -> Color {
        match self.table.get(idx as usize).map(|buf| &buf[..]) {
            Some([r, g, b, a]) => Color::rgba_linear(
                teint(i8::from_ne_bytes([*r])),
                teint(i8::from_ne_bytes([*g])),
                teint(i8::from_ne_bytes([*b])),
                teint(i8::from_ne_bytes([*a])),
            ),
            Some([r, g, b]) => Color::rgb_linear(
                teint(i8::from_ne_bytes([*r])),
                teint(i8::from_ne_bytes([*g])),
                teint(i8::from_ne_bytes([*b])),
            ),
            _ => Color::rgb_linear(0.5, 0.5, 0.5),
        }
    }
}

impl<'a> TryRead<'a> for Colors<'a> {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let count: u16 = bytes.read(offset)?;
        let mut table = Vec::with_capacity(count as usize);
        for _ in 0..count as usize {
            let size = size_from_tag(bytes.read(offset)?);
            let bytes = bytes.read_with(offset, Bytes::Len(size))?;
            table.push(Cow::Borrowed(bytes))
        }
        Ok((Colors { table }, *offset))
    }
}

fn size_from_tag(tag: u8) -> usize {
    let a = if tag & 0x1 == 0x1 { 3 } else { 0 };
    let b = if tag & 0x2 == 0x2 { 1 } else { 0 };
    let c = if tag & 0x4 == 0x4 { 2 } else { 1 };
    (a + b) * c
}

#[inline]
fn teint(v: i8) -> f32 {
    (v as f32 / 255.0f32) + 0.5f32
}
