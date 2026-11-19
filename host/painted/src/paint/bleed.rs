//! A box at an edge of the page paints its background on to the window's edge
//! (LLP 1115.001 D3.7). The page is laid out in the safe area; the bands
//! between it and the window's edges (the status bar's, the navigation bar's,
//! a cutout's) are [`Painter::bands`], which the presenter sets from the
//! window a device's shell told and leaves zero under `viewport-fit="cover"`.
//! Only the background reaches into them: its colour and gradients, as a
//! Compose bar's container does, never its border, shadows or content.
use super::*;

impl Painter {
    /// How far a box's background reaches past each edge of the viewport
    /// (top, right, bottom, left): to the window's edge at each edge its border
    /// box touches or passes with square corners, when no ancestor clips or scrolls it and
    /// nothing scales, skews or rotates it. `outer` is in the coordinates `ts` places in the
    /// viewport. The ancestors are read from the tree, not the walk: a
    /// scroller's rows may be recorded apart, in coordinates of their own.
    pub(crate) fn bleed(
        &mut self,
        kernel: &Kernel,
        id: ViewId,
        outer: &Shape,
        ts: Transform,
    ) -> [f32; 4] {
        let moved = ts.sx != 1.0 || ts.sy != 1.0 || ts.kx != 0.0 || ts.ky != 0.0;
        if self.bands == [0.0; 4] || moved || !unclipped(kernel, id) {
            return [0.0; 4];
        }
        let (x, y, w, h) = outer.rect;
        let (x, y) = (x + ts.tx, y + ts.ty);
        let (vw, vh) = self.viewport;
        // How far each edge is past the viewport's: a box moving off the page
        // (a sheet gliding back up) keeps its background at the window's edge.
        let past = [-y, x + w - vw, y + h - vh, -x];
        let square =
            |i: usize| outer.radii[i] == (0.0, 0.0) && outer.radii[(i + 1) % 4] == (0.0, 0.0);
        let bleed: [f32; 4] = std::array::from_fn(|i| match past[i] > -0.5 && square(i) {
            true => (self.bands[i] - past[i].max(0.0)).max(0.0),
            false => 0.0,
        });
        // What it paints in a band is outside the box, where damage never looks.
        self.damage.unsupported |= bleed != [0.0; 4];
        bleed
    }
}

/// Whether no ancestor of `id` clips or scrolls what it paints.
fn unclipped(kernel: &Kernel, id: ViewId) -> bool {
    let arena = kernel.arena();
    let mut at = arena.key_of(id).and_then(|k| arena.parent(k.index));
    while let Some(slot) = at {
        match kernel.node(arena.local_id(slot)) {
            Some(n) if effective_overflow(&n) == (Overflow::Visible, Overflow::Visible) => {}
            _ => return false,
        }
        at = arena.parent(slot);
    }
    true
}

/// The geometry a background paints in: the box's, grown by its bleed.
pub(super) fn geometry(geometry: &BoxGeometry) -> BoxGeometry {
    let [t, r, b, l] = geometry.bleed;
    let (x, y, w, h) = geometry.outer.rect;
    let mut outer = geometry.outer;
    outer.rect = (x - l, y - t, w + l + r, h + t + b);
    BoxGeometry {
        outer,
        content: geometry.content,
        inset: geometry.inset,
        bleed: [0.0; 4],
    }
}
