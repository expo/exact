# Recipe: a GPU module (a canvas drawn in Rust)

For WGSL alone, see [fragment shaders](gpu-shader.md). See also [device requirements](../reference.md#gpu-device-requirements).

A `canvas surface=name(…)` that no data module draws uses `gpu/` beside `app.json`,
loaded after first pixel. Implement `exact_gpu::Surface`; see `apps/caltrain/gpu`.
`exact_gpu::wgpu` is wgpu 30; `gpu/README.md` covers its use.

Name the crate after the app plus `-gpu`. Use the workspace versions for all
three web dependencies that `module!` exports through.

1. Create `gpu/Cargo.toml` as below. Exact generates its workspace at
   `.exact/gpu/` (LLP 1153.003 D5.1). The app needs no root `Cargo.toml`.
2. Add the canvas and run `bun exact.mjs web-build` or `bun exact.mjs mac`.
3. The workspace uses Exact's lockfile. If a dependency needs another version
   or crate, run `bun exact.mjs lock` and commit the resulting `Cargo.gpu.lock` beside `app.json`.

```toml
[package]
name = "field-log-gpu"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false
workspace = "../.exact/gpu"

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
exact-gpu.workspace = true

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen.workspace = true
wasm-bindgen-futures.workspace = true
web-sys.workspace = true
```

`gpu/src/lib.rs`, one surface that clears its canvas:

```rust
use exact_gpu::{wgpu, Frame, Registry, Surface, SurfaceError, Value};

struct Clear;

impl Surface for Clear {
    fn bind(&mut self, _inputs: &[Value], _at_ms: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn render(&mut self, _frame: &Frame, _device: &wgpu::Device, _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder, target: &wgpu::TextureView, _format: wgpu::TextureFormat) -> bool {
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target, depth_slice: None, resolve_target: None,
                ops: wgpu::Operations { load: wgpu::LoadOp::Clear(wgpu::Color::BLUE), store: wgpu::StoreOp::Store },
            })],
            ..Default::default()
        });
        false
    }
}

fn clear() -> Box<dyn Surface> {
    Box::new(Clear)
}

pub static REGISTRY: Registry = Registry { surfaces: &[("clear", 0, clear)], shaders: &[] };

exact_gpu::module!(REGISTRY);
```

```text
canvas surface=clear() width=200 height=120 testId="sky"
```

Return `true` from `Surface::wants_input` for raw events in `Surface::input`.
On macOS, Contract's `pointerdown` runs before the surface receives the press.
`pointermove`, `pointerup`, `wheel` and `contextmenu` handlers also run.
Update a camera in one handler so each gesture moves it once.

Native surfaces get file paths through [`Surface::storage`](../reference.md#gpu-storage).
On the web, use browser storage APIs.
