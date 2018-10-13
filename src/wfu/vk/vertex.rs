#[derive(Debug, Clone)]
pub struct Vertex {
    pub position: [f32; 2],
    pub tex_coords: [f32; 2],
    pub colors: [f32; 3],
    pub tex_id: u32,
}

impl_vertex!(Vertex, position, tex_coords, colors, tex_id);
