extern crate vulkano;
extern crate zip;

use std::io::{Read, Seek};
use wfu::io::decoder::{Decoder, DecoderCursor};
use wfu::util::indexed::Indexed;

pub struct TgamTexture {
    pub x: i16,
    pub y: i16,
    pub data: Vec<u8>,
    mask: AlphaMask,
}

pub struct AlphaMask {
    mask: Vec<u8>,
    layer_width: i16,
    resize: i8,
}

impl<R: Read> Decoder<R> for TgamTexture {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let resize_mask = cur.decode::<i8>() == 109;
        let mut header: [u8; 3] = [0, 0, 0];
        cur.reader.read_exact(&mut header).unwrap();
        let img_width: i16 = cur.decode();
        let img_height: i16 = cur.decode();
        let tga_size: i32 = cur.decode();
        let mask_size: i32 = cur.decode();
        let mask_resize = if resize_mask { cur.decode::<i8>() } else { 1 };
        let mut tga_data: Vec<u8> = Vec::with_capacity(tga_size as usize);
        unsafe { tga_data.set_len(tga_size as usize) };
        cur.reader.read_exact(tga_data.as_mut()).unwrap();
        let mut mask_data: Vec<u8> = Vec::with_capacity(mask_size as usize);
        unsafe { mask_data.set_len(mask_size as usize) };
        cur.reader.read_exact(mask_data.as_mut()).unwrap();
        let mask = AlphaMask {
            mask: mask_data,
            layer_width: img_width,
            resize: mask_resize,
        };
        TgamTexture {
            x: img_width,
            y: img_height,
            data: tga_data,
            mask,
        }
    }
}

pub struct TgamLoader<R: Read + Seek> {
    archive: zip::ZipArchive<R>,
}

impl<R: Read + Seek> TgamLoader<R> {
    pub fn new(reader: R) -> TgamLoader<R> {
        let archive = zip::ZipArchive::new(reader).unwrap();
        TgamLoader { archive }
    }
}

impl<R: Read + Seek> Indexed<i32, TgamTexture> for TgamLoader<R> {
    fn at(&mut self, id: i32) -> Option<TgamTexture> {
        use self::zip::result::ZipError;
        use wfu::io::decoder::DecoderCursor;

        match self.archive.by_name(&format!("gfx/{}.tgam", id)) {
            Err(ZipError::FileNotFound) => None,
            Ok(entry) => Some(DecoderCursor::new(entry).decode()),
            _ => panic!("Unknown zip file format"),
        }
    }
}
