use std::io::{Read, Seek};

use byte::ctx::{Endianess, LittleEndian};
use byte::{BytesExt, TryRead};

use super::{AssetError, is_chunk_entry_name};
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
        Self::load_archive(&mut archive)
    }

    /// Loads a map from an already-open zip archive.
    ///
    /// Sharing the archive lets callers read optional sidecar entries such as
    /// `groups.lib` without opening and indexing the jar a second time.
    pub fn load_archive<R: Read + Seek>(
        archive: &mut zip::ZipArchive<R>,
    ) -> Result<Map, AssetError> {
        let mut chunks = Vec::with_capacity(archive.len());

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)?;
            let name = file.name()?;
            if is_chunk_entry_name(&name) {
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
    #[allow(dead_code)]
    min_x: i32,
    /// The minimum Y cell coordinate of the chunk's bounds.
    #[allow(dead_code)]
    min_y: i32,
    /// The minimum Z (altitude) coordinate of the chunk's bounds.
    #[allow(dead_code)]
    min_z: i16,
    /// The maximum X cell coordinate of the chunk's bounds.
    #[allow(dead_code)]
    max_x: i32,
    /// The maximum Y cell coordinate of the chunk's bounds.
    #[allow(dead_code)]
    max_y: i32,
    /// The maximum Z (altitude) coordinate of the chunk's bounds.
    #[allow(dead_code)]
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
            let row_size = i32::from(sub.max_y) - i32::from(sub.min_y);
            let cells = if row_size > 0 { &sub.cells[..] } else { &[] };
            cells.iter().zip(0i32..).flat_map(move |(cell, i)| {
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
#[derive(Debug)]
struct MapSubChunk {
    /// The minimum relative X coordinate of the sub-chunk area.
    min_x: u8,
    /// The maximum relative X coordinate of the sub-chunk area.
    #[allow(dead_code)]
    max_x: u8,
    /// The minimum relative Y coordinate of the sub-chunk area.
    min_y: u8,
    /// The maximum relative Y coordinate of the sub-chunk area.
    max_y: u8,

    /// The cells contained within this sub-chunk area.
    cells: Vec<MapCell>,
}

impl<'a, C: Endianess> TryRead<'a, C> for MapSubChunk {
    fn try_read(bytes: &'a [u8], ctx: C) -> byte::Result<(Self, usize)> {
        let offset = &mut 0;
        // Bounds are unsigned bytes (`readByte() & 0xFF` in the reference
        // reader); parsing them as `i8` rejected or wrapped values >= 128.
        let min_x: u8 = bytes.read(offset, ctx)?;
        let max_x: u8 = bytes.read(offset, ctx)?;
        let min_y: u8 = bytes.read(offset, ctx)?;
        let max_y: u8 = bytes.read(offset, ctx)?;

        let width = i32::from(max_x) - i32::from(min_x);
        let height = i32::from(max_y) - i32::from(min_y);
        if width < 0 || height < 0 {
            return Err(byte::Error::BadInput {
                err: "invalid subchunk bounds",
            });
        }
        let count = (width as usize) * (height as usize);
        let cells: Vec<MapCell> = bytes.read(offset, WithSize::new(ctx, count))?;

        Ok((
            Self {
                min_x,
                max_x,
                min_y,
                max_y,
                cells,
            },
            *offset,
        ))
    }
}

/// A single cell within a map sub-chunk, containing rendering elements.
#[derive(Debug, TryRead)]
struct MapCell {
    /// The map sprites/elements positioned within this cell.
    #[byte(ctx = WithSizePrefix::<_, u8>::new(ctx))]
    elements: Vec<MapSprite>,
}

/// A group of map elements.
#[derive(Debug, TryRead)]
pub struct Group {
    /// The unique key associated with the group.
    key: i32,
    /// The layer index on which elements of this group are rendered.
    layer: u8,
    /// The identifier for the group.
    #[allow(dead_code)]
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
struct MapSprite {
    /// The Z (altitude) coordinate of the element.
    cell_z: i16,
    /// The physical height of the element.
    height: u8,
    /// The order or layer priority for depth sorting.
    altitude_order: u8,
    /// A generic tag value.
    #[allow(dead_code)]
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
    ///
    /// RGB channels use the engine's `gColorScale = 2.0`, which the reference
    /// renderer applies to RGB only (`GL_RGB_SCALE`). Alpha is decoded on its
    /// own and clamped to `[0, 1]`; it must *not* receive the RGB scale.
    pub fn to_f32_array(self) -> [f32; 4] {
        [
            rgb_to_f32(self.r),
            rgb_to_f32(self.g),
            rgb_to_f32(self.b),
            alpha_to_f32(self.a),
        ]
    }
}

/// The fixed-pipeline RGB scale (`gColorScale`) the reference engine applies
/// to element tints.
///
/// Folding it into the decoded tint lets Bevy's sprite shader reproduce the
/// reference output without an extra texture-stage state.
pub const TINT_SCALE: f32 = 2.0;

/// Decodes a single tint channel: `byte / 255 + 0.5`, scaled by the engine's
/// `gColorScale` and clamped to non-negative values.
fn rgb_to_f32(value: i8) -> f32 {
    ((value as f32 / 255. + 0.5) * TINT_SCALE).max(0.)
}

/// Decodes an alpha channel: `byte / 255 + 0.5`, clamped to `[0, 1]`.
fn alpha_to_f32(value: i8) -> f32 {
    (value as f32 / 255. + 0.5).clamp(0., 1.)
}

/// A color applied to a map element.
///
/// The tags encode the presence of specific color components as bit flags:
/// - `0x1` (bit 0): Indicates the presence of an RGB tint.
/// - `0x2` (bit 1): Indicates the presence of an Alpha channel.
/// - `0x4` (bit 2): Indicates the presence of a color gradient (requiring two color values).
///
/// The bits are independent, so alpha-only (`0x2`), gradient-only (`0x4`) and
/// alpha-only gradients (`0x6`) are valid entries. The reference's
/// `ScreenElement.readColors` only reads a second color when the tint bit is
/// also set, so `0x4` carries no payload and `0x6` carries only an alpha byte.
#[derive(Debug, Clone, Copy, TryRead)]
#[byte(tag_type = u8)]
pub enum Color {
    /// Represents no specific color.
    #[byte(tag = 0x0)]
    None,
    /// A single RGB tint.
    #[byte(tag = 0x1)]
    Rgb(Rgb),
    /// An alpha-only override with no RGB tint.
    #[byte(tag = 0x2)]
    Alpha(i8),
    /// An RGBA tint, including transparency.
    #[byte(tag = 0x3)]
    Rgba(Rgba),
    /// A gradient flag with no colour components.
    #[byte(tag = 0x4)]
    Gradient,
    /// A gradient between two RGB values.
    #[byte(tag = 0x5)]
    RgbGradient(Rgb, #[allow(dead_code)] Rgb),
    /// An alpha-only override on a gradient element.
    #[byte(tag = 0x6)]
    AlphaGradient(i8),
    /// A gradient between two RGBA values.
    #[byte(tag = 0x7)]
    RgbaGradient(Rgba, #[allow(dead_code)] Rgba),
}

impl Color {
    /// Decodes the element tint the way the reference engine does.
    ///
    /// A missing RGB tint defaults to `0.5` (identity once the `gColorScale`
    /// of `2.0` is applied) and a missing alpha defaults to `1.0`. Gradient
    /// elements are left untinted because the reference engine skips colour
    /// processing for them; per-vertex gradients are not modelled yet.
    pub fn to_f32_array(self) -> [f32; 4] {
        match self {
            Color::Rgb(rgb) => [rgb_to_f32(rgb.r), rgb_to_f32(rgb.g), rgb_to_f32(rgb.b), 1.],
            Color::Alpha(alpha) => [1., 1., 1., alpha_to_f32(alpha)],
            Color::Rgba(rgba) => rgba.to_f32_array(),
            // No tint, alpha-only gradients and gradient elements are all left
            // untinted: the engine defaults RGB to 0.5 (identity after the 2x
            // scale) and alpha to 1.0, and skips colour processing for
            // gradients entirely.
            Color::None
            | Color::Gradient
            | Color::AlphaGradient(_)
            | Color::RgbGradient(..)
            | Color::RgbaGradient(..) => [1., 1., 1., 1.],
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
    element: &'a MapSprite,
    /// A reference to the parent chunk containing this element.
    chunk: &'a MapChunk,
}

impl<'a> MapElementDetails<'a> {
    /// Returns the tag of the element.
    #[allow(dead_code)]
    pub fn tag(&self) -> u8 {
        self.element.tag
    }

    /// Returns the definition ID of the element.
    pub fn definition_id(&self) -> i32 {
        self.element.definition_id
    }

    /// Returns the group this element belongs to.
    pub fn group(&self) -> Option<&'a Group> {
        self.chunk.groups.get(self.element.group_index as usize)
    }

    /// Returns the color of the element.
    pub fn color(&self) -> Color {
        self.chunk
            .colors
            .get(self.element.color_index as usize)
            .copied()
            .unwrap_or(Color::None)
    }

    /// Computes the screen position of the element.
    pub fn screen_position(&self) -> (f32, f32) {
        let height = i32::from(self.element.cell_z) - i32::from(self.element.height);
        iso_to_screen(self.cell_x, self.cell_y, height)
    }

    /// Returns the X cell coordinate of the element.
    pub fn cell_x(&self) -> i32 {
        self.cell_x
    }

    /// Returns the Y cell coordinate of the element.
    pub fn cell_y(&self) -> i32 {
        self.cell_y
    }

    /// Computes the hashcode used primarily for determining rendering depth order (z-sorting).
    ///
    /// It relies on cell coordinates (`x` and `y`) along with the element's
    /// `altitude_order`, adjusted for the `is_before_mobile` sprite flag.
    pub fn hashcode(&self, is_before_mobile: bool) -> i64 {
        element_hashcode(
            self.cell_x,
            self.cell_y,
            sorted_altitude_order(self.element.altitude_order, is_before_mobile),
        )
    }

    /// Returns the Z (altitude) coordinate of the element.
    pub fn cell_z(&self) -> i16 {
        self.element.cell_z
    }

    /// Returns the physical height of the element.
    pub fn height(&self) -> u8 {
        self.element.height
    }

    /// Returns the altitude order of the element.
    pub fn altitude_order(&self) -> u8 {
        self.element.altitude_order
    }
}

/// The number of layers in the reference `LayerOrder` enum.
const LAYER_COUNT: u16 = 13;

/// Computes the depth-sorting hashcode from cell coordinates and `altitude_order`.
pub fn element_hashcode(cell_x: i32, cell_y: i32, altitude_order: u16) -> i64 {
    (i64::from(altitude_order) & 0x1FFFi64) << 6i64
        | ((cell_x as i64 + 8192i64) & 0x3FFFi64) << 19i64
        | ((cell_y as i64 + 8192i64) & 0x3FFFi64) << 34i64
}

/// Returns the altitude order used for depth sorting.
///
/// Elements flagged `is_before_mobile` are pushed above the other ground
/// layers by `LAYER_COUNT - 1`, matching `DisplayedScreenElement`.
pub fn sorted_altitude_order(altitude_order: u8, is_before_mobile: bool) -> u16 {
    if is_before_mobile {
        u16::from(altitude_order) + (LAYER_COUNT - 1)
    } else {
        u16::from(altitude_order)
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

/// Converts world/screen coordinates `(wx, wy)` at a given `height` back to the nearest isometric cell `(x, y)`.
pub fn screen_to_iso(wx: f32, wy: f32, height: i32) -> (i32, i32) {
    let half_w = CELL_WIDTH as f32 / 2.0;
    let half_h = CELL_HEIGHT as f32 / 2.0;
    let dx = wx / half_w;
    let sx = -((wy - (height * ELEVATION_UNIT) as f32) / half_h);
    let x = ((dx + sx) * 0.5).round() as i32;
    let y = ((sx - dx) * 0.5).round() as i32;
    (x, y)
}

/// Returns the 4 world-space corner vertices `[top, right, bottom, left]` of an isometric cell diamond.
pub fn iso_cell_diamond(x: i32, y: i32, height: i32) -> [(f32, f32); 4] {
    let (cx, cy) = iso_to_screen(x, y, height);
    let half_w = CELL_WIDTH as f32 / 2.0;
    let half_h = CELL_HEIGHT as f32 / 2.0;
    [
        (cx, cy + half_h),
        (cx + half_w, cy),
        (cx, cy - half_h),
        (cx - half_w, cy),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_center_is_finite_when_all_elements_are_skipped() {
        use byte::ctx::LittleEndian;

        use crate::assets::{LightMap, MapSpriteLibrary};
        use crate::render::MapRenderer;

        let map = Map {
            chunks: vec![MapChunk {
                min_x: 0,
                min_y: 0,
                min_z: 0,
                max_x: 1,
                max_y: 1,
                max_z: 0,
                groups: vec![],
                colors: vec![],
                map_x: 0,
                map_y: 0,
                sub_chunks: vec![MapSubChunk {
                    min_x: 0,
                    max_x: 1,
                    min_y: 0,
                    max_y: 1,
                    cells: vec![MapCell {
                        elements: vec![MapSprite {
                            cell_z: 0,
                            height: 0,
                            altitude_order: 0,
                            tag: 0,
                            definition_id: 1,
                            group_index: 0,
                            color_index: 0,
                        }],
                    }],
                }],
            }],
        };
        let (sprites, _) = MapSpriteLibrary::try_read(&0u32.to_le_bytes(), LittleEndian).unwrap();
        assert_eq!(map.chunks()[0].elements().count(), 1);
        let renderer = MapRenderer::new(&map, &sprites, LightMap::default());
        assert_eq!(renderer.center(), bevy::prelude::Vec2::ZERO);
    }

    #[test]
    fn test_iso_to_screen_origin() {
        let (x, y) = iso_to_screen(0, 0, 0);
        assert_eq!(x, 0.0);
        assert_eq!(y, 0.0);
    }

    #[test]
    fn test_iso_to_screen_transformation() {
        let (x, y) = iso_to_screen(1, 1, 2);
        assert_eq!(x, 0.0);
        assert_eq!(y, -23.0);

        let (x2, y2) = iso_to_screen(2, 0, 0);
        assert_eq!(x2, 86.0);
        assert_eq!(y2, -43.0);
    }

    #[test]
    fn test_rgba_to_f32_array_decoding() {
        // Alpha is decoded as `byte / 255 + 0.5`, clamped, and never doubled.
        let alpha = |a: i8| {
            Rgba {
                r: 0,
                g: 0,
                b: 0,
                a,
            }
            .to_f32_array()[3]
        };
        assert_eq!(alpha(0), 0.5);
        assert!((alpha(127) - (127.0 / 255.0 + 0.5)).abs() < 1e-6);
        assert_eq!(alpha(-128), 0.0);

        // RGB keeps the 2x colour scale and is clamped to non-negative values.
        let rgb = |v: i8| {
            let arr = Rgba {
                r: v,
                g: v,
                b: v,
                a: 0,
            }
            .to_f32_array();
            [arr[0], arr[1], arr[2]]
        };
        assert_eq!(rgb(0), [1.0, 1.0, 1.0]);
        assert_eq!(rgb(-128), [0.0, 0.0, 0.0]);
    }

    fn test_sprite(definition_id: i32) -> MapSprite {
        MapSprite {
            cell_z: 0,
            height: 0,
            altitude_order: 0,
            tag: 0,
            definition_id,
            group_index: 0,
            color_index: 0,
        }
    }

    fn test_chunk(sub_chunks: Vec<MapSubChunk>) -> MapChunk {
        MapChunk {
            min_x: 0,
            min_y: 0,
            min_z: 0,
            max_x: 10,
            max_y: 10,
            max_z: 0,
            groups: Vec::new(),
            colors: vec![Color::None],
            map_x: 100,
            map_y: 200,
            sub_chunks,
        }
    }

    #[test]
    fn elements_map_subchunk_cells_to_global_coordinates() {
        // A 2x2 sub-chunk offset by (1, 2) inside a chunk at (100, 200).
        // Cells are stored row-major: x advances every `max_y - min_y` cells.
        let sub = MapSubChunk {
            min_x: 1,
            max_x: 3,
            min_y: 2,
            max_y: 4,
            cells: vec![
                MapCell {
                    elements: vec![test_sprite(10)],
                },
                MapCell {
                    elements: vec![test_sprite(11)],
                },
                MapCell {
                    elements: vec![test_sprite(12)],
                },
                MapCell {
                    elements: vec![test_sprite(13)],
                },
            ],
        };
        let chunk = test_chunk(vec![sub]);

        let elements: Vec<(i32, i32, i32)> = chunk
            .elements()
            .map(|element| (element.definition_id(), element.cell_x(), element.cell_y()))
            .collect();
        assert_eq!(
            elements,
            vec![
                (10, 101, 202),
                (11, 101, 203),
                (12, 102, 202),
                (13, 102, 203),
            ]
        );
    }

    #[test]
    fn elements_skip_degenerate_subchunks() {
        // `row_size == 0` would divide by zero; such sub-chunks are ignored.
        let sub = MapSubChunk {
            min_x: 0,
            max_x: 1,
            min_y: 5,
            max_y: 5,
            cells: vec![MapCell {
                elements: vec![test_sprite(1)],
            }],
        };
        let chunk = test_chunk(vec![sub]);
        assert_eq!(chunk.elements().count(), 0);
    }

    #[test]
    fn subchunk_bounds_accept_unsigned_values_above_127() {
        // Bounds are read as unsigned bytes (`readByte() & 0xFF`).
        // min_x=128, max_x=200 (width 72), min_y=0, max_y=1 (height 1).
        let mut bytes = vec![128u8, 200, 0, 1];
        bytes.extend_from_slice(&[0u8; 72]);

        let (sub, consumed) = MapSubChunk::try_read(&bytes, LittleEndian).unwrap();
        assert_eq!(
            (sub.min_x, sub.max_x, sub.min_y, sub.max_y),
            (128, 200, 0, 1)
        );
        assert_eq!(sub.cells.len(), 72);
        assert_eq!(consumed, bytes.len());
    }

    #[test]
    fn subchunk_rejects_inverted_bounds() {
        // max_x < min_x would underflow the cell count.
        let bytes = [5u8, 3, 0, 1];
        assert!(matches!(
            MapSubChunk::try_read(&bytes, LittleEndian),
            Err(byte::Error::BadInput { .. })
        ));

        // max_y < min_y is rejected the same way.
        let bytes = [0u8, 1, 9, 2];
        assert!(matches!(
            MapSubChunk::try_read(&bytes, LittleEndian),
            Err(byte::Error::BadInput { .. })
        ));
    }

    #[test]
    fn test_color_to_f32_array_defaults() {
        // No tint decodes to identity; RGB defaults to 0.5 (x2 = 1.0) and
        // alpha defaults to 1.0.
        assert_eq!(Color::None.to_f32_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(
            Color::Rgb(Rgb { r: 0, g: 0, b: 0 }).to_f32_array(),
            [1.0, 1.0, 1.0, 1.0]
        );
        // Alpha-only entries leave RGB at the default tint and carry alpha.
        assert_eq!(Color::Alpha(0).to_f32_array(), [1.0, 1.0, 1.0, 0.5]);
        // Gradient-only entries skip colour processing like the reference.
        assert_eq!(Color::Gradient.to_f32_array(), [1.0, 1.0, 1.0, 1.0]);
        assert_eq!(Color::AlphaGradient(0).to_f32_array(), [1.0, 1.0, 1.0, 1.0]);
        // Gradients are left untinted, matching the reference engine.
        assert_eq!(
            Color::RgbGradient(
                Rgb {
                    r: 127,
                    g: -128,
                    b: 0
                },
                Rgb::default()
            )
            .to_f32_array(),
            [1.0, 1.0, 1.0, 1.0]
        );
        // RGBA carries an explicit, non-doubled alpha.
        let rgba = Color::Rgba(Rgba {
            r: 0,
            g: 0,
            b: 0,
            a: 0,
        })
        .to_f32_array();
        assert_eq!(rgba, [1.0, 1.0, 1.0, 0.5]);
    }

    #[test]
    fn alpha_only_and_gradient_colour_tags_parse() {
        use byte::ctx::LittleEndian;

        let (alpha, consumed) = Color::try_read(&[0x02, 0x40], LittleEndian).unwrap();
        assert_eq!(consumed, 2);
        assert!(matches!(alpha, Color::Alpha(0x40)));

        let (gradient, consumed) = Color::try_read(&[0x04], LittleEndian).unwrap();
        assert_eq!(consumed, 1);
        assert!(matches!(gradient, Color::Gradient));

        let (alpha_gradient, consumed) = Color::try_read(&[0x06, 0x20], LittleEndian).unwrap();
        assert_eq!(consumed, 2);
        assert!(matches!(alpha_gradient, Color::AlphaGradient(0x20)));
    }
}
