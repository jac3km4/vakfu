extern crate vulkano;

use std::collections::hash_map::VacantEntry;
use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;
use vulkano::descriptor::descriptor_set::PersistentDescriptorSet;
use vulkano::device::Queue;
use vulkano::pipeline::GraphicsPipelineAbstract;
use vulkano::sampler::Sampler;
use vulkano::sync::GpuFuture;
use wfu::util::indexed::Indexed;
use wfu::vk::vk_texture::VkTexture;
use wfu::vk::VkTexDescriptorSet;

pub struct TexturePool<P, T, R>
where
    T: VkTexture,
    P: GraphicsPipelineAbstract,
    R: Indexed<i32, T>,
{
    textures: R,
    cache: HashMap<i32, Arc<VkTexDescriptorSet<P>>>,
    pipeline: Arc<P>,
    sampler: Arc<Sampler>,
    pub queue: Arc<Queue>,
    phantom_t: PhantomData<T>,
}

impl<P, T, R> TexturePool<P, T, R>
where
    P: GraphicsPipelineAbstract,
    T: VkTexture,
    R: Indexed<i32, T>,
{
    pub fn new(
        loader: R,
        pipeline: Arc<P>,
        sampler: Arc<Sampler>,
        queue: Arc<Queue>,
    ) -> TexturePool<P, T, R> {
        TexturePool {
            textures: loader,
            cache: HashMap::new(),
            pipeline,
            sampler,
            queue,
            phantom_t: PhantomData,
        }
    }

    pub fn get_texture(&mut self, id: i32) -> Option<Arc<VkTexDescriptorSet<P>>> {
        use std::collections::hash_map::Entry;

        match self.cache.entry(id) {
            Entry::Occupied(texture) => Some(texture.get().clone()),
            Entry::Vacant(slot) => {
                let pipeline = self.pipeline.clone();
                let sampler = self.sampler.clone();
                let queue = self.queue.clone();
                self.textures
                    .at(id)
                    .map(|texture| initialize_texture(texture, pipeline, sampler, queue, slot))
            }
        }
    }
}

fn initialize_texture<P, T>(
    texture: T,
    pipeline: Arc<P>,
    sampler: Arc<Sampler>,
    queue: Arc<Queue>,
    slot: VacantEntry<i32, Arc<VkTexDescriptorSet<P>>>,
) -> Arc<VkTexDescriptorSet<P>>
where
    P: GraphicsPipelineAbstract,
    T: VkTexture,
{
    let (image, cmd) = texture.load(queue);

    cmd.flush().unwrap();
    let set = Arc::new(
        PersistentDescriptorSet::start(pipeline, 0)
            .add_sampled_image(image.clone(), sampler.clone())
            .unwrap()
            .build()
            .unwrap(),
    );

    slot.insert(set.clone());
    return set;
}
