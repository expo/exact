//! What a finger and predictive Back do to the stage.
//!
//! **A sheet's drag.** A press on the scrim dismisses the sheet unless its
//! route says `closedby="none"`. A contact on the sheet where nothing under
//! it scrolls up and down (its handle, a header, a form's rows) presses
//! what it is on until it has moved Android's 8-point slop vertically;
//! past that the sheet follows the finger, what it pressed is let go, and
//! on release it settles on the detent its speed carries it to, or leaves
//! the window down, which is the route's Back. Where a scroller is under
//! the finger the scroller has the drag.
//!
//! **Predictive Back** (Android 14 and later, `OnBackAnimationCallback`):
//! the shell passes the gesture's start, progress and cancel. A sheet that
//! Back would close shrinks toward 90 % about its foot; a route Back would
//! pop shrinks to 90 % and shifts toward the edge the swipe came from, by a
//! twentieth of the width less 8, with its corners rounding, the route it
//! returns to drawn beneath. A cancel puts it back; the commit is Back
//! itself, whose pop starts from where the preview left the route. Nothing
//! commits before the gesture does.
use super::*;

/// A slop of Android's (`ViewConfiguration`'s touch slop, 8 dp).
const SLOP: f32 = 8.0;
/// A release this fast, points a second, goes on to the next detent.
const FLING: f32 = 500.0;
/// How long a cancelled preview takes to come back.
const RETURN_MS: f64 = 150.0;

#[derive(Debug, Clone, Copy)]
pub(in crate::presenter) enum Gesture {
    /// A contact on a sheet's scrim.
    Scrim { modal: NodeKey, contact: Contact },
    /// A contact on a sheet that moves it once past the slop.
    Drag {
        modal: NodeKey,
        finger: f32,
        start: f32,
        top: f32,
        last: (f32, f64),
        velocity: f32,
        moving: bool,
        contact: Contact,
    },
}

/// Where a contact the stage holds is and when it was last heard, as a
/// driver reads a contact (`agent/contact.rs`).
#[derive(Debug, Clone, Copy)]
pub(in crate::presenter) struct Contact {
    at: (f32, f32),
    event_ms: f64,
}

impl Gesture {
    fn contact(&mut self) -> &mut Contact {
        match self {
            Gesture::Scrim { contact, .. } | Gesture::Drag { contact, .. } => contact,
        }
    }
}

/// What a predictive Back previews.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Target {
    Sheet(NodeKey),
    /// A pop: the route that leaves, the one beneath it, and the sheet
    /// both are in.
    Page {
        outgoing: NodeKey,
        incoming: NodeKey,
        sheet: Option<NodeKey>,
    },
}

#[derive(Debug, Clone, Copy)]
pub(in crate::presenter) struct Preview {
    target: Target,
    progress: f32,
    /// The edge the swipe started from: 0 the left, 1 the right.
    edge: i32,
    /// Going back to rest, once cancelled.
    release: Option<Glide>,
}

impl Preview {
    fn shown(&self, now: f64) -> f32 {
        self.release
            .map_or(self.progress, |g| g.at(now))
            .clamp(0.0, 1.0)
    }

    /// Whether frames are owed: until a cancelled preview is back at rest
    /// and the stage has let it go (`sync_stage`).
    pub(super) fn moving(&self) -> bool {
        self.release.is_some()
    }

    /// A cancelled preview back at rest.
    pub(super) fn returned(&self, now: f64) -> bool {
        self.release.is_some_and(|g| !g.moving(now))
    }

    pub(super) fn ends(&self) -> Option<f64> {
        self.release.and_then(|g| g.ends())
    }

    /// Whether the routes it previews are still where it found them.
    pub(super) fn live(&self, keys: &[NodeKey]) -> bool {
        match self.target {
            Target::Sheet(modal) => keys.contains(&modal),
            Target::Page { outgoing, .. } => keys.last() == Some(&outgoing),
        }
    }

    /// How far a sheet is shrunk.
    pub(super) fn sheet_scale(&self, modal: NodeKey) -> f32 {
        match self.target {
            Target::Sheet(m) if m == modal => 1.0 - 0.1 * self.progress,
            _ => 1.0,
        }
    }

    /// The pair a pop's preview draws.
    pub(super) fn pair(&self) -> Option<(NodeKey, NodeKey)> {
        match self.target {
            Target::Page {
                outgoing, incoming, ..
            } => Some((outgoing, incoming)),
            Target::Sheet(_) => None,
        }
    }

    pub(super) fn page_pair(&self) -> Option<(NodeKey, NodeKey)> {
        match self.target {
            Target::Page { sheet: None, .. } => self.pair(),
            _ => None,
        }
    }

    pub(super) fn inner_pair(&self, modal: NodeKey) -> Option<(NodeKey, NodeKey)> {
        match self.target {
            Target::Page { sheet: Some(s), .. } if s == modal => self.pair(),
            _ => None,
        }
    }

    /// The leaving route's pose at the preview's progress, in a box
    /// `width` by `height`.
    pub(super) fn page_pose(&self, now: f64, width: f32, height: f32) -> Pose {
        self.pose_at(self.shown(now), width, height)
    }

    fn pose_at(&self, p: f32, width: f32, height: f32) -> Pose {
        let way = if self.edge == 1 { -1.0 } else { 1.0 };
        Pose {
            dx: way * p * (width / 20.0 - 8.0).max(0.0),
            dy: 0.0,
            scale: 1.0 - 0.1 * p,
            origin: (width / 2.0, height / 2.0),
            opacity: 1.0,
            radius: 28.0 * p,
        }
    }

    /// Where it left `key`, the route that leaves, when Back committed, in
    /// a box `size` large.
    pub(super) fn pose_for(&self, key: NodeKey, size: (f32, f32)) -> Option<Pose> {
        match self.target {
            Target::Page { outgoing, .. } if outgoing == key => {
                Some(self.pose_at(self.progress, size.0, size.1))
            }
            _ => None,
        }
    }
}

impl<D: DataSource> Presenter<D> {
    /// The top sheet, with its frame now.
    fn grab_sheet(&self) -> Option<(NodeKey, f32)> {
        let sheet = self
            .front_sheets()
            .last()
            .filter(|_| self.stage_presents())?;
        Some((sheet.modal, self.sheet_top_now(sheet)))
    }

    /// Whether a vertical scroller inside the top sheet is under a point.
    fn sheet_scrolls_at(&mut self, x: f32, y: f32) -> bool {
        self.scrollers_at(x, y)
            .chain
            .iter()
            .any(|s| s.id != super::super::PAGE && s.axes.1 && s.max.1 > 0.0)
    }

    /// A contact down: one on a scrim is the stage's; one on a sheet may
    /// become its drag.
    pub(in crate::presenter) fn stage_down(&mut self, x: f32, y: f32, now: f64) -> bool {
        let Some((modal, top)) = self.grab_sheet() else {
            return false;
        };
        let (left, width) = self.sheet_span();
        let contact = Contact {
            at: (x, y),
            event_ms: now,
        };
        if y < top || x < left || x >= left + width {
            self.stage.gesture = Some(Gesture::Scrim { modal, contact });
            return true;
        }
        let handle = y < top + SHEET_HANDLE;
        self.stage.gesture = (handle || !self.sheet_scrolls_at(x, y)).then_some(Gesture::Drag {
            modal,
            finger: y,
            start: top,
            top,
            last: (y, now),
            velocity: 0.0,
            moving: false,
            contact,
        });
        false
    }

    /// The contact the stage holds where no other is down (a press on a
    /// scrim, a sheet's drag past the slop): where it is and when it was
    /// last heard.
    pub(in crate::presenter) fn stage_contact(&self) -> Option<((f32, f32), f64)> {
        match self.stage.gesture? {
            Gesture::Scrim { contact, .. } | Gesture::Drag { contact, .. } => {
                Some((contact.at, contact.event_ms))
            }
        }
    }

    /// Whether a contact down may yet drag a sheet.
    pub(in crate::presenter) fn stage_drag_pending(&self) -> bool {
        matches!(self.stage.gesture, Some(Gesture::Drag { .. }))
    }

    /// A driver held the contact still for `ms`.
    pub(in crate::presenter) fn stage_delay_contact(&mut self, ms: f64) {
        if let Some(gesture) = &mut self.stage.gesture {
            gesture.contact().event_ms += ms;
        }
    }

    pub(in crate::presenter) fn stage_move(&mut self, x: f32, y: f32, now: f64) -> bool {
        if let Some(gesture) = &mut self.stage.gesture {
            *gesture.contact() = Contact {
                at: (x, y),
                event_ms: now,
            };
        }
        let h = self.viewport.1;
        let Some(highest) =
            (self.front_sheets().last()).map(|s| h - s.heights[s.heights.len() - 1])
        else {
            return false;
        };
        match self.stage.gesture {
            Some(Gesture::Scrim { .. }) => return true,
            Some(Gesture::Drag {
                finger,
                moving: false,
                ..
            }) => {
                if (y - finger).abs() <= SLOP {
                    return false;
                }
                // Past the slop the drag is the sheet's: what the contact
                // pressed is let go.
                let gesture = self.stage.gesture.take();
                let _ = self.pointer_cancel(now);
                self.stage.gesture = gesture;
                if let Some(Gesture::Drag { moving, .. }) = &mut self.stage.gesture {
                    *moving = true;
                }
            }
            Some(Gesture::Drag { .. }) => {}
            None => return false,
        }
        if let Some(Gesture::Drag {
            finger,
            start,
            top,
            last,
            velocity,
            ..
        }) = &mut self.stage.gesture
        {
            *top = (*start + y - *finger).clamp(highest, h);
            let dt = (now - last.1) as f32 / 1000.0;
            if dt > 0.0 {
                let v = (y - last.0) / dt;
                *velocity = if dt > 0.1 {
                    v
                } else {
                    0.6 * v + 0.4 * *velocity
                };
                *last = (y, now);
            }
            self.dirty = true;
        }
        true
    }

    /// A contact up: a press on the scrim dismisses; a drag settles on the
    /// detent its speed carries it to, or out of the window.
    pub(in crate::presenter) fn stage_up(&mut self, x: f32, y: f32, now: f64, event: f64) -> bool {
        let Some(mut gesture) = self.stage.gesture.take() else {
            return false;
        };
        if let Gesture::Drag {
            moving: true,
            last,
            velocity,
            ..
        } = &mut gesture
        {
            // The release's own sample counts toward the speed.
            let dt = (event - last.1) as f32 / 1000.0;
            if dt > 0.0 && dt <= 0.1 {
                *velocity = 0.6 * ((y - last.0) / dt) + 0.4 * *velocity;
            }
        }
        match gesture {
            Gesture::Scrim { modal, .. } => {
                let closes = (self.host.kernel().node_by_key(modal))
                    .is_some_and(|n| n.props.str(PropId::Closedby) != Some("none"));
                let outside = self.grab_sheet().is_some_and(|(_, top)| {
                    let (left, width) = self.sheet_span();
                    y < top || x < left || x >= left + width
                });
                if closes && outside {
                    self.dismiss_sheet(modal, now);
                }
                true
            }
            Gesture::Drag { moving: false, .. } => false,
            Gesture::Drag {
                modal,
                top,
                velocity,
                ..
            } => {
                self.settle_sheet(modal, top, velocity, now);
                true
            }
        }
    }

    /// A down lets go of any contact before it, but keeps the drag
    /// `stage_down` began for this one.
    pub(in crate::presenter) fn cancel_before_down(&mut self, now: f64) -> Result<(), String> {
        let gesture = self.stage.gesture.take();
        let result = self.pointer_cancel(now);
        self.stage.gesture = gesture;
        result
    }

    pub(in crate::presenter) fn stage_cancel(&mut self, now: f64) {
        // A drag still inside the slop is dropped like a scrim press: left,
        // a later move without a contact would start it.
        if let Some(Gesture::Drag {
            modal,
            top,
            moving: true,
            ..
        }) = self.stage.gesture.take()
        {
            self.settle_sheet(modal, top, 0.0, now);
        }
    }

    /// A sheet let go at `top` moving at `velocity`: it rests on a detent,
    /// or, past its lowest toward the window's edge, goes Back.
    fn settle_sheet(&mut self, modal: NodeKey, top: f32, velocity: f32, now: f64) {
        let h = self.viewport.1;
        let still = self.motion_off();
        let closes = (self.host.kernel().node_by_key(modal))
            .is_some_and(|n| n.props.str(PropId::Closedby) != Some("none"));
        let Some(sheet) = self.stage.sheets.iter_mut().find(|s| s.modal == modal) else {
            return;
        };
        let mut rests: Vec<f32> = sheet.heights.iter().map(|d| h - d).collect();
        if closes {
            rests.push(h);
        }
        rests.sort_by(f32::total_cmp);
        let nearest = || {
            (rests.iter().copied())
                .min_by(|a, b| (a - top).abs().total_cmp(&(b - top).abs()))
                .unwrap_or(h)
        };
        let target = if velocity > FLING {
            rests
                .iter()
                .copied()
                .find(|r| *r > top + 1.0)
                .unwrap_or_else(nearest)
        } else if velocity < -FLING {
            (rests.iter().rev().copied())
                .find(|r| *r < top - 1.0)
                .unwrap_or(rests[0])
        } else {
            nearest()
        };
        if closes && target >= h - 0.5 {
            sheet.top = Glide::still(top);
            self.dismiss_sheet(modal, now);
            return;
        }
        let ms = if still { 0.0 } else { SETTLE_MS };
        sheet.top = Glide::new(top, target, now, ms, Curve::Decelerate);
        if let Some(i) = sheet
            .heights
            .iter()
            .position(|d| (h - d - target).abs() < 0.5)
        {
            sheet.at = i;
        }
        self.dirty = true;
    }

    /// The person dismissed the sheet `modal` opened (a press on its scrim,
    /// a drag out of the window): it closes whole, with what was pushed
    /// inside it, as a sheet swiped down on iOS does. A root that hears
    /// `navigate` is sent to the location beneath the sheet; else Back goes
    /// once for each route the sheet holds, each in Back's order (D6).
    fn dismiss_sheet(&mut self, modal: NodeKey, now: f64) {
        let kernel = self.host.kernel();
        let visit = (kernel.node_by_key(modal))
            .and_then(|n| n.props.str(PropId::NavigationKey)?.parse::<u64>().ok());
        let routes = (self.stage.seen.iter())
            .skip_while(|key| **key != modal)
            .count();
        let root = self.navigation_root();
        let hears = root.is_some_and(|r| {
            (self.host.runner().handlers_of(r)).contains(&exact_plan::EventKind::Navigate)
        });
        let beneath = visit.and_then(|v| self.host.runner().location_beneath(v));
        if let (true, Some(root), Some(location)) = (routes > 1 && hears, root, beneath) {
            let error = (self.host)
                .dispatch_at(root, Event::Navigate(location), now)
                .or(self.after_commit());
            if let Some(error) = error {
                self.host.log(error);
            }
            return;
        }
        for _ in 0..routes.max(1) {
            if !self.stage.seen.contains(&modal) || !self.system_back(now) {
                break;
            }
        }
    }

    /// Whether a contact on the stage claims up-and-down drags at a point
    /// (`scrollers.rs`): `None` where the stage has nothing there; else the
    /// claims, and whether the scrollers under the point are the sheet's
    /// own (a scrim has none).
    pub(in crate::presenter) fn stage_claims(&self, x: f32, y: f32) -> Option<bool> {
        let (_, top) = self.grab_sheet()?;
        let (left, width) = self.sheet_span();
        Some(y >= top && x >= left && x < left + width)
    }

    /// Predictive Back (`OnBackAnimationCallback`): `phase` 0 the gesture
    /// started, 1 it moved, 2 it was cancelled; `progress` 0 to 1 as the
    /// system eases it; `edge` 0 from the left, 1 from the right. Whether
    /// the stage previews it: a sheet Back would close, or a route Back
    /// would pop. The commit is [`Presenter::system_back`].
    pub fn back_progress(&mut self, phase: i32, progress: f32, edge: i32, now_ms: f64) -> bool {
        let progress = progress.clamp(0.0, 1.0);
        match phase {
            0 => {
                let target = self.preview_target().filter(|_| self.back_previewable());
                self.stage.preview = target.map(|target| Preview {
                    target,
                    progress,
                    edge,
                    release: None,
                });
            }
            1 => {
                if let Some(p) = &mut self.stage.preview {
                    p.progress = progress;
                    p.release = None;
                }
            }
            _ => {
                let still = self.motion_off();
                if let Some(p) = &mut self.stage.preview {
                    let ms = if still { 0.0 } else { RETURN_MS };
                    p.release = Some(Glide::new(p.progress, 0.0, now_ms, ms, Curve::Decelerate));
                    p.progress = 0.0;
                }
            }
        }
        self.dirty = true;
        if self
            .stage
            .preview
            .as_ref()
            .is_some_and(|p| p.release.is_some_and(|g| !g.moving(now_ms)))
        {
            self.stage.preview = None;
        }
        self.stage.preview.is_some()
    }

    /// Back went while a preview showed: where it did not take the route
    /// away (a handler kept it), the preview goes back to rest.
    pub(in crate::presenter) fn stage_back_done(&mut self, now: f64) {
        let still = self.motion_off();
        if let Some(p) = self.stage.preview.as_mut().filter(|p| p.release.is_none()) {
            let ms = if still { 0.0 } else { RETURN_MS };
            p.release = Some(Glide::new(p.progress, 0.0, now, ms, Curve::Decelerate));
            p.progress = 0.0;
            self.dirty = true;
        }
    }

    /// What Back would close or pop now, where the stage can preview it:
    /// the top route, a sheet or one pushed, when Back goes to the route
    /// (`back_previewable`); nothing while routes cross.
    fn preview_target(&self) -> Option<Target> {
        if !self.stage_presents() || self.motion_off() || self.stage.shift.is_some() {
            return None;
        }
        let kernel = self.host.kernel();
        let stack = self.host.shown_stack()?;
        let keys: Vec<NodeKey> = (stack.iter())
            .filter_map(|id| kernel.node(*id).map(|n| n.key))
            .collect();
        let kinds: Vec<Presented> = (stack.iter())
            .map(|id| {
                kernel
                    .node(*id)
                    .map_or(Presented::Pushed, |n| Presented::of(&n))
            })
            .collect();
        let top = keys.len().checked_sub(1)?;
        match kinds[top] {
            Presented::Sheet => Some(Target::Sheet(keys[top])),
            Presented::Pushed if top > 0 => {
                let sheet = sheet_of(&kinds, top);
                Some(Target::Page {
                    outgoing: keys[top],
                    incoming: keys[top - 1],
                    sheet: sheet.map(|i| keys[i]),
                })
            }
            _ => None,
        }
    }
}
