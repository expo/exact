struct Uniforms { tint: vec4f }
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn still() -> @location(0) vec4f {
    return u.tint;
}
