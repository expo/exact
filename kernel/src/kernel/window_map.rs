//! Where a content owner, or the document, lies in its window (@ref LLP 1123 D6's `"window"`
//! space): the map from its view's coordinates into its window's, which its host reports in the
//! owner's or the session's update. No layout reads it.

use super::area::{holder, Holder};
use super::Kernel;
use crate::id::ViewId;

/// A view's coordinates' map into its window's, in `CGAffineTransform`'s order `a b c d tx ty`:
/// a point (x, y) goes to (a·x + c·y + tx, b·x + d·y + ty). A sheet UIKit scales back behind
/// another maps with a scale below 1.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WindowMap(pub [f32; 6]);

impl WindowMap {
    /// The rectangle `x y width height` covers once mapped: the bounds of its four corners.
    pub fn rect(&self, [x, y, w, h]: [f64; 4]) -> [f64; 4] {
        let [a, b, c, d, tx, ty] = self.0.map(f64::from);
        let corners = [(x, y), (x + w, y), (x, y + h), (x + w, y + h)];
        let mapped = corners.map(|(px, py)| (a * px + c * py + tx, b * px + d * py + ty));
        let (xs, ys) = (mapped.map(|p| p.0), mapped.map(|p| p.1));
        let (x0, y0) = (
            xs.into_iter().fold(f64::MAX, f64::min),
            ys.into_iter().fold(f64::MAX, f64::min),
        );
        let (x1, y1) = (
            xs.into_iter().fold(f64::MIN, f64::max),
            ys.into_iter().fold(f64::MIN, f64::max),
        );
        [x0, y0, x1 - x0, y1 - y0]
    }

    /// Why a host's map is refused, if it is: a nonfinite term, or one that folds the plane
    /// onto a line, which no view's map is.
    pub(super) fn refusal(map: Option<&WindowMap>) -> Option<&'static str> {
        let [a, b, c, d, ..] = map?.0;
        if !map?.0.iter().all(|v| v.is_finite()) {
            return Some(
                "a window map with a nonfinite term; report a view out of its window without one",
            );
        }
        (a * d - b * c == 0.0).then_some("a window map that folds the plane onto a line; report the view's own map into its window")
    }
}

impl Kernel {
    /// The map into its window of the owner holding the box `view`, or of the document outside
    /// every owner, a document top-layer box's content included, as its host last reported it;
    /// `None` where it reported none.
    pub fn window_map(&self, view: ViewId) -> Option<WindowMap> {
        let slot = self.arena.slot_of(view)?;
        let observations = &self.arena.observations;
        match holder(&self.arena, slot) {
            Holder::Owner(owner) => observations.window_maps.get(&owner).copied(),
            Holder::TopLayer(_) | Holder::Session => observations.session_window_map,
        }
    }

    /// Hold `map` for the owner whose root is `slot`, or for the session where `slot` is `None`.
    pub(super) fn set_window_map(&mut self, slot: Option<u32>, map: Option<WindowMap>) {
        let observations = &mut self.arena.observations;
        match (slot, map) {
            (Some(s), Some(m)) => drop(observations.window_maps.insert(s, m)),
            (Some(s), None) => drop(observations.window_maps.remove(&s)),
            (None, m) => observations.session_window_map = m,
        }
    }
}
