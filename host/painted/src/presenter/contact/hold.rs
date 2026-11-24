//! A recognized drag's hold and the samples its release velocity reads.
use super::HeldKind;
use exact_motion::{HoldStart, Value, VelocityTracker};

pub(in crate::presenter) struct Hold {
    pub(in crate::presenter) primary: HoldStart,
    pub(in crate::presenter) velocity: VelocityTracker,
    pub(in crate::presenter) kind: HeldKind,
    /// The newest sample's input time (ms) and the input's travel then, for
    /// [`Hold::sample`].
    pub(in crate::presenter) sampled: Option<(f64, Value)>,
    /// The next sample is the contact's lift.
    pub(in crate::presenter) lifting: bool,
}
impl Hold {
    /// Record the held `value` at `at_ms`, the input's own time, which the
    /// release velocity reads (LLP 1057.001 §3), after `input` of travel. A
    /// lift where the input already was, soon after, is no sample
    /// (`exact_motion::gesture::LIFT_STILL`).
    pub(in crate::presenter) fn sample(&mut self, at_ms: f64, input: Value, value: Value) {
        if let Some((at, was)) = self.sampled.filter(|_| self.lifting) {
            if !exact_motion::gesture::lift_samples(input != was, (at_ms - at) / 1000.) {
                return;
            }
        }
        self.velocity.push(at_ms / 1000., value);
        self.sampled = Some((at_ms, input));
    }
    /// The release velocity, at the newest sample's input time.
    pub(in crate::presenter) fn release_velocity(&self) -> Value {
        self.sampled
            .map_or(Value::ZERO, |(at, _)| self.velocity.estimate(at / 1000.))
    }
}
