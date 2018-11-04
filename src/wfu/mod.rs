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
        pub mod light;
    }
    pub type TextureId = i32;
}

pub mod io {
    pub mod decoder;
    pub mod tgam;
}

pub mod util;

pub mod vk {
    pub mod fragment_shader {
        vulkano_shaders::shader! {
        ty: "fragment",
        src: "
#version 450

layout(location = 0) in vec2 f_tex_coords;
layout(location = 1) in vec3 f_colors;
layout(location = 2) flat in uint f_tex_id;

layout(location = 0) out vec4 f_color;
layout(set = 0, binding = 0) uniform sampler2D tex[2560];

void main() {
    f_color = vec4(2.0, 2.0, 2.0, 1.0) * vec4(f_colors, 1.0) * texture(tex[f_tex_id], f_tex_coords);
}
"
        }
    }
    pub mod map_batch_renderer;
    pub mod persistent;
    pub mod sprite;
    pub mod texture_pool;
    pub mod vertex;
    pub mod vertex_shader {
        vulkano_shaders::shader!{
        ty: "vertex",
        src: "
#version 450

layout(push_constant) uniform Matrix
{
    mat4 value;
} matrix;

layout(location = 0) in vec2 position;
layout(location = 1) in vec2 tex_coords;
layout(location = 2) in vec3 colors;
layout(location = 3) in uint tex_id;

layout(location = 0) out vec2 f_tex_coords;
layout(location = 1) out vec3 f_colors;
layout(location = 2) out uint f_tex_id;

void main() {
    f_tex_coords = tex_coords;
    f_colors = colors;
    f_tex_id = tex_id;
    gl_Position = matrix.value * vec4(position, 0.0, 1.0);
}
"
        }
    }
    pub mod vk_texture;
}
