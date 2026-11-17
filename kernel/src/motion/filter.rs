//! A `filter` row as the engine moves it (LLP 1003.002 D2, D7): the
//! kernel's [`FilterList`] to the engine's [`FilterValue`] at sync, and the
//! presented value back to a list a host draws.

use crate::style::{Color, ColorValue};
use crate::svg::filter::{FilterFn, FilterList};
use exact_motion::{FilterKind, FilterStep, FilterValue, Value};

/// A premultiplied colour as 8-bit sRGB, the precision a filter chain's
/// colour has on every native host (`svg::scene::filter`).
fn drawn(v: Value) -> Color {
    Color(u32::from_be_bytes(v.to_rgba8()))
}

/// `list` as the engine moves it, a `drop-shadow()`'s colour resolved under
/// both appearances as paint motion resolves one, and `currentcolor` kept
/// as a share; `None` for a list that names a `filter` element, or a shadow
/// in a profile's colour, neither of which interpolates (LLP 1100 D3).
pub fn filter_value(list: &FilterList) -> Option<FilterValue> {
    list.0
        .iter()
        .map(|f| {
            let one = |kind, v: f32| Some(FilterStep::new(kind, v as f64));
            match *f {
                FilterFn::Url(_) => None,
                FilterFn::Blur(v) => one(FilterKind::Blur, v),
                FilterFn::Brightness(v) => one(FilterKind::Brightness, v),
                FilterFn::Contrast(v) => one(FilterKind::Contrast, v),
                FilterFn::Grayscale(v) => one(FilterKind::Grayscale, v),
                FilterFn::HueRotate(v) => one(FilterKind::HueRotate, v),
                FilterFn::Invert(v) => one(FilterKind::Invert, v),
                FilterFn::Opacity(v) => one(FilterKind::Opacity, v),
                FilterFn::Saturate(v) => one(FilterKind::Saturate, v),
                FilterFn::Sepia(v) => one(FilterKind::Sepia, v),
                FilterFn::DropShadow(dx, dy, blur, c) => {
                    let (dx, dy, blur) = (dx as f64, dy as f64, blur as f64);
                    Some(match c {
                        Some(c) => FilterStep::shadow(
                            dx,
                            dy,
                            blur,
                            super::color(c, false)?,
                            super::color(c, true)?,
                        ),
                        None => FilterStep::current_shadow(dx, dy, blur),
                    })
                }
            }
        })
        .collect::<Option<Vec<_>>>()
        .map(FilterValue)
}

/// The list a host draws for a presented [`FilterValue`]: a shadow's colour
/// one value, or a `light-dark()` pair when its appearances differ, with its
/// share of `currentcolor` resolved against `text`, the element's colour as
/// it shows now; `currentcolor` itself while the share is whole.
pub fn filter_list(value: &FilterValue, text: ColorValue) -> FilterList {
    FilterList(
        value
            .0
            .iter()
            .map(|s| {
                let a = s.args[0] as f32;
                match s.kind {
                    FilterKind::Blur => FilterFn::Blur(a),
                    FilterKind::Brightness => FilterFn::Brightness(a),
                    FilterKind::Contrast => FilterFn::Contrast(a),
                    FilterKind::Grayscale => FilterFn::Grayscale(a),
                    FilterKind::HueRotate => FilterFn::HueRotate(a),
                    FilterKind::Invert => FilterFn::Invert(a),
                    FilterKind::Opacity => FilterFn::Opacity(a),
                    FilterKind::Saturate => FilterFn::Saturate(a),
                    FilterKind::Sepia => FilterFn::Sepia(a),
                    FilterKind::DropShadow => {
                        FilterFn::DropShadow(a, s.args[1] as f32, s.args[2] as f32, shadow(s, text))
                    }
                }
            })
            .collect(),
    )
}

/// A presented shadow's colour (`None`: `currentcolor`).
fn shadow(s: &FilterStep, text: ColorValue) -> Option<ColorValue> {
    let own = s.light == Value::ZERO && s.dark == Value::ZERO;
    if s.current == 1.0 && own {
        return None;
    }
    let under = |dark: bool, mine: Value| {
        let current = (s.current > 0.0)
            .then(|| super::color(text, dark))
            .flatten()
            .map_or(Value::ZERO, |c| c.map(|v| v * s.current));
        drawn(mine + current)
    };
    let (light, dark) = (under(false, s.light), under(true, s.dark));
    Some(if light == dark {
        ColorValue::Fixed(light)
    } else {
        ColorValue::LightDark(light, dark)
    })
}
