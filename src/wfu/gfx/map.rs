extern crate cgmath;
extern crate zip;

use self::cgmath::Vector2;
use std::io::{Read, Seek};
use vulkano::pipeline::GraphicsPipelineAbstract;
use wfu::gfx::render_element::RenderElementPatch;
use wfu::gfx::world::library::ElementLibrary;
use wfu::util::indexed::Indexed;
use wfu::util::uluru::{Entry, LRUCache};
use wfu::vk::sprite::Sprite;
use wfu::vk::texture_pool::TexturePool;
use wfu::vk::vk_texture::VkTexture;

// FIXME: this should be decoupled from vulkan

struct MapPatch<P> {
    xy: Vector2<i32>,
    sprites: Vec<Sprite<P>>,
}

pub struct Map<P, T, R, S>
where
    T: VkTexture,
    P: GraphicsPipelineAbstract,
    R: Indexed<i32, T>,
    S: Read + Seek,
{
    archive: zip::ZipArchive<S>,
    cache: LRUCache<[Entry<MapPatch<P>>; 12]>,
    pool: TexturePool<P, T, R>,
    library: ElementLibrary,
}

impl<P, T, R, S> Map<P, T, R, S>
where
    T: VkTexture,
    P: GraphicsPipelineAbstract,
    R: Indexed<i32, T>,
    S: Read + Seek,
{
    pub fn load(reader: S, pool: TexturePool<P, T, R>, library: ElementLibrary) -> Map<P, T, R, S> {
        let archive = zip::ZipArchive::new(reader).unwrap();
        Map {
            archive,
            cache: LRUCache::default(),
            pool,
            library,
        }
    }

    pub fn set_abs_center(&mut self, point: Vector2<f32>) {
        self.load_patches(point)
    }

    pub fn get_patches(&mut self) -> impl Iterator<Item = &Vec<Sprite<P>>> {
        self.cache.entries.iter().map(|entry| &entry.val.sprites)
    }

    fn load_patch(&mut self, x: i32, y: i32) {
        let archive = &mut self.archive;
        let library = &self.library;
        let pool = &mut self.pool;
        let xy = Vector2 { x, y };
        let cache = &mut self.cache;

        match cache.find(|v| v.xy == xy) {
            Some(_) => (),
            None => {
                RenderElementPatch::load(archive, x, y).map(|patch| {
                    let sprites = patch
                        .elements
                        .iter()
                        .filter_map(|spec| {
                            library
                                .elements
                                .get(&spec.display.element_id)
                                .and_then(|element| {
                                    pool.get_texture(element.gfx_id).map(|desc| {
                                        Sprite::new(
                                            pool.queue.device().clone(),
                                            spec,
                                            element,
                                            desc,
                                        )
                                    })
                                })
                        }).collect();
                    cache.insert(MapPatch { sprites, xy });
                });
            }
        }
    }

    fn load_patches(&mut self, point: Vector2<f32>) {
        // FIXME: this function currently always loads 9 patches at a time due to performance issues
        let i = point.x as i32;
        let j = point.y as i32;
        for i in i - 1..i + 2 {
            for j in j - 1..j + 2 {
                self.load_patch(i, j);
            }
        }
    }
}
