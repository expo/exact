use super::*;
use crate::paint::look::luminance;
use crate::presenter::bars::tests::{barred, tap, APP};

fn ratio(ink: [u8; 4], ground: [u8; 4]) -> f32 {
    let a = luminance(ink) + 0.05;
    let b = luminance(ground) + 0.05;
    a.max(b) / a.min(b)
}

#[test]
fn edit_uses_the_counterpart_primary_on_a_cross_scheme_header() {
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="#245c46" navigationKey"##,
        )
        .replace(
            "header display",
            r##"header background-color="light-dark(#173529, #faf4e8)" display"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "Edit""#);
    let (mut p, _) = barred(&source);
    for (dark, expected) in [
        (false, [0x8c, 0xd5, 0xb4, 255]),
        (true, [0x1e, 0x6a, 0x4f, 255]),
    ] {
        p.set_system_scheme(dark);
        p.sync_bars();
        let bar = &p.bars.top.as_ref().unwrap().layout;
        let Item::Text(_, _, ink, _) = &bar.items[0] else {
            panic!("missing Edit")
        };
        assert_eq!(
            *ink,
            expected,
            "Edit on {:?} measures {:.2}:1",
            bar.container,
            ratio(*ink, bar.container)
        );
    }
}

#[test]
fn a_translucent_header_checks_the_window_and_keeps_a_buttons_explicit_colour() {
    let source = APP
        .replace(
            "main navigationKey",
            r##"main background-color="#173529" accent-color="#245c46" navigationKey"##,
        )
        .replace(
            "header display",
            r##"header background-color="#ffffff08" display"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "Edit""#);
    for (tint, expected) in [
        ("", [0x8c, 0xd5, 0xb4, 255]),
        (r##"color="#173529""##, [0x17, 0x35, 0x29, 255]),
    ] {
        let (mut p, _) = barred(&source.replace(
            "button popovertarget",
            &format!("button {tint} popovertarget"),
        ));
        p.sync_bars();
        let bar = &p.bars.top.as_ref().unwrap().layout;
        assert_eq!(bar.container, [255, 255, 255, 8]);
        assert!(matches!(&bar.items[0], Item::Text(_, _, c, _) if *c == expected));
    }
}

#[test]
fn a_fractional_palette_tone_reads_where_tones_one_and_ninety_nine_miss() {
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="#245c46" navigationKey"##,
        )
        .replace(
            "header display",
            r##"header background-color="#6a7877" display"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "Edit""#);
    let (mut p, _) = barred(&source);
    p.sync_bars();
    let bar = &p.bars.top.as_ref().unwrap().layout;
    let Item::Text(_, _, ink, _) = &bar.items[0] else {
        panic!("missing Edit")
    };
    assert!(*ink != [0, 0, 0, 255] && *ink != [255; 4]);
    assert!(ratio(*ink, bar.container) >= 4.5);
}

#[test]
fn enabled_bar_ink_reads_on_the_fill_behind_each_glyph() {
    let check = |ink, ground, threshold, role| {
        let measured = ratio(ink, ground);
        assert!(
            measured >= threshold,
            "{role} {ink:?} on {ground:?}: {measured}:1"
        );
    };
    for seed in ["", "#245c46", "#ee4466", "#3355ff"] {
        for fill in ["#173529", "#faf4e8", "#777777", "#73835a"] {
            let mut source = APP
                .replace(
                    "header display",
                    &format!(r#"header background-color="{fill}" display"#),
                )
                .replace(
                    r#"testId="tabs""#,
                    &format!(r#"testId="tabs" background-color="{fill}""#),
                )
                .replace(
                    r#"                      text "Edit""#,
                    concat!(
                        "                      text \"Edit\"\n",
                        "                    button press=edit destructive=true\n",
                        "                      text \"Delete\"\n",
                        "                    button press=share\n",
                        "                      image \"symbol:more\""
                    ),
                );
            if !seed.is_empty() {
                source = source.replace(
                    "main navigationKey",
                    &format!(r#"main accent-color="{seed}" navigationKey"#),
                );
            }
            let (mut p, _) = barred(&source);
            tap(&mut p, "open").unwrap();
            for dark in [false, true] {
                p.set_system_scheme(dark);
                p.sync_bars();
                let top = &p.bars.top.as_ref().unwrap().layout;
                assert_eq!(top.items.len(), 5, "Up, Edit, Delete, More and title");
                for item in &top.items {
                    match item {
                        Item::Text(_, _, ink, _) => check(*ink, top.container, 4.5, "header text"),
                        Item::Symbol(_, _, ink) => check(*ink, top.container, 3., "header symbol"),
                        Item::Fill(..) => panic!("unexpected header fill"),
                    }
                }
                let nav = &p.bars.bottom.as_ref().unwrap().layout;
                let [Item::Fill(_, pill), Item::Symbol(_, _, selected), Item::Text(_, _, label, _), Item::Symbol(_, _, inactive), Item::Text(_, _, other, _)] =
                    &nav.items[..]
                else {
                    panic!("expected two destinations")
                };
                check(*selected, *pill, 3., "selected symbol");
                check(*label, nav.container, 4.5, "selected label");
                check(*inactive, nav.container, 3., "unselected symbol");
                check(*other, nav.container, 4.5, "unselected label");
            }
        }
    }
}

#[test]
fn device_ink_uses_a_reported_swatch_and_a_palette_edit_keeps_the_held_tab() {
    use crate::presenter::{ReaderPalette, MATERIAL_ROLES};
    use exact_kernel::{style::roles, Color, ColorValue};
    let source = APP
        .replace(
            "header display",
            r##"header background-color="#6a6a6a" display"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "Edit""#);
    let (mut p, _) = barred(&source);
    // Tones 40 and 80 of yellow both miss. Tone 95 retains the device palette's chroma.
    let mut words = vec![0; MATERIAL_ROLES * 2 + 1 + 3 * 13];
    words[Material::Primary as usize] = 0xff626200u32 as i32;
    words[MATERIAL_ROLES + Material::Primary as usize] = 0xffcdcd00u32 as i32;
    words[MATERIAL_ROLES * 2 + 1 + 9] = 0xfff9f900u32 as i32;
    let device = ReaderPalette::from_words(&words).unwrap();
    p.host.runner().roles().set(&[(
        roles::role("AccentColor").unwrap(),
        ColorValue::LightDark(Color(0x626200ff), Color(0xcdcd00ff)),
    )]);
    p.bars.set_device_inks(&device.roles, &device.swatches);
    p.sync_bars();
    let top = &p.bars.top.as_ref().unwrap().layout;
    assert!(matches!(
        top.items[0],
        Item::Text(_, _, [0xf9, 0xf9, 0, 255], _)
    ));
    let target = p.bars.bottom.as_ref().unwrap().layout.targets[1].rect;
    p.bar_event(super::super::BOTTOM, 0, target.0 + 5., target.1 + 5., 0.);
    let held = p.bars.pressed.expect("a held destination");
    p.bars.focused = Some(held);
    let revision = p.bars.revision;
    let mut update = device.roles;
    update.dark[Material::Error as usize] = 0xffbb1122;
    p.host.runner().roles().set(&[(
        roles::role("-exact-system-red").unwrap(),
        ColorValue::LightDark(Color(0xff383cff), Color(0xbb1122ff)),
    )]);
    p.bars.set_device_inks(&update, &device.swatches);
    p.sync_bars();
    assert_eq!((p.bars.pressed, p.bars.focused), (Some(held), Some(held)));
    assert!(p.bars.ripple.is_some());
    assert_eq!(p.bars.revision, revision, "neither bar uses error");
}

#[test]
fn a_transparent_indicator_puts_its_symbol_or_lone_label_on_the_authored_bar() {
    use exact_kernel::{style::roles, Color, ColorValue};
    for (fill, threshold, lone) in [
        ("#1d192b", 3., false),
        ("#707070", 3., false),
        ("#625b71", 4.5, true),
    ] {
        let mut source = APP.replace(
            r#"testId="tabs""#,
            &format!(r#"testId="tabs" background-color="{fill}""#),
        );
        if lone {
            source = source.replace(r#"          image "symbol:home""#, "");
        }
        let (mut p, _) = barred(&source);
        p.host.runner().roles().set(&[(
            roles::role("ButtonFace").unwrap(),
            ColorValue::Fixed(Color::TRANSPARENT),
        )]);
        p.sync_bars();
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(bottom.items[0], Item::Fill(_, c) if c[3]==0));
        let ink = match &bottom.items[1] {
            Item::Symbol(_, _, ink) | Item::Text(_, _, ink, _) => *ink,
            _ => panic!("expected selected content"),
        };
        assert!(ratio(ink, bottom.container) >= threshold);
        if fill == "#707070" {
            assert!(
                ratio(ink, bottom.container) < 4.5,
                "a readable symbol keeps its own role"
            );
        }
    }
}
