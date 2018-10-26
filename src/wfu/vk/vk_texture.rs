extern crate vulkano;

use std::sync::Arc;
use vulkano::device::Queue;
use vulkano::format::R8G8B8A8Unorm;
use vulkano::image::ImmutableImage;
use vulkano::sync::*;
use wfu::io::tgam::TgamTexture;
use wfu::util::first_greater_power_of_two;

pub trait VkTexture {
    fn load(&self, queue: Arc<Queue>) -> (Arc<ImmutableImage<R8G8B8A8Unorm>>, Box<GpuFuture>);
}

impl VkTexture for TgamTexture {
    fn load(&self, queue: Arc<Queue>) -> (Arc<ImmutableImage<R8G8B8A8Unorm>>, Box<GpuFuture>) {
        let width = first_greater_power_of_two(self.x.into()) as u32;
        let height = first_greater_power_of_two(self.y.into()) as u32;
        let (img, cmd) = ImmutableImage::from_iter(
            self.data.iter().cloned(),
            vulkano::image::Dimensions::Dim2d { width, height },
            vulkano::format::R8G8B8A8Unorm,
            queue.clone(),
        ).unwrap();
        (img, Box::new(cmd) as Box<GpuFuture>)
    }
}
