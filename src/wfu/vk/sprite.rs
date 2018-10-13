extern crate vulkano;

use wfu::gfx::render_element::RenderElement;
use wfu::gfx::world::world_element::WorldElement;
use wfu::vk::vertex::Vertex;

pub struct Sprite {
    pub vertex: Vec<Vertex>,
}

impl Sprite {
    pub fn new(spec: &RenderElement, element: &WorldElement, tex_id: u32) -> Sprite {
        let coords = &element.texture_coords;

        let left = spec.get_x(element) as f32;
        let top = spec.get_y(element) as f32;
        let bottom = top - element.img_height as f32;
        let right = left + element.img_width as f32;

        let colors = if spec.colors.len() == 3 {
            [spec.colors[0], spec.colors[1], spec.colors[2]]
        } else {
            [0.5f32, 0.5f32, 0.5f32]
        };

        let vertice1 = Vertex {
            position: [left, -bottom],
            tex_coords: [coords.left, coords.bottom],
            colors,
            tex_id,
        };
        let vertice2 = Vertex {
            position: [right, -bottom],
            tex_coords: [coords.right, coords.bottom],
            colors,
            tex_id,
        };
        let vertice3 = Vertex {
            position: [left, -top],
            tex_coords: [coords.left, coords.top],
            colors,
            tex_id,
        };
        let vertice4 = Vertex {
            position: [right, -top],
            tex_coords: [coords.right, coords.top],
            colors,
            tex_id,
        };

        let vertices = vec![vertice1, vertice2, vertice3, vertice4];

        Sprite { vertex: vertices }
    }
}

pub fn indexes_at(offset: u32) -> Vec<u32> {
    vec![
        offset * 4 + 0u32,
        offset * 4 + 1u32,
        offset * 4 + 2u32,
        offset * 4 + 2u32,
        offset * 4 + 1u32,
        offset * 4 + 3u32,
    ]
}
