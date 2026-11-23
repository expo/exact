// The four corners run clockwise from the panel's top left.
const corners = array(vec2f(0, 0), vec2f(1, 0), vec2f(1, 1), vec2f(0, 1));
const strip = array(0u, 1u, 3u, 2u);

@group(0) @binding(0) var source: texture_2d<f32>;

struct Vertex {
    @builtin(position) position: vec4f,
    @location(0) texel: vec2f,
}

@vertex fn vertex(@builtin(vertex_index) index: u32, @builtin(instance_index) turn: u32) -> Vertex {
    let corner = strip[index];
    let position = corners[corner] * vec2f(2, -2) + vec2f(-1, 1);
    let uv = corners[(corner + 4u - turn) % 4u];
    return Vertex(vec4f(position, 0, 1), uv * vec2f(textureDimensions(source)));
}

@fragment fn fragment(v: Vertex) -> @location(0) vec4f {
    return textureLoad(source, vec2i(v.texel), 0);
}
