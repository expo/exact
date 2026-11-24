//! `transition`: the declaration, and CSS's rules for running one.
//!
//! @ref LLP 1002 §2; CSS Transitions Level 1 §3 ("Starting of transitions")
//!
//! A [`Transitions`] list is the value of a node's `transition` style row in
//! the kernel — the same row a web host emits as CSS. The rest of this module
//! is what a browser does with that row: start a transition when the target
//! changes, interrupt it from the current value when the target changes
//! again, and shorten it when the change reverses (§3.1's reversing-adjusted
//! start value and reversing shortening factor), so a hover-off halfway
//! through a hover-on takes half the time on every platform, not just the web.

use crate::easing::{Easing, EasingError};
use crate::property::{Property, Value};
use crate::spring::{SpringConfig, SpringError};

/// Most transitions one node may declare.
pub const MAX_TRANSITIONS: usize = 8;

/// What a transition applies to (`transition-property`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionProperty {
    /// `all`.
    All,
    /// One named property; `box-shadow` names its colour half too.
    Property(Property),
    /// `border-color`, CSS's shorthand for the four sides' colours.
    BorderColor,
}

impl TransitionProperty {
    /// Whether this declaration covers `property`.
    pub fn covers(self, property: Property) -> bool {
        match self {
            TransitionProperty::All => true,
            TransitionProperty::Property(Property::BoxShadow) => {
                matches!(property, Property::BoxShadow | Property::ShadowColor)
            }
            TransitionProperty::Property(p) => p == property,
            TransitionProperty::BorderColor => matches!(
                property,
                Property::BorderTopColor
                    | Property::BorderRightColor
                    | Property::BorderBottomColor
                    | Property::BorderLeftColor
            ),
        }
    }

    /// From a `transition-property` name.
    pub fn from_name(name: &str) -> Option<TransitionProperty> {
        match name {
            "all" => Some(TransitionProperty::All),
            "border-color" => Some(TransitionProperty::BorderColor),
            // A path's `d` transitions (LLP 1055.000 D15); no keyframe names it.
            "d" => Some(TransitionProperty::Property(Property::D)),
            // So does `filter` (LLP 1003.002).
            "filter" => Some(TransitionProperty::Property(Property::Filter)),
            name => Property::from_author_name(name).map(TransitionProperty::Property),
        }
    }

    /// The name a browser knows it by ([`Property::css_name`]).
    pub fn css_name(self) -> &'static str {
        match self {
            TransitionProperty::All => "all",
            TransitionProperty::Property(p) => p.css_name(),
            TransitionProperty::BorderColor => "border-color",
        }
    }
}

/// How a transition progresses (`transition-timing-function`).
#[derive(Debug, Clone, PartialEq)]
pub enum TimingFunction {
    /// A CSS easing over the authored duration.
    Easing(Easing),
    /// A spring; the duration is derived from the spring, not authored.
    Spring(SpringConfig),
}

/// One `transition` declaration: property, duration, delay, timing function.
#[derive(Debug, Clone, PartialEq)]
pub struct Transition {
    /// What it applies to.
    pub property: TransitionProperty,
    /// `transition-duration`, seconds. Must be `0` for a spring.
    pub duration: f64,
    /// `transition-delay`, seconds. Negative starts partway through (easing
    /// only).
    pub delay: f64,
    /// The curve.
    pub timing: TimingFunction,
}

/// Why a transition declaration was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransitionError {
    /// More declarations than [`MAX_TRANSITIONS`].
    TooMany,
    /// A duration or delay was infinite or NaN: which one.
    NonFinite(&'static str),
    /// `transition-duration` was negative.
    NegativeDuration,
    /// A spring transition authored a duration; a spring's duration is derived.
    SpringDeclaresDuration,
    /// A spring transition authored a negative delay.
    SpringNegativeDelay,
    /// The easing was invalid.
    Easing(EasingError),
    /// The spring parameters were invalid.
    Spring(SpringError),
    /// A declaration reached an artifact that does not link CSS transitions
    /// ([`crate::link_transitions`]); its plan is refused at boot (LLP 1047 D6).
    Unlinked,
}

impl std::fmt::Display for TransitionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TransitionError::TooMany => {
                write!(f, "a node takes at most {MAX_TRANSITIONS} transitions")
            }
            TransitionError::NonFinite(what) => write!(f, "a transition's {what} is finite"),
            TransitionError::NegativeDuration => f.write_str("a duration is 0 or more"),
            TransitionError::SpringDeclaresDuration => f.write_str(
                "a spring's duration follows from its stiffness, damping and mass, and the first time beside `-exact-spring()` is a duration: write no time, or `0s` and then the delay (`opacity 0s -exact-spring() 200ms`)",
            ),
            TransitionError::SpringNegativeDelay => f.write_str("a spring's delay is 0 or more"),
            TransitionError::Easing(e) => e.fmt(f),
            TransitionError::Spring(e) => e.fmt(f),
            TransitionError::Unlinked => {
                f.write_str("CSS transitions are not linked into this artifact (LLP 1047 D6)")
            }
        }
    }
}

impl Transition {
    /// CSS's initial transition: covers every property but starts nothing.
    pub const INITIAL: Self = Self {
        property: TransitionProperty::All,
        duration: 0.0,
        delay: 0.0,
        timing: TimingFunction::Easing(Easing::Ease),
    };

    /// A transition with no delay.
    pub fn new(property: TransitionProperty, duration: f64, timing: TimingFunction) -> Transition {
        Transition {
            property,
            duration,
            delay: 0.0,
            timing,
        }
    }

    /// Check every rule the evaluator relies on.
    pub fn validate(&self) -> Result<(), TransitionError> {
        if !self.duration.is_finite() {
            return Err(TransitionError::NonFinite("duration"));
        }
        if !self.delay.is_finite() {
            return Err(TransitionError::NonFinite("delay"));
        }
        if self.duration < 0.0 {
            return Err(TransitionError::NegativeDuration);
        }
        match &self.timing {
            TimingFunction::Easing(easing) => easing.validate().map_err(TransitionError::Easing),
            TimingFunction::Spring(config) => {
                if self.duration != 0.0 {
                    return Err(TransitionError::SpringDeclaresDuration);
                }
                if self.delay < 0.0 {
                    return Err(TransitionError::SpringNegativeDelay);
                }
                config.validate().map_err(TransitionError::Spring)
            }
        }
    }

    /// Whether every number in the declaration is finite.
    pub fn is_finite(&self) -> bool {
        self.duration.is_finite()
            && self.delay.is_finite()
            && match &self.timing {
                TimingFunction::Easing(e) => e.is_finite(),
                TimingFunction::Spring(s) => s.is_finite(),
            }
    }

    /// The declaration as it governs `property`. A spring on a property no
    /// spring drives as physics ([`Property::springs`]: paint, a path's
    /// stroke) is its curve from rest, a `linear()` easing over its settle
    /// time: the web's CSS plays those properties itself, and CSS interrupts
    /// an easing from where it is (LLP 1062 D3).
    pub fn governing(&self, property: Property) -> Transition {
        match &self.timing {
            TimingFunction::Spring(config) if !property.springs() => {
                let (duration, easing) = config.easing();
                Transition {
                    duration,
                    timing: TimingFunction::Easing(easing),
                    ..self.clone()
                }
            }
            _ => self.clone(),
        }
    }

    /// Whether a change under this declaration starts a transition at all.
    /// CSS: the combined duration (`max(duration, 0) + delay`) must be
    /// positive. A spring's combined duration is its settle time, never zero.
    pub fn starts(&self) -> bool {
        match &self.timing {
            TimingFunction::Easing(_) => self.duration.max(0.0) + self.delay > 0.0,
            TimingFunction::Spring(_) => true,
        }
    }
}

/// A node's `transition` row: zero to [`MAX_TRANSITIONS`] declarations.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Transitions(pub Vec<Transition>);

impl Transitions {
    /// Explicit `transition: none`: covers no property.
    pub const NONE: Transitions = Transitions(Vec::new());

    /// An unset declaration: `all 0s ease 0s` still covers running transitions.
    pub fn initial() -> Self {
        Self(vec![Transition::INITIAL])
    }

    /// The declaration governing `property`, if any. When several cover it,
    /// the last wins (CSS Transitions §2.1: "the last one is used"). A
    /// spring on paint plays as [`Transition::governing`] says.
    pub fn matching(&self, property: Property) -> Option<&Transition> {
        self.0.iter().rev().find(|t| t.property.covers(property))
    }

    /// Validate every declaration and the count.
    pub fn validate(&self) -> Result<(), TransitionError> {
        if self.0.len() > MAX_TRANSITIONS {
            return Err(TransitionError::TooMany);
        }
        self.0.iter().try_for_each(Transition::validate)
    }

    /// Whether every number in every declaration is finite.
    pub fn is_finite(&self) -> bool {
        self.0.iter().all(Transition::is_finite)
    }
}

/// A transition in flight for one property of one node.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Running {
    property: Property,
    /// Value at the start of the transition.
    pub from: Value,
    /// Value at the end (the current target).
    pub to: Value,
    /// Clock time the transition starts moving (change time plus delay).
    pub start: f64,
    /// The curve and its effective duration.
    pub curve: Curve,
    /// The value a reversal is measured against (CSS Transitions 1 §3.1).
    pub reversing_adjusted_start: Value,
    /// The share of the authored duration to use (CSS Transitions 1 §3.1).
    pub reversing_shortening: f64,
    /// While it waits for the first presented frame (LLP 1003.001 D1), the
    /// engine time it began: every reading is at that time, so it does not
    /// move until the frame starts it.
    pub pending: Option<f64>,
}

/// The shape of a running transition's progress.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Curve {
    /// Eased over `duration` seconds (after any shortening).
    Easing { easing: Easing, duration: f64 },
    /// A spring released at `from` with `velocity`, which ends `settle`
    /// seconds after it starts: an underdamped spring passes through rest
    /// before then, so a sample at rest does not end it (LLP 1003 §4, §7).
    Spring {
        config: SpringConfig,
        velocity: Value,
        settle: f64,
    },
}

/// What a running transition looks like at one instant.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct RunningSample {
    pub value: Value,
    pub velocity: Value,
    pub done: bool,
}

impl Running {
    /// Start a transition under `declaration` at `now`.
    pub fn start(
        declaration: &Transition,
        property: Property,
        (from, to): (Value, Value),
        velocity: Value,
        now: f64,
        reversing_adjusted_start: Value,
        reversing_shortening: f64,
    ) -> Running {
        Self::start_with(
            declaration,
            property,
            (from, to),
            velocity,
            now,
            (reversing_adjusted_start, reversing_shortening),
            |config, displacement, velocity| {
                if property == Property::Rotate {
                    config.settle_time(-crate::rotation::distance(from, to), velocity.x)
                } else {
                    settle_time(config, displacement, velocity)
                }
            },
        )
    }

    fn start_with(
        declaration: &Transition,
        property: Property,
        (from, to): (Value, Value),
        velocity: Value,
        now: f64,
        (reversing_adjusted_start, reversing_shortening): (Value, f64),
        settle: impl FnOnce(&SpringConfig, Value, Value) -> f64,
    ) -> Running {
        let (curve, delay) = match &declaration.timing {
            TimingFunction::Easing(easing) => (
                Curve::Easing {
                    easing: easing.clone(),
                    duration: declaration.duration * reversing_shortening,
                },
                // The shortening factor scales a negative delay (CSS Transitions 1 §3.1).
                if declaration.delay >= 0.0 {
                    declaration.delay
                } else {
                    declaration.delay * reversing_shortening
                },
            ),
            TimingFunction::Spring(config) => (
                Curve::Spring {
                    config: *config,
                    velocity,
                    settle: 0.0,
                },
                declaration.delay,
            ),
        };
        // A modern colour at either end moves both in Oklab (CSS Color 4 §12.1).
        let (from, to, reversing_adjusted_start, curve) = if from.oklab != to.oklab {
            let curve = match curve {
                Curve::Spring { config, .. } => Curve::Spring {
                    config,
                    velocity: Value::oklab(0.0, 0.0, 0.0, 0.0),
                    settle: 0.0,
                },
                easing => easing,
            };
            (
                from.to_oklab(),
                to.to_oklab(),
                reversing_adjusted_start.to_oklab(),
                curve,
            )
        } else {
            (from, to, reversing_adjusted_start, curve)
        };
        let curve = match curve {
            Curve::Spring {
                config, velocity, ..
            } => {
                let settle = settle(&config, from - to, velocity);
                Curve::Spring {
                    config,
                    velocity,
                    settle,
                }
            }
            easing => easing,
        };
        Running {
            property,
            from,
            to,
            start: now + delay,
            curve,
            reversing_adjusted_start,
            reversing_shortening,
            pending: None,
        }
    }

    /// The reversing-adjusted start and shortening factor of a transition
    /// to `target` that interrupts this one at `current` (CSS Transitions 1
    /// §3.1). An easing back to this one's reversing-adjusted start is
    /// measured from its end and shortened by [`Running::reversing_factor`].
    /// Any other interruption is measured from `current` at full duration.
    pub(crate) fn reversal(
        &self,
        declaration: &Transition,
        current: Value,
        target: Value,
        now: f64,
    ) -> (Value, f64) {
        if matches!(declaration.timing, TimingFunction::Easing(_))
            && target.same(self.reversing_adjusted_start)
        {
            (self.to, self.reversing_factor(now))
        } else {
            (current, 1.0)
        }
    }

    /// The shortening factor for a reversal at `now` (CSS Transitions 1
    /// §3.1). It accounts for this transition's progress and its own
    /// shortening factor.
    pub(crate) fn reversing_factor(&self, now: f64) -> f64 {
        (self.easing_progress(now) * self.reversing_shortening + (1.0 - self.reversing_shortening))
            .abs()
            .clamp(0.0, 1.0)
    }

    /// The easing's output progress at `now` (CSS Transitions 1 §3.1). A
    /// spring reports `1`.
    pub fn easing_progress(&self, now: f64) -> f64 {
        let now = self.pending.unwrap_or(now);
        match &self.curve {
            Curve::Easing { easing, duration } => {
                if now <= self.start {
                    easing.progress(0.0)
                } else if *duration <= 0.0 {
                    1.0
                } else {
                    easing.progress((now - self.start) / duration)
                }
            }
            Curve::Spring { .. } => 1.0,
        }
    }

    /// Sample at `now`. Before `start` (in the delay) the value is `from`;
    /// while pending, at the time it began.
    pub fn sample(&self, now: f64) -> RunningSample {
        let now = self.pending.unwrap_or(now);
        match &self.curve {
            Curve::Easing { easing, duration } => {
                if now < self.start {
                    return RunningSample {
                        value: self.from,
                        velocity: Value::ZERO,
                        done: false,
                    };
                }
                if *duration <= 0.0 || now - self.start >= *duration {
                    return RunningSample {
                        value: self.to,
                        velocity: Value::ZERO,
                        done: true,
                    };
                }
                let input = (now - self.start) / duration;
                let progress = easing.progress(input);
                // Velocity by a central difference over one sample step; only
                // a spring started from this value reads it.
                let step = 1.0 / crate::spring::SAMPLE_RATE;
                let before = easing.progress(((now - self.start) - step).max(0.0) / duration);
                let after = easing.progress(((now - self.start) + step).min(*duration) / duration);
                let slope = (after - before) / (2.0 * step);
                let distance = if self.property == Property::Rotate {
                    Value::scalar(crate::rotation::distance(self.from, self.to))
                } else {
                    self.to - self.from
                };
                RunningSample {
                    value: self.interpolate(progress),
                    velocity: distance.map(|d| d * slope),
                    done: false,
                }
            }
            Curve::Spring {
                config,
                velocity,
                settle,
            } => {
                if now < self.start {
                    return RunningSample {
                        value: self.from,
                        velocity: *velocity,
                        done: false,
                    };
                }
                let elapsed = now - self.start;
                let (value, speed) = self.spring_sample(config, *velocity, elapsed);
                if elapsed >= *settle {
                    return RunningSample {
                        value: self.to,
                        velocity: Value::ZERO,
                        done: true,
                    };
                }
                RunningSample {
                    value,
                    velocity: speed,
                    done: false,
                }
            }
        }
    }

    fn interpolate(&self, progress: f64) -> Value {
        if self.property == Property::Rotate {
            crate::rotation::interpolate_axis(self.from, self.to, progress)
        } else {
            self.from.lerp(self.to, progress)
        }
    }

    fn spring_sample(&self, config: &SpringConfig, velocity: Value, t: f64) -> (Value, Value) {
        if self.property == Property::Rotate {
            let distance = crate::rotation::distance(self.from, self.to);
            let sample = config.sample(-distance, velocity.x, t);
            if distance == 0.0 {
                let v = Value {
                    x: self.to.x + sample.displacement,
                    ..self.to
                };
                return (v, Value::scalar(sample.velocity));
            }
            (
                self.interpolate(1.0 + sample.displacement / distance),
                Value::scalar(sample.velocity),
            )
        } else {
            let (moved, speed) = spring(config, self.from - self.to, velocity, t);
            (self.to + moved, speed)
        }
    }

    /// A running spring, lowered: its values on the [`SAMPLE_RATE`] grid from
    /// release to rest, evenly spaced, the last exactly `to`. `None` for an
    /// easing (CSS plays those itself). Each value is the same bits
    /// [`Running::sample`] would return at that grid time, so a host that
    /// plays these with linear interpolation shows the native curve.
    ///
    /// [`SAMPLE_RATE`]: crate::spring::SAMPLE_RATE
    pub fn spring_frames(&self) -> Option<(f64, Vec<Value>)> {
        let Curve::Spring {
            config, velocity, ..
        } = &self.curve
        else {
            return None;
        };
        let duration = self.end_time() - self.start;
        if duration <= 0.0 {
            return Some((0.0, vec![self.from, self.to]));
        }
        let count = (duration * crate::spring::SAMPLE_RATE).round() as usize;
        let mut values = Vec::with_capacity(count + 1);
        for n in 0..count {
            let t = n as f64 / crate::spring::SAMPLE_RATE;
            values.push(self.spring_sample(config, *velocity, t).0);
        }
        values.push(self.to);
        Some((duration, values))
    }

    /// When the transition ends if the frame at `now` starts it: its own end,
    /// or a pending one's moved by the wait (LLP 1003.001 D1).
    pub fn end_at(&self, now: f64) -> f64 {
        self.end_time() + self.wait(now)
    }

    /// When it starts moving if the frame at `now` starts it.
    pub fn start_at(&self, now: f64) -> f64 {
        self.start + self.wait(now)
    }

    fn wait(&self, now: f64) -> f64 {
        self.pending.map_or(0.0, |begin| (now - begin).max(0.0))
    }

    /// When the transition ends, on the clock. A spring's end is its settle
    /// time on the sample grid.
    pub fn end_time(&self) -> f64 {
        match &self.curve {
            Curve::Easing { duration, .. } => self.start + duration.max(0.0),
            Curve::Spring { settle, .. } => self.start + settle,
        }
    }
}

/// A host-owned blend between live style values, sampled in seconds. The
/// endpoints are progress values so style commits can change the values being
/// blended without restarting the transition.
#[derive(Debug)]
pub struct TransitionProgress(Running);

impl TransitionProgress {
    /// Start a blend from `from` to `to` at `now` (seconds) under a
    /// governing declaration, or interrupt `previous` from where it is,
    /// reversing it as CSS Transitions 1 §3.1 says. For a spring, `settle`
    /// receives the displacement and the inherited speed in progress units
    /// and returns the time to rest in seconds. Return `None` when the
    /// declaration starts no transition ([`Transition::starts`]), the blend
    /// is at `to` and at rest, or it ends by `now`. The caller then shows `to`.
    pub fn retarget(
        declaration: &Transition,
        (from, to): (f64, f64),
        now: f64,
        previous: Option<&Self>,
        settle: impl FnOnce(&SpringConfig, f64, f64) -> f64,
    ) -> Option<Self> {
        if !declaration.starts() {
            return None;
        }
        let (from, to) = (Value::scalar(from), Value::scalar(to));
        let (current, velocity, reversal) = match previous {
            Some(Self(running)) => {
                let sample = running.sample(now);
                let reversal = if sample.done {
                    (sample.value, 1.0)
                } else {
                    running.reversal(declaration, sample.value, to, now)
                };
                (sample.value, sample.velocity, reversal)
            }
            None => (from, Value::ZERO, (from, 1.0)),
        };
        if current == to
            && (velocity == Value::ZERO || !matches!(declaration.timing, TimingFunction::Spring(_)))
        {
            return None;
        }
        // A progress interpolates linearly, as opacity does.
        let running = Running::start_with(
            declaration,
            Property::Opacity,
            (current, to),
            velocity,
            now,
            reversal,
            |config, displacement, velocity| settle(config, displacement.x, velocity.x),
        );
        (running.end_time() > now).then_some(Self(running))
    }

    /// The blend's progress now.
    pub fn value(&self, now: f64) -> f64 {
        self.0.sample(now).value.x
    }

    /// The seconds on the clock at which the blend starts moving: its
    /// change time plus its delay.
    pub fn start_time(&self) -> f64 {
        self.0.start
    }

    /// The seconds on the clock until which the blend is active.
    pub fn end_time(&self) -> f64 {
        self.0.end_time()
    }
}

/// Seconds until a spring released at `displacement` with `velocity`
/// settles: the latest of its components' settle times on the sample grid.
fn settle_time(config: &SpringConfig, displacement: Value, velocity: Value) -> f64 {
    let (d, v) = (displacement.components(), velocity.components());
    (0..4)
        .filter(|&i| d[i] != 0.0 || v[i] != 0.0)
        .map(|i| config.settle_time(d[i], v[i]))
        .fold(0.0, f64::max)
}

/// A spring's displacement and velocity per component at `t`. A component
/// neither displaced nor moving stays exactly zero, and costs nothing.
fn spring(config: &SpringConfig, displacement: Value, velocity: Value, t: f64) -> (Value, Value) {
    let (d, v) = (displacement.components(), velocity.components());
    let (mut moved, mut speed) = ([0.0; 4], [0.0; 4]);
    for i in 0..4 {
        if d[i] == 0.0 && v[i] == 0.0 {
            continue;
        }
        let s = config.sample(d[i], v[i], t);
        (moved[i], speed[i]) = (s.displacement, s.velocity);
    }
    let value = |c: [f64; 4]| Value::four(c[0], c[1], c[2], c[3]);
    (value(moved), value(speed))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::easing::LinearStop;

    fn linear(duration: f64) -> Transition {
        Transition::new(
            TransitionProperty::All,
            duration,
            TimingFunction::Easing(Easing::Linear),
        )
    }

    fn eased(
        declaration: &Transition,
        ends: (f64, f64),
        now: f64,
        previous: Option<&TransitionProgress>,
    ) -> Option<TransitionProgress> {
        TransitionProgress::retarget(declaration, ends, now, previous, |_, _, _| {
            unreachable!("an easing has no spring")
        })
    }

    #[test]
    fn an_interrupted_blend_reverses_with_css_shortening() {
        let declaration = linear(0.2);
        let pressing = eased(&declaration, (0., 1.), 0., None).unwrap();
        assert_eq!(pressing.value(0.1), 0.5);
        // Back halfway through: from where it is, over half the duration.
        let releasing = eased(&declaration, (1., 0.), 0.1, Some(&pressing)).unwrap();
        assert_eq!(releasing.value(0.1), 0.5);
        assert!((releasing.end_time() - 0.2).abs() < 1e-12);
        assert!((releasing.value(0.15) - 0.25).abs() < 1e-12);
        // Reversing the reversal keeps its shortening: 0.5 · 0.5 + 0.5.
        let again = eased(&declaration, (0., 1.), 0.15, Some(&releasing)).unwrap();
        assert!((again.end_time() - 0.3).abs() < 1e-12);
        assert!((again.value(0.3) - 1.).abs() < 1e-12);
    }

    #[test]
    fn a_blend_after_a_finished_one_takes_its_whole_duration() {
        // An easing whose output ends short of 1 would shorten a reversal.
        let declaration = Transition::new(
            TransitionProperty::All,
            0.2,
            TimingFunction::Easing(Easing::PiecewiseLinear(vec![
                LinearStop {
                    input: 0.,
                    output: 0.,
                },
                LinearStop {
                    input: 1.,
                    output: 0.5,
                },
            ])),
        );
        let pressing = eased(&declaration, (0., 1.), 0., None).unwrap();
        let releasing = eased(&declaration, (1., 0.), 0.5, Some(&pressing)).unwrap();
        assert_eq!(releasing.value(0.5), 1.);
        assert!((releasing.end_time() - 0.7).abs() < 1e-12);
    }

    #[test]
    fn a_blend_with_nothing_to_play_is_none() {
        let declaration = linear(0.2);
        assert!(eased(&declaration, (1., 1.), 0., None).is_none());
        let pressing = eased(&declaration, (0., 1.), 0., None).unwrap();
        assert!(eased(&declaration, (0., 1.), 0.3, Some(&pressing)).is_none());
        assert!(eased(&Transition::INITIAL, (0., 1.), 0., None).is_none());
        let spring = Transition::new(
            TransitionProperty::All,
            0.,
            TimingFunction::Spring(SpringConfig {
                stiffness: 100.,
                damping: 10.,
                mass: 1.,
            }),
        );
        let settled = TransitionProgress::retarget(&spring, (0., 1.), 0., None, |_, _, _| 0.);
        assert!(
            settled.is_none(),
            "a spring that settles at once ends by now"
        );
    }

    #[test]
    fn an_interrupted_spring_keeps_its_velocity() {
        let spring = Transition::new(
            TransitionProperty::All,
            0.,
            TimingFunction::Spring(SpringConfig {
                stiffness: 100.,
                damping: 10.,
                mass: 1.,
            }),
        );
        let settle = |config: &SpringConfig, distance, speed| config.settle_time(distance, speed);
        let pressing =
            TransitionProgress::retarget(&spring, (0., 1.), 0., None, |config, distance, speed| {
                assert_eq!((distance, speed), (-1., 0.));
                settle(config, distance, speed)
            })
            .unwrap();
        let at = pressing.value(0.05);
        let mut launched = 0.;
        let releasing = TransitionProgress::retarget(
            &spring,
            (1., 0.),
            0.05,
            Some(&pressing),
            |config, distance, speed| {
                assert_eq!(distance, at);
                launched = speed;
                settle(config, distance, speed)
            },
        )
        .unwrap();
        assert!(launched > 0., "it still moves toward the press");
        assert_eq!(releasing.value(0.05), at);
        assert!(releasing.value(0.06) > at);
        assert!(releasing.value(releasing.end_time()).abs() < 1e-12);
    }
}
