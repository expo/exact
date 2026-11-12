use super::*;

pub(super) fn control_sizes_follow_the_root_in_measure_and_paint() {
    let mut source = String::from(
        "component App\n  view\n    column width=800 font-size=31 background-color=\"white\"\n",
    );
    let mut pairs = Vec::new();
    for (size, rem) in [
        ("mini", 0.875),
        ("small", 0.875),
        ("medium", 1.0),
        ("large", 1.125),
    ] {
        for (variant, button_font, title_font, reference_font) in [
            ("unsaid", "", "", format!(" font-size=\"{rem}rem\"")),
            ("button", " font-size=19", "", " font-size=19".into()),
            (
                "title",
                " font-size=19",
                " font-size=21",
                " font-size=21".into(),
            ),
        ] {
            let name = format!("{size}-{variant}");
            source.push_str("      row width=800 height=80 align-items=\"flex-start\"\n");
            for (suffix, font, title) in [
                ("", button_font, title_font),
                ("-ref", reference_font.as_str(), ""),
            ] {
                source.push_str(&format!(
                    "        column width=400 align-items=\"flex-start\"\n          button -exact-control-size=\"{size}\"{font} testId=\"{name}{suffix}\"\n            image \"symbol:book\"\n            text \"MMMM\"{title}\n"
                ));
            }
            pairs.push(name);
        }
    }
    let plan = contract::compile(&source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (800.0, 1000.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let _roles = p.host().runner().bind_roles();
    assert!(error.is_none(), "{error:?}");
    let mut initial = Vec::new();
    for root in [16.0, 12.0, 24.0, 12.0, 16.0] {
        assert!(p.set_root_font_size(root).is_none());
        for (index, name) in pairs.iter().enumerate() {
            let reference = format!("{name}-ref");
            let measured = size(&mut p, name);
            assert_eq!(measured, size(&mut p, &reference), "{name} at root {root}");
            let pixels = text_pixels(&mut p, name);
            assert!(pixels.iter().any(|c| *c != [255; 3]), "visible face");
            assert_eq!(
                pixels,
                text_pixels(&mut p, &reference),
                "{name} paint at root {root}"
            );
            if root == 16.0 {
                if initial.len() < pairs.len() {
                    initial.push((measured, pixels));
                } else {
                    assert_eq!((measured, pixels), initial[index], "restored {name}");
                }
            }
        }
    }
}

pub(super) fn plain_faces_fade_without_a_wash() {
    let mut failures = Vec::new();
    for scheme in ["light", "dark"] {
        for style in ["plain", "borderless"] {
            let source = format!(
                r##"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column width=240 height=480 padding=10 gap=8 align-items="flex-start" color-scheme="{scheme}" background-color="-exact-background"
      button press=go buttonStyle="{style}" width=200 height=52 testId="default"
        image "symbol:play-fill"
        text "MMMM"
        text "MMMM"
      button press=go buttonStyle="{style}" width=200 height=52 testId="authored"
        image "symbol:play-fill"
        text "MMMM" color="#aa2244"
        text "MMMM" color="#22884480"
      button press=go buttonStyle="{style}" width=200 height=52 testId="tinted"
        image "symbol:play-fill" -exact-tint-color="#228844"
        text "MMMM"
      button press=go buttonStyle="{style}" width=200 height=52 disabled=true testId="disabled"
        image "symbol:play-fill"
        text "MMMM"
      column role="menu"
        button press=go buttonStyle="{style}" width=200 height=52 testId="menu"
          image "symbol:play-fill"
          text "MMMM"
      button press=go buttonStyle="{style}" width=200 height=52 testId="focus"
        text "MMMM"
"##
            );
            let plan = contract::compile(&source).unwrap();
            let (mut p, error) = Presenter::boot_with(
                &plan.encode(),
                (),
                (240.0, 480.0),
                1.0,
                std::path::PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            let _roles = p.host().runner().bind_roles();
            assert!(error.is_none(), "{error:?}");
            let ground = if scheme == "dark" { [18; 3] } else { [255; 3] };
            for name in ["default", "authored", "tinted", "disabled"] {
                p.blur();
                let rest = text_pixels(&mut p, name);
                assert!(rest.iter().any(|c| *c != ground), "visible {name}");
                let target = id(&p, name);
                p.hover(target).unwrap();
                p.set_pointer(None);
                if text_pixels(&mut p, name) != rest {
                    failures.push(format!("{scheme} {style} {name}: hover changed the face"));
                }
                let (x, y, _, _) = p.rect_of(target).unwrap();
                p.pointer_down(x + 4.0, y + 4.0, 0.0).unwrap();
                p.set_pointer(None);
                p.blur();
                let held = text_pixels(&mut p, name);
                if pixel(&mut p, name, 4.0, 26.0) != ground {
                    failures.push(format!("{scheme} {style} {name}: held adds a wash"));
                }
                // The reference is the resting face composited at 75% over its
                // ground. Allow two levels for the separate alpha roundings.
                assert!(
                    rest.iter().filter(|c| **c != ground).count() > 20,
                    "{name} has a painted face"
                );
                for (before, after) in rest.iter().zip(&held) {
                    let explicit_tint =
                        name == "tinted" && before[1] > before[0] && before[1] > before[2];
                    let alpha = if name == "disabled" || explicit_tint {
                        1.0
                    } else {
                        0.75
                    };
                    let expected: [u8; 3] = std::array::from_fn(|i| {
                        (f32::from(before[i]) * alpha + f32::from(ground[i]) * (1.0 - alpha))
                            .round() as u8
                    });
                    if after.iter().zip(expected).any(|(a, b)| a.abs_diff(b) > 2) {
                        failures.push(format!("{scheme} {style} {name}: held pixel {after:?}, expected {expected:?} from {before:?}"));
                        break;
                    }
                }
                p.pointer_cancel(0.0).unwrap();
                p.blur();
            }
            // Menu rows retain their wash and full-strength ink.
            let target = id(&p, "menu");
            let rest = text_pixels(&mut p, "menu");
            p.hover(target).unwrap();
            p.set_pointer(None);
            assert_ne!(pixel(&mut p, "menu", 4.0, 26.0), ground);
            let (x, y, _, _) = p.rect_of(target).unwrap();
            p.pointer_down(x + 4.0, y + 4.0, 0.0).unwrap();
            p.set_pointer(None);
            assert_ne!(pixel(&mut p, "menu", 4.0, 26.0), ground);
            let held = text_pixels(&mut p, "menu");
            let accent = if scheme == "dark" {
                [0x68, 0xa5, 0xfe]
            } else {
                [0x0b, 0x66, 0xd6]
            };
            assert!(rest.contains(&accent), "menu ink {scheme}");
            assert!(held.contains(&accent), "held menu ink {scheme}");
            p.pointer_cancel(0.0).unwrap();
            p.blur();
            // A held face keeps the keyboard focus ring at its resting strength.
            p.type_key(target, "Tab", "Tab", true, false).unwrap();
            p.type_key(target, "Tab", "Tab", false, false).unwrap();
            let target = id(&p, "focus");
            assert_eq!(p.focus(), Some(target));
            let ring = pixel(&mut p, "focus", 100.0, 1.0);
            assert_eq!(ring, accent);
            let focused = text_pixels(&mut p, "focus");
            let (x, y, _, _) = p.rect_of(target).unwrap();
            p.pointer_down(x + 4.0, y + 4.0, 0.0).unwrap();
            p.set_pointer(None);
            assert!(text_pixels(&mut p, "focus") != focused, "held face changes");
            assert_eq!(pixel(&mut p, "focus", 100.0, 1.0), ring);
            p.pointer_cancel(0.0).unwrap();
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
