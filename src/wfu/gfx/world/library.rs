extern crate zip;

use std::collections::HashMap;
use std::io::{Read, Seek};
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::io::decoder::{Decoder, DecoderCursor};

pub type ElementId = i32;

pub struct ElementLibrary {
    elements: HashMap<ElementId, ElementDefinition>,
}

impl ElementLibrary {
    pub fn load<R: Seek + Read>(reader: R) -> ElementLibrary {
        let mut archive = zip::ZipArchive::new(reader).unwrap();
        let entry = archive.by_name("elements.lib").unwrap();
        let library = DecoderCursor::new(entry).decode();
        return library;
    }

    pub fn get(&self, id: ElementId) -> Option<&ElementDefinition> {
        self.elements.get(&id)
    }
}

impl<R: Read> Decoder<R> for ElementLibrary {
    fn decode(cur: &mut DecoderCursor<R>) -> Self {
        let count: i32 = cur.decode();
        let mut elements: HashMap<ElementId, ElementDefinition> =
            HashMap::with_capacity(count as usize);
        for _ in 0..count {
            let element: ElementDefinition = cur.decode();
            elements.insert(element.id, element);
        }
        ElementLibrary { elements }
    }
}
