use std::io::Read;
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::io::decoder::{Decoder, DecoderCursor};
use wfu::util::{iso_to_screen_x, iso_to_screen_y};

#[derive(Clone)]
pub struct RenderSpec {
    pub display: DisplaySpec,
    pub cell_x: i32,
    pub cell_y: i32,
    pub group_key: i32,
    pub layer_idx: u8,
    pub group_id: i32,
    pub colors: Vec<f32>,
}

impl RenderSpec {
    pub fn get_x(&self, world_element: &ElementDefinition) -> f32 {
        iso_to_screen_x(self.cell_x, self.cell_y) - world_element.origin_x as f32
    }

    pub fn get_y(&self, world_element: &ElementDefinition) -> f32 {
        iso_to_screen_y(
            self.cell_x,
            self.cell_y,
            (self.display.cell_z - self.display.height as i16) as i32,
        ) + world_element.origin_y as f32
    }

    pub fn hashcode(&self) -> i64 {
        (self.cell_y as i64 + 8192i64 & 0x3FFFi64) << 34i64
            | (self.cell_x as i64 + 8192i64 & 0x3FFFi64) << 19i64
            | (self.display.altitude_order as i64 & 0x1FFFi64) << 6i64
    }
}

#[derive(Clone)]
pub struct DisplaySpec {
    cell_z: i16,
    height: u8,
    altitude_order: u8,
    occluder: bool,
    tpe: u8,
    pub element_id: i32,
}

impl<R: Read> Decoder<R> for DisplaySpec {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let cell_z: i16 = cur.decode();
        let height: u8 = cur.decode();
        let altitude_order: u8 = cur.decode();
        let byte: u8 = cur.decode();
        let occluder = byte & 0b0000_0001 == 0b0000_0001;
        let tpe = (byte & 0b0000_1110) >> 1;
        let element_id: i32 = cur.decode();
        DisplaySpec {
            cell_z,
            height,
            altitude_order,
            occluder,
            tpe,
            element_id,
        }
    }
}
