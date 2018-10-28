extern crate vulkano;

use std::collections::{HashMap, HashSet};
use std::iter::*;
use std::sync::Arc;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Unorm;
use vulkano::image::{Dimensions, ImmutableImage};
use vulkano::sync::GpuFuture;
use wfu::gfx::TextureId;
use wfu::util::indexed::Indexed;
use wfu::vk::vk_texture::VkTexture;

pub type TextureIndex = u32;

pub struct TexturePool {
    indices: HashMap<TextureId, TextureIndex>,
}

const TEXTURE_LIMIT: usize = 2560;

impl TexturePool {
    pub fn load<R, T>(
        texture_loader: &mut R,
        working_set: HashSet<TextureId>,
        queue: Arc<Queue>,
    ) -> (TexturePool, Vec<Arc<ImmutableImage<R8G8B8A8Unorm>>>)
    where
        R: Indexed<TextureId, T>,
        T: VkTexture,
    {
        let default = load_empty_image(queue.clone());

        let tmp = load_working_set(texture_loader, working_set, queue);

        let indices = tmp
            .iter()
            .enumerate()
            .map(|(i, (tex_id, _))| (*tex_id, i as TextureIndex))
            .collect::<HashMap<_, _>>();

        let images = tmp
            .iter()
            .map(|(_, t)| t.clone())
            .chain(repeat(default))
            .take(TEXTURE_LIMIT)
            .collect::<Vec<_>>();

        (TexturePool { indices }, images)
    }

    pub fn get_texture_indice(&self, texture_id: TextureId) -> Option<&TextureIndex> {
        self.indices.get(&texture_id)
    }
}

fn load_empty_image(queue: Arc<Queue>) -> Arc<ImmutableImage<R8G8B8A8Unorm>> {
    let (image, cmd) = {
        let empty: Vec<u8> = vec![0, 0, 0, 0];
        let dimensions = Dimensions::Dim2d {
            width: 1,
            height: 1,
        };
        ImmutableImage::from_iter(
            empty.iter().cloned(),
            dimensions,
            R8G8B8A8Unorm,
            queue.clone(),
        ).unwrap()
    };

    cmd.flush().unwrap();
    return image;
}

fn load_working_set<R, T>(
    texture_loader: &mut R,
    working_set: HashSet<TextureId>,
    queue: Arc<Queue>,
) -> Vec<(TextureId, Arc<ImmutableImage<R8G8B8A8Unorm>>)>
where
    R: Indexed<TextureId, T>,
    T: VkTexture,
{
    working_set
        .iter()
        .filter_map(|e| texture_loader.at(*e).map(|tex| (*e, tex)))
        .map(|(e, texture)| {
            let (image, cmd) = texture.load(queue.clone());

            cmd.flush().unwrap();
            (e, image)
        }).collect::<Vec<_>>()
}
