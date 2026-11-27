//! Drags from outside the app over the displayed tree: files, plain text and
//! a link (LLP 1069.010 D1, LLP 1094.001 D1).
use super::*;
use exact_kernel::NodeKey;
use exact_plan::EventKind;
use exact_runner::{DropEvent, KeyModifiers};
use std::path::PathBuf;

/// A drag's edge band (points), its speed (points a second) and the most
/// time one step catches up (seconds), as the reorder's (LLP 1094 D7,
/// `group.rs`), for a drag held near a scroller's edge (LLP 1094.000 D15).
const EDGE_BAND: f32 = 32.;
const EDGE_SPEED: f32 = 720.;
const EDGE_CATCH_UP: f64 = 0.032;

/// What a drag drops: files by path, minted as `doc:` handles when they are
/// delivered, its plain text and its first link that is not a file, each
/// empty when it carries none.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct Dropped {
    /// The dropped files, in the order the platform gave them.
    pub paths: Vec<PathBuf>,
    /// The drag's plain text.
    pub text: String,
    /// The drag's first link that is not a file.
    pub url: String,
}

#[derive(Default)]
pub(super) struct DragIn {
    /// The nodes under the drag that hear `dragenter` or `dragleave`,
    /// innermost first.
    chain: Vec<Member>,
    /// Whether a drag is over the window at `point`, even where no node
    /// hears drags.
    over: bool,
    point: (f32, f32),
    held: KeyModifiers,
    /// The drops released whose items have yet to arrive (`drop_released`).
    pending: Vec<Pending>,
    /// The enters and leaves decided and not yet sent, in order. Each is
    /// recorded before any handler runs, and sent only while its node is
    /// still the one it was decided for (`drag_renewed`) and, for an enter,
    /// still hears drags.
    outbox: std::collections::VecDeque<Sent>,
    /// The last pending drop's number, kept across a new plan so that no
    /// copy that arrives late lands on a drop of the new one.
    last: u32,
    /// While a drop is released behind earlier ones (`drop_released_behind`),
    /// the newest of them: what is queued meanwhile waits for them to land.
    behind: Option<u32>,
    /// Whether the drag is the platform's, which the host reports: only it
    /// scrolls a scroller it is held near (LLP 1094.000 D15), not the
    /// agent's `dragenter`, which holds still until the next step.
    native: bool,
    /// The scroller the drag scrolls where it was last found, its axis and
    /// its speed, and the time of its last step.
    edge: Option<(ViewId, bool, f32)>,
    edge_clock: Option<f64>,
    /// A step moved a scroller: the drag is found again on the next frame,
    /// once the picture shows the move.
    refind: bool,
    /// A commit landed while the drag was over the window: the drag is
    /// found again once the painted picture shows the kernel at this epoch.
    committed: Option<u64>,
    /// The lifts under way, heard at their source (LLP 1094.000 D16), and
    /// the last lift's number, kept across a new plan as `last` is.
    pub(super) lifting: Vec<super::drag_source::Lifting>,
    pub(super) lifts: u32,
}

/// A node under the drag, and whether it heard the drag's `dragenter`: one
/// a pending drop is over has not, until that drop lands or this drag drops
/// (LLP 1094.001 D2).
#[derive(Clone, Copy, PartialEq)]
struct Member {
    key: NodeKey,
    entered: bool,
}

/// An enter or a leave to send, with its record's point and modifiers, for
/// the drag under way (`drop` none) or for pending drop `drop`; held, with
/// what is queued after it, while a pending drop numbered `after` or lower
/// has not landed.
struct Sent {
    key: NodeKey,
    kind: EventKind,
    point: (f32, f32),
    held: KeyModifiers,
    drop: Option<u32>,
    after: Option<u32>,
}

impl DragIn {
    /// A new plan's tracker: its views are new, so no drop or drag is over
    /// them yet.
    pub(super) fn reset(&mut self) {
        *self = DragIn {
            last: self.last,
            lifts: self.lifts,
            ..Default::default()
        };
    }

    fn send(
        &mut self,
        key: NodeKey,
        kind: EventKind,
        at: ((f32, f32), KeyModifiers),
        drop: Option<u32>,
    ) {
        let (point, held) = at;
        self.outbox.push_back(Sent {
            key,
            kind,
            point,
            held,
            drop,
            after: self.behind,
        });
    }

    /// `key` heard no enter of the drag under way, or of pending drop
    /// `drop`, after all, so it hears no leave of that one either; another's
    /// is still owed.
    fn unentered(&mut self, key: NodeKey, drop: Option<u32>) {
        let chain = match drop {
            None => Some(&mut self.chain),
            Some(id) => self
                .pending
                .iter_mut()
                .find(|p| p.id == id)
                .map(|p| &mut p.chain),
        };
        for m in chain.into_iter().flatten().filter(|m| m.key == key) {
            m.entered = false;
        }
        self.outbox
            .retain(|s| !(s.key == key && s.drop == drop && s.kind == EventKind::Dragleave));
    }
}

/// Whether `sent` waits for a drop in `pending` to land.
fn waits(pending: &[Pending], sent: &Sent) -> bool {
    sent.after
        .is_some_and(|after| pending.iter().any(|p| p.id <= after))
}

/// A drop released before its items arrived: its target (none once the
/// target is a new mount, LLP 1078), its record as at the release, and the
/// chain it was over, which hears a leave if the drop is not delivered.
struct Pending {
    id: u32,
    target: Option<NodeKey>,
    offsets: (f64, f64),
    point: (f32, f32),
    held: KeyModifiers,
    chain: Vec<Member>,
}

impl<D: DataSource> Presenter<D> {
    /// The drag is at `point`: the nodes under it hear their enters and
    /// leaves, then the innermost node that takes a drop there, if any, is
    /// what a host shows its copy feedback for.
    pub fn drag_over(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_in.native = true;
        self.drag_hover(point, held)
    }

    /// `drag_over` for a drag the agent makes, which scrolls nothing.
    fn drag_hover(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_at(point, held);
        self.drag_flush();
        let hit = self.drag_hit(point);
        self.drop_target(hit)
    }

    /// The scroller the drag scrolls, its axis and its speed, while it is
    /// over the window (LLP 1094.000 D15).
    pub(super) fn drag_edge(&self) -> Option<(ViewId, bool, f32)> {
        self.drag_in.edge.filter(|_| self.drag_in.over)
    }

    /// Whether the drag is held near the edge of a scroller it scrolls, as
    /// Windows marks it (`DROPEFFECT_SCROLL`).
    pub fn drag_scrolls(&self) -> bool {
        self.drag_edge().is_some()
    }

    /// Whether a drag needs a frame: it is held near an edge, or the last
    /// step's move is yet to be found under it.
    pub(super) fn drag_needs_frame(&self) -> bool {
        self.drag_in.over
            && (self.drag_in.edge.is_some()
                || self.drag_in.refind
                || self.drag_in.committed.is_some())
    }

    /// A frame of a drag over the window (LLP 1094.000 D15). After a step,
    /// the drag is found again at its point on the picture that shows the
    /// move, so the nodes there hear their enters and leaves and the host
    /// learns what takes it; then the scroller it is near moves on at
    /// `EDGE_SPEED`, by at most `EDGE_CATCH_UP` since the last step.
    pub(super) fn tick_drag_edge(&mut self, now: f64) {
        if !self.drag_in.over {
            return;
        }
        let (point, held) = (self.drag_in.point, self.drag_in.held);
        // A commit's picture may not be shown yet: while one is on its way, the request to look again stays; with
        // none coming (a paint that failed), the drag is found again on the picture there is, rather than waiting
        let shown = self.drag_in.committed.is_some_and(|epoch| {
            self.hit_epoch().is_some_and(|shown| shown >= epoch) || !self.picture_coming()
        });
        if shown {
            self.drag_in.committed = None;
        }
        if std::mem::take(&mut self.drag_in.refind) || shown {
            self.drag_at(point, held);
            self.drag_flush();
        }
        // What the drag may scroll is found on this frame's picture, after any enter it heard ran: a commit, or a
        // handler that made the scroller inert or took the last listener away, may have changed it
        if self.drag_in.native {
            self.drag_in.edge = self.drag_edge_at(point);
        }
        let Some((id, vertical, speed)) = self.drag_edge() else {
            self.drag_in.edge_clock = None;
            return;
        };
        let dt = self.drag_in.edge_clock.replace(now).map_or(0., |last| {
            ((now - last) / 1000.).clamp(0., EDGE_CATCH_UP) as f32
        });
        let limits = self.collection_scroll_limits();
        let Some(s) = self.scroller(id, &limits) else {
            self.drag_in.edge = None;
            return;
        };
        let by = if vertical {
            (0., speed * dt)
        } else {
            (speed * dt, 0.)
        };
        if by == (0., 0.) {
            return;
        }
        let to = (s.offset.0 + by.0, s.offset.1 + by.1);
        self.restore_scroll_anchors();
        let moved =
            self.scroll_to_completion(id, to, super::scroll_end::Completion::DragEdge, Some(by));
        self.arm_scroll_anchors();
        if moved.is_some_and(|(_, movement)| movement != (0., 0.)) {
            self.publish_scroll();
            self.drag_in.refind = true;
        }
    }

    /// A commit landed: a platform's drag over the window is found again on
    /// the next frame, since what it may scroll, and the nodes under it, may
    /// have changed though it did not move (a timer that mounts the first
    /// `drop`, a scroller that grows under it).
    pub(super) fn drag_committed(&mut self) {
        if self.drag_in.over && self.drag_in.native {
            self.drag_in.committed = Some(self.host.kernel().epoch());
        }
    }

    /// The drag is over for the window: what it scrolled stops, and a later
    /// drag starts its clock again.
    fn drag_edge_ended(&mut self) {
        self.drag_in.native = false;
        self.drag_in.edge = None;
        self.drag_in.edge_clock = None;
        self.drag_in.refind = false;
        self.drag_in.committed = None;
    }

    /// The scroller a drag at `point` scrolls (LLP 1094.000 D15): of the
    /// scrollers a reader's scroll there would move, innermost first and the
    /// page last (`scrollers_at`, which leaves out an axis an inner
    /// scroller's `overscroll-behavior` keeps and the page under an open
    /// surface), the first whose port, as far as it shows, holds the point
    /// within `EDGE_BAND` of an edge it can still move toward, vertically
    /// first, the nearer edge where the two bands meet; one a route covers,
    /// and a field, are passed over. None unless a node in the window hears
    /// `drop`, `dragenter` or `dragleave`.
    fn drag_edge_at(&mut self, point: (f32, f32)) -> Option<(ViewId, bool, f32)> {
        let kinds = [EventKind::Drop, EventKind::Dragenter, EventKind::Dragleave];
        if !self.host.runner().hears_any(&kinds) {
            return None;
        }
        let under = self.scrollers_at(point.0, point.1);
        under.chain.into_iter().find_map(|s| {
            // A field takes no drag on these hosts (LLP 1094.001 D2), so its text is no scroller here
            if s.id != super::scrollers::PAGE
                && (self.host.route_visibility(s.id).1 || self.brush.field_text.contains_key(&s.id))
            {
                return None;
            }
            let [x, y, w, h] = s.port;
            if point.0 < x || point.0 > x + w || point.1 < y || point.1 > y + h {
                return None;
            }
            // (a right-to-left scroller's offsets run from a negative minimum up to zero)
            [
                (
                    true,
                    s.axes.1,
                    point.1 - y,
                    y + h - point.1,
                    s.offset.1,
                    s.min.1,
                    s.max.1,
                ),
                (
                    false,
                    s.axes.0,
                    point.0 - x,
                    x + w - point.0,
                    s.offset.0,
                    s.min.0,
                    s.max.0,
                ),
            ]
            .into_iter()
            .filter(|axis| axis.1)
            .find_map(|(vertical, _, from_start, to_end, offset, min, max)| {
                // The nearer edge's band, then whether the scroller can move that way: a short port held near both
                // edges does not turn back at one
                let toward_end = match (from_start < EDGE_BAND, to_end < EDGE_BAND) {
                    (true, true) => to_end < from_start,
                    (back, on) if back || on => on,
                    _ => return None,
                };
                let travels = if toward_end {
                    offset < max
                } else {
                    offset > min
                };
                travels.then_some((
                    s.id,
                    vertical,
                    if toward_end { EDGE_SPEED } else { -EDGE_SPEED },
                ))
            })
        })
    }

    /// Cancel the drag, including when it leaves the window: the nodes under
    /// it hear their leave, and the enters it waited for are not sent.
    pub fn drag_left(&mut self) {
        self.drag_in.over = false;
        self.drag_edge_ended();
        self.drag_crossings(Vec::new());
        self.drag_flush();
    }

    /// The drag dropped at `point` where no node takes it, or a field took
    /// it: the nodes under it there hear the enters it waited for, then
    /// every one hears its leave (LLP 1094.001 D2).
    pub fn drag_dropped(&mut self, point: (f32, f32), held: KeyModifiers) {
        self.drag_at(point, held);
        self.drag_ended();
        self.drag_flush();
    }

    /// Deliver what a drag drops once, its files in platform order, to the
    /// innermost node that takes a drop, a field's included: Exact's fields
    /// take no drag on this host (LLP 1094.001 D2). None, and a leave, when
    /// nothing takes it or nothing it carried arrived.
    pub fn dropped(
        &mut self,
        point: (f32, f32),
        held: KeyModifiers,
        dropped: &Dropped,
    ) -> Option<ViewId> {
        let id = self.drop_released(point, held)?;
        self.drop_landed(id, dropped)
    }

    /// Release the drag at `point` while the drops pending now are about to
    /// land, as a second drop on Linux ends their wait (LLP 1094.001 D8):
    /// this drop's target, record and chain are taken now, as
    /// `drop_released` takes them, but its enters, and all that is queued
    /// after them, wait for those drops to land, so their actions cannot
    /// clear what this drag drew.
    pub fn drop_released_behind(
        &mut self,
        point: (f32, f32),
        held: KeyModifiers,
        lift: Option<u32>,
    ) -> Option<u32> {
        self.drag_in.behind = self.drag_in.pending.iter().map(|p| p.id).max();
        let id = self.drop_released_lift(point, held, lift);
        self.drag_in.behind = None;
        id
    }

    /// Release the drag at `point` before its items arrive, as Android's
    /// copies do (LLP 1094.001 D2): the innermost node that takes a drop
    /// there, its record and the chain under the drag become the drop's,
    /// which `drop_landed` delivers, and the enters this drag waited for are
    /// heard. All of it is decided before any handler runs. The drop's
    /// number, or None, and the drag ends, when nothing takes it.
    pub fn drop_released(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<u32> {
        self.drop_released_lift(point, held, None)
    }

    /// [`Presenter::drop_released`] of a drag the host's drop side knows as
    /// the app's own lift `lift` (LLP 1094.000 D16), whose end then waits
    /// for this drop to land.
    pub fn drop_released_lift(
        &mut self,
        point: (f32, f32),
        held: KeyModifiers,
        lift: Option<u32>,
    ) -> Option<u32> {
        let hit = self.drag_at(point, held);
        let found = self.drop_target(hit).and_then(|t| {
            let key = self.host.kernel().node(t)?.key;
            let record = self.pointer_record(t, point.0, point.1, -1, 0)?;
            Some((key, (record.offset_x, record.offset_y)))
        });
        let Some((target, offsets)) = found else {
            self.drag_ended();
            self.drag_flush();
            return None;
        };
        self.drag_in.over = false;
        self.drag_edge_ended();
        let mut chain = std::mem::take(&mut self.drag_in.chain);
        self.drag_in.last += 1;
        let id = self.drag_in.last;
        // The release's own enters are this drop's now, as its chain is
        for sent in self.drag_in.outbox.iter_mut() {
            if sent.drop.is_none() && chain.iter().any(|m| m.key == sent.key) {
                sent.drop = Some(id);
            }
        }
        for m in chain.iter_mut().filter(|m| !m.entered) {
            m.entered = true;
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), Some(id));
        }
        let target = Some(target);
        self.lift_drops(id, lift);
        self.drag_in.pending.push(Pending {
            id,
            target,
            offsets,
            point,
            held,
            chain,
        });
        self.drag_flush();
        Some(id)
    }

    /// The items of pending drop `id` arrived: its files, minted as `doc:`
    /// handles, its text and its link go to its target if that is still the
    /// same node and still takes a drop, which ends the drag with no leave;
    /// otherwise, or when nothing arrived, the chain it was over hears a
    /// leave. A drag still under way then hears the enters it waited for at
    /// the nodes under it now. The target, when delivered.
    pub fn drop_landed(&mut self, id: u32, dropped: &Dropped) -> Option<ViewId> {
        self.landed(id, dropped, |paths| {
            paths
                .iter()
                .filter(|path| path.is_absolute() && path.exists())
                .filter_map(|path| exact_data::documents::mint(path, 0))
                .collect()
        })
    }

    /// [`Presenter::drop_landed`] for files the host checked off this
    /// thread, each with its folder held (a Flatpak sandbox's, LLP 1094.001
    /// D8): `checked` are the files, minted with no further reading of the
    /// file system, and `dropped`'s paths are not read.
    #[cfg(target_os = "linux")]
    pub fn drop_landed_checked(
        &mut self,
        id: u32,
        dropped: &Dropped,
        checked: Vec<exact_data::documents::Checked>,
    ) -> Option<ViewId> {
        self.landed(id, dropped, |_| {
            checked
                .into_iter()
                .filter_map(|checked| exact_data::documents::mint_checked(checked, 0))
                .collect()
        })
    }

    /// Land drop `id`, its files the handles `mint` gives for its paths, when
    /// it has a target.
    fn landed(
        &mut self,
        id: u32,
        dropped: &Dropped,
        mint: impl FnOnce(&[PathBuf]) -> Vec<String>,
    ) -> Option<ViewId> {
        let at = self.drag_in.pending.iter().position(|p| p.id == id)?;
        let pending = self.drag_in.pending.remove(at);
        let target = pending
            .target
            .and_then(|key| self.host.kernel().node_by_key(key).map(|n| n.id))
            .filter(|&t| {
                self.pointer_node(t, &[EventKind::Drop]) == Some(t)
                    && !self.brush.region_blocks_action(t)
            });
        let files = match target {
            Some(_) => mint(&dropped.paths),
            None => Vec::new(),
        };
        let arrived = !(files.is_empty() && dropped.text.is_empty() && dropped.url.is_empty());
        let delivered = target.filter(|&t| {
            arrived
                && self.deliver_drop(
                    t,
                    &pending,
                    files,
                    dropped.text.clone(),
                    dropped.url.clone(),
                )
        });
        // The failed drop's leaves go first, ahead of what a drop released
        // behind it holds: they can change what the drag still under way is
        // over
        if delivered.is_none() {
            for m in pending.chain.iter().filter(|m| m.entered).rev() {
                self.drag_in.outbox.push_front(Sent {
                    key: m.key,
                    kind: EventKind::Dragleave,
                    point: pending.point,
                    held: pending.held,
                    drop: Some(id),
                    after: None,
                });
            }
        }
        self.drag_flush();
        self.lift_landed(id, delivered.is_some());
        if self.drag_in.over {
            self.drag_at(self.drag_in.point, self.drag_in.held);
            self.drag_flush();
        }
        delivered
    }

    /// The host's drags went away with what reported them (Linux's listener):
    /// each drop still waiting for its items fails, with its leaves, and each
    /// lift ends with nothing taking it (LLP 1094.000 D16).
    pub fn drags_gone(&mut self) {
        let pending: Vec<u32> = self.drag_in.pending.iter().map(|p| p.id).collect();
        for drop in pending {
            self.drop_landed(drop, &Dropped::default());
        }
        let held = self.modifiers();
        let lifts: Vec<u32> = self.drag_in.lifting.iter().map(|s| s.id).collect();
        for lift in lifts {
            self.lift_ended(lift, false, held);
        }
    }

    /// Who takes a drop that Android gives hosted field `id`'s view at
    /// `point` (LLP 1094.001 D5): -1 not the field, which another node is
    /// painted over there or which takes no touch; else as `drop_owner`.
    pub fn field_drop(&mut self, id: ViewId, point: (f32, f32)) -> i32 {
        if self.drag_hit(point) != Some(id) || self.field_inert(id) {
            return -1;
        }
        self.drop_owner(id)
    }

    /// Which node takes a drop over `id`: 0 none, 1 `id` itself, 2 one above
    /// it, so a hosted field knows whether to give a drag away (LLP 1094.001 D5).
    pub fn drop_owner(&self, id: ViewId) -> i32 {
        match self.pointer_node(id, &[EventKind::Drop]) {
            None => 0,
            Some(n) if n == id => 1,
            Some(_) => 2,
        }
    }

    /// Views a commit renewed (LLP 1078) are new mounts: a pending drop's
    /// target among them is not its target, none of them heard a drag's
    /// enter, and a lift's source among them hears no more of its drag.
    pub(super) fn drag_renewed(&mut self, renewed: &[ViewId]) {
        let kernel = self.host.kernel();
        let keys: Vec<NodeKey> = renewed
            .iter()
            .filter_map(|id| kernel.node(*id).map(|n| n.key))
            .collect();
        if keys.is_empty() {
            return;
        }
        let drag = &mut self.drag_in;
        for source in drag.lifting.iter_mut().filter(|s| keys.contains(&s.key)) {
            source.renewed = true;
        }
        drag.chain.retain(|m| !keys.contains(&m.key));
        drag.outbox.retain(|s| !keys.contains(&s.key));
        for pending in &mut drag.pending {
            pending.chain.retain(|m| !keys.contains(&m.key));
            if pending.target.is_some_and(|k| keys.contains(&k)) {
                pending.target = None;
            }
        }
    }

    pub(crate) fn agent_drag(
        &mut self,
        id: Option<ViewId>,
        request: &serde_json::Value,
    ) -> Result<String, String> {
        let id = id.ok_or("a drag requires a target")?;
        if request.get("dragleave").is_some() {
            self.drag_left();
            return Ok(
                serde_json::json!({"tapped":id, "dragleave":true, "delivery":"presenter"})
                    .to_string(),
            );
        }
        let kind = if request.get("drop").is_some() {
            "drop"
        } else {
            "dragenter"
        };
        let paths: Vec<PathBuf> = serde_json::from_value(request[kind].clone())
            .map_err(|_| format!("{kind} requires a list of file paths"))?;
        let [text, url] = ["text", "url"].map(|key| match request.get(key) {
            None => Ok(String::new()),
            Some(value) => value
                .as_str()
                .map(str::to_owned)
                .ok_or(format!("{kind}'s {key} is a string")),
        });
        let dropped = Dropped {
            paths,
            text: text?,
            url: url?,
        };
        if dropped == Dropped::default() {
            return Err(format!(
                "{kind} requires a file path, --text or --url: a drag carries something"
            ));
        }
        let at: Option<[f32; 2]> = serde_json::from_value(request["at"].clone())
            .map_err(|_| "a drag's at requires two coordinates")?;
        let point = self.pointer_target(id, at.map(|[x, y]| (x, y)))?;
        let held = match request.get("modifiers") {
            None => self.modifiers(),
            Some(keys) => keys
                .as_str()
                .and_then(KeyModifiers::held)
                .ok_or("invalid drag modifiers")?,
        };
        if let Some(path) = (kind == "drop" && cfg!(target_os = "android"))
            .then(|| dropped.paths.iter().find(|p| !p.is_file()))
            .flatten()
        {
            return Err(format!("{} is no file on the device: the reader's carrier puts a drop's files there, and the headless one does not", path.display()));
        }
        if kind == "drop" && cfg!(target_os = "android") && self.field_keeps(point, &dropped) {
            return Err(format!("view {id} is under a field, which keeps a drop of text or a link on Android when its own node takes none (LLP 1094.001 D2), and the driver inserts nothing: drop a file with it, or drop on a node that is not a field"));
        }
        let target = if kind == "drop" {
            self.dropped(point, held, &dropped).ok_or_else(|| {
                format!("view {id} took no drop: nothing under it declares `drop`, or none of its files could be opened")
            })?
        } else {
            self.drag_in.native = false;
            self.drag_hover(point, held);
            id
        };
        Ok(serde_json::json!({"tapped":target, "at":point, (kind):dropped.paths.len(), "delivery":"presenter"}).to_string())
    }

    /// Whether the field at `point` keeps `dropped`, as Android's text view
    /// does a drag of text or a link that its own node takes no drop of; one
    /// that carries a file goes to the `drop` above it (LLP 1094.001 D5).
    fn field_keeps(&mut self, point: (f32, f32), dropped: &Dropped) -> bool {
        dropped.paths.is_empty()
            && self.drag_hit(point).is_some_and(|hit| {
                self.hosts_field(hit) && !self.field_inert(hit) && self.drop_owner(hit) != 1
            })
    }

    fn drag_hit(&mut self, point: (f32, f32)) -> Option<ViewId> {
        self.hit(point.0, point.1)
            .filter(|id| !self.brush.region_blocks_action(*id))
    }

    /// `hit` and its listener ancestors that hear `dragenter` or
    /// `dragleave`, innermost first.
    fn drag_chain(&self, hit: Option<ViewId>) -> Vec<NodeKey> {
        let mut chain = Vec::new();
        let mut at = hit;
        while let Some(node) =
            at.and_then(|id| self.pointer_node(id, &[EventKind::Dragenter, EventKind::Dragleave]))
        {
            let kernel = self.host.kernel();
            let Some(key) = kernel.node(node).map(|n| n.key) else {
                break;
            };
            chain.push(key);
            at = self.listener_parent(kernel, node);
        }
        chain
    }

    /// The drag is at `point`: its chain there is recorded and its enters
    /// and leaves queued, before any is sent. What it hit there.
    fn drag_at(&mut self, point: (f32, f32), held: KeyModifiers) -> Option<ViewId> {
        self.drag_in.over = true;
        self.drag_in.point = point;
        self.drag_in.held = held;
        let hit = self.drag_hit(point);
        let chain = self.drag_chain(hit);
        self.drag_crossings(chain);
        self.drag_in.edge = if self.drag_in.native {
            self.drag_edge_at(point)
        } else {
            None
        };
        if self.drag_in.edge.is_none() {
            self.drag_in.edge_clock = None;
        }
        hit
    }

    /// The innermost node that takes a drop over `hit`.
    fn drop_target(&self, hit: Option<ViewId>) -> Option<ViewId> {
        self.pointer_node(hit?, &[EventKind::Drop])
    }

    /// The drag is over `keys`, innermost first: a node it left will hear a
    /// leave if it heard an enter, and one it came over an enter, unless a
    /// pending drop is over that node, whose enter then waits for the drop
    /// to land (LLP 1094.001 D2).
    fn drag_crossings(&mut self, keys: Vec<NodeKey>) {
        let previous = std::mem::take(&mut self.drag_in.chain);
        let pending = &self.drag_in.pending;
        let chain: Vec<Member> = keys
            .into_iter()
            .map(|key| {
                let was = previous.iter().any(|m| m.key == key && m.entered);
                let waits = pending.iter().any(|p| p.chain.iter().any(|m| m.key == key));
                Member {
                    key,
                    entered: was || !waits,
                }
            })
            .collect();
        let (point, held) = (self.drag_in.point, self.drag_in.held);
        for m in previous
            .iter()
            .filter(|m| m.entered && !chain.iter().any(|n| n.key == m.key))
        {
            self.drag_in
                .send(m.key, EventKind::Dragleave, (point, held), None);
        }
        for m in chain
            .iter()
            .filter(|m| m.entered && !previous.iter().any(|p| p.key == m.key && p.entered))
        {
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), None);
        }
        self.drag_in.chain = chain;
    }

    /// The drag dropped where no node took it: the nodes under it that
    /// waited for a pending drop hear their enter, then every one its leave.
    fn drag_ended(&mut self) {
        self.drag_in.over = false;
        self.drag_edge_ended();
        let (point, held) = (self.drag_in.point, self.drag_in.held);
        let chain = std::mem::take(&mut self.drag_in.chain);
        for m in chain.iter().filter(|m| !m.entered) {
            self.drag_in
                .send(m.key, EventKind::Dragenter, (point, held), None);
        }
        for m in &chain {
            self.drag_in
                .send(m.key, EventKind::Dragleave, (point, held), None);
        }
    }

    /// Send the queued enters and leaves in order, each to a node still the
    /// one it was decided for: a handler that renews, disables or hides one
    /// runs before the next is sent. An enter goes only to a node that still
    /// hears drags and has a box, and one that does not then hears no leave
    /// either.
    fn drag_flush(&mut self) {
        loop {
            let DragIn {
                outbox, pending, ..
            } = &mut self.drag_in;
            let Some(sent) = outbox.pop_front_if(|sent| !waits(pending, sent)) else {
                break;
            };
            let Some(id) = self.host.kernel().node_by_key(sent.key).map(|n| n.id) else {
                continue;
            };
            let kinds = [EventKind::Dragenter, EventKind::Dragleave];
            let enter = sent.kind == EventKind::Dragenter;
            let heard = (!enter || self.pointer_node(id, &kinds) == Some(id))
                && self.drag_event(id, sent.kind, sent.point, sent.held);
            if enter && !heard {
                self.drag_in.unentered(sent.key, sent.drop);
            }
        }
    }

    /// Send an enter or a leave, which carry no data (LLP 1094.001 D1), to
    /// `id`, its record at `point` with `held`; false when it has no box to
    /// take a record from.
    fn drag_event(
        &mut self,
        id: ViewId,
        kind: EventKind,
        point: (f32, f32),
        held: KeyModifiers,
    ) -> bool {
        let Some(record) = self.pointer_record(id, point.0, point.1, -1, 0) else {
            return false;
        };
        let data = DropEvent {
            offset_x: record.offset_x,
            offset_y: record.offset_y,
            held,
            files: Vec::new(),
            text: String::new(),
            url: String::new(),
            drop_effect: exact_runner::DropEffect::None,
        };
        let event = match kind {
            EventKind::Dragenter => Event::Dragenter(data),
            _ => Event::Dragleave(data),
        };
        if let Some(error) = self.pointer_dispatch(id, kind, event, self.pointer_now()) {
            self.host.log(error);
        }
        true
    }

    /// Send pending drop `drop` to `target` with what arrived, at the record it had at its release.
    fn deliver_drop(
        &mut self,
        target: ViewId,
        drop: &Pending,
        files: Vec<String>,
        text: String,
        url: String,
    ) -> bool {
        let data = DropEvent {
            offset_x: drop.offsets.0,
            offset_y: drop.offsets.1,
            held: drop.held,
            files,
            text,
            url,
            drop_effect: exact_runner::DropEffect::Copy,
        };
        if let Some(error) = self.pointer_dispatch(
            target,
            EventKind::Drop,
            Event::Drop(data),
            self.pointer_now(),
        ) {
            self.host.log(error);
            return false;
        }
        true
    }
}

#[cfg(test)]
#[path = "file_drop_tests.rs"]
mod tests;
