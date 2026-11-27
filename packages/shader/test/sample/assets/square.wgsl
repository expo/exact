struct Uniforms { resolution: vec2f, stripe: f32 }
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn square(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let point = pixel.xy / u.resolution * 64.0;
    if all(point >= vec2f(10)) && all(point < vec2f(30)) {
        return vec4f(1, 0, 0, 1);
    }
    if u.stripe > 0 && (point.x < 4 || (point.x >= 40 && point.x < 50)) {
        return vec4f(0, 0, 0, 0.5);
    }
    if u.stripe > 0 && point.x >= 50 && point.x < 60 {
        return vec4f(0.5, 0, 0, 0.5);
    }
    return vec4f(0);
}
