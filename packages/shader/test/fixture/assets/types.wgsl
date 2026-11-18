struct Uniforms {
    f: f32,
    i: i32,
    n: u32,
    v2: vec2f,
    v3: vec3f,
    v4: vec4f,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn types(@builtin(position) at: vec4f) -> @location(0) vec4f {
    switch u32(at.x / 16.0) {
        case 0u: { return vec4f(u.f, 0.0, 0.0, 0.0); }
        case 1u: { return vec4f(f32(u.i) / 4.0, 0.0, 0.0, 0.0); }
        case 2u: { return vec4f(f32(u.n) / 4.0, 0.0, 0.0, 0.0); }
        case 3u: { return vec4f(u.v2, 0.0, 0.0); }
        case 4u: { return vec4f(u.v3, 0.0); }
        default: { return u.v4; }
    }
}
