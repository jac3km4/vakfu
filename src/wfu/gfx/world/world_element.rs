use std::io::Read;
use wfu::gfx::texture_coords::TextureCoords;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct WorldElement {
    pub id: i32,
    pub origin_x: i16,
    pub origin_y: i16,
    pub img_width: i16,
    pub img_height: i16,
    pub gfx_id: i32,
    pub properties_flag: u8,
    visual_height: u8,
    pub visibility_mask: u8,
    export_mask: u8,
    shader: u8,
    pub anim_data: AnimData,
    pub texture_coords: TextureCoords,
    ground_sound: u8,
}

impl<R: Read> Decoder<R> for WorldElement {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let id: i32 = cur.decode();
        let origin_x: i16 = cur.decode();
        let origin_y: i16 = cur.decode();
        let img_width: i16 = cur.decode();
        let img_height: i16 = cur.decode();
        let gfx_id: i32 = cur.decode();
        let properties_flag: u8 = cur.decode();
        let visual_height: u8 = cur.decode();
        let visibility_mask: u8 = cur.decode();
        let export_mask: u8 = cur.decode();
        let shader: u8 = cur.decode();
        let flip = properties_flag & 0x10 == 0x10;
        let anim_data: AnimData = cur.decode();
        let texture_coords = TextureCoords::from(img_width, img_height, flip);
        let ground_sound: u8 = cur.decode();

        WorldElement {
            id,
            origin_x,
            origin_y,
            img_width,
            img_height,
            gfx_id,
            properties_flag,
            visual_height,
            visibility_mask,
            export_mask,
            shader,
            anim_data,
            texture_coords,
            ground_sound,
        }
    }
}

pub struct AnimData {
    pub total_time: i32,
    img_width: i16,
    img_height: i16,
    img_width_total: i16,
    img_height_total: i16,
    animation_times: Vec<i16>,
    coords: Vec<i16>,
}

impl AnimData {
    pub fn get_texture_coords(&self, flip: bool) -> Vec<TextureCoords> {
        TextureCoords::compute(
            self.coords.as_ref(),
            self.img_width,
            self.img_height,
            self.img_width_total,
            self.img_height_total,
            flip,
        )
    }
}

impl<R: Read> Decoder<R> for AnimData {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let count: u8 = cur.decode();
        if count == 0 {
            return AnimData {
                total_time: 0,
                img_width: 0,
                img_height: 0,
                img_width_total: 0,
                img_height_total: 0,
                animation_times: vec![],
                coords: vec![],
            };
        }
        let total_time: i32 = cur.decode();
        let img_width: i16 = cur.decode();
        let img_height: i16 = cur.decode();
        let img_width_total: i16 = cur.decode();
        let img_height_total: i16 = cur.decode();
        let animation_times: Vec<i16> = cur.decode_n(count.into());
        let coords: Vec<i16> = cur.decode_n(count as usize * 2);
        return AnimData {
            total_time,
            img_width,
            img_height,
            img_width_total,
            img_height_total,
            animation_times,
            coords,
        };
    }
}
