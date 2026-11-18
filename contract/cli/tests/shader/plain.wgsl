struct Uniforms {
  speed: f32,
  tint: vec4f,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn picture() -> @location(0) vec4f {
  return u.tint * u.speed;
}
