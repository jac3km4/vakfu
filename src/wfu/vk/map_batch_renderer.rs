extern crate cgmath;
extern crate itertools;
extern crate vulkano;
extern crate zip;

use self::cgmath::{Matrix2, Vector2};
use self::itertools::*;
use std::collections::HashSet;
use std::io::{Read, Seek};
use std::rc::Rc;
use std::sync::Arc;
use vulkano::buffer::ImmutableBuffer;
use vulkano::descriptor::descriptor_set::DescriptorSet;
use vulkano::descriptor::descriptor_set::DescriptorSetsCollection;
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::device::Queue;
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;
use wfu::gfx::lod::LevelOfDetail;
use wfu::gfx::map_patch::{parse_patch, MapPatch};
use wfu::gfx::render_spec::RenderSpec;
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::gfx::world::library::ElementLibrary;
use wfu::gfx::world::light::{LightDef, LightMap};
use wfu::gfx::TextureId;
use wfu::io::decoder::DecoderCursor;
use wfu::util::indexed::Indexed;
use wfu::util::*;
use wfu::vk::persistent::PersistentDescriptorSet;
use wfu::vk::sprite::indexes_at;
use wfu::vk::sprite::Sprite;
use wfu::vk::texture_pool::{TextureIndex, TexturePool};
use wfu::vk::vertex::Vertex;
use wfu::vk::vk_texture::VkTexture;

pub struct MapBatchRenderer<'a, D: DescriptorSetsCollection> {
    sprites: Vec<BoundedSprite<'a>>,
    descriptors: Arc<D>,
    index_buffer: Vec<u32>,
    vertex_buffer: Vec<Vertex>,
    lod: LevelOfDetail,
    enable_light: bool,
}

impl<'a, D: DescriptorSetsCollection> MapBatchRenderer<'a, D> {
    pub fn set_lod(&mut self, lod: LevelOfDetail) {
        self.lod = lod;
    }

    pub fn set_light_enabled(&mut self, enabled: bool) {
        self.enable_light = enabled;
    }

    pub fn is_light_enabled(&self) -> bool {
        self.enable_light
    }

    pub fn update(&mut self, time: u64, bounds: Matrix2<f32>) {
        let lod = self.lod;
        let disable_light = !self.enable_light;

        let vertices = self
            .sprites
            .iter_mut()
            .filter(|s| s.sprite.is_visible(lod.get_mask()) && s.intersects(bounds))
            .flat_map(|bounded| {
                bounded.sprite.update(time, disable_light);
                bounded.sprite.get_vertex()
            }).cloned();

        self.vertex_buffer.clear();
        self.vertex_buffer.extend(vertices);
    }

    pub fn get_index_buffer(
        &self,
        queue: Arc<Queue>,
    ) -> (Arc<ImmutableBuffer<[u32]>>, impl GpuFuture) {
        // there's 6 indexes per 4 vertices
        let index_count = self.vertex_buffer.len() * 6 / 4;
        ImmutableBuffer::from_iter(
            self.index_buffer.iter().take(index_count).cloned(),
            vulkano::buffer::BufferUsage::index_buffer(),
            queue.clone(),
        ).expect("failed to create buffer")
    }

    pub fn get_vertex_buffer(
        &self,
        queue: Arc<Queue>,
    ) -> (Arc<ImmutableBuffer<[Vertex]>>, impl GpuFuture) {
        ImmutableBuffer::from_iter(
            self.vertex_buffer.iter().cloned(),
            vulkano::buffer::BufferUsage::vertex_buffer(),
            queue.clone(),
        ).expect("failed to create buffer")
    }

    pub fn get_descriptors(&self) -> Arc<D> {
        self.descriptors.clone()
    }
}

pub fn new_batch_renderer<'a, T, R, S, L>(
    layout: Arc<L>,
    sampler: Arc<Sampler>,
    queue: Arc<Queue>,
    map_archive: &mut zip::ZipArchive<S>,
    element_library: &'a ElementLibrary,
    texture_loader: &mut R,
    light_map: LightMap,
) -> MapBatchRenderer<'a, impl DescriptorSet>
where
    T: VkTexture + 'static,
    R: Indexed<i32, T> + 'static,
    S: Read + Seek + 'static,
    L: PipelineLayoutAbstract + Sync + Send + 'static,
{
    let elements = load_sprites(map_archive, element_library);

    let working_set: HashSet<TextureId> = elements
        .iter()
        .map(|spec| spec.definition.texture_id)
        .collect();

    let (pool, images) = TexturePool::load(texture_loader, working_set, queue.clone());

    let sprites = elements
        .iter()
        .filter_map(|spec| {
            let color = light_map.get_color(
                spec.render.cell_x,
                spec.render.cell_y,
                spec.render.layer_idx as i32,
            );
            pool.get_texture_indice(spec.definition.texture_id)
                .map(|desc| spec.create_sprite(*desc, color))
        }).collect::<Vec<_>>();

    let descriptors = PersistentDescriptorSet::start(layout, 0)
        .add_sampled_images(images, sampler)
        .unwrap()
        .build()
        .unwrap();

    let index_buffer = (0..sprites.len() as u32)
        .flat_map(|i| indexes_at(i))
        .collect::<Vec<_>>();

    let vertex_buffer: Vec<Vertex> = Vec::new();

    MapBatchRenderer {
        sprites,
        descriptors: Arc::new(descriptors),
        index_buffer,
        vertex_buffer,
        lod: LevelOfDetail::High,
        enable_light: true,
    }
}

pub struct BoundedSprite<'a> {
    bounds: Matrix2<f32>,
    sprite: Sprite<'a>,
}

impl<'a> BoundedSprite<'a> {
    pub fn intersects(&self, other: Matrix2<f32>) -> bool {
        !(self.bounds.x[0] > other.x[1]
            || self.bounds.x[1] < other.x[0]
            || self.bounds.y[0] > other.y[1]
            || self.bounds.y[1] < other.y[0])
    }
}

struct SpriteSpec<'a> {
    render: RenderSpec,
    definition: &'a ElementDefinition,
    patch: Rc<MapPatch>,
}

impl<'a> SpriteSpec<'a> {
    pub fn create_sprite(&self, tex_idx: TextureIndex, light: Rc<LightDef>) -> BoundedSprite<'a> {
        let sprite = Sprite::new(&self.render, self.definition, tex_idx, light);
        let bounds = Matrix2 {
            x: Vector2 {
                x: iso_to_screen_x(self.patch.min_x, self.patch.min_y),
                y: iso_to_screen_x(self.patch.max_x, self.patch.max_y),
            },
            y: Vector2 {
                x: iso_to_screen_y(self.patch.min_x, self.patch.min_y, self.patch.min_z as i32),
                y: iso_to_screen_y(self.patch.max_x, self.patch.max_y, self.patch.max_z as i32),
            },
        };
        BoundedSprite { sprite, bounds }
    }
}

fn load_sprites<'a, S: Read + Seek>(
    archive: &mut zip::ZipArchive<S>,
    library: &'a ElementLibrary,
) -> Vec<SpriteSpec<'a>> {
    (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).unwrap();
            parse_patch(entry.name())
                .map(|_| Rc::new(DecoderCursor::new(entry).decode::<MapPatch>()))
        }).flat_map(|patch| {
            patch
                .elements
                .iter()
                .filter_map(|spec| {
                    library
                        .get(spec.display.element_id)
                        .map(|element| SpriteSpec {
                            render: spec.clone(),
                            definition: element,
                            patch: patch.clone(),
                        })
                }).collect::<Vec<_>>()
        }).sorted_by_key(|spec| spec.render.hashcode())
}
