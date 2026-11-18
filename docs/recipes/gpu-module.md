# Recipe: a GPU module (a canvas drawn in Rust)

For a package, `bun exact.mjs add <package>` then `canvas surface=name(…)`. For compute
limits and features, see [device requirements](../reference.md#gpu-device-requirements).

A `canvas surface=name(…)` that no data module draws is drawn by the app's GPU module: a crate
`gpu/` beside app.json whose surfaces implement `exact_gpu::Surface`, loaded after the first pixel
(`apps/caltrain/gpu` is a whole one). `exact_gpu::wgpu` is wgpu 30, whose fields differ from older
tutorials': `gpu/README.md` covers shaders, dynamic offsets, performance and color.

**It joins a workspace exact2 generates for the app's GPU crates** (LLP 1153.003 D5.1), `.exact/gpu/`, apart
from the host's: write no root `Cargo.toml` and copy nothing up.

1. Write `gpu/` (below). Its `package.workspace = "../.exact/gpu"` makes it a member.
2. Draw it from the view (below), then `bun exact.mjs web-build`, or `mac`, as any app. The first build
   writes `.exact/gpu/`, whose lock is exact2's.
3. When `gpu/` depends on a crate, or a version, that exact2's lock does not hold, the build stops and says
   to run `bun exact.mjs lock`, which captures `Cargo.gpu.lock` beside app.json. Commit it.

`gpu/Cargo.toml`. Its package is the app's name plus `-gpu`; `module!` exports through the three web
dependencies, so they are required, at the workspace's versions (the web build's wasm-bindgen CLI
refuses others):

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
