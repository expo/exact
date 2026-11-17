//! Rotate the completed output, leaving paint geometry in logical coordinates.
use super::*;
#[cfg(not(target_arch = "wasm32"))]
use exact_ink::device::{Device, TextureDesc};
#[cfg(not(target_arch = "wasm32"))]
use exact_ink::instance::TextureId;
use exact_ink::Rotation;
#[cfg(not(target_arch = "wasm32"))]
use exact_ink::Wgpu;

#[derive(Default)]
pub(crate) struct Output {
    pub rotation: Rotation,
    #[cfg(not(target_arch = "wasm32"))]
    fallback: Option<Pixels>,
}

#[cfg(not(target_arch = "wasm32"))]
struct Pixels {
    device: Wgpu,
    source: TextureId,
    target: TextureId,
}

impl Output {
    pub(super) fn finish(
        &mut self,
        backend: &mut dyn Backend,
        target: FrameTarget,
    ) -> Result<Option<Pixmap>, String> {
        if self.rotation == Rotation::R0 || target == FrameTarget::Geometry {
            return backend.finish_for(target);
        }
        if let Some(pixels) = backend.finish_rotated(self.rotation) {
            return pixels.map(Some);
        }
        let pixels = backend
            .finish_for(target)?
            .ok_or("rotation requires a pixel output")?;
        self.rotate(&pixels).map(Some)
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn rotate(&mut self, pixels: &Pixmap) -> Result<Pixmap, String> {
        let size = [pixels.width(), pixels.height()];
        let rotated = self.rotation.size(size);
        let result = (|| {
            if self.fallback.is_none() {
                let mut device = Wgpu::new()?;
                let source = device.create_texture(TextureDesc::rgba(size[0], size[1]))?;
                let target = device.create_target(rotated[0], rotated[1])?;
                self.fallback = Some(Pixels {
                    device,
                    source,
                    target,
                });
            }
            let p = self.fallback.as_mut().unwrap();
            p.source = p.device.resize_target(p.source, size[0], size[1])?;
            p.target = p.device.resize_target(p.target, rotated[0], rotated[1])?;
            p.device
                .upload_texture(p.source, [0, 0], size, pixels.data())?;
            self.rotation
                .blit(&mut p.device, p.source, size, p.target)?;
            p.device.readback(p.target)
        })()
        .map_err(|e: exact_ink::device::Error| e.to_string())?;
        Pixmap::from_vec(
            result,
            tiny_skia::IntSize::from_wh(rotated[0], rotated[1]).unwrap(),
        )
        .ok_or_else(|| "invalid rotated pixels".into())
    }
    #[cfg(target_arch = "wasm32")]
    fn rotate(&mut self, _: &Pixmap) -> Result<Pixmap, String> {
        Err("panel rotation requires a native GPU device".into())
    }
}

impl Painter {
    pub(crate) fn output_rotation(&self) -> Rotation {
        self.output.rotation
    }
}

impl<D: exact_runner::DataSource> crate::presenter::Presenter<D> {
    /// Clockwise output rotation. Captures show the same framebuffer as the panel;
    /// layout, agent targets and hit boxes retain logical coordinates.
    pub(crate) fn set_output_rotation(&mut self, rotation: Rotation) {
        self.brush.output = Output {
            rotation,
            ..Default::default()
        };
        self.repaint();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    #[test]
    fn captures_and_submitted_frames_rotate_after_layout_on_each_painter() {
        let plan = contract::compile(
            r##"component App
  view
    box width="100%" height="100%" background-color="#ff0000"
      box position="absolute" left=3 top=5 width=17 height=9 background-color="#00ff00"
"##,
        )
        .unwrap()
        .encode();
        let path = std::env::temp_dir().join(format!("exact-rotation-{}.png", std::process::id()));
        for (choice, scale) in [PainterChoice::Cpu, PainterChoice::Ink, PainterChoice::Gpu]
            .into_iter()
            .flat_map(|choice| [(choice, 1.), (choice, 2.)])
        {
            let mut p = Presenter::boot_with(&plan, (), (48., 32.), scale, ".".into(), choice)
                .unwrap()
                .0;
            for (w, h) in [(48., 32.), (72., 40.)] {
                assert!(p.resize(w, h).is_none());
                p.set_output_rotation(Rotation::R0);
                let base = p.frame();
                for (rotation, degrees) in [
                    (Rotation::R90, 90),
                    (Rotation::R180, 180),
                    (Rotation::R270, 270),
                    (Rotation::R0, 0),
                ] {
                    p.set_output_rotation(rotation);
                    let rotated = p.frame();
                    let viewport = p.host().runner().viewport();
                    assert_eq!((viewport.width, viewport.height), (w as f64, h as f64));
                    let [rw, rh] = rotation.size([base.width(), base.height()]);
                    assert_eq!((rotated.width(), rotated.height()), (rw, rh));
                    for y in 0..base.height() {
                        for x in 0..base.width() {
                            let (dx, dy) = match rotation {
                                Rotation::R0 => (x, y),
                                Rotation::R90 => (base.height() - 1 - y, x),
                                Rotation::R180 => (base.width() - 1 - x, base.height() - 1 - y),
                                Rotation::R270 => (y, base.width() - 1 - x),
                            };
                            assert_eq!(
                                rotated.pixel(dx, dy),
                                base.pixel(x, y),
                                "{choice:?} {rotation:?} {x},{y}"
                            );
                        }
                    }
                    let submitted = p.display_frame().unwrap();
                    assert_eq!(submitted.pixels.data(), rotated.data());
                    assert!(p.display_complete(&submitted));
                    let reply: serde_json::Value =
                        serde_json::from_str(&p.screenshot(path.to_str().unwrap()).unwrap())
                            .unwrap();
                    let capture = Pixmap::load_png(&path).unwrap();
                    std::fs::remove_file(&path).unwrap();
                    assert_eq!((capture.width(), capture.height()), (rw, rh));
                    assert_eq!(capture.data(), rotated.data());
                    assert_eq!(reply["w"], rw as f32 / scale);
                    assert_eq!(reply["h"], rh as f32 / scale);
                    assert_eq!(reply["rotation"], degrees);
                    if matches!(choice, PainterChoice::Ink) {
                        assert!(p.brush.output.fallback.is_none());
                    }
                }
            }
        }
    }
}
