# Recipe: blended hook geometry

Draw blended geometry that needs only its own inputs in `Hooks::background`.
This stage needs no scene copy. Use `Hooks::surface` for geometry that samples
the scene copy and request `Needs::SCENE_COPY` (LLP 1182.000 D8).

Set up the pipeline in `prepare` with blending enabled and the colour format,
depth format and sample count from `FrameView`. Supply a pipeline that generates
six vertices from their indices.

```rust,no_run
use exact_game_render::{exact_gpu::wgpu, FrameView, Hooks, Needs, RenderError};

#[derive(Default)]
struct BlendedGeometry {
    pipeline: Option<wgpu::RenderPipeline>,
}

impl Hooks for BlendedGeometry {
    fn needs(&self) -> Needs {
        Needs::NONE
    }

    fn background(
        &mut self,
        pass: &mut wgpu::RenderPass<'_>,
        _: &FrameView<'_>,
    ) -> Result<(), RenderError> {
        if let Some(pipeline) = &self.pipeline {
            pass.set_pipeline(pipeline);
            pass.draw(0..6, 0..1);
        }
        Ok(())
    }
}
```

- `SCENE_COPY` makes the forward pass store its colour attachment. Single-sample
  targets and frames with a separate translucent pass already store it. The
  renderer's attachment estimate adds 12 bytes per allocated pixel, including
  the padding in the texture's capacity.
- `background` runs after opaque geometry, outlines and sky, before the
  engine's water and translucent draws. `surface` runs after the engine draws
  water and takes the copy, before the translucent draws. All geometry hooks
  run before engine translucency. The later `post` hook works on the HDR image.
- When moving a draw between stages, compare the same view in `surface` with
  the copy on and in `background` with the copy off. Keep engine water,
  particles and outlines in view.
