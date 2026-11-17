//! CSS `filter` as an animatable value (LLP 1003.002).
//!
//! @ref Filter Effects 1 §14.1 (interpolation of filter function lists),
//! §6.4 (of filter functions), §6.1 (initial values for interpolation);
//! CSS Values 4 §3.1 (an interpolated value is clamped to its range)
//!
//! A filter is a list of functions, each with its numbers in CSS's units:
//! `blur()` and `drop-shadow()`'s offsets and blur in pixels, `hue-rotate()`
//! in degrees, the others a number (1 is 100%). Two lists interpolate when
//! each function at the same index is the same function; the shorter list
//! is padded at its end with the longer's remaining functions at their
//! initial values for interpolation, so `none` (the empty list) pads in
//! full. Any other pair is not interpolable, and a `transition` starts
//! nothing (CSS Transitions 1 §3), as Chrome shows it. A list naming a
//! `filter` element (`url()`) is never a [`FilterValue`]: the kernel gives
//! the engine none for it.
//!
//! A `drop-shadow()`'s colour is kept under both appearances, premultiplied
//! (CSS Color 4 §12.3), and each moves as a colour does; a host draws the
//! one its view shows (LLP 1003.002 D7). `currentcolor` is kept as a
//! share of the shadow's colour, as Blink keeps it: a shadow in
//! `currentcolor` is all share and no colour of its own, and between it and
//! another colour both move, so the shadow follows the element's `color` as
//! it is when drawn, a moving one included, and a change of `color` alone
//! is no change of the filter.

use crate::property::Value;

/// A CSS filter function.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FilterKind {
    /// `blur(<length>)`.
    Blur,
    /// `brightness(<number>)`.
    Brightness,
    /// `contrast(<number>)`.
    Contrast,
    /// `drop-shadow(<color> <dx> <dy> <blur>)`.
    DropShadow,
    /// `grayscale(<number>)`.
    Grayscale,
    /// `hue-rotate(<angle>)`.
    HueRotate,
    /// `invert(<number>)`.
    Invert,
    /// `opacity(<number>)`.
    Opacity,
    /// `saturate(<number>)`.
    Saturate,
    /// `sepia(<number>)`.
    Sepia,
}

impl FilterKind {
    /// The argument a padded function starts from (Filter Effects 1 §6.1).
    fn initial(self) -> f64 {
        match self {
            FilterKind::Brightness
            | FilterKind::Contrast
            | FilterKind::Opacity
            | FilterKind::Saturate => 1.0,
            _ => 0.0,
        }
    }

    /// The argument clamped to the function's range: a radius and the
    /// multipliers are 0 or more, the proportions 0 to 1, an angle anything.
    fn clamp(self, v: f64) -> f64 {
        match self {
            FilterKind::Blur
            | FilterKind::Brightness
            | FilterKind::Contrast
            | FilterKind::Saturate => v.max(0.0),
            FilterKind::Grayscale
            | FilterKind::Invert
            | FilterKind::Opacity
            | FilterKind::Sepia => v.clamp(0.0, 1.0),
            FilterKind::HueRotate | FilterKind::DropShadow => v,
        }
    }
}

/// One function of a filter list.
#[derive(Debug, Clone, Copy)]
pub struct FilterStep {
    /// Which function.
    pub kind: FilterKind,
    /// Its numbers: the argument in `[0]`; a `drop-shadow()`'s x and y
    /// offsets and blur radius in `[0]`, `[1]` and `[2]`.
    pub args: [f64; 3],
    /// A `drop-shadow()`'s colour under a light appearance, premultiplied;
    /// transparent for any other function.
    pub light: Value,
    /// Its colour under a dark appearance.
    pub dark: Value,
    /// A `drop-shadow()`'s share of `currentcolor`, 0 to 1: the colour
    /// drawn is `light` or `dark` plus this share of the element's colour,
    /// premultiplied. A shadow in `currentcolor` is 1 with no colour of its
    /// own.
    pub current: f64,
}

impl PartialEq for FilterStep {
    fn eq(&self, other: &FilterStep) -> bool {
        self.kind == other.kind
            && self.args == other.args
            && self.current == other.current
            && self.light.same(other.light)
            && self.dark.same(other.dark)
    }
}

impl FilterStep {
    /// A function of one argument.
    pub fn new(kind: FilterKind, arg: f64) -> FilterStep {
        FilterStep {
            kind,
            args: [arg, 0.0, 0.0],
            light: Value::ZERO,
            dark: Value::ZERO,
            current: 0.0,
        }
    }

    /// A `drop-shadow()` with its colour under each appearance.
    pub fn shadow(dx: f64, dy: f64, blur: f64, light: Value, dark: Value) -> FilterStep {
        FilterStep {
            kind: FilterKind::DropShadow,
            args: [dx, dy, blur],
            light,
            dark,
            current: 0.0,
        }
    }

    /// A `drop-shadow()` in `currentcolor`.
    pub fn current_shadow(dx: f64, dy: f64, blur: f64) -> FilterStep {
        FilterStep {
            current: 1.0,
            ..FilterStep::shadow(dx, dy, blur, Value::ZERO, Value::ZERO)
        }
    }

    /// The function at its initial value for interpolation: a shadow of
    /// no offset or blur in transparent black.
    fn initial(kind: FilterKind) -> FilterStep {
        FilterStep::new(kind, kind.initial())
    }

    fn lerp(&self, to: &FilterStep, progress: f64) -> FilterStep {
        let mix = |a: f64, b: f64| a + (b - a) * progress;
        let mut args = [0.0; 3];
        for (i, arg) in args.iter_mut().enumerate() {
            *arg = mix(self.args[i], to.args[i]);
        }
        args[0] = self.kind.clamp(args[0]);
        if self.kind == FilterKind::DropShadow {
            args[2] = args[2].max(0.0);
        }
        FilterStep {
            kind: self.kind,
            args,
            light: clamp_color(self.light.lerp(to.light, progress)),
            dark: clamp_color(self.dark.lerp(to.dark, progress)),
            current: mix(self.current, to.current).clamp(0.0, 1.0),
        }
    }
}

/// A premultiplied colour clamped as CSS clamps an interpolated one: alpha
/// to 0–1 and, in sRGB, each channel to 0 through its alpha. An Oklab
/// colour keeps its coordinates, out of gamut included (LLP 1100 D2).
fn clamp_color(c: Value) -> Value {
    let a = c.w.clamp(0.0, 1.0);
    if c.oklab {
        return Value { w: a, ..c };
    }
    Value {
        w: a,
        ..c.map(|v| v.clamp(0.0, a))
    }
}

/// A filter's functions, in order; empty is `none`.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilterValue(pub Vec<FilterStep>);

impl FilterValue {
    /// Whether the two lists interpolate: the same function at each index
    /// both have.
    pub fn interpolable(&self, other: &FilterValue) -> bool {
        self.0.iter().zip(&other.0).all(|(a, b)| a.kind == b.kind)
    }

    /// The list at `progress` from `self` to `to`, which must be
    /// [`FilterValue::interpolable`] with it: as long as the longer, each
    /// number linearly (an easing that overshoots extrapolates), clamped to
    /// its function's range.
    pub fn lerp(&self, to: &FilterValue, progress: f64) -> FilterValue {
        debug_assert!(self.interpolable(to));
        let n = self.0.len().max(to.0.len());
        FilterValue(
            (0..n)
                .map(|i| {
                    let (a, b) = match (self.0.get(i), to.0.get(i)) {
                        (Some(a), Some(b)) => (*a, *b),
                        (Some(a), None) => (*a, FilterStep::initial(a.kind)),
                        (None, Some(b)) => (FilterStep::initial(b.kind), *b),
                        (None, None) => unreachable!("within the longer list"),
                    };
                    a.lerp(&b, progress)
                })
                .collect(),
        )
    }

    /// Whether every number is finite.
    pub fn is_finite(&self) -> bool {
        self.0.iter().all(|s| {
            s.args.iter().all(|a| a.is_finite())
                && s.light.is_finite()
                && s.dark.is_finite()
                && s.current.is_finite()
        })
    }
}
