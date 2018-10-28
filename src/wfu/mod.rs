pub mod gfx {
    pub mod camera;
    pub mod color_table;
    pub mod lod;
    pub mod map_patch;
    pub mod render_spec;
    pub mod texture_coords;
    pub mod world {
        pub mod element_definition;
        pub mod library;
    }
    pub type TextureId = i32;
}

pub mod io {
    pub mod decoder;
    pub mod tgam;
}

pub mod util;

pub mod vk {
    pub mod fragment_shader;
    pub mod map_batch_renderer;
    pub mod persistent;
    pub mod sprite;
    pub mod texture_pool;
    pub mod vertex;
    pub mod vertex_shader;
    pub mod vk_texture;
}
