//! Box-dependent gradient stops for both Apple presenters (LLP 1066).
#![allow(unsafe_code)]
use exact_color::{HueMethod, Interpolation, MixSpace, Space, Wide};
use exact_kernel::gradient::{BackgroundImage, Extent, Geometry, GradientKind, RadialSize};
use exact_kernel::style::{roles, Color, ColorValue};

fn tint_share(color: ColorValue) -> Option<f32> {
    let name = crate::style::native_name(color)?;
    let suffix = name.strip_prefix("@tint")?;
    if suffix.is_empty() {
        Some(1.0)
    } else {
        suffix.strip_prefix('/')?.parse().ok()
    }
}

/// A tint stop: the view's tint `c` (extended sRGB, unclipped, then alpha) at
/// `share`. 8-bit where the gradient mixes as legacy colours do and the tint
/// is within sRGB; else a wide colour, so a Display P3 tint keeps its colour.
fn tint_stop(c: [f32; 4], share: f32, legacy: bool) -> ColorValue {
    let alpha = if share < 1.0 { share } else { c[3] }.clamp(0.0, 1.0);
    let rgb = [c[0], c[1], c[2]];
    let byte = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
    let fixed = ColorValue::Fixed(Color::rgba(byte(c[0]), byte(c[1]), byte(c[2]), byte(alpha)));
    if legacy && rgb.iter().all(|v| (0.0..=1.0).contains(v)) {
        return fixed;
    }
    let wide = Wide::new(Space::Srgb, rgb.map(f64::from), f64::from(alpha));
    exact_kernel::style::wide::parse_wide(&wide.css()).unwrap_or(fixed)
}

/// Resolve one gradient into the existing presenter ramp plus its geometry.
///
/// # Safety
/// `css` is valid for `len` reads; `tint`, when non-null, for four floats:
/// extended sRGB red, green and blue, unclipped, then alpha;
/// `out`, when non-null, for `cap` writes.
pub unsafe fn resolve(
    css: *const u8,
    len: usize,
    size: [f32; 2],
    dark: bool,
    tint: *const f32,
    out: *mut u8,
    cap: usize,
) -> usize {
    if css.is_null() || !size.iter().all(|n| n.is_finite() && *n >= 0.0) {
        return 0;
    }
    // SAFETY: the caller owns this byte range.
    let bytes = unsafe { std::slice::from_raw_parts(css, len) };
    let Some(image) = std::str::from_utf8(bytes)
        .ok()
        .and_then(BackgroundImage::parse)
    else {
        return 0;
    };
    let Some(g) = image.gradient() else {
        return 0;
    };
    // No session's overrides: the batch that carried it resolved its own.
    let _roles = exact_kernel::style::roles::bind(&Default::default());
    // SAFETY: the caller supplies four readable floats.
    let tint = (!tint.is_null()).then(|| unsafe { std::ptr::read(tint.cast::<[f32; 4]>()) });
    let mut g = g.clone();
    // What the stops choose before the tint joins them (CSS Color 4 §12.1).
    let how = g.interpolation(dark);
    for stop in &mut g.stops {
        if let Some(share) = tint_share(stop.color) {
            stop.color = match tint {
                Some(c) => tint_stop(c, share, how.is_none()),
                None => ColorValue::Fixed(stop.color.resolve(dark)),
            };
        }
    }
    // An unclipped tint is a modern colour to the kernel, which would then
    // mix in Oklab; the choice stays the one made without it.
    if g.interpolation.is_none()
        && g.stops
            .iter()
            .any(|s| matches!(s.color, ColorValue::Wide(_)))
    {
        g.interpolation = Some(how.unwrap_or(Interpolation {
            space: MixSpace::Srgb,
            hue: HueMethod::Shorter,
        }));
    }
    let (mut ramp, geometry) = g.placed(size[0], size[1], dark);
    // The placement crosses resolved, below; explicit radii in the shape
    // would send the gradient back as CSS.
    if let GradientKind::Radial { size, .. } = &mut ramp.kind {
        *size = RadialSize::Extent(Extent::FarthestCorner);
    }
    let placement = match geometry {
        Geometry::Linear { start, end } => {
            format!("\"line\":[{},{},{},{}]", start.0, start.1, end.0, end.1)
        }
        Geometry::Radial { center, radii } => format!(
            "\"ellipse\":[{},{},{},{}]",
            center.0, center.1, radii.0, radii.1
        ),
        Geometry::Conic { center, from } => format!("\"arc\":[{},{},{}]", center.0, center.1, from),
    };
    let mut json = gradient_json(&ramp);
    json.pop();
    json.push(',');
    json.push_str(&placement);
    json.push('}');
    if !out.is_null() && cap >= json.len() {
        // SAFETY: the caller supplies at least `cap` writable bytes.
        unsafe { std::ptr::copy_nonoverlapping(json.as_ptr(), out, json.len()) };
    }
    json.len()
}

/// Export size-dependent gradient resolution from an app's archive.
#[macro_export]
macro_rules! gradient_exports {
    () => {
        /// Resolve a CSS gradient.
        ///
        /// # Safety
        /// Input/output buffers have their stated lengths.
        #[no_mangle]
        pub unsafe extern "C" fn exact_gradient(
            css: *const u8,
            len: usize,
            width: f32,
            height: f32,
            dark: u8,
            tint: *const f32,
            out: *mut u8,
            cap: usize,
        ) -> usize {
            // SAFETY: forwarded from the caller's buffer contract.
            unsafe {
                $crate::gradient::resolve(css, len, [width, height], dark != 0, tint, out, cap)
            }
        }
    };
}

use crate::style::num;
/// A gradient for the presenter (LLP 1066): its shape — `linear` degrees,
/// a `to <corner>` as `[right, bottom]`, or `radial` as `[circle, extent,
/// x%, xpx, y%, ypx]` (extent: closest-side, closest-corner, farthest-side,
/// farthest-corner) — and `stops` flat as `t, r, g, b, a`, positions 0–1.
/// A gradient whose stops need the box, or a radial one with explicit radii,
/// crosses as its CSS instead, and the presenter has the kernel place it.
/// The box, so the placement, is the presenter's. Stops are already expanded
/// to mix as CSS's premultiplied ones do (Core Graphics and Core Animation
/// mix unpremultiplied); a `light-dark()` gradient also carries `dark`, and
/// the view picks by its own appearance (LLP 1034 D2).
/// A tint-bearing ramp also carries its source in `tint` (and `tintDark` where
/// the dark scheme interpolates otherwise): a live painter resolves it against
/// its view, while snapshots retain the session's resolved colours.
pub(crate) fn gradient_json(g: &exact_kernel::gradient::Gradient) -> String {
    use exact_kernel::gradient::{premultiplied_ramp, Direction, Length};
    // Its session's overrides now, while that session is bound: the
    // presenter resolves the rest later, outside any (LLP 1121 stage 5).
    // A tint role (`LinkText`) is the theme's `AccentColor` too.
    let mut session = g.clone();
    for stop in &mut session.stops {
        if let Some(o) = roles::painted_override(stop.color) {
            stop.color = o;
        }
    }
    let tint = if g.stops.iter().any(|s| tint_share(s.color).is_some()) {
        let mut live = g.clone();
        for (stop, resolved) in live.stops.iter_mut().zip(&session.stops) {
            if tint_share(stop.color).is_none() {
                stop.color = resolved.color;
            }
        }
        // The interpolation the session's colours choose, written out: the
        // view's tint stands in for the theme's accent, whose syntax chose it.
        let written = |dark: bool| {
            let mut g = live.clone();
            g.interpolation = session.interpolation(dark);
            let mut css = String::new();
            exact_runner::agent::quote(&g.wire(), &mut css);
            css
        };
        let (light, night) = (written(false), written(true));
        let night = if night == light {
            String::new()
        } else {
            format!(",\"tintDark\":{night}")
        };
        format!(",\"tint\":{light}{night},\"sys\":\"@tint\"")
    } else {
        String::new()
    };
    let radii = matches!(
        g.kind,
        GradientKind::Radial {
            size: RadialSize::Radii(_),
            ..
        }
    );
    if g.needs_box() || radii {
        let mut css = String::new();
        exact_runner::agent::quote(&session.wire(), &mut css);
        return format!("{{\"css\":{css}{tint}}}");
    }
    // LLP 1100 D2: interpolated outside sRGB, the ramp crosses as unclipped
    // extended linear sRGB floats, alpha 0–1.
    let stops = |dark: bool| {
        if let Some(ramp) = g.interpolated(dark) {
            let parts: Vec<String> = ramp
                .into_iter()
                .map(|(at, c, a)| {
                    format!(
                        "{},{},{},{},{}",
                        num(at),
                        c[0] as f32,
                        c[1] as f32,
                        c[2] as f32,
                        a as f32
                    )
                })
                .collect();
            return format!("[{}]", parts.join(","));
        }
        let parts: Vec<String> = premultiplied_ramp(&g.resolved(dark))
            .into_iter()
            .map(|(at, c)| format!("{},{},{},{},{}", num(at), c.r(), c.g(), c.b(), c.a()))
            .collect();
        format!("[{}]", parts.join(","))
    };
    let shape = match g.kind {
        GradientKind::Linear(Direction::Angle(deg)) => format!("\"linear\":{}", num(deg)),
        GradientKind::Linear(Direction::Corner { right, bottom }) => {
            format!("\"corner\":[{},{}]", u8::from(right), u8::from(bottom))
        }
        // @ref LLP 1077 D5 — `[from, x%, xpx, y%, ypx]`.
        GradientKind::Conic { from, at } => {
            let axis = |l: Length| match l {
                Length::Percent(p) => format!("{},0", num(p)),
                Length::Px(px) => format!("0,{}", num(px)),
            };
            format!("\"conic\":[{},{},{}]", num(from), axis(at[0]), axis(at[1]))
        }
        GradientKind::Radial {
            circle,
            size: RadialSize::Extent(extent),
            at,
        } => {
            let axis = |l: Length| match l {
                Length::Percent(p) => format!("{},0", num(p)),
                Length::Px(px) => format!("0,{}", num(px)),
            };
            format!(
                "\"radial\":[{},{},{},{}]",
                u8::from(circle),
                extent as u8,
                axis(at[0]),
                axis(at[1])
            )
        }
        GradientKind::Radial {
            size: RadialSize::Radii(_),
            ..
        } => unreachable!("sent as CSS above"),
    };
    let dark = if g.is_scheme_aware() {
        format!(",\"dark\":{}", stops(true))
    } else {
        String::new()
    };
    let space = if g.interpolation(false).is_some() {
        ",\"space\":\"srgb-linear\""
    } else {
        ""
    };
    let dark_space = if g.is_scheme_aware() && g.interpolation(true).is_some() {
        ",\"darkSpace\":\"srgb-linear\""
    } else {
        ""
    };
    format!(
        "{{{shape},\"stops\":{}{dark}{space}{dark_space}{tint}}}",
        stops(false)
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn tint_references_follow_the_presenting_platforms_role_column() {
        for role in ["LinkText", "Highlight"] {
            let image = exact_kernel::gradient::BackgroundImage::parse(&format!(
                "linear-gradient({role}, {role})"
            ))
            .unwrap();
            let sent: serde_json::Value =
                serde_json::from_str(&super::gradient_json(image.gradient().unwrap())).unwrap();
            assert_eq!(
                sent.get("tint").is_some(),
                !cfg!(target_os = "macos"),
                "{role}: {sent}"
            );
        }
    }

    #[test]
    fn the_views_tint_resolves_before_gradient_placement_and_interpolation() {
        for css in [
            "linear-gradient(AccentColor, Highlight)",
            "radial-gradient(40px at 20% 50%, AccentColor, Highlight)",
            "repeating-linear-gradient(AccentColor 0px, Highlight 10px)",
            "conic-gradient(from 45deg, AccentColor, Highlight)",
        ] {
            for tint in [[255, 0, 0, 255], [128, 128, 128, 153]] {
                let floats = tint.map(|v| v as f32 / 255.0);
                let mut out = vec![0; 16384];
                // SAFETY: all three buffers have the lengths required by resolve.
                let n = unsafe {
                    super::resolve(
                        css.as_ptr(),
                        css.len(),
                        [100., 50.],
                        false,
                        floats.as_ptr(),
                        out.as_mut_ptr(),
                        out.len(),
                    )
                };
                assert!(n > 0 && n <= out.len(), "{css}: {n}");
                let resolved: serde_json::Value = serde_json::from_slice(&out[..n]).unwrap();
                let stops = resolved["stops"].as_array().unwrap();
                assert_eq!(stops[1], tint[0], "{css}: {resolved}");
                assert_eq!(stops[4], tint[3], "{css}: {resolved}");
                assert_eq!(
                    stops.last().unwrap(),
                    51,
                    "Highlight keeps its share: {css}"
                );
                assert!(resolved.get("tint").is_none(), "resolved once: {resolved}");
            }
        }
    }

    #[test]
    fn tint_stops_keep_their_reference_for_the_presenting_view() {
        let image = exact_kernel::gradient::BackgroundImage::parse(
            "linear-gradient(AccentColor, Highlight)",
        )
        .unwrap();
        let sent: serde_json::Value =
            serde_json::from_str(&super::gradient_json(image.gradient().unwrap())).unwrap();
        assert_eq!(sent["sys"], "@tint", "{sent}");
        assert!(sent["tint"].as_str().unwrap().contains("AccentColor"));
    }

    #[test]
    fn explicit_radii_cross_as_css_and_the_kernel_places_them() {
        let css = "radial-gradient(120% 90% at 8% 0%, #FFCFA8 0%, #FFCFA800 58%)";
        let image = exact_kernel::gradient::BackgroundImage::parse(css).unwrap();
        let sent: serde_json::Value =
            serde_json::from_str(&super::gradient_json(image.gradient().unwrap())).unwrap();
        let css = sent["css"].as_str().unwrap().as_bytes();
        let mut out = vec![0; 4096];
        // SAFETY: both buffers have the lengths passed.
        let n = unsafe {
            super::resolve(
                css.as_ptr(),
                css.len(),
                [200.0, 100.0],
                false,
                std::ptr::null(),
                out.as_mut_ptr(),
                out.len(),
            )
        };
        let value: serde_json::Value = serde_json::from_slice(&out[..n]).unwrap();
        assert_eq!(value["ellipse"], serde_json::json!([16, 0, 240, 90]));
        // Placed, it crosses as stops: CSS again would be resolved again, forever.
        assert!(value.get("css").is_none(), "{value}");
        assert_eq!(value["stops"].as_array().unwrap().len(), 15, "{value}");
    }

    #[test]
    fn lengths_cross_the_apple_seam_after_the_box_is_known() {
        let css = b"linear-gradient(to right, red 20px, blue 80%)";
        // SAFETY: input is readable; the first call asks only for its size.
        let n = unsafe {
            super::resolve(
                css.as_ptr(),
                css.len(),
                [200.0, 50.0],
                false,
                std::ptr::null(),
                std::ptr::null_mut(),
                0,
            )
        };
        assert!(n > 0);
        let mut out = vec![0; n];
        // SAFETY: the output has the exact capacity just requested.
        assert_eq!(
            unsafe {
                super::resolve(
                    css.as_ptr(),
                    css.len(),
                    [200.0, 50.0],
                    false,
                    std::ptr::null(),
                    out.as_mut_ptr(),
                    n,
                )
            },
            n
        );
        let value: serde_json::Value = serde_json::from_slice(&out).unwrap();
        assert!((value["stops"][5].as_f64().unwrap() - 0.1).abs() < 0.0001);
        assert_eq!(value["line"][2], 200.0);
        assert!(value.get("css").is_none());
    }
}
