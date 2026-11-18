struct Camera {
    matrix: mat4x4f,
    eye: vec4f,
    clipPosition: vec4f,
    clipNormal: vec4f,
}
struct Style { color: vec4f, fade: vec4f }
@group(0) @binding(0) var<uniform> camera: Camera;
@group(0) @binding(1) var<storage, read> styles: array<Style>;
struct Vertex {
    @builtin(position) clip: vec4f,
    @location(0) position: vec3f,
    @location(1) normal: vec3f,
    @location(2) @interpolate(flat) primitive: u32,
}
@vertex fn vertex(@location(0) p: vec3f, @location(1) n: vec3f,
                  @location(2) node: u32, @location(3) primitive: u32) -> Vertex {
    var out: Vertex;
    out.clip = camera.matrix * vec4f(p, 1.0);
    out.position = p;
    out.normal = n;
    out.primitive = primitive;
    return out;
}
fn shade(v: Vertex, faded: bool) -> vec4f {
    if dot(v.position-camera.clipPosition.xyz, camera.clipNormal.xyz) < 0.0 { discard; }
    let style = styles[v.primitive];
    let normal = normalize(v.normal + vec3f(0.0, 0.0, 1e-20));
    let facing = abs(dot(normal, normalize(camera.eye.xyz-v.position)));
    var rgb = style.color.rgb * (0.25 + 0.75 * facing);
    var alpha = style.color.a;
    if faded {
        rgb = style.color.rgb + vec3f(pow(1.0-facing, 2.0) * 0.25);
        alpha *= style.fade.x;
    }
    return vec4f(rgb*alpha, alpha);
}
@fragment fn fragment(v: Vertex) -> @location(0) vec4f { return shade(v, false); }
@fragment fn faded(v: Vertex) -> @location(0) vec4f { return shade(v, true); }
