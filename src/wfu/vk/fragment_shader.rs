#[derive(VulkanoShader)]
#[ty = "fragment"]
#[src = "
#version 450
const float BRIGHTNESS_FACTOR = 1.8;
const vec4 BRIGHTNESS_MULT = vec4(BRIGHTNESS_FACTOR, BRIGHTNESS_FACTOR, BRIGHTNESS_FACTOR, 1.0);

layout(location = 0) in vec2 f_tex_coords;
layout(location = 1) in vec3 f_colors;
layout(location = 2) flat in uint f_tex_id;

layout(location = 0) out vec4 f_color;
layout(set = 0, binding = 0) uniform sampler2D tex[2048];

vec3 blendSoftLight(vec3 base, vec3 blend) {
    return mix(
        2.0 * base * blend + base * base * (1.0 - 2.0 * blend),
        sqrt(base) * (2.0 * blend - 1.0) + 2.0 * base * (1.0 - blend),
        step(base, vec3(0.5))
    );
}

void main() {
    vec4 temp = BRIGHTNESS_MULT * vec4(f_colors, 1.0) * texture(tex[f_tex_id], f_tex_coords);
    f_color = vec4(blendSoftLight(temp.xyz, vec3(1.0, 1.0, 1.0)), temp.w);
}
"]
#[allow(dead_code)]
struct Dummy;