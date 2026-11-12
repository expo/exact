# Recipe: a GPU module (a canvas drawn in Rust)

A `canvas surface=name(…)` that no data module draws is drawn by the app's GPU module: a crate
`gpu/` beside app.json whose surfaces implement `exact_gpu::Surface` (wgpu), loaded after the first
pixel (`apps/caltrain/gpu` is a whole one). For heavy shaders, read `gpu/README.md`
"Performance" (loop bounding, `opaque`, pipeline cache).

**It is Rust of the app's own, so the app writes its Cargo workspace** (LLP 1153 D1.4). Start from
the generated one; don't write a root `Cargo.toml` by hand:

1. Run a native verb once (`bun exact.mjs mac`, `ios`…): it writes `.exact/shells/`. It writes it
   only while the app has no crate of its own, so move an existing `gpu/` aside until step 2 is done.
2. Copy it up: `cp -R .exact/shells/. . && rm -rf .exact/shells .stamp.json`.
3. Write `gpu/` (below) and add `"gpu"` to `members` in `Cargo.toml`, then run `bun exact.mjs update`
   (it settles the copied crates' paths and `<exact2>`) and `bun exact.mjs lock`.
4. The web build's wasm-bindgen must be the version `exact setup` installed. When `lock` says the
   GPU module links another, write the `=` requirement it names in `gpu/Cargo.toml` and
   run `bun exact.mjs lock` again.
5. Draw it from the view (below), then `bun exact.mjs web-build`, or `mac`, as any app.

`gpu/Cargo.toml`. Its package is the app's name plus `-gpu`; `module!` exports through the three web
dependencies, so they are required:

```toml
[package]
name = "field-log-gpu"
version.workspace = true
edition.workspace = true
license.workspace = true
publish = false

[lib]
crate-type = ["cdylib", "rlib"]

[dependencies]
exact-gpu = { path = "<exact2>/gpu" }

[target.'cfg(target_arch = "wasm32")'.dependencies]
wasm-bindgen = "0.2"
wasm-bindgen-futures = "0.4"
web-sys = { version = "0.3", features = ["HtmlCanvasElement"] }
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

/// Each surface: its name, its number of inputs, its factory.
pub static REGISTRY: Registry = Registry { surfaces: &[("clear", 0, clear)], shaders: &[] };

exact_gpu::module!(REGISTRY);
```

```text
canvas surface=clear() width=200 height=120 testId="sky"
```
