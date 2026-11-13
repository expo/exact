use super::*;

/// A legacy colour's 8-bit value.
fn bytes(s: &str) -> Option<Rgba8> {
    match parse(s) {
        Some(Parsed::Legacy(c, _)) => Some(c),
        _ => None,
    }
}

fn wide(s: &str) -> Wide {
    match parse(s) {
        Some(Parsed::Wide(w)) => w,
        other => panic!("{s}: {other:?}"),
    }
}
fn close(a: [f64; 3], b: [f64; 3], tolerance: f64) -> bool {
    a.iter().zip(b).all(|(x, y)| (x - y).abs() <= tolerance)
}

/// RGB → XYZ from CIE xy primaries and white.
fn to_xyz(p: [(f64, f64); 3], w: (f64, f64)) -> M3 {
    let col = |(x, y): (f64, f64)| [x / y, 1.0, (1.0 - x - y) / y];
    let m: M3 = [0, 1, 2].map(|r| [col(p[0])[r], col(p[1])[r], col(p[2])[r]]);
    let det = |m: &M3| {
        m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
            - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
            + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0])
    };
    let wv = col(w);
    // Cramer's rule for m · s = white.
    let s = [0, 1, 2].map(|k| {
        let mut mk = m;
        for r in 0..3 {
            mk[r][k] = wv[r];
        }
        det(&mk) / det(&m)
    });
    m.map(|r| [r[0] * s[0], r[1] * s[1], r[2] * s[2]])
}

#[test]
fn the_matrices_are_the_standards_primaries() {
    let d65 = (0.3127, 0.3290);
    let d50 = (0.3457, 0.3585);
    for (m, p, w) in [
        (
            P3_TO_XYZ,
            [(0.680, 0.320), (0.265, 0.690), (0.150, 0.060)],
            d65,
        ),
        (A98_TO_XYZ, [(0.64, 0.33), (0.21, 0.71), (0.15, 0.06)], d65),
        (
            REC2020_TO_XYZ,
            [(0.708, 0.292), (0.170, 0.797), (0.131, 0.046)],
            d65,
        ),
        (
            PROPHOTO_TO_XYZ_D50,
            [
                (0.734699, 0.265301),
                (0.159597, 0.840403),
                (0.036598, 0.000105),
            ],
            d50,
        ),
    ] {
        let want = to_xyz(p, w);
        for r in 0..3 {
            assert!(close(m[r], want[r], 2e-4), "{:?} vs {:?}", m[r], want[r]);
        }
    }
}

#[test]
fn every_form_parses_into_its_own_space_and_serialises() {
    assert_eq!(
        parse("red"),
        Some(Parsed::Legacy(
            Rgba8 {
                r: 255,
                g: 0,
                b: 0,
                a: 1.0
            },
            Wide::new(Space::Srgb, [1.0, 0.0, 0.0], 1.0)
        ))
    );
    assert_eq!(
        parse("hsl(120 100% 50%)"),
        Some(Parsed::Legacy(
            Rgba8 {
                r: 0,
                g: 255,
                b: 0,
                a: 1.0
            },
            Wide::new(Space::Hsl, [120.0, 100.0, 50.0], 1.0)
        ))
    );
    assert_eq!(parse("currentColor"), Some(Parsed::Current));
    for (text, space, c) in [
        ("color(display-p3 1 0 0)", Space::DisplayP3, [1.0, 0.0, 0.0]),
        (
            "color(display-p3-linear 0.5 0.5 0.5)",
            Space::DisplayP3Linear,
            [0.5; 3],
        ),
        ("color(a98-rgb 0 1 0)", Space::A98Rgb, [0.0, 1.0, 0.0]),
        (
            "color(prophoto-rgb 50% 0 0)",
            Space::ProphotoRgb,
            [0.5, 0.0, 0.0],
        ),
        ("color(rec2020 0 0 1)", Space::Rec2020, [0.0, 0.0, 1.0]),
        ("color(xyz 0.95 1 1.09)", Space::XyzD65, [0.95, 1.0, 1.09]),
        (
            "color(xyz-d50 0.96 1 0.82)",
            Space::XyzD50,
            [0.96, 1.0, 0.82],
        ),
        (
            "color(rec2100-pq 0.75 0.75 0.75)",
            Space::Rec2100Pq,
            [0.75; 3],
        ),
        ("lab(50% 40 59.5)", Space::Lab, [50.0, 40.0, 59.5]),
        ("lch(50 30 400deg)", Space::Lch, [50.0, 30.0, 40.0]),
        ("oklab(0.5 -0.1 0.1)", Space::Oklab, [0.5, -0.1, 0.1]),
        ("oklch(70% 0.1 0.5turn)", Space::Oklch, [0.7, 0.1, 180.0]),
    ] {
        let w = wide(text);
        assert_eq!(w.space, space, "{text}");
        assert!(close(w.c, c, 1e-9), "{text}: {:?}", w.c);
    }
    for bad in [
        "color(display-p3 1, 0, 0)",
        "color(adobe 1 0 0)",
        "lab(50, 0, 0)",
        "oklch(0.5 0.1 10%)",
        "color(srgb 1 0)",
    ] {
        assert_eq!(parse(bad), None, "{bad}");
    }
    assert_eq!(
        wide("color(display-p3 100% 0 0)").css(),
        "color(display-p3 1 0 0)"
    );
    assert_eq!(
        wide("oklch(0.7 0.1 200 / 50%)").css(),
        "oklch(0.7 0.1 200 / 0.5)"
    );
    assert_eq!(wide("lab(50% 40 59.5)").css(), "lab(50 40 59.5)");
    assert_eq!(wide("color(xyz 0 0 0)").css(), "color(xyz-d65 0 0 0)");
    // Components are written as JavaScript's `String(n)` writes them.
    assert_eq!(
        wide("color(xyz 1e-7 0.5 1.5e21)").css(),
        "color(xyz-d65 1e-7 0.5 1.5e+21)"
    );
}

#[test]
fn a_form_css_refuses_is_not_a_colour() {
    // A number is CSS Syntax 3's (§4.3.13): a `.` is followed by a digit,
    // an `e` by an optional sign and digits.
    for (word, value) in [
        ("+.5", Some(0.5)),
        ("-.5e-2", Some(-0.005)),
        ("1e+2", Some(100.0)),
        ("1E2", Some(100.0)),
        ("5.0", Some(5.0)),
        ("1e.5", None),
        ("1.5e", None),
        (".e1", None),
        ("--1", None),
        ("1.2.3", None),
        (".", None),
        ("e1", None),
        ("5.e1", None),
        ("5.", None),
        ("1e", None),
        ("+", None),
        ("1-2", None),
    ] {
        assert_eq!(number(word), value, "{word}");
    }
    let mut refused = Vec::new();
    for name in [
        "rgb", "rgba", "hsl", "hsla", "hwb", "lab", "lch", "oklab", "oklch",
    ] {
        let first = if name.starts_with("rgb") {
            "255"
        } else {
            "50%"
        };
        let hue = matches!(name, "hsl" | "hsla" | "hwb");
        let (a, b, c) = if hue {
            ("120", "50%", "50%")
        } else {
            (first, "0", "0")
        };
        // A function token has no space before its `(` (§4.3.4).
        refused.push(format!("{name} ({a} {b} {c})"));
        // The space-separated syntax puts a `/` before the alpha (CSS Color 4 §4.1.1).
        refused.push(format!("{name}({a} {b} {c} 0.5)"));
        refused.push(format!("{name}({a} {b} {c} 50%)"));
        // A trailing dot is no number, in any component or the alpha.
        refused.push(format!("{name}(5. {b} {c})"));
        refused.push(format!("{name}({a} {b} {c} / .5.)"));
        // The comma-separated syntax takes no `none` (§4.1.2).
        refused.push(format!("{name}(none, {b}, {c})"));
        refused.push(format!("{name}({a}, {b}, {c}, none)"));
        // The same forms written correctly read.
        assert!(
            parse(&format!("{name}({a} {b} {c} / 0.5)")).is_some(),
            "{name}"
        );
        assert!(
            parse(&format!("{name}(none {b} {c} / none)")).is_some(),
            "{name}"
        );
        assert!(
            parse(&format!("{name}(+{a} {b} {c} / -.5e-2)")).is_some(),
            "{name}"
        );
    }
    refused.extend(
        [
            "color (srgb 1 0 0)",
            "color(srgb 1 0 0 0.5)",
            "color(srgb 1. 0 0)",
            "color(srgb 1, 0, 0)",
            "color(--rec709 1. 0 0)",
            "hsl(120. 100% 50%)",
            "hsl(120.deg 100% 50%)",
            "oklch(0.7 0.1 200.)",
            "rgb(1.e2 0 0)",
            "rgb(1e 0 0)",
            // The comma syntax: all numbers or all percentages (CSS Color 4 §5.1).
            "rgb(100%, 0, 0)",
            "rgb(0, 100%, 0)",
            "rgba(255, 0, 0%, 0.5)",
            // hsl()'s takes percentages after its hue (§7); hwb() has none (§8).
            "hsl(120, 100, 50)",
            "hwb(120, 0%, 0%)",
        ]
        .map(String::from),
    );
    for bad in &refused {
        assert_eq!(parse(bad), None, "{bad}");
        if !bad.starts_with("color(--") {
            assert!(refusal(bad).is_some(), "{bad} has no reason");
        }
    }
    assert_eq!(parse_profiled("color(--rec709 1. 0 0)"), None);
    assert_eq!(parse("rgb(--1 0 0)"), None);
    let legacy = |r, g, b, a| Some(Rgba8 { r, g, b, a });
    assert_eq!(bytes("rgb(none 0 0 / none)"), legacy(0, 0, 0, 0.0));
    assert_eq!(bytes("RGB(.5e1 +0 -0)"), legacy(5, 0, 0, 1.0));
    assert_eq!(bytes(" rgb(1e2 0 0) "), legacy(100, 0, 0, 1.0));
    assert_eq!(bytes("rgb( 255 , 0 , 0 , 50% )"), legacy(255, 0, 0, 0.5));
    assert_eq!(refusal("rgb(255 0 0 / 0.5)"), None);
    assert_eq!(refusal("not a colour"), None);
    // A rule is named only when it is certainly the one broken; any other
    // refusal leaves the caller's general description.
    let rule = |text: &str| refusal(text).map(|r| r.split(':').next().unwrap());
    const SPACE: &str = "a function's `(` follows its name with no space between";
    const NUMBER: &str = "a number's `.` is followed by a digit, and its exponent's `e` by digits";
    const SLASH: &str = "the space-separated syntax puts a `/` before the alpha";
    const COMMAS: &str = "only `rgb()` and `hsl()` take the comma-separated syntax";
    const NONE: &str = "the comma-separated syntax takes no `none`; the space-separated one does";
    const MIXED: &str =
        "the comma-separated syntax takes three numbers or three percentages, not both";
    const HSL: &str =
        "`hsl()`'s comma-separated syntax takes its saturation and lightness as percentages";
    // Each named rule is the colour's only fault: its repair, beside it, reads.
    for (text, want) in [
        ("rgb (255 0 0)", Some((SPACE, "rgb(255 0 0)"))),
        ("rgb(5. 0 0)", Some((NUMBER, "rgb(5 0 0)"))),
        ("rgb(1e 0 0)", Some((NUMBER, "rgb(1 0 0)"))),
        ("rgb(1.e2 0 0)", Some((NUMBER, "rgb(1e2 0 0)"))),
        ("rgb(1e.5 0 0)", Some((NUMBER, "rgb(1e5 0 0)"))),
        ("rgb(1.2.3 0 0)", Some((NUMBER, "rgb(1.2 0 0)"))),
        (
            "hsl(120.deg 100% 50%)",
            Some((NUMBER, "hsl(120deg 100% 50%)")),
        ),
        ("rgb(255 0 0 0.5)", Some((SLASH, "rgb(255 0 0 / 0.5)"))),
        ("rgb(255 0 0 none)", Some((SLASH, "rgb(255 0 0 / none)"))),
        (
            "color(srgb 1 0 0 50%)",
            Some((SLASH, "color(srgb 1 0 0 / 50%)")),
        ),
        ("lab(50, 0, 0)", Some((COMMAS, "lab(50 0 0)"))),
        ("hsl(none, 100%, 50%)", Some((NONE, "hsl(none 100% 50%)"))),
        ("rgb(0, 0, 0, none)", Some((NONE, "rgb(0 0 0 / none)"))),
        ("rgb(100%, 0, 0)", Some((MIXED, "rgb(255, 0, 0)"))),
        ("hsl(120, 100, 50)", Some((HSL, "hsl(120, 100%, 50%)"))),
        (
            "hsl(120deg, 100, 50)",
            Some((HSL, "hsl(120deg, 100%, 50%)")),
        ),
        // A unit, not the number, is the fault.
        ("hsl(120px 100% 50%)", None),
        ("rgb(10px, 0, 0)", None),
        ("rgb(1e2px, 0, 0)", None),
        ("rgb(10px, 0%, 0%)", None),
        ("hsl(120, 10px, 50%)", None),
        // `1-2` is two numbers; `1e2e3` a number with a unit `e3`; `1e309`
        // a number CSS's grammar takes, too large for a double.
        ("rgb(1-2 0 0)", None),
        ("rgb(1e2e3 0 0)", None),
        ("rgb(1e309 0 0)", None),
        ("rgb(1e999 0 0)", None),
        // A `/` mends none of these.
        ("rgb(255 0 0 1 / 0.5)", None),
        ("rgb(255 0 0 90deg)", None),
        ("rgb(255 0 0 red)", None),
        ("rgb(10px 0 0 0.5)", None),
        ("rgb(red 0 0 0.5)", None),
        ("hsl(120px 100% 50% 0.5)", None),
        ("rgb(5. 0 0 0.5)", None),
        // A profiled space takes up to four components.
        ("color(--rec709 1 0 0 0.5 0.5)", None),
    ] {
        assert_eq!(rule(text), want.map(|(r, _)| r), "{text}");
        if let Some((_, repaired)) = want {
            assert!(parse(repaired).is_some(), "{text}: {repaired}");
        }
    }
}

#[test]
fn the_arithmetic_is_css_colour_4s() {
    // CSS Color 4's sample code: P3 red in linear sRGB.
    assert!(close(
        wide("color(display-p3 1 0 0)").linear_srgb(),
        [1.2249, -0.0421, -0.0196],
        2e-4
    ));
    // sRGB white in every space comes back as white.
    for text in [
        "color(srgb 1 1 1)",
        "color(display-p3 1 1 1)",
        "color(a98-rgb 1 1 1)",
        "color(rec2020 1 1 1)",
        "color(prophoto-rgb 1 1 1)",
        "lab(100 0 0)",
        "oklab(1 0 0)",
        "oklch(1 0 0)",
    ] {
        assert!(
            close(wide(text).linear_srgb(), [1.0; 3], 2e-3),
            "{text}: {:?}",
            wide(text).linear_srgb()
        );
    }
    // PQ's 203 cd/m² is SDR white.
    let pq203 = 0.58069;
    assert!(close(
        wide(&format!("color(rec2100-pq {pq203} {pq203} {pq203})")).linear_srgb(),
        [1.0; 3],
        2e-3
    ));
    assert!((pq_nits(0.75) - 983.0).abs() < 2.0);
    assert!(close(
        wide("color(rec2100-hlg 0.75 0.75 0.75)").linear_srgb(),
        [1.0; 3],
        1e-6
    ));
    assert!(close(
        wide("color(rec2100-linear 4 4 4)").linear_srgb(),
        [4.0; 3],
        1e-9
    ));
}

#[test]
fn srgb_containment_and_the_clipped_fallback() {
    assert!(wide("color(display-p3 0.5 0.5 0.5)").in_srgb());
    assert!(!wide("color(display-p3 1 0 0)").in_srgb());
    assert!(!wide("color(rec2100-linear 2 2 2)").in_srgb());
    assert!(wide("oklch(0.7 0.05 200)").in_srgb());
    assert_eq!(
        wide("color(display-p3 1 0 0)").srgb8(),
        Rgba8 {
            r: 255,
            g: 0,
            b: 0,
            a: 1.0
        }
    );
    assert_eq!(
        wide("color(srgb 1 0 0.5 / 0.5)").srgb8(),
        Rgba8 {
            r: 255,
            g: 0,
            b: 128,
            a: 128.0 / 255.0
        }
    );
}

fn srgb(r: f64, g: f64, b: f64, alpha: f64) -> Wide {
    Wide {
        space: Space::Srgb,
        c: [r, g, b],
        alpha,
        missing: [false; 4],
    }
}

#[test]
fn folding_keeps_the_same_bits_on_every_host() {
    // glibc's pow gives 0x3f76_f5ad_db50_c915 for this sRGB channel.
    assert_eq!(
        srgb_to_linear(17.0 / 255.0).to_bits(),
        0x3f76_f5ad_db50_c914
    );

    // Tamber's mix.tamber: color-mix(in oklab, #1083fe 88%, #0b0f14).
    // Its a component differed by one ulp between the Linux run and the golden.
    let lab = |rgb: [f64; 3]| {
        MixSpace::Oklab.from_linear_srgb(
            Wide {
                space: Space::Srgb,
                c: rgb.map(|v| v / 255.0),
                alpha: 1.0,
                missing: [false; 4],
            }
            .linear_srgb(),
        )
    };
    let (a, b) = (lab([16.0, 131.0, 254.0]), lab([11.0, 15.0, 20.0]));
    let mixed = [0, 1, 2].map(|i| a[i] + (b[i] - a[i]) * 0.12);
    assert_eq!(
        mixed.map(f64::to_bits),
        [
            0x3fe2_2732_0e61_2567,
            0xbfa6_c513_a3d4_fb4c,
            0xbfc6_92aa_2fe5_3c8c
        ]
    );
}

#[test]
fn every_interpolation_space_round_trips() {
    use crate::mix::MixSpace::*;
    let samples = [
        [0.2, 0.5, 0.8],
        [1.0, 0.0, 0.0],
        [0.05, 0.9, 0.3],
        [1.2, -0.04, -0.02],
    ];
    for space in [
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
    ] {
        for v in samples {
            if matches!(space, Hsl | Hwb) && v.iter().any(|c| *c < 0.0 || *c > 1.0) {
                continue; // HSL and HWB are defined over sRGB's gamut
            }
            let back = space.to_linear_srgb(space.from_linear_srgb(v));
            assert!(close(back, v, 1e-6), "{space:?}: {v:?} -> {back:?}");
        }
    }
}

#[test]
fn interpolation_is_premultiplied_and_in_its_space() {
    let red = srgb(1.0, 0.0, 0.0, 1.0);
    let blue = srgb(0.0, 0.0, 1.0, 1.0);
    let legacy = Interpolation {
        space: crate::mix::MixSpace::Srgb,
        hue: HueMethod::Shorter,
    };
    // In gamma-encoded sRGB the midpoint is (0.5, 0, 0.5) encoded.
    let (mid, a) = mix(&red, &blue, 0.5, legacy);
    assert!(close(mid.map(linear_to_srgb), [0.5, 0.0, 0.5], 1e-9) && a == 1.0);
    // Premultiplied: red towards transparent black stays red, half as opaque.
    let (half, a) = mix(&red, &srgb(0.0, 0.0, 0.0, 0.0), 0.5, legacy);
    assert!(
        close(half.map(linear_to_srgb), [1.0, 0.0, 0.0], 1e-9),
        "{half:?}"
    );
    assert!((a - 0.5).abs() < 1e-12);
    // In oklab the midpoint is lighter than sRGB's dark purple.
    let (ok, _) = mix(&red, &blue, 0.5, Interpolation::OKLAB);
    assert!(
        crate::mix::MixSpace::Oklab.from_linear_srgb(ok)[0]
            > crate::mix::MixSpace::Oklab.from_linear_srgb(mid)[0]
    );
}

#[test]
fn hue_methods_go_round_as_css_says() {
    let how = |hue| Interpolation {
        space: crate::mix::MixSpace::Oklch,
        hue,
    };
    let at = |h: f64| Wide {
        space: Space::Oklch,
        c: [0.7, 0.1, h],
        alpha: 1.0,
        missing: [false; 4],
    };
    let hue = |v: [f64; 3]| crate::mix::MixSpace::Oklch.from_linear_srgb(v)[2];
    let (shorter, _) = mix(&at(10.0), &at(350.0), 0.5, how(HueMethod::Shorter));
    assert!(
        hue(shorter) < 1.0 || hue(shorter) > 359.0,
        "through 0: {}",
        hue(shorter)
    );
    let (longer, _) = mix(&at(10.0), &at(350.0), 0.5, how(HueMethod::Longer));
    assert!(
        (hue(longer) - 180.0).abs() < 0.5,
        "through 180: {}",
        hue(longer)
    );
    let (increasing, _) = mix(&at(350.0), &at(10.0), 0.5, how(HueMethod::Increasing));
    assert!(hue(increasing) < 1.0 || hue(increasing) > 359.0);
    let (decreasing, _) = mix(&at(350.0), &at(10.0), 0.5, how(HueMethod::Decreasing));
    assert!((hue(decreasing) - 180.0).abs() < 0.5);
    // A converted gray has no hue: white to blue keeps blue's hue all the way.
    let blue = Wide::new(Space::Oklch, [0.45, 0.3, 264.0], 1.0);
    let white = srgb(1.0, 1.0, 1.0, 1.0);
    let (m, _) = mix(&white, &blue, 0.5, how(HueMethod::Shorter));
    assert!((hue(m) - 264.0).abs() < 0.5, "{}", hue(m));
}

/// Hues exactly 180° apart take no adjustment under `shorter` or `longer`
/// (CSS Color 4 §12.4), so the hue rises from the first to the second.
#[test]
fn opposite_hues_go_round_from_the_first() {
    let in_space = |space, hue| Interpolation { space, hue };
    let lch = |h: f64| Wide {
        space: Space::Lch,
        c: [70.0, 30.0, h],
        alpha: 1.0,
        missing: [false; 4],
    };
    let lch_hue = |v: [f64; 3]| crate::mix::MixSpace::Lch.from_linear_srgb(v)[2];
    for method in [HueMethod::Shorter, HueMethod::Longer] {
        let how = in_space(crate::mix::MixSpace::Lch, method);
        for (a, b) in [(0.0, 180.0), (180.0, 0.0), (90.0, 270.0), (270.0, 90.0)] {
            let (m, _) = mix(&lch(a), &lch(b), 0.5, how);
            let want = (a + b) / 2.0;
            assert!(
                (lch_hue(m) - want).abs() < 0.01,
                "lch {a} to {b}, {method:?}: {}",
                lch_hue(m)
            );
        }
        // Legacy colours mix in hsl after a conversion, whose rounding must
        // not pick the arc: cyan and red, and #fff000 and #000fff.
        let how = in_space(crate::mix::MixSpace::Hsl, method);
        let (m, _) = mix(
            &srgb(0.0, 1.0, 1.0, 1.0),
            &srgb(1.0, 0.0, 0.0, 1.0),
            0.5,
            how,
        );
        assert!(
            close(m.map(linear_to_srgb), [0.5, 1.0, 0.0], 1e-9),
            "cyan to red, {method:?}: {:?}",
            m.map(linear_to_srgb)
        );
        let (m, _) = mix(
            &srgb(1.0, 240.0 / 255.0, 0.0, 1.0),
            &srgb(0.0, 15.0 / 255.0, 1.0, 1.0),
            0.5,
            how,
        );
        let hsl_hue = crate::mix::MixSpace::Hsl.from_linear_srgb(m)[0];
        assert!(
            (hsl_hue - 146.470588).abs() < 0.001,
            "#fff000 to #000fff, {method:?}: {hsl_hue}"
        );
    }
}

#[test]
fn interpolation_methods_parse_and_serialise() {
    let (how, n) = Interpolation::parse(&["in", "oklch", "longer", "hue", "45deg"]).unwrap();
    assert_eq!((how.css().as_str(), n), ("in oklch longer hue", 4));
    let (how, n) = Interpolation::parse(&["in", "display-p3"]).unwrap();
    assert_eq!((how.css().as_str(), n), ("in display-p3", 2));
    assert_eq!(
        Interpolation::parse(&["in", "srgb", "longer", "hue"]).map(|(_, n)| n),
        Some(2),
        "no hue method in a rectangular space"
    );
    assert!(Interpolation::parse(&["in", "cmyk"]).is_none());
    assert!(Interpolation::parse(&["to", "right"]).is_none());
}

#[test]
fn display_p3_bytes_are_css_conversion_clipped() {
    let p3 = |s: &str| match parse(s) {
        Some(Parsed::Wide(w)) => {
            let c = w.display_p3_8();
            [c.r, c.g, c.b]
        }
        Some(Parsed::Legacy(c, _)) => {
            let c = srgb8_to_p3(c);
            [c.r, c.g, c.b]
        }
        _ => panic!("{s}"),
    };
    // sRGB red is inside P3: color(display-p3 0.9175 0.2003 0.1386).
    assert_eq!(p3("red"), [234, 51, 35]);
    assert_eq!(p3("white"), [255, 255, 255]);
    assert_eq!(p3("color(display-p3 1 0 0)"), [255, 0, 0]);
    assert_eq!(p3("color(display-p3 0.2 0.4 0.6)"), [51, 102, 153]);
    // Outside P3 clips.
    assert_eq!(p3("color(rec2020 0 1 0)")[1], 255);
    // And back: P3 red is outside sRGB, so it clips to sRGB red.
    let back = p3_8_to_srgb(Rgba8 {
        r: 255,
        g: 0,
        b: 0,
        a: 1.0,
    });
    assert_eq!([back.r, back.g, back.b], [255, 0, 0]);
    // Greys are the same bytes in both (one white, one transfer).
    let grey = p3_8_to_srgb(srgb8_to_p3(Rgba8 {
        r: 99,
        g: 99,
        b: 99,
        a: 0.5,
    }));
    assert_eq!(
        [grey.r, grey.g, grey.b, (grey.a * 255.0).round() as u8],
        [99, 99, 99, 128]
    );
}

#[test]
fn equal_hues_take_a_full_turn_for_longer() {
    let c = wide("oklch(0.7 0.1 30)");
    let how = Interpolation {
        space: MixSpace::Oklch,
        hue: HueMethod::Longer,
    };
    let (mid, _) = mix(&c, &c, 0.5, how);
    assert!(close(mid, wide("oklch(0.7 0.1 210)").linear_srgb(), 1e-6));
}

#[test]
fn rec2100_components_round_trip_through_linear_srgb() {
    for space in [Space::Rec2100Linear, Space::Rec2100Pq, Space::Rec2100Hlg] {
        for c in [
            [0.2, 0.5, 0.7],
            [0.9, 0.1, 0.4],
            [0.58, 0.58, 0.58],
            [-0.05, 0.3, 0.3],
        ] {
            if space == Space::Rec2100Pq && c[0] < 0.0 {
                continue; // PQ's decode has no negative signal
            }
            let w = Wide {
                space,
                c,
                alpha: 1.0,
                missing: [false; 4],
            };
            let back = crate::rec2100_from_linear_srgb(space, w.linear_srgb()).unwrap();
            for i in 0..3 {
                assert!((back[i] - c[i]).abs() < 1e-9, "{space:?} {c:?} -> {back:?}");
            }
        }
    }
    assert_eq!(crate::rec2100_from_linear_srgb(Space::Srgb, [0.0; 3]), None);
}

/// A colour as `mix` takes it: as written.
fn mixable(s: &str) -> Wide {
    match parse(s) {
        Some(Parsed::Wide(w) | Parsed::Legacy(_, w)) => w,
        other => panic!("{s}: {other:?}"),
    }
}

/// `color-mix(in <space>, a, b)` against Chrome 156's computed value, in the
/// mix space's components (sRGB's for hsl and hwb, which Chrome serialises
/// as `color(srgb …)`), each within `tolerance` of the larger of 1 and its
/// size.
fn chrome_mixes(cases: &[(&str, &str, MixSpace, [f64; 3], f64)], tolerance: f64) {
    for &(a, b, space, want, alpha) in cases {
        let how = Interpolation {
            space,
            hue: HueMethod::Shorter,
        };
        let (mid, got_alpha) = mix(&mixable(a), &mixable(b), 0.5, how);
        let shown = match space {
            MixSpace::Hsl | MixSpace::Hwb => MixSpace::Srgb,
            _ => space,
        };
        let mut got = shown.from_linear_srgb(mid);
        if matches!(space, MixSpace::Lch | MixSpace::Oklch) && got[1] < 1e-4 {
            got[2] = want[2]; // no hue to compare on a gray
        }
        let near = got
            .iter()
            .zip(want)
            .all(|(g, w)| (g - w).abs() <= tolerance * w.abs().max(1.0));
        assert!(
            near && (got_alpha - alpha).abs() < 1e-9,
            "color-mix(in {space:?}, {a}, {b}): {got:?} / {got_alpha}, Chrome {want:?} / {alpha}"
        );
    }
}

/// A hue written on a gray acts as normal (CSS Color 4 §4.4.1); only the hue
/// a conversion gives an achromatic colour is powerless. Chrome: `color-mix(in
/// oklch, oklch(0.7 0 0), oklch(0.7 0.2 120))` is `oklch(0.7 0.1 60)`, where
/// white, converted, keeps the other's hue.
#[test]
fn a_written_hue_on_a_gray_is_interpolated() {
    use MixSpace::{Lch, Oklch};
    #[rustfmt::skip]
    let cases = [
        ("oklch(0.7 0 0)", "oklch(0.7 0.2 120)", Oklch, [0.7, 0.1, 60.0], 1.0),
        ("oklch(1 0 0)", "oklch(0.45 0.3 264)", Oklch, [0.725, 0.15, 312.0], 1.0),
        ("lch(50 0 0)", "lch(50 50 180)", Lch, [50.0, 25.0, 90.0], 1.0),
        ("oklch(0.7 0.0000001 30)", "oklch(0.7 0.2 120)", Oklch, [0.7, 0.1, 75.0], 1.0),
        ("white", "oklch(0.7 0.2 120)", Oklch, [0.849997, 0.100025, 120.0], 1.0),
        ("rgb(128 128 128)", "oklch(0.7 0.2 120)", Oklch, [0.649934, 0.100015, 120.0], 1.0),
        ("lab(50 0 0)", "oklch(0.7 0.2 120)", Oklch, [0.634482, 0.100007, 120.0], 1.0),
        // Written in oklch, but converted to lch: powerless there.
        ("oklch(0.7 0 30)", "lch(50 30 120)", Lch, [57.6, 15.0033, 120.0], 1.0),
    ];
    // Chrome's conversions differ from these in the fourth significant digit.
    chrome_mixes(&cases, 5e-4);
    let how = Interpolation {
        space: Oklch,
        hue: HueMethod::Longer,
    };
    let (mid, _) = mix(
        &wide("oklch(0.7 0 0)"),
        &wide("oklch(0.7 0.2 120)"),
        0.5,
        how,
    );
    let hue = Oklch.from_linear_srgb(mid)[2];
    assert!((hue - 240.0).abs() < 0.01, "longer: {hue}, Chrome 240");
}

/// `none` is a missing component (CSS Color 4 §4.4): it acts as 0, but an
/// interpolation takes the other colour's, carried to the analogous
/// component of the mix space when the colour is converted (§12.4), and so
/// is a missing alpha. Chrome 156's values.
#[test]
fn a_missing_component_takes_the_other_colours() {
    use MixSpace::*;
    let w = wide("oklch(0.7 0 none)");
    assert_eq!(
        (w.c, w.missing),
        ([0.7, 0.0, 0.0], [false, false, true, false])
    );
    assert_eq!(w.css(), "oklch(0.7 0 none)");
    assert_eq!(
        wide("oklch(0.7 0.1 120 / none)").css(),
        "oklch(0.7 0.1 120 / none)"
    );
    assert_eq!(wide("lab(none 20 20)").css(), "lab(none 20 20)");
    assert_eq!(
        wide("color(srgb none 0.5 0.5)").css(),
        "color(srgb none 0.5 0.5)"
    );
    #[rustfmt::skip]
    let cases = [
        ("oklch(0.7 0 none)", "oklch(0.7 0.2 120)", Oklch, [0.7, 0.1, 120.0], 1.0),
        ("oklch(0.7 0.1 none)", "oklch(0.7 0.2 120)", Oklch, [0.7, 0.15, 120.0], 1.0),
        ("oklch(none 0.1 30)", "oklch(0.5 0.2 120)", Oklch, [0.5, 0.15, 75.0], 1.0),
        ("oklch(0.7 none 30)", "oklch(0.5 0.2 120)", Oklch, [0.6, 0.2, 75.0], 1.0),
        ("oklch(none 0.1 30)", "oklab(0.5 0.1 0.1)", Oklab, [0.5, 0.0933013, 0.075], 1.0),
        // A hue has no analogue in oklab: converted as 0.
        ("oklch(0.6 0.1 none)", "oklab(0.5 0.1 0.1)", Oklab, [0.55, 0.1, 0.05], 1.0),
        ("oklch(0.6 0.1 none)", "lch(50 30 120)", Lch, [51.1711, 31.4999, 120.0], 1.0),
        ("oklch(none 0.1 30)", "lch(50 30 120)", Lch, [50.0, 15.4552, 103.987], 1.0),
        ("oklch(0.6 none 30)", "lch(50 30 120)", Lch, [51.8, 30.0, 120.0], 1.0),
        ("lab(none 20 20)", "oklch(0.5 0.1 120)", Oklch, [0.5, 0.286536, 66.1934], 1.0),
        ("lab(50 none 20)", "oklch(0.5 0.1 120)", Oklch, [0.533497, 0.0749323, 106.68], 1.0),
        ("lab(50 none 20)", "oklab(0.5 0.1 0.1)", Oklab, [0.533497, 0.1, 0.0748895], 1.0),
        ("oklab(0.5 none 0.1)", "lab(50 20 20)", Lab, [46.1105, 20.0, 33.6962], 1.0),
        ("color(srgb none 0.5 0.5)", "color(srgb 1 0 0)", Srgb, [1.0, 0.25, 0.25], 1.0),
        ("color(srgb none 0.5 0.5)", "color(srgb 1 0 0)", SrgbLinear, [1.0, 0.107021, 0.107021], 1.0),
        ("color(srgb none 0.5 0.5)", "color(xyz 0.5 0.2 0.1)", XyzD65, [0.5, 0.184259, 0.164467], 1.0),
        ("color(xyz none 0.2 0.1)", "color(srgb 1 0 0)", XyzD65, [0.412426, 0.206324, 0.0596587], 1.0),
        ("color(xyz none 0.2 0.1)", "color(srgb 1 0 0)", Srgb, [1.0, 0.324738, 0.141292], 1.0),
        ("color(srgb none 0.5 0.5)", "color(display-p3 1 0 0)", DisplayP3, [1.0, 0.246113, 0.247999], 1.0),
        ("oklch(0.6 0.1 none)", "hsl(120 50% 50%)", Hsl, [0.326454, 0.719118, 0.326454], 1.0),
        ("oklch(0.6 0.1 none)", "hwb(120 20% 20%)", Hwb, [0.299436, 0.746137, 0.299436], 1.0),
        ("oklch(none 0.1 30)", "hwb(120 20% 20%)", Hwb, [0.295405, 0.401894, 0.0958323], 1.0),
        ("oklch(0.7 0.1 120 / none)", "oklch(0.5 0.1 120 / 0.5)", Oklch, [0.6, 0.1, 120.0], 0.5),
        ("oklch(0.7 0.1 120 / none)", "oklch(0.5 0.1 120)", Oklch, [0.6, 0.1, 120.0], 1.0),
    ];
    chrome_mixes(&cases, 5e-4);
}

/// `hwb()` clamps no whiteness or blackness above 100% (CSS Color 4 §8),
/// scaling a sum over 100% down; `hsl()` clamps. Chrome paints
/// `hwb(0 200% 100%)` as rgb(170, 170, 170), 200 / 300.
#[test]
fn hwb_whiteness_and_blackness_are_not_clamped_above_100() {
    for (text, v) in [
        ("hwb(0 200% 100%)", [170; 3]),
        ("hwb(0 50% 150%)", [64; 3]),
        ("hwb(200 120% 30%)", [204; 3]),
        ("hwb(200 -20% 30%)", [0, 119, 179]),
        ("hwb(none 20% 30%)", [179, 51, 51]),
        ("hsl(200 150% 50%)", [0, 170, 255]),
        ("hsl(200 50% 150%)", [255; 3]),
    ] {
        let [r, g, b] = v;
        assert_eq!(bytes(text), Some(Rgba8 { r, g, b, a: 1.0 }), "{text}");
    }
}

/// `rgb()`, `hsl()` and `hwb()` mix as written (CSS Color 4 §12): unrounded,
/// in their own components, a gray's written hue acting as normal and `none`
/// missing, as Chrome 156 mixes them.
#[test]
fn legacy_forms_mix_as_written() {
    use MixSpace::*;
    let w = mixable("hsl(none 50% 50% / none)");
    assert_eq!(
        (w.space, w.missing),
        (Space::Hsl, [true, false, false, true])
    );
    assert_eq!(w.css(), "hsl(none 50% 50% / none)");
    #[rustfmt::skip]
    let cases = [
        ("hsl(30 0% 50%)", "hsl(120 50% 50%)", Hsl, [0.5625, 0.625, 0.375], 1.0),
        ("hwb(30 60% 40%)", "hwb(120 20% 20%)", Hwb, [0.625, 0.7, 0.4], 1.0),
        ("hsl(none 50% 50%)", "hsl(120 50% 50%)", Hsl, [0.25, 0.75, 0.25], 1.0),
        ("hsl(10 50% 50%)", "hsl(120 50% 50%)", Hsl, [0.708333, 0.75, 0.25], 1.0),
        ("hsl(30 none 50%)", "hsl(120 50% 50%)", Hsl, [0.625, 0.75, 0.25], 1.0),
        ("hsl(none 50% 50%)", "oklch(0.7 0.1 120)", Oklch, [0.626172, 0.131846, 120.0], 1.0),
        ("hsl(30 50% none)", "oklch(0.7 0.1 120)", Oklch, [0.7, 0.05, 120.0], 1.0),
        ("hsl(30 none 50%)", "oklch(0.7 0.1 120)", Oklch, [0.649089, 0.1, 120.0], 1.0),
        ("hwb(none 20% 30%)", "lch(50 30 120)", Lch, [46.0317, 45.3977, 120.0], 1.0),
        ("hwb(30 none 30%)", "hwb(120 20% 20%)", Hwb, [0.6125, 0.75, 0.2], 1.0),
        ("hwb(none 20% 30%)", "hsl(120 50% 50%)", Hsl, [0.224306, 0.725694, 0.224306], 1.0),
        ("hwb(0 200% 100%)", "hwb(0 0% 0%)", Hwb, [0.666667, 0.666667, 0.666667], 1.0),
        ("rgb(none 0 0)", "rgb(255 255 255)", Srgb, [1.0, 0.5, 0.5], 1.0),
        ("rgb(none 0 0 / none)", "rgb(255 255 255 / 0.5)", Srgb, [1.0, 0.5, 0.5], 0.5),
        ("rgb(none 0 0)", "rgb(255 255 255)", Oklab, [0.499997, 0.0000227839, 0.0000100434], 1.0),
        ("rgb(none 128 0)", "color(srgb 1 0 0)", SrgbLinear, [1.0, 0.10793, 0.0], 1.0),
        ("red", "hsl(120 50% 50%)", Hsl, [0.875, 0.875, 0.125], 1.0),
        ("rgb(12.6 0 0)", "hsl(120 50% 50%)", Hsl, [0.459118, 0.459118, 0.0655882], 1.0),
    ];
    chrome_mixes(&cases, 5e-4);
}

/// A component missing from both colours stays missing, so 0, whatever a
/// conversion made of it (CSS Color 4 §12.4). Chrome 156: `color-mix(in
/// display-p3, color(srgb none 1 0), color(display-p3 none 0 1))` is
/// `color(display-p3 none 0.492614 0.649132)`.
#[test]
fn a_component_missing_from_both_stays_missing() {
    use MixSpace::*;
    #[rustfmt::skip]
    let cases = [
        ("color(srgb none 1 0)", "color(display-p3 none 0 1)", DisplayP3, [0.0, 0.492614, 0.649132], 1.0),
        ("color(srgb none 1 0)", "color(display-p3 none 0 1)", Srgb, [0.0, 0.500073, 0.521041], 1.0),
        ("oklch(none 0.1 30)", "lab(none 20 20)", Oklch, [0.0, 0.286536, 21.1934], 1.0),
    ];
    chrome_mixes(&cases, 5e-4);
}

/// A dashed ident keeps its case, as CSS's does; the function's name and a
/// standard space's are matched in any case.
#[test]
fn a_profile_name_keeps_its_case() {
    let name = |s| parse_profiled(s).map(|p| p.0);
    assert_eq!(name("color(--Brand 1 0 0)").as_deref(), Some("--Brand"));
    assert_eq!(name("COLOR(--Brand 1 0 0)").as_deref(), Some("--Brand"));
    assert_eq!(name("color(--brand 1 0 0)").as_deref(), Some("--brand"));
    assert_eq!(name("color(--DCI-P3 1 0 0)").as_deref(), Some("--dci-p3"));
    assert_eq!(
        parse_profiled("color(--Brand 50% 0 1E0 / 50%)"),
        Some(("--Brand".to_string(), vec![0.5, 0.0, 1.0], 0.5))
    );
}
