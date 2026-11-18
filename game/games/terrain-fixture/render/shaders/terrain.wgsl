struct Vertex {
    @builtin(position) clip: vec4<f32>,
    @location(0) height: f32,
}

@vertex fn terrain_vs(@location(0) position: vec3<f32>) -> Vertex {
    return Vertex(frame.view_proj * vec4(position, 1.0), position.y);
}

@fragment fn terrain_fs(input: Vertex) -> @location(0) vec4<f32> {
    let color = mix(vec3(0.09, 0.24, 0.16), vec3(0.47, 0.51, 0.28), input.height / 105.0);
    return vec4(color, 1.0);
}
