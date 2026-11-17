use super::*;

// Bar symbol cases from host/web/tests/bar-symbols.test.mjs, through Linux layout and CPU paint.
const APP: &str = r##"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column width=600 background-color="#ffffff" gap=10 align-items="flex-start"
      column navigationKey="route" width=600
        header display="flex" flex-wrap="wrap" gap=8 align-items="center"
          text "Title" role="heading" aria-level=2
          row press=go testId="sized-item"
            image "symbol:circle" font-size=13 width=22 height=20 object-fit="none" color="#ff0000" testId="sized"
            if taps % 2 == 1
              text "3"
          row press=go font-size=13
            image "symbol:circle" color="#ff0000" testId="inherited"
          row press=go role="status"
            image "symbol:circle" font-size=13 width=22 height=20 color="#ff0000" testId="status"
          row press=go testId="relative-item"
            image "symbol:circle" font-size=40 min-width="1em" min-height="1em" color="#ff0000" testId="relative"
            if taps % 2 == 1
              text "3"
          row press=go
            image "symbol:circle" font-size=40 max-width="0.5em" max-height="0.5em" padding="0.25em" color="#ff0000" testId="constrained"
          row press=go
            image "symbol:icon||0 0 24 24|M0 0H24V24H0Z|icons/square.svg" font-size=13 width=22 height=20 color="#ff0000" testId="own"
          row press=go
            image "symbol:sf/airpodsmax" font-size=13 width=22 height=20 testId="blank"
          button appearance="auto" aria-label="Circle" testId="native"
            image "symbol:circle" font-size=13 -exact-tint-color="#ff0000" testId="native-image"
          button appearance="auto" command="show-modal" commandfor="confirmation" aria-label="Confirm" testId="native-command"
            image "symbol:circle" font-size=13 -exact-tint-color="#ff0000" testId="command-image"
          button appearance="auto" press=go testId="worded"
            text "Edit"
          row press=go
            image "symbol:circle" font-size=13 width=22 height=20 color="#ff0000" testId="counted"
            text "3"
          row press=go
            box
              image "symbol:circle" font-size=13 width=22 height=20 color="#ff0000" testId="nested"
          row role="tablist"
            row press=go
              image "symbol:circle" width=22 height=20 testId="tab"
      column navigationKey="none" navigationBar="none"
        header
          text "No bar" role="heading"
          row press=go
            image "symbol:circle" width=22 height=20 testId="no-bar"
      column navigationKey="authored"
        header appearance="none"
          text "Authored" role="heading"
          row press=go
            image "symbol:circle" width=22 height=20 testId="authored"
      column navigationKey="headless"
        header
          row press=go
            image "symbol:circle" width=22 height=20 testId="headless"
      column navigationKey="readout"
        header
          column role="status"
            text "Readout" role="heading"
            row press=go
              image "symbol:circle" width=22 height=20 testId="readout"
      row press=go
        image "symbol:circle" font-size=13 width=22 height=20 testId="page"
      dialog id="confirmation"
        text "Confirm"
"##;

pub(super) fn route_bar_symbols() {
    selection_boundaries();
    let plan = contract::compile(APP).unwrap().encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (600., 600.),
        4.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for root in [16., 20., 24., 16.] {
        assert!(p.set_text_scale(root / 16.).is_none());
        let bar = root * 1.2941;
        for name in ["status", "relative", "sized", "inherited", "headless"] {
            let (w, h) = size(&mut p, name);
            assert!((w - bar * 1.2).abs() < 0.06, "{name} at {root}: {w}");
            assert!((h - bar * 1.2).abs() < 0.06, "{name} at {root}: {h}");
        }
        let constrained = size(&mut p, "constrained");
        assert!((constrained.0 - bar).abs() < 0.06);
        assert!((constrained.1 - bar).abs() < 0.06);
        let (w, h) = size(&mut p, "own");
        assert!((w - bar).abs() < 0.06 && (h - bar).abs() < 0.06);
        let blank = size(&mut p, "blank");
        assert!((blank.0 - bar).abs() < 0.06 && (blank.1 - bar).abs() < 0.06);
        for name in ["native-image", "command-image"] {
            let image = p.host().kernel().node(id(&p, name)).unwrap();
            assert!((image.bar_symbol_size().unwrap() - bar * 1.2).abs() < 0.06);
        }
        for name in ["native", "native-command"] {
            assert_eq!(size(&mut p, name).1, size(&mut p, "worded").1);
        }
        for name in [
            "counted", "nested", "tab", "no-bar", "authored", "page", "readout",
        ] {
            assert_eq!(size(&mut p, name), (22., 20.), "{name} at {root}");
        }
        for name in [
            "sized",
            "inherited",
            "status",
            "relative",
            "own",
            "native",
            "native-command",
        ] {
            let target = id(&p, name);
            let (x, y, w, h) = p.rect_of(target).unwrap();
            let frame = p.frame();
            let (mut left, mut top, mut right, mut bottom) = (u32::MAX, u32::MAX, 0, 0);
            for py in (y * 4.).floor() as u32..((y + h) * 4.).ceil() as u32 {
                for px in (x * 4.).floor() as u32..((x + w) * 4.).ceil() as u32 {
                    let c = frame.pixel(px, py).unwrap().demultiply();
                    if c.red().saturating_sub(c.green().max(c.blue())) > 127 {
                        left = left.min(px);
                        top = top.min(py);
                        right = right.max(px);
                        bottom = bottom.max(py);
                    }
                }
            }
            assert!(left != u32::MAX, "{name} has ink");
            let ink = (
                (right - left + 1) as f32 / 4.,
                (bottom - top + 1) as f32 / 4.,
            );
            if name.starts_with("native") {
                let center = (
                    (left + right + 1) as f32 / 8.,
                    (top + bottom + 1) as f32 / 8.,
                );
                assert!(
                    (center.0 - x - w / 2.).abs() < 0.5,
                    "native icon centered horizontally"
                );
                assert!(
                    (center.1 - y - h / 2.).abs() < 0.5,
                    "native icon centered vertically"
                );
            }
            let expected = if name == "own" {
                bar
            } else {
                bar * 1.2 * 20. / 24.
            };
            assert!((ink.0 - expected).abs() < 1., "{name} at {root}: {ink:?}");
            assert!((ink.1 - expected).abs() < 1., "{name} at {root}: {ink:?}");
        }
        let item = id(&p, "sized-item");
        p.tap(item).unwrap();
        assert_eq!(
            size(&mut p, "sized"),
            (22., 20.),
            "a count restores authored sizing"
        );
        // Its own natural size again, `symbol-scale` times its font: the
        // source document.rs writes for it, above its 1em minimum.
        assert_eq!(size(&mut p, "relative"), (48., 48.));
        p.tap(item).unwrap();
        assert!((size(&mut p, "sized").0 - bar * 1.2).abs() < 0.06);
    }
    site_header_symbols();
    titled_buttons();
}

/// Headings inside buttons no handler presses: a header with one is a bar,
/// and one with two is not, as the web counts them.
fn titled_buttons() {
    for (count, projected) in [(1, true), (2, false)] {
        let titles = (0..count)
            .map(|n| format!("button\n          text \"Title {n}\" role=\"heading\"\n        "))
            .collect::<String>();
        let source = format!(
            r#"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column navigationKey="route" width=400
      header
        {titles}row press=go
          image "symbol:circle" width=22 height=20 testId="symbol"
"#
        );
        let plan = contract::compile(&source).unwrap().encode();
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let bar = 16. * 1.2941 * 1.2;
        let expected = if projected { (bar, bar) } else { (22., 20.) };
        let actual = size(&mut p, "symbol");
        assert!(
            (actual.0 - expected.0).abs() < 0.06 && (actual.1 - expected.1).abs() < 0.06,
            "{count} titles: {actual:?}, expected {expected:?}"
        );
    }
}

fn selection_boundaries() {
    let mut failures = Vec::new();
    for (tag, command) in [
        ("column", ""),
        ("column popover=\"auto\"", "show-popover"),
        ("dialog", "show-modal"),
    ] {
        let appearance = if command.is_empty() {
            "appearance=\"auto\""
        } else {
            ""
        };
        let source = format!(
            r#"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column width=400
      button "Open" commandfor="layer" command="{command}" testId="open"
      {tag} id="layer" width=300
        column navigationKey="route" width=300
          header
            text "Title" role="heading"
            row press=go
              image "symbol:circle" font-size=13 width=22 height=20 testId="custom"
            button {appearance} aria-label="Circle"
              image "symbol:circle" font-size=13 testId="native"
            button {appearance} commandfor="confirmation" command="show-modal" aria-label="Confirm"
              image "symbol:circle" font-size=13 testId="command"
      dialog id="confirmation"
        text "Confirm"
"#
        );
        let plan = contract::compile(&source).unwrap().encode();
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (400., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        if !command.is_empty() {
            p.tap(id(&p, "open")).unwrap();
            p.run_commands(<()>::default);
        }
        for root in [16., 20., 24.] {
            assert!(p.set_text_scale(root / 16.).is_none());
            for name in ["custom", "native", "command"] {
                let image = p.host().kernel().node(id(&p, name)).unwrap();
                let actual = image.bar_symbol_size();
                let expected = command.is_empty().then_some(root * 1.2941 * 1.2);
                if !match (actual, expected) {
                    (Some(a), Some(b)) => (a - b).abs() < 0.06,
                    (None, None) => true,
                    _ => false,
                } {
                    failures.push(format!(
                        "{tag}, {name} at {root}: {actual:?}, expected {expected:?}"
                    ));
                }
            }
            if !command.is_empty() {
                let actual = size(&mut p, "custom");
                if actual != (22., 20.) {
                    failures.push(format!("{tag} at {root}: {actual:?}, expected (22, 20)"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

fn site_header_symbols() {
    for (contents, projected) in [
        ("nav", false),
        (
            "link href=\"/one\" width=20 height=20\n          link href=\"/two\" width=20 height=20",
            false,
        ),
        ("box popover=\"auto\"\n            nav", false),
        (
            "link href=\"/one\" width=20 height=20\n          box popover=\"auto\"\n            link href=\"/two\" width=20 height=20",
            true,
        ),
        (
            "link href=\"/one\" width=20 height=20\n          dialog\n            link href=\"/two\" width=20 height=20",
            true,
        ),
    ] {
        let source = format!(
            r#"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column navigationKey="route" width=400
      header
        text "Title" role="heading"
        row press=go
          image "symbol:circle" font-size=13 width=22 height=20 testId="symbol"
        box
          {contents}
"#
        );
        let plan = contract::compile(&source).unwrap().encode();
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (400., 300.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        for root in [16., 20., 24.] {
            assert!(p.set_text_scale(root / 16.).is_none());
            let expected = if projected {
                (root * 1.2941 * 1.2, root * 1.2941 * 1.2)
            } else {
                (22., 20.)
            };
            let actual = size(&mut p, "symbol");
            assert!(
                (actual.0 - expected.0).abs() < 0.06 && (actual.1 - expected.1).abs() < 0.06,
                "{contents} at {root}: {actual:?}, expected {expected:?}"
            );
        }
    }
}

pub(super) fn file_icons_draw_in_a_linux_menu_row() {
    let dir = std::env::temp_dir().join(format!("exact-file-icon-menu-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(
        dir.join("assets/ai.svg"),
        r#"<svg viewBox="0 0 24 24"><defs><style>.cls-1{fill:#000}</style></defs><path class="cls-1" d="M2 2h20v20H2Z"/></svg>"#,
    )
    .unwrap();
    let plan = contract::compile(
        r##"component App
  state clicks = 0
  action choose
    clicks = clicks + 1
  view
    column width=160 height=160 background-color="#ffffff"
      column role="menu" width=96 background-color="#ffffff"
        button appearance="auto" aria-label="Illustrator" testId="item" width=48 height=48 press=choose
          image "symbol:icon||0 0 24 24||ai.svg" font-size=24 -exact-tint-color="#00ff00"
"##,
    )
    .unwrap()
    .encode();
    let (mut p, error) =
        Presenter::boot_with(&plan, (), (160., 160.), 1., dir.clone(), PainterChoice::Cpu).unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host().kernel();
    let item = k.node_by_key(k.find_by_test_id("item")[0]).unwrap().id;
    p.wait_images(std::time::Duration::from_secs(30));
    assert_eq!(p.images().bitmaps.len(), 1, "the file icon loaded");
    let (x, y, w, h) = p.rect_of(item).unwrap();
    let (cx, cy) = ((x + w / 2.) as u32, (y + h / 2.) as u32);
    let frame = p.frame();
    let ink = (cy - 8..cy + 8)
        .flat_map(|y| (cx - 8..cx + 8).map(move |x| (x, y)))
        .map(|(x, y)| frame.pixel(x, y).unwrap())
        .filter(|c| c.red() == 0 && c.green() == 0 && c.blue() == 0)
        .count();
    assert_eq!(ink, 256, "the menu row draws the file's black centre");
    std::fs::remove_dir_all(dir).unwrap();
}
