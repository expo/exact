//! Host-owned press feedback (LLP 1061 D7): a track per pressed property,
//! each sampling how far to blend toward the current pressed value, so that a
//! style commit can change that value without restarting the track. The scale
//! multiplies into `scale` about `transform-origin`; the opacity is a share of
//! the node's own, or `:active`'s absolute one (`press_opacity_to`), blended
//! from what shows. The user's motion preference never removes a press (LLP
//! 1061, the 2026-09-27 ruling).
use super::*;
use exact_kernel::StyleId;
use exact_motion::{Easing, SpringConfig, TimingFunction, Transition, TransitionProgress, Value};

/// What a press moves, each on its own timing (D7 "Timing").
const PROPERTIES: [Property; 5] = [
    Property::BackgroundColor,
    Property::Color,
    Property::Opacity,
    Property::Translate,
    Property::Scale,
];

/// How far each of [`PROPERTIES`] is toward its pressed value: 0 at rest, 1
/// pressed, and past either while a spring overshoots.
#[derive(Clone, Copy, Default)]
pub(super) struct Progress([f32; PROPERTIES.len()]);

impl Progress {
    fn of(self, property: Property) -> f32 {
        let i = PROPERTIES.iter().position(|p| *p == property);
        self.0[i.expect("a press property")]
    }
}

/// Where a press is (D7 "Timing" item 3). A held phase names what releases
/// it.
#[derive(Clone, Copy, PartialEq)]
enum Phase {
    /// Held. While some picture still has to show it, the press generation
    /// the first picture that does carries; a present of that picture clears
    /// it (`presses_presented`).
    Down(Option<u64>),
    /// Released before a picture showed it: held at the pressed values until
    /// a picture of `generation` or later presents (`presses_presented`), or
    /// until `deadline` for a display that never reports one, when it ends
    /// at once (`retire_presses`).
    Quick { generation: u64, deadline: f64 },
    /// Its pressed picture presented: the release starts as the next frame
    /// is painted (`release_quick_presses`).
    ReleaseNextFrame,
    /// Released, moving to rest on its timing.
    Up,
}

impl Phase {
    fn target(self) -> f64 {
        if self == Self::Up {
            0.
        } else {
            1.
        }
    }

    fn picture_generation(self) -> Option<u64> {
        match self {
            Self::Down(generation) => generation,
            Self::Quick { generation, .. } => Some(generation),
            _ => None,
        }
    }
}

/// A press edge (D7 "Timing" item 3).
#[derive(Clone, Copy, PartialEq)]
pub(crate) enum Edge {
    Down,
    /// A release that may hold for the first pressed picture.
    Up,
    /// Sliding off, cancellation or gesture takeover releases from the
    /// current progress without waiting for a picture.
    Cancel,
}

pub(super) struct Feedback {
    phase: Phase,
    /// A property with no track shows its phase's target.
    tracks: [Option<TransitionProgress>; PROPERTIES.len()],
}

impl Feedback {
    fn landed(phase: Phase) -> Self {
        Self {
            phase,
            tracks: std::array::from_fn(|_| None),
        }
    }

    fn progress(&self, now: f64) -> Progress {
        Progress(self.tracks.each_ref().map(|track| {
            track
                .as_ref()
                .map_or(self.phase.target(), |t| t.value(now / 1000.)) as f32
        }))
    }

    fn end(&self) -> Option<f64> {
        self.tracks
            .iter()
            .flatten()
            .map(|t| t.end_time() * 1000.)
            .reduce(f64::max)
    }
}

/// The painted hosts whose toolkit times an unwritten `transition` (item 2).
#[derive(Clone, Copy)]
enum Platform {
    Linux,
    Windows,
    Android,
}

impl Platform {
    fn current() -> Self {
        if cfg!(target_os = "android") || crate::paint::look::material() {
            Self::Android
        } else if cfg!(windows) {
            Self::Windows
        } else {
            Self::Linux
        }
    }

    /// Item 2's row for this host: the toolkit's published defaults for a
    /// property change it is not told how to time.
    fn timing(self, property: Property, down: bool) -> Transition {
        let bezier = |x1, y1, x2, y2| Easing::CubicBezier { x1, y1, x2, y2 };
        let (duration, delay, easing) = match (self, property, down) {
            // GTK 4's basic button (`gtk/theme/Default/_common.scss`).
            (Self::Linux, _, _) => (
                if down { 0.05 } else { 0.2 },
                0.,
                bezier(0.25, 0.46, 0.45, 0.94),
            ),
            // WinUI's stock `Button`: the background on its `BrushTransition`,
            // which interpolates a solid brush linearly, and the foreground at
            // once (`Button_themeresources.xaml`, `SharedTransitionAnimations.cpp`).
            (Self::Windows, Property::BackgroundColor, _) => (0.083, 0., Easing::Linear),
            (Self::Windows, Property::Color, _) => (0., 0., Easing::Linear),
            // WinUI's pointer tilt (`ThemeAnimations.cpp`).
            (Self::Windows, _, true) => (0., 0.01, Easing::Linear),
            (Self::Windows, _, false) => (0.2, 0.15, bezier(0.1, 0.25, 0.75, 0.9)),
            // Compose's `spring()` defaults: stiffness 1500 at a damping ratio
            // of 1, critical damping, `2·sqrt(stiffness·mass)`.
            (Self::Android, _, _) => {
                return Transition {
                    timing: TimingFunction::Spring(SpringConfig {
                        stiffness: 1500.,
                        damping: 2. * exact_motion::math::sqrt(1500.),
                        mass: 1.,
                    }),
                    ..Transition::INITIAL
                };
            }
        };
        Transition {
            duration,
            delay,
            timing: TimingFunction::Easing(easing),
            ..Transition::INITIAL
        }
    }
}

/// Who times a press property (D7 "Timing").
#[derive(Debug, PartialEq)]
enum Timing {
    /// Item 1: the item of the node's `transition` that covers it.
    Authored(Transition),
    /// Item 2: the platform's row. On Android it is Compose's spring.
    Platform(Transition),
}

/// How a press moves `property` of `node` at an edge, or `None` when it
/// lands at once.
fn timing(
    node: &exact_kernel::NodeRef<'_>,
    property: Property,
    down: bool,
    platform: Platform,
) -> Option<Timing> {
    let style = node.style;
    let authored = match property {
        Property::BackgroundColor => style.mask.has(StyleId::PressBackgroundColor),
        Property::Color => style.mask.has(StyleId::PressColor),
        Property::Opacity => {
            style.mask.has(StyleId::PressOpacity) || style.mask.has(StyleId::PressOpacityTo)
        }
        Property::Translate => {
            style.mask.has(StyleId::PressTranslate)
                || style.mask.has(StyleId::PressTranslatePercent)
        }
        Property::Scale => style.press_scale != 1.,
        _ => unreachable!(),
    };
    let platform = || Timing::Platform(platform.timing(property, down));
    let written = style.mask.union(style.written).has(StyleId::Transition);
    let covered = || {
        let item = style.transition.matching(property);
        item.map(|t| Timing::Authored(t.governing(property)))
    };
    if !native_button(node) {
        // Items 1 and 2: a written `transition` governs, a property it does
        // not cover landing at once; else the platform's.
        return match (authored, written) {
            (false, _) => None,
            (true, true) => covered(),
            (true, false) => Some(platform()),
        };
    }
    // Item 6: the button's own pressed look moves on the platform's row
    // whatever the node writes; an authored override adds no curve but its
    // `transition`'s, which names no colour (`lower-button-style-attr`).
    match property {
        Property::BackgroundColor | Property::Color => {
            (!authored && crate::paint::look::neutral()).then(platform)
        }
        _ if authored && written => covered(),
        _ => None,
    }
}

fn spring_distance(
    kernel: &Kernel,
    node: &exact_kernel::NodeRef<'_>,
    shown: &Presented,
    property: Property,
    dark: bool,
) -> f64 {
    let style = node.style;
    let color_distance = |a: Value, b| {
        let rgba = |color| {
            let [r, g, b, a] = crate::paint::rgba(color);
            Value::rgba8(r, g, b, a)
        };
        (a - rgba(b))
            .components()
            .into_iter()
            .map(f64::abs)
            .fold(0., f64::max)
    };
    match property {
        Property::BackgroundColor | Property::Color if native_button(node) => 1.,
        Property::BackgroundColor => {
            let [r, g, b, a] = shown.background(node, kernel, dark);
            color_distance(
                Value::rgba8(r, g, b, a),
                style.press_background_color.resolve(dark),
            )
        }
        Property::Color => {
            let [r, g, b, a] = shown
                .colors
                .color(property)
                .unwrap_or_else(|| crate::paint::rgba(node.text_color().resolve(dark)));
            color_distance(Value::rgba8(r, g, b, a), style.press_color.resolve(dark))
        }
        Property::Opacity => {
            let base = shown.opacity.clamp(0., 1.);
            let target = if style.mask.has(StyleId::PressOpacityTo) {
                style.press_opacity_to.clamp(0., 1.)
            } else {
                base * style.press_opacity.clamp(0., 1.)
            };
            (target - base).abs().into()
        }
        Property::Translate => {
            let x = f64::from(style.press_translate.x)
                + f64::from(style.press_translate_percent.x) / 100. * f64::from(node.frame.width);
            let y = f64::from(style.press_translate.y)
                + f64::from(style.press_translate_percent.y) / 100. * f64::from(node.frame.height);
            x.abs().max(y.abs())
        }
        Property::Scale => (f64::from(shown.scale) * (f64::from(style.press_scale) - 1.)).abs(),
        _ => unreachable!(),
    }
}

/// A critically damped spring can turn once. Find the final crossing of the
/// visibility threshold so a retarget does not settle on its way past it.
fn compose_settle(config: &SpringConfig, displacement: f64, velocity: f64, threshold: f64) -> f64 {
    let rate = config.damping / (2. * config.mass);
    let turn = velocity / (rate * (velocity + rate * displacement));
    // Search no further than 20 s: at Compose's stiffness a displacement of
    // a million units settles in under a second.
    let limit = 20.;
    let peak = if turn.is_finite() {
        turn.clamp(0., limit)
    } else {
        0.
    };
    let outside = |at| config.sample(displacement, velocity, at).displacement.abs() > threshold;
    let (mut lo, mut hi) = if outside(peak) {
        (peak, limit)
    } else if outside(0.) {
        (0., peak)
    } else {
        return 0.;
    };
    for _ in 0..20 {
        let middle = (lo + hi) / 2.;
        if outside(middle) {
            lo = middle;
        } else {
            hi = middle;
        }
    }
    hi
}

/// A state held as the real one (LLP 1122 D8, LLP 1121 stage 8 E3): the
/// build channel's `hold` or the agent's `holdState`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HoldState {
    Active,
    Hover,
    FocusVisible,
}
impl HoldState {
    pub(crate) fn parse(name: &str) -> Option<Self> {
        match name {
            "active" => Some(Self::Active),
            "hover" => Some(Self::Hover),
            "focus-visible" => Some(Self::FocusVisible),
            _ => None,
        }
    }
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Hover => "hover",
            Self::FocusVisible => "focus-visible",
        }
    }
}

/// The one state held: on `view` (`key`), for the hold frame `revision`
/// (none: the agent's), and whether the pointer and the focus were in it
/// when last read, so that their coming or going is real input.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Held {
    pub view: ViewId,
    /// The node it was named on (a node inside the press, for `active`).
    pub asked: ViewId,
    pub key: NodeKey,
    pub state: HoldState,
    pub revision: Option<u64>,
    pub over: bool,
    pub focused: bool,
    /// The build channel's stream it was held on (`Link::stream`).
    pub stream: u64,
}

/// The looks that hold: the pointer's (`:hover`), the focus's
/// (`:focus-visible`), and the state held beside them.
#[derive(Default)]
pub(crate) struct Shown {
    pub(crate) motion: exact_kernel::motion::LookMotion,
    pub hover: Vec<ViewId>,
    pub focus: Option<ViewId>,
    pub held: Option<Held>,
}
impl Shown {
    /// Whether `id`'s `:hover` and `:focus-visible` looks hold, real or held.
    pub(crate) fn on(&self, id: ViewId) -> [bool; 2] {
        let held = self.held.filter(|h| h.view == id).map(|h| h.state);
        [
            self.hover.contains(&id) || held == Some(HoldState::Hover),
            self.focus == Some(id) || held == Some(HoldState::FocusVisible),
        ]
    }

    /// Every node with a look on, and which.
    pub(crate) fn marks(&self) -> Vec<(ViewId, [bool; 2])> {
        let mut ids: Vec<ViewId> = self.hover.iter().copied().chain(self.focus).collect();
        ids.extend(self.held.map(|h| h.view));
        ids.sort_unstable();
        ids.dedup();
        ids.into_iter().map(|id| (id, self.on(id))).collect()
    }
}

/// Whether a node writes any of the pressed paint (LLP 1061 D7).
pub(crate) fn paints(style: &exact_kernel::StyleProps) -> bool {
    use exact_kernel::StyleId::{
        PressBackgroundColor, PressColor, PressOpacity, PressOpacityTo, PressTranslate,
        PressTranslatePercent,
    };
    [
        PressTranslate,
        PressTranslatePercent,
        PressOpacity,
        PressOpacityTo,
        PressBackgroundColor,
        PressColor,
    ]
    .into_iter()
    .any(|row| style.mask.has(row))
}

/// Whether the host may paint `property` over `node`'s own: a press's
/// paint, or a `:hover` or `:focus-visible` look naming it. A look's `color`
/// is not one: this host's looks paint the box alone (`BoxPaint::looked`).
pub(crate) fn paints_over(node: &exact_kernel::NodeRef<'_>, property: Property) -> bool {
    use exact_kernel::StyleId::*;
    let (rows, row): (&[exact_kernel::StyleId], _) = match property {
        // The press's scale and offset compose with the node's own transform
        // where the engine samples it; the reader would play it without them.
        Property::Translate | Property::Scale | Property::Rotate => {
            (&[PressScale, PressTranslate, PressTranslatePercent], None)
        }
        Property::Opacity => (&[PressOpacity, PressOpacityTo], Some(Opacity)),
        Property::BackgroundColor => (&[PressBackgroundColor], Some(BackgroundColor)),
        Property::Color => (&[PressColor], None),
        _ => return false,
    };
    // A look's `currentcolor` (a colour row set with no value, or a side it
    // clears) reads the node's colour as that moves too.
    let current = |l: &exact_kernel::StyleProps| {
        let sides = [
            (BorderColorTop, l.border_color_top),
            (BorderColorRight, l.border_color_right),
            (BorderColorBottom, l.border_color_bottom),
            (BorderColorLeft, l.border_color_left),
        ];
        (l.mask.has(BackgroundColor) && l.background_color.is_none())
            || sides
                .iter()
                .any(|(id, c)| l.mask.union(l.written).has(*id) && c.is_none())
    };
    rows.iter().any(|r| node.style.mask.has(*r))
        || row
            .is_some_and(|row| (node.looks().iter()).any(|(_, l)| l.mask.union(l.written).has(row)))
        || property == Property::Color && node.looks().iter().any(|(_, l)| current(l))
}

/// Whether a press on `node` shows anything of its own: a scale or the
/// pressed paint.
pub(crate) fn shows(node: &exact_kernel::NodeRef<'_>) -> bool {
    node.style.press_scale != 1. || paints(node.style)
}

pub(crate) fn native_button(node: &exact_kernel::NodeRef<'_>) -> bool {
    node.node_type == exact_kernel::NodeType::Control
        && node.props.str(exact_kernel::PropId::Type) == Some("button")
}

/// `shown` at a press's `progress`, and in the looks that hold (`looks`:
/// `:hover`, `:focus-visible`), whose opacity replaces the node's: the
/// press's opacity is `:active`'s absolute target, eased from what shows, or
/// the pressed paint's share of it.
pub(super) fn present(
    node: &exact_kernel::NodeRef<'_>,
    progress: Progress,
    looks: [bool; 2],
    shown: &mut Presented,
) {
    use exact_kernel::{StyleId, LOOK_FOCUS_VISIBLE, LOOK_HOVER};
    let disabled = node.props.bool(exact_kernel::PropId::Disabled) == Some(true);
    shown.looks = looks.map(|on| on && !disabled);
    for (state, on) in [
        (LOOK_HOVER, shown.looks[0]),
        (LOOK_FOCUS_VISIBLE, shown.looks[1]),
    ] {
        let names = |l: &exact_kernel::StyleProps| l.mask.union(l.written).has(StyleId::Opacity);
        let look = node.looks().iter().find(|(s, l)| *s == state && names(l));
        if let Some((_, l)) = look.filter(|_| on) {
            // A cleared opacity (`unset`) is CSS's initial one.
            shown.opacity = if l.mask.has(StyleId::Opacity) {
                l.opacity
            } else {
                1.0
            };
        }
    }
    // A disabled node shows no press, its release ease included; the
    // painter clamps opacity, so a base beyond 0 to 1 is clamped first.
    let progress = if disabled {
        Progress::default()
    } else {
        progress
    };
    shown.opacity = shown.opacity.clamp(0., 1.);
    shown.press = scale(node.style.press_scale, progress.of(Property::Scale));
    shown.press_background = progress.of(Property::BackgroundColor);
    shown.press_color = progress.of(Property::Color);
    let s = node.style;
    shown.press_translate = [
        s.press_translate.x,
        s.press_translate_percent.x,
        s.press_translate.y,
        s.press_translate_percent.y,
    ]
    .map(|v| v * progress.of(Property::Translate));
    if s.mask.has(StyleId::PressOpacityTo) {
        shown.opacity +=
            (s.press_opacity_to.clamp(0., 1.) - shown.opacity) * progress.of(Property::Opacity);
    } else if s.mask.has(StyleId::PressOpacity) {
        shown.opacity *= 1. + (s.press_opacity.clamp(0., 1.) - 1.) * progress.of(Property::Opacity);
    }
}

/// The scale a press at `progress` shows: none for a scale no press can use.
pub(crate) fn scale(to: f32, progress: f32) -> f32 {
    match to.is_finite() && to > 0. {
        true => 1. + (to - 1.) * progress,
        false => 1.,
    }
}

impl<D: DataSource> Host<D> {
    pub(crate) fn look_motion(&mut self, view: ViewId) {
        let Some(node) = self.runner.kernel().node(view) else {
            return;
        };
        let [hover, focus] = self.looks_shown.on(view);
        let states = u8::from(hover) | (u8::from(focus) << 1);
        let result = self
            .looks_shown
            .motion
            .observe(&node, states, &mut self.engine);
        debug_assert!(result.is_ok());
        self.present();
    }

    /// The value a sample replaces, before hover, focus and press feedback.
    pub(crate) fn kept_presented(&self, view: ViewId) -> Presented {
        self.presented
            .get(&view)
            .copied()
            .unwrap_or_else(|| self.unpresented(view))
    }

    /// A node's committed presentation, before a press or a look shows on
    /// it: what the engine's cache starts from (`Host::presented` adds them
    /// at every read, so neither is ever kept).
    pub(crate) fn unpresented(&self, view: ViewId) -> Presented {
        let node = self.runner.kernel().node(view);
        node.map_or(Presented::IDENTITY, |n| Presented::from_style(n.style))
    }

    /// A press `edge` on `key` at `now`, timed (D7 "Timing") when `animate`,
    /// else landed at once, as under the agent's clock (item 4).
    pub(crate) fn press_feedback(&mut self, key: NodeKey, edge: Edge, now: f64, animate: bool) {
        let down = edge == Edge::Down;
        // A held press is the hold's: a real press's release, or its moving
        // off, leaves it (LLP 1121 stage 8 E3), and letting go of the hold
        // releases it.
        if !down && self.holds_press(key) {
            return;
        }
        // An edge the press is already on has nothing to change: retimed, it
        // would restart its timing.
        let old = self.presses.get(&key).map(|p| p.phase);
        if down == matches!(old, Some(Phase::Down(_))) {
            return;
        }
        // Item 5: a disabled or hidden node's press ends at once.
        if !press_enabled(self.kernel(), key) {
            self.cancel_press(key);
            return;
        }
        // A page that presents nothing has no frame to time an edge on.
        if !animate || !self.engine.presentable() {
            if down {
                self.presses
                    .insert(key, Feedback::landed(Phase::Down(None)));
                self.row_dirty.node(key);
            } else {
                self.cancel_press(key);
            }
            return;
        }
        let node = self.kernel().node_by_key(key).expect("eligible node");
        let shows = shows(&node) || native_button(&node) && crate::paint::look::neutral();
        let now = now.max(self.now_ms);
        let phase = match edge {
            Edge::Down => Phase::Down(shows.then(|| self.next_press_generation())),
            Edge::Up if matches!(old, Some(Phase::Down(Some(_)))) && shows => Phase::Quick {
                generation: self.next_press_generation(),
                deadline: now + 1000. * exact_motion::HOLD_DEADLINE,
            },
            Edge::Up | Edge::Cancel => Phase::Up,
        };
        self.retarget_press(key, phase, now);
    }

    /// Whether `key`'s press is held as `active` (LLP 1121 stage 8 E3).
    fn holds_press(&self, key: NodeKey) -> bool {
        (self.looks_shown.held).is_some_and(|h| h.key == key && h.state == HoldState::Active)
    }

    fn next_press_generation(&mut self) -> u64 {
        self.press_generation += 1;
        self.press_generation
    }

    /// The press generation a picture painted now shows.
    pub(crate) fn press_generation(&self) -> u64 {
        self.press_generation
    }

    /// The press generation of the latest picture painted.
    pub(crate) fn painted_press_generation(&self) -> u64 {
        self.painted_press_generation
    }

    /// A picture showing press `generation` painted.
    pub(crate) fn press_painted(&mut self, generation: u64) {
        self.painted_press_generation = generation;
    }

    fn retarget_press(&mut self, key: NodeKey, phase: Phase, now: f64) {
        // A removed node's press ended with it (`commit_effects`).
        let Some(node) = self.kernel().node_by_key(key) else {
            return;
        };
        let old = self.presses.get(&key);
        let target = phase.target();
        let _roles = self.runner.bind_roles();
        let platform = Platform::current();
        // What the node shows unpressed, which a spring's distances measure from.
        let unpressed = || {
            let mut base = self.kept_presented(node.id);
            let dark = (self.presented(node.id).dark).unwrap_or_else(|| self.paint.dark(key));
            present(
                &node,
                Progress::default(),
                self.looks_shown.on(node.id),
                &mut base,
            );
            (base, dark)
        };
        let mut feedback = Feedback::landed(phase);
        if !matches!(phase, Phase::Quick { .. }) {
            let from = old.map_or(0., |p| p.phase.target());
            feedback.tracks = std::array::from_fn(|i| {
                let property = PROPERTIES[i];
                let timing = timing(&node, property, target == 1., platform)?;
                let compose = matches!(timing, Timing::Platform(_));
                let (Timing::Authored(declaration) | Timing::Platform(declaration)) = timing;
                let previous = old.and_then(|p| p.tracks[i].as_ref());
                TransitionProgress::retarget(
                    &declaration,
                    (from, target),
                    now / 1000.,
                    previous,
                    |config, distance, speed| {
                        let (base, dark) = unpressed();
                        let units = spring_distance(self.kernel(), &node, &base, property, dark);
                        // Compose's spring, the only platform one, ends within
                        // its visibility threshold, and keeps its velocity when
                        // retargeted, a colour's too, as `Animatable.animateTo`
                        // does.
                        if compose {
                            let threshold = match property {
                                Property::Translate => {
                                    1. / self.runner.viewport().device_pixel_ratio.unwrap_or(1.)
                                }
                                _ => 0.01,
                            };
                            compose_settle(config, distance * units, speed * units, threshold)
                        } else {
                            let units = if units > 0. { units } else { 1. };
                            config.settle_time(distance * units, speed * units)
                        }
                    },
                )
            });
        }
        if phase == Phase::Up && feedback.end().is_none() {
            self.presses.remove(&key);
        } else {
            self.presses.insert(key, feedback);
        }
        self.row_dirty.node(key);
    }

    /// End `key`'s press at once (item 5).
    pub(crate) fn cancel_press(&mut self, key: NodeKey) {
        if self.presses.remove(&key).is_some() {
            self.row_dirty.node(key);
        }
    }

    /// End every press at once as the window or the page goes (item 5), but
    /// land a held one: a hold lasts until it is let go, and the design tool
    /// that holds it takes the window's activation. Return whether a press
    /// ended or was landed.
    pub(crate) fn cancel_presses(&mut self) -> bool {
        let held = (self.looks_shown.held)
            .filter(|h| h.state == HoldState::Active)
            .map(|h| h.key);
        let mut changed = false;
        self.presses.retain(|key, p| {
            let keep = Some(*key) == held;
            if keep {
                if p.phase == Phase::Down(None) && p.tracks.iter().all(Option::is_none) {
                    return true;
                }
                *p = Feedback::landed(Phase::Down(None));
            }
            self.row_dirty.node(*key);
            changed = true;
            keep
        });
        changed
    }

    /// Land every press under the agent's clock (item 4): a held press shows
    /// its pressed values, and a release, a quick tap's included, ends.
    pub(crate) fn land_presses(&mut self) {
        self.presses.retain(|key, p| {
            self.row_dirty.node(*key);
            if matches!(p.phase, Phase::Down(_)) {
                *p = Feedback::landed(Phase::Down(None));
                true
            } else {
                false
            }
        });
    }

    /// Whether `key` is held down.
    pub(crate) fn pressed(&self, key: NodeKey) -> bool {
        self.presses
            .get(&key)
            .is_some_and(|p| matches!(p.phase, Phase::Down(_)))
    }

    pub(super) fn press_progress(&self, key: NodeKey) -> Progress {
        self.presses
            .get(&key)
            .map_or_else(Progress::default, |p| p.progress(self.now_ms))
    }

    /// When the last press track ends, while one has yet to.
    pub(super) fn press_settle(&self) -> Option<f64> {
        self.presses
            .values()
            .filter_map(Feedback::end)
            .filter(|&end| end > self.now_ms)
            .reduce(f64::max)
    }

    /// When a press next changes what shows: now while a track moves, else
    /// the earliest start of a track still in its delay.
    pub(super) fn press_due(&self) -> Option<f64> {
        let now = self.now_ms;
        (self.presses.values())
            .flat_map(|p| p.tracks.iter().flatten())
            .filter(|t| t.end_time() * 1000. > now)
            .map(|t| (t.start_time() * 1000.).max(now))
            .reduce(f64::min)
    }

    pub(super) fn press_releases_next_frame(&self) -> bool {
        self.presses
            .values()
            .any(|p| p.phase == Phase::ReleaseNextFrame)
    }

    /// The earliest deadline of a quick tap waiting for its picture.
    pub(super) fn press_deadline(&self) -> Option<f64> {
        self.presses
            .values()
            .filter_map(|p| match p.phase {
                Phase::Quick { deadline, .. } => Some(deadline),
                _ => None,
            })
            .reduce(f64::min)
    }

    /// Whether a press waits for a picture to show it, so that the present
    /// of the next is to be reported.
    pub(crate) fn press_wants_picture(&self) -> bool {
        self.presses
            .values()
            .any(|p| p.phase.picture_generation().is_some())
    }

    /// Whether a frame is owed to a press: one waits for a picture newer than
    /// the latest painted, or a release for the next frame.
    pub(crate) fn press_picture_owed(&self) -> bool {
        self.presses.values().any(|p| {
            p.phase == Phase::ReleaseNextFrame
                || p.phase
                    .picture_generation()
                    .is_some_and(|generation| generation > self.painted_press_generation)
        })
    }

    /// A picture of press `generation` presented: the presses it showed stop
    /// waiting, and its quick taps release at the next frame. Whether any
    /// will.
    pub(crate) fn presses_presented(&mut self, generation: u64) -> bool {
        let mut release = false;
        for p in self.presses.values_mut() {
            match p.phase {
                Phase::Down(Some(start)) if start <= generation => p.phase = Phase::Down(None),
                Phase::Quick {
                    generation: start, ..
                } if start <= generation => {
                    p.phase = Phase::ReleaseNextFrame;
                    release = true;
                }
                _ => {}
            }
        }
        release
    }

    /// Start the releases deferred to this frame (item 3).
    pub(crate) fn release_quick_presses(&mut self) {
        let releasing: Vec<_> = self
            .presses
            .iter()
            .filter_map(|(&key, p)| (p.phase == Phase::ReleaseNextFrame).then_some(key))
            .collect();
        for key in releasing {
            self.retarget_press(key, Phase::Up, self.now_ms);
        }
    }

    /// Drop the presses that have ended. Whether any did.
    pub(super) fn retire_presses(&mut self) -> bool {
        let mut paint = false;
        let kernel = self.runner.kernel();
        let now = self.now_ms;
        self.presses.retain(|&key, p| {
            // Keep a press while its node can show one: a quick tap until its
            // deadline, a release until it ends, a held press while held.
            let keep = press_enabled(kernel, key)
                && match p.phase {
                    Phase::Quick { deadline, .. } => now < deadline,
                    Phase::Up => p.end().is_some_and(|end| end > now),
                    _ => true,
                };
            if !keep {
                self.row_dirty.node(key);
                paint = true;
            }
            keep
        });
        paint
    }
}

fn press_enabled(kernel: &Kernel, key: NodeKey) -> bool {
    let Some(node) = kernel.node_by_key(key) else {
        return false;
    };
    let mut at = Some(node);
    while let Some(node) = at {
        if node.props.bool(exact_kernel::PropId::Disabled) == Some(true)
            || node.style.display == exact_kernel::Display::None
        {
            return false;
        }
        at = node.parent.and_then(|id| kernel.node(id));
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Item 1, 2 and 6's choice of timing for each kind of node, in the
    /// neutral look, whose native buttons have a pressed look of their own.
    #[test]
    fn each_press_property_takes_the_timing_its_node_and_platform_give() {
        const CHILD: &str = "EXACT_PRESS_TIMING_TEST";
        if std::env::var_os(CHILD).is_none() {
            let name = std::thread::current().name().unwrap().to_owned();
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", &name, "--nocapture"])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}\n{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        crate::app::show_neutral_look();
        let source = r##"component App
  state n = 0
  action go
    n = n + 1
  view
    column
      box press=go -exact-press-scale=0.5 testId="unwritten"
      box press=go -exact-press-scale=0.5 transition="scale 100ms linear" testId="written"
        :active
          background-color = "#0000ff"
      box press=go -exact-press-scale=0.5 transition="none" testId="none"
      button press=go testId="native"
        text "n"
      button press=go transition="opacity 300ms linear" testId="native-written"
        :active
          background-color = "#0000ff"
          color = "#ff0000"
          opacity = 0.5
          translate = "2px 2px"
        text "n"
      button press=go testId="native-unwritten"
        :active
          opacity = 0.5
        text "n"
"##;
        let (h, error) = Host::boot(
            &contract::compile(source)
                .unwrap_or_else(|e| panic!("{e}"))
                .encode(),
            (),
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.,
            400.,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let timed = |test_id: &str, property| {
            let k = h.kernel();
            let node = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap();
            match timing(&node, property, true, Platform::Linux) {
                Some(Timing::Authored(t)) => Some(("authored", t.duration)),
                Some(Timing::Platform(t)) => Some(("platform", t.duration)),
                None => None,
            }
        };
        use Property::{BackgroundColor, Color, Opacity, Scale, Translate};
        let platform = Some(("platform", 0.05));
        for (test_id, property, expected) in [
            ("unwritten", Scale, platform),
            ("unwritten", Opacity, None),
            ("written", Scale, Some(("authored", 0.1))),
            ("written", BackgroundColor, None),
            ("none", Scale, None),
            ("native", BackgroundColor, platform),
            ("native", Color, platform),
            ("native", Scale, None),
            ("native-written", BackgroundColor, None),
            ("native-written", Color, None),
            ("native-written", Opacity, Some(("authored", 0.3))),
            ("native-written", Translate, None),
            ("native-unwritten", Opacity, None),
            ("native-unwritten", BackgroundColor, platform),
        ] {
            assert_eq!(timed(test_id, property), expected, "{test_id} {property:?}");
        }
    }

    #[test]
    fn desktop_press_tracks_use_platform_easing_and_delays() {
        let run = |platform: Platform, property, down| {
            let declaration = platform.timing(property, down);
            let (from, to) = if down { (0., 1.) } else { (1., 0.) };
            TransitionProgress::retarget(&declaration, (from, to), 0., None, |_, _, _| {
                unreachable!("desktop press easing")
            })
        };
        let linux_in = run(Platform::Linux, Property::Scale, true).unwrap();
        let linux_out = run(Platform::Linux, Property::Scale, false).unwrap();
        assert_eq!(linux_in.end_time(), 0.05);
        assert_eq!(linux_out.end_time(), 0.2);
        assert!((linux_in.value(0.025) - 0.7713).abs() < 0.001);
        assert!((linux_out.value(0.1) - 0.2287).abs() < 0.001);

        let windows = Platform::Windows;
        let brush = run(windows, Property::BackgroundColor, true).unwrap();
        assert_eq!(brush.end_time(), 0.083);
        assert_eq!(brush.value(0.0415), 0.5);
        assert!(run(windows, Property::Color, true).is_none());
        let tilt_in = run(windows, Property::Translate, true).unwrap();
        assert_eq!(tilt_in.value(0.009), 0.);
        assert_eq!(tilt_in.value(0.01), 1.);
        let tilt_out = run(windows, Property::Opacity, false).unwrap();
        assert_eq!(tilt_out.value(0.149), 1.);
        assert!((tilt_out.value(0.25) - 0.3884).abs() < 0.001);
        assert_eq!(tilt_out.value(0.351), 0.);
    }

    #[test]
    fn android_press_springs_settle_at_the_property_threshold() {
        let timing = Platform::Android.timing(Property::Translate, true);
        let TimingFunction::Spring(config) = timing.timing else {
            panic!("Android press spring");
        };
        let omega = 1500_f64.sqrt();
        let at = 0.04;
        let sample = config.sample(1., 0., at);
        assert!((sample.displacement - (1. + omega * at) * (-omega * at).exp()).abs() < 1e-6);
        for (displacement, velocity, threshold) in [(16., 0., 1.), (0.5, 0., 0.01), (0., 40., 0.01)]
        {
            let end = compose_settle(&config, displacement, velocity, threshold);
            let value = |at| config.sample(displacement, velocity, at).displacement.abs();
            assert!(value(end) <= threshold && value(end - 0.001) > threshold);
        }
    }
}
