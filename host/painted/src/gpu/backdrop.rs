//! CSS `backdrop-filter` on the GPU painter (LLP 1053.000 D2).
//!
//! Vello draws a frame in one pass and cannot read what it has drawn, so a
//! backdrop flushes: the scene so far is rendered and read back, the patch
//! under the border box is filtered on the CPU with the CPU painter's own
//! blur and saturation (`exact_svg_raster`), and the scene starts again
//! from that frame as an image, with the open clip and opacity layers
//! pushed again and the patch drawn inside the box. One extra render and
//! readback per backdrop node per frame — declared in LLP 1053.000 §3, as is
//! that an opacity group's content before the backdrop is composited early.

use super::{shape, Gpu};
use crate::paint::Shape;
use exact_kernel::style::{BackdropFilter, BackdropOp};
use std::sync::Arc;
use tiny_skia::{IntSize, Pixmap, Transform};
use vello::kurbo::{Affine, BezPath, Rect, Shape as _};
use vello::peniko::{
    BlendMode, Color, Compose, Fill, ImageAlphaType, ImageBrush, ImageData, ImageFormat, Mix,
};

/// A layer open in the scene, as pushed, so a flush can push it again.
#[derive(Clone)]
pub(super) enum Layer {
    Clip(Fill, Affine, BezPath),
    Union(Affine, Vec<(Fill, BezPath)>, Rect),
    Opacity(f32),
    /// An isolated group clipped to a path: a mask's (LLP 1077 D2).
    Group(Affine, BezPath),
}

impl Layer {
    pub(super) fn svg_clip(clip: &exact_kernel::svg::scene::Clip, a: Affine) -> Self {
        let mut paths = Vec::new();
        for shape in &clip.shapes {
            let mut b = BezPath::new();
            for seg in &shape.path.0 {
                match *seg {
                    exact_kernel::svg::Seg::Move(x, y) => b.move_to((x as f64, y as f64)),
                    exact_kernel::svg::Seg::Line(x, y) => b.line_to((x as f64, y as f64)),
                    exact_kernel::svg::Seg::Cubic(p, q, r, s, x, y) => b.curve_to(
                        (p as f64, q as f64),
                        (r as f64, s as f64),
                        (x as f64, y as f64),
                    ),
                    exact_kernel::svg::Seg::Close => b.close_path(),
                }
            }
            let rule = if shape.even_odd {
                Fill::EvenOdd
            } else {
                Fill::NonZero
            };
            paths.push((rule, b));
        }
        if paths.len() <= 1 {
            let (rule, path) = paths.pop().unwrap_or((Fill::NonZero, BezPath::new()));
            Self::Clip(rule, a, path)
        } else {
            let bounds = paths
                .iter()
                .map(|(_, p)| p.bounding_box())
                .reduce(|a, b| a.union(b))
                .unwrap();
            Self::Union(a, paths, bounds)
        }
    }

    pub(super) fn bounds(&self) -> Option<Rect> {
        match self {
            Self::Clip(_, a, path) | Self::Group(a, path) => {
                Some(a.transform_rect_bbox(path.bounding_box()))
            }
            Self::Union(a, _, bounds) => Some(a.transform_rect_bbox(*bounds)),
            Self::Opacity(_) => None,
        }
    }

    fn pop(&self, scene: &mut vello::Scene) {
        if let Self::Union(a, paths, bounds) = self {
            // Union each child's coverage before masking the isolated content.
            scene.push_layer(
                Fill::NonZero,
                BlendMode::new(Mix::Normal, Compose::DestIn),
                1.0,
                *a,
                bounds,
            );
            for (rule, path) in paths {
                scene.fill(*rule, *a, Color::WHITE, None, path);
            }
            scene.pop_layer();
        }
        scene.pop_layer();
    }
}

struct Pixels(Arc<Pixmap>);
impl AsRef<[u8]> for Pixels {
    fn as_ref(&self) -> &[u8] {
        self.0.data()
    }
}

fn brush(p: Pixmap) -> ImageBrush {
    let (width, height) = (p.width(), p.height());
    ImageBrush::new(ImageData {
        data: vello::peniko::Blob::new(Arc::new(Pixels(Arc::new(p)))),
        format: ImageFormat::Rgba8,
        alpha_type: ImageAlphaType::AlphaPremultiplied,
        width,
        height,
    })
}

impl Gpu {
    pub(super) fn push_recorded(&mut self, layer: Layer) {
        if !self.record(super::rows::Op::Push(layer.clone())) {
            self.push_layer(&layer);
        }
        self.layers.push(layer);
    }

    pub(super) fn push_layer(&mut self, layer: &Layer) {
        match layer {
            Layer::Clip(fill, a, path) => self.scene.push_clip_layer(*fill, *a, path),
            Layer::Union(a, _, bounds) => {
                self.scene
                    .push_layer(Fill::NonZero, Mix::Normal, 1.0, *a, bounds)
            }
            Layer::Group(a, path) => {
                self.scene
                    .push_layer(Fill::NonZero, Mix::Normal, 1.0, *a, path)
            }
            Layer::Opacity(alpha) => {
                let whole = Rect::new(
                    0.0,
                    0.0,
                    (self.width * self.scale) as f64,
                    (self.height * self.scale) as f64,
                );
                self.scene
                    .push_layer(Fill::NonZero, Mix::Normal, *alpha, Affine::IDENTITY, &whole);
            }
        }
    }

    pub(super) fn pop_recorded(&mut self) {
        if let Some(layer) = self.layers.pop() {
            if !self.record(super::rows::Op::Pop) {
                layer.pop(&mut self.scene);
            }
        }
    }

    pub(super) fn filter_backdrop(&mut self, s: &Shape, filter: &BackdropFilter, ts: Transform) {
        let ts = self.record_transform(ts);
        if self.record(super::rows::Op::Backdrop(*s, filter.clone(), ts)) {
            return;
        }
        if s.rect.2 <= 0.0 || s.rect.3 <= 0.0 || self.image_refused {
            return;
        }
        let mut snapshot = self.scene.clone();
        for layer in self.layers.iter().rev() {
            layer.pop(&mut snapshot);
        }
        let Ok(frame) = self.render_scene(&snapshot) else {
            return;
        };
        let a = self.affine(ts);
        let outline = shape(s);
        let b = a.transform_rect_bbox(Rect::new(
            s.rect.0 as f64,
            s.rect.1 as f64,
            (s.rect.0 + s.rect.2) as f64,
            (s.rect.1 + s.rect.3) as f64,
        ));
        let (fw, fh) = (frame.width() as f64, frame.height() as f64);
        let (x0, y0) = (b.x0.floor().max(0.0), b.y0.floor().max(0.0));
        let (x1, y1) = (b.x1.ceil().min(fw), b.y1.ceil().min(fh));
        // The scene starts again from the frame so far, whatever the patch.
        self.scene.reset();
        let full = brush(frame.clone());
        self.scene.draw_image(&full, Affine::IDENTITY);
        let open = std::mem::take(&mut self.layers);
        for layer in &open {
            self.push_layer(layer);
        }
        self.layers = open;
        if !(x0 < x1 && y0 < y1) {
            return;
        }
        let (px, py, w, h) = (
            x0 as usize,
            y0 as usize,
            (x1 - x0) as usize,
            (y1 - y0) as usize,
        );
        let stride = frame.width() as usize * 4;
        let mut pixels = Vec::with_capacity(w * h * 4);
        for row in py..py + h {
            let at = row * stride + px * 4;
            pixels.extend_from_slice(&frame.data()[at..at + w * 4]);
        }
        let k = a.determinant().abs().sqrt() as f32;
        for op in &filter.0 {
            match op {
                BackdropOp::Blur(sigma) => {
                    exact_svg_raster::backdrop_blur(&mut pixels, w, h, sigma * k)
                }
                BackdropOp::Saturate(amount) => {
                    exact_svg_raster::backdrop_saturate(&mut pixels, *amount)
                }
            }
        }
        let Some(patch) =
            IntSize::from_wh(w as u32, h as u32).and_then(|s| Pixmap::from_vec(pixels, s))
        else {
            return;
        };
        self.scene.push_clip_layer(Fill::NonZero, a, &outline);
        self.scene
            .draw_image(&brush(patch), Affine::translate((x0, y0)));
        self.scene.pop_layer();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_kernel::svg::scene::{Clip, ClipShape};
    use vello::kurbo::Point;

    fn clip(children: &[(&str, bool)]) -> Clip {
        Clip {
            shapes: children
                .iter()
                .map(|(d, even_odd)| ClipShape {
                    path: exact_kernel::svg::parse_d_whole(d).unwrap(),
                    even_odd: *even_odd,
                })
                .collect(),
            then: None,
        }
    }

    fn covers(layer: &Layer, point: Point) -> bool {
        let filled = |rule, path: &BezPath, a: Affine| {
            let winding = path.winding(a.inverse() * point);
            match rule {
                Fill::NonZero => winding != 0,
                Fill::EvenOdd => winding % 2 != 0,
            }
        };
        match layer {
            Layer::Clip(rule, a, path) => filled(*rule, path, *a),
            Layer::Union(a, paths, _) => paths.iter().any(|(rule, path)| filled(*rule, path, *a)),
            _ => panic!("expected a clip"),
        }
    }

    #[test]
    fn svg_clip_children_union_without_cancelling_windings_or_holes() {
        let transform = Affine::translate((7., 9.)) * Affine::scale(2.);
        let cases = [
            // Opposite windings under different rules still cover the triangle.
            vec![("M0 0 H80 L0 80 Z", false), ("M0 0 V80 L80 0 Z", true)],
            // Evenodd belongs to each child; their overlap stays filled.
            vec![("M0 0 H80 V80 H0 Z", true), ("M10 10 H90 V90 H10 Z", true)],
            // A nonzero sibling must not fill the evenodd child's hole.
            vec![
                ("M0 0 H90 V90 H0 Z M10 10 H40 V40 H10 Z", true),
                ("M60 60 H80 V80 H60 Z", false),
            ],
        ];
        for (i, children) in cases.iter().enumerate() {
            let layer = Layer::svg_clip(&clip(children), transform);
            assert_eq!(
                covers(&layer, transform * Point::new(20., 20.)),
                i != 2,
                "clip {i} at its overlap or hole",
            );
            assert!(covers(&layer, transform * Point::new(5., 5.)));
            assert!(!covers(&layer, transform * Point::new(95., 95.)));
        }
        assert!(!covers(
            &Layer::svg_clip(&clip(&[]), transform),
            Point::ZERO
        ));
    }

    #[test]
    #[ignore = "requires a native GPU adapter; run explicitly in native validation"]
    fn svg_clip_union_intersects_and_restores_through_rows_and_backdrops() {
        use crate::paint::Backend;

        let mut gpu = Gpu::new().expect("native GPU adapter");
        let mut clip = clip(&[("M0 0 H80 L0 80 Z", false), ("M0 0 V80 L80 0 Z", true)]);
        clip.then = Some(Box::new(self::clip(&[("M0 0 H40 V80 H0 Z", false)])));
        for scale in [1., 2.] {
            for retained in [false, true] {
                gpu.page([0, 0, 255, 255]);
                gpu.begin(120., 120., scale);
                if retained {
                    gpu.row_begin(1, (10., 10.), None);
                }
                let ts = Transform::from_translate(10., 10.);
                let depth = gpu.push_svg_clip(&clip, ts);
                gpu.fill(&Shape::rect((0., 0., 80., 80.)), [255, 0, 0, 255], ts);
                gpu.backdrop_filter(
                    &Shape::rect((12., 12., 20., 20.)),
                    &BackdropFilter(vec![BackdropOp::Saturate(1.)]),
                    ts,
                );
                for _ in 0..depth {
                    gpu.pop_clip();
                }
                let (x, y) = if retained {
                    gpu.row_end((10., 10., 80., 80.));
                    gpu.row_draw(1, (30., 20.), (30., 20., 80., 80.));
                    (30., 20.)
                } else {
                    (10., 10.)
                };
                gpu.fill(
                    &Shape::rect((100., 100., 10., 10.)),
                    [0, 255, 0, 255],
                    Transform::identity(),
                );
                assert!(gpu.layers.is_empty());
                let frame = gpu.finish().unwrap();
                for (px, py, color) in [
                    (x + 20., y + 20., [255, 0, 0, 255]),
                    (x + 50., y + 20., [0, 0, 255, 255]),
                    (x + 30., y + 60., [0, 0, 255, 255]),
                    (105., 105., [0, 255, 0, 255]),
                ] {
                    assert_eq!(
                        frame
                            .pixel((px * scale) as u32, (py * scale) as u32)
                            .unwrap(),
                        tiny_skia::PremultipliedColorU8::from_rgba(
                            color[0], color[1], color[2], color[3],
                        )
                        .unwrap(),
                        "scale={scale}, retained={retained}, point=({px}, {py})",
                    );
                }
            }
        }
    }
}
