struct Uniforms {
    time: f32,
    resolution: vec2f,
    devicePixelRatio: f32,
    speed: f32,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn motion(@builtin(position) at: vec4f) -> @location(0) vec4f {
    return vec4f(fract(u.time * u.speed / 4.0), at.y / u.resolution.y, u.devicePixelRatio / 4.0, 0.0);
}
