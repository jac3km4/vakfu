extern crate cgmath;
extern crate vulkano;

use std::rc::Rc;
use wfu::gfx::render_spec::RenderSpec;
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::gfx::world::light::LightDef;
use wfu::vk::texture_pool::TextureIndex;
use wfu::vk::vertex::Vertex;

pub struct Sprite<'a> {
    vertex: [Vertex; 4],
    element: &'a ElementDefinition,
    colors: [f32; 3],
    light: Rc<LightDef>,
    layer: u8,
}

impl<'a> Sprite<'a> {
    pub fn is_visible(&self, mask: u8) -> bool {
        self.element.visibility_mask & mask == self.element.visibility_mask
    }

    pub fn is_in_layer(&self, layer: u8) -> bool {
        (1 << self.layer) & layer == (1 << self.layer)
    }

    pub fn get_vertex(&self) -> &[Vertex; 4] {
        &self.vertex
    }

    pub fn update(&mut self, time: i64, disable_light: bool) {
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
        let colors = self.get_colors(disable_light);
        for v in self.vertex.iter_mut() {
            v.colors = colors;
        }
    }

    fn get_colors(&mut self, disable_light: bool) -> [f32; 3] {
        if !disable_light {
            [
                self.colors[0] * self.light.ambiance_light[0],
                self.colors[1] * self.light.ambiance_light[1],
                self.colors[2] * self.light.ambiance_light[2],
            ]
        } else {
            self.colors.to_owned()
        }
    }

    pub fn new(
        spec: &RenderSpec,
        element: &'a ElementDefinition,
        tex_id: TextureIndex,
        light: Rc<LightDef>,
    ) -> Sprite<'a> {
        let coords = element
            .frames
            .clone()
            .map_or(element.texture_coords, |frames| {
                frames.get_texture_coords(0i64)
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

        let vertex = [vertice1, vertice2, vertice3, vertice4];

        Sprite {
            vertex,
            element,
            colors,
            light,
            layer: spec.layer_idx,
        }
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
