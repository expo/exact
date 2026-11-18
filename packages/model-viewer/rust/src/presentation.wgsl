@group(0) @binding(0) var image: texture_2d<f32>;
@vertex fn vertex(@builtin(vertex_index) i: u32) -> @builtin(position) vec4f {
    return vec4f(f32(i & 1u) * 4.0 - 1.0, f32(i & 2u) * 2.0 - 1.0, 0.0, 1.0);
}
fn encode(c: vec3f) -> vec3f {
    return select(1.055 * pow(c, vec3f(1.0 / 2.4)) - 0.055, c * 12.92, c <= vec3f(0.0031308));
}
fn decode(c: vec3f) -> vec3f {
    return select(pow((c + 0.055) / 1.055, vec3f(2.4)), c / 12.92, c <= vec3f(0.04045));
}
fn premultiplied(p: vec4f) -> vec4f {
    let c = textureLoad(image, vec2i(p.xy), 0);
    if c.a == 0.0 { return vec4f(0.0); }
    return vec4f(encode(clamp(c.rgb / c.a, vec3f(0.0), vec3f(1.0))) * c.a, c.a);
}
@fragment fn encoded(@builtin(position) p: vec4f) -> @location(0) vec4f {
    return premultiplied(p);
}
@fragment fn srgb(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let c = premultiplied(p);
    // The target's sRGB transfer function must encode the already premultiplied bytes.
    return vec4f(decode(c.rgb), c.a);
}
@fragment fn copied_srgb(@builtin(position) p: vec4f) -> @location(0) vec4f {
    let c = textureLoad(image, vec2i(p.xy), 0);
    return vec4f(decode(c.rgb), c.a);
}
