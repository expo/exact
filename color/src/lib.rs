//! CSS colour values (LLP 1100 D1, D2): CSS Color 4 and CSS Color HDR's
//! forms, their arithmetic and serialisation. Legacy forms parse to
//! [`Rgba8`]; the rest keep their space ([`Wide`]). No ICC conversion.
//!
//! Transcendentals use the locked `libm` kernels, shared by folding and runtime
//! colour arithmetic, so host libm rounding cannot change Tamber's IR or digests.
//! Cubes use explicit multiplication rather than `powi`'s unspecified precision.

mod mix;
mod named;
pub use mix::{hue_arc, mix, HueMethod, Interpolation, MixSpace};

/// A legacy colour: sRGB channels 0–255 and alpha 0–1, non-premultiplied.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Rgba8 {
    /// Red.
    pub r: u8,
    /// Green.
    pub g: u8,
    /// Blue.
    pub b: u8,
    /// Alpha, 0–1.
    pub a: f64,
}

/// A space a modern colour names (CSS Color 4 §8–§10; CSS Color HDR).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum Space {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    XyzD50,
    XyzD65,
    Lab,
    Lch,
    Oklab,
    Oklch,
    Rec2100Pq,
    Rec2100Hlg,
    Rec2100Linear,
    /// An `hsl()` colour as written: hue in degrees, saturation and
    /// lightness in percent. Only [`Parsed::Legacy`] holds one.
    Hsl,
    /// An `hwb()` colour as written: hue in degrees, whiteness and blackness
    /// in percent, unclamped above. Only [`Parsed::Legacy`] holds one.
    Hwb,
}

impl Space {
    /// `color()`'s identifier, or the function's name.
    pub fn name(self) -> &'static str {
        match self {
            Space::Srgb => "srgb",
            Space::SrgbLinear => "srgb-linear",
            Space::DisplayP3 => "display-p3",
            Space::DisplayP3Linear => "display-p3-linear",
            Space::A98Rgb => "a98-rgb",
            Space::ProphotoRgb => "prophoto-rgb",
            Space::Rec2020 => "rec2020",
            Space::XyzD50 => "xyz-d50",
            Space::XyzD65 => "xyz-d65",
            Space::Lab => "lab",
            Space::Lch => "lch",
            Space::Oklab => "oklab",
            Space::Oklch => "oklch",
            Space::Rec2100Pq => "rec2100-pq",
            Space::Rec2100Hlg => "rec2100-hlg",
            Space::Rec2100Linear => "rec2100-linear",
            Space::Hsl => "hsl",
            Space::Hwb => "hwb",
        }
    }

    fn is_function(self) -> bool {
        matches!(
            self,
            Space::Lab | Space::Lch | Space::Oklab | Space::Oklch | Space::Hsl | Space::Hwb
        )
    }

    /// A Rec. 2100 space, where values above SDR white are meaningful.
    pub fn is_hdr(self) -> bool {
        matches!(
            self,
            Space::Rec2100Pq | Space::Rec2100Hlg | Space::Rec2100Linear
        )
    }

    fn from_color_ident(s: &str) -> Option<Space> {
        Some(match s {
            "srgb" => Space::Srgb,
            "srgb-linear" => Space::SrgbLinear,
            "display-p3" => Space::DisplayP3,
            "display-p3-linear" => Space::DisplayP3Linear,
            "a98-rgb" => Space::A98Rgb,
            "prophoto-rgb" => Space::ProphotoRgb,
            "rec2020" => Space::Rec2020,
            "xyz-d50" => Space::XyzD50,
            "xyz" | "xyz-d65" => Space::XyzD65,
            "rec2100-pq" => Space::Rec2100Pq,
            "rec2100-hlg" => Space::Rec2100Hlg,
            "rec2100-linear" => Space::Rec2100Linear,
            _ => return None,
        })
    }

    /// Percentages' reference ranges (CSS Color 4 §8–§10).
    fn percent(self) -> [f64; 3] {
        match self {
            Space::Lab => [100.0, 125.0, 125.0],
            Space::Lch => [100.0, 150.0, 0.0],
            Space::Oklab => [1.0, 0.4, 0.4],
            Space::Oklch => [1.0, 0.4, 0.0],
            _ => [1.0, 1.0, 1.0],
        }
    }
}

/// A modern colour in its own space.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Wide {
    /// The space.
    pub space: Space,
    /// The components in the space's units, after CSS's clamps.
    pub c: [f64; 3],
    /// Alpha, 0–1.
    pub alpha: f64,
    /// The components, then the alpha, written `none` (CSS Color 4 §4.4):
    /// each is 0 in `c` or `alpha`, as a missing component acts everywhere
    /// but in an interpolation, which takes the other colour's (§12.4).
    pub missing: [bool; 4],
}

/// What a colour string parsed to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Parsed {
    /// A legacy sRGB colour (hex, named, `rgb()`, `hsl()`, `hwb()`): its
    /// 8-bit value, and the colour as written, which an interpolation reads
    /// (CSS Color 4 §12): unrounded, in `hsl` or `hwb` for those functions,
    /// with its `none`s.
    Legacy(Rgba8, Wide),
    /// A modern colour in its own space.
    Wide(Wide),
    /// `currentColor`.
    Current,
}

/// Parse a CSS colour, or `None` when it is not one.
pub fn parse(input: &str) -> Option<Parsed> {
    let s = input.trim().to_ascii_lowercase();
    if s == "currentcolor" {
        return Some(Parsed::Current);
    }
    if s == "transparent" {
        return Some(bytes(Rgba8 {
            r: 0,
            g: 0,
            b: 0,
            a: 0.0,
        }));
    }
    if let Some(hex) = s.strip_prefix('#') {
        return hex_color(hex).map(bytes);
    }
    if let Some(open) = s.find('(') {
        // `rgb (…)` is an identifier and a block, not a function (CSS Syntax 3 §4.3.4).
        let name = &s[..open];
        let body = s[open + 1..].strip_suffix(')')?;
        if let Some(wide) = wide(name, body) {
            return Some(Parsed::Wide(wide));
        }
        return legacy(name, body).map(|(c, w)| Parsed::Legacy(c, w));
    }
    named::named(&s).map(|[r, g, b]| bytes(Rgba8 { r, g, b, a: 1.0 }))
}

/// A colour written as its bytes (hex, named, `transparent`).
fn bytes(c: Rgba8) -> Parsed {
    let w = Wide::new(
        Space::Srgb,
        [c.r, c.g, c.b].map(|v| f64::from(v) / 255.0),
        c.a,
    );
    Parsed::Legacy(c, w)
}

/// The CSS rule a colour function [`parse`] refuses breaks, for a
/// diagnostic: named only when mending that rule alone makes it a colour,
/// so `None` when it reads or has another fault.
pub fn refusal(input: &str) -> Option<&'static str> {
    const FUNCTIONS: [&str; 10] = [
        "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch", "color",
    ];
    if parse(input).is_some() {
        return None;
    }
    let s = input.trim().to_ascii_lowercase();
    let open = s.find('(')?;
    let name = &s[..open];
    let body = s[open + 1..].strip_suffix(')')?;
    let reads = |name: &str, body: &str| parse(&format!("{name}({body})")).is_some();
    if !FUNCTIONS.contains(&name) {
        let name = name.trim_end();
        return (FUNCTIONS.contains(&name) && reads(name, body)).then_some(
            "a function's `(` follows its name with no space between: `rgb(…)`, not `rgb (…)` \
             (CSS Syntax 3 §4.3.4)",
        );
    }
    if name == "color" && body.trim_start().starts_with("--") {
        return None;
    }
    // Each word written as a number that CSS Syntax 3's grammar refuses
    // (`5.`, `1e`, `1.2.3`, `120.deg`) put as `0`, keeping its unit; `10px`,
    // `1-2` (two numbers) and `1e2e3` (a unit `e3`) are not such words.
    let mut mended = String::new();
    let mut malformed = false;
    for piece in body.split_inclusive([',', '/', ' ', '\t', '\n']) {
        let word = piece.trim_end_matches([',', '/', ' ', '\t', '\n']);
        let unit = ["%", "deg", "grad", "rad", "turn"]
            .into_iter()
            .find(|u| word.ends_with(u))
            .unwrap_or("");
        let b = &word.as_bytes()[..word.len() - unit.len()];
        let numeric = b.first().is_some_and(|c| c.is_ascii_digit() || *c == b'.')
            || (matches!(b.first(), Some(b'+' | b'-'))
                && b.get(1).is_some_and(|c| c.is_ascii_digit() || *c == b'.'));
        let chars = b
            .iter()
            .all(|c| c.is_ascii_digit() || matches!(c, b'+' | b'-' | b'.' | b'e' | b'E'));
        let signs = (1..b.len())
            .filter(|&i| matches!(b[i], b'+' | b'-'))
            .all(|i| matches!(b[i - 1], b'e' | b'E'));
        let exponents = b.iter().filter(|c| matches!(c, b'e' | b'E')).count();
        if numeric && chars && signs && exponents <= 1 && !scans(b) {
            malformed = true;
            mended.push('0');
            mended.push_str(unit);
        } else {
            mended.push_str(word);
        }
        mended.push_str(&piece[word.len()..]);
    }
    if malformed {
        return reads(name, &mended).then_some(
            "a number's `.` is followed by a digit, and its exponent's `e` by digits: `5` or \
             `5.0`, not `5.` (CSS Syntax 3 §4.3.13)",
        );
    }
    // Comma-separated components, written space-separated.
    let spaced = |parts: &[&str]| match parts {
        [c @ .., alpha] if parts.len() == 4 => format!("{} / {alpha}", c.join(" ")),
        _ => parts.join(" "),
    };
    if !body.contains(',') {
        let words: Vec<&str> = body.split_whitespace().collect();
        let count = if name == "color" { 5 } else { 4 };
        let slashed = match words.split_last() {
            Some((alpha, c)) if words.len() == count => format!("{} / {alpha}", c.join(" ")),
            _ => return None,
        };
        return (!body.contains('/') && reads(name, &slashed)).then_some(
            "the space-separated syntax puts a `/` before the alpha: `rgb(255 0 0 / 0.5)` \
             (CSS Color 4 §4.1.1)",
        );
    }
    let parts: Vec<&str> = body.split(',').map(str::trim).collect();
    if !matches!(name, "rgb" | "rgba" | "hsl" | "hsla") {
        return reads(name, &spaced(&parts)).then_some(
            "only `rgb()` and `hsl()` take the comma-separated syntax: separate the \
             components with spaces (CSS Color 4 §4.1.2)",
        );
    }
    if parts.contains(&"none") {
        return reads(name, &spaced(&parts)).then_some(
            "the comma-separated syntax takes no `none`; the space-separated one does: \
             `rgb(none 0 0)` (CSS Color 4 §4.1.2)",
        );
    }
    if parts.len() < 3 {
        return None;
    }
    let pct = parts[..3].iter().filter(|p| p.ends_with('%')).count();
    if name.starts_with("rgb") && pct != 0 && pct != 3 {
        let numbers: Vec<String> = parts
            .iter()
            .enumerate()
            .map(|(i, p)| match p.strip_suffix('%').and_then(number) {
                Some(v) if i < 3 => exact_num::js(v * 255.0 / 100.0),
                _ => p.to_string(),
            })
            .collect();
        return reads(name, &numbers.join(", ")).then_some(
            "the comma-separated syntax takes three numbers or three percentages, not both: \
             `rgb(255, 0, 0)` or `rgb(100%, 0%, 0%)` (CSS Color 4 §5.1)",
        );
    }
    if name.starts_with("hsl") && !parts[1..3].iter().all(|p| p.ends_with('%')) {
        let mut mended: Vec<String> = parts.iter().map(|p| p.to_string()).collect();
        for p in &mut mended[1..3] {
            if !p.ends_with('%') {
                p.push('%');
            }
        }
        return reads(name, &mended.join(", ")).then_some(
            "`hsl()`'s comma-separated syntax takes its saturation and lightness as percentages: \
             `hsl(120, 100%, 50%)` (CSS Color 4 §7)",
        );
    }
    None
}

fn hex_color(hex: &str) -> Option<Rgba8> {
    if !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let digit = |i: usize| u8::from_str_radix(&hex[i..i + 1], 16).ok();
    let pair = |i: usize| u8::from_str_radix(&hex[i..i + 2], 16).ok();
    let (r, g, b, a) = match hex.len() {
        3 | 4 => {
            let d = |i| digit(i).map(|v| v * 17);
            let a = if hex.len() == 4 { d(3)? } else { 255 };
            (d(0)?, d(1)?, d(2)?, a)
        }
        6 | 8 => {
            let a = if hex.len() == 8 { pair(6)? } else { 255 };
            (pair(0)?, pair(2)?, pair(4)?, a)
        }
        _ => return None,
    };
    Some(Rgba8 {
        r,
        g,
        b,
        a: a as f64 / 255.0,
    })
}

/// A component; an angle is in degrees. Only a hue takes an angle (Chrome
/// drops `rgb(90deg 0 0)`).
#[derive(Debug, Clone, Copy)]
enum Arg {
    Num(f64),
    Pct(f64),
    Angle(f64),
}

fn component(t: &str) -> Option<Arg> {
    if t == "none" {
        return Some(Arg::Num(0.0));
    }
    if let Some(p) = t.strip_suffix('%') {
        return number(p).map(Arg::Pct);
    }
    for (unit, per_degree) in [
        ("deg", 1.0),
        ("grad", 0.9),
        ("rad", 180.0 / std::f64::consts::PI),
        ("turn", 360.0),
    ] {
        if let Some(v) = t.strip_suffix(unit) {
            return number(v).map(|v| Arg::Angle(v * per_degree));
        }
    }
    number(t).map(Arg::Num)
}

/// A number as CSS Syntax 3 reads one (§4.3.13), and finite.
fn number(t: &str) -> Option<f64> {
    if !scans(t.as_bytes()) {
        return None;
    }
    let v = exact_num::parse_f64(t).ok()?;
    v.is_finite().then_some(v)
}

/// Whether `b` is one number token in CSS Syntax 3's grammar (§4.3.13): a
/// sign, digits, a `.` only before a digit, and an exponent of an `e` and
/// digits. Its value may still be too large for a double.
fn scans(b: &[u8]) -> bool {
    let mut i = usize::from(matches!(b.first(), Some(b'+' | b'-')));
    let digits = |i: &mut usize| {
        let start = *i;
        while b.get(*i).is_some_and(u8::is_ascii_digit) {
            *i += 1;
        }
        *i > start
    };
    let whole = digits(&mut i);
    if b.get(i) == Some(&b'.') {
        i += 1;
        if !digits(&mut i) {
            return false;
        }
    } else if !whole {
        return false;
    }
    if matches!(b.get(i), Some(b'e' | b'E')) {
        i += 1 + usize::from(matches!(b.get(i + 1), Some(b'+' | b'-')));
        if !digits(&mut i) {
            return false;
        }
    }
    i == b.len()
}

/// `a, b, c[, d]` or `a b c [/ d]`.
fn args(body: &str) -> Option<(Vec<&str>, Option<&str>)> {
    let body = body.trim();
    if body.contains(',') {
        let parts: Vec<&str> = body.split(',').map(str::trim).collect();
        if parts
            .iter()
            // The comma-separated syntax takes no `none` (CSS Color 4 §4.1.2).
            .any(|p| p.is_empty() || p.contains(' ') || p.contains('/') || *p == "none")
        {
            return None;
        }
        return match parts.len() {
            3 => Some((parts, None)),
            4 => Some((parts[..3].to_vec(), Some(parts[3]))),
            _ => None,
        };
    }
    let (main, alpha) = match body.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (body, None),
    };
    let parts: Vec<&str> = main.split_whitespace().collect();
    (parts.len() == 3 && alpha != Some("")).then_some((parts, alpha))
}

fn alpha_value(t: Option<&str>) -> Option<f64> {
    let a = match t.map(component) {
        None => 1.0,
        Some(Some(Arg::Num(v))) => v,
        Some(Some(Arg::Pct(p))) => p / 100.0,
        Some(Some(Arg::Angle(_)) | None) => return None,
    };
    Some(a.clamp(0.0, 1.0))
}

fn channel(v: f64) -> u8 {
    v.clamp(0.0, 255.0).round() as u8
}

/// The 8-bit colour and the colour as written.
fn legacy(name: &str, body: &str) -> Option<(Rgba8, Wide)> {
    let (parts, alpha) = args(body)?;
    let c: Vec<Arg> = parts.iter().map(|p| component(p)).collect::<Option<_>>()?;
    let a = alpha_value(alpha)?;
    let legacy = body.contains(',');
    let mut missing = [false; 4];
    for (i, part) in parts.iter().enumerate() {
        missing[i] = *part == "none";
    }
    missing[3] = alpha == Some("none");
    let written = |space, c| Wide {
        space,
        c,
        alpha: a,
        missing,
    };
    match name {
        "rgb" | "rgba" => {
            // Legacy syntax: all numbers or all percentages.
            let pct = c.iter().filter(|x| matches!(x, Arg::Pct(_))).count();
            if legacy && pct != 0 && pct != 3 {
                return None;
            }
            let v = |x: Arg| match x {
                Arg::Num(n) => Some(n.clamp(0.0, 255.0)),
                Arg::Pct(p) => Some((p * 255.0 / 100.0).clamp(0.0, 255.0)),
                Arg::Angle(_) => None,
            };
            let v = [v(c[0])?, v(c[1])?, v(c[2])?];
            Some((
                Rgba8 {
                    r: channel(v[0]),
                    g: channel(v[1]),
                    b: channel(v[2]),
                    a,
                },
                written(Space::Srgb, v.map(|x| x / 255.0)),
            ))
        }
        "hsl" | "hsla" | "hwb" => {
            let (Arg::Num(h) | Arg::Angle(h)) = c[0] else {
                return None;
            };
            // hsl() clamps its saturation and lightness to 100%; hwb()'s
            // whiteness and blackness have no upper bound, `hwb()` scaling a
            // sum over 100% down (CSS Color 4 §7, §8).
            let top = if name == "hwb" { f64::INFINITY } else { 1.0 };
            let frac = |x: Arg| match x {
                Arg::Pct(p) => Some((p / 100.0).clamp(0.0, top)),
                Arg::Num(n) if !legacy => Some((n / 100.0).clamp(0.0, top)),
                Arg::Num(_) | Arg::Angle(_) => None,
            };
            let (s, l) = (frac(c[1])?, frac(c[2])?);
            let ((r, g, b), space) = if name == "hwb" {
                if legacy {
                    return None;
                }
                (hwb(h, s, l), Space::Hwb)
            } else {
                (hsl(h, s, l), Space::Hsl)
            };
            Some((
                Rgba8 {
                    r: channel(r * 255.0),
                    g: channel(g * 255.0),
                    b: channel(b * 255.0),
                    a,
                },
                written(space, [h.rem_euclid(360.0), s * 100.0, l * 100.0]),
            ))
        }
        _ => None,
    }
}

pub(crate) fn hsl(h: f64, s: f64, l: f64) -> (f64, f64, f64) {
    let h = h.rem_euclid(360.0);
    let f = |n: f64| {
        let k = (n + h / 30.0) % 12.0;
        let a = s * l.min(1.0 - l);
        l - a * (k - 3.0).min(9.0 - k).clamp(-1.0, 1.0)
    };
    (f(0.0), f(8.0), f(4.0))
}

pub(crate) fn hwb(h: f64, w: f64, b: f64) -> (f64, f64, f64) {
    if w + b >= 1.0 {
        let g = w / (w + b);
        return (g, g, g);
    }
    let (r, g, bl) = hsl(h, 1.0, 0.5);
    let f = |c: f64| c * (1.0 - w - b) + w;
    (f(r), f(g), f(bl))
}

fn wide(name: &str, body: &str) -> Option<Wide> {
    let (space, body) = match name {
        "lab" => (Space::Lab, body),
        "lch" => (Space::Lch, body),
        "oklab" => (Space::Oklab, body),
        "oklch" => (Space::Oklch, body),
        "color" => {
            let body = body.trim_start();
            let end = body.find(char::is_whitespace)?;
            (Space::from_color_ident(&body[..end])?, &body[end..])
        }
        _ => return None,
    };
    if body.contains(',') {
        return None;
    }
    let (parts, a) = args(body)?;
    let c: Vec<Arg> = parts.iter().map(|p| component(p)).collect::<Option<_>>()?;
    let pct = space.percent();
    let polar = matches!(space, Space::Lch | Space::Oklch);
    let mut v = [0.0; 3];
    for i in 0..3 {
        let hue = polar && i == 2;
        v[i] = match c[i] {
            Arg::Num(n) => n,
            Arg::Angle(d) if hue => d,
            Arg::Pct(p) if !hue => p / 100.0 * pct[i],
            Arg::Pct(_) | Arg::Angle(_) => return None,
        };
    }
    match space {
        Space::Lab | Space::Lch => v[0] = v[0].clamp(0.0, 100.0),
        Space::Oklab | Space::Oklch => v[0] = v[0].clamp(0.0, 1.0),
        _ => {}
    }
    if polar {
        v[1] = v[1].max(0.0);
        v[2] = v[2].rem_euclid(360.0);
    }
    let mut missing = [false; 4];
    for (i, part) in parts.iter().enumerate() {
        missing[i] = *part == "none";
    }
    missing[3] = a == Some("none");
    Some(Wide {
        space,
        c: v,
        alpha: alpha_value(a)?,
        missing,
    })
}

impl Wide {
    /// A colour with every component given.
    pub const fn new(space: Space, c: [f64; 3], alpha: f64) -> Wide {
        Wide {
            space,
            c,
            alpha,
            missing: [false; 4],
        }
    }

    /// CSS's serialisation: `oklch(0.7 0.1 200 / 0.5)`, a missing
    /// component as `none`.
    pub fn css(&self) -> String {
        let n = |i: usize, v: f64| {
            if self.missing[i] {
                "none".to_string()
            } else {
                exact_num::js(v)
            }
        };
        let [a, b, c] = [0, 1, 2].map(|i| n(i, self.c[i]));
        let pct = |t: String| if t == "none" { t } else { t + "%" };
        let mut text = if matches!(self.space, Space::Hsl | Space::Hwb) {
            format!("{}({a} {} {}", self.space.name(), pct(b), pct(c))
        } else if self.space.is_function() {
            format!("{}({a} {b} {c}", self.space.name())
        } else {
            format!("color({} {a} {b} {c}", self.space.name())
        };
        if self.alpha < 1.0 || self.missing[3] {
            text.push_str(" / ");
            text.push_str(&n(3, self.alpha));
        }
        text.push(')');
        text
    }

    /// Extended linear sRGB, unclipped; HDR reference white (203 cd/m²) is 1.
    pub fn linear_srgb(&self) -> [f64; 3] {
        let v = self.c;
        let xyz = match self.space {
            Space::Srgb => return v.map(srgb_to_linear),
            Space::SrgbLinear => return v,
            Space::DisplayP3 => mul(&P3_TO_XYZ, v.map(srgb_to_linear)),
            Space::DisplayP3Linear => mul(&P3_TO_XYZ, v),
            Space::A98Rgb => mul(
                &A98_TO_XYZ,
                v.map(|c| libm::pow(c.abs(), 563.0 / 256.0).copysign(c)),
            ),
            Space::ProphotoRgb => mul(
                &D50_TO_D65,
                mul(
                    &PROPHOTO_TO_XYZ_D50,
                    v.map(|c| {
                        let a = c.abs();
                        (if a <= 16.0 / 512.0 {
                            a / 16.0
                        } else {
                            libm::pow(a, 1.8)
                        })
                        .copysign(c)
                    }),
                ),
            ),
            Space::Rec2020 => mul(&REC2020_TO_XYZ, v.map(rec2020_to_linear)),
            Space::XyzD50 => mul(&D50_TO_D65, v),
            Space::XyzD65 => v,
            Space::Lab => mul(&D50_TO_D65, lab_to_xyz_d50(v[0], v[1], v[2])),
            Space::Lch => {
                let h = v[2].to_radians();
                mul(
                    &D50_TO_D65,
                    lab_to_xyz_d50(v[0], v[1] * libm::cos(h), v[1] * libm::sin(h)),
                )
            }
            Space::Oklab => return oklab_to_linear(v[0], v[1], v[2]),
            Space::Hsl => return MixSpace::Hsl.to_linear_srgb(v),
            Space::Hwb => return MixSpace::Hwb.to_linear_srgb(v),
            Space::Oklch => {
                let h = v[2].to_radians();
                return oklab_to_linear(v[0], v[1] * libm::cos(h), v[1] * libm::sin(h));
            }
            Space::Rec2100Pq => mul(&REC2020_TO_XYZ, v.map(|e| pq_nits(e) / 203.0)),
            Space::Rec2100Hlg => mul(
                &REC2020_TO_XYZ,
                v.map(|e| (hlg_linear(e.abs()) / hlg_linear(0.75)).copysign(e)),
            ),
            Space::Rec2100Linear => mul(&REC2020_TO_XYZ, v),
        };
        mul(&XYZ_TO_SRGB, xyz)
    }

    /// Within sRGB's gamut and SDR range, to a rounding.
    pub fn in_srgb(&self) -> bool {
        self.linear_srgb()
            .iter()
            .all(|c| (-1e-4..=1.0 + 1e-4).contains(c))
    }

    /// 8-bit sRGB, clipped.
    pub fn srgb8(&self) -> Rgba8 {
        encode8(self.linear_srgb(), quantized(self.alpha))
    }

    /// 8-bit Display P3, clipped: a `display-p3` canvas's pixels.
    pub fn display_p3_8(&self) -> Rgba8 {
        p3_8(self.linear_srgb(), self.alpha)
    }
}

fn quantized(alpha: f64) -> f64 {
    (alpha * 255.0).round() / 255.0
}

/// Linear components, sRGB-encoded and clipped to 8 bits.
fn encode8(linear: [f64; 3], a: f64) -> Rgba8 {
    let [r, g, b] = linear.map(|c| channel(linear_to_srgb(c).clamp(0.0, 1.0) * 255.0));
    Rgba8 { r, g, b, a }
}

/// Written out so `canvas/recorder.js` carries the same digits.
const LINEAR_SRGB_TO_P3: M3 = [
    [0.8224619687143622, 0.17753803128563764, 0.0],
    [0.033194198850961525, 0.966805801149038, 0.0],
    [0.01708263072111999, 0.07239744066396336, 0.9105199286149165],
];
const LINEAR_P3_TO_SRGB: M3 = [
    [1.22494017628056, -0.22494017628055996, 0.0],
    [-0.04205695470968818, 1.0420569547096883, 0.0],
    [
        -0.01963755459033439,
        -0.0786360455506318,
        1.0982736001409665,
    ],
];

fn p3_8(linear_srgb: [f64; 3], alpha: f64) -> Rgba8 {
    encode8(mul(&LINEAR_SRGB_TO_P3, linear_srgb), quantized(alpha))
}

/// An 8-bit sRGB colour as 8-bit Display P3.
pub fn srgb8_to_p3(c: Rgba8) -> Rgba8 {
    p3_8(
        [c.r, c.g, c.b].map(|v| srgb_to_linear(f64::from(v) / 255.0)),
        c.a,
    )
}

/// An 8-bit Display P3 colour as 8-bit sRGB, clipped.
pub fn p3_8_to_srgb(c: Rgba8) -> Rgba8 {
    let lin = mul(
        &LINEAR_P3_TO_SRGB,
        [c.r, c.g, c.b].map(|v| srgb_to_linear(f64::from(v) / 255.0)),
    );
    encode8(lin, c.a)
}

type M3 = [[f64; 3]; 3];
const XYZ_TO_SRGB: M3 = [
    [3.2409699419045226, -1.537383177570094, -0.4986107602930034],
    [-0.9692436362808796, 1.8759675015077202, 0.04155505740717559],
    [
        0.05563007969699366,
        -0.20397695888897652,
        1.0569715142428786,
    ],
];
const D50_TO_D65: M3 = [
    [0.955473421488075, -0.02309845494876471, 0.06325924320057072],
    [
        -0.0283697093338637,
        1.0099953980813041,
        0.021041441191917323,
    ],
    [
        0.012314014864481998,
        -0.020507649298898964,
        1.330365926242124,
    ],
];
const P3_TO_XYZ: M3 = [
    [0.4865709486482162, 0.26566769316909306, 0.1982172852343625],
    [0.2289745640697488, 0.6917385218365064, 0.079286914093745],
    [0.0, 0.04511338185890264, 1.043944368900976],
];
const A98_TO_XYZ: M3 = [
    [0.5766690429101305, 0.1855582379065463, 0.1882286462349947],
    [0.29734497525053605, 0.6273635662554661, 0.0752914584939978],
    [0.02703136138641234, 0.07068885253582723, 0.9913375368376388],
];
const PROPHOTO_TO_XYZ_D50: M3 = [
    [0.7977666449006423, 0.13518129740053308, 0.0313477341283922],
    [0.2880748288194013, 0.711835234241873, 0.00008993693872564],
    [0.0, 0.0, 0.8251046025104602],
];
const REC2020_TO_XYZ: M3 = [
    [0.6369580483012914, 0.14461690358620832, 0.1688809751641721],
    [0.2627002120112671, 0.6779980715188708, 0.05930171646986196],
    [0.0, 0.028072693049087428, 1.060985057710791],
];

fn mul(m: &M3, v: [f64; 3]) -> [f64; 3] {
    [0, 1, 2].map(|i| m[i][0] * v[0] + m[i][1] * v[1] + m[i][2] * v[2])
}

/// sRGB's transfer, extended to negatives.
pub fn srgb_to_linear(c: f64) -> f64 {
    let a = c.abs();
    let v = if a <= 0.04045 {
        a / 12.92
    } else {
        libm::pow((a + 0.055) / 1.055, 2.4)
    };
    v.copysign(c)
}

/// sRGB's inverse transfer, extended to negatives and above 1.
pub fn linear_to_srgb(c: f64) -> f64 {
    let a = c.abs();
    let v = if a <= 0.0031308 {
        12.92 * a
    } else {
        1.055 * libm::pow(a, 1.0 / 2.4) - 0.055
    };
    v.copysign(c)
}

/// BT.2020's inverse OETF (CSS Color 4 sample code).
fn rec2020_to_linear(c: f64) -> f64 {
    const ALPHA: f64 = 1.09929682680944;
    const BETA: f64 = 0.018053968510807;
    let a = c.abs();
    let v = if a < BETA * 4.5 {
        a / 4.5
    } else {
        libm::pow((a + ALPHA - 1.0) / ALPHA, 1.0 / 0.45)
    };
    v.copysign(c)
}

/// BT.2100 PQ: a signal to cd/m².
fn pq_nits(e: f64) -> f64 {
    let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
    let (c1, c2, c3) = (
        3424.0 / 4096.0,
        2413.0 / 4096.0 * 32.0,
        2392.0 / 4096.0 * 32.0,
    );
    let p = libm::pow(e.max(0.0), 1.0 / m2);
    10000.0 * libm::pow((p - c1).max(0.0) / (c2 - c3 * p), 1.0 / m1)
}

/// Extended linear sRGB in a Rec. 2100 space's components, the inverse of
/// [`Wide::linear_srgb`] there (HDR reference white, 203 cd/m², is 1); `None`
/// for another space.
pub fn rec2100_from_linear_srgb(space: Space, v: [f64; 3]) -> Option<[f64; 3]> {
    let linear = from_xyz65(Space::Rec2020, to_xyz65_from_srgb(v));
    Some(match space {
        Space::Rec2100Linear => linear,
        Space::Rec2100Pq => linear.map(|l| {
            let (m1, m2) = (2610.0 / 16384.0, 2523.0 / 4096.0 * 128.0);
            let (c1, c2, c3) = (
                3424.0 / 4096.0,
                2413.0 / 4096.0 * 32.0,
                2392.0 / 4096.0 * 32.0,
            );
            let y = libm::pow((l * 203.0 / 10000.0).max(0.0), m1);
            libm::pow((c1 + c2 * y) / (1.0 + c3 * y), m2)
        }),
        // Signed, as the decode is: an extended component keeps its sign.
        Space::Rec2100Hlg => linear.map(|l| {
            let (a, b, c) = (0.17883277, 0.28466892, 0.55991073);
            let scene = (l * hlg_linear(0.75)).abs();
            let e = if scene <= 1.0 / 12.0 {
                libm::sqrt(3.0 * scene)
            } else {
                a * libm::log(12.0 * scene - b) + c
            };
            e.copysign(l)
        }),
        _ => return None,
    })
}

/// BT.2100 HLG's inverse OETF: a signal to scene-linear light, 0–1.
fn hlg_linear(e: f64) -> f64 {
    let (a, b, c) = (0.17883277, 0.28466892, 0.55991073);
    if e <= 0.5 {
        e * e / 3.0
    } else {
        (libm::exp((e - c) / a) + b) / 12.0
    }
}

fn cube(v: f64) -> f64 {
    v * v * v
}

fn lab_to_xyz_d50(l: f64, a: f64, b: f64) -> [f64; 3] {
    const K: f64 = 24389.0 / 27.0;
    const E: f64 = 216.0 / 24389.0;
    let f1 = (l + 16.0) / 116.0;
    let f0 = a / 500.0 + f1;
    let f2 = f1 - b / 200.0;
    let x = if cube(f0) > E {
        cube(f0)
    } else {
        (116.0 * f0 - 16.0) / K
    };
    let y = if l > K * E { cube(f1) } else { l / K };
    let z = if cube(f2) > E {
        cube(f2)
    } else {
        (116.0 * f2 - 16.0) / K
    };
    [x * 0.3457 / 0.3585, y, z * (1.0 - 0.3457 - 0.3585) / 0.3585]
}

fn oklab_to_linear(l: f64, a: f64, b: f64) -> [f64; 3] {
    let l_ = cube(l + 0.3963377774 * a + 0.2158037573 * b);
    let m_ = cube(l - 0.1055613458 * a - 0.0638541728 * b);
    let s_ = cube(l - 0.0894841775 * a - 1.2914855480 * b);
    [
        4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_,
        -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_,
        -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_,
    ]
}

const D65_TO_D50: M3 = [
    [
        1.0479297925449969,
        0.022946870601609652,
        -0.05019226628920524,
    ],
    [
        0.02962780877005599,
        0.9904344267538799,
        -0.017073799063418826,
    ],
    [
        -0.009243040646204504,
        0.015055191490298152,
        0.7518742814281371,
    ],
];

pub(crate) fn to_xyz65_from_srgb(v: [f64; 3]) -> [f64; 3] {
    mul(&inv3(&XYZ_TO_SRGB), v)
}

pub(crate) fn xyz65_to_d50(v: [f64; 3]) -> [f64; 3] {
    mul(&D65_TO_D50, v)
}

/// XYZ (D65) to an RGB space's linear components.
pub(crate) fn from_xyz65(space: Space, xyz: [f64; 3]) -> [f64; 3] {
    match space {
        Space::DisplayP3 | Space::DisplayP3Linear => mul(&inv3(&P3_TO_XYZ), xyz),
        Space::A98Rgb => mul(&inv3(&A98_TO_XYZ), xyz),
        Space::ProphotoRgb => mul(&inv3(&PROPHOTO_TO_XYZ_D50), xyz65_to_d50(xyz)),
        Space::Rec2020 => mul(&inv3(&REC2020_TO_XYZ), xyz),
        _ => mul(&XYZ_TO_SRGB, xyz),
    }
}

/// BT.2020's OETF (CSS Color 4 sample code).
pub(crate) fn linear_to_rec2020(c: f64) -> f64 {
    const ALPHA: f64 = 1.09929682680944;
    const BETA: f64 = 0.018053968510807;
    let a = c.abs();
    let v = if a < BETA {
        4.5 * a
    } else {
        ALPHA * libm::pow(a, 0.45) - (ALPHA - 1.0)
    };
    v.copysign(c)
}

pub(crate) fn xyz_d50_to_lab(xyz: [f64; 3]) -> [f64; 3] {
    const K: f64 = 24389.0 / 27.0;
    const E: f64 = 216.0 / 24389.0;
    let white = [0.3457 / 0.3585, 1.0, (1.0 - 0.3457 - 0.3585) / 0.3585];
    let f = |v: f64| {
        if v > E {
            libm::cbrt(v)
        } else {
            (K * v + 16.0) / 116.0
        }
    };
    let [fx, fy, fz] = [
        f(xyz[0] / white[0]),
        f(xyz[1] / white[1]),
        f(xyz[2] / white[2]),
    ];
    [116.0 * fy - 16.0, 500.0 * (fx - fy), 200.0 * (fy - fz)]
}

/// Extended linear sRGB as Oklab `[L, a, b]`, unclipped.
pub fn linear_to_oklab([r, g, b]: [f64; 3]) -> [f64; 3] {
    let l = libm::cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
    let m = libm::cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
    let s = libm::cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn inv3(m: &M3) -> M3 {
    let (a, b, c) = (m[0][0], m[0][1], m[0][2]);
    let (d, e, f) = (m[1][0], m[1][1], m[1][2]);
    let (g, h, i) = (m[2][0], m[2][1], m[2][2]);
    let det = a * (e * i - f * h) - b * (d * i - f * g) + c * (d * h - e * g);
    [
        [
            (e * i - f * h) / det,
            (c * h - b * i) / det,
            (b * f - c * e) / det,
        ],
        [
            (f * g - d * i) / det,
            (a * i - c * g) / det,
            (c * d - a * f) / det,
        ],
        [
            (d * h - e * g) / det,
            (b * g - a * h) / det,
            (a * e - b * d) / det,
        ],
    ]
}

/// Standard spaces the platforms ship but CSS doesn't predefine (LLP 1100
/// D1): the dashed name, Apple's `CGColorSpace` name, the component count.
pub const PROFILES: [(&str, &str, usize); 12] = [
    ("--dci-p3", "kCGColorSpaceDCIP3", 3),
    ("--rec709", "kCGColorSpaceITUR_709", 3),
    (
        "--rec2020-srgb-transfer",
        "kCGColorSpaceITUR_2020_sRGBGamma",
        3,
    ),
    ("--rec2020-linear", "kCGColorSpaceLinearITUR_2020", 3),
    ("--display-p3-pq", "kCGColorSpaceDisplayP3_PQ", 3),
    ("--display-p3-hlg", "kCGColorSpaceDisplayP3_HLG", 3),
    ("--rec709-pq", "kCGColorSpaceITUR_709_PQ", 3),
    ("--rec709-hlg", "kCGColorSpaceITUR_709_HLG", 3),
    ("--aces-cg", "kCGColorSpaceACESCGLinear", 3),
    ("--romm-rgb", "kCGColorSpaceROMMRGB", 3),
    ("--gray-gamma-2.2", "kCGColorSpaceGenericGrayGamma2_2", 1),
    ("--gray-linear", "kCGColorSpaceLinearGray", 1),
];

/// `color(--name c… [/ alpha])`: the name, one to four components, alpha.
/// The function's name is case-insensitive and a dashed ident is not, as
/// in CSS: an app's `--Brand` keeps its case. A standard space (`PROFILES`)
/// is matched in any case and named in lowercase.
pub fn parse_profiled(input: &str) -> Option<(String, Vec<f64>, f64)> {
    let s = input.trim();
    if !s.get(..6)?.eq_ignore_ascii_case("color(") {
        return None;
    }
    let body = s[6..].strip_suffix(')')?.trim_start();
    if !body.starts_with("--") || body.contains(',') {
        return None;
    }
    let end = body.find(char::is_whitespace)?;
    let (name, rest) = (&body[..end], body[end..].to_ascii_lowercase());
    let lower = name.to_ascii_lowercase();
    let name = if PROFILES.iter().any(|p| p.0 == lower) {
        lower
    } else {
        name.to_string()
    };
    let (main, alpha) = match rest.split_once('/') {
        Some((m, a)) => (m, Some(a.trim())),
        None => (rest.as_str(), None),
    };
    let c: Vec<f64> = main
        .split_whitespace()
        .map(|t| match component(t)? {
            Arg::Num(n) => Some(n),
            Arg::Pct(p) => Some(p / 100.0),
            Arg::Angle(_) => None,
        })
        .collect::<Option<_>>()?;
    if c.is_empty() || c.len() > 4 || alpha == Some("") {
        return None;
    }
    Some((name, c, alpha_value(alpha)?))
}

#[cfg(test)]
mod tests;
