//! A box's `clip-path` as an animatable value (LLP 1003.003 D2).
//!
//! @ref CSS Masking 1 §5.1 (`clip-path`, "Animation type: by computed
//! value"); CSS Shapes 1 §3.1.1.2 (interpolating `path()`)
//!
//! Only a `path()` moves: its fill rule and its path data made absolute, the
//! value a `d` transition keeps ([`PathValue`]), so the two interpolate by one
//! rule. `none` and a basic shape are no value here and change at once.

use crate::path::PathValue;

/// A `path()` clip: its fill rule and its commands, every one absolute.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClipValue {
    /// `evenodd` rather than the default `nonzero`.
    pub even_odd: bool,
    /// The path data, every command absolute.
    pub path: PathValue,
}

impl ClipValue {
    /// Whether the two clips interpolate: the same fill rule and the same
    /// commands, one for one.
    pub fn interpolable(&self, other: &ClipValue) -> bool {
        self.even_odd == other.even_odd && self.path.interpolable(&other.path)
    }

    /// The clip at `progress` from `self` to `to`, which must be
    /// [`ClipValue::interpolable`] with it.
    pub fn lerp(&self, to: &ClipValue, progress: f64) -> ClipValue {
        ClipValue {
            even_odd: self.even_odd,
            path: self.path.lerp(&to.path, progress),
        }
    }

    /// Whether every number is finite.
    pub fn is_finite(&self) -> bool {
        self.path.is_finite()
    }

    /// The clip as CSS writes it, which draws it: `path(evenodd, 'M0 0…')`.
    pub fn to_css(&self) -> String {
        let rule = if self.even_odd { "evenodd, " } else { "" };
        format!("path({rule}'{}')", self.path.to_d())
    }
}
