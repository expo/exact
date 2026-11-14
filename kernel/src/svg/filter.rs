//! Filters (LLP 1055.000 D14): a `filter` reference or CSS filter
//! functions, resolved into a chain of primitives in the element's user
//! space. The chain and its wire form are `exact-svg-filter`'s, re-exported
//! here; the pixels are the hosts' and `exact-svg-raster`'s.
//!
//! @ref LLP 1055.000 D14; Filter Effects 1 §7, §12 (the CSS functions and
//! their equivalents)

pub use exact_svg_filter::{
    CompositeOp, Convolve, Filter, Input, Light, Lighting, Op, Primitive, Transfer, BLEND_MODES,
};

use crate::gradient::ColorText;

/// One entry of CSS `filter` on an SVG element (Filter Effects 1 §7,
/// §12): a reference to a `filter`, or a filter function.
#[derive(Debug, Clone, PartialEq)]
pub enum FilterFn {
    /// `url(#id)`.
    Url(Box<str>),
    /// `blur(<length>)`, px.
    Blur(f32),
    /// `brightness()`, as a number.
    Brightness(f32),
    /// `contrast()`.
    Contrast(f32),
    /// `drop-shadow([ <color>? && <length>{2,3} ])`: dx, dy, blur (px),
    /// colour (`None` for an omitted colour or `currentcolor`), which may
    /// be a reference (LLP 1095 D1).
    DropShadow(f32, f32, f32, Option<crate::style::ColorValue>),
    /// `grayscale()`.
    Grayscale(f32),
    /// `hue-rotate(<angle>)`, degrees.
    HueRotate(f32),
    /// `invert()`.
    Invert(f32),
    /// `opacity()`.
    Opacity(f32),
    /// `saturate()`.
    Saturate(f32),
    /// `sepia()`.
    Sepia(f32),
}

/// CSS `filter`: `none` (empty) or a list of [`FilterFn`]s, applied in
/// order.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct FilterList(pub Vec<FilterFn>);

/// A number or percentage argument; `None` when absent.
fn amount(arg: &str) -> Option<Option<f32>> {
    let t = arg.trim();
    if t.is_empty() {
        return Some(None);
    }
    let (n, k) = match t.strip_suffix('%') {
        Some(n) => (n, 0.01),
        None => (t, 1.0),
    };
    let v = crate::style::parse_css_number(n)? * k;
    (v >= 0.0 && (v as f32).is_finite()).then_some(Some(v as f32))
}

/// A length in px (a unitless zero admitted), its unit in any case.
fn px(t: &str) -> Option<f32> {
    crate::style::parse_pixel_length(t.trim())
}

/// An angle in degrees (`deg`, `rad`, `grad`, `turn`, in any case; a
/// unitless zero).
fn angle(t: &str) -> Option<f32> {
    let t = t.trim();
    for (unit, k) in [
        ("deg", 1.0),
        ("grad", 0.9),
        ("rad", 180.0 / std::f64::consts::PI),
        ("turn", 360.0),
    ] {
        let at = t.len().saturating_sub(unit.len());
        if t.get(at..).is_some_and(|u| u.eq_ignore_ascii_case(unit)) {
            let v = (crate::style::parse_css_number(&t[..at])? * k) as f32;
            return v.is_finite().then_some(v);
        }
    }
    (crate::style::parse_css_number(t)? == 0.0).then_some(0.0)
}

impl FilterList {
    /// CSS's grammar: `none`, or functions separated by white space. On the
    /// web, once linked ([`crate::style::link_effects`]).
    pub fn parse(css: &str) -> Option<FilterList> {
        crate::style::effects::parse(css, |e| e.filter)
    }

    /// [`Self::parse`]'s grammar.
    pub(crate) fn grammar(css: &str) -> Option<FilterList> {
        let t = css.trim();
        if t.eq_ignore_ascii_case("none") {
            return Some(FilterList::default());
        }
        let mut out = Vec::new();
        let mut rest = t;
        while !rest.is_empty() {
            let open = rest.find('(')?;
            // A function's name meets its parenthesis: `saturate (2)` is
            // not a function.
            let name = rest[..open].to_ascii_lowercase();
            // The matching parenthesis: a colour may hold its own.
            let mut depth = 0;
            let close = open
                + rest[open..].char_indices().find_map(|(i, c)| {
                    match c {
                        '(' => depth += 1,
                        ')' => depth -= 1,
                        _ => {}
                    }
                    (depth == 0).then_some(i)
                })?;
            let arg = &rest[open + 1..close];
            rest = rest[close + 1..].trim_start();
            let one = |d: f32| amount(arg).map(|a| a.unwrap_or(d));
            out.push(match name.as_str() {
                "url" => {
                    let id = arg.trim().trim_matches(|c| c == '"' || c == '\'');
                    FilterFn::Url(id.strip_prefix('#').filter(|i| !i.is_empty())?.into())
                }
                "blur" => FilterFn::Blur(if arg.trim().is_empty() {
                    0.0
                } else {
                    px(arg).filter(|v| *v >= 0.0)?
                }),
                "brightness" => FilterFn::Brightness(one(1.0)?),
                "contrast" => FilterFn::Contrast(one(1.0)?),
                "grayscale" => FilterFn::Grayscale(one(1.0)?.min(1.0)),
                "invert" => FilterFn::Invert(one(1.0)?.min(1.0)),
                "opacity" => FilterFn::Opacity(one(1.0)?.min(1.0)),
                "saturate" => FilterFn::Saturate(one(1.0)?),
                "sepia" => FilterFn::Sepia(one(1.0)?.min(1.0)),
                "hue-rotate" => FilterFn::HueRotate(if arg.trim().is_empty() {
                    0.0
                } else {
                    angle(arg)?
                }),
                "drop-shadow" => {
                    let mut lengths = Vec::new();
                    let mut color = None;
                    // The lengths are together: a colour after the first
                    // ends them (`<color>? && <length>{2,3}`).
                    let mut ended = false;
                    for word in split_words(arg) {
                        match px(word) {
                            Some(v) if lengths.len() < 3 && !ended => lengths.push(v),
                            _ if color.is_none() => {
                                ended = !lengths.is_empty();
                                color = Some(if word.eq_ignore_ascii_case("currentcolor") {
                                    None
                                } else {
                                    Some(crate::style::ColorValue::parse_light_dark(word).or_else(
                                        || crate::style::Color::parse(word).map(Into::into),
                                    )?)
                                })
                            }
                            _ => return None,
                        }
                    }
                    if lengths.len() < 2 || lengths.get(2).is_some_and(|b| *b < 0.0) {
                        return None;
                    }
                    FilterFn::DropShadow(
                        lengths[0],
                        lengths[1],
                        lengths.get(2).copied().unwrap_or(0.0),
                        color.flatten(),
                    )
                }
                _ => return None,
            });
        }
        Some(FilterList(out))
    }

    /// The value as CSS reads it.
    pub fn css(&self) -> String {
        self.text(ColorText::Css)
    }

    /// The wire form: [`Self::css`], with every reference kept (LLP 1095 D1).
    pub fn wire(&self) -> String {
        self.text(ColorText::Wire)
    }

    fn text(&self, mode: ColorText) -> String {
        if self.0.is_empty() {
            return "none".into();
        }
        let n = |v: f32| exact_num::Shortest(v as f64).to_string();
        self.0
            .iter()
            .map(|f| match f {
                FilterFn::Url(id) => format!("url(#{id})"),
                FilterFn::Blur(v) => format!("blur({}px)", n(*v)),
                FilterFn::Brightness(v) => format!("brightness({})", n(*v)),
                FilterFn::Contrast(v) => format!("contrast({})", n(*v)),
                FilterFn::Grayscale(v) => format!("grayscale({})", n(*v)),
                FilterFn::HueRotate(v) => format!("hue-rotate({}deg)", n(*v)),
                FilterFn::Invert(v) => format!("invert({})", n(*v)),
                FilterFn::Opacity(v) => format!("opacity({})", n(*v)),
                FilterFn::Saturate(v) => format!("saturate({})", n(*v)),
                FilterFn::Sepia(v) => format!("sepia({})", n(*v)),
                FilterFn::DropShadow(x, y, b, c) => {
                    let mut color = String::new();
                    if let Some(c) = c {
                        color.push(' ');
                        crate::gradient::color_text(&mut color, *c, mode);
                    }
                    format!("drop-shadow({}px {}px {}px{color})", n(*x), n(*y), n(*b))
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Every number is finite.
    pub fn is_finite(&self) -> bool {
        true
    }

    /// Whether the list is `none`.
    pub fn is_none(&self) -> bool {
        self.0.is_empty()
    }
}

/// Words separated by white space, a function's parentheses kept whole.
fn split_words(t: &str) -> Vec<&str> {
    let mut out = Vec::new();
    let (mut depth, mut start) = (0, None);
    for (i, c) in t.char_indices() {
        match c {
            '(' => depth += 1,
            ')' => depth -= 1,
            c if c.is_whitespace() && depth == 0 => {
                if let Some(s) = start.take() {
                    out.push(&t[s..i]);
                }
                continue;
            }
            _ => {}
        }
        if start.is_none() {
            start = Some(i);
        }
    }
    if let Some(s) = start {
        out.push(&t[s..]);
    }
    out
}
