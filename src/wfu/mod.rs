pub mod gfx {
    pub mod camera;
    pub mod color_table;
    pub mod lod;
    pub mod map;
    pub mod render_element;
    pub mod texture_coords;
    pub mod world {
        pub mod library;
        pub mod world_element;
    }
}

pub mod io {
    pub mod decoder;
    pub mod tgam;
}

pub mod util;

pub mod vk {
    pub mod fragment_shader;
    pub mod persistent;
    pub mod sprite;
    pub mod texture_pool;
    pub mod vertex;
    pub mod vertex_shader;
    pub mod vk_texture;
}
