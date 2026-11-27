use super::*;

/// Three destinations, the first selected and the last disabled, in a
/// 412 wide window under the light scheme, no palette told.
fn bar() -> Layout {
    badged(None)
}

/// `bar()`, its second destination holding `badge`.
fn badged(badge: Option<&str>) -> Layout {
    labelled(badge, true)
}

/// `badged`, its destinations' symbols left out where `symbols` is false.
fn labelled(badge: Option<&str>, symbols: bool) -> Layout {
    let mut engine = TextEngine::from_font_files(
        &[std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ))],
        "DejaVu Sans",
    );
    let tab = |node, label: &str, selected, enabled| Tab {
        node,
        label: label.into(),
        direction: Direction::Ltr,
        symbol: symbols.then(|| "star".into()),
        selected,
        enabled,
        badge: badge.filter(|_| node == 3).map(Into::into),
    };
    let bottom = Bottom {
        tablist: 1,
        direction: Direction::Ltr,
        tabs: vec![
            tab(2, "Home", true, true),
            tab(3, "Starred", false, true),
            tab(4, "Settings", false, false),
        ],
    };
    let inputs = Inputs {
        device_inks: &Default::default(),
        colors: BarColors::default(),
        dark: false,
        font_scale: 1.0,
        width: 412.0,
        edge: 0.0,
        sides: (0.0, 0.0),
    };
    Layout::bottom(&mut engine, &bottom, &inputs)
}

#[test]
fn truncated_rtl_bar_labels_fit_and_center_in_their_targets() {
    let mut engine = TextEngine::from_font_files(
        &[std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ))],
        "DejaVu Sans",
    );
    let title = "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة";
    let top = Top {
        route: 1,
        heading: Some(2),
        direction: Direction::Rtl,
        title: title.into(),
        title_direction: Direction::Rtl,
        subtitle_node: None,
        subtitle: String::new(),
        subtitle_direction: Direction::Ltr,
        up: false,
        leading: vec![],
        trailing: vec![Button {
            node: 3,
            title: title.into(),
            direction: Direction::Rtl,
            symbol: None,
            enabled: true,
            destructive: false,
            tint: None,
            badge: None,
        }],
        scrolled: false,
    };
    for font_scale in [1., 2.] {
        let inputs = Inputs {
            device_inks: &Default::default(),
            colors: BarColors::default(),
            dark: false,
            font_scale,
            width: 300.,
            edge: 0.,
            sides: (0., 0.),
        };
        for symbol in [None, Some("star".into())] {
            let bottom = Bottom {
                tablist: 4,
                direction: Direction::Rtl,
                tabs: vec![Tab {
                    node: 5,
                    label: title.repeat(2),
                    direction: Direction::Rtl,
                    symbol,
                    selected: true,
                    enabled: true,
                    badge: None,
                }],
            };
            let layout = Layout::bottom(&mut engine, &bottom, &inputs);
            for item in &layout.items {
                let Item::Text(p, at, ..) = item else {
                    continue;
                };
                assert!(p
                    .layout_runs()
                    .flat_map(|r| r.glyphs)
                    .any(|g| g.start == g.end));
                let (left, _, width, _) = text_rect(p, *at);
                assert!(left >= -0.01 && left + width <= inputs.width + 0.01);
                assert!((left + width / 2. - inputs.width / 2.).abs() < 0.01);
            }
        }
        let layout = Layout::top(&mut engine, &top, &inputs);
        for item in &layout.items {
            let Item::Text(p, at, _, node, _) = item else {
                continue;
            };
            assert!(p
                .layout_runs()
                .flat_map(|r| r.glyphs)
                .any(|g| g.start == g.end));
            let (left, _, width, _) = text_rect(p, *at);
            let target = layout.targets[0].rect;
            if *node == 3 {
                assert!(
                    (left + width / 2. - (target.0 + target.2 / 2.)).abs() < 0.01,
                    "button label must be centered at scale {font_scale}"
                );
                assert!(left >= target.0 + TEXT_BUTTON_PADDING - 0.01);
                assert!(left + width <= target.0 + target.2 - TEXT_BUTTON_PADDING + 0.01);
            } else {
                assert!(left >= target.0 + target.2 + dp(metric::TOP_EDGE) - 0.01);
                assert!(left + width <= inputs.width - 2. * dp(metric::TOP_EDGE) + 0.01);
            }
        }
    }
}

#[test]
fn rtl_bars_keep_each_labels_authored_direction() {
    let source = r#"routes nav
  tab home "/"
  tab other "/other"
component App
  state edited = false
  action edit
    edited = !edited
  view
    main navigationKey=`${top(nav).id}` direction="rtl"
      column role="tabpanel" id="panel-home"
        column navigationKey=`${top(nav).id}`
          header
            text "Beginning of a very long English heading ending here" role="heading" direction="ltr"
            button press=edit
              text "Beginning of a very long English action ending here" direction="ltr"
          text "Body"
      column role="tabpanel" id="panel-other"
        text "Other body"
      row role="tablist"
        button role="tab" aria-controls="panel-home" aria-selected=true
          text "Beginning of a very long English destination ending here" direction="ltr"
        button "Other" role="tab" aria-controls="panel-other" aria-selected=false
"#;
    for rtl in [false, true] {
        let source = if rtl {
            ["heading", "action", "destination"]
                .into_iter()
                .fold(source.to_string(), |s, kind| {
                    s.replace(
                        &format!("Beginning of a very long English {kind} ending here"),
                        "بداية هذا النص العربي الطويل الذي يستمر حتى نهاية العبارة",
                    )
                })
                .replace("direction=\"ltr\"", "direction=\"rtl\"")
        } else {
            source.to_string()
        };
        let (mut p, error) = crate::presenter::Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (300., 600.),
            1.,
            std::path::PathBuf::new(),
            crate::presenter::PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host.set_bar_room(Some((64., 80., 80.))).unwrap();
        let (top, bottom) = p.read_bars();
        let inputs = Inputs {
            device_inks: &Default::default(),
            colors: BarColors::default(),
            dark: false,
            font_scale: 1.,
            width: 300.,
            edge: 0.,
            sides: (0., 0.),
        };
        let mut engine = TextEngine::from_font_files(
            &[std::path::Path::new(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../scripts/fixtures/fonts/assets"
            ))],
            "DejaVu Sans",
        );
        let top = Layout::top(&mut engine, &top.unwrap(), &inputs);
        let bottom = Layout::bottom(&mut engine, &bottom.unwrap(), &inputs);
        let mut checked = 0;
        for layout in [&top, &bottom] {
            let texts = layout.texts();
            for item in &layout.items {
                if let Item::Text(p, at, _, node, _) = item {
                    let rect = texts.iter().find(|(id, _)| id == node).unwrap().1;
                    for glyph in p.layout_runs().flat_map(|r| r.glyphs) {
                        assert!(at.0 + glyph.x >= rect.0 - 0.01);
                        assert!(at.0 + glyph.x + glyph.w <= rect.0 + rect.2 + 0.01);
                    }
                    for run in p.layout_runs() {
                        if !run
                            .text
                            .starts_with(if rtl { "بداية" } else { "Beginning" })
                        {
                            continue;
                        }
                        assert_eq!(run.rtl, rtl, "the label keeps its authored direction");
                        let first = run
                            .glyphs
                            .iter()
                            .find(|g| g.start == 0 && g.end > 0)
                            .expect("the first cluster survives truncation");
                        let ellipsis = run
                            .glyphs
                            .iter()
                            .find(|g| g.start == g.end)
                            .expect("the long label has an ellipsis");
                        for glyph in run.glyphs {
                            if rtl {
                                assert!(glyph.x <= first.x, "first cluster at the right");
                                assert!(glyph.x >= ellipsis.x, "ellipsis at the left");
                            } else {
                                assert!(glyph.x >= first.x, "first cluster at the left");
                                assert!(glyph.x <= ellipsis.x, "ellipsis at the right");
                            }
                        }
                        checked += 1;
                    }
                }
            }
        }
        assert_eq!(checked, 3);
        assert!(bottom.targets[0].rect.0 > bottom.targets[1].rect.0);
    }
}

#[test]
fn a_title_has_the_room_what_leads_and_trails_leave_it() {
    let mut engine = TextEngine::from_font_files(
        &[std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ))],
        "DejaVu Sans",
    );
    let title = "A title nearly as wide as its bar";
    let wide =
        crate::presenter::top::look::paragraph(&mut engine, title, TITLE_LARGE, 1.0, None).width;
    let button = Button {
        node: 3,
        title: "Star".into(),
        direction: Direction::Ltr,
        symbol: Some("star".into()),
        enabled: true,
        destructive: false,
        tint: None,
        badge: None,
    };
    let top = Top {
        route: 1,
        heading: Some(2),
        direction: Direction::Ltr,
        title: title.into(),
        title_direction: Direction::Ltr,
        subtitle_node: None,
        subtitle: String::new(),
        subtitle_direction: Direction::Ltr,
        up: false,
        leading: Vec::new(),
        trailing: vec![button],
        scrolled: false,
    };
    // Compose's room: the bar less 4 and a 48 button at its end, 4 at
    // its start, and the title's own 4 either side; 2 to spare. From
    // the inset (16) instead, the title would be cut.
    let width = wide + 4.0 + 48.0 + 4.0 + 8.0 + 2.0;
    let inputs = Inputs {
        device_inks: &Default::default(),
        colors: BarColors::default(),
        dark: false,
        font_scale: 1.0,
        width,
        edge: 0.0,
        sides: (0.0, 0.0),
    };
    let bar = Layout::top(&mut engine, &top, &inputs);
    let (p, at) = (bar.items.iter())
        .find_map(|item| match item {
            Item::Text(p, at, _, 2, _) => Some((p.clone(), *at)),
            _ => None,
        })
        .expect("the title");
    assert_eq!(p.width, wide, "not cut");
    // Moved back from the button: it ends 4 before the button's target.
    let bounds = text_rect(&p, at);
    assert!((bounds.0 + bounds.2 - (width - 4.0 - 48.0 - 4.0)).abs() < 0.01);
}

#[test]
fn bars_without_authored_colours_keep_every_device_paint_role() {
    use super::super::tests::{barred, APP};
    let (mut p, _) = barred(APP);
    let paints = |layout: &Layout| {
        layout
            .items
            .iter()
            .map(|item| match item {
                Item::Fill(_, c) | Item::Symbol(_, _, c) | Item::Text(_, _, c, _, _) => *c,
                Item::Badge(_) => panic!("unexpected badge item"),
            })
            .collect::<Vec<_>>()
    };
    for dark in [false, true] {
        p.set_system_scheme(dark);
        p.sync_bars();
        let top = &p.bars.top.as_ref().unwrap().layout;
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert_eq!(top.container, Role::Surface.color(dark));
        assert_eq!(
            paints(top),
            [
                Role::OnSurfaceVariant.color(dark),
                Role::OnSurface.color(dark)
            ]
        );
        let inactive = told(Material::OnSurfaceVariant, dark, 0xff49454f, 0xffcac4d0);
        assert_eq!(bottom.container, Role::SurfaceContainer.color(dark));
        assert_eq!(
            paints(bottom),
            [
                Role::SecondaryContainer.color(dark),
                told(Material::OnSecondaryContainer, dark, 0xff1d192b, 0xffe8def8),
                told(Material::Secondary, dark, 0xff625b71, 0xffccc2dc),
                inactive,
                inactive,
            ]
        );
        assert_eq!(
            (
                p.chrome().status_light_icons,
                p.chrome().navigation_light_icons
            ),
            (dark, dark)
        );
    }
}

#[test]
fn the_root_accent_seeds_only_the_bars_accent_roles() {
    use super::super::tests::{barred, APP};
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="#245c46" navigationKey"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "More""#)
        .replace(
            r#"header display"#,
            r##"header accent-color="#bb2222" display"##,
        );
    let (mut p, _) = barred(&source);
    // Google's Material Color Utilities, SchemeTonalSpot(#245c46), standard contrast.
    for (dark, primary, indicator, symbol, label) in [
        (
            false,
            0xff1e6a4fu32,
            0xffcfe9dau32,
            0xff354b41u32,
            0xff4c6358u32,
        ),
        (
            true,
            0xff8cd5b4u32,
            0xff354b41u32,
            0xffcfe9dau32,
            0xffb3ccbeu32,
        ),
    ] {
        p.set_system_scheme(dark);
        p.sync_bars();
        let rgba = |argb: u32| {
            let [a, r, g, b] = argb.to_be_bytes();
            [r, g, b, a]
        };
        let top = &p.bars.top.as_ref().unwrap().layout;
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(&top.items[0], Item::Text(_, _, c, _, _) if *c == rgba(primary)));
        assert!(matches!(&bottom.items[0], Item::Fill(_, c) if *c == rgba(indicator)));
        assert!(matches!(&bottom.items[1], Item::Symbol(_, _, c) if *c == rgba(symbol)));
        assert!(matches!(&bottom.items[2], Item::Text(_, _, c, _, _) if *c == rgba(label)));
        assert_eq!(top.container, Role::Surface.color(dark));
        assert_eq!(bottom.container, Role::SurfaceContainer.color(dark));
    }
}

#[test]
fn each_bar_uses_its_own_scheme_with_the_roots_single_seed() {
    use super::super::tests::{barred, APP};
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="light-dark(#245c46, #bb2222)" navigationKey"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "More""#)
        .replace("header display", r##"header color-scheme="dark" display"##)
        .replace(
            r#"testId="tabs""#,
            r##"testId="tabs" background-color="#4c6358""##,
        );
    let (mut p, _) = barred(&source);
    p.set_system_scheme(false);
    p.sync_bars();
    let top = &p.bars.top.as_ref().unwrap().layout;
    let bottom = &p.bars.bottom.as_ref().unwrap().layout;
    assert!(matches!(
        &top.items[0],
        Item::Text(_, _, [0x8c, 0xd5, 0xb4, 255], _, _)
    ));
    assert!(matches!(
        &bottom.items[1],
        Item::Symbol(_, _, [0x35, 0x4b, 0x41, 255])
    ));
    // Both secondary roles miss on #4c6358. Material's foreground tone is 86.28.
    assert!(matches!(
        &bottom.items[2],
        Item::Text(_, _, [0xc5, 0xde, 0xcf, 255], _, _)
    ));
}

#[test]
fn a_seeded_bar_tracks_material_contrast_and_respects_action_tints() {
    use super::super::tests::{barred, APP};
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="#245c46" navigationKey"##,
        )
        .replace(r#"image "symbol:more""#, r#"text "More""#)
        .replace(
            "button popovertarget",
            r##"button color="#bb2222" popovertarget"##,
        );
    let (mut p, _) = barred(&source);
    for (level, indicator) in [
        (0.5, [0x5b, 0x72, 0x66, 255]),
        (1., [0x38, 0x4e, 0x43, 255]),
    ] {
        p.colors.material_contrast = level;
        p.sync_bars();
        let top = &p.bars.top.as_ref().unwrap().layout;
        let bottom = &p.bars.bottom.as_ref().unwrap().layout;
        assert!(matches!(
            &top.items[0],
            Item::Text(_, _, [0xbb, 0x22, 0x22, 255], _, _)
        ));
        assert!(matches!(&bottom.items[0], Item::Fill(_, c) if *c == indicator));
        assert!(matches!(&bottom.items[1], Item::Symbol(_, _, c) if *c == [255; 4]));
    }
    drop(p);
    let lone = source.replace(r#"          image "symbol:home""#, "");
    let (mut p, _) = barred(&lone);
    p.colors.material_contrast = 1.;
    p.sync_bars();
    let bottom = &p.bars.bottom.as_ref().unwrap().layout;
    assert!(matches!(
        &bottom.items[0],
        Item::Fill(_, [0x38, 0x4e, 0x43, 255])
    ));
    assert!(matches!(&bottom.items[1], Item::Text(_, _, c, _, _) if *c == [255; 4]));
    drop(p);
    for accent in [r##"accent-color="#245c46""##, ""] {
        let source = source
            .replace(r##"accent-color="#245c46""##, accent)
            .replace(
                r##"button color="#bb2222""##,
                r##"button disabled=true color="#bb2222""##,
            );
        let (mut p, _) = barred(&source);
        p.sync_bars();
        let top = &p.bars.top.as_ref().unwrap().layout;
        assert!(
            matches!(&top.items[0], Item::Text(_, _, c, _, _) if *c == Role::Disabled.color(false))
        );
    }
}

#[test]
fn a_disabled_selected_symbol_keeps_its_on_colour_over_the_seeded_indicator() {
    use super::super::tests::{barred, APP};
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color="transparent" navigationKey"##,
        )
        .replace(r#"testId="tabs""#, r##"testId="tabs" color="#ffd9e2""##)
        .replace(r#"testId="tab-home""#, r#"testId="tab-home" disabled=true"#);
    let (mut p, _) = barred(&source);
    p.sync_bars();
    let bottom = &p.bars.bottom.as_ref().unwrap().layout;
    assert!(matches!(
        &bottom.items[0],
        Item::Fill(_, [0xff, 0xd9, 0xe2, 255])
    ));
    assert!(matches!(
        &bottom.items[1],
        Item::Symbol(_, _, [0x5a, 0x3f, 0x47, 97])
    ));
    assert!(matches!(
        &bottom.items[2],
        Item::Text(_, _, [0x74, 0x56, 0x5f, 97], _, _)
    ));
}

#[test]
fn the_navigation_bar_has_materials_indicator_and_colours() {
    let bar = bar();
    let mut texts = Vec::new();
    let mut symbols = Vec::new();
    let mut fills = Vec::new();
    for item in &bar.items {
        match item {
            Item::Text(_, _, color, node, _) => texts.push((*node, *color)),
            Item::Symbol(_, rect, color) => symbols.push((*rect, *color)),
            Item::Fill(shape, _) => fills.push(shape.rect),
            Item::Badge(_) => panic!("unexpected badge item"),
        }
    }
    // Where no palette was told, Material's baseline: the selected
    // label `secondary`, its symbol `onSecondaryContainer`; the others
    // `onSurfaceVariant`, at 38 % when disabled.
    let variant = [0x49, 0x45, 0x4F, 0xFF];
    assert_eq!(
        texts,
        [
            (2, [0x62, 0x5B, 0x71, 0xFF]),
            (3, variant),
            (4, [0x49, 0x45, 0x4F, 97])
        ]
    );
    assert_eq!(symbols[0].1, [0x1D, 0x19, 0x2B, 0xFF]);
    assert_eq!(symbols[1].1, variant);
    // One indicator, 56 by 32, behind the first symbol: the first cell
    // is (412 − 16) / 3 = 132 wide.
    assert_eq!(fills.len(), 1);
    let (x, _, w, h) = fills[0];
    assert_eq!((w, h), (56.0, 32.0));
    assert_eq!(x + w / 2.0, 66.0);
    assert_eq!(symbols[2].0 .0 + 12.0, 280.0 + 66.0);
}

#[test]
fn a_destinations_badge_is_materials_large_badge_on_its_icon() {
    let bar = badged(Some("12"));
    let icon = bar
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Symbol(_, rect, _) => Some(*rect),
            _ => None,
        })
        .nth(1)
        .expect("the second destination's icon");
    let pill = bar
        .items
        .iter()
        .find_map(|item| match item {
            Item::Fill(shape, color) if *color == [0xB3, 0x26, 0x1E, 0xFF] => Some(shape.rect),
            _ => None,
        })
        .expect("one badge, in `error`");
    // 12 before the icon's end, 14 into it, 16 tall, wider for two digits.
    assert_eq!(
        (pill.0, pill.1 + pill.3, pill.3),
        (icon.0 + icon.2 - 12.0, icon.1 + 14.0, 16.0)
    );
    assert!(pill.2 > 16.0, "{pill:?}");
    assert!(
        bar.items
            .iter()
            .any(|item| matches!(item, Item::Text(_, _, [255, 255, 255, 255], 3, _))),
        "its count in `onError`"
    );
}

#[test]
fn a_destination_of_a_label_alone_keeps_its_badge_on_the_label() {
    let bar = labelled(Some("3"), false);
    let label = bar
        .items
        .iter()
        .find_map(|item| match item {
            Item::Text(p, at, color, 3, _) if *color != [255, 255, 255, 255] => {
                Some(text_rect(p, *at))
            }
            _ => None,
        })
        .expect("the label");
    let pill = bar
        .items
        .iter()
        .find_map(|item| match item {
            Item::Fill(shape, color) if *color == [0xB3, 0x26, 0x1E, 0xFF] => Some(shape.rect),
            _ => None,
        })
        .expect("its badge");
    assert_eq!(
        (pill.0, pill.1 + pill.3),
        (label.0 + label.2 - 12.0, label.1 + 14.0)
    );
}
