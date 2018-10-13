#[derive(VulkanoShader)]
#[ty = "fragment"]
#[src = "
#version 450

const float BRIGHTNESS_FACTOR = 2.5;

layout(location = 0) in vec2 f_tex_coords;
layout(location = 1) in vec3 f_colors;
layout(location = 2) flat in uint f_tex_id;

layout(location = 0) out vec4 f_color;
layout(set = 0, binding = 0) uniform sampler2D tex[2048];

void main() {
    f_color = vec4(BRIGHTNESS_FACTOR, BRIGHTNESS_FACTOR, BRIGHTNESS_FACTOR, 1.0) * vec4(f_colors, 1.0) * texture(tex[f_tex_id], f_tex_coords);
}
"]
#[allow(dead_code)]
struct Dummy;
