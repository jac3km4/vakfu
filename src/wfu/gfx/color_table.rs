use std::io::Read;
use std::ops::Index;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct ColorTable {
    colors: Vec<Vec<f32>>,
}

impl Index<usize> for ColorTable {
    type Output = Vec<f32>;

    fn index(&self, index: usize) -> &Vec<f32> {
        &self.colors[index]
    }
}

impl<R: Read> Decoder<R> for ColorTable {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let count: u16 = cur.decode();
        let mut colors: Vec<Vec<f32>> = Vec::with_capacity(count as usize);
        for i in 0..count as usize {
            let tpe: u8 = cur.decode();
            colors.push(allocate_colors(tpe));
            fill_colors(cur, &mut colors[i], tpe)
        }
        ColorTable { colors }
    }
}

fn allocate_colors(tpe: u8) -> Vec<f32> {
    let mut size: i32 = 0;
    size = size + (if (tpe & 0x1) == 0x1 { 3 } else { 0 });
    size = size + (if (tpe & 0x2) == 0x2 { 1 } else { 0 });
    size = size + (if (tpe & 0x4) == 0x4 { 2 } else { 1 });

    Vec::with_capacity(size as usize)
}

fn teint_value(v: u8) -> f32 {
    v as f32 / 255.0f32 + 0.5f32
}

fn fill_colors<R: Read>(cur: &mut DecoderCursor<R>, colors: &mut Vec<f32>, tpe: u8) -> () {
    if (tpe & 0x1) == 0x1 {
        colors.push(teint_value(cur.decode()));
        colors.push(teint_value(cur.decode()));
        colors.push(teint_value(cur.decode()));
    }
    if (tpe & 0x2) == 0x2 {
        colors.push(teint_value(cur.decode()));
    }
    if (tpe & 0x4) == 0x4 {
        if (tpe & 0x1) == 0x1 {
            colors.push(teint_value(cur.decode()));
            colors.push(teint_value(cur.decode()));
            colors.push(teint_value(cur.decode()));
        }
        if (tpe & 0x2) == 0x2 {
            colors.push(teint_value(cur.decode()));
        }
    }
}
