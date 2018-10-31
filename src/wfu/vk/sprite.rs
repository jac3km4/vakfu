extern crate vulkano;

use wfu::gfx::render_spec::RenderSpec;
use wfu::gfx::world::element_definition::ElementDefinition;
use wfu::gfx::world::light::LightMap;
use wfu::vk::texture_pool::TextureIndex;
use wfu::vk::vertex::Vertex;

pub struct Sprite<'a> {
    pub vertex: Vec<Vertex>,
    element: &'a ElementDefinition,
    colors: Vec<f32>,
}

impl<'a> Sprite<'a> {
    pub fn is_visible(&self, mask: u8) -> bool {
        self.element.visibility_mask & mask == self.element.visibility_mask
    }

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

    pub fn lit(&mut self, switch: bool) {
        if switch {
            for v in 0..4 {
                for i in 0..3 {
                    self.vertex[v].colors[i] = self.colors[i + 3];
                }
            }
        } else {
            for v in  0..4 {
                for i in 0..3 {
                    self.vertex[v].colors[i] = self.colors[i];
                }
            }
        }
    }

    pub fn new(
        spec: &RenderSpec,
        element: &'a ElementDefinition,
        lights: &LightMap,
        tex_id: TextureIndex,
    ) -> Sprite<'a> {
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

        //light==================

        let mapx = (spec.cell_x as f32 / 18f32).floor() as i32;
        let mapy = (spec.cell_y as f32 / 18f32).floor() as i32;
        let key = mapx << 16 | (mapy & 0xFFFF);

        let no_map = lights.get_noMap();
        let no_cell = lights.get_noCell(mapx,mapy);
        let no_light = no_cell.get_noLight();

        let cell = lights.lightmaps.get(&key).unwrap_or(&no_cell);
        let hash = (spec.cell_x - cell.cellX) + ((spec.cell_y - cell.cellY) + (spec.layer_idx as i32 * 18)) * 18;
        let def = &cell.layerColors
                    .as_ref()
                    .unwrap_or(&no_map)
                    .get(&hash)
                    .unwrap_or(&no_light);

        let colorOrg = if spec.colors.len() == 3 {
            [spec.colors[0], spec.colors[1], spec.colors[2]]
        } else {
            [0.5f32, 0.5f32, 0.5f32]
        };

        let colors = if spec.colors.len() == 3 {
            [  
                spec.colors[0] * def.ambianceLight[0],
                spec.colors[1] * def.ambianceLight[1], 
                spec.colors[2] * def.ambianceLight[2]
            ]
        } else {
            [ 
                0.5 * def.ambianceLight[0],
                0.5 * def.ambianceLight[1], 
                0.5 * def.ambianceLight[2]
            ]
        };

        let colorlights = vec![
            colorOrg[0],
            colorOrg[1],
            colorOrg[2],
            colors[0],
            colors[1],
            colors[2],
        ];

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

       Sprite { 
            vertex, 
            element, 
            colors: colorlights
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
