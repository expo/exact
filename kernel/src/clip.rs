//! CSS `clip-path`: basic shapes relative to the border box, `none`,
//! `path([<fill-rule>,]? <string>)` in CSS pixels, or an SVG `clipPath` reference,
//! `url(#id)` (LLP 1055.000 D10). The string is SVG path data in full, read
//! by the SVG path parser (LLP 1055.000 addendum A); the parsed commands
//! (absolute `M L C Z`) cross the Apple batch, so presenters parse no CSS.

mod basic;
use crate::generated::FillRule;
use crate::svg::{parse_d_whole, Paint, PaintFallback, Seg};
use basic::BasicShape;

/// A validated clipping path. The initial value has no clipping.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct ClipPath {
    commands: Vec<(char, Vec<f32>)>,
    rule: FillRule,
    url: Option<Box<str>>,
    shape: Option<Box<BasicShape>>,
}

impl ClipPath {
    /// Basic shapes in px or %, `none`, `url(#id)`, or `path()`: a fill rule, then
    /// SVG path data in a quoted string. Data with any error is refused
    /// whole, as CSS refuses the declaration; so is data that draws nothing.
    /// On the web, once linked ([`crate::style::link_effects`]).
    pub fn parse(css: &str) -> Option<Self> {
        crate::style::effects::parse(css, |e| e.clip_path)
    }

    /// [`Self::parse`]'s grammar.
    pub(crate) fn grammar(css: &str) -> Option<Self> {
        let css = css.trim();
        if css == "none" {
            return Some(Self::default());
        }
        if let Some(Paint::Url(id, PaintFallback::Default)) = Paint::parse(css) {
            return Some(Self {
                url: Some(id),
                ..Self::default()
            });
        }
        if let Some((shape, rule)) = BasicShape::parse(css) {
            return Some(Self {
                shape: Some(Box::new(shape)),
                rule,
                ..Self::default()
            });
        }
        let inner = css.strip_prefix("path(")?.strip_suffix(')')?.trim();
        let (rule, inner) = match inner.split_once(',') {
            Some((rule, rest)) if !rule.trim_start().starts_with(['"', '\'']) => {
                (FillRule::from_name(rule.trim())?, rest.trim())
            }
            _ => (FillRule::Nonzero, inner),
        };
        let quote = inner.chars().next()?;
        if quote != '\'' && quote != '"' {
            return None;
        }
        let data = inner.strip_prefix(quote)?.strip_suffix(quote)?;
        if data.contains(quote) {
            return None;
        }
        let commands = parse_d_whole(data)?
            .0
            .into_iter()
            .map(|seg| match seg {
                Seg::Move(x, y) => ('M', vec![x, y]),
                Seg::Line(x, y) => ('L', vec![x, y]),
                Seg::Cubic(a, b, c, d, x, y) => ('C', vec![a, b, c, d, x, y]),
                Seg::Close => ('Z', Vec::new()),
            })
            .collect();
        Some(Self {
            commands,
            rule,
            url: None,
            shape: None,
        })
    }

    /// Which points are inside: CSS's `nonzero` unless the value said
    /// `evenodd`.
    pub fn rule(&self) -> FillRule {
        self.rule
    }

    /// The `clipPath` a `url(#id)` names, if this is one.
    pub fn url(&self) -> Option<&str> {
        self.url.as_deref()
    }

    /// Pixel path commands. For a basic shape, call `resolved` at the box size first.
    pub fn commands(&self) -> &[(char, Vec<f32>)] {
        &self.commands
    }

    /// A path or basic shape, including one whose resolved area is empty.
    pub fn has_path(&self) -> bool {
        self.shape.is_some() || !self.commands.is_empty()
    }

    /// The resolved pixel outline, also used for native SVG clips and pointer hits.
    pub fn path(&self) -> crate::svg::Path {
        crate::svg::Path(
            self.commands
                .iter()
                .map(|(op, v)| match op {
                    'M' => Seg::Move(v[0], v[1]),
                    'L' => Seg::Line(v[0], v[1]),
                    'C' => Seg::Cubic(v[0], v[1], v[2], v[3], v[4], v[5]),
                    'Z' => Seg::Close,
                    _ => unreachable!("canonical clip path"),
                })
                .collect(),
        )
    }

    /// The numeric host payload's length, without allocating it.
    pub fn shape_value_count(&self) -> usize {
        self.shape.as_ref().map_or(0, |s| s.value_count())
    }

    /// A basic shape's typed host payload: name and length pairs [px, percent].
    /// Polygon: x/y pairs; inset: edges, horizontal radii, vertical radii;
    /// circle/ellipse: center x/y, radii x/y. A radius [-1, 0] is closest-side,
    /// [-2, 0] farthest-side. All other radii are nonnegative.
    pub fn shape_data(&self) -> Option<(&'static str, Vec<f32>)> {
        self.shape.as_ref().map(|s| s.data())
    }

    /// Resolve a basic shape at the current border-box size; pixel paths borrow.
    pub fn resolved(&self, width: f32, height: f32) -> std::borrow::Cow<'_, Self> {
        let Some(shape) = &self.shape else {
            return std::borrow::Cow::Borrowed(self);
        };
        let commands = shape
            .path(width, height)
            .0
            .into_iter()
            .map(|seg| match seg {
                Seg::Move(x, y) => ('M', vec![x, y]),
                Seg::Line(x, y) => ('L', vec![x, y]),
                Seg::Cubic(a, b, c, d, x, y) => ('C', vec![a, b, c, d, x, y]),
                Seg::Close => ('Z', Vec::new()),
            })
            .collect();
        std::borrow::Cow::Owned(Self {
            commands,
            rule: self.rule,
            ..Self::default()
        })
    }

    /// Canonical, validated CSS (also the wire representation).
    pub fn css(&self) -> String {
        if let Some(shape) = &self.shape {
            return shape.css(self.rule);
        }
        if let Some(id) = &self.url {
            return format!("url(#{id})");
        }
        if self.commands.is_empty() {
            return "none".into();
        }
        use std::fmt::Write as _;
        let mut text = String::from("path(");
        if self.rule == FillRule::Evenodd {
            text.push_str("evenodd, ");
        }
        text.push('"');
        for (i, (command, values)) in self.commands.iter().enumerate() {
            if i > 0 {
                text.push(' ');
            }
            text.push(*command);
            for value in values {
                let _ = write!(text, " {}", exact_num::Shortest32(*value));
            }
        }
        text.push_str("\")");
        text
    }
}

#[cfg(test)]
mod tests {
    use super::ClipPath;
    use crate::{
        wire::codec::{Reader, Writer},
        DecodeError, StyleId, StyleMask, StyleProps, StyleValue,
    };

    #[test]
    fn generated_patch_codec_preserves_curves_and_refuses_bad_wire_data() {
        let mut style = StyleProps::default();
        style
            .set_dynamic(
                StyleId::ClipPath,
                &StyleValue::Text("path('M 0 0 C 1 2 3 4 5 6 Z')".into()),
            )
            .unwrap();
        let mut bytes = Writer::new();
        style.encode_patch(&mut bytes);
        assert_eq!(
            StyleProps::decode_patch(&mut Reader::new(bytes.as_slice())).unwrap(),
            style
        );
        assert!(!StyleId::ClipPath.affects_layout());
        let mut bad = Writer::new();
        bad.style_mask(StyleMask::of(StyleId::ClipPath));
        bad.style_mask(StyleMask::EMPTY);
        bad.string("path('M 0 NaN')");
        assert_eq!(
            StyleProps::decode_patch(&mut Reader::new(bad.as_slice())),
            Err(DecodeError::BadClipPath)
        );
    }

    #[test]
    fn curves_round_trip_and_invalid_paths_are_refused() {
        // Quadratics (and arcs) arrive as cubics: presenters draw four verbs.
        let path = ClipPath::parse("path('M0,0 C0,10 5,18 20,18 Q14,15 14,0 Z')").unwrap();
        assert_eq!(
            path.css(),
            "path(\"M 0 0 C 0 10 5 18 20 18 C 16 16 14 10 14 0 Z\")"
        );
        assert_eq!(ClipPath::parse(&path.css()), Some(path));
        assert_eq!(ClipPath::default().css(), "none");
        // SVG path data in full, relative and implicit commands included, and
        // a fill rule before it.
        let full = ClipPath::parse("path(evenodd, 'm10 10 h20 v20 h-20 z M0 0 l5 5')").unwrap();
        assert_eq!(full.rule(), crate::generated::FillRule::Evenodd);
        assert_eq!(
            full.css(),
            "path(evenodd, \"M 10 10 L 30 10 L 30 30 L 10 30 Z M 0 0 L 5 5\")"
        );
        assert_eq!(ClipPath::parse(&full.css()), Some(full));
        for data in [
            "path('M12 2a10 10 0 1 1 0 20a10 10 0 1 1 0-20Z')",
            "path('m2 2h20v20h-20z')",
        ] {
            let path = ClipPath::parse(data).unwrap();
            assert_eq!(ClipPath::parse(&path.css()), Some(path));
        }
        for bad in [
            "path('')",
            "path('M0 0 Z 1')",
            "path('M0 0 z 1')",
            "path('L 0 0')",
            "path('M 0 NaN')",
            "path('M 0 0 C 1 2')",
            "path('M 0 0 L 1 1,')",
            "path(winding, 'M 0 0 L 1 1')",
            "path('M 0 0');color:red",
        ] {
            assert!(ClipPath::parse(bad).is_none(), "{bad}");
        }
    }
}

#[cfg(test)]
mod basic_tests {
    use super::*;
    #[test]
    fn basic_shapes_keep_percentages_through_css_and_wire() {
        for css in [
            "polygon(0% 2%, 50% 0%, 100% 3%, 100% 97%, 50% 100%, 0% 98%)",
            "polygon(evenodd, 0 0, 100% 0, 0 100%)",
            "inset(4px round 8px)",
            "inset(10% 20% 30% 5px round 8px 20% 3px / 10% 5px)",
            "circle()",
            "circle(30% at right bottom)",
            "ellipse(closest-side farthest-side at 25% 60%)",
        ] {
            let path = ClipPath::parse(css).unwrap_or_else(|| panic!("{css}"));
            assert_eq!(ClipPath::parse(&path.css()), Some(path.clone()));
            let mut style = crate::StyleProps::default();
            style
                .set_dynamic(
                    crate::StyleId::ClipPath,
                    &crate::StyleValue::Text(css.into()),
                )
                .unwrap();
            let mut wire = crate::wire::codec::Writer::new();
            style.encode_patch(&mut wire);
            assert_eq!(
                crate::StyleProps::decode_patch(&mut crate::wire::codec::Reader::new(
                    wire.as_slice()
                ))
                .unwrap(),
                style
            );
            assert!(path
                .resolved(100., 80.)
                .commands()
                .iter()
                .flat_map(|(_, v)| v)
                .all(|v| v.is_finite()));
        }
    }
    #[test]
    fn basic_shapes_refuse_malformed_or_unsupported_values() {
        for css in [
            "polygon()",
            "polygon(0 0,)",
            "polygon(1 0,0 0)",
            "polygon(winding,0 0)",
            "polygon(NaN% 0)",
            "inset()",
            "inset(0 round)",
            "inset(0 round -1px)",
            "inset(0 round 1px / 2px / 3px)",
            "circle(-10%)",
            "ellipse(10%)",
            "circle(10% at)",
            "circle(10% at left right)",
            "polygon(0 0) padding-box",
            "circle(1em)",
            "circle(inf%)",
        ] {
            assert!(ClipPath::parse(css).is_none(), "{css}");
        }
    }
    #[test]
    fn finite_large_radii_reduce_without_overflow() {
        let large = ClipPath::parse("inset(0 round 2e38px)").unwrap();
        let reduced = ClipPath::parse("inset(0 round 50px)").unwrap();
        assert_eq!(large.resolved(100., 100.), reduced.resolved(100., 100.));
    }
    #[test]
    fn polygon_resolves_each_axis_at_each_size() {
        let shape = ClipPath::parse("polygon(0% 20%, 100% 0%, 25% 100%)").unwrap();
        for (w, h) in [(100., 80.), (240., 160.)] {
            assert_eq!(
                shape.resolved(w, h).commands(),
                &[
                    ('M', vec![0., h * 0.2]),
                    ('L', vec![w, 0.]),
                    ('L', vec![w * 0.25, h]),
                    ('Z', vec![]),
                ]
            );
        }
    }
    #[test]
    fn empty_shapes_still_clip_and_circles_use_the_normalized_diagonal() {
        for css in ["inset(75% 0 50%)", "circle(0)", "ellipse(0 10px)"] {
            assert!(ClipPath::parse(css).unwrap().resolved(100., 80.).has_path());
        }
        let circle = ClipPath::parse("circle(50% at 0 0)").unwrap();
        let resolved = circle.resolved(60., 80.);
        let radius = 50. / std::f32::consts::SQRT_2;
        assert!((resolved.commands()[0].1[0] - radius).abs() < 0.001);
    }
}
