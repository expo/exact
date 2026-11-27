//! What the painter animates on its own, painted per frame from the host's
//! clock: a number rolling, a field's label floating, a ripple, a symbol's
//! effect (`symbol_effect.rs`).
use super::*;

impl Painter {
    /// Something the painter animates owes a frame: a number rolling, a
    /// field's label floating, a ripple spreading or fading, a symbol's
    /// effect playing.
    pub(crate) fn moving(&self) -> bool {
        let (now, still) = (self.numerals.now, self.numerals.still);
        self.numerals.rolling()
            || self.labels.moving()
            || self.ripples.moving(now, still)
            || self.symbol_effects.moving(now)
    }

    /// When the last of them comes to rest, ms.
    pub(crate) fn settles_at(&self) -> Option<f64> {
        let now = self.numerals.now;
        (self.numerals.settles_at().into_iter())
            .chain(self.labels.settles_at())
            .chain(self.ripples.settles_at(now))
            .chain(self.symbol_effects.settles_at(now))
            .reduce(f64::max)
    }

    /// After a paint, each of them forgets what is gone.
    pub(super) fn motion_painted(&mut self, kernel: &Kernel) {
        self.numerals.painted();
        self.labels.painted();
        self.ripples.painted(self.numerals.now, self.numerals.still);
        self.symbol_effects.painted(kernel);
    }
}
