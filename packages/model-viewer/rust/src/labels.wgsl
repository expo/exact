@group(0) @binding(0) var image: texture_2d<f32>;
@group(0) @binding(1) var smp: sampler;
struct Vertex {
    @builtin(position) p: vec4f,
    @location(0) uv: vec2f,
    @location(1) opacity: f32,
}
@vertex fn vertex(@location(0) p: vec2f, @location(1) uv: vec2f, @location(2) opacity: f32) -> Vertex {
    return Vertex(vec4f(p, 0.0, 1.0), uv, opacity);
}
@fragment fn fragment(v: Vertex) -> @location(0) vec4f {
    return textureSample(image, smp, v.uv) * v.opacity;
}
