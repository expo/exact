//! CSS replaced-object geometry and defaults, shared by graphical hosts.
use crate::{Dimension, Env, ObjectFit};
use cssparser::{ParseError, Parser, Token};
use std::borrow::Cow;

/// HTML's video UA default applies only when the author leaves `object-fit` unsaid.
/// An authored clear supplies CSS's initial `fill`, like any other written value.
/// https://html.spec.whatwg.org/multipage/rendering.html#embedded-content-rendering-rules
pub fn video_fit(value: ObjectFit, written: bool) -> ObjectFit {
    if written {
        value
    } else {
        ObjectFit::Contain
    }
}

/// CSS `object-position`: percentages resolve against the space left by the object.
#[derive(Debug, Clone, PartialEq)]
pub struct Position {
    /// Horizontal offset.
    pub x: Dimension,
    /// Vertical offset.
    pub y: Dimension,
    // A variable can replace keywords or both coordinates.
    source: Option<crate::style::calc::Comparison>,
}

impl Default for Position {
    fn default() -> Self {
        Self {
            x: Dimension::Percent(50.0),
            y: Dimension::Percent(50.0),
            source: None,
        }
    }
}

impl Position {
    /// CSS `<position>`: one or two coordinates, or edge-offset pairs.
    pub fn parse(css: &str) -> Option<Self> {
        let (text, deferred) = crate::style::calc::position_tokens(css, None)?;
        if deferred || text != css {
            if !deferred {
                Self::coordinates(&text)?;
            }
            return Some(Self {
                source: Some(crate::style::calc::Comparison::substitution(
                    css,
                    crate::StyleId::ObjectPosition,
                )),
                ..Self::default()
            });
        }
        Self::coordinates(&text)
    }

    fn coordinates(css: &str) -> Option<Self> {
        let words = position_components(css)?;
        let length = crate::style::calc::length_percentage;
        let part = |s: &str| match s.to_ascii_lowercase().as_str() {
            "left" | "top" => Some(Dimension::Percent(0.0)),
            "right" | "bottom" => Some(Dimension::Percent(100.0)),
            "center" => Some(Dimension::Percent(50.0)),
            _ => length(s),
        };
        let horizontal =
            |s: &str| s.eq_ignore_ascii_case("left") || s.eq_ignore_ascii_case("right");
        let vertical = |s: &str| s.eq_ignore_ascii_case("top") || s.eq_ignore_ascii_case("bottom");
        match words.as_slice() {
            [a] => {
                let v = part(a)?;
                Some(if vertical(a) {
                    Self {
                        y: v,
                        ..Self::default()
                    }
                } else {
                    Self {
                        x: v,
                        ..Self::default()
                    }
                })
            }
            [a, b] => {
                if vertical(a) || horizontal(b) {
                    if !(vertical(a) || a.eq_ignore_ascii_case("center"))
                        || !(horizontal(b) || b.eq_ignore_ascii_case("center"))
                    {
                        return None;
                    }
                    Some(Self {
                        x: part(b)?,
                        y: part(a)?,
                        source: None,
                    })
                } else {
                    Some(Self {
                        x: part(a)?,
                        y: part(b)?,
                        source: None,
                    })
                }
            }
            [a, first, b, second] => {
                let (x, dx, y, dy) = if horizontal(a) && vertical(b) {
                    (a, first, b, second)
                } else if vertical(a) && horizontal(b) {
                    (b, second, a, first)
                } else {
                    return None;
                };
                let offset = |edge: &str, value: &str| {
                    let value = length(value)?;
                    if edge.eq_ignore_ascii_case("right") || edge.eq_ignore_ascii_case("bottom") {
                        Dimension::parse_calc(&format!("calc(100% - {})", dimension_css(&value)))
                    } else {
                        Some(value)
                    }
                };
                Some(Self {
                    x: offset(x, dx)?,
                    y: offset(y, dy)?,
                    source: None,
                })
            }
            _ => None,
        }
    }

    /// Resolve both coordinates as one property; an invalid coordinate resets it.
    pub fn resolve(&self, env: &Env) -> Self {
        self.try_resolve(env).unwrap_or_default()
    }

    /// Resolve the whole property, retaining invalidity for CSS projections.
    pub fn try_resolve(&self, env: &Env) -> Option<Self> {
        if let Some(source) = &self.source {
            let mut css = String::new();
            source.css(&mut css);
            return crate::style::calc::position_tokens(&css, Some(env))
                .and_then(|(text, _)| Self::coordinates(&text))
                .and_then(|p| p.try_resolve(env));
        }
        let x = self.x.resolve(env);
        let y = self.y.resolve(env);
        if matches!(x, Dimension::Auto) || matches!(y, Dimension::Auto) {
            None
        } else {
            Some(Self { x, y, source: None })
        }
    }

    /// Canonical CSS.
    pub fn css(&self) -> String {
        if let Some(source) = &self.source {
            let mut css = String::new();
            source.css(&mut css);
            css
        } else {
            format!("{} {}", dimension_css(&self.x), dimension_css(&self.y))
        }
    }

    pub(crate) fn calculations(&self) -> [Option<crate::style::calc::Comparison>; 2] {
        if let Some(source) = &self.source {
            return [Some(source.clone()), None];
        }
        [&self.x, &self.y].map(|d| match d {
            Dimension::Compare(c) => Some(c.clone()),
            _ => None,
        })
    }

    /// Whether each coordinate is finite.
    pub fn is_finite(&self) -> bool {
        self.x.is_finite() && self.y.is_finite()
    }
}

fn position_components(css: &str) -> Option<Vec<Cow<'_, str>>> {
    let mut parser = Parser::new(css);
    let mut values = Vec::new();
    while !parser.is_exhausted() {
        parser.skip_whitespace();
        let start = parser.position();
        let value = match parser.next().ok()? {
            // The source text, not the unescaped name: `\37px` is an identifier,
            // and reparsing `7px` would read it as a length.
            Token::Ident(_) => Cow::Borrowed(parser.slice_from(start)),
            Token::Function(_) => {
                parser
                    .parse_nested_block(|p| {
                        while p.next().is_ok() {}
                        Ok::<_, ParseError<()>>(())
                    })
                    .ok()?;
                Cow::Borrowed(parser.slice_from(start))
            }
            Token::Dimension { .. } | Token::Percentage { .. } | Token::Number { .. } => {
                Cow::Borrowed(parser.slice_from(start))
            }
            _ => return None,
        };
        values.push(value);
    }
    Some(values)
}

/// A valid SVG viewBox, or the portable roles' square when it is invalid.
pub fn view_box(text: &str) -> [f32; 4] {
    let mut words = text
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .filter(|s| !s.is_empty());
    let parsed = (|| {
        Some([
            words.next()?.parse().ok()?,
            words.next()?.parse().ok()?,
            words.next()?.parse().ok()?,
            words.next()?.parse().ok()?,
        ])
    })();
    match parsed {
        Some(v)
            if words.next().is_none()
                && v.iter().all(|n: &f32| n.is_finite())
                && v[2] > 0.0
                && v[3] > 0.0 =>
        {
            v
        }
        _ => [0.0, 0.0, 24.0, 24.0],
    }
}

/// An icon's natural box: its viewBox's aspect with its longer side at one em.
pub fn symbol_size(view_box: &str, em: f32) -> (f32, f32) {
    let [_, _, w, h] = self::view_box(view_box);
    (em * (w / w.max(h)), em * (h / w.max(h)))
}

/// CSS Images 3 concrete object size and position in a content box.
pub fn placed(
    natural: (f32, f32),
    fit: ObjectFit,
    position: &Position,
    content: (f32, f32, f32, f32),
    env: &Env,
) -> Option<(f32, f32, f32, f32)> {
    let position = position.resolve(env);
    let (nw, nh) = natural;
    let (x, y, w, h) = content;
    if nw <= 0.0 || nh <= 0.0 || w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = match fit {
        ObjectFit::Contain => Some((w / nw).min(h / nh)),
        ObjectFit::Cover => Some((w / nw).max(h / nh)),
        ObjectFit::None => Some(1.0),
        ObjectFit::ScaleDown => Some((w / nw).min(h / nh).min(1.0)),
        ObjectFit::Fill => None,
    };
    let (dw, dh) = scale.map_or((w, h), |s| (nw * s, nh * s));
    Some((
        x + position.x.at_basis(env, w - dw),
        y + position.y.at_basis(env, h - dh),
        dw,
        dh,
    ))
}

fn dimension_css(d: &Dimension) -> String {
    let mut out = String::new();
    crate::style::calc::length_css(d, &mut out);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn video_uses_its_ua_fit_only_without_a_declaration() {
        for fit in ObjectFit::ALL {
            assert_eq!(video_fit(fit, true), fit);
        }
        assert_eq!(video_fit(ObjectFit::Fill, false), ObjectFit::Contain);
    }

    #[test]
    fn position_substitutes_variables_before_reading_coordinates_and_uses_the_nodes_font() {
        let mut env = Env {
            properties: Some(Default::default()),
            viewport_width: 840.,
            ..Env::default()
        };
        for font in [20., 30.] {
            env.font_size = font;
            let mut failures = Vec::new();
            for (css, expected) in [
                ("1em 0", (font, 0.)),
                ("calc(1em + 2px) 0", (font + 2., 0.)),
                ("var(--missing, right) bottom", (140., 75.)),
                ("var(--missing, 7px 11px)", (7., 11.)),
                ("var(--missing, var(--other, 1em 0))", (font, 0.)),
                ("var(--missing) 75%", (70., 37.5)),
                ("var(--missing, bogus) 75%", (70., 37.5)),
                ("var(--absent, left top 11px)", (70., 37.5)),
            ] {
                let got = Position::parse(css)
                    .and_then(|p| placed((20., 5.), ObjectFit::None, &p, (0., 0., 160., 80.), &env))
                    .map(|r| (r.0, r.1));
                if got != Some(expected) {
                    failures.push(format!("{css}: {got:?} != {expected:?}"));
                }
            }
            assert!(failures.is_empty(), "{}", failures.join("\n"));
        }
    }

    #[test]
    fn template_objects_place_their_viewport_in_every_fit() {
        let env = Env::default();
        let p = Position::default();
        for (text, expected) in [
            (
                "0 0 799.1 184",
                [
                    (0., 21.579277, 160., 36.841446),
                    (-93.71739, 0., 347.43478, 80.),
                    (70., 37.69741, 20., 4.605181),
                ],
            ),
            (
                "0 0 184 799.1",
                [
                    (70.78964, 0., 18.420723, 80.),
                    (0., -307.43478, 160., 694.86957),
                    (77.69741, 30., 4.605181, 20.),
                ],
            ),
        ] {
            for (fit, want) in [
                (ObjectFit::Contain, expected[0]),
                (ObjectFit::Fill, (0., 0., 160., 80.)),
                (ObjectFit::Cover, expected[1]),
                (ObjectFit::None, expected[2]),
                (ObjectFit::ScaleDown, expected[2]),
            ] {
                let got =
                    placed(symbol_size(text, 20.), fit, &p, (0., 0., 160., 80.), &env).unwrap();
                for (a, b) in [got.0, got.1, got.2, got.3]
                    .into_iter()
                    .zip([want.0, want.1, want.2, want.3])
                {
                    assert!((a - b).abs() < 0.001, "{text} {fit:?}: {got:?} != {want:?}");
                }
            }
        }
        assert_eq!(symbol_size("0 0 24 24", 20.), (20., 20.));
        for invalid in ["0 0 -1 24", "0 0 NaN 24", "0 0 inf 24", "0 bad 0 24 24", ""] {
            assert_eq!(symbol_size(invalid, 20.), (20., 20.));
        }
    }

    #[test]
    fn position_substitutes_environment_values_before_reading_coordinates() {
        let env = Env {
            properties: Some(Default::default()),
            ..Env::default().with_insets(0., 0., 0., 13.)
        };
        let pos = |css| {
            let spec = Position::parse(css).unwrap_or_else(|| panic!("position refused: {css}"));
            let rect = placed((20., 5.), ObjectFit::None, &spec, (0., 0., 160., 80.), &env)
                .expect("non-empty object");
            (rect.0, rect.1)
        };
        assert_eq!(pos("env(safe-area-inset-left, 0px) top"), (13., 0.));
        assert_eq!(
            pos("env(titlebar-area-x, right 7px bottom 11px)"),
            (133., 64.)
        );
        assert_eq!(
            pos("var(--absent, env(titlebar-area-x, 7px 11px))"),
            (7., 11.)
        );
        assert_eq!(pos("env(titlebar-area-x, invalid) top"), (70., 37.5));
        // An escaped identifier stays an identifier: `\37px` is not the length `7px`.
        assert_eq!(pos(r"var(--missing, \37px) top"), (70., 37.5));
    }

    #[test]
    fn object_positions_resolve_against_the_remaining_space() {
        let env = Env::default();
        for (css, expected) in [
            ("left bottom", (0., 60.)),
            ("25% 75%", (35., 45.)),
            ("7px 11px", (7., 11.)),
            ("right 7px bottom 11px", (133., 49.)),
            ("top 11px left 7px", (7., 11.)),
            ("calc(50% + 3px) 25%", (73., 15.)),
            ("calc(10px)calc(12px)", (10., 12.)),
        ] {
            let position = Position::parse(css).unwrap();
            let rect = placed(
                (20., 20.),
                ObjectFit::None,
                &position,
                (0., 0., 160., 80.),
                &env,
            )
            .unwrap();
            assert_eq!((rect.0, rect.1), expected, "{css}");
            assert_eq!(Position::parse(&position.css()), Some(position));
        }
        for bad in [
            "top bottom",
            "left right",
            "10px left",
            "top 10px",
            "4",
            "left 0 0",
            "left top 11px",
            "center bottom 11px",
            "right 7px center",
        ] {
            assert!(Position::parse(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn object_positions_accept_css_relative_lengths() {
        let env = Env {
            viewport_width: 400.,
            viewport_height: 800.,
            root_font_size: 18.,
            ..Env::default()
        };
        for (css, expected) in [
            ("10vw 0", (40., 0.)),
            ("1rem 0", (18., 0.)),
            ("right 10vw bottom 1rem", (100., 42.)),
        ] {
            let position = Position::parse(css).expect(css);
            let rect = placed(
                (20., 20.),
                ObjectFit::None,
                &position,
                (0., 0., 160., 80.),
                &env,
            )
            .unwrap();
            assert_eq!((rect.0, rect.1), expected, "{css}");
            assert_eq!(Position::parse(&position.css()), Some(position));
        }
    }
}
