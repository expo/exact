@vertex
fn triangle(@builtin(vertex_index) index: u32) -> @builtin(position) vec4f {
    let points = array<vec2f, 3>(vec2f(-1.0, -1.0), vec2f(3.0, -1.0), vec2f(-1.0, 3.0));
    return vec4f(points[index], 0.0, 1.0);
}
