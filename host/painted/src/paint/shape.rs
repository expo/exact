//! A box with corner radii and shapes: what a background, a border and a clip are cut to.
use super::{border, Rect4};

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

/// A resolved outline and the map from viewport points into its own space.
#[derive(Debug, Clone)]
pub(super) struct HitClip {
    path: std::rc::Rc<exact_kernel::svg::Path>,
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
    let clip = HitClip {
        path: std::rc::Rc::new(css.path()),
        even_odd: css.rule() == exact_kernel::FillRule::Evenodd,
        inverse: ts
            .invert()
            .map(|t| [t.sx, t.kx, t.tx, t.ky, t.sy, t.ty, 0., 0., 1.]),
    };
    let mut hit_clip = clip.clone();
    hit_clip.inverse = hit_ts
        .invert()
        .map(|t| [t.sx, t.kx, t.tx, t.ky, t.sy, t.ty, 0., 0., 1.]);
    for b in boxes {
        if let Some(hit) = &mut b.hit {
            hit.shape_clips.push(hit_clip.clone());
        }
        b.shape_clips.push(clip.clone());
    }
}

impl super::PaintedBox {
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
            ts.invert().is_some_and(|inverse| {
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
    pub(super) fn map_clips(&mut self, inverse: [f32; 9]) {
        for clip in &mut self.shape_clips {
            clip.inverse = clip.inverse.map(|m| super::space::mul3(&m, &inverse));
        }
    }

    pub(super) fn translate_clips(&mut self, x: f32, y: f32) {
        self.map_clips([1., 0., -x, 0., 1., -y, 0., 0., 1.]);
    }
}
