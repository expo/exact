//! A box with corner radii and shapes: what a background, a border and a clip are cut to.
use super::{border, Rect4};
use std::rc::Rc;
use vello::kurbo::{BezPath, PathEl};

/// A rectangle with per-corner radii (top-left, top-right, bottom-right,
/// bottom-left), reduced by CSS's one factor so neighbours never overlap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Shape {
    /// The box.
    pub rect: Rect4,
    /// The radii.
    pub radii: [(f32, f32); 4],
    /// CSS `corner-shape` when any corner is not `round` (LLP 1077 D1).
    pub corners: Option<exact_kernel::corner::CornerShape>,
}

impl Shape {
    /// A box with radii, reduced as CSS reduces them: every corner by the
    /// one factor that fits the tightest edge.
    pub fn new(rect: Rect4, radii: [f32; 4]) -> Shape {
        Self::elliptical(rect, radii.map(|r| (r.max(0.0), r.max(0.0))))
    }

    /// A box with independent horizontal and vertical corner radii.
    pub fn elliptical(rect: Rect4, radii: [(f32, f32); 4]) -> Shape {
        Shape {
            rect,
            radii: border::reduced(radii, rect.2, rect.3),
            corners: None,
        }
    }

    /// A plain box.
    pub fn rect(rect: Rect4) -> Shape {
        Shape {
            rect,
            radii: [(0.0, 0.0); 4],
            corners: None,
        }
    }

    /// The same box with these corner shapes; `round` ones are none.
    pub fn with_corners(mut self, corners: Option<exact_kernel::corner::CornerShape>) -> Shape {
        self.corners = corners.filter(|c| !c.is_round());
        self
    }

    /// Whether any corner is rounded.
    pub fn rounded(&self) -> bool {
        self.radii.iter().any(|r| r.0 > 0.0 && r.1 > 0.0)
    }

    pub(super) fn rounded_outline(&self) -> Option<Rc<exact_kernel::svg::Path>> {
        self.rounded().then(|| Rc::new(hit_outline(self)))
    }

    /// The same box inset on every side (radii shrink with it).
    pub fn inset(&self, by: f32) -> Shape {
        Shape::elliptical(
            (
                self.rect.0 + by,
                self.rect.1 + by,
                (self.rect.2 - 2.0 * by).max(0.0),
                (self.rect.3 - 2.0 * by).max(0.0),
            ),
            self.radii
                .map(|(x, y)| ((x - by).max(0.0), (y - by).max(0.0))),
        )
        .with_corners(self.corners)
    }
}

/// CSS permits a radius larger than half the shorter side when adjacent radii fit.
pub(crate) fn bezier_outline(shape: &Shape) -> BezPath {
    let mut ops = Vec::new();
    border::shape_path(&mut ops, shape);
    bezier_path(&ops)
}

pub(crate) fn bezier_path(ops: &[border::PathOp]) -> BezPath {
    use border::PathOp;
    ops.iter()
        .map(|op| match *op {
            PathOp::Move(x, y) => PathEl::MoveTo(point(x, y)),
            PathOp::Line(x, y) => PathEl::LineTo(point(x, y)),
            PathOp::Cubic(a, b, c, d, x, y) => {
                PathEl::CurveTo(point(a, b), point(c, d), point(x, y))
            }
            PathOp::Close => PathEl::ClosePath,
        })
        .collect()
}

pub(crate) fn svg_bezier_path(path: &exact_kernel::svg::Path) -> BezPath {
    use exact_kernel::svg::Seg;
    path.0
        .iter()
        .map(|seg| match *seg {
            Seg::Move(x, y) => PathEl::MoveTo(point(x, y)),
            Seg::Line(x, y) => PathEl::LineTo(point(x, y)),
            Seg::Cubic(a, b, c, d, x, y) => PathEl::CurveTo(point(a, b), point(c, d), point(x, y)),
            Seg::Close => PathEl::ClosePath,
        })
        .collect()
}

fn point(x: f32, y: f32) -> vello::kurbo::Point {
    vello::kurbo::Point::new(f64::from(x), f64::from(y))
}

/// A resolved outline and the map from viewport points into its own space.
#[derive(Debug, Clone)]
pub(super) struct HitClip {
    path: Rc<exact_kernel::svg::Path>,
    even_odd: bool,
    pub(super) inverse: Option<[f32; 9]>,
}

impl HitClip {
    pub(super) fn translate(&mut self, x: f32, y: f32) {
        self.inverse = self
            .inverse
            .map(|m| super::space::mul3(&m, &[1., 0., -x, 0., 1., -y, 0., 0., 1.]));
    }
}

pub(super) fn clip_hits(
    boxes: &mut [super::PaintedBox],
    css: &exact_kernel::clip::ClipPath,
    ts: tiny_skia::Transform,
    hit_ts: tiny_skia::Transform,
) {
    if !css.has_path() {
        return;
    }
    path_hits(
        boxes,
        css.path(),
        css.rule() == exact_kernel::FillRule::Evenodd,
        ts,
        hit_ts,
    );
}

pub(super) fn overflow_hits(
    boxes: &mut [super::PaintedBox],
    shape: &Shape,
    ts: tiny_skia::Transform,
    hit_ts: tiny_skia::Transform,
) {
    // An upright rectangle is already represented by the intersected bounds.
    let upright = [ts, hit_ts].iter().all(|t| t.kx == 0. && t.ky == 0.);
    if boxes.is_empty() || (upright && !shape.rounded()) {
        return;
    }
    path_hits(boxes, hit_outline(shape), false, ts, hit_ts);
}

fn hit_outline(shape: &Shape) -> exact_kernel::svg::Path {
    use super::border::PathOp;
    use exact_kernel::svg::{Path, Seg};
    let mut ops = Vec::new();
    border::shape_path(&mut ops, shape);
    Path(
        ops.into_iter()
            .map(|op| match op {
                PathOp::Move(x, y) => Seg::Move(x, y),
                PathOp::Line(x, y) => Seg::Line(x, y),
                PathOp::Cubic(a, b, c, d, x, y) => Seg::Cubic(a, b, c, d, x, y),
                PathOp::Close => Seg::Close,
            })
            .collect(),
    )
}

fn path_hits(
    boxes: &mut [super::PaintedBox],
    path: exact_kernel::svg::Path,
    even_odd: bool,
    ts: tiny_skia::Transform,
    hit_ts: tiny_skia::Transform,
) {
    let clip = HitClip {
        path: Rc::new(path),
        even_odd,
        inverse: super::space::inverse_affine(ts)
            .map(|t| [t.sx, t.kx, t.tx, t.ky, t.sy, t.ty, 0., 0., 1.]),
    };
    let mut hit_clip = clip.clone();
    hit_clip.inverse = super::space::inverse_affine(hit_ts)
        .map(|t| [t.sx, t.kx, t.tx, t.ky, t.sy, t.ty, 0., 0., 1.]);
    for b in boxes {
        if let Some(hit) = &mut b.hit {
            hit.shape_clips.push(hit_clip.clone());
        }
        b.shape_clips.push(clip.clone());
    }
}

impl super::PaintedBox {
    /// Whether the point survives the border's corner cutouts, in hit space.
    pub(crate) fn inside_rounded_border(&self, x: f32, y: f32) -> bool {
        let Some(outline) = &self.rounded_border else {
            return true;
        };
        let (x, y) = self
            .projective
            .map_or((x, y), |(inv, ..)| crate::placement::map(&inv, x, y));
        self.affine
            .and_then(|(ts, _)| super::space::inverse_affine(ts))
            .is_some_and(|inverse| {
                let mut point = tiny_skia::Point::from_xy(x, y);
                inverse.map_point(&mut point);
                outline.contains((point.x, point.y), false)
            })
    }

    /// Whether a point (viewport points) is inside the box and its clip.
    pub fn contains(&self, x: f32, y: f32) -> bool {
        if self.hit.is_some() {
            return self.for_hit().contains(x, y);
        }
        let inside = |r: Rect4| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3;
        let (local_x, local_y) = self
            .projective
            .map_or((x, y), |(inv, ..)| crate::placement::map(&inv, x, y));
        let affine_hit = self.affine.is_none_or(|(ts, rect)| {
            super::space::inverse_affine(ts).is_some_and(|inverse| {
                let mut point = tiny_skia::Point::from_xy(local_x, local_y);
                inverse.map_point(&mut point);
                point.x >= rect.0
                    && point.x < rect.0 + rect.2
                    && point.y >= rect.1
                    && point.y < rect.1 + rect.3
            })
        });
        affine_hit
            && self.shape_clips.iter().all(|clip| {
                clip.inverse.is_some_and(|inv| {
                    clip.path
                        .contains(crate::placement::map(&inv, x, y), clip.even_odd)
                })
            })
            && inside(self.rect)
            && self.clip.is_none_or(inside)
            && self.projective.is_none_or(|(inv, rect, clip, planes)| {
                let (x, y) = crate::placement::map(&inv, x, y);
                let inside = |r: Rect4| x >= r.0 && x < r.0 + r.2 && y >= r.1 && y < r.1 + r.3;
                inside(rect) && clip.is_none_or(inside) && crate::placement::accepts(&planes, x, y)
            })
    }
    /// Whether shape clips leave painted coverage inside this rectangle.
    pub(crate) fn visible_through_shapes(&self, rect: Rect4, scale: f32) -> bool {
        if self.shape_clips.is_empty() {
            return true;
        }
        use exact_kernel::svg::Seg;
        use tiny_skia::{FillRule, Mask, PathBuilder, Rect, Transform};
        let (x, y, w, h) = rect;
        let device = Transform::from_row(scale, 0., 0., scale, -x * scale, -y * scale);
        let Some(mut mask) = Mask::new((w * scale).ceil() as u32, (h * scale).ceil() as u32) else {
            return false;
        };
        let Some(rect) = Rect::from_xywh(x, y, w, h) else {
            return false;
        };
        mask.fill_path(
            &PathBuilder::from_rect(rect),
            FillRule::Winding,
            true,
            device,
        );
        for clip in &self.shape_clips {
            let Some(inv) = clip.inverse else {
                return false;
            };
            let Some(ts) = super::space::inverse_affine(Transform::from_row(
                inv[0], inv[3], inv[1], inv[4], inv[2], inv[5],
            )) else {
                return false;
            };
            let mut builder = PathBuilder::new();
            for seg in &clip.path.0 {
                match *seg {
                    Seg::Move(x, y) => builder.move_to(x, y),
                    Seg::Line(x, y) => builder.line_to(x, y),
                    Seg::Cubic(a, b, c, d, x, y) => builder.cubic_to(a, b, c, d, x, y),
                    Seg::Close => builder.close(),
                }
            }
            let Some(path) = builder.finish() else {
                return false;
            };
            let rule = if clip.even_odd {
                FillRule::EvenOdd
            } else {
                FillRule::Winding
            };
            mask.intersect_path(&path, rule, true, device.pre_concat(ts));
        }
        mask.data().iter().any(|coverage| *coverage > 0)
    }

    pub(super) fn map_clips(&mut self, inverse: [f32; 9]) {
        for clip in &mut self.shape_clips {
            clip.inverse = clip.inverse.map(|m| super::space::mul3(&m, &inverse));
        }
    }

    pub(super) fn translate_clips(&mut self, x: f32, y: f32) {
        self.map_clips([1., 0., -x, 0., 1., -y, 0., 0., 1.]);
    }
}
