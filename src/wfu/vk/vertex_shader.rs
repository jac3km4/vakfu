#[derive(VulkanoShader)]
#[ty = "vertex"]
#[src = "
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
"]
#[allow(dead_code)]
struct Dummy;
