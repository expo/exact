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
            assert!(
                crop(&shots[0]).data() != crop(&shots[1]).data(),
                "the symbol"
            );
        }
    }
}

#[test]
fn the_dialog_and_the_menu_keep_materials_metrics() {
    let engine = crate::text::TextEngine::shared();
    let inputs = |width: f32| Inputs {
        contrast: None,
        bold_text: false,
        dark: false,
        font_scale: 1.0,
        window: (width, 800.0),
    };
    let row = |title: &str, cancel: bool| Row {
        node: 1,
        title: title.into(),
        symbol: None,
        destructive: false,
        enabled: true,
        cancel,
        checked: false,
        opens: None,
    };
    let alert = |rows: Vec<Row>| Plan::Alert {
        title: Some("Title".into()),
        message: Some("Message".into()),
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
        look::place((20.0, 100.0, 40.0, 40.0), size, window, margins),
        (20.0, 140.0)
    );
    assert_eq!(
        look::place((20.0, 700.0, 40.0, 40.0), size, window, margins),
        (20.0, 483.0)
    );
    assert_eq!(
        look::place((340.0, 100.0, 40.0, 40.0), size, window, margins),
        (180.0, 140.0)
    );
    assert_eq!(
        look::place((340.0, 700.0, 40.0, 40.0), size, window, margins),
        (180.0, 483.0)
    );
    assert_eq!(
        look::place((20.0, 10.0, 40.0, 20.0), size, window, margins),
        (20.0, 48.0),
        "never under the status bar"
    );
}
