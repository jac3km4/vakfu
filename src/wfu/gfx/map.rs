extern crate cgmath;
extern crate itertools;
extern crate zip;

use self::cgmath::Vector2;
use self::itertools::*;
use std::collections::HashSet;
use std::io::{Read, Seek};
use std::sync::Arc;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Srgb;
use vulkano::image::ImmutableImage;
use wfu::gfx::render_element::{RenderElementPatch, parse_patch};
use wfu::gfx::world::library::ElementLibrary;
use wfu::io::decoder::DecoderCursor;
use wfu::util::indexed::Indexed;
use wfu::vk::sprite::Sprite;
use wfu::vk::texture_pool::TexturePool;
use wfu::vk::vk_texture::VkTexture;

pub struct Map {
    sprites: Vec<Sprite>,
}

impl Map {
    pub fn load<T: VkTexture, R: Indexed<i32, T>, S: Read + Seek>(
        queue: Arc<Queue>,
        reader: S,
        library: ElementLibrary,
        loader: &mut R,
    ) -> (Map, Vec<Arc<ImmutableImage<R8G8B8A8Srgb>>>) {
        let mut archive = zip::ZipArchive::new(reader).unwrap();

        let len = archive.len();

        let mut working_set: HashSet<i32> = HashSet::new();

        let mut xys: Vec<Vector2<i32>> = Vec::new();

        for i in 0..len {
            let entry = archive.by_index(i).unwrap();
            match parse_patch(entry.name()) {
                Some((x, y)) => {
                    xys.push(Vector2 { x, y });
                    let patch = DecoderCursor::new(entry).decode::<RenderElementPatch>();
                    working_set.extend(patch.elements.iter().filter_map(|e| {
                        library
                            .elements
                            .get(&e.display.element_id)
                            .map(|x| x.gfx_id)
                    }));
                }
                _ => (),
            }
        }

        let (pool, images) = TexturePool::new(loader, working_set, queue.clone());

        let sprites = xys
            .iter()
            .flat_map(|xy| {
                RenderElementPatch::load(&mut archive, xy.x, xy.y)
                    .map_or(vec![], |patch| patch.elements)
            }).sorted_by_key(|e| e.hashcode())
            .iter()
            .filter_map(|spec| {
                library
                    .elements
                    .get(&spec.display.element_id)
                    .and_then(|element| {
                        pool.indices
                            .get(&element.gfx_id)
                            .map(|desc| Sprite::new(&spec, element, *desc))
                    })
            }).collect::<Vec<_>>();

        let map = Map { sprites };
        (map, images)
    }

    pub fn get_sprites(&mut self) -> &Vec<Sprite> {
        &self.sprites
    }
}
