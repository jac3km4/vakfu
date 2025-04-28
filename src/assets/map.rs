use std::io::{Read, Seek};

use byte::TryRead;
use byte::ctx::LittleEndian;

use super::AssetError;
use crate::util::{WithSize, WithSizePrefix};

const CELL_WIDTH: i32 = 86;
const CELL_HEIGHT: i32 = 43;
const ELEVATION_UNIT: i32 = 10;

#[derive(Debug)]
pub struct Map {
    chunks: Vec<MapChunk>,
}

impl Map {
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

    pub fn chunks(&self) -> &[MapChunk] {
        &self.chunks
    }
}

#[derive(Debug, TryRead)]
pub struct MapChunk {
    min_x: i32,
    min_y: i32,
    min_z: i16,
    max_x: i32,
    max_y: i32,
    max_z: i16,
    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    groups: Vec<Group>,
    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    colors: Vec<Color>,

    map_x: i32,
    map_y: i32,

    #[byte(ctx = WithSizePrefix::<_, u16>::new(ctx))]
    sub_chunks: Vec<MapSubChunk>,
}

impl MapChunk {
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

#[derive(Debug, TryRead)]
struct MapSubChunk {
    min_x: i8,
    max_x: i8,
    min_y: i8,
    max_y: i8,

    #[byte(ctx = WithSize::new(ctx, (max_x - min_x) as usize * (max_y - min_y) as usize))]
    cells: Vec<MapCell>,
}

#[derive(Debug, TryRead)]
struct MapCell {
    #[byte(ctx = WithSizePrefix::<_, u8>::new(ctx))]
    elements: Vec<MapSrite>,
}

#[derive(Debug, TryRead)]
pub struct Group {
    key: i32,
    layer: u8,
    id: i32,
}

impl Group {
    pub fn key(&self) -> i32 {
        self.key
    }

    pub fn layer(&self) -> u8 {
        self.layer
    }
}

#[derive(Debug, TryRead)]
struct MapSrite {
    cell_z: i16,
    height: u8,
    altitude_order: u8,
    tag: u8,
    definition_id: i32,
    group_index: u16,
    color_index: u16,
}

#[derive(Debug, Default, Clone, Copy, TryRead)]
pub struct Rgb {
    r: i8,
    g: i8,
    b: i8,
}

#[derive(Debug, Default, Clone, Copy, TryRead)]
pub struct Rgba {
    r: i8,
    g: i8,
    b: i8,
    a: i8,
}

impl Rgba {
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

#[derive(Debug, Clone, Copy, TryRead)]
#[byte(tag_type = u8)]
pub enum Color {
    #[byte(tag = 0x0)]
    None,
    #[byte(tag = 0x1)]
    Rgb(Rgb),
    #[byte(tag = 0x3)]
    Rgba(Rgba),
    #[byte(tag = 0x5)]
    RgbGradient(Rgb, Rgb),
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

#[derive(Debug)]
pub struct MapElementDetails<'a> {
    cell_x: i32,
    cell_y: i32,
    element: &'a MapSrite,
    chunk: &'a MapChunk,
}

impl<'a> MapElementDetails<'a> {
    pub fn tag(&self) -> u8 {
        self.element.tag
    }

    pub fn definition_id(&self) -> i32 {
        self.element.definition_id
    }

    pub fn group(&self) -> &'a Group {
        &self.chunk.groups[self.element.group_index as usize]
    }

    pub fn color(&self) -> Color {
        self.chunk.colors[self.element.color_index as usize]
    }

    pub fn screen_position(&self) -> (f32, f32) {
        let height = i32::from(self.element.cell_z) - i32::from(self.element.height);
        iso_to_screen(self.cell_x, self.cell_y, height)
    }

    pub fn hashcode(&self) -> i64 {
        (self.element.altitude_order as i64 & 0x1FFFi64) << 6i64
            | ((self.cell_x as i64 + 8192i64) & 0x3FFFi64) << 19i64
            | ((self.cell_y as i64 + 8192i64) & 0x3FFFi64) << 34i64
    }
}

pub fn iso_to_screen(x: i32, y: i32, height: i32) -> (f32, f32) {
    let fx = ((x - y) * CELL_WIDTH) as f32 / 2.;
    let fy = ((-(x + y) * CELL_HEIGHT) as f32 / 2.) + (height * ELEVATION_UNIT) as f32;
    (fx, fy)
}
