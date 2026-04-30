use std::io::{Read, Seek};

use byte::TryRead;
use byte::ctx::LittleEndian;

use super::AssetError;
use crate::util::{WithSize, WithSizePrefix};

const CELL_WIDTH: i32 = 86;
const CELL_HEIGHT: i32 = 43;
const ELEVATION_UNIT: i32 = 10;

/// A map, consisting of multiple chunks.
#[derive(Debug)]
pub struct Map {
    /// The chunks that make up the map.
    chunks: Vec<MapChunk>,
}

impl Map {
    /// Loads a map from a zip archive containing chunk files.
    pub fn load<R: Read + Seek>(input: R) -> Result<Map, AssetError> {
        let mut archive = zip::ZipArchive::new(input)?;
        let mut chunks = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            if file
                .name()
                .trim_matches(|c| char::is_numeric(c) || c == '-')
                == "_"
            {
                let mut buffer = Vec::with_capacity(file.size() as usize);
                file.read_to_end(&mut buffer)?;

                let (chunk, _) = MapChunk::try_read(&buffer, LittleEndian)?;
                chunks.push(chunk);
            }
        }
        Ok(Map { chunks })
    }

    /// Returns the chunks of the map.
    pub fn chunks(&self) -> &[MapChunk] {
        &self.chunks
    }
}

/// A single chunk of a map.
#[derive(Debug, TryRead)]
pub struct MapChunk {
    /// The minimum X cell coordinate of the chunk's bounds.
    min_x: i32,
    /// The minimum Y cell coordinate of the chunk's bounds.
    min_y: i32,
    /// The minimum Z (altitude) coordinate of the chunk's bounds.
    min_z: i16,
    /// The maximum X cell coordinate of the chunk's bounds.
    max_x: i32,
    /// The maximum Y cell coordinate of the chunk's bounds.
    max_y: i32,
    /// The maximum Z (altitude) coordinate of the chunk's bounds.
    max_z: i16,
    /// The palette of groups used by the elements in this chunk.
    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    groups: Vec<Group>,
    /// The palette of colors used by the elements in this chunk.
    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    colors: Vec<Color>,

    /// The base X coordinate of this chunk within the global map grid.
    map_x: i32,
    /// The base Y coordinate of this chunk within the global map grid.
    map_y: i32,

    /// The rectangular sub-chunks (often sections of cells) that make up this chunk.
    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    sub_chunks: Vec<MapSubChunk>,
}

impl MapChunk {
    /// Returns an iterator over the elements in the chunk.
    pub fn elements(&self) -> impl Iterator<Item = MapElementDetails<'_>> {
        self.sub_chunks.iter().flat_map(move |sub| {
            let row_size = i32::from(sub.max_y - sub.min_y);
            sub.cells.iter().zip(0i32..).flat_map(move |(cell, i)| {
                let cell_x = self.map_x + i32::from(sub.min_x) + i / row_size;
                let cell_y = self.map_y + i32::from(sub.min_y) + i % row_size;
                cell.elements.iter().map(move |element| MapElementDetails {
                    cell_x,
                    cell_y,
                    element,
                    chunk: self,
                })
            })
        })
    }
}

/// Represents a sub-section of a chunk, defining a rectangular area of cells.
#[derive(Debug, TryRead)]
struct MapSubChunk {
    /// The minimum relative X coordinate of the sub-chunk area.
    min_x: i8,
    /// The maximum relative X coordinate of the sub-chunk area.
    max_x: i8,
    /// The minimum relative Y coordinate of the sub-chunk area.
    min_y: i8,
    /// The maximum relative Y coordinate of the sub-chunk area.
    max_y: i8,

    /// The cells contained within this sub-chunk area.
    #[byte(ctx = WithSize::new(ctx, (max_x - min_x) as usize * (max_y - min_y) as usize))]
    cells: Vec<MapCell>,
}

/// A single cell within a map sub-chunk, containing rendering elements.
#[derive(Debug, TryRead)]
struct MapCell {
    /// The map sprites/elements positioned within this cell.
    #[byte(ctx = WithSizePrefix::<_, u8>::new(ctx))]
    elements: Vec<MapSrite>,
}

/// A group of map elements.
#[derive(Debug, TryRead)]
pub struct Group {
    /// The unique key associated with the group.
    key: i32,
    /// The layer index on which elements of this group are rendered.
    layer: u8,
    /// The identifier for the group.
    id: i32,
}

impl Group {
    /// Returns the group's key.
    pub fn key(&self) -> i32 {
        self.key
    }

    /// Returns the layer index of the group.
    pub fn layer(&self) -> u8 {
        self.layer
    }
}

/// Represents the raw properties of a sprite placed on a map cell.
#[derive(Debug, TryRead)]
struct MapSrite {
    /// The Z (altitude) coordinate of the element.
    cell_z: i16,
    /// The physical height of the element.
    height: u8,
    /// The order or layer priority for depth sorting.
    altitude_order: u8,
    /// A generic tag value.
    tag: u8,
    /// The ID referring to the specific visual definition (e.g., texture mapping).
    definition_id: i32,
    /// The index resolving to the group this sprite belongs to.
    group_index: u16,
    /// The index resolving to the tint/color applied to this sprite.
    color_index: u16,
}

/// An RGB color definition.
#[derive(Debug, Default, Clone, Copy, TryRead)]
pub struct Rgb {
    /// The red component.
    r: i8,
    /// The green component.
    g: i8,
    /// The blue component.
    b: i8,
}

/// An RGBA color definition.
#[derive(Debug, Default, Clone, Copy, TryRead)]
pub struct Rgba {
    /// The red component.
    r: i8,
    /// The green component.
    g: i8,
    /// The blue component.
    b: i8,
    /// The alpha (transparency) component.
    a: i8,
}

impl Rgba {
    /// Converts the color to an array of 4 `f32` values.
    pub fn to_f32_array(self) -> [f32; 4] {
        [
            (self.r as f32 / 255. + 0.5) * 2.,
            (self.g as f32 / 255. + 0.5) * 2.,
            (self.b as f32 / 255. + 0.5) * 2.,
            (self.a as f32 / 255. + 0.5) * 2.,
        ]
    }
}

impl From<Rgb> for Rgba {
    fn from(rgb: Rgb) -> Self {
        Rgba {
            r: rgb.r,
            g: rgb.g,
            b: rgb.b,
            a: 0,
        }
    }
}

/// A color applied to a map element.
///
/// The tags encode the presence of specific color components as bit flags:
/// - `0x1` (bit 0): Indicates the presence of an RGB tint.
/// - `0x2` (bit 1): Indicates the presence of an Alpha channel.
/// - `0x4` (bit 2): Indicates the presence of a color gradient (requiring two color values).
#[derive(Debug, Clone, Copy, TryRead)]
#[byte(tag_type = u8)]
pub enum Color {
    /// Represents no specific color.
    #[byte(tag = 0x0)]
    None,
    /// A single RGB tint.
    #[byte(tag = 0x1)]
    Rgb(Rgb),
    /// An RGBA tint, including transparency.
    #[byte(tag = 0x3)]
    Rgba(Rgba),
    /// A gradient between two RGB values.
    #[byte(tag = 0x5)]
    RgbGradient(Rgb, Rgb),
    /// A gradient between two RGBA values.
    #[byte(tag = 0x7)]
    RgbaGradient(Rgba, Rgba),
}

impl From<Color> for Rgba {
    fn from(color: Color) -> Self {
        match color {
            Color::None => Rgba::default(),
            Color::Rgb(rgb) => Rgba::from(rgb),
            Color::Rgba(rgba) => rgba,
            Color::RgbGradient(start, _) => Rgba::from(start),
            Color::RgbaGradient(start, _) => start,
        }
    }
}

/// Details about a specific map element.
#[derive(Debug)]
pub struct MapElementDetails<'a> {
    /// The X cell coordinate of the element.
    cell_x: i32,
    /// The Y cell coordinate of the element.
    cell_y: i32,
    /// The underlying raw map sprite element properties.
    element: &'a MapSrite,
    /// A reference to the parent chunk containing this element.
    chunk: &'a MapChunk,
}

impl<'a> MapElementDetails<'a> {
    /// Returns the tag of the element.
    pub fn tag(&self) -> u8 {
        self.element.tag
    }

    /// Returns the definition ID of the element.
    pub fn definition_id(&self) -> i32 {
        self.element.definition_id
    }

    /// Returns the group this element belongs to.
    pub fn group(&self) -> &'a Group {
        &self.chunk.groups[self.element.group_index as usize]
    }

    /// Returns the color of the element.
    pub fn color(&self) -> Color {
        self.chunk.colors[self.element.color_index as usize]
    }

    /// Computes the screen position of the element.
    pub fn screen_position(&self) -> (f32, f32) {
        let height = i32::from(self.element.cell_z) - i32::from(self.element.height);
        iso_to_screen(self.cell_x, self.cell_y, height)
    }

    /// Computes the hashcode used primarily for determining rendering depth order (z-sorting).
    /// It relies on cell coordinates (`x` and `y`) along with the element's `altitude_order`.
    pub fn hashcode(&self) -> i64 {
        (self.element.altitude_order as i64 & 0x1FFFi64) << 6i64
            | ((self.cell_x as i64 + 8192i64) & 0x3FFFi64) << 19i64
            | ((self.cell_y as i64 + 8192i64) & 0x3FFFi64) << 34i64
    }
}

/// Converts isometric coordinates `(x, y)` and `height` to screen coordinates.
/// 
/// This transformation uses specific scaling factors:
/// - A cell width of 86 units and cell height of 43 units.
/// - An elevation unit multiplier of 10 for the `height` coordinate.
pub fn iso_to_screen(x: i32, y: i32, height: i32) -> (f32, f32) {
    let fx = ((x - y) * CELL_WIDTH) as f32 / 2.;
    let fy = ((-(x + y) * CELL_HEIGHT) as f32 / 2.) + (height * ELEVATION_UNIT) as f32;
    (fx, fy)
}
