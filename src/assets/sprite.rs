use std::io::{Read, Seek};
use std::ops;
use std::sync::Arc;

use anyhow::anyhow;
use bitfield_struct::bitfield;
use byte::ctx::{Endianess, LittleEndian};
use byte::{BytesExt, TryRead};
use hashbrown::HashMap;

use super::AssetError;

#[derive(Debug, TryRead)]
pub struct MapSpriteDefinition {
    id: i32,
    origin_x: i16,
    origin_y: i16,
    texture_width: u16,
    texture_height: u16,
    render_width: u16,
    render_height: u16,
    texture_id: i32,
    flags: SpriteFlags,
    visual_height: u8,
    visibility_mask: u8,
    export_mask: u8,
    shader: u8,
    frame_count: u8,
    #[byte(ctx = (ctx, *frame_count))]
    animation: Animation,
    ground_sound: u8,
}

impl MapSpriteDefinition {
    pub fn origin(&self) -> (i16, i16) {
        (self.origin_x, self.origin_y)
    }

    pub fn texture_size(&self) -> (u16, u16) {
        (self.texture_width, self.texture_height)
    }

    pub fn texture_id(&self) -> i32 {
        self.texture_id
    }

    pub fn flags(&self) -> SpriteFlags {
        self.flags
    }

    pub fn size(&self) -> (u16, u16) {
        (self.render_width, self.render_height)
    }

    pub fn animation(&self) -> Animation {
        self.animation.clone()
    }
}

#[bitfield(u8)]
pub struct SpriteFlags {
    #[bits(4)]
    pub slope: u8,
    pub is_flip: bool,
    pub is_move_top: bool,
    pub is_before_mobile: bool,
    pub is_walkable: bool,
}

impl<C> TryRead<'_, C> for SpriteFlags {
    fn try_read(bytes: &[u8], ctx: C) -> Result<(Self, usize), byte::Error> {
        let (val, offset) = u8::try_read(bytes, ctx)?;
        Ok((Self::from_bits(val), offset))
    }
}

#[derive(Debug, Clone)]
pub enum Animation {
    None,
    Frames(Arc<Frames>),
}

impl<C: Endianess> TryRead<'_, (C, u8)> for Animation {
    fn try_read(bytes: &[u8], (ctx, count): (C, u8)) -> Result<(Self, usize), byte::Error> {
        if count == 0 {
            return Ok((Self::None, 0));
        }
        let (animation, offset) = Frames::try_read(bytes, (ctx, count))?;
        Ok((Self::Frames(Arc::new(animation)), offset))
    }
}

#[derive(Debug, Default)]
pub struct Frames {
    total_time: u32,
    width: u16,
    height: u16,
    full_width: u16,
    full_height: u16,
    frame_durations: Vec<u16>,
    frame_coords: Vec<[u16; 2]>,
}

impl Frames {
    pub fn total_time(&self) -> u32 {
        self.total_time
    }

    pub fn width(&self) -> u16 {
        self.width
    }

    pub fn height(&self) -> u16 {
        self.height
    }

    pub fn iter(&self) -> impl ExactSizeIterator<Item = Frame> {
        let mut time = 0;
        self.frame_durations
            .iter()
            .zip(self.frame_coords.iter())
            .map(move |(&duration, &[x, y])| {
                let frame = Frame { time, x, y };
                time += duration;
                frame
            })
    }
}

impl<C: Endianess> TryRead<'_, (C, u8)> for Frames {
    fn try_read(bytes: &[u8], (ctx, count): (C, u8)) -> Result<(Self, usize), byte::Error> {
        let offset = &mut 0;

        let total_time: u32 = bytes.read(offset, ctx)?;
        let width: u16 = bytes.read(offset, ctx)?;
        let height: u16 = bytes.read(offset, ctx)?;
        let full_width: u16 = bytes.read(offset, ctx)?;
        let full_height: u16 = bytes.read(offset, ctx)?;
        let frame_durations = bytes
            .read_iter(offset, ctx)
            .take(count.into())
            .collect::<Result<Vec<u16>, _>>()?;
        let frame_coords = bytes
            .read_iter::<[u16; 2]>(offset, ctx)
            .take(count as usize)
            .collect::<Result<Vec<[u16; 2]>, _>>()?;

        Ok((
            Frames {
                total_time,
                width,
                height,
                full_width,
                full_height,
                frame_durations,
                frame_coords,
            },
            *offset,
        ))
    }
}

#[derive(Debug)]
pub struct Frame {
    pub time: u16,
    pub x: u16,
    pub y: u16,
}

#[derive(Debug)]
pub struct MapSpriteLibrary {
    elements: HashMap<i32, MapSpriteDefinition>,
}

impl MapSpriteLibrary {
    pub fn load<R: Seek + Read>(input: R) -> Result<Self, AssetError> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut entry = archive.by_name("elements.lib")?;
        let mut bytes = Vec::with_capacity(entry.size() as usize);
        entry.read_to_end(&mut bytes)?;

        let (result, _) = MapSpriteLibrary::try_read(&bytes, LittleEndian)?;
        Ok(result)
    }
}

impl ops::Index<i32> for MapSpriteLibrary {
    type Output = MapSpriteDefinition;

    fn index(&self, index: i32) -> &Self::Output {
        &self.elements[&index]
    }
}

impl<'a, C: Endianess> TryRead<'a, C> for MapSpriteLibrary {
    fn try_read(bytes: &'a [u8], ctx: C) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;

        let count: u32 = bytes.read(offset, ctx)?;
        let mut elements = HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let element: MapSpriteDefinition = bytes.read(offset, ctx)?;
            elements.insert(element.id, element);
        }
        Ok((Self { elements }, *offset))
    }
}
