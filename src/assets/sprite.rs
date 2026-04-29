use std::io::{Read, Seek};
use std::ops;
use std::sync::Arc;

use anyhow::anyhow;
use bitfield_struct::bitfield;
use byte::ctx::{Endianess, LittleEndian};
use byte::{BytesExt, TryRead};
use hashbrown::HashMap;

use super::AssetError;

/// Defines the properties of a map sprite.
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
    /// Returns the original coordinates `(x, y)` of the sprite.
    pub fn origin(&self) -> (i16, i16) {
        (self.origin_x, self.origin_y)
    }

    /// Returns the dimensions of the texture.
    pub fn texture_size(&self) -> (u16, u16) {
        (self.texture_width, self.texture_height)
    }

    /// Returns the ID of the texture.
    pub fn texture_id(&self) -> i32 {
        self.texture_id
    }

    /// Returns the flags associated with the sprite.
    pub fn flags(&self) -> SpriteFlags {
        self.flags
    }

    /// Returns the dimensions of the rendered sprite.
    pub fn size(&self) -> (u16, u16) {
        (self.render_width, self.render_height)
    }

    /// Returns the animation data for the sprite, if any.
    pub fn animation(&self) -> Animation {
        self.animation.clone()
    }
}

/// Flags dictating the behavior and properties of a sprite.
/// These represent various boolean properties and a 4-bit slope value.
/// Specifically:
/// - `slope`: 4 bits representing the slope of the terrain block.
/// - `is_flip`: Represents if the texture is horizontally flipped.
/// - `is_move_top`: Represents if the element should be moved to the top layer.
/// - `is_before_mobile`: Represents if the element should be drawn before mobile entities.
/// - `is_walkable`: Represents if the cell containing this element is walkable.
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

/// An animation defined for a sprite.
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

/// The frames constituting an animation.
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
    /// Returns the total duration of the animation.
    pub fn total_time(&self) -> u32 {
        self.total_time
    }

    /// Returns the width of a frame.
    pub fn width(&self) -> u16 {
        self.width
    }

    /// Returns the height of a frame.
    pub fn height(&self) -> u16 {
        self.height
    }

    /// Returns an iterator over the individual frames.
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

/// A single frame of an animation.
#[derive(Debug)]
pub struct Frame {
    pub time: u16,
    pub x: u16,
    pub y: u16,
}

/// A library of map sprite definitions.
#[derive(Debug)]
pub struct MapSpriteLibrary {
    elements: HashMap<i32, MapSpriteDefinition>,
}

impl MapSpriteLibrary {
    /// Loads a library from a zip archive.
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
