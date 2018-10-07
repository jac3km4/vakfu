extern crate zip;

use std::collections::HashMap;
use std::io::{Read, Seek};
use wfu::gfx::world::world_element::WorldElement;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub struct ElementLibrary {
    pub elements: HashMap<i32, WorldElement>,
}

impl ElementLibrary {
    pub fn load<R: Seek + Read>(reader: R) -> ElementLibrary {
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        let entry = archive.by_name("elements.lib").unwrap();
        let library = DecoderCursor::new(entry).decode();
        return library;
    }
}

impl<R: Read> Decoder<R> for ElementLibrary {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let count: i32 = cur.decode();
        let mut elements: HashMap<i32, WorldElement> = HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let element: WorldElement = cur.decode();
            elements.insert(element.id, element);
        }
        ElementLibrary { elements }
    }
}
