//! CSS's 3D transform properties (LLP 1077 D8): `rotate`'s axis forms,
//! `translate`'s z, and `perspective`. The `rotate` attribute sets angle
//! and axis rows together; the motion seam joins them into one rotation.

use super::{parse_pixel_length, Dimension, StyleId, StyleValue, StyleValueError, Vec2};

impl StyleValue<'_> {
    pub(crate) fn vec2(&self, style: StyleId) -> Result<Vec2, StyleValueError> {
        if style == StyleId::Scale {
            let value = match self {
                StyleValue::Number(n) => Some(exact_motion::Value::new(*n, *n)),
                StyleValue::Percent(n) => Some(exact_motion::Value::new(*n / 100.0, *n / 100.0)),
                StyleValue::Vec2(x, y) => Some(exact_motion::Value::new(*x as f64, *y as f64)),
                StyleValue::Text(t) => exact_motion::property::scale(t),
                _ => None,
            };
            return value
                .filter(|v| v.is_finite() && (v.x as f32).is_finite() && (v.y as f32).is_finite())
                .map(|v| Vec2 {
                    x: v.x as f32,
                    y: v.y as f32,
                })
                .ok_or(StyleValueError::WrongKind {
                    style,
                    expected: "one or two scale factors (numbers or percentages), x then y",
                });
        }
        if matches!(
            style,
            StyleId::PressTranslate | StyleId::PressTranslatePercent
        ) && matches!(self, StyleValue::Text(t) if t.split_ascii_whitespace().count() > 2)
        {
            return Err(StyleValueError::WrongKind {
                style,
                expected:
                    "x and optional y only: a press is a 2D offset; use transitions for 3D motion",
            });
        }
        let style = match style {
            StyleId::PressTranslate => StyleId::Translate,
            StyleId::PressTranslatePercent => StyleId::TranslatePercent,
            other => other,
        };
        match self {
            StyleValue::Vec2(x, y) if x.is_finite() && y.is_finite() => Ok(Vec2 { x: *x, y: *y }),
            StyleValue::Text(t) if style == StyleId::TranslateVelocity => velocity(t),
            // @ref LLP 1077 D8 — each row sets its part of translate (lengths, percentages, z).
            StyleValue::Text(t)
                if style == StyleId::Translate || style == StyleId::TranslatePercent =>
            {
                translate(t)
                    .map(|(px, pct)| if style == StyleId::Translate { px } else { pct })
                    .ok_or(StyleValueError::WrongKind {
                        style,
                        expected: "one or two lengths, each px (unitless zero allowed), a percentage of the box, or a calc() of px and a percentage such as calc(50% - 12px), then an optional z length in px",
                    })
            }
            // `translate: -50%`: one percentage, the x's, which the
            // compiler and the runner pass as a percentage.
            StyleValue::Percent(p)
                if p.is_finite()
                    && (style == StyleId::Translate || style == StyleId::TranslatePercent) =>
            {
                let x = if style == StyleId::Translate { 0.0 } else { *p as f32 };
                Ok(Vec2 { x, y: 0.0 })
            }
            _ => Err(StyleValueError::WrongKind {
                style,
                expected: "vec2",
            }),
        }
    }
}

#[cfg(target_arch = "wasm32")]
type Parsers = (
    fn(&str) -> Option<([f32; 3], f32)>,
    fn(&str) -> Option<RotateAxis>,
);
#[cfg(target_arch = "wasm32")]
static LINKED: std::sync::OnceLock<Parsers> = std::sync::OnceLock::new();

/// Link the rotation grammars on wasm (LLP 1047 D2). Native hosts parse directly.
pub fn link() {
    #[cfg(target_arch = "wasm32")]
    let _ = LINKED.set((rotate_grammar, RotateAxis::grammar));
}

pub(super) fn velocity(text: &str) -> Result<Vec2, StyleValueError> {
    let mut numbers = text.split_whitespace().map(str::parse::<f32>);
    match (numbers.next(), numbers.next(), numbers.next()) {
        (Some(Ok(x)), Some(Ok(y)), None) if x.is_finite() && y.is_finite() => Ok(Vec2 { x, y }),
        _ => Err(StyleValueError::WrongKind {
            style: StyleId::TranslateVelocity,
            expected: "two finite numbers, x and y velocity in CSS px/s",
        }),
    }
}

/// The axis a `rotate` turns about, as authored (not normalised): `z` for
/// a bare angle, CSS's initial.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RotateAxis(pub [f32; 3]);

impl Default for RotateAxis {
    fn default() -> Self {
        Self([0.0, 0.0, 1.0])
    }
}

/// A rotation's axis and angle in degrees, using the motion grammar.
pub fn rotate(text: &str) -> Option<([f32; 3], f32)> {
    #[cfg(target_arch = "wasm32")]
    return LINKED.get().and_then(|p| (p.0)(text));
    #[cfg(not(target_arch = "wasm32"))]
    rotate_grammar(text)
}

fn rotate_grammar(text: &str) -> Option<([f32; 3], f32)> {
    let v = exact_motion::rotation::parse(text)?;
    let axis = exact_motion::rotation::axis(v).map(|v| v as f32);
    let angle = v.x as f32;
    (angle.is_finite() && axis != [0.0; 3] && axis.iter().all(|v| v.is_finite()))
        .then_some((axis, angle))
}

impl RotateAxis {
    /// The row's parse: the axis of a `rotate` value, or the axis alone
    /// (its wire form, `css()`). Keep its sign for angle-only keyframes.
    pub fn parse(css: &str) -> Option<Self> {
        #[cfg(target_arch = "wasm32")]
        return LINKED.get().and_then(|p| (p.1)(css));
        #[cfg(not(target_arch = "wasm32"))]
        Self::grammar(css)
    }

    fn grammar(css: &str) -> Option<Self> {
        let axis = rotate_grammar(css)
            .map(|(axis, _)| axis)
            .or_else(|| axis_only(css))?;
        Some(Self(axis))
    }

    /// Canonical CSS: `x`, `y`, `z`, or three numbers.
    pub fn css(&self) -> String {
        match self.0 {
            [1.0, 0.0, 0.0] => "x".into(),
            [0.0, 1.0, 0.0] => "y".into(),
            [0.0, 0.0, 1.0] => "z".into(),
            [x, y, z] => exact_num::text!(
                "{} {} {}",
                exact_num::Shortest32(x),
                exact_num::Shortest32(y),
                exact_num::Shortest32(z)
            ),
        }
    }

    /// Whether it turns out of the screen's plane.
    pub fn is_3d(&self) -> bool {
        self.0[0] != 0.0 || self.0[1] != 0.0
    }
}

/// An axis on its own: `x`, `y`, `z` or three numbers.
fn axis_only(text: &str) -> Option<[f32; 3]> {
    let words: Vec<&str> = text.split_ascii_whitespace().collect();
    match words[..] {
        [w] => match w.to_ascii_lowercase().as_str() {
            "x" => Some([1.0, 0.0, 0.0]),
            "y" => Some([0.0, 1.0, 0.0]),
            "z" => Some([0.0, 0.0, 1.0]),
            _ => None,
        },
        [x, y, z] => {
            let v = [x, y, z].map(|n| exact_num::parse_f32(n).ok().filter(|n| n.is_finite()));
            let [Some(x), Some(y), Some(z)] = v else {
                return None;
            };
            (x != 0.0 || y != 0.0 || z != 0.0).then_some([x, y, z])
        }
        _ => None,
    }
}

/// `translate`'s x and y: each a `<length-percentage>` of the node's own
/// border box (CSS Transforms 2 §5.1), as `(lengths, percentages)`: a
/// length in px (unitless zero allowed), a percentage, or a math function
/// that simplifies to px plus a percentage (`calc(50% - 12px)`), the used
/// value their sum. `none` is deliberately not zero: CSS gives the two
/// different containing-block and stacking semantics. A third length is z,
/// its own row (LLP 1077 D8).
pub fn translate(text: &str) -> Option<(Vec2, Vec2)> {
    let parts = super::sides::values(text);
    let (x, px) = translate_axis(parts.first()?)?;
    let (y, py) = parts
        .get(1)
        .map_or(Some((0.0, 0.0)), |p| translate_axis(p))?;
    match parts[..] {
        [_] | [_, _] => {}
        [_, _, z] if translate_length(z).is_some() => {}
        _ => return None,
    }
    Some((Vec2 { x, y }, Vec2 { x: px, y: py }))
}

/// One axis of `translate` as its px and its percentage.
fn translate_axis(token: &str) -> Option<(f32, f32)> {
    if let Some(n) = token.strip_suffix('%') {
        return parse_pixel_length(&format!("{n}px")).map(|p| (0.0, p));
    }
    match parse_pixel_length(token) {
        Some(l) => Some((l, 0.0)),
        None => match Dimension::parse_calc(token)? {
            Dimension::Points(l) => Some((l, 0.0)),
            Dimension::Percent(p) => Some((0.0, p)),
            Dimension::Calc(p, l) => Some((l, p)),
            _ => None,
        },
    }
}

/// `translate`'s z: a length in px, or a math function of px alone (CSS
/// takes no percentage there).
fn translate_length(token: &str) -> Option<f32> {
    parse_pixel_length(token).or_else(|| match Dimension::parse_calc(token)? {
        Dimension::Points(l) => Some(l),
        _ => None,
    })
}

/// `translate`'s z: its third length, 0 when it has two or fewer.
pub fn translate_z(text: &str) -> Option<f32> {
    match super::sides::values(text)[..] {
        [_] | [_, _] => Some(0.0),
        [_, _, z] => translate_length(z),
        _ => None,
    }
}

/// `perspective`: `none` (0) or a nonnegative length in px.
pub fn perspective(text: &str) -> Option<f32> {
    let t = text.trim();
    if t.eq_ignore_ascii_case("none") {
        return Some(0.0);
    }
    parse_pixel_length(t).filter(|n| *n >= 0.0)
}

/// `font-stretch` (CSS Fonts 4 §2.4) as a percentage of normal width: a
/// nonnegative `<percentage>` or one of the nine keywords.
pub fn font_stretch(text: &str) -> Option<f32> {
    let t = text.trim();
    if let Some(n) = t.strip_suffix('%') {
        return exact_num::parse_f32(n.trim_end())
            .ok()
            .filter(|n| n.is_finite() && *n >= 0.0);
    }
    const KEYWORDS: [(&str, f32); 9] = [
        ("ultra-condensed", 50.0),
        ("extra-condensed", 62.5),
        ("condensed", 75.0),
        ("semi-condensed", 87.5),
        ("normal", 100.0),
        ("semi-expanded", 112.5),
        ("expanded", 125.0),
        ("extra-expanded", 150.0),
        ("ultra-expanded", 200.0),
    ];
    KEYWORDS
        .iter()
        .find(|(k, _)| t.eq_ignore_ascii_case(k))
        .map(|(_, v)| *v)
}

/// The `f32` rows that take CSS text or a range of their own (LLP 1077):
/// `rotate`'s angle, `translate`'s z and `perspective` (D8), a symbol's
/// value (D11), `-webkit-text-stroke`'s width (D7) and `font-stretch`
/// (LLP 1019 D8). `None` for any other row, which the plain number parse
/// takes.
pub(crate) fn f32_row(
    value: &super::StyleValue,
    style: crate::StyleId,
) -> Option<Result<f32, crate::error::StyleValueError>> {
    use super::StyleValue;
    use crate::error::StyleValueError;
    use crate::StyleId;
    let wrong = |expected| Err(StyleValueError::WrongKind { style, expected });
    Some(match (style, value) {
        (StyleId::Rotate, StyleValue::Text(t)) => rotate(t).map(|(_, deg)| deg).map_or_else(
            || wrong("an angle, and optionally an axis: `x`, `y`, `z` or three numbers"),
            Ok,
        ),
        (StyleId::TranslateZ, StyleValue::Text(t)) => translate_z(t).map_or_else(
            || wrong("a z length in px, or a calc() of px, after x and y"),
            Ok,
        ),
        // `translate: -50%` has no z.
        (StyleId::TranslateZ, StyleValue::Percent(p)) if p.is_finite() => Ok(0.0),
        (StyleId::Perspective, StyleValue::Text(t)) => {
            perspective(t).map_or_else(|| wrong("`none` or a nonnegative length"), Ok)
        }
        (StyleId::Perspective, StyleValue::Number(n)) if n.is_nan() || *n < 0.0 => {
            wrong("`none` or a nonnegative length")
        }
        (StyleId::SymbolValue, StyleValue::Text(t)) if t.trim().eq_ignore_ascii_case("none") => {
            Ok(-1.0)
        }
        (StyleId::SymbolValue, StyleValue::Number(n)) if (0.0..=1.0).contains(n) || *n == -1.0 => {
            Ok(*n as f32)
        }
        (StyleId::SymbolValue, _) => wrong("`none` or a number from 0 to 1"),
        (StyleId::TextStrokeWidth, StyleValue::Number(n))
            if (*n as f32).is_finite() && *n >= 0.0 =>
        {
            Ok(*n as f32)
        }
        (StyleId::TextStrokeWidth, StyleValue::Text(t)) => super::stroke::parse(t)
            .map(|(w, _)| w)
            .map_err(|reason| StyleValueError::BadTextStroke { style, reason }),
        (StyleId::TextStrokeWidth, _) => wrong("a nonnegative width in px"),
        // @ref LLP 1019 D8 — CSS Fonts 4 §2.4: a nonnegative percentage or a
        // keyword, never a bare number.
        (StyleId::FontStretch, StyleValue::Percent(p)) if (*p as f32).is_finite() && *p >= 0.0 => {
            Ok(*p as f32)
        }
        (StyleId::FontStretch, StyleValue::Text(t)) => font_stretch(t).map_or_else(
            || wrong("a nonnegative percentage or a keyword such as `condensed`"),
            Ok,
        ),
        (StyleId::FontStretch, _) => {
            wrong("a nonnegative percentage or a keyword such as `condensed`")
        }
        // @ref LLP 1057.001.000 D1 — a zoom, as CSS `zoom` takes one: a
        // number or a percentage of 1, above 0.
        (StyleId::MinZoom | StyleId::MaxZoom, StyleValue::Number(n))
            if (*n as f32).is_finite() && *n > 0.0 =>
        {
            Ok(*n as f32)
        }
        (StyleId::MinZoom | StyleId::MaxZoom, StyleValue::Percent(p))
            if ((*p / 100.0) as f32).is_finite() && *p > 0.0 =>
        {
            Ok((*p / 100.0) as f32)
        }
        (StyleId::MinZoom | StyleId::MaxZoom, _) => wrong("a number or a percentage above 0"),
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axes_that_collapse_in_kernel_storage_are_refused() {
        for axis in ["1e-50 0 0", "0 -1e-50 0", "0 0 -1e-50"] {
            assert_eq!(rotate(&format!("{axis} 90deg")), None);
            assert_eq!(RotateAxis::parse(&format!("{axis} 90deg")), None);
            assert_eq!(RotateAxis::parse(axis), None);
        }
        for text in ["1e-20 0 0 90deg", "0 0 -1e-20 90deg", "1e-50 1 0 90deg"] {
            let (axis, angle) = rotate(text).unwrap();
            assert_ne!(axis, [0.; 3]);
            let axis = RotateAxis(axis);
            assert_eq!(RotateAxis::parse(&axis.css()), Some(axis));
            assert_eq!(
                rotate(&format!("{} {angle}deg", axis.css())),
                Some((axis.0, angle))
            );
        }
    }

    #[test]
    fn rotate_takes_css_forms_and_splits_into_axis_and_angle() {
        assert_eq!(rotate("45deg"), Some(([0.0, 0.0, 1.0], 45.0)));
        assert_eq!(rotate("x 0.25turn"), Some(([1.0, 0.0, 0.0], 90.0)));
        assert_eq!(rotate("30deg y"), Some(([0.0, 1.0, 0.0], 30.0)));
        assert_eq!(rotate("1 1 0 10deg"), Some(([1.0, 1.0, 0.0], 10.0)));
        assert_eq!(rotate("none"), Some(([0.0, 0.0, 1.0], 0.0)));
        assert_eq!(rotate("12"), Some(([0.0, 0.0, 1.0], 12.0)));
        assert_eq!(rotate("w 10deg"), None);
        assert_eq!(rotate("0 0 0 10deg"), None);
        assert_eq!(RotateAxis::parse("y 30deg").unwrap().css(), "y");
        assert!(RotateAxis::parse("y 30deg").unwrap().is_3d());
        // The wire form round-trips, including the authored sign.
        for text in ["y 30deg", "1 1 0 10deg", "x 1turn", "0 0 -1 30deg"] {
            let a = RotateAxis::parse(text).unwrap();
            assert_eq!(RotateAxis::parse(&a.css()), Some(a), "{text}");
        }
        assert_eq!(
            RotateAxis::parse("0 0 -1 30deg").unwrap().0,
            [0.0, 0.0, -1.0]
        );
        let mut s = crate::StyleProps::default();
        s.set_dynamic(
            crate::StyleId::Rotate,
            &crate::StyleValue::Text("0 0 -1 30deg".into()),
        )
        .unwrap();
        assert_eq!(s.rotate, 30.0);
        assert_eq!(translate_z("10px 20px 30px"), Some(30.0));
        assert_eq!(translate_z("10px"), Some(0.0));
        assert_eq!(perspective("none"), Some(0.0));
        assert_eq!(perspective("800px"), Some(800.0));
        assert_eq!(perspective("-1px"), None);
    }

    #[test]
    fn translate_takes_math_functions_of_px_and_a_percentage() {
        let v = |x, y| Vec2 { x, y };
        let t = |text| translate(text);
        assert_eq!(
            t("calc(50% - 12px) calc(-100% + 4px)"),
            Some((v(-12.0, 4.0), v(50.0, -100.0)))
        );
        assert_eq!(t("calc(10px * 2) 50%"), Some((v(20.0, 0.0), v(0.0, 50.0))));
        assert_eq!(
            t("calc((50% + 1in) / 2)"),
            Some((v(48.0, 0.0), v(25.0, 0.0)))
        );
        assert_eq!(t("max(4px, 2px)"), Some((v(4.0, 0.0), v(0.0, 0.0))));
        assert_eq!(
            t("calc(50%) 0 calc(2px + 3px)"),
            Some((v(0.0, 0.0), v(50.0, 0.0)))
        );
        assert_eq!(translate_z("calc(50%) 0 calc(2px + 3px)"), Some(5.0));
        // What two rows of px and a percentage cannot hold, and z's percentage.
        for refused in [
            "calc(1rem + 50%)",
            "calc(10vw)",
            "max(10px, 50%)",
            "env(safe-area-inset-top)",
            "calc(50% * 50%)",
            "0 0 calc(10% + 1px)",
            "0 0 0 0",
        ] {
            assert_eq!(t(refused), None, "{refused}");
        }
        let mut s = crate::StyleProps::default();
        s.set_dynamic(
            crate::StyleId::TranslatePercent,
            &crate::StyleValue::Text("calc(50% - 12px) 0".into()),
        )
        .unwrap();
        assert_eq!(s.translate_percent, v(50.0, 0.0));
    }
}
