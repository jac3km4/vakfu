extern crate vulkano;

use wfu::gfx::render_element::RenderElement;
use wfu::gfx::world::world_element::WorldElement;
use wfu::vk::vertex::Vertex;

pub struct Sprite<'a> {
    pub vertex: Vec<Vertex>,
    element: &'a WorldElement,
}

impl<'a> Sprite<'a> {
    pub fn update(&mut self, time: u64) {
        match &self.element.frames {
            Some(frames) => {
                let coords = frames.get_texture_coords(time);
                self.vertex[0].tex_coords = [coords.left, coords.bottom];
                self.vertex[1].tex_coords = [coords.right, coords.bottom];
                self.vertex[2].tex_coords = [coords.left, coords.top];
                self.vertex[3].tex_coords = [coords.right, coords.top];
            }
            None => (),
        }
    }

    pub fn new(spec: &RenderElement, element: &'a WorldElement, tex_id: u32) -> Sprite<'a> {
        let coords = element
            .frames
            .clone()
            .map_or(element.texture_coords, |frames| {
                frames.get_texture_coords(0u64)
            });

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

        let vertex = vec![vertice1, vertice2, vertice3, vertice4];

        Sprite { vertex, element }
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
