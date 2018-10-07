extern crate vulkano;

use std::sync::Arc;
use vulkano::descriptor::descriptor_set::{
    PersistentDescriptorSet, PersistentDescriptorSetImg, PersistentDescriptorSetSampler,
};
use vulkano::format::R8G8B8A8Srgb;
use vulkano::image::ImmutableImage;

pub mod fragment_shader;
pub mod sprite;
pub mod texture_pool;
pub mod vertex;
pub mod vertex_shader;
pub mod vk_texture;

pub type VkTexDescriptorSet<P> = PersistentDescriptorSet<
    Arc<P>,
    (
        (
            (),
            PersistentDescriptorSetImg<Arc<ImmutableImage<R8G8B8A8Srgb>>>,
        ),
        PersistentDescriptorSetSampler,
    ),
>;
