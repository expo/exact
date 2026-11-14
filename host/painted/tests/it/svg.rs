//! LLP 1055.000 D17: a press inside an `svg` goes to the element under it,
//! by `pointer-events`, and bubbles as a box's press does.
use exact_painted::{presenter::PainterChoice, Presenter};

#[test]
fn an_inline_svg_reports_its_cost_limit_once_per_node_lifetime() {
    let app = r##"component App
  state shown = true
  state limited = false
  action toggle
    limited = !limited
  action replace
    shown = !shown
  view
    column background-color="#ffffff"
      if shown
        svg testId="chart" width=100 height=40
          path d=(limited ? "M0 10H100000" : "M0 10H100") stroke="#ff0000" stroke-width=4 stroke-dasharray="0.25"
      button testId="toggle" press=toggle
        text "Toggle limit"
      button testId="replace" press=replace
        text "Replace node"
"##;
    let plan = contract::compile(app).unwrap().encode();
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (200., 200.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let warnings = |p: &Presenter<()>| {
        let logs: serde_json::Value =
            serde_json::from_str(&p.host().agent(r#"{"op":"logs","since":0}"#)).unwrap();
        logs["lines"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|line| line.as_str()?.split_once(' ').map(|(_, message)| message))
            .filter(|line| line.starts_with("svg #"))
            .map(str::to_string)
            .collect::<Vec<_>>()
    };
    assert!(p.frame().pixel(1, 10).unwrap().green() < 255);
    assert!(warnings(&p).is_empty());
    let chart = id(&p, "chart");
    let toggle = id(&p, "toggle");
    let replace = id(&p, "replace");
    p.tap(toggle).unwrap();
    assert_eq!(p.frame().pixel(1, 10).unwrap().green(), 255);
    let expected = format!(
        "svg #{chart}: {}; drawing nothing",
        exact_kernel::svg::scene::Limit::Dashes
    );
    assert_eq!(warnings(&p).as_slice(), std::slice::from_ref(&expected));
    for size in [210., 220.] {
        assert!(p.resize(size, size).is_none());
        p.frame();
    }
    p.tap(toggle).unwrap();
    assert!(p.frame().pixel(1, 10).unwrap().green() < 255);
    p.tap(toggle).unwrap();
    p.frame();
    assert_eq!(warnings(&p).as_slice(), std::slice::from_ref(&expected));
    let old_key = p.host().kernel().find_by_test_id("chart")[0];
    assert!(p.reload(&plan, ()).unwrap().is_none());
    assert_eq!(p.host().kernel().find_by_test_id("chart")[0], old_key);
    assert_eq!(p.frame().pixel(1, 10).unwrap().green(), 255);
    assert_eq!(warnings(&p).as_slice(), std::slice::from_ref(&expected));
    assert!(p.resize(230., 230.).is_none());
    p.frame();
    assert_eq!(warnings(&p), [expected]);
    p.tap(replace).unwrap();
    p.frame();
    p.tap(replace).unwrap();
    p.frame();
    assert_ne!(p.host().kernel().find_by_test_id("chart")[0], old_key);
    let chart = id(&p, "chart");
    assert_eq!(warnings(&p).len(), 2);
    assert!(warnings(&p)[1].starts_with(&format!("svg #{chart}: ")));
}

#[test]
fn svg_overflow_clips_to_its_content_box() {
    for overflow in [
        "",
        "overflow=\"hidden\"",
        "overflow=\"clip\"",
        "overflow=\"visible\"",
    ] {
        let plan = contract::compile(&format!(
            r##"component App
  view
    box width=400 height=400 background-color="#ffffff"
      svg width=100 height=100 padding=20 viewBox="0 0 100 100" {overflow}
        rect x=-50 y=-50 width=200 height=200 fill="#ff0000"
"##,
        ))
        .unwrap_or_else(|e| panic!("{e}"));
        let (mut presenter, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (400., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let frame = presenter.frame();
        let clips = overflow != "overflow=\"visible\"";
        for (x, y, red) in [
            (10, 10, !clips),
            (25, 25, true),
            (130, 130, !clips),
            (180, 180, false),
        ] {
            let c = frame.pixel(x, y).unwrap().demultiply();
            let expected = if red {
                (255, 0, 0, 255)
            } else {
                (255, 255, 255, 255)
            };
            assert_eq!(
                (c.red(), c.green(), c.blue(), c.alpha()),
                expected,
                "({x}, {y}), {overflow}"
            );
        }
    }
}

#[test]
fn svg_content_clip_uses_inner_radii_and_leaves_its_border_visible() {
    for radius in [10, 50] {
        let plan = contract::compile(&format!(
            r##"component App
  view
    box width=400 height=400 background-color="#ffffff"
      svg width=100 height=100 padding="10px 20px 30px 40px" border-width="2px 4px 6px 8px" border-style="solid" border-color="#0000ff" border-radius={radius} viewBox="0 0 100 100"
        rect x=-50 y=-50 width=200 height=200 fill="#ff0000"
"##,
        ))
        .unwrap_or_else(|e| panic!("{e}"));
        let (mut presenter, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (400., 400.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let frame = presenter.frame();
        let red = (255, 0, 0);
        let white = (255, 255, 255);
        let blue = (0, 0, 255);
        for (x, y, expected) in [
            (45, 60, white),
            (150, 70, white),
            (80, 8, white),
            (80, 120, white),
            (4, 70, blue),
            (170, 70, blue),
            (86, 1, blue),
            (86, 145, blue),
            (49, 60, red),
            (138, 30, red),
            (146, 13, if radius == 50 { white } else { red }),
            (146, 110, if radius == 50 { white } else { red }),
        ] {
            let c = frame.pixel(x, y).unwrap().demultiply();
            assert_eq!(
                (c.red(), c.green(), c.blue()),
                expected,
                "({x}, {y}), radius={radius}"
            );
        }
    }
}

#[test]
fn a_clip_use_paints_at_its_targets_transformed_position() {
    let plan = contract::compile(
        r##"component App
  view
    svg width=100 height=100 viewBox="0 0 100 100"
      defs
        rect id="shape" width=10 height=10 transform="translate(50 0)"
      clipPath id="clip"
        use href="#shape"
      rect width=100 height=100 fill="#ff0000" clip-path="url(#clip)"
"##,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let (mut presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (100., 100.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = presenter.frame();
    let red = |x, y| {
        let c = frame.pixel(x, y).unwrap().demultiply();
        (c.red(), c.green(), c.blue(), c.alpha()) == (255, 0, 0, 255)
    };
    assert!(red(55, 5), "the target's translation moves the clip");
    assert!(!red(5, 5), "the untransformed position is clipped away");
    let count = (0..100)
        .flat_map(|y| (0..100).map(move |x| (x, y)))
        .filter(|&(x, y)| red(x, y))
        .count();
    assert_eq!(count, 100, "only the 10 by 10 clip paints");
}

const APP: &str = "component App\n  state picked = \"none\"\n  action pick(which: string)\n    picked = which\n  view\n    column\n      text picked testId=\"picked\"\n      svg testId=\"chart\" width=100 height=100 viewBox=\"0 0 100 100\"\n        rect width=100 height=100 fill=\"#eeeeee\" press=pick(\"back\")\n        g press=pick(\"bars\")\n          rect x=10 y=10 width=20 height=80 fill=\"#2563eb\"\n          rect x=40 y=40 width=20 height=50 fill=\"#2563eb\" pointer-events=\"none\"\n        polyline points=\"70,90 90,10\" fill=\"none\" stroke=\"#000000\" stroke-width=6 press=pick(\"line\")\n";

#[test]
fn a_press_in_an_svg_reaches_the_element_under_it() {
    let plan = contract::compile(APP).unwrap_or_else(|e| panic!("{e}"));
    let (mut presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (400., 400.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kernel = presenter.host().kernel();
    let chart = kernel
        .node_by_key(kernel.find_by_test_id("chart")[0])
        .unwrap()
        .id;
    let b = *presenter
        .boxes()
        .iter()
        .find(|b| b.id == chart)
        .expect("the svg's box");
    let picked = |p: &mut Presenter<()>, x: f32, y: f32| {
        p.press_at(b.rect.0 + x, b.rect.1 + y, 0.0);
        let k = p.host().kernel();
        let text = k.node_by_key(k.find_by_test_id("picked")[0]).unwrap();
        text.props
            .str(exact_kernel::PropId::Text)
            .unwrap_or("")
            .to_string()
    };
    assert_eq!(
        picked(&mut presenter, 20., 50.),
        "bars",
        "a bar bubbles to its g"
    );
    assert_eq!(
        picked(&mut presenter, 50., 60.),
        "back",
        "pointer-events: none passes through to what is below"
    );
    assert_eq!(picked(&mut presenter, 80., 50.), "line", "on the stroke");
    assert_eq!(picked(&mut presenter, 95., 95.), "back");
}

/// SVG 2 §13.4.7: a zero-length subpath with round or square caps paints
/// its cap, a dot where Chrome paints one; a butt cap paints nothing.
#[test]
fn a_zero_length_subpath_paints_its_cap() {
    const DOTS: &str = "component App\n  view\n    svg testId=\"dots\" width=100 height=40 viewBox=\"0 0 100 40\"\n      path d=\"M 10 20 Z\" stroke=\"#ff0000\" stroke-width=12 stroke-linecap=\"round\"\n      path d=\"M 40 20 L 40 20\" stroke=\"#ff0000\" stroke-width=12 stroke-linecap=\"square\"\n      path d=\"M 70 20 L 70 20\" stroke=\"#ff0000\" stroke-width=12\n";
    let plan = contract::compile(DOTS).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (100., 40.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let k = p.host().kernel();
    let dots = k.node_by_key(k.find_by_test_id("dots")[0]).unwrap().id;
    let b = *p.boxes().iter().find(|b| b.id == dots).unwrap();
    let (x0, y0) = (b.rect.0 as u32, b.rect.1 as u32);
    let frame = p.frame();
    let red = |x: u32| {
        let c = frame.pixel(x0 + x, y0 + 20).unwrap().demultiply();
        (c.red(), c.green(), c.alpha())
    };
    assert_eq!(red(10), (255, 0, 255), "M p Z, round: a dot");
    assert_eq!(red(40), (255, 0, 255), "M p L p, square: a square");
    assert_ne!(red(70), (255, 0, 255), "a butt cap paints nothing");
}

#[test]
fn a_collapsed_calibrated_path_keeps_its_cap_and_siblings() {
    let plan = contract::compile(
        r##"component App
  view
    svg width=120 height=60 viewBox="0 0 120 60"
      rect width=120 height=60 fill="#ffffff"
      rect x=100 y=45 width=10 height=10 fill="#0000ff"
      path d="M20 30Z" stroke="#ff0000" stroke-width=12 stroke-linecap="round" pathLength=1 stroke-dasharray="0.2 0.2"
      path d="M50 30L50 30" stroke="#ff0000" stroke-width=12 stroke-linecap="square" pathLength=1 stroke-dasharray="0.2 0.2"
      path d="M80 30L80 30" stroke="#ff0000" stroke-width=12 pathLength=1 stroke-dasharray="0.2 0.2"
"##,
    )
    .unwrap_or_else(|e| panic!("{e}"));
    let (mut presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (120., 60.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = presenter.frame();
    // Chrome 157: a calibrated zero-length dash is solid; caps still apply.
    for (x, y, expected) in [
        (105, 50, (0, 0, 255, 255)),
        (20, 30, (255, 0, 0, 255)),
        (16, 30, (255, 0, 0, 255)),
        (50, 30, (255, 0, 0, 255)),
        (46, 26, (255, 0, 0, 255)),
        (80, 30, (255, 255, 255, 255)),
    ] {
        let pixel = frame.pixel(x, y).unwrap().demultiply();
        assert_eq!(
            (pixel.red(), pixel.green(), pixel.blue(), pixel.alpha()),
            expected,
            "({x}, {y})"
        );
    }
}

/// LLP 1084 §4: a portable symbol role is its path, stroked as the web
/// strokes it, in its `color`; an `sf/` name stays an empty box, unless it is a
/// role's own Apple name, which draws that role (LLP 1121 D10).
#[test]
fn a_symbol_role_is_its_path_and_an_sf_name_is_empty() {
    const SYMBOLS: &str = "component App\n  view\n    row\n      image \"symbol:checkmark\" testId=\"role\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n      image \"symbol:sf/checkmark.seal\" testId=\"sf\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n      image \"symbol:sf/checkmark\" testId=\"apple\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n";
    let plan = contract::compile(SYMBOLS).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (150., 48.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let reds = |p: &mut Presenter<()>, id: &str| {
        let k = p.host().kernel();
        let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id;
        let b = *p.boxes().iter().find(|b| b.id == node).unwrap();
        let frame = p.frame();
        let mut n = 0;
        for y in 0..48 {
            for x in 0..48 {
                let c = frame
                    .pixel(b.rect.0 as u32 + x, b.rect.1 as u32 + y)
                    .unwrap()
                    .demultiply();
                n += usize::from(c.red() > 200 && c.green() < 80 && c.alpha() > 200);
            }
        }
        n
    };
    assert!(
        reds(&mut p, "role") > 40,
        "the checkmark's stroke, in its color"
    );
    assert_eq!(
        reds(&mut p, "sf"),
        0,
        "an SF Symbol name draws nothing on Linux"
    );
    assert!(
        reds(&mut p, "apple") > 40,
        "a role's Apple name draws the role"
    );
    // An icon token's form: its path filled under its own viewBox (a square
    // over Material's box fills the image).
    const ICON: &str = "component App\n  view\n    image \"symbol:icon||0 -960 960 960|M0-960h960v960H0Z|\" testId=\"icon\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n";
    let plan = contract::compile(ICON).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, _) = Presenter::boot_with(
        &plan.encode(),
        (),
        (48., 48.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(
        reds(&mut p, "icon") > 2000,
        "the icon's square fills its box"
    );
}

/// Every portable role draws on Linux from its path alone: a role added to
/// the schema with a path this parser cannot read would be an empty box here
/// while Apple shows its SF Symbol (podcast F3's media roles among them).
#[test]
fn every_symbol_role_draws_its_path() {
    let roles = exact_kernel::generated::SYMBOL_ROLES;
    let mut source = String::from("component App\n  view\n    column\n");
    for role in roles {
        source.push_str(&format!(
            "      image \"symbol:{role}\" testId=\"{role}\" width=24 height=24 font-size=24 color=\"#ff0000\"\n"
        ));
    }
    let plan = contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
    let height = 24. * roles.len() as f32;
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (24., height),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.frame();
    let empty: Vec<_> = roles
        .iter()
        .enumerate()
        .filter(|(i, _)| {
            let ink = (0..24u32)
                .flat_map(|y| (0..24u32).map(move |x| (x, y)))
                .filter_map(|(x, y)| frame.pixel(x, *i as u32 * 24 + y))
                .filter(|c| c.alpha() > 40 && c.red() > c.green())
                .count();
            // `ellipsis`, three round dots, is the least ink of any role.
            ink < 3
        })
        .map(|(_, role)| *role)
        .collect();
    assert!(empty.is_empty(), "roles that draw nothing: {empty:?}");
}

/// LLP 1055.000 D15 (issue #123): a path's `d` under `transition` morphs
/// on the agent's clock, as Chrome does: the chevron is flat half way, and
/// stands the other way when it settles.
#[test]
fn a_transitioning_d_morphs_the_path() {
    const MORPH: &str = "component App\n  state open = false\n  action flip\n    open = not open\n  view\n    column\n      button press=flip testId=\"flip\"\n        text \"Flip\"\n      svg testId=\"icon\" width=96 height=96 viewBox=\"0 0 24 24\"\n        path fill=\"none\" stroke=\"#000000\" stroke-width=2 d=(open ? \"M6 15 L12 9 L18 15\" : \"M6 9 L12 15 L18 9\") transition=\"d 400ms linear\"\n";
    let plan = contract::compile(MORPH).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (200., 200.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = |p: &Presenter<()>, name: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    let icon = id(&p, "icon");
    // The rows of the icon with ink in them: its height in points.
    let ink = |p: &mut Presenter<()>| {
        let b = *p.boxes().iter().find(|b| b.id == icon).unwrap();
        let frame = p.frame();
        (0..96u32)
            .filter(|y| {
                (0..96u32).any(|x| {
                    let c = frame
                        .pixel(b.rect.0 as u32 + x, b.rect.1 as u32 + y)
                        .unwrap()
                        .demultiply();
                    c.alpha() > 200 && c.red() < 100
                })
            })
            .count()
    };
    let rest = ink(&mut p);
    assert!((28..=34).contains(&rest), "a chevron: {rest}");
    p.tap(id(&p, "flip")).unwrap();
    p.tick(200.);
    let flat = ink(&mut p);
    assert!(flat <= 10, "flat half way: {flat}");
    p.tick(500.);
    assert_eq!(ink(&mut p), rest, "settled the other way");
}

/// Stage 14 C2: an even-odd icon's form (`evenodd:` before its path) fills
/// its path even-odd, so a ring of two squares wound alike has its hole; the
/// same path as a nonzero icon is filled through, as before.
#[test]
fn an_even_odd_icon_draws_its_hole() {
    const RINGS: &str = "component App\n  view\n    column\n      image \"symbol:icon||0 0 24 24|evenodd:M2 2H22V22H2Z M8 8H16V16H8Z|\" testId=\"odd\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n      image \"symbol:icon||0 0 24 24|M2 2H22V22H2Z M8 8H16V16H8Z|\" testId=\"nonzero\" width=48 height=48 object-fit=\"contain\" color=\"#ff0000\"\n";
    let plan = contract::compile(RINGS).unwrap_or_else(|e| panic!("{e}"));
    let (mut p, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (48., 96.),
        1.,
        std::env::temp_dir(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let red = |p: &mut Presenter<()>, id: &str, x: u32, y: u32| {
        let k = p.host().kernel();
        let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id;
        let b = *p.boxes().iter().find(|b| b.id == node).unwrap();
        let c = p
            .frame()
            .pixel(b.rect.0 as u32 + x, b.rect.1 as u32 + y)
            .unwrap()
            .demultiply();
        c.red() > 200 && c.green() < 80 && c.alpha() > 200
    };
    // (8, 8) is on the ring (viewBox 4, 4); (24, 24) its middle (12, 12).
    assert!(red(&mut p, "odd", 8, 8), "the even-odd ring is drawn");
    assert!(!red(&mut p, "odd", 24, 24), "its hole is clear");
    assert!(red(&mut p, "nonzero", 8, 8), "the nonzero ring is drawn");
    assert!(
        red(&mut p, "nonzero", 24, 24),
        "and filled through, as before"
    );
}

fn file_icons_app(source: &str, case: &str) -> (Vec<u8>, std::path::PathBuf) {
    const AI: &str = r#"<svg viewBox="0 0 24 24"><defs><style>.cls-1{fill:#000}</style></defs><path class="cls-1" d="M2 2h20v20H2Z"/></svg>"#;
    const COLOURS: &str = r##"<svg viewBox="0 0 24 24"><defs><style>.red{fill:#ff0000}.blue{fill:#0000ff}</style></defs><g transform="translate(2 2)" opacity="0.5"><path class="red" d="M0 0h10v20H0Z"/><path class="blue" d="M10 0h10v20H10Z"/></g></svg>"##;
    let dir = std::env::temp_dir().join(format!("exact-file-icons-{case}-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    for (name, bytes) in [
        ("ai", AI),
        ("colours", COLOURS),
        (
            "rectangle",
            r##"<svg viewBox="0 0 40 20"><style>.red{fill:#ff0000}</style><circle class="red" cx="20" cy="10" r="8"/></svg>"##,
        ),
        (
            "template",
            r#"<svg viewBox="0 0 24 24"><path d="M2 2h20v20H2Z"/></svg>"#,
        ),
    ] {
        std::fs::write(dir.join(format!("assets/{name}.svg")), bytes).unwrap();
    }
    std::fs::write(
        dir.join("app.json"),
        r#"{"app":{"id":"dev.test.file-icons","name":"File icons"},"host":{"web":{},"linux":{}}}"#,
    )
    .unwrap();
    std::fs::write(
        dir.join("app.tamber"),
        "tamber t\n\nicons\n  icon.ai = icon(svg \"ai.svg\")\n  icon.colours = icon(svg \"colours.svg\")\n  icon.rectangle = icon(svg \"rectangle.svg\")\n  icon.template = icon(svg \"template.svg\")\n",
    )
    .unwrap();
    let entry = dir.join("app.contract");
    std::fs::write(&entry, source).unwrap();
    let plan = contract::compile_path(&entry).unwrap_or_else(|e| panic!("{e}"));
    (plan.encode(), dir)
}

#[test]
fn file_icons_draw_the_svg_scene_and_templates_keep_their_tint() {
    let source = r##"component App
  state chosen = "symbol:icon.colours"
  action change
    chosen = chosen == "symbol:icon.template" ? "symbol:icon.ai" : "symbol:icon.template"
  view
    column width=96 background-color="#ffffff"
      row
        image "symbol:icon.ai" testId="ai" width=48 height=48 color="#00ff00"
        svg width=48 height=48 viewBox="0 0 24 24"
          path fill="#000000" d="M2 2h20v20H2Z"
      row
        image chosen testId="chosen" width=48 height=48 color="#00ff00"
        svg width=48 height=48 viewBox="0 0 24 24"
          g transform="translate(2 2)" opacity=0.5
            path fill="#ff0000" d="M0 0h10v20H0Z"
            path fill="#0000ff" d="M10 0h10v20H10Z"
      row
        image "symbol:icon.template" testId="template" width=48 height=48 color="#00ff00"
        svg width=48 height=48 viewBox="0 0 24 24"
          path fill="#00ff00" d="M2 2h20v20H2Z"
      row
        button appearance="auto" aria-label="Illustrator" width=48 height=48
          image "symbol:icon.ai" font-size=48 -exact-tint-color="#00ff00"
        svg width=48 height=48 viewBox="0 0 24 24"
          path fill="#000000" d="M2 2h20v20H2Z"
      button press=change testId="change"
        text "Change"
"##;
    let (plan, dir) = file_icons_app(source, "paint");
    let (mut p, error) =
        Presenter::boot_with(&plan, (), (96., 250.), 1., dir.clone(), PainterChoice::Cpu).unwrap();
    assert!(error.is_none(), "{error:?}");
    let wait = |p: &mut Presenter<()>, pictures| {
        p.wait_images(std::time::Duration::from_secs(30));
        assert_eq!(
            p.images().bitmaps.len(),
            pictures,
            "file icons did not load"
        );
    };
    wait(&mut p, 3);
    let crop = |p: &mut Presenter<()>, x: u32, y: u32| {
        let frame = p.frame();
        (0..48)
            .flat_map(|dy| (0..48).map(move |dx| (x + dx, y + dy)))
            .map(|(x, y)| frame.pixel(x, y).unwrap())
            .collect::<Vec<_>>()
    };
    for (row, name) in [
        (0, "Illustrator"),
        (48, "colours, transform and opacity"),
        (96, "template tint"),
    ] {
        let actual = crop(&mut p, 0, row);
        let expected = crop(&mut p, 48, row);
        // The file's translucent scene is composited through an 8-bit
        // bitmap, rounding half opacity to 128 before painting on white.
        let tolerance = u8::from(row == 48);
        let different = actual
            .iter()
            .zip(expected)
            .filter(|(a, b)| {
                [
                    a.red().abs_diff(b.red()),
                    a.green().abs_diff(b.green()),
                    a.blue().abs_diff(b.blue()),
                    a.alpha().abs_diff(b.alpha()),
                ]
                .into_iter()
                .any(|delta| delta > tolerance)
            })
            .count();
        assert_eq!(different, 0, "{name}: differing pixels");
    }
    let pixel = |p: &mut Presenter<()>, x, y| {
        let c = p.frame().pixel(x, y).unwrap().demultiply();
        (c.red(), c.green(), c.blue(), c.alpha())
    };
    assert_eq!(pixel(&mut p, 12, 12), (0, 0, 0, 255));
    assert_eq!(pixel(&mut p, 12, 108), (0, 255, 0, 255));
    let native = crop(&mut p, 0, 144);
    assert!(
        native
            .iter()
            .filter(|c| c.red() == 0 && c.green() == 0 && c.blue() == 0)
            .count()
            > 100,
        "a native button draws the file in its platform-sized symbol box"
    );
    assert!(
        native
            .iter()
            .all(|c| c.red() != 0 || c.green() != 255 || c.blue() != 0),
        "a native button keeps the file's paint"
    );
    let k = p.host().kernel();
    let change = k.node_by_key(k.find_by_test_id("change")[0]).unwrap().id;
    p.tap(change).unwrap();
    wait(&mut p, 2);
    assert_eq!(crop(&mut p, 0, 48), crop(&mut p, 0, 96), "file to template");
    p.tap(change).unwrap();
    wait(&mut p, 3);
    assert_eq!(crop(&mut p, 0, 48), crop(&mut p, 0, 0), "template to file");
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn file_icons_in_native_buttons_resolve_rectangular_svgs_at_the_drawn_size() {
    for scale in [1., 2.] {
        let (plan, dir) = file_icons_app(
            r##"component App
  state large = false
  action grow
    large = !large
  view
    column width=192 background-color="#ffffff"
      row
        button appearance="auto" aria-label="Circle" testId="button" width=96 height=96 press=grow
          image "symbol:icon.rectangle" testId="icon" font-size=(large ? 48 : 24)
        box width=96 height=96 display="flex" align-items="center" justify-content="center"
          svg testId="reference" width=(large ? 48 : 24) height=(large ? 48 : 24) viewBox="0 0 40 20"
            circle cx=20 cy=10 r=8 fill="#ff0000"
"##,
            &format!("button-rectangle-{scale}"),
        );
        let (mut p, error) = Presenter::boot_with(
            &plan,
            (),
            (192., 96.),
            scale,
            dir.clone(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let k = p.host().kernel();
        let id = |name| k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
        let (button, icon, reference) = (id("button"), id("icon"), id("reference"));
        for size in [24., 48.] {
            p.wait_images(std::time::Duration::from_secs(30));
            let (x, y, w, h) = p.rect_of(button).unwrap();
            let expected = p.rect_of(reference).unwrap();
            let origin = ((x + (w - size) / 2.) * scale, (y + (h - size) / 2.) * scale);
            let frame = p.frame();
            let red = |x, y| {
                let c = frame.pixel(x, y).unwrap();
                c.red().saturating_sub(c.green().max(c.blue())) > 127
            };
            let mut ink = 0;
            for dy in 0..(size * scale) as u32 {
                for dx in 0..(size * scale) as u32 {
                    let actual = red(origin.0 as u32 + dx, origin.1 as u32 + dy);
                    let expected = red(
                        (expected.0 * scale) as u32 + dx,
                        (expected.1 * scale) as u32 + dy,
                    );
                    ink += usize::from(expected);
                    assert_eq!(actual, expected, "{size}px at {scale}x, ({dx}, {dy})");
                }
            }
            assert!(ink > 40, "the reference circle has ink");
            let bitmap = &p.images().bitmaps[&icon];
            let pixels = (size * scale) as u32;
            assert_eq!((bitmap.width(), bitmap.height()), (pixels, pixels));
            if size == 24. {
                p.tap(button).unwrap();
            }
        }
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[test]
fn file_icons_keep_their_natural_size_after_a_colour_update() {
    let (plan, dir) = file_icons_app(
        r##"component App
  state green = false
  action change
    green = !green
  view
    column align-items="flex-start"
      image "symbol:icon.ai" testId="icon" font-size=16 color=(green ? "#00ff00" : "#ff0000")
      button "Change" testId="change" press=change
"##,
        "size",
    );
    let (mut p, error) =
        Presenter::boot_with(&plan, (), (100., 100.), 1., dir.clone(), PainterChoice::Cpu).unwrap();
    assert!(error.is_none(), "{error:?}");
    p.wait_images(std::time::Duration::from_secs(30));
    let k = p.host().kernel();
    let image = k.node_by_key(k.find_by_test_id("icon")[0]).unwrap().id;
    let change = k.node_by_key(k.find_by_test_id("change")[0]).unwrap().id;
    let size = |p: &Presenter<()>| {
        let frame = p.host().kernel().node(image).unwrap().frame;
        (frame.width, frame.height)
    };
    assert_eq!(size(&p), (24., 24.));
    p.tap(change).unwrap();
    p.wait_images(std::time::Duration::from_secs(30));
    assert_eq!(
        size(&p),
        (24., 24.),
        "an unrelated update keeps the file's size"
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn file_icons_are_found_in_the_layout_reply() {
    let (plan, dir) = file_icons_app(
        "component App\n  view\n    image \"symbol:icon.ai\" testId=\"icon\" width=24 height=24\n",
        "layout",
    );
    let (mut p, error) =
        Presenter::boot_with(&plan, (), (100., 100.), 1., dir.clone(), PainterChoice::Cpu).unwrap();
    assert!(error.is_none(), "{error:?}");
    p.wait_images(std::time::Duration::from_secs(30));
    let k = p.host().kernel();
    let image = k.node_by_key(k.find_by_test_id("icon")[0]).unwrap().id;
    let reply: serde_json::Value =
        serde_json::from_str(&p.layout_json(Some(image), false)).unwrap();
    assert_eq!(reply["node"]["native"]["symbol"]["found"], true, "{reply}");
    assert!(
        reply["node"]["native"]["symbol"]["reason"].is_null(),
        "{reply}"
    );
    std::fs::remove_dir_all(dir).unwrap();
}
