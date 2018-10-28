extern crate itertools;
extern crate vulkano;
extern crate zip;

use self::itertools::*;
use std::collections::HashSet;
use std::io::{Read, Seek};
use std::sync::Arc;
use vulkano::buffer::ImmutableBuffer;
use vulkano::descriptor::descriptor_set::DescriptorSet;
use vulkano::descriptor::descriptor_set::DescriptorSetsCollection;
use vulkano::descriptor::pipeline_layout::PipelineLayoutAbstract;
use vulkano::device::Queue;
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;
use wfu::gfx::map_patch::{parse_patch, MapPatch};
use wfu::gfx::render_spec::RenderSpec;
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::gfx::world::library::ElementLibrary;
use wfu::gfx::TextureId;
use wfu::io::decoder::DecoderCursor;
use wfu::util::indexed::Indexed;
use wfu::vk::persistent::PersistentDescriptorSet;
use wfu::vk::sprite::indexes_at;
use wfu::vk::sprite::Sprite;
use wfu::vk::texture_pool::TexturePool;
use wfu::vk::vertex::Vertex;
use wfu::vk::vk_texture::VkTexture;

pub struct MapBatchRenderer<'a, D: DescriptorSetsCollection> {
    sprites: Vec<Sprite<'a>>,
    descriptors: Arc<D>,
    index_buffer: Vec<u32>,
    vertex_buffer: Vec<Vertex>,
}

impl<'a, D: DescriptorSetsCollection> MapBatchRenderer<'a, D> {
    pub fn update(&mut self, time: u64) {
        let vertices = self
            .sprites
            .iter_mut()
            .flat_map(|sprite| {
                sprite.update(time);
                &sprite.vertex
            }).cloned();

        self.vertex_buffer.clear();
        self.vertex_buffer.extend(vertices);
    }

    pub fn get_index_buffer(
        &self,
        queue: Arc<Queue>,
    ) -> (Arc<ImmutableBuffer<[u32]>>, impl GpuFuture) {
        ImmutableBuffer::from_iter(
            self.index_buffer.iter().cloned(),
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

pub fn initialize_batch_renderer<'a, T, R, S, L>(
    layout: Arc<L>,
    sampler: Arc<Sampler>,
    queue: Arc<Queue>,
    map_archive: S,
    element_library: &'a ElementLibrary,
    texture_loader: &mut R,
) -> MapBatchRenderer<'a, impl DescriptorSet>
where
    T: VkTexture + 'static,
    R: Indexed<i32, T> + 'static,
    S: Read + Seek + 'static,
    L: PipelineLayoutAbstract + Sync + Send + 'static,
{
    let elements = load_all_elements(map_archive, element_library);

    let working_set: HashSet<TextureId> = elements
        .iter()
        .map(|(_, element)| element.texture_id)
        .collect();

    let (pool, images) = TexturePool::load(texture_loader, working_set, queue.clone());

    let sprites = elements
        .iter()
        .filter_map(|(spec, element)| {
            pool.get_texture_indice(element.texture_id)
                .map(|desc| Sprite::new(&spec, element, *desc))
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
    }
}

fn load_all_elements<S: Read + Seek>(
    reader: S,
    library: &ElementLibrary,
) -> Vec<(RenderSpec, &ElementDefinition)> {
    let mut archive = zip::ZipArchive::new(reader).unwrap();

    (0..archive.len())
        .filter_map(|i| {
            let entry = archive.by_index(i).unwrap();
            parse_patch(entry.name()).map(|_| DecoderCursor::new(entry).decode::<MapPatch>())
        }).flat_map(|patch| patch.elements)
        .filter_map(|spec| {
            library
                .get(spec.display.element_id)
                .map(|element| (spec, element))
        }).sorted_by_key(|(spec, _)| spec.hashcode())
}
