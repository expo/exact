//! Zoomable scroll containers on the painted hosts (@ref LLP 1057.001.000
//! §5: Android, Windows, Linux). The painter draws the child at the zoom and
//! places it by D3 (`paint/zoom.rs`); this is the zoom itself: the app's
//! `scrollZoom` write and D2's rules for it, `ctrl`+wheel about the pointer,
//! a pinch a shell recognizes (Android's reader, Windows's two contacts), and
//! the touch double tap with the press it holds (§3 rule 5, D5).
//!
//! The zoom is host state, as the offset is: each move sets both, so the
//! point under the focal point stays there, and reports `scroll` with the
//! zoom. A write is held while a pinch runs. A write equal to a zoom reported
//! since the last gesture began is the app mirroring the gesture back and
//! moves nothing (D2's echo).
use super::*;
use crate::paint::zoom::{range, Fit};
use contact::Contact;
use exact_kernel::NodeKey;

/// An animated zoom's length, ms: a glide's (`glide.rs`) and the web's.
const ZOOM_MS: f64 = 300.;
/// Wheel ticks closer than this are one gesture: an echo of an earlier tick's
/// zoom is still the app's mirror, as the web's wheel rests after 150 ms.
const WHEEL_QUIET_MS: f64 = 150.;
/// How long a first tap waits for a second, ms, and how near it must land,
/// points: Android's `ViewConfiguration` double-tap timeout, and the web's
/// slop (`zoom.js`).
pub(super) const DOUBLE_TAP_MS: f64 = 300.;
const DOUBLE_TAP_SLOP: f32 = 30.;

#[derive(Default)]
pub(super) struct Zooms {
    /// Whether the plan has a zoomable scroller, read once.
    plan: Option<bool>,
    by: BTreeMap<ViewId, Zoom>,
    /// A first tap's press, waiting to learn whether a second tap comes.
    held: Option<HeldTap>,
    /// The last tap on a double-tappable container (its id, where, when),
    /// for the presenter's own recognizer.
    first: Option<(ViewId, (f32, f32), f64)>,
    /// The contact that is down is a double tap's second: no press, and its
    /// lift zooms (the presenter's recognizer).
    second: Option<ViewId>,
    last_handler_tap: Option<(NodeKey, (f32, f32), f64)>,
    handler_down: Option<HandlerTap>,
    /// A shell recognizes the double tap itself (Android's reader, with
    /// `GestureDetector`) and says so through [`Presenter::zoom_tap_down`].
    pub(super) shell_taps: bool,
}

impl Zooms {
    /// Nothing of a replaced tree's, but whether the shell recognizes the
    /// double tap.
    pub(super) fn kept(old: &Self) -> Self {
        Self {
            shell_taps: old.shell_taps,
            ..Self::default()
        }
    }
}

struct Zoom {
    key: exact_kernel::NodeKey,
    /// The `scrollZoom` the app last wrote, applied or not.
    written: Option<f64>,
    /// A write not yet applied: a pinch holds it.
    pending: Option<f64>,
    /// The zooms reported since the last gesture began.
    reported: Vec<f32>,
    pinching: bool,
    /// When the last `ctrl`+wheel tick zoomed it.
    wheel_at: Option<f64>,
    animation: Option<Animation>,
}

#[derive(Clone, Copy)]
struct Animation {
    from: f32,
    to: f32,
    focal: (f32, f32),
    start: f64,
}

struct HandlerTap {
    listener: NodeKey,
    origin: (f32, f32),
    down: f64,
    second: bool,
}

struct HeldTap {
    container: ViewId,
    at: (f32, f32),
    due: f64,
    contact: Contact,
}

/// Who moves the zoom: what the scroll event and the scroll's end follow.
#[derive(Clone, Copy, PartialEq)]
enum Mover {
    /// The app's write, after a commit: its `scroll` is queued.
    App,
    /// The person's input outside a commit: its `scroll` goes now.
    Input,
    /// A frame of an animated zoom: as input, and its scroll ends when the
    /// animation does.
    Frame,
}

impl<D: DataSource> Presenter<D> {
    /// The shell recognizes the double tap itself and says so
    /// ([`Presenter::zoom_tap_down`]): Android's reader.
    pub fn set_shell_taps(&mut self) {
        self.zooms.shell_taps = true;
    }

    /// After a commit, before its offsets (`sync_authored_scroll`): each
    /// zoomable container's range, and a `scrollZoom` written, about the
    /// port's centre. Offsets the same batch writes then land in the new
    /// zoom's coordinates, so zoom and offsets are one move (D2).
    pub(super) fn sync_zoom(&mut self) {
        let plan = self.host.runner().plan();
        if !*self
            .zooms
            .plan
            .get_or_insert_with(|| exact_runner::uses::zooms(plan))
        {
            return;
        }
        let kernel = self.host.kernel();
        let ids: Vec<ViewId> = kernel
            .preorder_where(&self.host.roots(), |_, _| true)
            .into_iter()
            .filter(|id| kernel.node(*id).is_some_and(|n| range(&n).is_some()))
            .collect();
        self.zooms
            .by
            .retain(|id, z| ids.contains(id) && kernel.node(*id).is_some_and(|n| n.key == z.key));
        let by = &self.zooms.by;
        self.brush
            .zoom
            .retain(|id, _| by.contains_key(id) || ids.contains(id));
        for id in ids {
            self.settle_zoom(id, false);
        }
    }

    /// Container `id`'s range and a write, unless the reader holds it;
    /// `resting` when the reader is letting go and the write may move.
    fn settle_zoom(&mut self, id: ViewId, resting: bool) {
        let kernel = self.host.kernel();
        let Some(node) = kernel.node(id) else { return };
        let Some((low, high)) = range(&node) else {
            return;
        };
        let written = node
            .props
            .get(PropId::ScrollZoom)
            .and_then(exact_kernel::PropValue::as_float)
            .filter(|z| z.is_finite());
        let smooth = node.style.scroll_behavior == exact_kernel::ScrollBehavior::Smooth
            && self.reveal_glides(None);
        let key = node.key;
        let current = self.brush.zoom.get(&id).copied();
        let held = !resting && self.scroll_held(id) || self.host.route_visibility(id).0;
        let zoom = self.zooms.by.entry(id).or_insert_with(|| Zoom {
            key,
            written: None,
            pending: None,
            reported: Vec::new(),
            pinching: false,
            wheel_at: None,
            animation: None,
        });
        // A change is a write; the same value written again moves nothing.
        if written != zoom.written {
            zoom.written = written;
            zoom.pending = written.filter(|z| *z > 0.);
        }
        let z = current.unwrap_or(1.);
        // D2: a write waits while the reader pinches or pans.
        if zoom.pinching || held {
            self.brush.zoom.entry(id).or_insert(z.clamp(low, high));
            return;
        }
        let mut target = zoom.animation.map_or(z, |a| a.to).clamp(low, high);
        let mut animated = false;
        if let Some(asked) = zoom.pending.take() {
            if !zoom.reported.contains(&(asked as f32)) {
                target = (asked as f32).clamp(low, high);
                animated = smooth;
            }
            // Only a write the gesture held can echo it: a later one is the
            // app's, whatever its value.
            zoom.reported.clear();
        }
        let running = zoom.animation.map(|a| a.to);
        let centre = Fit::of(kernel, &node).map_or((0., 0.), |f| (f.port.0 / 2., f.port.1 / 2.));
        if animated && target != z {
            self.animate_zoom(id, target, centre);
        } else if running.map_or(current != Some(target), |to| to != target) {
            // A running animation goes on to where it was going; any other
            // change of target lands at once.
            if let Some(z) = self.zooms.by.get_mut(&id) {
                z.animation = None;
            }
            self.move_zoom(id, target, centre, Mover::App);
        }
    }

    /// To zoom `to`, keeping under `focal` (a point of the port, from its
    /// top left) the child's point that is there; the offset clamped to the
    /// new extent. Whether anything moved.
    fn move_zoom(&mut self, id: ViewId, to: f32, focal: (f32, f32), mover: Mover) -> bool {
        let kernel = self.host.kernel();
        let Some(node) = kernel.node(id) else {
            return false;
        };
        let from = self.brush.zoom_of(&node).unwrap_or(1.);
        let Some(fit) = Fit::of(kernel, &node) else {
            self.brush.zoom.insert(id, to);
            return false;
        };
        let key = node.key;
        let before = self.scroll_of(id);
        let to_offset = fit.about(from, before, focal, to);
        let max = fit.max(to);
        let next = (to_offset.0.clamp(0., max.0), to_offset.1.clamp(0., max.1));
        let was = self.brush.zoom.insert(id, to);
        if was == Some(to) && next == before {
            return false;
        }
        self.scroll.insert(id, next);
        self.dirty = true;
        if let Some(z) = self.zooms.by.get_mut(&id) {
            if z.reported.len() == 64 {
                z.reported.remove(0);
            }
            z.reported.push(to);
        }
        if self.booting || was.is_none() && from == to && next == before {
            return true;
        }
        let completion = if self.scroll_held(id) {
            Completion::Reader
        } else if mover == Mover::Frame {
            Completion::Glide
        } else {
            Completion::Instant
        };
        self.scroll_end_moved(id, completion);
        match mover {
            Mover::Input | Mover::Frame => self.collection_scrolled(id),
            Mover::App => {
                self.collection.scroll_event(id, key);
                self.executor.notify();
            }
        }
        self.scroll_timelines();
        if let Some(error) = self.refresh_transform_geometry() {
            self.host.log(error);
        }
        true
    }

    /// A viewport point in zoomable container `id`'s port, from its top
    /// left, through whatever it is painted under.
    fn port_point(&mut self, id: ViewId, at: (f32, f32)) -> Option<(f32, f32)> {
        let local = self.box_of(id)?.local(at.0, at.1)?;
        let key = self.host.kernel().node(id)?.key;
        let (left, top, _, _) = self.host.kernel().resolved_border(key).unwrap_or_default();
        Some((local.0 - left, local.1 - top))
    }

    /// The innermost zoomable scroller under a viewport point: what a
    /// shell's second finger there pinches (Windows's `touch.rs`).
    pub fn zoomable_under(&mut self, x: f32, y: f32) -> Option<ViewId> {
        self.hit(x, y)
            .filter(|&hit| self.allows_touch_zoom(hit, self.zoomable_from(hit)))
            .and_then(|hit| self.zoomable_from(hit))
    }

    /// The innermost zoomable container from `hit` out.
    pub(super) fn zoomable_from(&self, hit: ViewId) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let mut at = Some(hit);
        while let Some(id) = at {
            let node = kernel.node(id)?;
            if range(&node).is_some() {
                return Some(id);
            }
            at = node.parent;
        }
        None
    }

    /// `ctrl`+wheel at a point zooms the innermost zoomable container under
    /// it about the point, as a trackpad's pinch reaches a window as one
    /// (§3: a pinch that is not a touch goes to the innermost consumer).
    /// The web's rate: `dy` of 100 is a factor of e. Whether it took the
    /// wheel; at a limit it still does, and nothing scrolls.
    pub fn wheel_zoom(&mut self, x: f32, y: f32, dy: f32) -> bool {
        let Some(id) = self.hit(x, y).and_then(|hit| self.zoomable_from(hit)) else {
            return false;
        };
        let Some((low, high)) = self.host.kernel().node(id).and_then(|n| range(&n)) else {
            return false;
        };
        let Some(focal) = self.port_point(id, (x, y)) else {
            return false;
        };
        let now = self.host.now();
        let z = self.zoom_now(id);
        if let Some(zoom) = self.zooms.by.get_mut(&id) {
            if zoom.wheel_at.is_none_or(|at| now - at > WHEEL_QUIET_MS) {
                zoom.reported.clear();
            }
            zoom.wheel_at = Some(now);
            zoom.animation = None;
            zoom.pending = None;
        }
        let to = (z * (-dy * 0.01).exp()).clamp(low, high);
        self.move_zoom(id, to, focal, Mover::Input);
        true
    }

    /// Container `id`'s zoom as drawn now.
    fn zoom_now(&self, id: ViewId) -> f32 {
        let kernel = self.host.kernel();
        kernel
            .node(id)
            .and_then(|n| self.brush.zoom_of(&n))
            .unwrap_or(1.)
    }

    /// A shell's pinch began on zoomable container `id` (Android's reader,
    /// Windows's second contact): a write waits for its end, and the zooms
    /// it reports are the ones an app echoes. Its scroll owes no `scrollend`
    /// until it ends.
    pub fn zoom_pinch_began(&mut self, id: ViewId) {
        let Some(zoom) = self.zooms.by.get_mut(&id) else {
            return;
        };
        zoom.pinching = true;
        zoom.reported.clear();
        zoom.animation = None;
        self.scroll_end_hold(id);
    }

    /// The pinch on `id` scaled by `factor` while its focal point moved from
    /// `from` to `to` (viewport points): the child's point under the fingers
    /// stays under them, the zoom clamped to the range.
    pub fn zoom_pinched(&mut self, id: ViewId, factor: f32, from: (f32, f32), to: (f32, f32)) {
        if !(factor.is_finite() && factor > 0.)
            || !self.zooms.by.get(&id).is_some_and(|z| z.pinching)
        {
            return;
        }
        let Some((low, high)) = self.host.kernel().node(id).and_then(|n| range(&n)) else {
            return;
        };
        let (Some(a), Some(b)) = (self.port_point(id, from), self.port_point(id, to)) else {
            return;
        };
        let z = self.zoom_now(id);
        let target = (z * factor).clamp(low, high);
        // Zoomed about where the fingers were, then panned by how far they went.
        let before = self.scroll_of(id);
        self.scroll
            .insert(id, (before.0 + a.0 - b.0, before.1 + a.1 - b.1));
        if !self.move_zoom(id, target, b, Mover::Input) {
            self.scroll.insert(id, before);
            let pan = (before.0 + a.0 - b.0, before.1 + a.1 - b.1);
            self.scroll_container_to(id, pan);
        }
    }

    /// The pinch on `id` ended: a write it held applies now, and its
    /// `scrollend` is due.
    pub fn zoom_pinch_ended(&mut self, id: ViewId) {
        let Some(zoom) = self.zooms.by.get_mut(&id) else {
            return;
        };
        zoom.pinching = false;
        self.scroll_end_release(id);
    }

    /// The reader let go of `id`: a write it held moves now, as one move
    /// before the `scrollend` (D2).
    pub(super) fn zoom_rests(&mut self, id: ViewId) {
        if self
            .zooms
            .by
            .get(&id)
            .is_some_and(|z| !z.pinching && z.pending.is_some())
        {
            self.settle_zoom(id, true);
        }
    }

    /// Move `id` to `to` as the person's scroll, clamped.
    fn scroll_container_to(&mut self, id: ViewId, to: (f32, f32)) {
        let before = self.scroll_of(id);
        let Some(bounds) = self
            .host
            .kernel()
            .node(id)
            .and_then(|n| self.brush.zoom_bounds(self.host.kernel(), &n))
        else {
            return;
        };
        let next = bounds.clamp(to);
        if next == before {
            return;
        }
        self.scroll.insert(id, next);
        self.dirty = true;
        self.scroll_end_moved(id, Completion::Reader);
        self.collection_scrolled(id);
    }

    fn animate_zoom(&mut self, id: ViewId, to: f32, focal: (f32, f32)) {
        let from = self.zoom_now(id);
        let start = self.host.now();
        if let Some(zoom) = self.zooms.by.get_mut(&id) {
            zoom.animation = Some(Animation {
                from,
                to,
                focal,
                start,
            });
        }
        self.dirty = true;
    }

    /// Whether an animated zoom runs: the display wants frames.
    pub(crate) fn zooms_moving(&self) -> bool {
        self.zooms.by.values().any(|z| z.animation.is_some())
    }

    /// Whether container `id`'s zoom is animating.
    pub(super) fn zoom_animating(&self, id: ViewId) -> bool {
        self.zooms
            .by
            .get(&id)
            .is_some_and(|z| z.animation.is_some())
    }

    /// Each animated zoom's frame at `now`: geometric in the zoom, on a
    /// glide's ease, and one at its end lands.
    pub(super) fn step_zooms(&mut self, now: f64) {
        let running: Vec<(ViewId, Animation)> = (self.zooms.by.iter())
            .filter_map(|(id, z)| z.animation.map(|a| (*id, a)))
            .collect();
        for (id, a) in running {
            let t = ((now - a.start) / ZOOM_MS).clamp(0., 1.);
            let z = a.from * (a.to / a.from).powf(glide::ease(t) as f32);
            if t >= 1. {
                if let Some(zoom) = self.zooms.by.get_mut(&id) {
                    zoom.animation = None;
                }
            }
            self.move_zoom(id, if t >= 1. { a.to } else { z }, a.focal, Mover::Frame);
        }
    }

    /// The zoom `id` reports in its `scroll`: 1 for a scroller that does
    /// not zoom.
    pub(super) fn reported_zoom(&self, id: ViewId) -> f64 {
        self.host
            .kernel()
            .node(id)
            .and_then(|n| self.brush.zoom_of(&n))
            .map_or(1., f64::from)
    }

    /// The agent's `layout` fields of a zoomable container: its zoom and
    /// range, beside its offset (§6).
    pub(super) fn zoom_layout(&self, id: ViewId, out: &mut String) {
        let kernel = self.host.kernel();
        let Some(node) = kernel.node(id) else { return };
        let (Some((low, high)), Some(z)) = (range(&node), self.brush.zoom_of(&node)) else {
            return;
        };
        let _ = write!(
            out,
            ",\"scrollZoom\":{},\"minZoom\":{},\"maxZoom\":{}",
            num(z as f64),
            num(low as f64),
            num(high as f64)
        );
    }

    // The double tap (§3 rule 5, D5).

    /// The zoomable container a touch double tap on `hit` would zoom: the
    /// innermost one out from `hit`, where every node on the way has
    /// `touch-action: auto`, none is a control or an editor, and none hears
    /// `dblclick`.
    pub(super) fn double_tap_container(&self, hit: ViewId) -> Option<ViewId> {
        let kernel = self.host.kernel();
        let runner = self.host.runner();
        let mut at = Some(hit);
        while let Some(id) = at {
            let node = kernel.node(id)?;
            let control = matches!(
                node.node_type,
                NodeType::Control
                    | NodeType::TextInput
                    | NodeType::Pressable
                    | NodeType::NativeView
                    | NodeType::WebView
                    | NodeType::Video
                    | NodeType::Canvas
            ) || node.props.str(PropId::Href).is_some()
                || matches!(
                    node.props.str(PropId::AccessibilityRole),
                    Some("button" | "link" | "checkbox" | "switch" | "slider" | "tab")
                );
            if control
                || node.style.touch_action != exact_kernel::TouchAction::Auto
                || runner.handlers_of(id).contains(&EventKind::Dblclick)
            {
                return None;
            }
            if range(&node).is_some() {
                return Some(id);
            }
            at = node.parent;
        }
        None
    }

    /// At a touch's lift, whether its press waits for a second tap: a press
    /// on content that is not a control, where a double tap could zoom. A
    /// press already waiting goes first.
    pub(super) fn tap_waits(&mut self, hit: Option<exact_kernel::NodeKey>) -> Option<ViewId> {
        if !self.contact_is_direct() {
            return None;
        }
        let hit = self.host.kernel().node_by_key(hit?)?.id;
        self.double_tap_container(hit)
    }

    /// The press of `contact`, lifted at `at` over `container`, waits.
    pub(super) fn hold_press(
        &mut self,
        container: ViewId,
        contact: Contact,
        at: (f32, f32),
        now: f64,
    ) {
        if self.zooms.second.take().is_some() {
            return;
        }
        self.release_held_tap(true);
        self.zooms.first = Some((container, at, now));
        self.zooms.held = Some(HeldTap {
            container,
            at,
            due: now + DOUBLE_TAP_MS,
            contact,
        });
    }

    /// When the waiting press is due.
    pub(super) fn held_tap_due(&self) -> Option<f64> {
        self.zooms.held.as_ref().map(|h| h.due)
    }

    /// The waiting press, once no second tap can come (or `now`, when
    /// another press takes its place).
    pub(super) fn release_held_tap(&mut self, now: bool) -> Option<String> {
        let due = self.zooms.held.as_ref()?.due;
        if !now && due > self.host.now() {
            return None;
        }
        let held = self.zooms.held.take()?;
        self.press_lifted(held.contact, held.at.0, held.at.1, self.host.now())
            .err()
    }

    fn double_tap_listener(&mut self, at: (f32, f32)) -> Option<NodeKey> {
        let hit = self.hit(at.0, at.1)?;
        let listener = self.pointer_node(hit, &[EventKind::Dblclick])?;
        let key = self.host.kernel().node(listener)?.key;
        self.input_live(key).then_some(key)
    }

    pub(super) fn handler_tap_moved(&mut self, at: (f32, f32), now: f64) {
        if self.zooms.handler_down.as_ref().is_some_and(|tap| {
            now - tap.down > DOUBLE_TAP_MS
                || (at.0 - tap.origin.0).hypot(at.1 - tap.origin.1) > self.slop as f32
        }) {
            self.zooms.handler_down = None;
        }
    }

    /// The ordinary second press is delivered before its double click. A press
    /// that removes the listener cannot send the double click to its replacement.
    pub(super) fn press_and_double_tap(
        &mut self,
        contact: Contact,
        x: f32,
        y: f32,
        now: f64,
        event: f64,
    ) -> Result<bool, String> {
        let tap = self
            .zooms
            .handler_down
            .take()
            .filter(|tap| self.double_tap_listener((x, y)) == Some(tap.listener));
        let pressed = self.press_lifted(contact, x, y, now)?;
        if let Some(tap) = tap.filter(|tap| self.input_live(tap.listener)) {
            if tap.second {
                if let Some(id) = self.host.kernel().node_by_key(tap.listener).map(|n| n.id) {
                    if let Some(e) =
                        self.pointer_dispatch(id, EventKind::Dblclick, Event::Dblclick, now)
                    {
                        return Err(e);
                    }
                }
            } else {
                self.zooms.last_handler_tap = Some((tap.listener, (x, y), event));
            }
        }
        Ok(pressed)
    }

    /// A touch landed at `at` (viewport points): when it is a double tap's
    /// second (the presenter's recognizer), the first tap's press never
    /// fires and this one presses nothing; its lift zooms.
    pub(super) fn zoom_contact_down(&mut self, at: (f32, f32), now: f64, event: f64) {
        self.zooms.second = None;
        let last = self.zooms.last_handler_tap.take();
        self.zooms.handler_down = if self.pointer_kind == PointerKind::Touch {
            self.double_tap_listener(at).map(|listener| {
                // Android GestureDetector excludes the first 40 ms after the first up.
                let minimum = if self.zooms.shell_taps { 40. } else { 0. };
                let second = last.is_some_and(|(key, point, when)| {
                    key == listener
                        && (minimum..=DOUBLE_TAP_MS).contains(&(event - when))
                        && (at.0 - point.0).hypot(at.1 - point.1) <= DOUBLE_TAP_SLOP
                });
                HandlerTap {
                    listener,
                    origin: at,
                    down: event,
                    second,
                }
            })
        } else {
            None
        };
        if self.zooms.shell_taps || !self.contact_is_direct() {
            return;
        }
        let Some((container, first, when)) = self.zooms.first else {
            return;
        };
        let near = (at.0 - first.0).hypot(at.1 - first.1) <= DOUBLE_TAP_SLOP;
        let under = self
            .hit(at.0, at.1)
            .and_then(|hit| self.double_tap_container(hit));
        if near && now - when <= DOUBLE_TAP_MS && under == Some(container) {
            self.zooms.first = None;
            self.zooms.held = None;
            self.zooms.second = Some(container);
        }
    }

    /// The lifting contact was a double tap's second: it zooms at `at` and
    /// presses nothing. Whether it was.
    pub(super) fn zoom_contact_lifted(&mut self, at: (f32, f32)) -> bool {
        let Some(container) = self.zooms.second.take() else {
            return false;
        };
        self.double_tap_zoom(container, at);
        true
    }

    /// A shell's double tap landed its second tap at `at` (Android's
    /// `GestureDetector`): the first tap's press never fires. Whether `at`
    /// is over a container a double tap zooms.
    pub fn zoom_tap_down(&mut self, at: (f32, f32)) -> bool {
        let Some(container) = self
            .hit(at.0, at.1)
            .and_then(|hit| self.double_tap_container(hit))
        else {
            return false;
        };
        if self
            .zooms
            .held
            .as_ref()
            .is_some_and(|h| h.container == container)
        {
            self.zooms.held = None;
        }
        self.zooms.first = None;
        true
    }

    /// A shell's double tap ended at `at`: the container under it zooms.
    pub fn zoom_double_tap(&mut self, at: (f32, f32)) -> bool {
        let Some(container) = self
            .hit(at.0, at.1)
            .and_then(|hit| self.double_tap_container(hit))
        else {
            return false;
        };
        self.double_tap_zoom(container, at)
    }

    /// D5's double tap on `container` at viewport point `at`: from above the
    /// minimum to the minimum, at it to twice it within the range, about the
    /// tap, animated where the display owns the clock.
    pub(super) fn double_tap_zoom(&mut self, container: ViewId, at: (f32, f32)) -> bool {
        let Some((low, high)) = self.host.kernel().node(container).and_then(|n| range(&n)) else {
            return false;
        };
        if self.zooms.by.get(&container).is_some_and(|z| z.pinching) {
            return false;
        }
        let Some(focal) = self.port_point(container, at) else {
            return false;
        };
        let z = self.zoom_now(container);
        let to = if z > low { low } else { high.min(2. * low) };
        if let Some(zoom) = self.zooms.by.get_mut(&container) {
            zoom.reported.clear();
            zoom.pending = None;
        }
        if to == z {
            return true;
        }
        if self.reveal_glides(None) {
            self.animate_zoom(container, to, focal);
        } else {
            self.move_zoom(container, to, focal, Mover::Input);
        }
        true
    }

    // The agent's pinch and touch double tap (§6).

    /// `tap <target> pinch <scale> [at]` and `tap <target> dblclick touch
    /// [at]`; `None` for any other tap. The pinch is what a shell's
    /// recognizer tells the presenter (Android's reader, Windows's two
    /// contacts), in eight steps about the point; the double tap is two
    /// touch contacts through the presenter's own recognizer, which the
    /// agent's contacts reach instead of the reader's `GestureDetector`.
    pub(crate) fn zoom_tap(&mut self, request: &serde_json::Value) -> Option<String> {
        let pinch = request.get("pinch");
        let double = request["dblclick"] == true && request["touch"] == true;
        if pinch.is_none() && !double {
            return None;
        }
        let fail = |e: &str| Some(serde_json::json!({ "error": e }).to_string());
        let Some(id) = request["id"].as_u64().and_then(|id| u32::try_from(id).ok()) else {
            return fail("tap needs an id");
        };
        let Some(b) = self.box_of(id) else {
            return fail(&format!("no view {id} on screen"));
        };
        let at = match request.get("at").and_then(|a| a.as_array()) {
            Some(a) => match (
                a.first().and_then(|v| v.as_f64()),
                a.get(1).and_then(|v| v.as_f64()),
            ) {
                (Some(x), Some(y)) => (b.rect.0 + x as f32, b.rect.1 + y as f32),
                _ => return fail("at needs two finite numbers"),
            },
            None => (b.rect.0 + b.rect.2 / 2., b.rect.1 + b.rect.3 / 2.),
        };
        let container = self.hit(at.0, at.1).and_then(|hit| self.zoomable_from(hit));
        if let Some(scale) = pinch {
            let Some(scale) = scale.as_f64().filter(|s| s.is_finite() && *s > 0.) else {
                return fail("pinch: expected a positive finite scale");
            };
            let previous = self.pointer_kind;
            self.set_pointer_kind(PointerKind::Touch);
            let took = self.drive_transform_pinch(at, scale);
            self.set_pointer_kind(previous);
            match took {
                Ok(true) => return Some(serde_json::json!({"tapped":id, "at":[at.0,at.1], "pinch":scale, "delivery":"presenter", "native":"Presenter.drive_transform_pinch"}).to_string()),
                Err(e) => return fail(&e),
                Ok(false) => {}
            }
            let Some(container) = self.zoomable_under(at.0, at.1) else {
                return fail(&format!("unsupported: no zoomable scroller or eligible transform binding under view {id}"));
            };
            self.zoom_pinch_began(container);
            let step = (scale as f32).powf(1. / 8.);
            for _ in 0..8 {
                self.zoom_pinched(container, step, at, at);
            }
            self.zoom_pinch_ended(container);
            return Some(
                serde_json::json!({
                    "tapped": id, "at": [at.0, at.1], "pinch": scale,
                    "scrollZoom": self.reported_zoom(container),
                    "delivery": "presenter", "native": "Presenter.zoom_pinch_*",
                    "timing": "deterministic pinch samples; native touch unobserved"
                })
                .to_string(),
            );
        }
        let previous = self.pointer_kind;
        self.set_pointer_kind(PointerKind::Touch);
        let shell = std::mem::replace(&mut self.zooms.shell_taps, false);
        let now = self.host.now();
        let mut error = None;
        for _ in 0..2 {
            let down = self.pointer_down(at.0, at.1, now).err();
            let up = self.pointer_up(at.0, at.1, now).err();
            error = error.or(down).or(up);
        }
        self.zooms.shell_taps = shell;
        self.set_pointer_kind(previous);
        if let Some(e) = error {
            return fail(&e);
        }
        let zoom = container.map(|c| self.reported_zoom(c));
        Some(
            serde_json::json!({
                "tapped": id, "at": [at.0, at.1], "dblclick": true, "touch": true,
                "scrollZoom": zoom, "native": "Presenter.pointer_down/up, twice"
            })
            .to_string(),
        )
    }
}

#[cfg(test)]
#[path = "zoom_tests.rs"]
mod tests;
