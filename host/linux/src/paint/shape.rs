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
