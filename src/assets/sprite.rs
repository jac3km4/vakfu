use std::io::{Read, Seek};
use std::ops;
use std::sync::Arc;

use bitfield_struct::bitfield;
use byte::ctx::{Endianess, LittleEndian};
use byte::{BytesExt, TryRead};
use hashbrown::HashMap;

use super::AssetError;

/// Defines the properties of a map sprite.
#[derive(Debug, TryRead)]
pub struct MapSpriteDefinition {
    /// The unique identifier for this sprite definition.
    id: i32,
    /// The X coordinate of the sprite's visual origin relative to the cell center.
    origin_x: i16,
    /// The Y coordinate of the sprite's visual origin relative to the cell center.
    origin_y: i16,
    /// The display width of the sprite in world space.
    texture_width: u16,
    /// The display height of the sprite in world space.
    texture_height: u16,
    /// The sampled texture rectangle width.
    render_width: u16,
    /// The sampled texture rectangle height.
    render_height: u16,
    /// The identifier for the graphics texture to use.
    texture_id: i32,
    /// Flags conveying various rendering and behavior properties.
    flags: SpriteFlags,
    /// The perceived visual height used for logic or occlusion.
    #[allow(dead_code)]
    visual_height: u8,
    /// A bitmask controlling the visibility state under specific views.
    #[allow(dead_code)]
    visibility_mask: u8,
    /// A bitmask controlling export rules.
    export_mask: u8,
    /// The shader ID to apply when drawing the sprite.
    #[allow(dead_code)]
    shader: u8,
    /// The number of animation frames. If 0, the sprite is not animated.
    #[allow(dead_code)]
    frame_count: u8,
    /// The animation data, mapped out depending on `frame_count`.
    #[byte(ctx = (ctx, *frame_count))]
    animation: Animation,
    /// The identifier for the sound to play when entities interact with this sprite on the ground.
    #[allow(dead_code)]
    ground_sound: u8,
}

impl MapSpriteDefinition {
    /// Returns the unique identifier of the sprite definition.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// Returns the original coordinates `(x, y)` of the sprite.
    pub fn origin(&self) -> (i16, i16) {
        (self.origin_x, self.origin_y)
    }

    /// Returns the display dimensions `(width, height)` of the sprite in world space.
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

    /// Returns the export mask.
    ///
    /// The reference renderer hides elements whose bit `0` is unset
    /// (`(EXPORT_MASK & exportMask) == 0`); they are editor-only helpers.
    pub fn export_mask(&self) -> u8 {
        self.export_mask
    }

    /// Returns the sampled texture rectangle dimensions `(width, height)`.
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
    /// The sprite is static.
    None,
    /// The sprite uses a series of animated frames.
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
    /// The total duration of all frames combined.
    total_time: u32,
    /// The target width of the frame to render.
    width: u16,
    /// The target height of the frame to render.
    height: u16,
    /// The full actual width of the sprite sheet.
    full_width: u16,
    /// The full actual height of the sprite sheet.
    full_height: u16,
    /// A list of durations for each sequential frame.
    frame_durations: Vec<u16>,
    /// The end time of each frame, i.e. the running sum of `frame_durations`.
    cumulative_ends: Vec<u32>,
    /// A list of coordinates representing the top-left corner of each frame inside the texture sheet.
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

    /// Returns the full width of the underlying sprite sheet.
    pub fn full_width(&self) -> u16 {
        self.full_width
    }

    /// Returns the full height of the underlying sprite sheet.
    pub fn full_height(&self) -> u16 {
        self.full_height
    }

    /// Returns an iterator over the individual frames.
    pub fn iter(&self) -> impl ExactSizeIterator<Item = Frame> {
        let mut time = 0u32;
        self.frame_durations
            .iter()
            .zip(self.frame_coords.iter())
            .map(move |(&duration, &[x, y])| {
                let frame = Frame { time, x, y };
                time += u32::from(duration);
                frame
            })
    }

    /// Returns the coordinates of the frame at `index`.
    pub fn frame_coords(&self, index: usize) -> Option<[u16; 2]> {
        self.frame_coords.get(index).copied()
    }

    /// Returns the index of the frame active at `elapsed` milliseconds.
    ///
    /// Uses the precomputed running end times so the lookup is logarithmic in
    /// the frame count instead of scanning every frame.
    pub fn frame_index_at(&self, elapsed: u32) -> usize {
        let index = self
            .cumulative_ends
            .partition_point(|&end| end <= elapsed);
        index.min(self.frame_durations.len().saturating_sub(1))
    }

    #[cfg(test)]
    pub fn new_for_test(total_time: u32, frame_durations: Vec<u16>) -> Self {
        let frame_coords = vec![[0, 0]; frame_durations.len()];
        let cumulative_ends = cumulative_ends(&frame_durations);
        Self {
            total_time,
            width: 16,
            height: 16,
            full_width: 16,
            full_height: 16,
            frame_durations,
            cumulative_ends,
            frame_coords,
        }
    }
}

/// Returns the running end time of each frame.
fn cumulative_ends(frame_durations: &[u16]) -> Vec<u32> {
    let mut elapsed = 0u32;
    frame_durations
        .iter()
        .map(|&duration| {
            elapsed += u32::from(duration);
            elapsed
        })
        .collect()
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
        let cumulative_ends = cumulative_ends(&frame_durations);

        Ok((
            Frames {
                total_time,
                width,
                height,
                full_width,
                full_height,
                frame_durations,
                cumulative_ends,
                frame_coords,
            },
            *offset,
        ))
    }
}

/// A single frame of an animation.
#[derive(Debug)]
pub struct Frame {
    /// The localized time at which this frame should start.
    pub time: u32,
    /// The X coordinate of the frame in the texture.
    pub x: u16,
    /// The Y coordinate of the frame in the texture.
    pub y: u16,
}

/// A library of map sprite definitions.
#[derive(Debug)]
pub struct MapSpriteLibrary {
    /// A mapping from a definition ID to the sprite's properties.
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

    /// Gets a sprite definition by ID.
    pub fn get(&self, id: i32) -> Option<&MapSpriteDefinition> {
        self.elements.get(&id)
    }

    /// Iterates over all `(id, definition)` pairs in the library.
    pub fn iter(&self) -> impl Iterator<Item = (&i32, &MapSpriteDefinition)> {
        self.elements.iter()
    }

    /// Returns the number of sprite definitions in the library.
    pub fn len(&self) -> usize {
        self.elements.len()
    }

    /// Returns `true` if the library contains no sprite definitions.
    pub fn is_empty(&self) -> bool {
        self.elements.is_empty()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn push_u16(buf: &mut Vec<u8>, value: u16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i16(buf: &mut Vec<u8>, value: i16) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    fn push_i32(buf: &mut Vec<u8>, value: i32) {
        buf.extend_from_slice(&value.to_le_bytes());
    }

    /// Serialises a non-animated definition with fixed, recognisable fields.
    fn static_definition(id: i32, flags: u8) -> Vec<u8> {
        let mut bytes = Vec::new();
        push_i32(&mut bytes, id);
        push_i16(&mut bytes, -5); // origin_x
        push_i16(&mut bytes, 7); // origin_y
        push_u16(&mut bytes, 64); // texture_width
        push_u16(&mut bytes, 48); // texture_height
        push_u16(&mut bytes, 60); // render_width
        push_u16(&mut bytes, 40); // render_height
        push_i32(&mut bytes, 900); // texture_id
        bytes.push(flags);
        bytes.push(12); // visual_height
        bytes.push(0x07); // visibility_mask
        bytes.push(0x01); // export_mask
        bytes.push(0); // shader
        bytes.push(0); // frame_count (static)
        bytes.push(4); // ground_sound
        bytes
    }

    #[test]
    fn sprite_flags_decode_bits() {
        // slope = 3 (low nibble), flip, before_mobile, walkable.
        let flags = SpriteFlags::from_bits(0b1101_0011);
        assert_eq!(flags.slope(), 3);
        assert!(flags.is_flip());
        assert!(!flags.is_move_top());
        assert!(flags.is_before_mobile());
        assert!(flags.is_walkable());

        let none = SpriteFlags::from_bits(0);
        assert_eq!(none.slope(), 0);
        assert!(!none.is_flip());
        assert!(!none.is_before_mobile());
        assert!(!none.is_walkable());
    }

    #[test]
    fn sprite_definition_parses_static_fields() {
        let bytes = static_definition(42, 0b1101_0011);
        let (definition, consumed) =
            MapSpriteDefinition::try_read(&bytes, LittleEndian).unwrap();
        assert_eq!(consumed, bytes.len());

        assert_eq!(definition.id(), 42);
        assert_eq!(definition.origin(), (-5, 7));
        assert_eq!(definition.texture_size(), (64, 48));
        assert_eq!(definition.size(), (60, 40));
        assert_eq!(definition.texture_id(), 900);
        assert_eq!(definition.export_mask(), 0x01);
        assert!(matches!(definition.animation(), Animation::None));

        let flags = definition.flags();
        assert_eq!(flags.slope(), 3);
        assert!(flags.is_flip());
        assert!(!flags.is_move_top());
        assert!(flags.is_before_mobile());
        assert!(flags.is_walkable());
    }

    #[test]
    fn frames_parse_durations_coordinates_and_time_offsets() {
        let mut bytes = Vec::new();
        bytes.extend_from_slice(&300u32.to_le_bytes()); // total_time
        push_u16(&mut bytes, 32); // width
        push_u16(&mut bytes, 24); // height
        push_u16(&mut bytes, 64); // full_width
        push_u16(&mut bytes, 48); // full_height
        push_u16(&mut bytes, 100); // frame 0 duration
        push_u16(&mut bytes, 200); // frame 1 duration
        push_u16(&mut bytes, 0); // frame 0 x
        push_u16(&mut bytes, 0); // frame 0 y
        push_u16(&mut bytes, 32); // frame 1 x
        push_u16(&mut bytes, 0); // frame 1 y

        let (frames, consumed) = Frames::try_read(&bytes, (LittleEndian, 2u8)).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(frames.total_time(), 300);
        assert_eq!((frames.width(), frames.height()), (32, 24));
        assert_eq!((frames.full_width(), frames.full_height()), (64, 48));

        let starts: Vec<u32> = frames.iter().map(|frame| frame.time).collect();
        assert_eq!(starts, vec![0, 100]);
        let coords: Vec<[u16; 2]> = frames
            .iter()
            .map(|frame| [frame.x, frame.y])
            .collect();
        assert_eq!(coords, vec![[0, 0], [32, 0]]);
        assert_eq!(frames.frame_coords(1), Some([32, 0]));
        assert_eq!(frames.frame_coords(2), None);

        // Cumulative end times: frame 0 ends at 100, frame 1 at 300.
        assert_eq!(frames.frame_index_at(0), 0);
        assert_eq!(frames.frame_index_at(99), 0);
        assert_eq!(frames.frame_index_at(100), 1);
        assert_eq!(frames.frame_index_at(299), 1);
    }

    #[test]
    fn sprite_library_reads_empty_and_populated_input() {
        let empty_bytes = 0u32.to_le_bytes();
        let (empty, consumed) =
            MapSpriteLibrary::try_read(&empty_bytes, LittleEndian).unwrap();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        assert_eq!(consumed, empty_bytes.len());

        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&static_definition(42, 0));
        let (library, consumed) = MapSpriteLibrary::try_read(&bytes, LittleEndian).unwrap();
        assert_eq!(consumed, bytes.len());
        assert_eq!(library.len(), 1);
        assert_eq!(library.get(42).unwrap().texture_id(), 900);
        assert!(library.get(7).is_none());
    }

    #[test]
    fn sprite_library_rejects_truncated_definitions() {
        // The count promises one definition but the payload is cut short.
        let mut bytes = 1u32.to_le_bytes().to_vec();
        bytes.extend_from_slice(&static_definition(42, 0)[..10]);
        assert!(MapSpriteLibrary::try_read(&bytes, LittleEndian).is_err());
    }

    #[test]
    fn frame_lookup_honours_boundaries() {
        let frames = Frames::new_for_test(300, vec![100, 100, 100]);
        assert_eq!(frames.frame_index_at(0), 0);
        assert_eq!(frames.frame_index_at(99), 0);
        assert_eq!(frames.frame_index_at(100), 1);
        assert_eq!(frames.frame_index_at(199), 1);
        assert_eq!(frames.frame_index_at(200), 2);
        assert_eq!(frames.frame_index_at(299), 2);
    }

    #[test]
    fn frame_lookup_clamps_out_of_range_times() {
        let frames = Frames::new_for_test(100, vec![50, 50]);
        // Malformed assets can declare a total longer than the durations sum.
        assert_eq!(frames.frame_index_at(500), 1);
        let empty = Frames::new_for_test(0, Vec::new());
        assert_eq!(empty.frame_index_at(0), 0);
    }
}
