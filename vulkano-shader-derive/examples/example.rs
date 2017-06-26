#[macro_use] extern crate vulkano_shader_derive;

extern crate vulkano;

#[repr(C)]
#[derive(VulkanoStruct)]
struct Uniforms {
    pos: [u32; 2]
}

mod shader {
    #[derive(VulkanoShader)]
    #[ty = "vertex"]
    #[src = "
#version 450

layout(set = 0, binding = 0) uniform #[vulkano_struct(Uniforms)] uniforms;

layout(location = 0) in vec2 point;

void main() {
    gl_Position = vec4(uniforms.pos * point, 0, 1);
}
    "]
    struct Dummy;
}

fn main() {}
