use std::collections::HashMap;
use std::io::{Read, Seek};

use anyhow::{anyhow, Result};
use bevy::sprite::Rect;
use byte::{BytesExt, TryRead};
use glam::Vec2;
use modular_bitfield::prelude::*;

use super::frames::Frames;

#[derive(Debug)]
pub struct MapElement {
    pub id: i32,
    pub origin_x: i16,
    pub origin_y: i16,
    pub img_width: u16,
    pub img_height: u16,
    pub texture_id: i32,
    pub flags: ElementFlags,
    pub visual_height: u8,
    pub visibility_mask: u8,
    pub export_mask: u8,
    pub shader: u8,
    pub animation: Option<Frames>,
    pub ground_sound: u8,
}

impl MapElement {
    #[inline]
    pub fn image_size(&self) -> Vec2 {
        Vec2::new(self.img_width as f32, self.img_height as f32)
    }

    pub fn size(&self) -> Vec2 {
        match self.animation {
            None => self.image_size(),
            Some(ref frames) => frames.frame_rects[0].size(),
        }
    }

    #[inline]
    pub fn origin(&self) -> Vec2 {
        Vec2::new(self.origin_x as f32, self.origin_y as f32)
    }

    #[inline]
    pub fn rect(&self) -> Rect {
        Rect {
            min: Vec2::ZERO,
            max: self.size(),
        }
    }
}

impl<'a> TryRead<'a> for MapElement {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let id: i32 = bytes.read(offset)?;
        let origin_x: i16 = bytes.read(offset)?;
        let origin_y: i16 = bytes.read(offset)?;
        let img_width: u16 = bytes.read(offset)?;
        let img_height: u16 = bytes.read(offset)?;
        let texture_id: i32 = bytes.read(offset)?;
        let flags: ElementFlags = bytes.read(offset)?;
        let visual_height: u8 = bytes.read(offset)?;
        let visibility_mask: u8 = bytes.read(offset)?;
        let export_mask: u8 = bytes.read(offset)?;
        let shader: u8 = bytes.read(offset)?;
        let frame_count: u8 = bytes.read(offset)?;
        let animation = if frame_count > 0 {
            Some(bytes.read_with(offset, frame_count)?)
        } else {
            None
        };
        let ground_sound = bytes.read(offset)?;

        let result = MapElement {
            id,
            origin_x,
            origin_y,
            img_width,
            img_height,
            texture_id,
            flags,
            visual_height,
            visibility_mask,
            export_mask,
            shader,
            animation,
            ground_sound,
        };
        Ok((result, *offset))
    }
}

#[bitfield]
#[derive(Debug)]
pub struct ElementFlags {
    pub slope: B4,
    pub is_flip: bool,
    pub is_move_top: bool,
    pub is_before_mobile: bool,
    pub is_walkable: bool,
}

impl<'a> TryRead<'a> for ElementFlags {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let byte = bytes.read(&mut 0)?;
        let result = ElementFlags::from_bytes([byte]);
        Ok((result, 1))
    }
}

#[derive(Debug)]
pub struct ElementLibrary {
    elements: HashMap<i32, MapElement>,
}

impl ElementLibrary {
    pub fn load<R: Seek + Read>(input: R) -> Result<Self> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut entry = archive.by_name("elements.lib")?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;

        let result = bytes
            .read(&mut 0)
            .map_err(|err| anyhow!("Read error: {:?}", err))?;
        Ok(result)
    }

    #[inline]
    pub fn get(&self, id: i32) -> Option<&MapElement> {
        self.elements.get(&id)
    }
}

impl<'a> TryRead<'a> for ElementLibrary {
    fn try_read(bytes: &'a [u8], _ctx: ()) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let count: u32 = bytes.read(offset)?;
        let mut elements = HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let element: MapElement = bytes.read(offset)?;
            elements.insert(element.id, element);
        }

        let result = Self { elements };
        Ok((result, *offset))
    }
}
