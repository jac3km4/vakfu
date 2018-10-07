extern crate vulkano;

use std::sync::Arc;
use vulkano::buffer::cpu_access::CpuAccessibleBuffer;
use vulkano::device::Device;
use vulkano::memory::pool::{PotentialDedicatedAllocation, StdMemoryPoolAlloc};
use wfu::gfx::render_element::RenderElement;
use wfu::gfx::world::world_element::WorldElement;
use wfu::vk::vertex::Vertex;
use wfu::vk::VkTexDescriptorSet;

type VertexBuffer = CpuAccessibleBuffer<[Vertex], PotentialDedicatedAllocation<StdMemoryPoolAlloc>>;

pub struct Sprite<P> {
    pub vertex: Arc<VertexBuffer>,
    pub desc: Arc<VkTexDescriptorSet<P>>,
    cell_x: i32,
    cell_y: i32,
}

impl<P> Sprite<P> {
    pub fn new(
        device: Arc<Device>,
        spec: &RenderElement,
        element: &WorldElement,
        desc: Arc<VkTexDescriptorSet<P>>,
    ) -> Sprite<P> {
        let coords = &element.texture_coords;

        let divisor = 1000.0f32;

        let left = spec.get_x(element) as f32 / divisor;
        let top = spec.get_y(element) as f32 / divisor;
        let bottom = top - element.img_height as f32 / divisor;
        let right = left + element.img_width as f32 / divisor;

        let vertice1 = Vertex {
            position: [left, -top],
            tex_coords: [coords.left, coords.top],
        };
        let vertice2 = Vertex {
            position: [left, -bottom],
            tex_coords: [coords.left, coords.bottom],
        };
        let vertice3 = Vertex {
            position: [right, -top],
            tex_coords: [coords.right, coords.top],
        };
        let vertice4 = Vertex {
            position: [right, -bottom],
            tex_coords: [coords.right, coords.bottom],
        };

        let vertices: [Vertex; 4] = [vertice1, vertice2, vertice3, vertice4];

        let vertex_buffer = CpuAccessibleBuffer::<[Vertex]>::from_iter(
            device.clone(),
            vulkano::buffer::BufferUsage::all(),
            vertices.iter().cloned(),
        ).expect("failed to create buffer");

        Sprite {
            desc,
            vertex: vertex_buffer,
            cell_x: spec.cell_x,
            cell_y: spec.cell_y,
        }
    }
}
