extern crate itertools;

use std::io::Read;
use wfu::gfx::texture_coords::TextureCoords;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct ElementProperties(i8);

impl ElementProperties {
    pub fn is_flip(&self) -> bool {
        self.0 & 0x10 == 0x10
    }

    pub fn get_slope(&self) -> i8 {
        self.0 & 0xFi8
    }
}

pub struct ElementDefinition {
    pub id: i32,
    pub origin_x: i16,
    pub origin_y: i16,
    pub img_width: i16,
    pub img_height: i16,
    pub texture_id: i32,
    properties: ElementProperties,
    visual_height: u8,
    pub visibility_mask: u8,
    export_mask: u8,
    shader: u8,
    pub frames: Option<Frames>,
    pub texture_coords: TextureCoords,
    ground_sound: u8,
}

impl<R: Read> Decoder<R> for ElementDefinition {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let id: i32 = cur.decode();
        let origin_x: i16 = cur.decode();
        let origin_y: i16 = cur.decode();
        let img_width: i16 = cur.decode();
        let img_height: i16 = cur.decode();
        let gfx_id: i32 = cur.decode();
        let properties = ElementProperties(cur.decode());
        let visual_height: u8 = cur.decode();
        let visibility_mask: u8 = cur.decode();
        let export_mask: u8 = cur.decode();
        let shader: u8 = cur.decode();
        let anim_data: Option<AnimData> = cur.decode();
        let frames = anim_data.map(|data| data.as_frames(properties.is_flip()));
        let texture_coords = TextureCoords::from(img_width, img_height, properties.is_flip());
        let ground_sound: u8 = cur.decode();

        ElementDefinition {
            id,
            origin_x,
            origin_y,
            img_width,
            img_height,
            texture_id: gfx_id,
            properties,
            visual_height,
            visibility_mask,
            export_mask,
            shader,
            frames,
            texture_coords,
            ground_sound,
        }
    }
}

#[derive(Clone)]
pub struct Frames {
    total_time: i32,
    texture_coords: Vec<TextureCoords>,
    frame_offsets: Vec<i16>,
}

impl Frames {
    pub fn get_texture_coords(&self, time: u64) -> TextureCoords {
        let passed = time % self.total_time as u64;
        let idx = self
            .frame_offsets
            .iter()
            .take_while(|i| passed > (**i as u64))
            .count();
        self.texture_coords[idx]
    }
}

pub struct AnimData {
    total_time: i32,
    img_width: i16,
    img_height: i16,
    img_width_total: i16,
    img_height_total: i16,
    animation_times: Vec<i16>,
    coords: Vec<i16>,
}

impl AnimData {
    pub fn as_frames(&self, flip: bool) -> Frames {
        let frame_offsets = (1..self.animation_times.len() + 1)
            .map(|i| self.animation_times.iter().take(i).sum())
            .collect::<Vec<_>>();
        Frames {
            total_time: self.total_time,
            texture_coords: self.get_texture_coords(flip),
            frame_offsets,
        }
    }

    pub fn get_texture_coords(&self, flip: bool) -> Vec<TextureCoords> {
        TextureCoords::compute(
            &self.coords,
            self.img_width,
            self.img_height,
            self.img_width_total,
            self.img_height_total,
            flip,
        )
    }
}

impl<R: Read> Decoder<R> for Option<AnimData> {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let count: u8 = cur.decode();
        if count == 0 {
            None
        } else {
            let total_time: i32 = cur.decode();
            let img_width: i16 = cur.decode();
            let img_height: i16 = cur.decode();
            let img_width_total: i16 = cur.decode();
            let img_height_total: i16 = cur.decode();
            let animation_times: Vec<i16> = cur.decode_n(count.into());
            let coords: Vec<i16> = cur.decode_n(count as usize * 2);
            let data = AnimData {
                total_time,
                img_width,
                img_height,
                img_width_total,
                img_height_total,
                animation_times,
                coords,
            };
            Some(data)
        }
    }
}
