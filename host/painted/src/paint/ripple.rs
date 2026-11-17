//! Material's press (LLP 1115.001 D9): a ripple from the touch, bounded by
//! the shape of what was pressed, in its content colour at the pressed
//! state's opacity. Compose Material 3 draws a press with the framework's
//! `RippleDrawable` (`RippleHostView`), whose Android 12 wave this is
//! (`RippleShader`, `RippleAnimationSession`): its progress runs to 0.5
//! over 450 ms on `FAST_OUT_SLOW_IN` while the finger is down, and on to 1
//! over 375 ms, linearly, once it has lifted and those 450 ms are over. At
//! progress `p` the wave is a soft circle (opaque to half its radius, gone
//! at one and a half) of radius `2.3 R p`, `R` Compose's bounded radius
//! (half the box's diagonal and 10 dp); its centre moves from the touch to
//! the box's middle by `p = 0.5`; it fades in by `p = 0.13` and out from
//! `p = 0.4`. A press shows no state layer beside it: Compose's state layer
//! is a hover's, a focus's and a drag's. The wave's sparkles are not drawn.
//!
//! In what scrolls, a press shows 100 ms after the touch, the platform's
//! tap timeout (`ViewConfiguration.getTapTimeout`, Compose's
//! `TapIndicationDelay`), so a touch that a scroller takes within it shows
//! none; one that lifts sooner shows at its lift.
//!
//! A ripple is painted per frame from the host's clock. With animations
//! removed (`prefers-reduced-motion`, as Android's "Remove animations"
//! ends an animator at once) and on the agent's clock, a press is the wave
//! at rest, `p = 0.5`, and a release takes it away.
use super::{gradient::GradientPaint, Backend, Painter, Rect4, Shape};
use exact_kernel::gradient::Geometry;
use exact_kernel::{Color, NodeKey, NodeRef, NodeType, ViewId};
use std::collections::HashMap;
use tiny_skia::Transform;

/// The wave's way to rest, ms.
const ENTER_MS: f64 = 450.0;
/// Its way out after the release, ms.
const EXIT_MS: f64 = 375.0;
/// The tap timeout a press in what scrolls waits before it shows, ms.
const TAP_DELAY_MS: f64 = 100.0;
/// Compose's `BoundedRippleExtraRadius`, dp.
const EXTRA_RADIUS: f32 = 10.0;
/// A checkbox's and a radio's state layer, dp: a circle round the mark.
const MARK_LAYER: f32 = 40.0;
/// `RippleShader.setRadius`: the shader's radius is the ripple's times this.
const SPREAD: f32 = 2.3;

/// One press's ripple.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Ripple {
    /// The touch, from the pressed box's top left; `None`: its middle.
    at: Option<(f32, f32)>,
    start: f64,
    /// When the finger lifted, ms after `start`.
    released: Option<f64>,
}

impl Ripple {
    /// A press at `at` from its box's top left (`None`: no point, its
    /// middle), now.
    pub(crate) fn new(at: Option<(f32, f32)>, now: f64) -> Self {
        Ripple {
            at,
            start: now,
            released: None,
        }
    }

    /// The finger lifted (`cancelled`: its press was cancelled, a scroller
    /// took it). One lifted before it showed shows now; one cancelled
    /// before then never shows, and is gone (false).
    pub(crate) fn release(&mut self, now: f64, cancelled: bool) -> bool {
        if now < self.start {
            if cancelled {
                return false;
            }
            self.start = now;
        }
        if self.released.is_none() {
            self.released = Some(now - self.start);
        }
        true
    }

    /// When it comes to rest (held) or is gone (released), ms.
    fn settles(&self) -> f64 {
        match self.released {
            Some(r) => self.start + r.max(ENTER_MS) + EXIT_MS,
            None => self.start + ENTER_MS,
        }
    }

    /// Whether it moves at `now` (`still`: none does).
    pub(crate) fn moving(&self, now: f64, still: bool) -> bool {
        !still && (self.released.is_some() || now < self.settles())
    }

    /// Whether it is over at `now`.
    pub(crate) fn gone(&self, now: f64, still: bool) -> bool {
        self.released.is_some() && (still || now >= self.settles())
    }

    /// The agent's clock: at rest now; false once released, when it goes.
    pub(crate) fn land(&mut self, now: f64) -> bool {
        self.start = now - ENTER_MS;
        self.released.is_none()
    }

    /// Its wave at `now` in the box `rect`.
    pub(crate) fn wave(&self, rect: Rect4, now: f64, still: bool) -> Wave {
        let (x, y, w, h) = rect;
        let touch = self
            .at
            .map_or((x + w / 2.0, y + h / 2.0), |(dx, dy)| (x + dx, y + dy));
        wave(
            rect,
            touch,
            progress(now - self.start, self.released, still),
        )
    }
}

/// The ripples that show or are fading, by the node each is on.
#[derive(Default)]
pub(crate) struct Ripples {
    live: HashMap<ViewId, (NodeKey, Ripple)>,
    /// The latest input's time: the host's clock moves only with frames,
    /// and an idle page's is as old as its last one.
    input: f64,
}

/// What a ripple draws at an instant, in the coordinates of its box.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Wave {
    pub center: (f32, f32),
    pub radius: f32,
    /// The share of the pressed state's opacity shown.
    pub alpha: f32,
}

/// `cubic-bezier(0.4, 0, 0.2, 1)`, Android's `FAST_OUT_SLOW_IN`.
fn fast_out_slow_in(x: f64) -> f64 {
    exact_motion::Easing::CubicBezier {
        x1: 0.4,
        y1: 0.,
        x2: 0.2,
        y2: 1.,
    }
    .progress(x)
}

/// The wave's progress `elapsed` ms after the press, released `released`
/// ms after it, or at rest (`still`).
pub(crate) fn progress(elapsed: f64, released: Option<f64>, still: bool) -> f32 {
    let enter = 0.5 * fast_out_slow_in((elapsed / ENTER_MS).clamp(0., 1.));
    match released {
        None if still => 0.5,
        Some(_) if still => 1.0,
        None => enter as f32,
        Some(r) => {
            let out = r.max(ENTER_MS);
            match elapsed <= out {
                true => enter as f32,
                false => (0.5 + 0.5 * ((elapsed - out) / EXIT_MS).min(1.)) as f32,
            }
        }
    }
}

/// The wave at progress `p` in a box `rect` touched at `touch`.
pub(crate) fn wave(rect: Rect4, touch: (f32, f32), p: f32) -> Wave {
    let (x, y, w, h) = rect;
    let bound = (w.hypot(h) / 2.0 + EXTRA_RADIUS) * SPREAD;
    let middle = (x + w / 2.0, y + h / 2.0);
    let k = (2.0 * p).clamp(0.0, 1.0);
    let fade_in = (p / 0.13).clamp(0.0, 1.0);
    let fade_out = ((p - 0.4) / 0.6).clamp(0.0, 1.0);
    Wave {
        center: (
            touch.0 + (middle.0 - touch.0) * k,
            touch.1 + (middle.1 - touch.1) * k,
        ),
        radius: bound * p.clamp(0.0, 1.0),
        alpha: fade_in.min(1.0 - fade_out),
    }
}

/// The wave in `shape`, in `ink` at the pressed state's opacity: the soft
/// circle, `1 - smoothstep(0.5 r, 1.5 r, d)`, as stops along a radial
/// gradient that ends at 1.5 r, filled over the shape and so cut to it.
pub(crate) fn fill(
    backend: &mut dyn Backend,
    shape: &Shape,
    ink: [u8; 4],
    wave: Wave,
    ts: Transform,
) {
    let peak = f32::from(ink[3]) * super::material::pressed_opacity() * wave.alpha;
    if wave.radius <= 0.0 || peak < 0.5 {
        return;
    }
    let [r, g, b, _] = ink;
    let stops = (0..=6)
        .map(|i| {
            let t = i as f32 / 6.0;
            let a = peak * (1.0 - t * t * (3.0 - 2.0 * t));
            let c = Color(u32::from_be_bytes([r, g, b, a.round() as u8]));
            ((1.0 + 2.0 * t) / 3.0, c)
        })
        .collect();
    let reach = wave.radius * 1.5;
    let paint = GradientPaint {
        geometry: Geometry::Radial {
            center: wave.center,
            radii: (reach, reach),
        },
        repeat: false,
        stops,
    };
    backend.fill_gradient(shape, &paint, ts);
}

impl Ripples {
    /// Input came at `now`, in the host clock's domain.
    pub(crate) fn input(&mut self, now: f64) {
        self.input = self.input.max(now);
    }

    /// The time of what happens now: the latest input's, or the host's
    /// clock `host` where that is later.
    pub(crate) fn clock(&self, host: f64) -> f64 {
        self.input.max(host)
    }

    /// A press on `id` (`key`) at `at` from its box's top left (`None`: no
    /// point, its middle), now.
    pub(crate) fn press(&mut self, id: ViewId, key: NodeKey, at: Option<(f32, f32)>, now: f64) {
        self.live.insert(id, (key, Ripple::new(at, now)));
    }

    /// The press on `id` waits the tap timeout before it shows: it is in
    /// what scrolls.
    pub(crate) fn delay(&mut self, id: ViewId) {
        if let Some((_, r)) = self.live.get_mut(&id) {
            r.start += TAP_DELAY_MS;
        }
    }

    /// The finger on `id` lifted (`cancelled`: its press was cancelled).
    pub(crate) fn release(&mut self, id: ViewId, now: f64, cancelled: bool) {
        if let Some((_, r)) = self.live.get_mut(&id) {
            if !r.release(now, cancelled) {
                self.live.remove(&id);
            }
        }
    }

    /// The agent's clock: a press held is at rest and a released one gone,
    /// as its presses land (`Host::land_presses`).
    pub(crate) fn land(&mut self, now: f64) {
        self.live.retain(|_, (_, r)| r.land(now));
    }

    /// A new tree: none of the old one's presses shows.
    pub(crate) fn clear(&mut self) {
        self.live.clear();
    }

    /// The ripple on `node`: the one pressed on that node, not on another
    /// that had its view id.
    fn on(&self, node: &NodeRef<'_>) -> Option<&Ripple> {
        (self.live.get(&node.id))
            .filter(|(key, _)| *key == node.key)
            .map(|(_, r)| r)
    }

    /// The nodes a ripple is on, whose rows paint again.
    pub(crate) fn keys(&self) -> impl Iterator<Item = NodeKey> + '_ {
        self.live.values().map(|(key, _)| *key)
    }

    /// Whether a ripple moves at `now` (`still`: none does).
    pub(crate) fn moving(&self, now: f64, still: bool) -> bool {
        self.live.values().any(|(_, r)| r.moving(now, still))
    }

    /// When the last ripple comes to rest or goes, ms.
    pub(crate) fn settles_at(&self, now: f64) -> Option<f64> {
        (self.live.values())
            .map(|(_, r)| r.settles())
            .filter(|end| *end > now)
            .reduce(f64::max)
    }

    /// After a paint: the ripples that have gone.
    pub(super) fn painted(&mut self, now: f64, still: bool) {
        self.live.retain(|_, (_, r)| !r.gone(now, still));
    }
}

impl Painter {
    /// A pressable box under the Material look that is no control (a list's
    /// row, a card, a link): the hover's state layer over its `outer`
    /// shape, unless it has a `:hover` of its own, and its ripple, in its
    /// own `color`.
    pub(super) fn pressable(&mut self, node: &NodeRef<'_>, outer: &Shape, ts: Transform) {
        let (hovered, pressed) = (
            self.feedback.0 == Some(node.id),
            self.ripples.on(node).is_some() || self.held_press == Some(node.id),
        );
        if !(hovered || pressed)
            || !super::look::material()
            || matches!(node.node_type, NodeType::Control | NodeType::TextInput)
        {
            return;
        }
        let ink = super::rgba(node.text_color().resolve(self.dark));
        // An authored `:hover` replaces the platform's (LLP 1121 D8).
        let own_hover = || (node.looks().iter()).any(|(s, _)| *s == exact_kernel::LOOK_HOVER);
        if hovered && !own_hover() {
            let hover = super::material::hover_opacity();
            self.backend.fill(outer, super::look::fade(ink, hover), ts);
        }
        self.ripple(node, outer, ink, ts);
    }

    /// A checkbox's or a radio's press under the Material look: from the
    /// middle of the 40 dp circle round its mark, its state layer's
    /// (`CheckboxTokens`, `RadioButtonTokens.StateLayerSize`), as Compose's
    /// unbounded ripple there starts, `primary` while it is on and
    /// `onSurface` while off. `held`: a contact's toggle not yet committed.
    pub(super) fn mark_ripple(
        &mut self,
        node: &NodeRef<'_>,
        content: Rect4,
        held: Option<bool>,
        ts: Transform,
    ) {
        let switch = node.props.str(exact_kernel::PropId::AccessibilityRole) == Some("switch");
        if switch || !super::look::material() {
            return;
        }
        let on = (node.props.bool(exact_kernel::PropId::Checked))
            .or(held)
            .unwrap_or(false);
        let ink = match on {
            true => super::look::accents(super::control::accent(node, self.dark), self.dark).0,
            false => super::look::role("CanvasText", self.dark),
        };
        let (x, y, w, h) = content;
        let (cx, cy) = (x + w / 2.0, y + h / 2.0);
        let r = MARK_LAYER / 2.0;
        let circle = Shape::new((cx - r, cy - r, 2.0 * r, 2.0 * r), [r; 4]);
        self.ripple_in(node, &circle, ink, ts, true);
    }

    /// Material's press on `node`, drawn over `shape` in `ink`: a contact's
    /// ripple, or a held `active`'s at rest. Nothing where the author wrote
    /// a press look of their own (LLP 1121 D8), or under another look.
    pub(super) fn ripple(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        ink: [u8; 4],
        ts: Transform,
    ) {
        self.ripple_in(node, shape, ink, ts, false);
    }

    /// [`Painter::ripple`], from the shape's middle where `centred` (a
    /// shape that is not the pressed box, which the touch is in).
    fn ripple_in(
        &mut self,
        node: &NodeRef<'_>,
        shape: &Shape,
        ink: [u8; 4],
        ts: Transform,
        centred: bool,
    ) {
        if !super::look::material() || crate::host::press::shows(node) {
            return;
        }
        let (now, still) = (self.numerals.now, self.numerals.still);
        let wave = match self.ripples.on(node) {
            Some(r) if centred => Ripple { at: None, ..*r }.wave(shape.rect, now, still),
            Some(r) => r.wave(shape.rect, now, still),
            None if self.held_press == Some(node.id) => {
                Ripple::new(None, now - ENTER_MS).wave(shape.rect, now, still)
            }
            None => return,
        };
        // A ripple moves: its row is drawn by the walk, never from a recording.
        self.row_refuse();
        fill(self.backend.as_mut(), shape, ink, wave, ts);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The wave runs to rest in 450 ms while held, waits that long before it
    /// leaves however soon the finger lifts, and is gone 375 ms after.
    #[test]
    fn the_wave_rests_while_held_and_leaves_after_its_entrance() {
        assert_eq!(progress(0.0, None, false), 0.0);
        let half = 0.5 * fast_out_slow_in(0.5) as f32;
        assert!((progress(225.0, None, false) - half).abs() < 1e-6);
        assert_eq!(progress(450.0, None, false), 0.5);
        assert_eq!(progress(5000.0, None, false), 0.5);
        // A tap: released at 50 ms, it keeps entering to 450, then leaves.
        assert!(progress(300.0, Some(50.0), false) < 0.5);
        assert_eq!(progress(450.0, Some(50.0), false), 0.5);
        assert!((progress(450.0 + 187.5, Some(50.0), false) - 0.75).abs() < 1e-6);
        assert_eq!(progress(825.0, Some(50.0), false), 1.0);
        // Held a second: it leaves from the release.
        assert_eq!(progress(1000.0, Some(1000.0), false), 0.5);
        assert!((progress(1375.0, Some(1000.0), false) - 1.0).abs() < 1e-6);
        // Animations removed: at rest while held, gone once released.
        assert_eq!(progress(0.0, None, true), 0.5);
        assert_eq!(progress(0.0, Some(0.0), true), 1.0);
    }

    /// From the touch to the middle by half its progress, its radius 2.3
    /// times Compose's bounded one at full progress, faded in by 0.13 and
    /// out from 0.4.
    #[test]
    fn the_wave_grows_from_the_touch_to_the_middle() {
        let rect = (10.0, 20.0, 60.0, 80.0);
        let start = wave(rect, (15.0, 25.0), 0.0);
        assert_eq!(start.center, (15.0, 25.0));
        assert_eq!((start.radius, start.alpha), (0.0, 0.0));
        let half = wave(rect, (15.0, 25.0), 0.25);
        assert_eq!(half.center, (27.5, 42.5));
        assert_eq!(half.alpha, 1.0);
        let rest = wave(rect, (15.0, 25.0), 0.5);
        assert_eq!(rest.center, (40.0, 60.0));
        // R: half the diagonal (50) and 10 dp.
        assert!((rest.radius - 60.0 * 2.3 * 0.5).abs() < 1e-4);
        assert!((rest.alpha - (1.0 - 0.1 / 0.6)).abs() < 1e-6);
        // At rest the soft circle reaches past the corners: the box shows
        // the wave whole, softer towards them.
        assert!(rest.radius * 1.5 > 50.0);
        assert_eq!(wave(rect, (15.0, 25.0), 1.0).alpha, 0.0);
    }

    /// A ripple lives until its exit ends, then goes; a held one stays;
    /// the agent's clock lands each at its end.
    #[test]
    fn ripples_go_when_their_exit_ends_and_land_on_the_agents_clock() {
        let (a, b): (ViewId, ViewId) = (1, 2);
        let key = NodeKey {
            index: 7,
            generation: 0,
        };
        let mut r = Ripples::default();
        r.press(a, key, Some((1.0, 2.0)), 100.0);
        r.release(a, 150.0, false);
        assert!(r.moving(200.0, false));
        assert_eq!(r.settles_at(200.0), Some(100.0 + 450.0 + 375.0));
        r.painted(924.0, false);
        assert_eq!(r.keys().count(), 1);
        r.painted(925.0, false);
        assert_eq!(r.keys().count(), 0);
        r.press(a, key, None, 0.0);
        r.press(b, key, None, 0.0);
        r.release(b, 10.0, false);
        r.land(1000.0);
        assert_eq!(r.live.len(), 1);
        assert_eq!(r.live[&a].1.start, 550.0);
        assert!(!r.moving(1000.0, false));
        assert_eq!(r.settles_at(1000.0), None);
    }

    /// In what scrolls a press shows after the tap timeout: cancelled
    /// before, it never shows; lifted before, it shows from the lift.
    #[test]
    fn a_press_in_what_scrolls_waits_the_tap_timeout() {
        let key = NodeKey {
            index: 7,
            generation: 0,
        };
        let mut r = Ripples::default();
        r.press(1, key, None, 0.0);
        r.delay(1);
        assert_eq!(
            r.live[&1].1.wave((0., 0., 10., 10.), 50.0, false).alpha,
            0.0
        );
        r.release(1, 60.0, true);
        assert!(r.live.is_empty(), "a scroll took it: never shown");
        r.press(1, key, None, 0.0);
        r.delay(1);
        r.release(1, 60.0, false);
        let (_, ripple) = r.live[&1];
        assert_eq!(ripple.start, 60.0, "a tap shows from its lift");
        assert_eq!(ripple.settles(), 60.0 + 450.0 + 375.0);
        r.press(1, key, None, 0.0);
        r.delay(1);
        r.release(1, 300.0, true);
        assert_eq!(
            r.live[&1].1.settles(),
            100.0 + 450.0 + 375.0,
            "shown, it leaves"
        );
    }
}
