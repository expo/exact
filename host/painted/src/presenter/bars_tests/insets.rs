use super::*;

const ROUTE: &str = r#"component App
  view
    main navigationKey="home" navigationBack="" width="100%" height="100%" viewport-fit="cover"
      column navigationKey="home" testId="route" position="absolute" inset=0
        header
          text "Title" role="heading" aria-level=2
        scroll flex=1
          column padding=16
            box testId="card" height=80 width="100%"
"#;

#[test]
fn a_covered_routes_content_clears_only_the_sides_it_leaves_to_the_host() {
    for (style, clear_left, clear_right) in [
        ("", true, true),
        (r#"-exact-safe-area="left""#, false, true),
        (r#"-exact-safe-area="right""#, true, false),
        (
            r#"padding-left="env(safe-area-inset-left)" padding-right="var(--exact-safe-area-inset-right)""#,
            true,
            true,
        ),
        (r#"navigationBar="none""#, true, true),
    ] {
        let source = ROUTE.replace("testId=\"route\"", &format!("testId=\"route\" {style}"));
        let (mut p, _) = barred(&source);
        for (left, right) in [(52., 0.), (0., 52.), (0., 0.)] {
            let window = Window {
                size: (900., 400.),
                cutout: [0., right, 0., left],
                ..WINDOW
            };
            assert_eq!(p.set_window(window), None);
            let frame = |name| p.host().kernel().node(id(&p, name)).unwrap().frame;
            assert_eq!((frame("route").x, frame("route").width), (0., 900.));
            let start = if clear_left { left } else { 0. };
            let end = if clear_right { right } else { 0. };
            let card = frame("card");
            assert_eq!(
                (card.x, card.width),
                (16. + start, 868. - start - end),
                "{style}, cutout {:?}",
                window.cutout
            );
        }
    }
}

#[test]
fn a_route_only_clears_the_side_band_that_reaches_its_box() {
    let source = ROUTE.replace("inset=0", "inset=0 left=80 right=24");
    let (mut p, _) = barred(&source);
    for (left, right, x, end) in [(52., 0., 96., 860.), (0., 52., 96., 832.)] {
        assert_eq!(
            p.set_window(Window {
                size: (900., 400.),
                cutout: [0., right, 0., left],
                ..WINDOW
            }),
            None
        );
        let card = p.host().kernel().node(id(&p, "card")).unwrap().frame;
        assert_eq!((card.x, card.x + card.width), (x, end));
    }
}

#[test]
fn positioning_a_route_by_its_local_inset_clears_the_band_once() {
    let source = ROUTE.replace(
        "inset=0",
        r#"inset=0 left="var(--exact-safe-area-inset-left)""#,
    );
    let (mut p, _) = barred(&source);
    assert_eq!(
        p.set_window(Window {
            size: (900., 400.),
            cutout: [0., 0., 0., 52.],
            ..WINDOW
        }),
        None
    );
    assert_eq!(p.rect_of(id(&p, "route")).unwrap().0, 52.);
    assert_eq!(p.rect_of(id(&p, "card")).unwrap().0, 68.);
}

#[test]
fn a_centered_intrinsic_route_settles_against_the_cutout() {
    let source = ROUTE
        .replace(
            "viewport-fit=",
            r#"display="flex" flex-direction="column" align-items="center" viewport-fit="#,
        )
        .replace(r#"position="absolute" inset=0"#, r#"width="max-content""#)
        .replace(
            "        scroll flex=1",
            "        box width=820 height=1\n        scroll flex=1",
        )
        .replace("column padding=16", "column");
    let (mut p, _) = barred(&source);
    assert_eq!(
        p.set_window(Window {
            size: (900., 400.),
            cutout: [0., 0., 0., 52.],
            ..WINDOW
        }),
        None
    );
    let x = p.rect_of(id(&p, "card")).unwrap().0;
    assert!((x - 52.).abs() <= 0.5, "card starts at {x}");
}

#[test]
fn a_sheet_clears_only_the_cutout_that_reaches_its_sides() {
    let source = format!("{ROUTE}      column navigationKey=\"sheet\" navigationPresentation=\"modal\" testId=\"sheet\"\n        box testId=\"child\" height=20\n")
        .replacen("navigationKey=\"home\"", "navigationKey=\"sheet\"", 1);
    let (mut p, _) = barred(&source);
    for (width, clearance) in [(600., 52.), (700., 22.), (800., 0.)] {
        assert_eq!(
            p.set_window(Window {
                size: (width, 400.),
                cutout: [0., 0., 0., 52.],
                ..WINDOW
            }),
            None
        );
        let k = p.host().kernel();
        let sheet = id(&p, "sheet");
        assert_eq!(k.local_insets(sheet).unwrap()[3], clearance);
        let child = k.node(id(&p, "child")).unwrap().frame;
        assert_eq!(child.x, clearance);
        assert_eq!(
            p.rect_of(id(&p, "child")).unwrap().0,
            (width - 640.).max(0.) / 2. + clearance
        );
    }
}

#[test]
fn a_rotation_that_only_swaps_the_cutout_repaints_the_idle_route() {
    let source = ROUTE.replace("height=80", r##"background-color="#2468ac" height=80"##);
    let (mut p, _) = barred(&source);
    let mut window = Window {
        size: (900., 400.),
        cutout: [0., 0., 0., 52.],
        ..WINDOW
    };
    assert_eq!(p.set_window(window), None);
    let left = p.frame();
    let y = p.rect_of(id(&p, "card")).unwrap().1 as u32 + 8;
    let blue = left.pixel(80, y).unwrap();
    assert_ne!(left.pixel(25, y).unwrap(), blue);
    let before = p.still().expect("the route is idle");
    window.cutout = [0., 52., 0., 0.];
    assert_eq!(p.set_window(window), None);
    assert!(p.dirty(), "a new frame is due without an input or commit");
    assert!(
        p.still().is_some_and(|now| now != before),
        "a scroll-only frame cannot move the cutout"
    );
    assert_eq!(p.frame().pixel(25, y).unwrap(), blue);
}
