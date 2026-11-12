//! Colour interpolation (CSS Color 4 §12).

use crate::{linear_to_srgb, srgb_to_linear, Space, Wide};

/// A `<color-interpolation-method>` space.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[allow(missing_docs)]
pub enum MixSpace {
    Srgb,
    SrgbLinear,
    DisplayP3,
    DisplayP3Linear,
    A98Rgb,
    ProphotoRgb,
    Rec2020,
    Lab,
    Oklab,
    XyzD50,
    XyzD65,
    Hsl,
    Hwb,
    Lch,
    Oklch,
}

/// How a polar space's hue goes round (CSS Color 4 §12.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[allow(missing_docs)]
pub enum HueMethod {
    #[default]
    Shorter,
    Longer,
    Increasing,
    Decreasing,
}

/// A `<color-interpolation-method>`: `in oklch longer hue`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Interpolation {
    /// The space.
    pub space: MixSpace,
    /// The hue method; polar spaces only.
    pub hue: HueMethod,
}

impl Interpolation {
    /// CSS's default for non-legacy colours.
    pub const OKLAB: Interpolation = Interpolation {
        space: MixSpace::Oklab,
        hue: HueMethod::Shorter,
    };

    /// `in <space> [<method> hue]` at the start of `words`, and the words it took.
    pub fn parse(words: &[&str]) -> Option<(Interpolation, usize)> {
        if words.first().map(|w| w.to_ascii_lowercase()) != Some("in".into()) {
            return None;
        }
        let space = MixSpace::from_name(&words.get(1)?.to_ascii_lowercase())?;
        let polar = space.is_polar();
        let method = match (words.get(2), words.get(3)) {
            (Some(m), Some(h)) if polar && h.eq_ignore_ascii_case("hue") => {
                Some(match m.to_ascii_lowercase().as_str() {
                    "shorter" => HueMethod::Shorter,
                    "longer" => HueMethod::Longer,
                    "increasing" => HueMethod::Increasing,
                    "decreasing" => HueMethod::Decreasing,
                    _ => return None,
                })
            }
            _ => None,
        };
        Some((
            Interpolation {
                space,
                hue: method.unwrap_or_default(),
            },
            if method.is_some() { 4 } else { 2 },
        ))
    }

    /// CSS's serialisation, omitting the default hue method.
    pub fn css(self) -> String {
        match self.hue {
            HueMethod::Shorter => format!("in {}", self.space.name()),
            m => format!(
                "in {} {} hue",
                self.space.name(),
                ["shorter", "longer", "increasing", "decreasing"][m as usize]
            ),
        }
    }
}

impl MixSpace {
    fn name(self) -> &'static str {
        match self {
            MixSpace::Srgb => "srgb",
            MixSpace::SrgbLinear => "srgb-linear",
            MixSpace::DisplayP3 => "display-p3",
            MixSpace::DisplayP3Linear => "display-p3-linear",
            MixSpace::A98Rgb => "a98-rgb",
            MixSpace::ProphotoRgb => "prophoto-rgb",
            MixSpace::Rec2020 => "rec2020",
            MixSpace::Lab => "lab",
            MixSpace::Oklab => "oklab",
            MixSpace::XyzD50 => "xyz-d50",
            MixSpace::XyzD65 => "xyz-d65",
            MixSpace::Hsl => "hsl",
            MixSpace::Hwb => "hwb",
            MixSpace::Lch => "lch",
            MixSpace::Oklch => "oklch",
        }
    }

    fn from_name(name: &str) -> Option<MixSpace> {
        Some(match name {
            "srgb" => MixSpace::Srgb,
            "srgb-linear" => MixSpace::SrgbLinear,
            "display-p3" => MixSpace::DisplayP3,
            "display-p3-linear" => MixSpace::DisplayP3Linear,
            "a98-rgb" => MixSpace::A98Rgb,
            "prophoto-rgb" => MixSpace::ProphotoRgb,
            "rec2020" => MixSpace::Rec2020,
            "lab" => MixSpace::Lab,
            "oklab" => MixSpace::Oklab,
            "xyz" | "xyz-d65" => MixSpace::XyzD65,
            "xyz-d50" => MixSpace::XyzD50,
            "hsl" => MixSpace::Hsl,
            "hwb" => MixSpace::Hwb,
            "lch" => MixSpace::Lch,
            "oklch" => MixSpace::Oklch,
            _ => return None,
        })
    }

    fn is_polar(self) -> bool {
        matches!(
            self,
            MixSpace::Hsl | MixSpace::Hwb | MixSpace::Lch | MixSpace::Oklch
        )
    }

    /// The colour space whose components are this space's.
    pub fn own(self) -> Space {
        match self {
            MixSpace::Srgb => Space::Srgb,
            MixSpace::SrgbLinear => Space::SrgbLinear,
            MixSpace::DisplayP3 => Space::DisplayP3,
            MixSpace::DisplayP3Linear => Space::DisplayP3Linear,
            MixSpace::A98Rgb => Space::A98Rgb,
            MixSpace::ProphotoRgb => Space::ProphotoRgb,
            MixSpace::Rec2020 => Space::Rec2020,
            MixSpace::Lab => Space::Lab,
            MixSpace::Oklab => Space::Oklab,
            MixSpace::XyzD50 => Space::XyzD50,
            MixSpace::XyzD65 => Space::XyzD65,
            MixSpace::Lch => Space::Lch,
            MixSpace::Oklch => Space::Oklch,
            MixSpace::Hsl => Space::Hsl,
            MixSpace::Hwb => Space::Hwb,
        }
    }

    fn hue_index(self) -> usize {
        match self {
            MixSpace::Hsl | MixSpace::Hwb => 0,
            _ => 2,
        }
    }

    /// Extended linear sRGB to this space's components.
    pub fn from_linear_srgb(self, v: [f64; 3]) -> [f64; 3] {
        let encode = |v: [f64; 3]| v.map(linear_to_srgb);
        match self {
            MixSpace::Srgb => encode(v),
            MixSpace::SrgbLinear => v,
            MixSpace::DisplayP3 => encode(crate::from_xyz65(
                Space::DisplayP3Linear,
                crate::to_xyz65_from_srgb(v),
            )),
            MixSpace::DisplayP3Linear => {
                crate::from_xyz65(Space::DisplayP3Linear, crate::to_xyz65_from_srgb(v))
            }
            MixSpace::A98Rgb => crate::from_xyz65(Space::A98Rgb, crate::to_xyz65_from_srgb(v))
                .map(|c| libm::pow(c.abs(), 256.0 / 563.0).copysign(c)),
            MixSpace::ProphotoRgb => {
                crate::from_xyz65(Space::ProphotoRgb, crate::to_xyz65_from_srgb(v)).map(|c| {
                    let a = c.abs();
                    (if a < 1.0 / 512.0 {
                        a * 16.0
                    } else {
                        libm::pow(a, 1.0 / 1.8)
                    })
                    .copysign(c)
                })
            }
            MixSpace::Rec2020 => crate::from_xyz65(Space::Rec2020, crate::to_xyz65_from_srgb(v))
                .map(crate::linear_to_rec2020),
            MixSpace::XyzD65 => crate::to_xyz65_from_srgb(v),
            MixSpace::XyzD50 => crate::xyz65_to_d50(crate::to_xyz65_from_srgb(v)),
            MixSpace::Lab => {
                crate::xyz_d50_to_lab(crate::xyz65_to_d50(crate::to_xyz65_from_srgb(v)))
            }
            MixSpace::Lch => polar(crate::xyz_d50_to_lab(crate::xyz65_to_d50(
                crate::to_xyz65_from_srgb(v),
            ))),
            MixSpace::Oklab => crate::linear_to_oklab(v),
            MixSpace::Oklch => polar(crate::linear_to_oklab(v)),
            MixSpace::Hsl => rgb_to_hsl(encode(v)),
            MixSpace::Hwb => rgb_to_hwb(encode(v)),
        }
    }

    /// This space's components to extended linear sRGB.
    pub fn to_linear_srgb(self, c: [f64; 3]) -> [f64; 3] {
        let as_wide = |space| Wide::new(space, c, 1.0).linear_srgb();
        match self {
            MixSpace::Srgb => c.map(srgb_to_linear),
            MixSpace::SrgbLinear => c,
            MixSpace::DisplayP3 => as_wide(Space::DisplayP3),
            MixSpace::DisplayP3Linear => as_wide(Space::DisplayP3Linear),
            MixSpace::A98Rgb => as_wide(Space::A98Rgb),
            MixSpace::ProphotoRgb => as_wide(Space::ProphotoRgb),
            MixSpace::Rec2020 => as_wide(Space::Rec2020),
            MixSpace::XyzD65 => as_wide(Space::XyzD65),
            MixSpace::XyzD50 => as_wide(Space::XyzD50),
            MixSpace::Lab => as_wide(Space::Lab),
            MixSpace::Lch => as_wide(Space::Lch),
            MixSpace::Oklab => as_wide(Space::Oklab),
            MixSpace::Oklch => as_wide(Space::Oklch),
            MixSpace::Hsl => {
                let (r, g, b) = crate::hsl(c[0], c[1] / 100.0, c[2] / 100.0);
                [r, g, b].map(srgb_to_linear)
            }
            MixSpace::Hwb => {
                let (r, g, b) = crate::hwb(c[0], c[1] / 100.0, c[2] / 100.0);
                [r, g, b].map(srgb_to_linear)
            }
        }
    }
}

/// a/b to chroma/hue (degrees).
fn polar([l, a, b]: [f64; 3]) -> [f64; 3] {
    let h = libm::atan2(b, a).to_degrees().rem_euclid(360.0);
    [l, libm::sqrt(a * a + b * b), h]
}

fn rgb_to_hsl([r, g, b]: [f64; 3]) -> [f64; 3] {
    let (max, min) = (r.max(g).max(b), r.min(g).min(b));
    let (l, d) = ((max + min) / 2.0, max - min);
    let s = if d == 0.0 || l == 0.0 || l == 1.0 {
        0.0
    } else {
        d / (1.0 - (2.0 * l - 1.0).abs())
    };
    let h = if d == 0.0 {
        0.0
    } else if max == r {
        60.0 * ((g - b) / d).rem_euclid(6.0)
    } else if max == g {
        60.0 * ((b - r) / d + 2.0)
    } else {
        60.0 * ((r - g) / d + 4.0)
    };
    [h, s * 100.0, l * 100.0]
}

fn rgb_to_hwb(rgb: [f64; 3]) -> [f64; 3] {
    let h = rgb_to_hsl(rgb)[0];
    let (w, bl) = (
        rgb[0].min(rgb[1]).min(rgb[2]),
        1.0 - rgb[0].max(rgb[1]).max(rgb[2]),
    );
    [h, w * 100.0, bl * 100.0]
}

/// The kinds of component CSS Color 4 §12.4 calls analogous, which a missing
/// component keeps across a conversion. Whiteness and blackness are hwb's
/// alone.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Red,
    Green,
    Blue,
    Lightness,
    Colorfulness,
    Hue,
    OppositeA,
    OppositeB,
    Whiteness,
    Blackness,
}

/// A colour space's components, by kind.
fn kinds(space: Space) -> [Kind; 3] {
    use Kind::*;
    match space {
        Space::Lab | Space::Oklab => [Lightness, OppositeA, OppositeB],
        Space::Lch | Space::Oklch => [Lightness, Colorfulness, Hue],
        Space::Hsl => [Hue, Colorfulness, Lightness],
        Space::Hwb => [Hue, Whiteness, Blackness],
        // The RGB spaces, and XYZ, whose x, y and z are reds, greens and blues.
        _ => [Red, Green, Blue],
    }
}

/// One colour of an interpolation, in the interpolation's space (CSS Color 4
/// §12.4).
struct End {
    c: [f64; 3],
    alpha: f64,
    /// Missing components, then alpha: those written `none`, carried to the
    /// analogous component when converted, and a converted colour's powerless
    /// hue (an achromatic colour's).
    missing: [bool; 4],
    /// The components as written: §12 converts a colour only when it is not
    /// already in the space.
    written: bool,
}

fn endpoint(w: &Wide, space: MixSpace) -> End {
    let written = space.own() == w.space;
    let c = if written {
        w.c
    } else {
        space.from_linear_srgb(w.linear_srgb())
    };
    let mut missing = [false; 4];
    missing[3] = w.missing[3];
    if written {
        missing[..3].copy_from_slice(&w.missing[..3]);
    } else {
        let from = kinds(w.space);
        for (m, kind) in missing.iter_mut().zip(kinds(space.own())) {
            *m = (0..3).any(|i| w.missing[i] && from[i] == kind);
        }
    }
    // A written hue acts as normal even on a gray (§4.4.1); only the hue a
    // conversion gives an achromatic colour is powerless, so missing.
    if space.is_polar() && !written {
        let chroma = match space {
            MixSpace::Hsl | MixSpace::Hwb => {
                let rgb = MixSpace::Srgb.from_linear_srgb(w.linear_srgb());
                rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2])
            }
            _ => c[1],
        };
        missing[space.hue_index()] |= chroma.abs() < 1e-6;
    }
    End {
        c,
        alpha: w.alpha,
        missing,
        written,
    }
}

/// How far from a half turn a converted end's hue may land by rounding alone.
const HALF_TURN_ROUNDING: f64 = 1e-6;

/// `h2` moved by whole turns so that interpolating from `h1` goes round as
/// `method` says (CSS Color 4 §12.4). `written` says both hues are as
/// written; otherwise a difference within rounding of ±180° is taken as
/// exactly that, which `shorter` and `longer` leave as it is, so a
/// conversion's last bit does not pick the arc.
pub fn hue_arc(h1: f64, h2: f64, method: HueMethod, written: bool) -> f64 {
    let mut d = h2 - h1;
    if !written && (d.abs() - 180.0).abs() < HALF_TURN_ROUNDING {
        d = 180f64.copysign(d);
    }
    match method {
        HueMethod::Shorter if d > 180.0 => h2 - 360.0,
        HueMethod::Shorter if d < -180.0 => h2 + 360.0,
        HueMethod::Longer if d > 0.0 && d < 180.0 => h2 - 360.0,
        HueMethod::Longer if d > -180.0 && d <= 0.0 => h2 + 360.0,
        HueMethod::Increasing if d < 0.0 => h2 + 360.0,
        HueMethod::Decreasing if d > 0.0 => h2 - 360.0,
        _ => h2,
    }
}

/// The colour `t` of the way from `a` to `b`, premultiplied, as extended
/// linear sRGB and alpha. A component missing from one colour takes the
/// other's; missing from both, it stays 0 (CSS Color 4 §12.4).
pub fn mix(a: &Wide, b: &Wide, t: f64, how: Interpolation) -> ([f64; 3], f64) {
    let space = how.space;
    let (mut a, mut b) = (endpoint(a, space), endpoint(b, space));
    for i in 0..4 {
        let (va, vb) = if i < 3 {
            (&mut a.c[i], &mut b.c[i])
        } else {
            (&mut a.alpha, &mut b.alpha)
        };
        match (a.missing[i], b.missing[i]) {
            (true, false) => *va = *vb,
            (false, true) => *vb = *va,
            // Missing from both, it stays missing, which is 0, whatever a
            // conversion made of it.
            (true, true) => (*va, *vb) = (0.0, 0.0),
            (false, false) => {}
        }
    }
    let hi = space.hue_index();
    if space.is_polar() {
        b.c[hi] = hue_arc(a.c[hi], b.c[hi], how.hue, a.written && b.written);
    }
    let pre = |c: [f64; 3], alpha: f64| {
        let mut out = c;
        for (i, v) in out.iter_mut().enumerate() {
            if !(space.is_polar() && i == hi) {
                *v *= alpha;
            }
        }
        out
    };
    let (pa, pb) = (pre(a.c, a.alpha), pre(b.c, b.alpha));
    let alpha = a.alpha + (b.alpha - a.alpha) * t;
    let mut c = [0.0; 3];
    for i in 0..3 {
        let v = pa[i] + (pb[i] - pa[i]) * t;
        c[i] = if space.is_polar() && i == hi {
            v.rem_euclid(360.0)
        } else if alpha > 0.0 {
            v / alpha
        } else {
            v
        };
    }
    (space.to_linear_srgb(c), alpha)
}
