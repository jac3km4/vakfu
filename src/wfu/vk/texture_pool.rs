extern crate vulkano;

use std::collections::{HashMap, HashSet};
use std::iter::*;
use std::sync::Arc;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Unorm;
use vulkano::image::ImmutableImage;
use vulkano::sync::GpuFuture;
use wfu::util::indexed::Indexed;
use wfu::vk::vk_texture::VkTexture;

pub struct TexturePool {
    pub indices: HashMap<i32, u32>,
}

impl TexturePool {
    pub fn new<R: Indexed<i32, T>, T: VkTexture>(
        loader: &mut R,
        working_set: HashSet<i32>,
        queue: Arc<Queue>,
    ) -> (TexturePool, Vec<Arc<ImmutableImage<R8G8B8A8Unorm>>>) {
        let (default, cmd) = {
            let empty: Vec<u8> = vec![0, 0, 0, 0];
            vulkano::image::immutable::ImmutableImage::from_iter(
                empty.iter().cloned(),
                vulkano::image::Dimensions::Dim2d {
                    width: 1,
                    height: 1,
                },
                vulkano::format::R8G8B8A8Unorm,
                queue.clone(),
            ).unwrap()
        };

        cmd.flush().unwrap();

        let tmp = working_set
            .iter()
            .filter_map(|e| {
                loader
                    .at(*e)
                    .map(|texture| (e, initialize_texture(texture, queue.clone())))
            }).collect::<Vec<_>>();

        let indices = tmp
            .iter()
            .enumerate()
            .map(|(i, (tex_id, _))| (**tex_id, i as u32))
            .collect::<HashMap<i32, u32>>();

        let images = tmp
            .iter()
            .map(|(_, t)| t.clone())
            .chain(repeat(default))
            .take(2560)
            .collect::<Vec<_>>();

        (TexturePool { indices }, images)
    }
}

fn initialize_texture<T>(texture: T, queue: Arc<Queue>) -> Arc<ImmutableImage<R8G8B8A8Unorm>>
where
    T: VkTexture,
{
    let (image, cmd) = texture.load(queue);

    cmd.flush().unwrap();
    return image;
}
