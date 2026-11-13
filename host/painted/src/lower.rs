//! Animations the Canvas host's reader plays (LLP 1076 on Android, after LLP
//! 1055 D7's lowering on Apple): an `opacity`, `translate`, `scale` or
//! `rotate` animation of a box, an `opacity` or `r` animation of a filled
//! circle, and a stroked shape's `stroke-dashoffset` (a draw-in: the reader
//! records that one path again with the dash phase), are not sampled by the
//! engine. The painter records the node once
//! into a layer at its underlying values ([`Presented::lowered`]); the reader
//! samples the keyframes this module encodes ([`Host::layer_tracks`]) every
//! display frame and moves or fades the layer, as a browser's compositor
//! plays them without the main thread. Rows are not recorded again for them.
//!
//! A node is lowered only when every animation on it is, and nothing the
//! layer cannot show depends on the values: a turned SVG element (its
//! transform is its scene's), a stroked circle (its stroke would scale), a
//! filter, mask, clip or blend, a paint server, a `light-dark()` keyframe, a
//! drag timeline. Anything else is sampled as before.

use super::Host;
#[cfg(target_os = "android")]
use crate::paint::Presented;
use exact_kernel::motion::MotionSync;
use exact_kernel::svg::Paint;
use exact_kernel::{Dimension, NodeKey, NodeType};
#[cfg(any(target_os = "android", test))]
use exact_motion::{Easing, StepPosition};
use exact_motion::{PlayedCurve, PlayedTransition, Property, Value};
use exact_runner::DataSource;

#[cfg(test)]
#[path = "lower_tests.rs"]
mod tests;

/// Set in a layer key ([`spinner_layer`]): Material's indeterminate
/// indicator a `progress` draws, apart from the node's own layer.
const SPINNER: u64 = 1 << 63;

/// The layer key of `key`'s indeterminate indicator (`paint/control.rs`).
pub(crate) fn spinner_layer(key: NodeKey) -> u64 {
    node_u64(key) | SPINNER
}

/// Whether a layer key is an indicator's ([`spinner_layer`]).
#[cfg(target_os = "android")]
pub(crate) fn is_spinner(layer: u64) -> bool {
    layer & SPINNER != 0
}

/// Material's indeterminate indicator, as Compose's `CircularProgressIndicator`
/// moves it, in cycles of 1332 ms: the head runs 290° ahead of the tail in
/// the first half, the tail catches up in the second, each along
/// `cubic-bezier(0.4, 0, 0.2, 1)`, and under both the arc turns 286°.
/// Compose steps it on 216° at each cycle's start, which is where the cycle
/// before ended (286° + 290° = 576° is 216° and a whole turn), so the
/// reader's rotation runs on without a jump.
#[cfg(any(target_os = "android", test))]
const SPINNER_CYCLE: f64 = 1.332;
const SPINNER_HEAD: f64 = 290.0;
#[cfg(any(target_os = "android", test))]
const SPINNER_TURN: f64 = 286.0;
/// The dash phase, as a fraction of the circle, of the arc at its longest:
/// what a still frame draws.
pub(crate) const SPINNER_REST: f64 = 1.0 - SPINNER_HEAD / 360.0;

/// The properties a reader plays.
pub(crate) const LOWERED: [Property; 8] = [
    Property::Opacity,
    Property::Translate,
    Property::Scale,
    Property::Rotate,
    Property::R,
    Property::StrokeDashoffset,
    Property::Cx,
    Property::Cy,
];

/// [`Presented::lowered`] bits: the reader plays the node's opacity…
pub const LOWER_OPACITY: u8 = 1;
/// …its `translate`, `rotate` and `scale` (all three, about its origin)…
pub const LOWER_TRANSFORM: u8 = 2;
/// …or a filled circle's radius, as a scale about its centre…
pub const LOWER_R: u8 = 4;
/// …or a stroked shape's dash offset, its one path recorded again…
pub const LOWER_DASH: u8 = 8;
/// …or a circle's centre, as a translation from where it is drawn…
pub const LOWER_MOVE: u8 = 16;
/// …or a text's `color` or a box's `background-color` transition: the
/// layer recorded again by its reader with the colour of each frame (the
/// row is not, and the engine paints nothing while it runs).
pub const LOWER_COLOR: u8 = 32;

/// Track property codes on the wire.
#[cfg(target_os = "android")]
const OPACITY: u32 = 0;
#[cfg(target_os = "android")]
const TX: u32 = 1;
#[cfg(target_os = "android")]
const TY: u32 = 2;
#[cfg(target_os = "android")]
const SCALE: u32 = 3;
#[cfg(any(target_os = "android", test))]
const ROTATE: u32 = 4;
#[cfg(target_os = "android")]
const R: u32 = 5;
#[cfg(any(target_os = "android", test))]
const DASH: u32 = 6;
#[cfg(target_os = "android")]
const CX: u32 = 7;
#[cfg(target_os = "android")]
const CY: u32 = 8;
/// A text's colour: four tracks from here, straight red, green, blue and
/// alpha, 0-255.
#[cfg(target_os = "android")]
const COLOR: u32 = 9;
/// A box's background colour, likewise.
#[cfg(target_os = "android")]
const BACKGROUND: u32 = 13;
/// Keyframes a colour transition is sent as: its values along the engine's
/// own interpolation (premultiplied, or OKLab), straight 8-bit channels the
/// reader joins linearly.
#[cfg(target_os = "android")]
const COLOR_STEPS: usize = 8;

impl<D: DataSource> Host<D> {
    /// Lower the reader's properties when the painter is the Canvas host's
    /// (`EXACT_PAINTER=canvas`), unless `EXACT_LOWER=0`. Before the boot
    /// tree's animations start, so none starts sampled.
    pub(super) fn lowering_from_env(&mut self) {
        self.lowering = std::env::var("EXACT_PAINTER").is_ok_and(|p| p == "canvas")
            && !std::env::var("EXACT_LOWER").is_ok_and(|v| v == "0");
        if self.lowering {
            self.engine.set_lowered_properties(&LOWERED);
        }
        // `EXACT_LOWER_COLOR=0`: colour transitions sampled and painted by the
        // engine every frame, to compare.
        self.lower_colors =
            self.lowering && !std::env::var("EXACT_LOWER_COLOR").is_ok_and(|v| v == "0");
    }

    /// Per node a sync set animations on: whether the reader can play them
    /// all, else the engine samples them; what it plays, kept by node.
    pub(super) fn lower_eligibility(&mut self, sync: &MotionSync) {
        if !self.lowering {
            return;
        }
        for node in &sync.removed {
            self.lowered_changed.remove(node);
            if self.lowered.remove(node).is_some() | self.played.remove(node).is_some() {
                self.lowered_epoch += 1;
            }
        }
        for (node, animations) in &sync.animations {
            let key = NodeKey {
                index: *node as u32,
                generation: (*node >> 32) as u32,
            };
            let props = animations.properties();
            let playable = !props.is_empty()
                && props.iter().all(|p| LOWERED.contains(p))
                && !animations
                    .0
                    .iter()
                    .any(|a| a.keyframes.0.iter().any(|f| !f.dark.is_empty()))
                && !self.engine.timeline_bound(*node)
                && self
                    .runner
                    .kernel()
                    .node_by_key(key)
                    .is_some_and(|n| playable(&n, &props));
            self.engine.set_node_sampled(*node, !playable);
            let mask = if playable { mask(&props) } else { 0 };
            let old = if mask == 0 {
                self.lowered.remove(node)
            } else {
                self.lowered.insert(*node, mask)
            };
            // Tracks change with the plays: a new start, a pause.
            if old.is_some() || mask != 0 {
                self.lowered_epoch += 1;
                self.lowered_changed.insert(*node, self.lowered_epoch);
            }
        }
    }

    /// The [`Presented::lowered`] bits of a node: what its reader plays.
    pub(crate) fn lowered_mask(&self, key: NodeKey) -> u8 {
        if !self.lowering || (self.lowered.is_empty() && self.played.is_empty()) {
            return 0;
        }
        let node = node_u64(key);
        let played = self
            .played
            .get(&node)
            .map_or(0, |t| mask(&t.iter().map(|(p, _)| *p).collect::<Vec<_>>()));
        self.lowered.get(&node).copied().unwrap_or(0) | played
    }

    /// Hand the reader every transition it can play (opacity, translate,
    /// scale, rotate of a node it can layer): the engine presents their
    /// targets from now on and the reader moves the layer. Before the
    /// presentation, so the painter draws the target. Ended or reclaimed
    /// tracks go.
    pub(super) fn play_transitions(&mut self) {
        if !self.lowering {
            return;
        }
        let now = self.engine.now();
        // A track the reader plays whose node now has something painted over
        // the property (a press row, a look naming it, set while it played)
        // is the engine's again, from where its curve is (LLP 1121 D8).
        for (node, t) in &self.played {
            let Some(n) = self
                .runner
                .kernel()
                .node_by_key(exact_kernel::motion::node_key(*node))
            else {
                continue;
            };
            for (property, _) in t {
                if crate::host::press::paints_over(&n, *property) {
                    self.engine.reclaim_played(*node, *property);
                }
            }
        }
        let mut ended = Vec::new();
        self.played.retain(|node, t| {
            let before = t.len();
            t.retain(|(property, p)| {
                self.engine.is_played(*node, *property) && p.start + played_seconds(p) > now
            });
            if t.len() != before {
                ended.push(*node);
            }
            !t.is_empty()
        });
        let mut changed = !ended.is_empty();
        let keys: Vec<(u64, Property)> = self
            .engine
            .running_transitions()
            .filter(|(_, p)| {
                matches!(
                    p,
                    Property::Opacity
                        | Property::Translate
                        | Property::Scale
                        | Property::Rotate
                        | Property::Color
                        | Property::BackgroundColor
                ) && (self.lower_colors
                    || !matches!(p, Property::Color | Property::BackgroundColor))
            })
            .collect();
        for (node, property) in keys {
            let key = exact_kernel::motion::node_key(node);
            let layered = self
                .runner
                .kernel()
                .node_by_key(key)
                .is_some_and(|n| playable(&n, &[property]) && recolors(&n, property));
            if !layered {
                continue;
            }
            if let Some(t) = self.engine.play_transition(node, property) {
                let list = self.played.entry(node).or_default();
                list.retain(|(p, _)| *p != property);
                list.push((property, t));
                ended.push(node);
                changed = true;
            }
        }
        if changed {
            self.lowered_epoch += 1;
            for node in ended {
                self.lowered_changed.insert(node, self.lowered_epoch);
            }
        }
    }

    /// Sample the reader's curve before a hold takes it. The painter and
    /// engine publish the target while the reader plays these values.
    pub(super) fn played_value(&self, node: u64, property: Property, now: f64) -> Option<Value> {
        if !self.engine.is_played(node, property) {
            return None;
        }
        let (_, t) = self
            .played
            .get(&node)?
            .iter()
            .find(|(p, _)| *p == property)?;
        let duration = played_seconds(t);
        if now >= t.start + duration {
            return Some(t.to);
        }
        let progress = if duration <= 0.0 {
            0.0
        } else {
            ((now - t.start) / duration).clamp(0.0, 1.0)
        };
        Some(match &t.curve {
            PlayedCurve::Easing { easing, .. } => {
                let progress = easing.progress(progress);
                t.from.zip(t.to, |a, b| a + (b - a) * progress)
            }
            PlayedCurve::Frames { values, .. } => {
                let position = progress * values.len().saturating_sub(1) as f64;
                let i = position as usize;
                let from = values.get(i)?;
                let to = values.get(i + 1).unwrap_or(from);
                from.zip(*to, |a, b| a + (b - a) * position.fract())
            }
        })
    }

    /// Bumped whenever a lowered node's plays may have changed: the reader's
    /// tracks are encoded again only then.
    #[cfg(target_os = "android")]
    pub(crate) fn lowered_epoch(&self) -> u64 {
        self.lowered_epoch
    }

    /// Whether `key`'s plays changed after `epoch` ([`Host::lowered_epoch`]):
    /// its tracks are encoded again only then, not every layer's at each
    /// change of any (rows mounting in a fling start their own).
    #[cfg(target_os = "android")]
    pub(crate) fn lowered_changed_after(&self, key: NodeKey, epoch: u64) -> bool {
        self.lowered_changed
            .get(&node_u64(key))
            .is_some_and(|at| *at > epoch)
    }

    /// A lowered node's tracks for its reader, as words: per animation and
    /// property (a translation is two, x and y), `[property, start, hold?,
    /// hold, delay, duration, iterations, direction, fill, easing…, frames,
    /// (offset, value, easing…)…]` with every number an `f32`'s bits, the
    /// keyframes with CSS's implicit ones from the underlying `base`. Easing:
    /// `0` none (the animation's), `1` cubic + 4, `2` steps + count +
    /// position, `3` linear() + n + n pairs. Empty when nothing is lowered.
    #[cfg(target_os = "android")]
    pub(crate) fn layer_tracks(&self, key: NodeKey, base: &Presented) -> Vec<u32> {
        let mut out = Vec::new();
        if self.lowered_mask(key) == 0 {
            return out;
        }
        let node = self.runner.kernel().node_by_key(key);
        let r0 = node
            .as_ref()
            .and_then(|n| match n.style.r {
                Dimension::Points(r) => Some(r),
                _ => None,
            })
            .unwrap_or(0.0);
        let point = |d: &Dimension| match d {
            Dimension::Points(v) => *v,
            _ => 0.0,
        };
        let (cx0, cy0) = node
            .as_ref()
            .map_or((0.0, 0.0), |n| (point(&n.style.cx), point(&n.style.cy)));
        // Only for a dash track: the one row, where it is set.
        let dash0 = || {
            node.as_ref().map_or(0.0, |n| {
                n.computed_row(exact_kernel::StyleId::StrokeDashoffset, |s| {
                    s.stroke_dashoffset
                })
            })
        };
        // An animation the engine samples (a look's or a press's opacity over
        // it, `playable`) is in the recording and the layer's base already:
        // never a reader track too, beside a transition a sibling lowered.
        let sampled = self.engine.node_sampled(node_u64(key));
        for play in self
            .engine
            .animation_plays(node_u64(key))
            .into_iter()
            .filter(|_| !sampled)
        {
            let a = &play.animation;
            for p in a.keyframes.properties() {
                let (underlying, axes): (Value, &[(u32, usize)]) = match p {
                    Property::Opacity => (Value::scalar(base.opacity as f64), &[(OPACITY, 0)]),
                    Property::Translate => (
                        Value::new(base.translate.0 as f64, base.translate.1 as f64),
                        &[(TX, 0), (TY, 1)],
                    ),
                    Property::Scale => (Value::scalar(base.scale as f64), &[(SCALE, 0)]),
                    Property::Rotate => (Value::scalar(base.rotate as f64), &[(ROTATE, 0)]),
                    Property::R => (Value::scalar(r0 as f64), &[(R, 0)]),
                    Property::StrokeDashoffset => (Value::scalar(dash0() as f64), &[(DASH, 0)]),
                    Property::Cx => (Value::scalar(cx0 as f64), &[(CX, 0)]),
                    Property::Cy => (Value::scalar(cy0 as f64), &[(CY, 0)]),
                    _ => continue,
                };
                let track = a.keyframes.track(p, underlying, play.dark, &a.easing);
                for &(code, axis) in axes {
                    out.push(code);
                    f(&mut out, play.start);
                    out.push(play.hold.is_some() as u32);
                    f(&mut out, play.hold.unwrap_or(0.0));
                    f(&mut out, a.delay);
                    f(&mut out, a.duration);
                    f(&mut out, a.iterations);
                    out.push(a.direction as u32);
                    out.push(a.fill as u32);
                    easing(&mut out, Some(&a.easing));
                    out.push(track.len() as u32);
                    for (offset, e, v) in &track {
                        f(&mut out, *offset);
                        f(&mut out, if axis == 1 { v.y } else { v.x });
                        easing(&mut out, Some(*e));
                    }
                }
            }
        }
        // Transitions after animations: CSS's cascade puts them above.
        for (p, t) in self.played.get(&node_u64(key)).into_iter().flatten() {
            let axes: &[(u32, usize)] = match p {
                Property::Opacity => &[(OPACITY, 0)],
                Property::Translate => &[(TX, 0), (TY, 1)],
                Property::Scale => &[(SCALE, 0)],
                Property::Rotate => &[(ROTATE, 0)],
                Property::Color | Property::BackgroundColor => {
                    let code = if *p == Property::Color {
                        COLOR
                    } else {
                        BACKGROUND
                    };
                    color_tracks(&mut out, code, t);
                    continue;
                }
                _ => continue,
            };
            let axis = |v: &Value, a: usize| if a == 1 { v.y } else { v.x };
            for &(code, a) in axes {
                out.push(code);
                f(&mut out, t.start);
                out.push(0);
                f(&mut out, 0.0);
                f(&mut out, 0.0);
                f(&mut out, played_seconds(t));
                f(&mut out, 1.0);
                out.push(0);
                // Backwards: during its delay it shows where it starts.
                out.push(2);
                match &t.curve {
                    PlayedCurve::Easing { easing: e, .. } => {
                        easing(&mut out, Some(e));
                        out.push(2);
                        for (offset, v) in [(0.0, &t.from), (1.0, &t.to)] {
                            f(&mut out, offset);
                            f(&mut out, axis(v, a));
                            out.push(0);
                        }
                    }
                    PlayedCurve::Frames { values, .. } => {
                        easing(&mut out, Some(&Easing::Linear));
                        out.push(values.len() as u32);
                        let last = values.len().saturating_sub(1).max(1) as f64;
                        for (i, v) in values.iter().enumerate() {
                            f(&mut out, i as f64 / last);
                            f(&mut out, axis(v, a));
                            out.push(0);
                        }
                    }
                }
            }
        }
        out
    }
}

impl<D: DataSource> Host<D> {
    /// The tracks of an indicator's layer ([`spinner_layer`]) for its
    /// reader ([`spinner_words`]): held still at its longest arc while the
    /// device removes animations (Android's "Remove animations", which
    /// reaches the page as `prefers-reduced-motion`), as Compose's indicator
    /// stands still at a zero animator duration scale. None when the reader
    /// plays no layers (the agent's held clock, `EXACT_LOWER=0`), which then
    /// shows the arc still as drawn. The words change only with the setting,
    /// so a host sends them again exactly when it changes.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn spinner_tracks(&self) -> Vec<u32> {
        if !self.lowering {
            return Vec::new();
        }
        spinner_words(self.runner.viewport().preferences.reduced_motion)
    }
}

/// An indicator's tracks, as [`Host::layer_tracks`] writes a node's: from
/// the reader clock's zero, so every indicator turns in step and the words
/// are the same at every paint; or, `still`, each held where the drawing
/// already is (no turn, the longest arc).
///
/// The arc is the visible stretch of a dash (`paint/control.rs`): a phase
/// of `1 - sweep / 360` circles. The layer's rotation is where the tail is:
/// the turns so far plus the tail's own run. In a cycle's first half that
/// is the turn alone, linear; in the second it adds the tail's eased run,
/// and a linear and an eased curve over the same span sum to one cubic
/// Bézier whose output control points are theirs, weighted. Five cycles
/// turn 5 × 576° = 2880°, eight whole turns, so the rotation track repeats
/// every five.
#[cfg(any(target_os = "android", test))]
fn spinner_words(still: bool) -> Vec<u32> {
    let mut out = Vec::new();
    let ease = Easing::CubicBezier {
        x1: 0.4,
        y1: 0.0,
        x2: 0.2,
        y2: 1.0,
    };
    let header = |out: &mut Vec<u32>, code: u32, cycles: f64, rest: f64| {
        out.extend([code, 0, u32::from(still)]);
        f(out, if still { rest } else { 0.0 });
        f(out, 0.0);
        f(out, SPINNER_CYCLE * cycles);
        f(out, f64::INFINITY);
        out.extend([0, 3, 0]);
    };
    let shortest = SPINNER_REST;
    header(&mut out, DASH, 1.0, SPINNER_CYCLE / 2.0);
    out.push(3);
    for (offset, value) in [(0.0, 1.0), (0.5, shortest), (1.0, 1.0)] {
        f(&mut out, offset);
        f(&mut out, value);
        easing(&mut out, Some(&ease));
    }
    let half = SPINNER_TURN / 2.0;
    let second = half + SPINNER_HEAD;
    let catch = Easing::CubicBezier {
        x1: 0.4,
        y1: 0.4 * half / second,
        x2: 0.2,
        y2: (0.2 * half + SPINNER_HEAD) / second,
    };
    header(&mut out, ROTATE, 5.0, 0.0);
    out.push(11);
    for cycle in 0..5 {
        let at = cycle as f64 * (SPINNER_TURN + SPINNER_HEAD);
        let offset = cycle as f64 / 5.0;
        f(&mut out, offset);
        f(&mut out, at);
        easing(&mut out, Some(&Easing::Linear));
        f(&mut out, offset + 0.1);
        f(&mut out, at + half);
        easing(&mut out, Some(&catch));
    }
    f(&mut out, 1.0);
    f(&mut out, 5.0 * (SPINNER_TURN + SPINNER_HEAD));
    out.push(0);
    out
}

/// A played transition's length, seconds.
fn played_seconds(t: &PlayedTransition) -> f64 {
    match &t.curve {
        PlayedCurve::Easing { duration, .. } | PlayedCurve::Frames { duration, .. } => *duration,
    }
}

/// The [`Presented::lowered`] bits for these animated properties.
fn mask(props: &[Property]) -> u8 {
    props.iter().fold(0, |m, p| {
        m | match p {
            Property::Opacity => LOWER_OPACITY,
            Property::Translate | Property::Scale | Property::Rotate => LOWER_TRANSFORM,
            Property::R => LOWER_R,
            Property::StrokeDashoffset => LOWER_DASH,
            Property::Cx | Property::Cy => LOWER_MOVE,
            Property::Color | Property::BackgroundColor => LOWER_COLOR,
            _ => 0,
        }
    })
}

/// Whether a reader can show `property`'s transition on `n` by recording its
/// layer again in each frame's colour: a text without inline runs (every
/// glyph run is its one colour), or a box's own background (a flat colour).
/// Any other property: yes.
fn recolors(n: &exact_kernel::NodeRef<'_>, property: Property) -> bool {
    match property {
        Property::Color => n.node_type == NodeType::Text && n.children().is_empty(),
        Property::BackgroundColor => {
            !n.node_type.is_svg_element()
                && n.node_type != NodeType::Text
                && n.style.background_image == Default::default()
        }
        _ => true,
    }
}

/// A colour transition as four tracks from `code` (red, green, blue, alpha;
/// straight, 0-255): [`COLOR_STEPS`] linear pieces along its curve.
#[cfg(target_os = "android")]
fn color_tracks(out: &mut Vec<u32>, code: u32, t: &PlayedTransition) {
    let values: Vec<[u8; 4]> = match &t.curve {
        PlayedCurve::Easing { .. } => (0..=COLOR_STEPS)
            .map(|i| {
                let k = i as f64 / COLOR_STEPS as f64;
                t.from.zip(t.to, |a, b| a + (b - a) * k).to_rgba8()
            })
            .collect(),
        PlayedCurve::Frames { values, .. } => values.iter().map(|v| v.to_rgba8()).collect(),
    };
    let last = values.len().saturating_sub(1).max(1) as f64;
    for channel in 0..4 {
        out.push(code + channel as u32);
        f(out, t.start);
        out.push(0);
        f(out, 0.0);
        f(out, 0.0);
        f(out, played_seconds(t));
        f(out, 1.0);
        out.push(0);
        out.push(2);
        match &t.curve {
            PlayedCurve::Easing { easing: e, .. } => easing(out, Some(e)),
            PlayedCurve::Frames { .. } => easing(out, Some(&Easing::Linear)),
        }
        out.push(values.len() as u32);
        for (i, v) in values.iter().enumerate() {
            f(out, i as f64 / last);
            f(out, v[channel] as f64);
            // Linear between the samples: the curve is the whole track's.
            easing(out, Some(&Easing::Linear));
        }
    }
}

fn node_u64(key: NodeKey) -> u64 {
    (key.generation as u64) << 32 | key.index as u64
}

#[cfg(any(target_os = "android", test))]
fn f(out: &mut Vec<u32>, v: f64) {
    out.push((v as f32).to_bits());
}

#[cfg(any(target_os = "android", test))]
fn easing(out: &mut Vec<u32>, e: Option<&Easing>) {
    let cubic = |out: &mut Vec<u32>, c: [f64; 4]| {
        out.push(1);
        for v in c {
            f(out, v);
        }
    };
    match e {
        None => out.push(0),
        Some(Easing::Linear) => cubic(out, [0.0, 0.0, 1.0, 1.0]),
        Some(Easing::Ease) => cubic(out, [0.25, 0.1, 0.25, 1.0]),
        Some(Easing::EaseIn) => cubic(out, [0.42, 0.0, 1.0, 1.0]),
        Some(Easing::EaseOut) => cubic(out, [0.0, 0.0, 0.58, 1.0]),
        Some(Easing::EaseInOut) => cubic(out, [0.42, 0.0, 0.58, 1.0]),
        Some(Easing::CubicBezier { x1, y1, x2, y2 }) => cubic(out, [*x1, *y1, *x2, *y2]),
        Some(Easing::Steps { count, position }) => {
            out.extend([2, *count as u32]);
            out.push(match position {
                StepPosition::JumpStart => 0,
                StepPosition::JumpEnd => 1,
                StepPosition::JumpNone => 2,
                StepPosition::JumpBoth => 3,
            });
        }
        Some(Easing::PiecewiseLinear(stops)) => {
            out.extend([3, stops.len() as u32]);
            for s in stops {
                f(out, s.input);
                f(out, s.output);
            }
        }
    }
}

/// Whether the reader can show these animated properties of `n` as CSS
/// does: a box's transform and opacity; an SVG element's opacity; a filled
/// circle's radius as a scale about its centre.
fn playable(n: &exact_kernel::NodeRef<'_>, props: &[Property]) -> bool {
    // What the host paints over the node's own (a press, a look: LLP 1061 D7,
    // LLP 1121 D8) is composed where the engine samples, so it stays there.
    if props.iter().any(|p| crate::host::press::paints_over(n, *p)) {
        return false;
    }
    let s = n.style;
    let svg = n.node_type.is_svg_element();
    let served = |p: &Paint| matches!(p, Paint::Url(..));
    let turns = props
        .iter()
        .any(|p| matches!(p, Property::Translate | Property::Scale | Property::Rotate));
    if svg {
        // The scene keeps an animated element's transform in parts about its
        // origin; a non-scaling stroke would scale, a clip or effect would not
        // move, and a blend reads the backdrop, which a layer does not hold.
        let moves = props
            .iter()
            .any(|p| matches!(p, Property::Cx | Property::Cy));
        if !s.filter.is_none()
            || s.mix_blend_mode != exact_kernel::MixBlendMode::Normal
            || s.rare.svg_mask.url().is_some()
            || s.rare.clip_path.url().is_some()
            || served(&s.fill)
            || served(&s.stroke)
            || (turns && s.vector_effect == exact_kernel::VectorEffect::NonScalingStroke)
        {
            return false;
        }
        let point = |d: Dimension| matches!(d, Dimension::Points(_));
        // A circle's centre is a translation of its drawing, when its centre
        // is a length; not with a turn (two pivots).
        if moves
            && (n.node_type != NodeType::SvgCircle
                || turns
                || !point(s.cx.clone())
                || !point(s.cy.clone()))
        {
            return false;
        }
        // A radius scales the whole drawing: a fill alone. A dash offset is one
        // stroked path's phase; the two in one layer would scale the dash.
        if props.contains(&Property::R) {
            return n.node_type == NodeType::SvgCircle
                && !turns
                && matches!(s.stroke, Paint::None)
                && !props.contains(&Property::StrokeDashoffset)
                && matches!(s.r, Dimension::Points(r) if r > 0.0);
        }
        if props.contains(&Property::StrokeDashoffset) {
            return !turns && n.node_type.is_svg_shape() && !matches!(s.stroke, Paint::None);
        }
        return true;
    }
    !props.contains(&Property::R)
}
