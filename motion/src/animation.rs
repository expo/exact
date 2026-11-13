//! CSS Animations: `@keyframes` and the `animation` row, sampled in closed form.
//!
//! @ref LLP 1055 D5 (CSS Animations Level 1 over the Web Animations timing
//! model); LLP 1002 D1 (the style row is the binding)
//!
//! An [`Animations`] list is the value of a node's `animation` style row. Each
//! entry carries the keyframes its `animation-name` resolved to, so a host has
//! everything it needs from the node's style: the web emits it as CSS, Apple
//! lowers it to Core Animation, and the engine samples it everywhere else.
//! Sampling is a pure function of local time, so a seek and sixty frames give
//! the same bits.
//!
//! A keyframe colour may be a `light-dark()` pair (LLP 1062 D9): the light
//! value is the keyframe's, the dark one rides beside it, and which one plays
//! is the appearance the animation started under, as Chrome resolves a rule
//! once.

use crate::easing::{Easing, EasingError};
use crate::property::{Property, Value};

mod parse;
mod schedule;
pub use parse::{easing_css, is_easing, link, value_css, Role, Slots, LONGHANDS};

/// Most animations one node may declare.
pub const MAX_ANIMATIONS: usize = 8;
/// Most keyframes one `@keyframes` rule may hold.
pub const MAX_KEYFRAMES: usize = 32;

/// Most values one keyframe declares: one per property, and one per axis
/// of a `font-variation-settings`.
pub const MAX_VALUES: usize = Property::COUNT + parse::MAX_AXES - 1;

/// `animation-direction`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum Direction {
    /// Every iteration plays forwards.
    #[default]
    Normal = 0,
    /// Every iteration plays backwards.
    Reverse = 1,
    /// Even iterations forwards, odd backwards.
    Alternate = 2,
    /// Even iterations backwards, odd forwards.
    AlternateReverse = 3,
}

/// `animation-fill-mode`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[repr(u8)]
pub enum FillMode {
    /// Nothing outside the active interval.
    #[default]
    None = 0,
    /// The last value after the end.
    Forwards = 1,
    /// The first value during the delay.
    Backwards = 2,
    /// Both.
    Both = 3,
}

impl Direction {
    /// The CSS keyword.
    pub fn name(self) -> &'static str {
        ["normal", "reverse", "alternate", "alternate-reverse"][self as usize]
    }
    /// From the wire discriminant.
    pub fn from_wire(v: u8) -> Option<Direction> {
        [
            Direction::Normal,
            Direction::Reverse,
            Direction::Alternate,
            Direction::AlternateReverse,
        ]
        .get(v as usize)
        .copied()
    }
}

impl FillMode {
    /// The CSS keyword.
    pub fn name(self) -> &'static str {
        ["none", "forwards", "backwards", "both"][self as usize]
    }
    /// From the wire discriminant.
    pub fn from_wire(v: u8) -> Option<FillMode> {
        [
            FillMode::None,
            FillMode::Forwards,
            FillMode::Backwards,
            FillMode::Both,
        ]
        .get(v as usize)
        .copied()
    }
    fn backwards(self) -> bool {
        matches!(self, FillMode::Backwards | FillMode::Both)
    }
    fn forwards(self) -> bool {
        matches!(self, FillMode::Forwards | FillMode::Both)
    }
}

/// One keyframe: an offset in `[0, 1]`, its own timing function if it set
/// one, and the values it declares.
#[derive(Debug, Clone, PartialEq)]
pub struct Keyframe {
    /// The selector as a fraction (`from` is 0, `to` is 1).
    pub offset: f64,
    /// This keyframe's `animation-timing-function`, applying to the interval
    /// that starts here; `None` takes the animation's.
    pub easing: Option<Easing>,
    /// The declared values, one per property.
    pub values: Vec<(Property, Value)>,
    /// A `light-dark()` colour's value under a dark appearance, for each of
    /// `values` that has one; `values` holds the light one (LLP 1062 D9).
    pub dark: Vec<(Property, Value)>,
}

impl Keyframe {
    /// The `font-variation-settings` axes it sets, sorted.
    pub fn axis_tags(&self) -> Vec<u32> {
        let mut tags: Vec<u32> = (self.values.iter())
            .filter(|(p, _)| *p == Property::FontVariationSettings)
            .map(|(_, v)| v.axis_tag())
            .collect();
        tags.sort_unstable();
        tags
    }

    /// A property's value under an appearance.
    fn get(&self, property: Property, dark: bool) -> Option<Value> {
        let own =
            |list: &[(Property, Value)]| list.iter().find(|(q, _)| *q == property).map(|(_, v)| *v);
        dark.then(|| own(&self.dark))
            .flatten()
            .or_else(|| own(&self.values))
    }
}

/// A resolved `@keyframes` rule: keyframes in offset order. Shared: every
/// copy of an animation (a style's row, the engine's play) holds one list.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Keyframes(pub std::sync::Arc<[Keyframe]>);

/// One entry of the `animation` row.
#[derive(Debug, Clone, PartialEq)]
pub struct Animation {
    /// `animation-name`, as authored.
    pub name: String,
    /// `animation-duration`, seconds.
    pub duration: f64,
    /// `animation-delay`, seconds; negative starts partway through.
    pub delay: f64,
    /// `animation-timing-function`, per keyframe interval. Never a spring.
    pub easing: Easing,
    /// `animation-iteration-count`; `f64::INFINITY` is `infinite`.
    pub iterations: f64,
    /// `animation-direction`.
    pub direction: Direction,
    /// `animation-fill-mode`.
    pub fill: FillMode,
    /// `animation-play-state: paused`.
    pub paused: bool,
    /// The keyframes the name resolved to; empty when no rule has the name
    /// (CSS keeps such an animation: it runs and animates nothing).
    pub keyframes: Keyframes,
}

/// The `animation` row: up to [`MAX_ANIMATIONS`] entries. Later entries win
/// over earlier ones for a property both animate, as in CSS's composite order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Animations(pub Vec<Animation>);

/// Why an animation was refused: one variant per check, so its sentence
/// names the rule that broke.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AnimationError {
    /// More than [`MAX_ANIMATIONS`] entries.
    TooMany,
    /// More than [`MAX_KEYFRAMES`] keyframes.
    TooManyKeyframes,
    /// More than [`MAX_VALUES`] values in one keyframe.
    TooManyValues,
    /// A property, a `font-variation-settings` axis or a dark value set
    /// twice in one keyframe.
    Repeated(Property),
    /// A number that was not finite: which one.
    NonFinite(&'static str),
    /// A negative duration.
    NegativeDuration,
    /// A negative iteration count.
    NegativeCount,
    /// A keyframe offset outside `[0, 1]`.
    OffsetOutOfRange,
    /// Keyframes out of offset order.
    Unordered,
    /// A scalar property carried a second component.
    ValueShape,
    /// The easing was invalid.
    Easing(EasingError),
    /// Half of a `box-shadow`, its geometry or its colour, without the other.
    HalfShadow,
    /// A dark value for a property that is no colour, or with no light value
    /// beside it.
    DarkWithoutLight(Property),
    /// Keyframes that name different `font-variation-settings` axes.
    AxesDiffer,
    /// An `-exact-exit-animation` that never ends — an `infinite` count, or
    /// `paused` — would keep its leaving node forever (LLP 1063).
    Endless,
}

impl std::fmt::Display for AnimationError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AnimationError::TooMany => write!(f, "a node takes at most {MAX_ANIMATIONS} animations"),
            AnimationError::TooManyKeyframes => {
                write!(f, "keyframes hold at most {MAX_KEYFRAMES} keyframes")
            }
            AnimationError::TooManyValues => {
                write!(f, "a keyframe sets at most {MAX_VALUES} values")
            }
            AnimationError::Repeated(Property::FontVariationSettings) => {
                f.write_str("a keyframe sets each `font-variation-settings` axis once")
            }
            AnimationError::Repeated(p) => write!(f, "a keyframe sets `{}` once", p.name()),
            AnimationError::NonFinite("iteration count") => {
                f.write_str("an iteration count is a number or `infinite`")
            }
            AnimationError::NonFinite(what) => write!(f, "an animation's {what} is finite"),
            AnimationError::NegativeDuration => f.write_str("a duration is 0 or more"),
            AnimationError::NegativeCount => f.write_str("an iteration count is 0 or more"),
            AnimationError::OffsetOutOfRange => {
                f.write_str("a keyframe's offset is from 0% to 100%")
            }
            AnimationError::Unordered => f.write_str("keyframes are in the order of their offsets"),
            AnimationError::ValueShape => {
                f.write_str("a property is given more numbers than it takes")
            }
            AnimationError::Easing(e) => e.fmt(f),
            AnimationError::HalfShadow => {
                f.write_str("a keyframe sets a `box-shadow`'s geometry and colour together")
            }
            AnimationError::DarkWithoutLight(p) if !p.is_color() => {
                write!(f, "`{}` is no colour, and only a colour takes a dark value", p.name())
            }
            AnimationError::DarkWithoutLight(p) => write!(
                f,
                "a keyframe gives `{}` a dark value only beside its light one",
                p.name()
            ),
            // @ref LLP 1019 §13 D10 — the axes interpolate one by one.
            AnimationError::AxesDiffer => f.write_str(
                "every keyframe that sets `font-variation-settings` names the same axes, which interpolate one by one",
            ),
            AnimationError::Endless => f.write_str(
                "an exit animation ends: it is not `infinite` and not `paused`",
            ),
        }
    }
}

impl Default for Animation {
    fn default() -> Self {
        Animation {
            name: String::new(),
            duration: 0.0,
            delay: 0.0,
            easing: Easing::Ease,
            iterations: 1.0,
            direction: Direction::Normal,
            fill: FillMode::None,
            paused: false,
            keyframes: Keyframes::default(),
        }
    }
}

/// Where local time falls (Web Animations §4.5.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    /// Before the delay ends.
    Before,
    /// Playing.
    Active,
    /// After the last iteration.
    After,
}

impl Keyframes {
    /// Every property some keyframe declares, in wire order.
    pub fn properties(&self) -> Vec<Property> {
        let mut out: Vec<Property> = Vec::new();
        for frame in self.0.iter() {
            for (p, _) in &frame.values {
                if !out.contains(p) {
                    out.push(*p);
                }
            }
        }
        out.sort();
        out
    }

    /// Check the rule's shape.
    pub fn validate(&self) -> Result<(), AnimationError> {
        if self.0.len() > MAX_KEYFRAMES {
            return Err(AnimationError::TooManyKeyframes);
        }
        let mut last = 0.0;
        let mut axes: Option<Vec<u32>> = None;
        for frame in self.0.iter() {
            if !frame.offset.is_finite() {
                return Err(AnimationError::NonFinite("keyframe offset"));
            }
            if !(0.0..=1.0).contains(&frame.offset) {
                return Err(AnimationError::OffsetOutOfRange);
            }
            if frame.offset < last {
                return Err(AnimationError::Unordered);
            }
            last = frame.offset;
            if let Some(e) = &frame.easing {
                e.validate().map_err(AnimationError::Easing)?;
            }
            if frame.values.len() > MAX_VALUES {
                return Err(AnimationError::TooManyValues);
            }
            for (i, (p, v)) in frame.values.iter().enumerate() {
                // One value per property; `font-variation-settings` has one
                // per axis.
                let twice = |(q, w): &(Property, Value)| {
                    q == p
                        && (*p != Property::FontVariationSettings || w.axis_tag() == v.axis_tag())
                };
                if frame.values[..i].iter().any(twice) {
                    return Err(AnimationError::Repeated(*p));
                }
                if !v.is_finite() {
                    return Err(AnimationError::NonFinite("keyframe value"));
                }
                if !v.fits(*p) {
                    return Err(AnimationError::ValueShape);
                }
                // `box-shadow` is one declaration: its geometry and colour
                // are set together or not at all (LLP 1062 D9).
                let half = match p {
                    Property::BoxShadow => Some(Property::ShadowColor),
                    Property::ShadowColor => Some(Property::BoxShadow),
                    _ => None,
                };
                if half.is_some_and(|h| !frame.values.iter().any(|(q, _)| *q == h)) {
                    return Err(AnimationError::HalfShadow);
                }
            }
            // Axes interpolate one by one (CSS Fonts 4 §7.2), so every
            // keyframe that sets them names the same ones.
            let tags = frame.axis_tags();
            if !tags.is_empty() {
                match &axes {
                    Some(first) if *first != tags => return Err(AnimationError::AxesDiffer),
                    Some(_) => {}
                    None => axes = Some(tags),
                }
            }
            for (i, (p, v)) in frame.dark.iter().enumerate() {
                if !p.is_color() || !frame.values.iter().any(|(q, _)| q == p) {
                    return Err(AnimationError::DarkWithoutLight(*p));
                }
                if frame.dark[..i].iter().any(|(q, _)| q == p) {
                    return Err(AnimationError::Repeated(*p));
                }
                if !v.is_finite() {
                    return Err(AnimationError::NonFinite("keyframe value"));
                }
            }
        }
        Ok(())
    }

    /// The keyframes one property takes part in, as (offset, easing, value),
    /// under an animation whose own timing function is `easing`, with CSS's
    /// implicit `0%` and `100%` from `underlying` where no keyframe
    /// declares it (CSS Animations 1 §3). A `light-dark()` colour takes its
    /// value under `dark`.
    ///
    /// Keyframes at one offset whose timing functions are equal, a keyframe
    /// without one taking the animation's, are one keyframe in which the
    /// later value wins (CSS Animations 2 §3.2). The parser groups only
    /// what it can without the animation.
    pub fn track<'a>(
        &'a self,
        property: Property,
        underlying: Value,
        dark: bool,
        easing: &'a Easing,
    ) -> Vec<(f64, &'a Easing, Value)> {
        let mut out: Vec<(f64, &Easing, Value)> = Vec::new();
        for f in self.0.iter() {
            let Some(v) = f.get(property, dark) else {
                continue;
            };
            let e = f.easing.as_ref().unwrap_or(easing);
            match out.iter_mut().find(|k| k.0 == f.offset && k.1 == e) {
                Some(same) => same.2 = v,
                None => out.push((f.offset, e, v)),
            }
        }
        if out.first().is_none_or(|f| f.0 > 0.0) {
            out.insert(0, (0.0, easing, underlying));
        }
        if out.last().is_none_or(|f| f.0 < 1.0) {
            out.push((1.0, easing, underlying));
        }
        out
    }
}

impl Animation {
    /// Check every rule the sampler relies on.
    pub fn validate(&self) -> Result<(), AnimationError> {
        if !self.duration.is_finite() {
            return Err(AnimationError::NonFinite("duration"));
        }
        if !self.delay.is_finite() {
            return Err(AnimationError::NonFinite("delay"));
        }
        if self.iterations.is_nan() {
            return Err(AnimationError::NonFinite("iteration count"));
        }
        if self.duration < 0.0 {
            return Err(AnimationError::NegativeDuration);
        }
        if self.iterations < 0.0 {
            return Err(AnimationError::NegativeCount);
        }
        self.easing.validate().map_err(AnimationError::Easing)?;
        self.keyframes.validate()
    }

    /// Whether every number that must be finite is (`iterations` may be
    /// infinite).
    pub fn is_finite(&self) -> bool {
        !matches!(self.validate(), Err(AnimationError::NonFinite(_)))
    }

    /// The active duration: duration × iteration count (0 when either is 0).
    pub fn active_duration(&self) -> f64 {
        if self.duration == 0.0 || self.iterations == 0.0 {
            0.0
        } else {
            self.duration * self.iterations
        }
    }

    /// Local time (seconds since the animation started, delay included) at
    /// which it ends; infinite for an infinite iteration count.
    pub fn end_time(&self) -> f64 {
        (self.delay + self.active_duration()).max(0.0)
    }

    /// The phase at local time `t` (Web Animations §4.5.8, positive rate).
    pub fn phase(&self, t: f64) -> Phase {
        let end = self.end_time();
        let before = self.delay.min(end).max(0.0);
        let after = (self.delay + self.active_duration()).min(end).max(0.0);
        if t < before {
            Phase::Before
        } else if t >= after {
            Phase::After
        } else {
            Phase::Active
        }
    }

    /// The directed progress at local time `t`, or `None` when the animation
    /// has no effect then (outside its active interval without a fill).
    pub fn directed_progress(&self, t: f64) -> Option<f64> {
        // An unresolved local time (NaN) is the idle phase: no effect (Web
        // Animations 1, animation effect phases).
        if t.is_nan() {
            return None;
        }
        let ad = self.active_duration();
        let phase = self.phase(t);
        let active = match phase {
            Phase::Before => {
                if !self.fill.backwards() {
                    return None;
                }
                (t - self.delay).max(0.0)
            }
            Phase::Active => t - self.delay,
            Phase::After => {
                if !self.fill.forwards() {
                    return None;
                }
                (t - self.delay).min(ad).max(0.0)
            }
        };
        let overall = if self.duration == 0.0 {
            if phase == Phase::Before {
                0.0
            } else {
                self.iterations
            }
        } else {
            active / self.duration
        };
        let mut simple = if overall.is_infinite() {
            0.0
        } else {
            overall % 1.0
        };
        if simple == 0.0 && phase != Phase::Before && active == ad && self.iterations != 0.0 {
            simple = 1.0;
        }
        let iteration = if phase == Phase::After && self.iterations.is_infinite() {
            f64::INFINITY
        } else if simple == 1.0 {
            overall.floor() - 1.0
        } else {
            overall.floor()
        };
        let odd = iteration.is_finite() && (iteration as i64).rem_euclid(2) == 1;
        let forwards = match self.direction {
            Direction::Normal => true,
            Direction::Reverse => false,
            Direction::Alternate => !odd,
            Direction::AlternateReverse => odd,
        };
        Some(if forwards { simple } else { 1.0 - simple })
    }

    /// One property's animated value at local time `t` over `underlying` (the
    /// value the property would have without this animation), or `None` when
    /// the animation does not apply to it then.
    pub fn sample(&self, t: f64, property: Property, underlying: Value) -> Option<Value> {
        self.sample_in(t, property, underlying, false)
    }

    /// [`Animation::sample`] under an appearance: a keyframe's `light-dark()`
    /// colour takes its dark value when `dark` (LLP 1062 D9).
    pub fn sample_in(
        &self,
        t: f64,
        property: Property,
        underlying: Value,
        dark: bool,
    ) -> Option<Value> {
        if !self
            .keyframes
            .0
            .iter()
            .any(|f| f.values.iter().any(|(p, _)| *p == property))
        {
            return None;
        }
        let p = self.directed_progress(t)?;
        Some(self.value_at(p, property, underlying, dark))
    }

    /// The keyframe effect at directed progress `p` for one property, a
    /// `light-dark()` colour under `dark`.
    pub fn value_at(&self, p: f64, property: Property, underlying: Value, dark: bool) -> Value {
        let track = self
            .keyframes
            .track(property, underlying, dark, &self.easing);
        // The interval: the last keyframe at or before p (the last of equal
        // offsets), and the next one after it; at p = 1 the final interval,
        // or, when more than one keyframe is at 1, the last of them (Web
        // Animations 1, "the effect value of a keyframe effect").
        let mut start = 0;
        for (i, frame) in track.iter().enumerate() {
            if frame.0 <= p && i + 1 < track.len() {
                start = i;
            }
        }
        if p >= 1.0 {
            let n = track.len();
            if n >= 2 && track[n - 2].0 >= 1.0 {
                return track[n - 1].2;
            }
            start = n.saturating_sub(2);
        }
        let (a, b) = (&track[start], &track[(start + 1).min(track.len() - 1)]);
        let span = b.0 - a.0;
        if span <= 0.0 {
            return if p >= b.0 { b.2 } else { a.2 };
        }
        let local = (p - a.0) / span;
        let eased = a.1.progress(local);
        a.2.lerp(b.2, eased)
    }
}

impl Animations {
    /// No animation (`animation: none`).
    pub const NONE: Animations = Animations(Vec::new());

    /// Check every entry.
    pub fn validate(&self) -> Result<(), AnimationError> {
        if self.0.len() > MAX_ANIMATIONS {
            return Err(AnimationError::TooMany);
        }
        self.0.iter().try_for_each(Animation::validate)
    }

    /// Whether every number that must be finite is.
    pub fn is_finite(&self) -> bool {
        self.0.iter().all(Animation::is_finite)
    }

    /// Local time at which the last entry ends; zero for none, infinite when
    /// one is endless.
    pub fn end_time(&self) -> f64 {
        self.0.iter().map(Animation::end_time).fold(0.0, f64::max)
    }

    /// [`Animations::validate`], and every entry runs to an end: the rule for
    /// an `-exact-exit-animation`, whose node is removed when it ends (LLP 1063).
    pub fn validate_ending(&self) -> Result<(), AnimationError> {
        self.validate()?;
        if self.0.iter().any(|a| a.paused || !a.end_time().is_finite()) {
            return Err(AnimationError::Endless);
        }
        Ok(())
    }

    /// Every property some entry animates, in wire order.
    pub fn properties(&self) -> Vec<Property> {
        let mut out: Vec<Property> = Vec::new();
        for a in &self.0 {
            for p in a.keyframes.properties() {
                if !out.contains(&p) {
                    out.push(p);
                }
            }
        }
        out.sort();
        out
    }
}

#[cfg(test)]
mod tests;
