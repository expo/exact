//! CSS Shapes 1 §3.1: lengths stay relative until the reference box is known.
use crate::svg::{Path, Seg};
use crate::FillRule;

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Length {
    value: f32,
    percent: bool,
}
impl Length {
    const ZERO: Self = Self {
        value: 0.,
        percent: false,
    };
    fn pct(value: f32) -> Self {
        Self {
            value,
            percent: true,
        }
    }
    fn parse(s: &str) -> Option<Self> {
        let mut input = cssparser::ParserInput::new(s);
        let mut parser = cssparser::Parser::new(&mut input);
        match parser.next().ok()? {
            cssparser::Token::Number { value, .. } if *value == 0. => {}
            cssparser::Token::Dimension { unit, .. } if unit.eq_ignore_ascii_case("px") => {}
            cssparser::Token::Percentage { .. } => {}
            _ => return None,
        }
        parser.expect_exhausted().ok()?;
        let (n, percent, unit) = if let Some(n) = s.strip_suffix('%') {
            (n, true, true)
        } else if let Some(n) = s.strip_suffix("px") {
            (n, false, true)
        } else {
            (s, false, false)
        };
        if n.is_empty()
            || !n
                .bytes()
                .all(|b| b.is_ascii_digit() || b"+-.eE".contains(&b))
        {
            return None;
        }
        let value = exact_num::parse_f32(n).ok()?;
        (value.is_finite() && (unit || value == 0.)).then_some(Self { value, percent })
    }
    fn resolve(self, basis: f32) -> f32 {
        finite(if self.percent {
            self.value as f64 * basis as f64 / 100.
        } else {
            self.value as f64
        })
    }
    fn css(self) -> String {
        format!(
            "{}{}",
            exact_num::Shortest32(self.value),
            if self.percent { "%" } else { "px" }
        )
    }
    fn data(self) -> [f32; 2] {
        if self.percent {
            [0., self.value]
        } else {
            [self.value, 0.]
        }
    }
}
fn finite(v: f64) -> f32 {
    v.clamp(-(f32::MAX as f64), f32::MAX as f64) as f32
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) enum Radius {
    Length(Length),
    Closest,
    Farthest,
}
impl Radius {
    fn parse(s: &str) -> Option<Self> {
        match s {
            "closest-side" => Some(Self::Closest),
            "farthest-side" => Some(Self::Farthest),
            _ => Length::parse(s).filter(|v| v.value >= 0.).map(Self::Length),
        }
    }
    fn css(self) -> String {
        match self {
            Self::Closest => "closest-side".into(),
            Self::Farthest => "farthest-side".into(),
            Self::Length(v) => v.css(),
        }
    }
    fn resolve(self, center: f32, extent: f32) -> f32 {
        let a = center.abs();
        let b = finite(extent as f64 - center as f64).abs();
        match self {
            Self::Closest => a.min(b),
            Self::Farthest => a.max(b),
            Self::Length(v) => v.resolve(extent),
        }
    }
    fn data(self) -> [f32; 2] {
        match self {
            Self::Closest => [-1., 0.],
            Self::Farthest => [-2., 0.],
            Self::Length(v) => v.data(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) enum BasicShape {
    Polygon(Vec<[Length; 2]>),
    Inset {
        edges: [Length; 4],
        x: [Length; 4],
        y: [Length; 4],
    },
    Oval {
        center: [Length; 2],
        radii: [Radius; 2],
        circle: bool,
    },
}

fn four(s: &str, positive: bool) -> Option<[Length; 4]> {
    let v: Vec<_> = s
        .split_ascii_whitespace()
        .map(Length::parse)
        .collect::<Option<_>>()?;
    if positive && v.iter().any(|n| n.value < 0.) {
        return None;
    }
    Some(match v.as_slice() {
        [a] => [*a; 4],
        [a, b] => [*a, *b, *a, *b],
        [a, b, c] => [*a, *b, *c, *b],
        [a, b, c, d] => [*a, *b, *c, *d],
        _ => return None,
    })
}
fn position(tokens: &[&str]) -> Option<[Length; 2]> {
    let axis = |s: &str, low: &str, high: &str| match s {
        "center" => Some(Length::pct(50.)),
        s if s == low => Some(Length::pct(0.)),
        s if s == high => Some(Length::pct(100.)),
        s => Length::parse(s),
    };
    match tokens {
        [] => None,
        ["top" | "bottom"] => Some([Length::pct(50.), axis(tokens[0], "top", "bottom")?]),
        [x] => Some([axis(x, "left", "right")?, Length::pct(50.)]),
        [x, y] => {
            if matches!(*x, "top" | "bottom") || matches!(*y, "left" | "right") {
                if !matches!(*x, "top" | "bottom" | "center")
                    || !matches!(*y, "left" | "right" | "center")
                {
                    return None;
                }
                Some([axis(y, "left", "right")?, axis(x, "top", "bottom")?])
            } else {
                Some([axis(x, "left", "right")?, axis(y, "top", "bottom")?])
            }
        }
        _ => None,
    }
}

impl BasicShape {
    pub(super) fn parse(css: &str) -> Option<(Self, FillRule)> {
        let lower = css.to_ascii_lowercase();
        let (name, body) = lower.split_once('(')?;
        let body = body.strip_suffix(')')?.trim();
        if body.contains(['(', ')']) {
            return None;
        }
        let mut rule = FillRule::Nonzero;
        let shape = match name {
            "polygon" => {
                let mut parts = body.split(',').peekable();
                if let Some(r) = parts.peek().and_then(|p| FillRule::from_name(p.trim())) {
                    rule = r;
                    parts.next();
                }
                let points = parts
                    .map(|part| {
                        let v: Vec<_> = part.split_ascii_whitespace().collect();
                        match v.as_slice() {
                            [x, y] => Some([Length::parse(x)?, Length::parse(y)?]),
                            _ => None,
                        }
                    })
                    .collect::<Option<Vec<_>>>()?;
                if points.is_empty() {
                    return None;
                }
                Self::Polygon(points)
            }
            "inset" => {
                if let Some((before, after)) = body.split_once("round") {
                    if !before.ends_with(char::is_whitespace)
                        || !after.starts_with(char::is_whitespace)
                    {
                        return None;
                    }
                }
                let (edges, round) = body
                    .split_once("round")
                    .map_or((body, None), |(e, r)| (e, Some(r)));
                let edges = four(edges, false)?;
                let (x, y) = if let Some(r) = round {
                    let (x, y) = r.split_once('/').unwrap_or((r, r));
                    (four(x, true)?, four(y, true)?)
                } else {
                    ([Length::ZERO; 4], [Length::ZERO; 4])
                };
                Self::Inset { edges, x, y }
            }
            "circle" | "ellipse" => {
                let tokens: Vec<_> = body.split_ascii_whitespace().collect();
                let at = tokens
                    .iter()
                    .position(|s| *s == "at")
                    .unwrap_or(tokens.len());
                let center = if at == tokens.len() {
                    [Length::pct(50.); 2]
                } else {
                    position(&tokens[at + 1..])?
                };
                let circle = name == "circle";
                let radii = match &tokens[..at] {
                    [] => [Radius::Closest; 2],
                    [r] if circle => [Radius::parse(r)?; 2],
                    [x, y] if !circle => [Radius::parse(x)?, Radius::parse(y)?],
                    _ => return None,
                };
                Self::Oval {
                    center,
                    radii,
                    circle,
                }
            }
            _ => return None,
        };
        Some((shape, rule))
    }
    pub(super) fn css(&self, rule: FillRule) -> String {
        let list = |v: &[Length]| v.iter().map(|v| v.css()).collect::<Vec<_>>().join(" ");
        match self {
            Self::Polygon(points) => format!(
                "polygon({}{})",
                if rule == FillRule::Evenodd {
                    "evenodd, "
                } else {
                    ""
                },
                points
                    .iter()
                    .map(|p| list(p))
                    .collect::<Vec<_>>()
                    .join(", ")
            ),
            Self::Inset { edges, x, y } => {
                format!("inset({} round {} / {})", list(edges), list(x), list(y))
            }
            Self::Oval {
                center,
                radii,
                circle,
            } => {
                if *circle {
                    format!("circle({} at {})", radii[0].css(), list(center))
                } else {
                    format!(
                        "ellipse({} {} at {})",
                        radii[0].css(),
                        radii[1].css(),
                        list(center)
                    )
                }
            }
        }
    }
    // Apple receives numbers, never a second CSS grammar. Lengths are [px, percent];
    // radius [-1, 0] means closest-side, [-2, 0] farthest-side.
    pub(super) fn value_count(&self) -> usize {
        match self {
            Self::Polygon(p) => p.len() * 4,
            Self::Inset { .. } => 24,
            Self::Oval { .. } => 8,
        }
    }
    pub(super) fn data(&self) -> (&'static str, Vec<f32>) {
        match self {
            Self::Polygon(points) => (
                "polygon",
                points.iter().flatten().flat_map(|n| n.data()).collect(),
            ),
            Self::Inset { edges, x, y } => (
                "inset",
                edges
                    .iter()
                    .chain(x)
                    .chain(y)
                    .flat_map(|n| n.data())
                    .collect(),
            ),
            Self::Oval {
                center,
                radii,
                circle,
            } => (
                if *circle { "circle" } else { "ellipse" },
                center
                    .iter()
                    .flat_map(|n| n.data())
                    .chain(radii.iter().flat_map(|r| r.data()))
                    .collect(),
            ),
        }
    }
    pub(super) fn path(&self, w: f32, h: f32) -> Path {
        match self {
            Self::Polygon(points) => Path::polyline(
                &points
                    .iter()
                    .map(|p| (p[0].resolve(w), p[1].resolve(h)))
                    .collect::<Vec<_>>(),
                true,
            ),
            Self::Inset { edges, x, y } => {
                let [mut t, mut r, mut b, mut l] =
                    std::array::from_fn(|i| edges[i].resolve(if i % 2 == 0 { h } else { w }));
                reduce(&mut t, &mut b, h);
                reduce(&mut l, &mut r, w);
                let iw = (w - l - r).max(0.);
                let ih = (h - t - b).max(0.);
                // Percentage radii use the reference box, not the inset rectangle.
                let mut radii = std::array::from_fn(|i| {
                    (f64::from(x[i].resolve(w)), f64::from(y[i].resolve(h)))
                });
                let sums = [
                    radii[0].0 + radii[1].0,
                    radii[3].0 + radii[2].0,
                    radii[0].1 + radii[3].1,
                    radii[1].1 + radii[2].1,
                ];
                let mut factor = 1f64;
                for (extent, sum) in [iw, iw, ih, ih].into_iter().zip(sums) {
                    if sum > 0. {
                        factor = factor.min(f64::from(extent) / sum);
                    }
                }
                for (x, y) in &mut radii {
                    *x *= factor;
                    *y *= factor;
                }
                rounded(l, t, iw, ih, radii.map(|(x, y)| (x as f32, y as f32)))
            }
            Self::Oval {
                center,
                radii,
                circle,
            } => {
                let cx = center[0].resolve(w);
                let cy = center[1].resolve(h);
                let mut rx = radii[0].resolve(cx, w);
                let mut ry = radii[1].resolve(cy, h);
                if *circle {
                    rx = match radii[0] {
                        Radius::Length(n) => n.resolve(finite(
                            (w as f64).hypot(h as f64) / std::f64::consts::SQRT_2,
                        )),
                        Radius::Closest => rx.min(ry),
                        Radius::Farthest => rx.max(ry),
                    };
                    ry = rx;
                }
                crate::svg::ellipse(cx, cy, Some(rx), Some(ry)).unwrap_or_else(empty)
            }
        }
    }
}
fn reduce(a: &mut f32, b: &mut f32, extent: f32) {
    let sum = *a as f64 + *b as f64;
    if sum > extent as f64 {
        let f = extent as f64 / sum;
        *a = finite(*a as f64 * f);
        *b = finite(*b as f64 * f);
    }
}
fn empty() -> Path {
    Path(vec![Seg::Move(0., 0.), Seg::Close])
}
fn rounded(x: f32, y: f32, w: f32, h: f32, r: [(f32, f32); 4]) -> Path {
    if w <= 0. || h <= 0. {
        return empty();
    }
    let k = 0.552_284_8;
    let [(a, b), (c, d), (e, f), (g, i)] = r;
    Path(vec![
        Seg::Move(x + a, y),
        Seg::Line(x + w - c, y),
        Seg::Cubic(x + w - c + k * c, y, x + w, y + d - k * d, x + w, y + d),
        Seg::Line(x + w, y + h - f),
        Seg::Cubic(
            x + w,
            y + h - f + k * f,
            x + w - e + k * e,
            y + h,
            x + w - e,
            y + h,
        ),
        Seg::Line(x + g, y + h),
        Seg::Cubic(x + g - k * g, y + h, x, y + h - i + k * i, x, y + h - i),
        Seg::Line(x, y + b),
        Seg::Cubic(x, y + b - k * b, x + a - k * a, y, x + a, y),
        Seg::Close,
    ])
}
