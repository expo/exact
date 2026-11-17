//! A zoomable scroll container's geometry (@ref LLP 1057.001.000 D1, D3):
//! what the painter draws and what the presenter scrolls by. The child keeps
//! the box layout gave it; at zoom `z` it is drawn scaled by `z` from its top
//! left, outside its own transform, and placed by D3: padding and margins
//! unscaled, the scaled margin box centred by half the slack, the extent the
//! larger of the port and what the content reaches.
use super::*;

/// D1's range, when either row is set on a container that scrolls on both
/// axes: its minimum, and the larger of the two. A value that is not finite
/// and above zero is the default, 1.
pub(crate) fn range(node: &NodeRef<'_>) -> Option<(f32, f32)> {
    let style = node.style;
    if !style.mask.has(StyleId::MinZoom) && !style.mask.has(StyleId::MaxZoom) {
        return None;
    }
    let scrolls = |o: Overflow| matches!(o, Overflow::Auto | Overflow::Scroll);
    let (x, y) = effective_overflow(node);
    if !scrolls(x) || !scrolls(y) {
        return None;
    }
    let valid = |v: f32| if v.is_finite() && v > 0. { v } else { 1. };
    let low = valid(style.rare.min_zoom);
    Some((low, low.max(valid(style.rare.max_zoom))))
}

/// D3's terms on both axes for one container and its one child, unscaled.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct Fit {
    /// The child, which the zoom scales.
    pub child: ViewId,
    /// Where layout put the child's border box in the port: the leading
    /// padding and margin.
    pub lead: (f32, f32),
    /// The child's border box.
    size: (f32, f32),
    /// After the child's border box: its end margin and the container's end
    /// padding.
    trail: (f32, f32),
    /// The port: the container's padding box.
    pub port: (f32, f32),
}

impl Fit {
    /// The geometry of `node`, a zoomable container, from its last layout;
    /// `None` while it holds no element child.
    pub(crate) fn of(kernel: &Kernel, node: &NodeRef<'_>) -> Option<Self> {
        let child = node.children().into_iter().find_map(|id| {
            kernel
                .node_unblocked(id)
                .filter(|c| !c.is_inline_run() && c.style.display != Display::None)
        })?;
        let (bl, bt, br, bb) = kernel.resolved_border(node.key).unwrap_or_default();
        let (_, _, pr, pb) = kernel.layout_padding(node.key).unwrap_or_default();
        let [_, mr, mb, _] = kernel.laid_margins(child.id).unwrap_or_default();
        let f = node.frame;
        let c = child.frame;
        Some(Self {
            child: child.id,
            lead: (c.x - f.x - bl, c.y - f.y - bt),
            size: (c.width, c.height),
            trail: (mr.max(0.) + pr, mb.max(0.) + pb),
            port: ((f.width - bl - br).max(0.), (f.height - bt - bb).max(0.)),
        })
    }

    /// How far the content reaches at zoom `z`: `p₁ + M + p₂`.
    fn used(&self, z: f32) -> (f32, f32) {
        (
            self.lead.0 + self.size.0 * z + self.trail.0,
            self.lead.1 + self.size.1 * z + self.trail.1,
        )
    }

    /// Half the slack at zoom `z`: how far the child's scaled box stands
    /// from where layout put it.
    pub(crate) fn shift(&self, z: f32) -> (f32, f32) {
        let used = self.used(z);
        (
            (self.port.0 - used.0).max(0.) / 2.,
            (self.port.1 - used.1).max(0.) / 2.,
        )
    }

    /// The farthest offset at zoom `z`: the extent less the port.
    pub(crate) fn max(&self, z: f32) -> (f32, f32) {
        let used = self.used(z);
        (
            (used.0 - self.port.0).max(0.),
            (used.1 - self.port.1).max(0.),
        )
    }

    /// The offset at zoom `to` that keeps under `focal` (a point of the port,
    /// from its top left) the child's point that was there at zoom `from`
    /// and offset `offset`, before clamping.
    pub(crate) fn about(
        &self,
        from: f32,
        offset: (f32, f32),
        focal: (f32, f32),
        to: f32,
    ) -> (f32, f32) {
        let (was, now) = (self.shift(from), self.shift(to));
        let axis = |offset: f32, focal: f32, lead: f32, was: f32, now: f32| {
            let u = (offset + focal - lead - was) / from;
            lead + now + u * to - focal
        };
        (
            axis(offset.0, focal.0, self.lead.0, was.0, now.0),
            axis(offset.1, focal.1, self.lead.1, was.1, now.1),
        )
    }
}

impl Painter {
    /// The zoom `node`'s child is drawn at, when `node` is a zoomable
    /// container: what the presenter set, else the range's nearest to 1.
    pub(crate) fn zoom_of(&self, node: &NodeRef<'_>) -> Option<f32> {
        let (low, high) = range(node)?;
        Some(
            self.zoom
                .get(&node.id)
                .copied()
                .unwrap_or(1.)
                .clamp(low, high),
        )
    }

    /// A zoomable container's travel: D3's extent less its port.
    pub(crate) fn zoom_bounds(&self, kernel: &Kernel, node: &NodeRef<'_>) -> Option<ScrollBounds> {
        let z = self.zoom_of(node)?;
        let fit = Fit::of(kernel, node)?;
        Some(ScrollBounds {
            axes: effective_overflow(node),
            max: fit.max(z),
        })
    }

    /// The transform the children of `node` are drawn under: `ts`, and for
    /// a zoomable container its child scaled from its top left and placed
    /// by D3. `offset` is where the children are painted from (the
    /// container's scroll folded in).
    pub(super) fn zoom_children(
        &self,
        kernel: &Kernel,
        node: &NodeRef<'_>,
        ts: Transform,
        offset: (f32, f32),
    ) -> Transform {
        let Some((z, fit)) = self.zoom_of(node).zip(Fit::of(kernel, node)) else {
            return ts;
        };
        let Some(child) = kernel.node(fit.child) else {
            return ts;
        };
        let at = (child.frame.x - offset.0, child.frame.y - offset.1);
        let shift = fit.shift(z);
        ts.pre_translate(at.0 + shift.0, at.1 + shift.1)
            .pre_scale(z, z)
            .pre_translate(-at.0, -at.1)
    }
}

impl PaintedBox {
    /// A viewport point in the box's own coordinates, from its border box's
    /// top left, through the transform it was painted under (a zoom around
    /// it included).
    pub(crate) fn local(&self, x: f32, y: f32) -> Option<(f32, f32)> {
        let (ts, rect) = self.affine?;
        let mut p = [Point::from_xy(x, y)];
        ts.invert()?.map_points(&mut p);
        Some((p[0].x - rect.0, p[0].y - rect.1))
    }
}

#[cfg(test)]
mod tests {
    use super::Fit;

    fn fit() -> Fit {
        // A 300 by 200 port with 10 of padding all round, and a 100 by 50
        // child with a 5 margin all round.
        Fit {
            child: 1,
            lead: (15., 15.),
            size: (100., 50.),
            trail: (15., 15.),
            port: (300., 200.),
        }
    }

    #[test]
    fn a_child_smaller_than_the_port_is_centred_and_does_not_scroll() {
        let f = fit();
        // Content 130 by 80 of the 300 by 200 port: half the slack before.
        assert_eq!(f.shift(1.), (85., 60.));
        assert_eq!(f.max(1.), (0., 0.));
    }

    #[test]
    fn padding_and_margins_stay_unscaled_as_the_child_grows() {
        let f = fit();
        // At 4 the margin box is 410 by 210: no slack, and the extent is
        // the padding and the margins around the scaled child.
        assert_eq!(f.shift(4.), (0., 0.));
        assert_eq!(f.max(4.), (430. - 300., 230. - 200.));
    }

    #[test]
    fn the_point_under_the_focal_point_stays_there() {
        let f = fit();
        // At 1 the child's top left is at (100, 75) of the port, its centre
        // at (150, 100); at 2 its centre is still there.
        let to = f.about(1., (0., 0.), (150., 100.), 2.);
        let shift = f.shift(2.);
        assert_eq!(15. + shift.0 + 50. * 2. - to.0, 150.);
        assert_eq!(15. + shift.1 + 25. * 2. - to.1, 100.);
    }
}
