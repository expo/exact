//! Material's container transform for navigationSource (LLP 1115.001 D12;
//! LLP 1013.001 §4, Android row).
//! The source follows ZoomPushIOS's id ordering. The route's whole box
//! transforms; there is no alignment to a matching image inside it.
use super::*;
use crate::paint::{FrameTarget, PaintedBox, Painter, Scene};

// MaterialContainerTransform's motionDurationMedium4 / motionDurationMedium3
// and motionEasingEmphasizedInterpolator; material-components-android's
// lib/java/com/google/android/material/motion/res/values/tokens.xml.
pub(super) const ENTER: f64 = 400.0;
const RETURN: f64 = 350.0;
// Its default thresholds (DEFAULT_ENTER_THRESHOLDS, DEFAULT_RETURN_THRESHOLDS),
// on the eased progress: the route fades in over its source, and on return
// the source fades in over the route; the height mask reaches the source's
// by 0.9 on return.
const ENTER_FADE: f32 = 0.25;
const RETURN_FADE: (f32, f32) = (0.6, 0.9);
const RETURN_MASK: f32 = 0.9;

/// The route and its source identity, retained until the route is popped.
pub(super) struct Source {
    pub(super) route: NodeKey,
    node: Option<NodeKey>,
    name: String,
    pub(super) bounds: Rect4,
}

#[derive(Debug, Clone, Copy)]
pub(super) struct Container {
    source: Rect4,
    pub(super) route: Rect4,
    /// The source, which the container draws in itself, fitted to its width.
    pub(super) node: Option<NodeKey>,
}

pub(super) fn progress(forward: bool, now: f64) -> Glide {
    Glide::new(
        0.0,
        1.0,
        now,
        if forward { ENTER } else { RETURN },
        Curve::Emphasized,
    )
}

/// A cancelled preview's return from `from` as a flight's curve: the
/// container plays its entrance back on the standard curve (`go_back`) and
/// shows `emphasized` of it, so the flights, held at `emphasized(from)`,
/// fall as the box does.
pub(super) fn returning(from: f32) -> exact_motion::Transition {
    let held = emphasized(from).max(f32::EPSILON);
    sampled(ENTER, |t| {
        1.0 - emphasized(from * (1.0 - Curve::Standard.ease(t))) / held
    })
}

fn sampled(ms: f64, curve: impl Fn(f32) -> f32) -> exact_motion::Transition {
    use exact_motion::easing::{Easing, LinearStop, MAX_LINEAR_STOPS};
    let last = (MAX_LINEAR_STOPS - 1) as f32;
    let stops = (0..MAX_LINEAR_STOPS).map(|i| LinearStop {
        input: f64::from(i as f32 / last),
        output: f64::from(curve(i as f32 / last)),
    });
    exact_motion::Transition {
        property: exact_motion::TransitionProperty::All,
        duration: ms / 1000.0,
        delay: 0.0,
        timing: exact_motion::TimingFunction::Easing(Easing::PiecewiseLinear(stops.collect())),
    }
}

/// Material's emphasized path has two cubic segments, not one Bézier.
pub(super) fn emphasized(t: f32) -> f32 {
    let cubic = |a: f32, b: f32, c: f32, d: f32, s: f32| {
        let u = 1.0 - s;
        u * u * u * a + 3.0 * u * u * s * b + 3.0 * u * s * s * c + s * s * s * d
    };
    let path = |s: f32| {
        if s < 0.5 {
            (
                cubic(0., 0.05, 0.133333, 0.166666, s * 2.),
                cubic(0., 0., 0.06, 0.4, s * 2.),
            )
        } else {
            (
                cubic(0.166666, 0.208333, 0.25, 1., s * 2. - 1.),
                cubic(0.4, 0.82, 1., 1., s * 2. - 1.),
            )
        }
    };
    let (mut lo, mut hi) = (0., 1.);
    for _ in 0..24 {
        let mid = (lo + hi) / 2.;
        if path(mid).0 < t {
            lo = mid;
        } else {
            hi = mid;
        }
    }
    path((lo + hi) / 2.).1
}

impl Container {
    /// The source's scale in the container: the route's width over its own.
    pub(super) fn fit(self) -> f32 {
        self.route.2 / self.source.2
    }

    pub(super) fn pose(self, forward: bool, t: f32, from: Option<Pose>) -> Pose {
        let (x, y, w, h) = self.route;
        let (sx, sy, sw, sh) = self.source;
        let small = Pose {
            dx: sx - x,
            dy: sy - y,
            scale: sw / w,
            radius: 16.0 / (sw / w),
            height: Some(sh / (sw / w)),
            ..Pose::REST
        };
        let full = from.filter(|_| !forward).unwrap_or(Pose::REST);
        // Fold the predictive pose's origin into its translation so the
        // return begins at exactly the preview's transform.
        let full = Pose {
            dx: full.dx + full.origin.0 * (1.0 - full.scale),
            dy: full.dy + full.origin.1 * (1.0 - full.scale),
            origin: (0., 0.),
            ..full
        };
        let (a, b) = if forward {
            (small, full)
        } else {
            (full, small)
        };
        let lerp = |a: f32, b: f32| a + (b - a) * t;
        let scale = lerp(a.scale, b.scale);
        // Both fitted to the container's width, the taller is masked to the
        // shorter's height (FitModeEvaluators.WIDTH).
        let mask = if forward {
            t
        } else {
            (t / RETURN_MASK).min(1.0)
        };
        let (ha, hb) = (a.height.unwrap_or(h), b.height.unwrap_or(h));
        Pose {
            dx: lerp(a.dx, b.dx),
            dy: lerp(a.dy, b.dy),
            scale,
            radius: lerp(a.radius * a.scale, b.radius * b.scale) / scale,
            height: Some(ha + (hb - ha) * mask),
            opacity: if forward {
                (t / ENTER_FADE).clamp(0., 1.)
            } else {
                let fade = (t - RETURN_FADE.0) / (RETURN_FADE.1 - RETURN_FADE.0);
                (1.0 - fade).clamp(0., 1.).min(full.opacity)
            },
            ..Pose::REST
        }
    }
}

impl<D: DataSource> Presenter<D> {
    pub(super) fn navigation_container(
        &mut self,
        before: &[NodeKey],
        stack: &[ViewId],
        keys: &[NodeKey],
        kinds: &[Presented],
    ) -> Option<Container> {
        let top = stack.len() - 1;
        let pushed = keys.len() == before.len() + 1 && keys.starts_with(before);
        let popped = before.len() == keys.len() + 1 && before.starts_with(keys);
        let mut container = None;
        if pushed && sheet_of(kinds, top).is_none() && sheet_of(kinds, top - 1).is_none() {
            let kernel = self.host.kernel();
            let route = kernel.node(stack[top])?;
            let name = route
                .props
                .str(PropId::NavigationSource)
                .filter(|s| !s.is_empty())?
                .to_owned();
            let bounds = (
                route.frame.x - self.page.0,
                route.frame.y - self.page.1,
                route.frame.width,
                route.frame.height,
            );
            if bounds.2 <= 0.0 || bounds.3 <= 0.0 {
                return None;
            }
            let source = self.source_node(stack[top - 1], &name);
            let node = source.and_then(|id| kernel.node(id)).map(|n| n.key);
            // The commit has hidden the route beneath; use its last paint.
            let shown = source.and_then(|id| self.source_rect(&self.boxes, id))?;
            self.stage.zooms.push(Source {
                route: keys[top],
                node,
                name,
                bounds,
            });
            container = Some(Container {
                source: shown,
                route: bounds,
                node,
            });
        } else if popped && sheet_of(kinds, top).is_none() {
            container = self.return_container(*before.last()?, stack[top]);
        }
        container
    }

    /// The container `route`'s pop returns into on `beneath`, the route it
    /// reveals: its source there, where it is still the one it came from
    /// and still shows.
    pub(super) fn return_container(&self, route: NodeKey, beneath: ViewId) -> Option<Container> {
        let link = self.stage.zooms.iter().find(|z| z.route == route)?;
        let source = self.source_node(beneath, &link.name).filter(|id| {
            self.host
                .kernel()
                .node(*id)
                .is_some_and(|n| Some(n.key) == link.node)
        })?;
        let boxes = self.return_boxes(beneath);
        self.source_rect(&boxes, source).map(|shown| Container {
            source: shown,
            route: link.bounds,
            node: link.node,
        })
    }

    /// The source a container draws in itself while `route` is its route,
    /// the scale that fits it to the route's width, and whether it returns.
    pub(super) fn container_lift(&self, route: NodeKey) -> Option<(NodeKey, f32, bool)> {
        let (key, container, back) = match &self.stage.shift {
            Some(s) => match s.forward {
                true => (s.incoming, s.container?, false),
                false => (s.outgoing, s.container?, true),
            },
            None => {
                let p = self.stage.preview.as_ref()?;
                (p.pair()?.0, p.container?, true)
            }
        };
        (key == route).then_some((container.node?, container.fit(), back))
    }

    /// Keep each binding current while its route is in any tab's stack.
    /// After teardown, Back uses the last committed binding (LLP 1013.001
    /// §4, Android row; LLP 1115.001 D12).
    pub(super) fn sync_sources(&mut self) {
        let kernel = self.host.kernel();
        let sources = std::mem::take(&mut self.stage.zooms);
        for mut source in sources {
            let Some(route) = kernel.node_by_key(source.route) else {
                continue;
            };
            let Some((stack, at)) = self.host.navigation.bars.stacks.iter().find_map(|stack| {
                stack
                    .iter()
                    .position(|id| *id == route.id)
                    .map(|at| (stack, at))
            }) else {
                continue;
            };
            source.name = route
                .props
                .str(PropId::NavigationSource)
                .unwrap_or("")
                .to_owned();
            source.node = at
                .checked_sub(1)
                .filter(|_| !source.name.is_empty())
                .and_then(|below| self.source_node(stack[below], &source.name))
                .and_then(|id| kernel.node(id))
                .map(|n| n.key);
            self.stage.zooms.push(source);
        }
    }

    fn source_node(&self, beneath: ViewId, name: &str) -> Option<ViewId> {
        self.host
            .kernel()
            .preorder_where(&[beneath], |_, props| props.str(PropId::Id) == Some(name))
            .into_iter()
            .min()
    }

    fn source_rect(&self, boxes: &[PaintedBox], source: ViewId) -> Option<Rect4> {
        let kernel = self.host.kernel();
        let node = kernel.node(source)?;
        if !crate::paint::paints(kernel, source, None) {
            return None;
        }
        let mut ancestor = Some(source);
        while let Some(id) = ancestor {
            // Android's reader owns these samples; neither the last paint
            // nor a geometry walk can locate their live layer (LLP 1115.001 D12).
            let key = kernel.node(id)?.key;
            if self.host.lowered_mask(key)
                & (crate::host::lower::LOWER_TRANSFORM | crate::host::lower::LOWER_OPACITY)
                != 0
            {
                return None;
            }
            if self.host.presented(id).opacity <= 0.0 {
                return None;
            }
            ancestor = kernel.node(id).and_then(|n| n.parent);
        }
        let b = boxes.iter().find(|b| b.id == node.id)?;
        // Under a perspective, rotation or skew the box's clip is not square
        // to the screen and the rectangle test below cannot tell whether it
        // shows; rather than zoom toward a source that may be clipped away,
        // such a source takes the fallback (QUEUE.md, #808).
        if !b.flat() {
            return None;
        }
        let r = b.rect;
        let clip = b.clip.unwrap_or((0., 0., self.viewport.0, self.viewport.1));
        let left = r.0.max(clip.0).max(0.);
        let top = r.1.max(clip.1).max(0.);
        let right = (r.0 + r.2).min(clip.0 + clip.2).min(self.viewport.0);
        let bottom = (r.1 + r.3).min(clip.1 + clip.3).min(self.viewport.1);
        (r.2 > 0.
            && r.3 > 0.
            && right > left
            && bottom > top
            && b.visible_through_shapes((left, top, right - left, bottom - top), self.brush.scale))
        .then_some(r)
    }

    /// Recheck the revealed route on Back: a live source can have scrolled,
    /// moved, or become clipped while its route was covered. This geometry
    /// walk draws no pixels and leaves the frame's tapes and hit boxes alone.
    fn return_boxes(&self, beneath: ViewId) -> Vec<PaintedBox> {
        let displaced = self.displaced_scroll(&self.scroll);
        let host = &self.host;
        let roots = host.roots();
        let scene = Scene {
            kernel: host.kernel(),
            roots: &roots,
            hidden: &|id| host.route_hidden_but(id, &[beneath]),
            presented: &|id| host.presented(id),
            scaled_presented: host.scaled_presented(),
            paths: &|id| host.presented_path(id),
            filters: &|id| host.presented_filter(id),
            clips: &|id| host.presented_clip(id),
            scroll: displaced.as_ref().unwrap_or(&self.scroll),
            page: self.page,
            images: &self.images.bitmaps,
            focus: None,
            selection: None,
            caret_upstream: false,
            pointer: None,
            controls: &self.controls,
            chosen: &self.chosen,
            menu: None,
        };
        let mut painter = Painter::new(
            self.text.clone(),
            self.brush.scale,
            Box::new(crate::raster::Raster::new()),
        );
        painter
            .paint_for(&scene, self.viewport, FrameTarget::Geometry)
            .map(|f| f.boxes)
            .unwrap_or_default()
    }
}
