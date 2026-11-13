use super::*;

pub(super) fn popover_sources_and_sheet_word_clips_match_the_browser() {
    let sf = "symbol:icon|add|0 0 24 24|M0 0H24V24H0Z|icons/square.svg";
    let source = format!(
        r##"component App
  action plain
    showPopover("pop")
  action sourced
    showPopover("pop", source="source")
  action hidden
    showPopover("pop", source="hidden")
  action invisible
    showPopover("pop", source="invisible")
  action close
    hidePopover("pop")
  state note = false
  action toggleNote
    note = not note
  view
    column width=600 padding=40 gap=20 align-items="flex-start" background-color="white"
      button "Source" id="source" testId="sourced" press=sourced
      button "Hidden" id="hidden" display="none"
      button "Invisible" id="invisible" visibility="hidden"
      button "No source" testId="plain" press=plain
      button "Hidden source" testId="hidden" press=hidden
      button "Invisible source" testId="invisible" press=invisible
      button "Note" testId="note" press=toggleNote
      column popover="manual" role="alertdialog" id="pop" width=300 align-items="flex-start"
        button font-size=20 testId="choice" press=close popovertarget="pop" popovertargetaction="hide"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
        button "Close" testId="close" popovertarget="pop" popovertargetaction="hide"
        if note
          text "Note"
"##
    );
    let plan = contract::compile(&source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (600., 600.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let _roles = p.host().runner().bind_roles();
    assert!(error.is_none(), "{error:?}");
    let mut failures = Vec::new();
    use exact_kernel::{AxisOffer, ButtonMeasureRequest, TextMeasurer};
    let mut measurer = exact_linux::text::Measurer(exact_linux::text::TextEngine::shared());
    for (opener, chooser) in [
        ("plain", false),
        ("sourced", true),
        ("hidden", false),
        ("invisible", true),
        ("plain", false),
        ("sourced", true),
    ] {
        p.tap(id(&p, opener)).unwrap();
        p.run_commands(|| ());
        let style = p
            .host()
            .kernel()
            .button_face_style(id(&p, "choice"))
            .unwrap();
        if style.neutral_menu_row != chooser {
            failures.push(format!(
                "{opener}: chooser={}, expected {chooser}",
                style.neutral_menu_row
            ));
        }
        let mut request = ButtonMeasureRequest {
            face: p.host().kernel().press_face(id(&p, "choice")).unwrap(),
            style,
            button_style: None,
            width: AxisOffer::MaxContent,
        };
        let measured = measurer.button_measure(&request).unwrap();
        assert_eq!(
            size(&mut p, "choice"),
            (measured.width, measured.height),
            "fresh measurement after {opener}"
        );
        request.face.symbol = None;
        let title = measurer.button_measure(&request).unwrap();
        let expected = if chooser { 30. } else { 38. };
        if (measured.width - title.width - expected).abs() > 0.01 {
            failures.push(format!(
                "{opener}: image and gap {}, expected {expected}",
                measured.width - title.width
            ));
        }
        let expected = if chooser { 20 } else { 30 };
        let painted = painted_choice(&mut p);
        if painted != (expected, expected) {
            failures.push(format!(
                "{opener}: symbol painted {painted:?}, expected {expected}px"
            ));
        }
        p.tap(id(&p, "close")).unwrap();
        p.run_commands(|| ());
    }
    // The web decides a chooser once, as it opens: text added while it is
    // open leaves it one, and it is none when it opens with that text.
    for (step, chooser) in [("open", true), ("note", true), ("reopen", false)] {
        match step {
            "open" => {
                p.tap(id(&p, "sourced")).unwrap();
            }
            "note" => {
                p.tap(id(&p, "note")).unwrap();
            }
            _ => {
                p.tap(id(&p, "close")).unwrap();
                p.run_commands(|| ());
                p.tap(id(&p, "sourced")).unwrap();
            }
        }
        p.run_commands(|| ());
        let row = p
            .host()
            .kernel()
            .button_face_style(id(&p, "choice"))
            .unwrap()
            .neutral_menu_row;
        let expected = if chooser { 20 } else { 30 };
        let painted = painted_choice(&mut p);
        if row != chooser || painted != (expected, expected) {
            failures.push(format!(
                "{step}: chooser={row}, symbol painted {painted:?}; expected {chooser}, {expected}px"
            ));
        }
    }
    for (name, header, label, symbol, before, wrapped, clipped) in [
        ("word", "", "aria-label=\"Discard\"", sf, false, false, true),
        (
            "wrapped",
            "",
            "aria-label=\"Discard\"",
            sf,
            false,
            true,
            true,
        ),
        (
            "authored-header",
            "appearance=\"auto\"",
            "aria-label=\"Discard\"",
            sf,
            false,
            false,
            true,
        ),
        (
            "unbarred-header",
            "appearance=\"none\"",
            "aria-label=\"Discard\"",
            sf,
            false,
            false,
            false,
        ),
        (
            "close",
            "",
            "aria-label=\"Close\"",
            "symbol:close",
            false,
            false,
            false,
        ),
        (
            "leading-close",
            "",
            "aria-label=\"Close\"",
            "symbol:close",
            true,
            false,
            true,
        ),
    ] {
        // No bar, so the symbols keep the face's size (a bar item's is the bar's).
        let heading = "text \"Title\" role=\"heading\"";
        let source = format!(
            r##"component App
  view
    column width=300 padding=40 background-color="white"
      column navigationKey="sheet" navigationPresentation="modal" navigationBar="none"
        header {header} display="flex" gap=40
          {}
          button appearance="auto" {label} width=20 height=20 font-size={} testId="word"
            image "{symbol}" -exact-tint-color="red"
          {}
"##,
            if before { "" } else { heading },
            if symbol == "symbol:close" { 40 } else { 20 },
            if before { heading } else { "" }
        );
        let source = if wrapped {
            source
                .lines()
                .map(|line| {
                    if line.starts_with("        ") {
                        format!("  {line}")
                    } else {
                        line.to_owned()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
                .replace("          header", "        column\n          header")
        } else {
            source
        };
        let plan = contract::compile(&source).unwrap();
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (300., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        let _roles = p.host().runner().bind_roles();
        assert!(error.is_none(), "{error:?}");
        let (x, y, w, h) = p.rect_of(id(&p, "word")).unwrap();
        assert_eq!((w, h), (20., 20.), "{name} hit bounds");
        let shot = p.frame();
        let mut outside = 0;
        let mut inside = 0;
        for py in (y - 8.) as u32..(y + h + 8.).ceil() as u32 {
            for px in (x - 8.) as u32..(x + w + 8.).ceil() as u32 {
                let c = shot.pixel(px, py).unwrap();
                if c.red().saturating_sub(c.green().max(c.blue())) <= 127 {
                    continue;
                }
                if px < x as u32 || px >= (x + w) as u32 || py < y as u32 || py >= (y + h) as u32 {
                    outside += 1;
                } else {
                    inside += 1;
                }
            }
        }
        assert!(inside > 0, "{name} paints its symbol inside the face");
        if (outside == 0) != clipped {
            failures.push(format!(
                "{name}: {outside} symbol pixels outside the face, clip={clipped}"
            ));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// The size of the red symbol painted inside the "choice" button.
fn painted_choice(p: &mut Presenter<()>) -> (u32, u32) {
    let (x, y, w, h) = p.rect_of(id(p, "choice")).unwrap();
    let shot = p.frame();
    let mut bounds = (u32::MAX, u32::MAX, 0, 0);
    for py in y as u32..(y + h).ceil() as u32 {
        for px in x as u32..(x + w).ceil() as u32 {
            let c = shot.pixel(px, py).unwrap();
            if c.red().saturating_sub(c.green().max(c.blue())) > 127 {
                bounds = (
                    bounds.0.min(px),
                    bounds.1.min(py),
                    bounds.2.max(px + 1),
                    bounds.3.max(py + 1),
                );
            }
        }
    }
    (
        bounds.2.saturating_sub(bounds.0),
        bounds.3.saturating_sub(bounds.1),
    )
}

pub(super) fn symbols_use_the_neutral_metrics_in_layout_and_paint() {
    let sf = "symbol:icon|circle|0 0 24 24|M0 0H24V24H0Z|icons/square.svg";
    let own = "symbol:icon||0 0 24 24|M0 0H24V24H0Z|icons/square.svg";
    let source = format!(
        r##"component App
  state large = false
  action grow
    large = not large
  view
    column width=600 gap=10 align-items="flex-start" background-color="white"
      row height=60 gap=10 align-items="flex-start" font-size=(large ? 40 : 20)
        image "symbol:circle" testId="role"
        image "{sf}" testId="sf" color="red"
        image "{own}" testId="own" color="red"
        image "symbol:sf/unmapped" testId="missing"
        image "{sf}" width=48 height=40 object-fit="none" testId="fixed" color="red"
      button "Grow" press=grow testId="grow"
      button font-size=20 testId="title"
        text "MMMM"
      button font-size=20 testId="titled"
        image "{sf}" -exact-tint-color="red"
        text "MMMM"
      button appearance="auto" font-size=20 gap=3 testId="gap"
        image "{sf}" -exact-tint-color="red"
        text "MMMM"
      button appearance="auto" aria-label="Square" font-size=20 testId="alone"
        image "{sf}" -exact-tint-color="red"
      button appearance="auto" aria-label="Square" font-size=20 testId="alone-own"
        image "{own}" -exact-tint-color="red"
      button font-size=20 flex-direction="column" testId="stacked"
        image "{sf}" -exact-tint-color="red"
        text "MMMM"
      button appearance="auto" font-size=20 width=200 height=36 gap=0 testId="sized"
        image "{sf}" -exact-tint-color="red"
        text "MMMM"
      button appearance="auto" font-size=20 testId="image-font"
        image "{sf}" font-size=24 -exact-tint-color="red"
        text "MMMM"
      button font-size=20 role="radio" testId="radio"
        image "{sf}" -exact-tint-color="red"
        text "MMMM"
      column role="menu" align-items="flex-start"
        button font-size=20 testId="menu"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
        button font-size=20 flex-direction="column" testId="menu-stacked"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
        button font-size=20 flex-direction="column" gap=3 testId="menu-stacked-gap"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
        button font-size=20 flex-direction="row" testId="menu-row-said"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
"##
    );
    let plan = contract::compile(&source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (600., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let _roles = p.host().runner().bind_roles();
    assert!(error.is_none(), "{error:?}");
    let mut failures = Vec::new();
    let mut check = |name: &str, got: (f32, f32), want: (f32, f32)| {
        if (got.0 - want.0).abs() > 0.01 || (got.1 - want.1).abs() > 0.01 {
            failures.push(format!("{name}: {got:?}, expected {want:?}"));
        }
    };
    for (index, em) in [20., 40., 20.].into_iter().enumerate() {
        if index > 0 {
            p.tap(id(&p, "grow")).unwrap();
        }
        // Read before the image sync: the commit must supply the same size.
        for (name, scale) in [("role", 1.2), ("sf", 1.2), ("own", 1.), ("missing", 1.)] {
            let f = p.host().kernel().node(id(&p, name)).unwrap().frame;
            check(name, (f.width, f.height), (em * scale, em * scale));
        }
        check("authored image", size(&mut p, "fixed"), (48., 40.));
        let mut images = exact_linux::image::Images::new(std::path::PathBuf::new());
        let image = id(&p, "sf");
        let reports = images.sync(p.host().kernel(), &[image]);
        check("image sync", reports[0].1.unwrap(), (em * 1.2, em * 1.2));
    }
    let title = size(&mut p, "title");
    let titled = size(&mut p, "titled");
    check(
        "button image and gap",
        titled,
        (title.0 + 30. + 8., title.1),
    );
    check(
        "authored gap",
        size(&mut p, "gap"),
        (title.0 + 30. + 3., title.1),
    );
    check(
        "lone symbol",
        size(&mut p, "alone"),
        size(&mut p, "alone-own"),
    );
    check("authored button", size(&mut p, "sized"), (200., 36.));
    check(
        "authored image font",
        size(&mut p, "image-font"),
        (title.0 + 36. + 8., title.1),
    );
    // 30 px image, a -2.5 px margin toward the 25 px title line, 8 px gap,
    // and the same 19.5 px block padding and border as the ordinary face.
    check("stacked", size(&mut p, "stacked"), (title.0, 80.));
    // A menu row's space beside its title is `.5em`, a space in its type
    // once its author writes a direction, and 1em stacked (button_css.rs).
    let menu = size(&mut p, "menu");
    check(
        "stacked menu row",
        size(&mut p, "menu-stacked"),
        (
            size(&mut p, "menu-stacked").0,
            size(&mut p, "menu-stacked-gap").1 + 17.,
        ),
    );
    let space = size(&mut p, "menu-row-said").0 - menu.0 + 10.;
    if !(5.0..8.0).contains(&space) {
        failures.push(format!("written row: a {space} px space"));
    }
    // The filled square makes the painted symbol box observable without font rasterization.
    let shot = p.frame();
    for (name, wanted) in [
        ("sf", 24.),
        ("own", 20.),
        ("fixed", 24.),
        ("titled", 30.),
        ("alone", 30.),
        ("stacked", 30.),
        ("sized", 30.),
        ("image-font", 36.),
        ("radio", 20.),
        ("menu", 20.),
    ] {
        let (x, y, w, h) = p.rect_of(id(&p, name)).unwrap();
        let mut bounds = (u32::MAX, u32::MAX, 0, 0);
        for py in y as u32..(y + h).ceil() as u32 {
            for px in x as u32..(x + w).ceil() as u32 {
                let c = shot.pixel(px, py).unwrap();
                if c.red() > 240 && c.green() < 20 && c.blue() < 20 {
                    bounds = (
                        bounds.0.min(px),
                        bounds.1.min(py),
                        bounds.2.max(px + 1),
                        bounds.3.max(py + 1),
                    );
                }
            }
        }
        if bounds.0 == u32::MAX {
            failures.push(format!("{name}: no painted symbol"));
        } else {
            let painted = ((bounds.2 - bounds.0) as f32, (bounds.3 - bounds.1) as f32);
            // Fractional positions can leave a partially covered edge pixel.
            if (painted.0 - wanted).abs() > 1.0 || (painted.1 - wanted).abs() > 1.0 {
                failures.push(format!("{name}: painted {painted:?}, expected {wanted}"));
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

pub(super) fn symbol_face_exceptions_match_the_browser() {
    let sf = "symbol:icon|circle|0 0 24 24|M0 0H24V24H0Z|icons/square.svg";
    let source = format!(
        r##"component App
  state taps = 0
  action go
    taps = taps + 1
  action openChooser
    showPopover("chooser", source="tight")
  view
    column width=600 padding=20 gap=20 align-items="flex-start" background-color="white"
      row gap=30 align-items="flex-start"
        button appearance="auto" font-size=20 width=24 height=24 press=go aria-label="Square" id="tight" testId="tight"
          image "{sf}" -exact-tint-color="red"
        column width=24 height=24 overflow="hidden" testId="ancestor"
          button appearance="auto" font-size=20 width=24 height=24 aria-label="Square" testId="parent-clipped"
            image "{sf}" -exact-tint-color="red"
      row gap=30 align-items="flex-start"
        button font-size=20 flex-direction="column" height=62 testId="above"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
        button font-size=20 flex-direction="column" height=62 testId="below"
          text "MMMM"
          image "{sf}" -exact-tint-color="red"
      button "Open" press=openChooser testId="open"
      column id="chooser" popover="manual" role=(taps % 2 == 0 ? "alertdialog" : "group") testId="chooser"
        button font-size=20 press=go testId="choice"
          image "{sf}" -exact-tint-color="red"
          text "MMMM"
      column navigationKey="route" navigationBar=(taps % 2 == 0 ? "automatic" : "none") width=400
        header display="flex" gap=20 align-items="flex-start"
          text "Title" role="heading"
          button appearance="auto" font-size=13 press=go aria-label="Square" testId="bar"
            image "{sf}" font-size=13 -exact-tint-color="red"
          button font-size=13 press=go aria-label="Square" testId="bar-own"
            image "symbol:icon||0 0 24 24|M0 0H24V24H0Z|icons/square.svg" font-size=9 -exact-tint-color="red"
          button appearance="auto" font-size=13 press=go testId="bar-titled"
            image "{sf}" -exact-tint-color="red"
            text "MMMM"
      text `Taps ${{taps}}` testId="taps"
"##
    );
    let plan = contract::compile(&source).unwrap();
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (600., 800.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let _roles = p.host().runner().bind_roles();
    assert!(error.is_none(), "{error:?}");
    let shot = p.frame();
    let mut failures = Vec::new();
    // Scan beyond the button's bounds: a lone symbol paints outside its hit box.
    for (name, wanted) in [
        ("tight", (-3., -3., 30., 30.)),
        ("parent-clipped", (0., 0., 24., 24.)),
        ("above", (1., 1., 30., 30.)),
        ("below", (1., 31.5, 30., 30.)),
        ("bar", (0., 0., 16. * 1.2941 * 1.2, 16. * 1.2941 * 1.2)),
        ("bar-own", (0., 0., 16. * 1.2941, 16. * 1.2941)),
        ("bar-titled", (1., 0., 19.5, 19.5)),
    ] {
        let (x, y, w, h) = p.rect_of(id(&p, name)).unwrap();
        let mut bounds = (f32::INFINITY, f32::INFINITY, 0f32, 0f32);
        for py in (y - 8.).max(0.) as u32..(y + h + 8.).ceil() as u32 {
            for px in (x - 8.).max(0.) as u32..(x + w + 8.).ceil() as u32 {
                let c = shot.pixel(px, py).unwrap();
                if c.red().saturating_sub(c.green().max(c.blue())) > 127 {
                    bounds = (
                        bounds.0.min(px as f32),
                        bounds.1.min(py as f32),
                        bounds.2.max(px as f32 + 1.),
                        bounds.3.max(py as f32 + 1.),
                    );
                }
            }
        }
        let actual = (
            bounds.0 - x,
            bounds.1 - y,
            bounds.2 - bounds.0,
            bounds.3 - bounds.1,
        );
        let expected = if name == "above" || name == "below" {
            ((w - wanted.2) / 2., wanted.1, wanted.2, wanted.3)
        } else if name == "bar" || name == "bar-own" {
            ((w - wanted.2) / 2., (h - wanted.3) / 2., wanted.2, wanted.3)
        } else {
            wanted
        };
        if [
            if name == "bar-titled" {
                0.
            } else {
                actual.0 - expected.0
            },
            if name == "bar-titled" {
                0.
            } else {
                actual.1 - expected.1
            },
            actual.2 - expected.2,
            actual.3 - expected.3,
        ]
        .iter()
        .any(|d| d.abs() > 1.)
        {
            failures.push(format!("{name}: painted {actual:?}, expected {expected:?}"));
        }
    }
    p.tap(id(&p, "open")).unwrap();
    p.run_commands(|| ());
    let choice = p
        .host()
        .kernel()
        .button_face_style(id(&p, "choice"))
        .unwrap();
    assert!(choice.neutral_role_floor && choice.neutral_menu_row);
    use exact_kernel::{AxisOffer, ButtonMeasureRequest, TextMeasurer};
    let mut measurer = exact_linux::text::Measurer(exact_linux::text::TextEngine::shared());
    let mut request = ButtonMeasureRequest {
        face: p.host().kernel().press_face(id(&p, "choice")).unwrap(),
        style: choice,
        button_style: None,
        width: AxisOffer::MaxContent,
    };
    let row = measurer.button_measure(&request).unwrap();
    request.face.symbol = None;
    let title = measurer.button_measure(&request).unwrap();
    // Chooser rows keep a 20px symbol and the menu's .5em gap.
    assert_eq!(row.width - title.width, 30.);
    request.face.symbol = Some(sf.strip_prefix("symbol:").unwrap().into());
    request.style.image_gap = Some(3.);
    assert_eq!(
        measurer.button_measure(&request).unwrap().width - title.width,
        23.
    );
    let bar_size = size(&mut p, "bar");
    assert!(p.set_root_font_size(24.).is_none());
    assert_eq!(size(&mut p, "bar"), (bar_size.0 + 8., bar_size.1 + 8.));
    assert!(p.set_root_font_size(16.).is_none());
    assert_eq!(size(&mut p, "bar"), bar_size);
    let (x, y, w, h) = p.rect_of(id(&p, "tight")).unwrap();
    p.pointer_down(x - 1., y + h / 2., 0.).unwrap();
    p.pointer_up(x - 1., y + h / 2., 1.).unwrap();
    assert_eq!(
        p.host().kernel().node(id(&p, "taps")).unwrap().text_runs()[0]
            .text
            .as_ref(),
        "Taps 0"
    );
    p.pointer_down(x + w / 2., y + h / 2., 2.).unwrap();
    p.pointer_up(x + w / 2., y + h / 2., 3.).unwrap();
    assert_eq!(
        p.host().kernel().node(id(&p, "taps")).unwrap().text_runs()[0]
            .text
            .as_ref(),
        "Taps 1"
    );
    // The open chooser's role is now `group`: the web marked it a chooser
    // as it opened and keeps the mark until it opens again.
    let k = p.host().kernel();
    assert!(
        k.button_face_style(id(&p, "choice"))
            .unwrap()
            .neutral_menu_row
    );
    assert!(k
        .button_face_style(id(&p, "bar"))
        .unwrap()
        .neutral_bar_symbol
        .is_none());
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

pub(super) fn chooser_symbols_exclude_text_and_inputs() {
    for (content, chooser) in [
        ("text \"Message\"", false),
        ("input value=\"Name\"", false),
        ("input type=\"checkbox\"", false),
        ("column\n            input value=\"Name\"", false),
        ("textarea value=\"Message\"", true),
    ] {
        let source = format!(
            r#"component App
  state chooserRole = "alertdialog"
  action open
    showPopover("pop", source="source")
  view
    column
      button "Open" id="source" testId="open" press=open
      column id="pop" popover="manual" role=chooserRole
        button font-size=20 testId="choice"
          image "symbol:book"
          text "Book"
        {content}
"#
        );
        let plan = contract::compile(&source).unwrap();
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (600., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        let _roles = p.host().runner().bind_roles();
        assert!(error.is_none(), "{error:?}");
        p.tap(id(&p, "open")).unwrap();
        p.run_commands(|| ());
        assert_eq!(
            p.host()
                .kernel()
                .button_face_style(id(&p, "choice"))
                .unwrap()
                .neutral_menu_row,
            chooser,
            "{content}"
        );
    }
}

pub(super) fn bar_symbols_keep_the_header_exclusions() {
    for (header, wrapper, content, expected) in [
        ("", "", "", true),
        ("appearance=\"none\"", "", "", false),
        ("", "appearance=\"none\"", "", false),
        ("", "", "nav", false),
        (
            "",
            "",
            "link href=\"/one\" width=20 height=20\n            link href=\"/two\" width=20 height=20",
            false,
        ),
        (
            "",
            "",
            "column popover=\"manual\"\n              link href=\"/one\" width=20 height=20\n              link href=\"/two\" width=20 height=20",
            true,
        ),
    ] {
        let source = format!(
            r#"component App
  state taps = 0
  action go
    taps = taps + 1
  view
    column navigationBack="back" navigationKey="route"
      column navigationKey="route"
        header {header}
          text "Title" role="heading"
          column {wrapper}
            button press=go aria-label="Book" testId="item"
              image "symbol:book" testId="image"
            {content}
"#
        );
        let plan = contract::compile(&source).unwrap();
        let (p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (600., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        let _roles = p.host().runner().bind_roles();
        assert!(error.is_none(), "{error:?}");
        let k = p.host().kernel();
        assert_eq!(
            k.node(id(&p, "image")).unwrap().bar_symbol_size().is_some(),
            expected,
            "header={header}, wrapper={wrapper}, content={content}"
        );
        assert_eq!(
            k.button_face_style(id(&p, "item"))
                .unwrap()
                .neutral_bar_symbol
                .is_some(),
            expected
        );
    }
}

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
