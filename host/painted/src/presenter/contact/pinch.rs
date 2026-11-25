//! The transform hold follows the midpoint and span of its fingers (LLP 1057.001 §4).
use super::*;

type Point = (f32, f32);
type Finger = (u64, Point);

#[derive(Clone, Copy)]
pub(super) struct Pinch {
    first: Finger,
    second: Option<Finger>,
    binding: TransformDragBinding,
    revision: u64,
    began: f64,
    scroll: [Option<ViewId>; 2],
    scrolling: Option<(ViewId, usize)>,
    starts: [Point; 2],
    plane: tiny_skia::Transform,
    centre: Point,
    anchor: (Point, f64),
    values: [Value; 2],
}

impl Pinch {
    fn measure(&self) -> (Point, f64) {
        let a = self.first.1;
        let Some((_, b)) = self.second else {
            return (a, 0.);
        };
        (
            ((a.0 + b.0) * 0.5, (a.1 + b.1) * 0.5),
            (a.0 - b.0).hypot(a.1 - b.1) as f64,
        )
    }

    pub(super) fn position(&self) -> [Value; 2] {
        let (focus, span) = self.measure();
        let (start, initial_span) = self.anchor;
        let k = if initial_span > 0. && span > 0. {
            span / initial_span
        } else {
            1.
        };
        focused(self.values, self.local(start), self.local(focus), k)
    }

    fn local(&self, p: Point) -> Point {
        let mut p = tiny_skia::Point::from_xy(p.0, p.1);
        self.plane.map_point(&mut p);
        (p.x - self.centre.0, p.y - self.centre.1)
    }
}

fn focused(values: [Value; 2], from: Point, to: Point, k: f64) -> [Value; 2] {
    let [t, scale] = values;
    let x = to.0 as f64 - k * (from.0 as f64 - t.x);
    let y = to.1 as f64 - k * (from.1 as f64 - t.y);
    [Value::new(x, y), Value::new(scale.x * k, scale.y * k)]
}

impl<D: DataSource> Presenter<D> {
    /// Whether the hit leaves the touch pinch to its scroll container.
    pub(in crate::presenter) fn allows_touch_zoom(
        &self,
        hit: ViewId,
        through: Option<ViewId>,
    ) -> bool {
        let kernel = self.host.kernel();
        let mut at = Some(hit);
        while let Some(id) = at {
            let Some(node) = kernel.node(id) else {
                return false;
            };
            if !node.style.touch_action.pinch_zoom() {
                return false;
            }
            if Some(id) == through {
                break;
            }
            at = node.parent;
        }
        true
    }

    fn pinch_binding(&mut self, at: Point) -> Option<TransformDragBinding> {
        let hit = self.hit(at.0, at.1)?;
        if self.allows_touch_zoom(hit, self.zoomable_from(hit)) {
            return None;
        }
        let node = self.host.kernel().node(hit)?;
        let Candidate::Transform(b) = self.transform_candidate(node.key)? else {
            return None;
        };
        Some(b)
    }

    /// The agent supplies a recognized pinch, paced like UIKit's recognized carrier.
    pub(in crate::presenter) fn drive_transform_pinch(
        &mut self,
        at: Point,
        factor: f64,
    ) -> Result<bool, String> {
        if self.contact_position().is_some() {
            return Err("another contact is down".into());
        }
        let Some(binding) = self.pinch_binding(at) else {
            return Ok(false);
        };
        let clip = self.host.kernel().node_by_key(binding.clip).unwrap().id;
        let Some(frame) = self.pane_frame(clip) else {
            return Ok(false);
        };
        let Some(point) = self.box_of(clip).and_then(|b| b.local(at.0, at.1)) else {
            return Ok(false);
        };
        let focus = (point.0 - frame.width / 2., point.1 - frame.height / 2.);
        let now = self.host.now();
        let Some(mut held) = self.begin_transform_drag(binding, now, now)? else {
            return Ok(false);
        };
        let HeldKind::Transform { pair, .. } = held.kind else {
            unreachable!()
        };
        let start = [pair.translate().value, pair.scale().value];
        let result = (|| {
            for i in 1..=8 {
                let k = 1. + (factor - 1.) * (i as f64 / 8.);
                let values = focused(start, focus, focus, k);
                if !self.move_transform_drag(
                    &mut held,
                    Value::ZERO,
                    Some(values),
                    now,
                    now + i as f64 * 1000. / 60.,
                )? {
                    return Ok(false);
                }
            }
            self.finish_transform_drag(&held, true, now)
        })();
        if !matches!(result, Ok(true)) {
            self.end_transform_drag(pair, None, now)?;
        }
        self.set_collection_interaction(None);
        result
    }

    /// Remember both contacts. An existing pan keeps its hold across the change in membership.
    pub fn begin_transform_pinch(
        &mut self,
        first: Finger,
        next: u64,
        at: Point,
        event: f64,
    ) -> Result<bool, String> {
        if self.pointer_kind != PointerKind::Touch {
            return Ok(false);
        }
        let Some(binding) = self.pinch_binding(at) else {
            return Ok(false);
        };
        let Some(c) = self.contact.as_ref() else {
            return Ok(false);
        };
        if c.canvas.is_some()
            || c.group
            || c.panning
            || c.retained.is_some()
            || !self.contact_live(c)
        {
            return Ok(false);
        }
        let primary = first.1;
        if !c
            .hit
            .and_then(|key| self.host.kernel().node_by_key(key))
            .is_some_and(|node| !self.allows_touch_zoom(node.id, self.zoomable_from(node.id)))
        {
            return Ok(false);
        }
        if c.hold.as_ref().is_some_and(
            |h| !matches!(h.kind, HeldKind::Transform { binding: b, .. } if b == binding),
        ) {
            return Ok(false);
        }
        if !c
            .candidate
            .into_iter()
            .chain(c.rest.iter().copied())
            .any(|candidate| matches!(candidate,Candidate::Transform(b) if b == binding))
        {
            return Ok(false);
        }
        let origin = c.origin;
        let kept = c.touch;
        let revision = self.transform_revision(binding).unwrap();
        let clip = self.host.kernel().node_by_key(binding.clip).unwrap().id;
        let Some(frame) = self.pane_frame(clip) else {
            return Ok(false);
        };
        let Some(plane) = self.box_of(clip).and_then(|b| b.local_transform()) else {
            return Ok(false);
        };
        let under = self.scrollers_at(origin.0, origin.1);
        let scroll = [
            under
                .chain
                .iter()
                .find(|s| !kept.0 && s.axes.0)
                .map(|s| s.id),
            under
                .chain
                .iter()
                .find(|s| !kept.1 && s.axes.1)
                .map(|s| s.id),
        ];
        let c = self.contact.as_mut().unwrap();
        let values = c
            .hold
            .as_ref()
            .and_then(|h| match h.kind {
                HeldKind::Transform { pair, .. } => {
                    Some([pair.translate(), pair.scale()].map(|start| {
                        self.host
                            .engine()
                            .value(start.token.node(), start.token.property())
                            .unwrap_or(start.value)
                    }))
                }
                _ => None,
            })
            .unwrap_or([Value::ZERO, Value::new(1., 1.)]);
        let mut pinch = Pinch {
            first,
            second: Some((next, at)),
            binding,
            revision,
            began: event,
            plane,
            scroll,
            scrolling: None,
            starts: [primary, at],
            centre: (frame.width / 2., frame.height / 2.),
            anchor: (primary, 0.),
            values,
        };
        pinch.anchor = pinch.measure();
        c.pinch = Some(pinch);
        if let Some(key) = c.feedback.take() {
            self.press_feedback(key, Edge::Cancel, self.pointer_now());
        }
        self.clear_pending_sheet_drag();
        Ok(true)
    }

    /// Whether a pending or recognized pair owns these pointer IDs.
    pub fn tracking_transform_pinch(&self) -> bool {
        self.contact.as_ref().is_some_and(|c| c.pinch.is_some())
    }

    /// A finger in a paired hold moved, lifted or was cancelled. One finger left down pans
    /// from its own position. Only the last lift dispatches the release.
    pub fn transform_finger(
        &mut self,
        finger: Finger,
        companion: Option<Finger>,
        action: i32,
        now: f64,
        event: f64,
    ) -> Result<bool, String> {
        let Some(c) = self.contact.as_mut() else {
            return Ok(false);
        };
        let Some(mut p) = c.pinch else {
            return Ok(false);
        };
        let previous = p;
        let pending = c.hold.is_none();
        let (id, at) = finger;
        if let Some((other, point)) = companion {
            if other == p.first.0 {
                p.first.1 = point;
            } else if let Some(second) = p.second.as_mut().filter(|f| f.0 == other) {
                second.1 = point;
            }
        }
        if action == 0 {
            if p.second.is_none() && id != p.first.0 {
                self.begin_transform_pinch(p.first, id, at, event)?;
            }
            return Ok(true);
        }
        if id == p.first.0 {
            p.first.1 = at;
        } else if let Some(f) = p.second.as_mut().filter(|f| f.0 == id) {
            f.1 = at;
        } else {
            return Ok(true);
        }
        if action == 3 {
            self.pointer_released();
            self.pointer_cancel(now)?;
            return Ok(true);
        }
        let (focus, span) = p.measure();
        if pending {
            if p.second.is_some() && p.anchor.1 == 0. {
                p.anchor = (focus, span);
            }
            let (start, distance) = p.anchor;
            let dx = focus.0 - start.0;
            let dy = focus.1 - start.1;
            let travel = dx.hypot(dy) as f64;
            let spread = (span - distance).abs();
            c.pinch = Some(p);
            let axis = usize::from(dy.abs() >= dx.abs());
            let past_slop = travel > self.slop || spread > self.slop;
            // Windows reports the two positions separately. A single moving finger
            // on a scrollable axis can be the first half of either gesture.
            let ambiguous = action == 2
                && companion.is_none()
                && previous.first.1 == p.starts[0]
                && previous.second.is_some_and(|s| s.1 == p.starts[1])
                && p.scroll[axis].is_some()
                && p.second
                    .is_some_and(|s| (p.first.1 == p.starts[0]) != (s.1 == p.starts[1]));
            if p.scrolling.is_none() && (!past_slop || ambiguous) {
                if action == 1 {
                    if let Some(other) = p.second.take() {
                        if id == p.first.0 {
                            p.first = other;
                        }
                        p.anchor = p.measure();
                        p.began = event;
                        c.pinch = Some(p);
                    } else {
                        self.pointer_released();
                        self.pointer_cancel(now)?;
                    }
                }
                return Ok(true);
            }
            let parallel = p.second.is_none_or(|s| {
                let along = |point: Point| if axis == 0 { point.0 } else { point.1 };
                (along(p.first.1) - along(p.starts[0])) * (along(s.1) - along(p.starts[1])) > 0.
            });
            if p.scrolling.is_none()
                && travel > self.slop
                && (spread <= self.slop || parallel && 2. * travel >= spread)
            {
                p.scrolling = p.scroll[axis].map(|id| (id, axis));
            }
            if let Some((target, axis)) = p.scrolling {
                if self.zooms.shell_taps {
                    self.pointer_released();
                    self.pointer_cancel(now)?;
                } else {
                    let (x, y) = self.scroll_of(target);
                    self.scroll_to(target, if axis == 0 { (x - dx, y) } else { (x, y - dy) });
                    if action == 1 {
                        if let Some(other) = p.second.take() {
                            if id == p.first.0 {
                                p.first = other;
                            }
                        } else {
                            self.pointer_released();
                            self.pointer_cancel(now)?;
                            return Ok(true);
                        }
                    }
                    p.anchor = p.measure();
                    self.contact.as_mut().unwrap().pinch = Some(p);
                }
                return Ok(true);
            }
            if self.transform_revision(p.binding) != Some(p.revision) {
                self.pointer_released();
                self.pointer_cancel(now)?;
                return Ok(true);
            }
            let Some(held) = self.begin_transform_drag(p.binding, now, p.began)? else {
                self.pointer_released();
                self.pointer_cancel(now)?;
                return Ok(true);
            };
            let HeldKind::Transform { pair, .. } = held.kind else {
                unreachable!()
            };
            p.values = [pair.translate().value, pair.scale().value];
            let c = self.contact.as_mut().unwrap();
            c.hold = Some(held);
            c.candidate = Some(Candidate::Transform(p.binding));
            c.rest.clear();
        }
        let c = self.contact.as_mut().unwrap();
        if action == 1 && p.second.is_some() {
            // Membership changes the centroid. Only motion at the lift adds a sample.
            let moved = pending || previous.position() != p.position();
            p.values = p.position();
            let other = p.second.take().unwrap();
            if id == p.first.0 {
                p.first = other;
            }
            p.anchor = p.measure();
            c.position = p.first.1;
            c.pinch = Some(p);
            if moved {
                let (x, y) = p.first.1;
                self.pointer_move_at(x, y, now, event)?;
            }
        } else {
            c.pinch = Some(p);
            if action == 1 {
                self.pointer_up_at(focus.0, focus.1, now, event)?;
            } else if p.second.is_none() || span > 0. {
                self.pointer_move_at(focus.0, focus.1, now, event)?;
            }
        }
        Ok(true)
    }
}
