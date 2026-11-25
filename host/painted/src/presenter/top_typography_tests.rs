//! Surface typography and its cached presentation.
use super::look::{Inputs, Layout};
use super::*;

fn near(a: f32, b: f32) -> bool {
    (a - b).abs() < 0.01
}

#[test]
fn a_surface_follows_the_scheme_the_font_scale_and_a_reader_made_again() {
    let (mut p, pictures) = windowed();
    tap(&mut p, "ask").unwrap();
    let serial = p.surface().unwrap().serial;
    let first = told(&mut p)[0];
    let light = pictures.borrow().last().unwrap().pixel(20, 20).unwrap();
    // Night mode with the alert open: the same surface, painted again.
    let phone = Device {
        night: true,
        font_scale: 1.0,
        orientation: None,
        screen_reader: exact_runner::ScreenReader::Unknown,
        locale: "en-US".into(),
        rtl: false,
        hour24: false,
        reduced_motion: false,
        bold_text: false,
        high_contrast: false,
        touch: true,
        tv: false,
        touch_slop: 8.0,
        dates: String::new(),
        format_locale: String::new(),
    };
    assert!(p.set_device(&phone).is_none());
    let dark = told(&mut p);
    assert_eq!(dark.len(), 1);
    assert_eq!((dark[0].0, dark[0].1, dark[0].2), (serial, 1, first.2));
    assert_ne!(
        pictures.borrow().last().unwrap().pixel(20, 20).unwrap(),
        light
    );
    // A larger font scale: taller, the same surface still.
    assert!(p
        .set_device(&Device {
            font_scale: 2.0,
            ..phone.clone()
        })
        .is_none());
    let large = told(&mut p);
    assert_eq!((large[0].0, large[0].1), (serial, 1));
    assert!(large[0].2[3] > first.2[3], "{large:?} {first:?}");
    // A reader made again is told the open surface anew, by its serial.
    p.reader_replaced(Box::new(crate::raster::Raster::new()));
    let again = told(&mut p);
    assert_eq!((again.len(), again[0].0, again[0].1), (1, serial, 1));
    // The shell could not make it: it closes, the page is live again.
    p.surface_event(serial, event::REFUSED, 0.0, 0.0, 0.0);
    assert!(p.surface().is_none());
    assert_eq!(says(&p, "was refused"), 1);
    tap(&mut p, "poke").unwrap();
    assert_eq!(text(&p, "said"), "poked");
}

#[test]
fn material_surfaces_refresh_when_bold_text_changes_in_both_directions() {
    for trigger in ["ask", "more"] {
        let (mut p, pictures) = windowed();
        let mut device = Device {
            night: false,
            font_scale: 1.0,
            orientation: None,
            screen_reader: exact_runner::ScreenReader::Unknown,
            locale: "en-US".into(),
            rtl: false,
            hour24: false,
            reduced_motion: false,
            bold_text: false,
            high_contrast: false,
            touch: true,
            tv: false,
            touch_slop: 8.0,
            dates: String::new(),
            format_locale: String::new(),
        };
        assert!(p.set_device(&device).is_none());
        tap(&mut p, trigger).unwrap();
        let first = told(&mut p)[0];
        let mut shots = vec![pictures.borrow().last().unwrap().clone()];
        for bold_text in [true, false] {
            device.bold_text = bold_text;
            assert!(p.set_device(&device).is_none());
            let changed = told(&mut p);
            assert_eq!(changed.len(), 1, "{trigger}, bold text {bold_text}");
            assert_eq!((changed[0].0, changed[0].1), (first.0, first.1));
            shots.push(pictures.borrow().last().unwrap().clone());
            assert!(told(&mut p).is_empty(), "the refreshed surface is cached");
        }
        assert!(shots[0].data() != shots[1].data(), "{trigger} becomes bold");
        assert!(shots[0].data() == shots[2].data(), "{trigger} is restored");
        if trigger == "more" {
            let crop = |shot: &Pixmap| {
                shot.clone_rect(tiny_skia::IntRect::from_xywh(12, 20, 24, 24).unwrap())
                    .unwrap()
            };
            // Material's filled `edit` is a vector, as Compose's icon is, and
            // Bold Text weights only text.
            let symbol = crop(&shots[0]);
            assert!(
                symbol.pixels().iter().any(|p| *p != symbol.pixels()[0]),
                "the symbol is drawn"
            );
            assert!(symbol.data() == crop(&shots[1]).data(), "the symbol");
        }
    }
}

#[test]
fn the_dialog_and_the_menu_keep_materials_metrics() {
    let engine = crate::text::fixtures::shared();
    let inputs = |width: f32| Inputs {
        direction: exact_kernel::Direction::Ltr,
        contrast: None,
        bold_text: false,
        dark: false,
        font_scale: 1.0,
        window: (width, 800.0),
        room: 792.0,
    };
    let row = |title: &str, cancel: bool| Row {
        node: 1,
        title: title.into(),
        menu_label: None,
        subtitle: None,
        direction: exact_kernel::Direction::Ltr,
        symbol: None,
        destructive: false,
        enabled: true,
        cancel,
        checked: false,
        opens: None,
    };
    let alert = |rows: Vec<Row>| Plan::Alert {
        font_family: None,
        title: Some("Title".into()),
        title_direction: exact_kernel::Direction::Ltr,
        message: Some("Message".into()),
        message_directions: vec![exact_kernel::Direction::Ltr],
        rows,
        cancellable: true,
        outside: false,
    };
    let two = alert(vec![row("Cancel", true), row("Delete", false)]);
    // 280 to 560 wide, 24 from the window's sides.
    for (window, wide) in [
        (300.0, 280.0),
        (400.0, 352.0),
        (1000.0, 560.0),
        (200.0, 200.0),
    ] {
        let layout = Layout::of(&mut engine.borrow_mut(), &two, 1, &inputs(window));
        assert_eq!(layout.size.0, wide, "a {window} wide window");
    }
    // The buttons end 24 from the trailing edge, 8 apart, each at least 40
    // tall, the cancel first.
    let layout = Layout::of(&mut engine.borrow_mut(), &two, 1, &inputs(400.0));
    let (cancel, delete) = (layout.targets[0].rect, layout.targets[1].rect);
    assert!(near(delete.0 + delete.2, 352.0 - 24.0), "{delete:?}");
    assert!(
        near(cancel.0 + cancel.2 + 8.0, delete.0),
        "{cancel:?} {delete:?}"
    );
    assert!(cancel.3 >= 40.0 && delete.3 >= 40.0);
    assert_eq!(layout.size.1, delete.1 + delete.3 + 24.0);
    assert_eq!((layout.radius, layout.elevation), (28.0, 0.0));
    // Three long ones take a second line.
    let long = alert(vec![
        row("Cancel", true),
        row("Save everything and go on", false),
        row("Throw everything away", false),
    ]);
    let layout = Layout::of(&mut engine.borrow_mut(), &long, 1, &inputs(400.0));
    let lines: std::collections::BTreeSet<i32> =
        layout.targets.iter().map(|t| t.rect.1 as i32).collect();
    assert!(lines.len() > 1, "{:?}", layout.targets);
    assert!(layout
        .targets
        .iter()
        .all(|t| t.rect.0 >= 24.0 && t.rect.0 + t.rect.2 <= 352.0 - 24.0 + 0.01));
    // A menu's place: under its anchor from its start; over it where the
    // window ends; ending at its end where the window's side is near.
    let (window, margins, size) = ((400.0, 800.0), (48.0, 48.0), (200.0, 217.0));
    assert_eq!(
        look::place(
            (20.0, 100.0, 40.0, 40.0),
            size,
            window,
            margins,
            exact_kernel::Direction::Ltr
        ),
        (20.0, 140.0)
    );
    assert_eq!(
        look::place(
            (20.0, 700.0, 40.0, 40.0),
            size,
            window,
            margins,
            exact_kernel::Direction::Ltr
        ),
        (20.0, 483.0)
    );
    assert_eq!(
        look::place(
            (340.0, 100.0, 40.0, 40.0),
            size,
            window,
            margins,
            exact_kernel::Direction::Ltr
        ),
        (180.0, 140.0)
    );
    assert_eq!(
        look::place(
            (340.0, 700.0, 40.0, 40.0),
            size,
            window,
            margins,
            exact_kernel::Direction::Ltr
        ),
        (180.0, 483.0)
    );
    assert_eq!(
        look::place(
            (20.0, 10.0, 40.0, 20.0),
            size,
            window,
            margins,
            exact_kernel::Direction::Ltr
        ),
        (20.0, 48.0),
        "never under the status bar"
    );
}

#[test]
fn alert_horizontal_keys_follow_visual_order_without_changing_tab_order() {
    for desktop in [false, true] {
        for direction in ["ltr", "rtl"] {
            let source = APP.replace(
                "dialog id=\"three\"",
                &format!("dialog direction=\"{direction}\" id=\"three\""),
            );
            let (mut p, error) = Presenter::boot_with_fixture_fonts(
                &crate::test_plan(&source),
                (),
                (400., 800.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            if desktop {
                p.serve_overlays();
            } else {
                p.serve_windows(Box::new(Pixels(Rc::new(RefCell::new(Vec::new())))));
            }
            p.frame();
            tap(&mut p, "ask-three").unwrap();
            let targets = p.top.open.as_ref().unwrap().layout.targets.clone();
            let mut visual: Vec<_> = targets.iter().collect();
            visual.sort_by(|a, b| a.rect.0.total_cmp(&b.rect.0));
            assert!(visual.iter().all(|t| near(t.rect.1, visual[1].rect.1)));
            for (key, expected) in [
                ("ArrowLeft", visual[0].node),
                ("ArrowRight", visual[2].node),
            ] {
                assert!(p.surface_focus(visual[1].node));
                assert!(p.surface_key(key, 1.));
                let open = p.top.open.as_ref().unwrap();
                assert_eq!(
                    open.layout.targets[open.focused.unwrap()].node,
                    expected,
                    "{direction}, desktop={desktop}, {key}"
                );
            }
            assert!(p.surface_focus(id(&p, "three-save")));
            assert!(p.surface_key("Tab", 2.));
            let open = p.top.open.as_ref().unwrap();
            assert_eq!(
                open.layout.targets[open.focused.unwrap()].node,
                id(&p, "three-later")
            );
        }
    }
}

#[test]
fn an_alerts_accessible_message_spans_all_directed_paragraphs() {
    let source = APP.replace(
        "text \"It cannot be undone.\"",
        "text \"It cannot be undone.\" direction=\"ltr\"\n        text \"رسالة أخرى\" direction=\"rtl\"",
    );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_windows(Box::new(Pixels(Rc::new(RefCell::new(Vec::new())))));
    p.frame();
    tap(&mut p, "ask").unwrap();
    let (nodes, _, _) = p.access_tree();
    let message = nodes
        .iter()
        .find(|n| n.name == "It cannot be undone.\nرسالة أخرى")
        .unwrap();
    assert_eq!(message.rect[3], 40.);
    assert!(near(message.rect[0], 24.));
    let right = p.surface().unwrap().size.0 - 24.;
    assert!(near(message.rect[0] + message.rect[2], right));
}

#[test]
fn short_rtl_alert_texts_have_accessibility_bounds_at_the_right() {
    let source = APP
        .replace(
            "aria-label=\"Delete this?\"",
            "aria-label=\"عنوان\" direction=\"rtl\"",
        )
        .replace(
            "text \"It cannot be undone.\"",
            "text \"نعم\"\n        text \"لا\"",
        );
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_windows(Box::new(Pixels(Rc::new(RefCell::new(Vec::new())))));
    p.frame();
    tap(&mut p, "ask").unwrap();
    let width = p.surface().unwrap().size.0;
    let (nodes, _, _) = p.access_tree();
    use crate::presenter::access::role;
    for (name, role) in [("عنوان", role::HEADING), ("نعم\nلا", role::TEXT)] {
        let text = nodes
            .iter()
            .find(|n| n.role == role && n.name == name)
            .unwrap();
        assert!(text.rect[0] > width / 2., "{name}: {:?}", text.rect);
        assert!(
            (text.rect[0] + text.rect[2] - (width - 24.)).abs() < 1.,
            "{name}: {:?}",
            text.rect
        );
        assert_eq!(text.rect[3], if name == "عنوان" { 32. } else { 20. });
    }
}

/// A finger on a Material surface's row ripples in its state layer's shape
/// (LLP 1115.001 D9), as the page's and the bars' targets do: nothing at
/// the touch's instant, the wave at rest after 450 ms, the surface told
/// again at each new instant while it moves and once at rest, and as it
/// was once the wave has left after a lift outside the row.
#[test]
fn a_menu_row_and_an_alert_button_ripple_and_are_told_at_each_instant() {
    for (trigger, row) in [("more", "menu-share"), ("ask", "confirm-cancel")] {
        let (mut p, pictures) = windowed();
        tap(&mut p, trigger).unwrap();
        assert_eq!(told(&mut p).len(), 1);
        let last = || pictures.borrow().last().unwrap().clone();
        let rest = last();
        let node = id(&p, row);
        let shown = p.surface().unwrap();
        let serial = shown.serial;
        let (x, y, w, h) = shown.targets.iter().find(|t| t.node == node).unwrap().rect;
        p.clock(1000.);
        p.surface_event(serial, 0, x + w / 2.0, y + h / 2.0, 1000.);
        assert_eq!(told(&mut p).len(), 1, "{trigger}: pressed");
        assert_eq!(last().data(), rest.data(), "{trigger}: nothing at first");
        assert!(
            p.needs_animation_frame(),
            "{trigger}: the ripple owes frames"
        );
        assert!(told(&mut p).is_empty(), "{trigger}: the same instant once");
        p.clock(1500.);
        assert_eq!(told(&mut p).len(), 1, "{trigger}: a new instant");
        let held = last();
        assert_ne!(held.data(), rest.data(), "{trigger}: the wave at rest");
        assert!(!p.needs_animation_frame(), "{trigger}: at rest");
        // Lifted outside the row: it chooses nothing, and the wave leaves.
        p.surface_event(serial, 2, 0.5, 0.5, 1500.);
        p.surface_event(serial, 1, 0.5, 0.5, 1500.);
        assert!(p.surface().is_some(), "{trigger}: nothing chosen");
        assert!(p.needs_animation_frame());
        p.clock(1700.);
        assert_eq!(told(&mut p).len(), 1, "{trigger}: leaving");
        p.clock(1900.);
        assert_eq!(told(&mut p).len(), 1, "{trigger}: gone");
        assert_eq!(last().data(), rest.data(), "{trigger}: as it was");
        assert!(!p.needs_animation_frame());
        assert!(told(&mut p).is_empty());
    }
}
