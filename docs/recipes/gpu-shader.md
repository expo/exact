# Recipe: a fragment shader in WGSL

Run `bun exact.mjs add @exact/shader`. In `app.contract`, import the package and give the
canvas the shader's asset path. A surface reports its first draw and any error through
`exactSurface("shader")`. If the browser rejects a shader while you run `bun exact.mjs web`,
the development page shows the file and the error. Click the message to dismiss it.
With a literal `src`, the build also checks that any WGSL `override` used by the fragment
has a default value.

```text
use ShaderState from "@exact/shader"

component App
  resource fx = exactSurface("shader") as shape ShaderState
  view
    column
      canvas surface=shader(src="assets/gradient.wgsl") width=240 height=160 aria-hidden=true
      if fx.error != ""
        text fx.error role="alert"
```

Write `assets/gradient.wgsl` with one fragment entry. This ports a Shadertoy
`mainImage(out vec4 fragColor, in vec2 fragCoord)` that colours pixels by position and time.

```wgsl
struct Uniforms {
    time: f32,
    resolution: vec2f,
}
@group(0) @binding(0) var<uniform> u: Uniforms;

@fragment
fn main(@builtin(position) pixel: vec4f) -> @location(0) vec4f {
    let fragCoord = vec2f(pixel.x, u.resolution.y - pixel.y);
    let uv = fragCoord / u.resolution;
    return vec4f(uv, 0.5 + 0.5 * cos(u.time), 1.0);
}
```

`u.resolution` is Shadertoy's `iResolution.xy` in device pixels. WGSL's pixel origin is the
top left, so subtracting `pixel.y` from the height gives Shadertoy's bottom-left `fragCoord`.
`u.time` is `iTime` in seconds. Return the colour you would assign to `fragColor`.
To add a parameter, declare a member such as `speed: f32` in `Uniforms`, then supply
`speed=0.5` beside `src`. A shader that reads time animates. `maxFps=30` caps its frame
rate, and Reduce Motion holds it still. For texture channels or multiple passes, use the
[Rust GPU surface](gpu-module.md).
