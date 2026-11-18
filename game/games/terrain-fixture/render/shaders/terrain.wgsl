struct Vertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) height: f32,
}

struct LitVertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) height: f32,
    @location(1) light: f32,
}

fn terrain_color(height: f32) -> vec3<f32> {
    return mix(vec3(0.09, 0.24, 0.16), vec3(0.47, 0.51, 0.28), height / 105.0);
}

@vertex fn terrain_vs(@location(0) position: vec3<f32>) -> Vertex {
    return Vertex(frame.view_proj * vec4(position, 1.0), position.y);
}

@fragment fn terrain_fs(input: Vertex) -> @location(0) vec4<f32> {
    return vec4(terrain_color(input.height), 1.0);
}

@vertex fn lit_vs(@location(0) position: vec3<f32>, @location(1) light: f32) -> LitVertex {
    return LitVertex(frame.view_proj * vec4(position, 1.0), position.y, light);
}

@vertex fn packed_vs(@location(0) packed: vec3<f32>) -> LitVertex {
    let y = floor(packed.y);
    let light = 0.3 + 0.7 * (packed.y - y) * (16.0 / 15.0);
    let position = vec3(packed.x, y, packed.z);
    return LitVertex(frame.view_proj * vec4(position, 1.0), y, light);
}

@fragment fn lit_fs(input: LitVertex) -> @location(0) vec4<f32> {
    return vec4(terrain_color(input.height) * input.light, 1.0);
}

@fragment fn water_fs(_input: Vertex) -> @location(0) vec4<f32> {
    return vec4(0.08, 0.28, 0.46, 0.48);
}
