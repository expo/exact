//! The display loop's input dispatch, testable without a DRM master.
use crate::input::InputEvent;
use exact_painted::Presenter;
use exact_runner::DataSource;
use std::time::{Duration, SystemTime};

pub(super) struct Pointer {
    position: (f32, f32),
    previous: Option<(SystemTime, (f32, f32), u32)>,
}

impl Pointer {
    pub(super) fn new(position: (f32, f32)) -> Self {
        Self {
            position,
            previous: None,
        }
    }

    // The standalone evdev and VNC display has no window toolkit. Use GtkSettings'
    // default multi-click distance (logical pixels) and interval (milliseconds).
    fn moved(&mut self) {
        self.previous = self.previous.filter(|(_, (x, y), _)| {
            (self.position.0 - x).abs() <= 5. && (self.position.1 - y).abs() <= 5.
        });
    }

    fn clicks(&mut self, at: SystemTime) -> u32 {
        let count = self
            .previous
            .filter(|(last, _, _)| {
                at.duration_since(*last)
                    .is_ok_and(|d| d <= Duration::from_millis(400))
            })
            .map_or(1, |(_, _, n)| n % 3 + 1);
        self.previous = Some((at, self.position, count));
        count
    }
}

pub(super) fn dispatch<D: DataSource>(
    p: &mut Presenter<D>,
    mouse: &mut Pointer,
    viewport: (f32, f32),
    scale: f32,
    event: InputEvent,
    now_ms: f64,
) -> Result<(), String> {
    if !matches!(
        event,
        InputEvent::Key { .. } | InputEvent::Pad { .. } | InputEvent::Cancel
    ) && !p.display_input_mapping(viewport, scale)
    {
        // An uncertified physical release must end capture without executing
        // a drop. Refusing its coordinates must not strand a hold or item pin.
        mouse.previous = None;
        if matches!(event, InputEvent::ButtonUp | InputEvent::Aux(_, false)) {
            p.pointer_lost(now_ms)?;
        }
        return Err("pointer mapping does not match the acknowledged surface".into());
    }
    let pointer = &mut mouse.position;
    match event {
        InputEvent::Motion(dx, dy) => {
            pointer.0 = (pointer.0 + dx / scale).clamp(0., viewport.0 - 1.);
            pointer.1 = (pointer.1 + dy / scale).clamp(0., viewport.1 - 1.);
            p.set_pointer(Some(*pointer));
            // A canvas sees the device's motion, unbounded by the screen edge.
            p.raw_motion(dx / scale, dy / scale);
            p.pointer_move(pointer.0, pointer.1, now_ms)?;
            mouse.moved();
        }
        InputEvent::Absolute(fx, fy) => {
            let previous = *pointer;
            if let Some(fx) = fx {
                pointer.0 = (fx * viewport.0).clamp(0., viewport.0 - 1.);
            }
            if let Some(fy) = fy {
                pointer.1 = (fy * viewport.1).clamp(0., viewport.1 - 1.);
            }
            p.set_pointer(Some(*pointer));
            if *pointer != previous {
                p.pointer_move(pointer.0, pointer.1, now_ms)?;
                mouse.moved();
            }
        }
        InputEvent::MouseDown(at) => {
            let (x, y) = *pointer;
            p.pointer_down_with_clicks(x, y, now_ms, mouse.clicks(at))?;
        }
        InputEvent::TouchDown => {
            mouse.previous = None;
            p.pointer_down(pointer.0, pointer.1, now_ms)?;
        }
        InputEvent::ButtonUp => {
            p.pointer_up(pointer.0, pointer.1, now_ms)?;
        }
        InputEvent::Aux(bit, down) => {
            mouse.previous = None;
            p.pointer_aux(bit, down, pointer.0, pointer.1, now_ms)
        }
        InputEvent::Cancel => {
            mouse.previous = None;
            p.pointer_lost(now_ms)?;
        }
        InputEvent::Wheel(dx, dy) => {
            mouse.previous = None;
            p.wheel_at(pointer.0, pointer.1, dx, dy);
        }
        // The report's own time, as the read converted it.
        InputEvent::Pad {
            index,
            connected,
            reading,
            at,
        } => p.hardware_pad(index, connected, reading.buttons, reading.axes, at),
        // The same targeted path as VNC and agent keys: text, Escape blur and a
        // canvas's held keys (`hardware_key`). Escape also abandons the pointer's
        // contact; the button stays down, and a replacement waits for its release.
        InputEvent::Key {
            code,
            shift,
            down,
            repeat,
        } => {
            let Some((code, key)) = crate::input::key(code, shift) else {
                return Ok(());
            };
            if code == "Escape" && down {
                p.pointer_abandoned(now_ms)?;
            }
            p.hardware_key(code, key, down, repeat);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_kernel::PropId;
    use exact_painted::presenter::PainterChoice;
    use std::path::PathBuf;
    /// An evdev key press (1 Escape, 45 x).
    fn press(code: u16) -> InputEvent {
        InputEvent::Key {
            code,
            shift: false,
            down: true,
            repeat: false,
        }
    }
    fn fixture() -> Presenter<()> {
        let plan = contract::compile(r#"component App
  state count = 0
  action reply
    count = count + 1
  view
    column
      box testId="row" width=400 height=100 swipe=reply swipe-follow="right" touch-action="pan-y" transition="translate -exact-spring(300, 30, 1)"
        text "swipe"
      text `${count}` testId="count"
"#).unwrap();
        Presenter::boot_with(
            &plan.encode(),
            (),
            (400., 500.),
            1.,
            PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
            PainterChoice::Cpu,
        )
        .unwrap()
        .0
    }
    fn count(p: &Presenter<()>) -> &str {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id("count")[0])
            .unwrap()
            .props
            .str(PropId::Text)
            .unwrap()
    }
    #[test]
    fn repeated_mouse_clicks_select_and_copy_the_rendered_word_and_paragraph() {
        use exact_painted::presenter::clipboard::Clipboard;
        use std::{cell::RefCell, rc::Rc};

        struct CopyBuffer(Rc<RefCell<String>>);
        impl Clipboard for CopyBuffer {
            fn read_text(&mut self) -> Result<Option<String>, String> {
                Ok(Some(self.0.borrow().clone()))
            }
            fn write_text(&mut self, text: &str) -> Result<(), String> {
                *self.0.borrow_mut() = text.into();
                Ok(())
            }
        }
        let plan = contract::compile(r#"component App
  state selected = ""
  action remember(s: Selection)
    selected = s.text
  view
    column
      text "One.\u2029Hello **world**. More words." testId="reader" markup="markdown" user-select="text" selectionchange=remember
        width=120 font-size=18
      text `Selection: ${selected}` testId="count"
"#).unwrap().encode();
        let viewport = (320., 200.);
        let (mut p, err) = Presenter::boot_with(
            &plan,
            (),
            viewport,
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(err.is_none());
        let copied = Rc::new(RefCell::new(String::new()));
        p.set_clipboard(Box::new(CopyBuffer(copied.clone())));
        let clear = p.frame();
        let kernel = p.host().kernel();
        let id = kernel
            .node_by_key(kernel.find_by_test_id("reader")[0])
            .unwrap()
            .id;
        let paragraph = p.paragraph(id).unwrap();
        // Click inside the glyph in "world" even when the chosen font wraps the line.
        let glyph_point = |byte| {
            let row = paragraph
                .layout_runs()
                .find(|r| r.glyphs.iter().any(|g| g.start == byte))
                .unwrap();
            let g = row.glyphs.iter().find(|g| g.start == byte).unwrap();
            (g.x + g.w / 2., row.line_y - 2.)
        };
        let point = glyph_point(8);
        let next_word = glyph_point(14);
        let mut at = Pointer::new(point);
        for (ms, selected) in [
            (0, ""),
            (120, "world"),
            (240, "Hello world. More words."),
            (800, ""),
        ] {
            let down = InputEvent::MouseDown(std::time::UNIX_EPOCH + Duration::from_millis(ms));
            // Several old evdev reports can arrive in one display turn.
            dispatch(&mut p, &mut at, viewport, 1., down, 1000.).unwrap();
            dispatch(&mut p, &mut at, viewport, 1., InputEvent::ButtonUp, 1000.).unwrap();
            assert_eq!(count(&p), format!("Selection: {selected}"));
            if !selected.is_empty() {
                for (code, key, down) in [
                    ("ControlLeft", "Control", true),
                    ("KeyC", "c", true),
                    ("KeyC", "c", false),
                    ("ControlLeft", "Control", false),
                ] {
                    p.hardware_key(code, key, down, false);
                }
                assert_eq!(copied.borrow().as_str(), selected);
                let marked = p.frame();
                assert!((point.0 as u32..point.0 as u32 + 5)
                    .any(|x| (0..point.1 as u32 + 3)
                        .any(|y| clear.pixel(x, y) != marked.pixel(x, y))));
            }
        }
        // Leaving the click rectangle breaks the sequence even after returning to the word.
        for event in [
            InputEvent::Motion(20., 0.),
            InputEvent::Motion(-20., 0.),
            InputEvent::MouseDown(std::time::UNIX_EPOCH + Duration::from_millis(850)),
            InputEvent::ButtonUp,
        ] {
            dispatch(&mut p, &mut at, viewport, 1., event, 1000.).unwrap();
        }
        assert_eq!(count(&p), "Selection: ");
        // Extend the double-click by whole words as the held pointer crosses a soft wrap.
        for event in [
            InputEvent::MouseDown(std::time::UNIX_EPOCH + Duration::from_millis(900)),
            InputEvent::Motion(next_word.0 - point.0, next_word.1 - point.1),
            InputEvent::ButtonUp,
        ] {
            dispatch(&mut p, &mut at, viewport, 1., event, 1000.).unwrap();
        }
        assert_eq!(count(&p), "Selection: world. More");
    }

    /// LLP 1115.000 D5.4: a desktop's ways Back as evdev reports them, a
    /// mouse's back button (at its release, D5.5), Alt (56) around Left
    /// (105), and KEY_BACK (158), each popping a pushed route that says
    /// nothing about Back. The forward button returns to it (D5.5).
    #[test]
    fn the_back_button_and_the_back_keys_pop_a_pushed_route() {
        let plan = contract::compile(
            r#"routes nav
  home "/"
    detail "/detail"
component App
  action open
    nav = push(nav, "/detail")
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          if e.name == "home"
            button press=open testId="open"
              text "Open"
          if e.name == "detail"
            text "Detail" testId="detail"
"#,
        )
        .unwrap();
        let mut p = Presenter::boot_with(
            &plan.encode(),
            (),
            (400., 500.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap()
        .0;
        let find = |p: &Presenter<()>, name: &str| {
            let k = p.host().kernel();
            (k.find_by_test_id(name).first()).and_then(|key| Some(k.node_by_key(*key)?.id))
        };
        let on_detail = |p: &Presenter<()>| {
            find(p, "detail").is_some_and(|id| !p.host().route_visibility(id).0)
        };
        let key = |code, down| InputEvent::Key {
            code,
            shift: false,
            down,
            repeat: false,
        };
        let mut at = Pointer::new((20., 40.));
        let ways = [
            vec![InputEvent::Aux(8, true), InputEvent::Aux(8, false)],
            vec![
                key(56, true),
                key(105, true),
                key(105, false),
                key(56, false),
            ],
            vec![key(158, true), key(158, false)],
        ];
        for way in ways {
            let open = find(&p, "open").unwrap();
            p.dispatch_press(open, 0., false);
            assert!(on_detail(&p), "pushed");
            for event in &way {
                dispatch(&mut p, &mut at, (400., 500.), 1., *event, 10.).unwrap();
            }
            assert!(!on_detail(&p), "{way:?} went back");
            // From the first screen there is nowhere to go: nothing happens.
            for event in &way {
                dispatch(&mut p, &mut at, (400., 500.), 1., *event, 20.).unwrap();
            }
            assert!(find(&p, "open").is_some());
        }
        // The forward button goes where Back left, at its release.
        let mut forward = |p: &mut Presenter<()>, down| {
            dispatch(p, &mut at, (400., 500.), 1., InputEvent::Aux(16, down), 30.).unwrap();
        };
        forward(&mut p, true);
        assert!(!on_detail(&p), "not at the press");
        forward(&mut p, false);
        assert!(on_detail(&p), "forward");
        forward(&mut p, true);
        forward(&mut p, false);
        assert!(on_detail(&p), "nothing further ahead");
    }
    #[test]
    fn relative_and_absolute_display_events_release_or_escape_once() {
        for cancel in [None, Some(press(1)), Some(InputEvent::Cancel)] {
            let mut p = fixture();
            let mut at = Pointer::new((20., 40.));
            for (time, event) in [
                (0., InputEvent::MouseDown(std::time::UNIX_EPOCH)),
                (10., InputEvent::Motion(20., 0.)), // scale2: recognition at30
                (30., InputEvent::Absolute(Some(0.275), None)), // logical110
            ] {
                dispatch(&mut p, &mut at, (400., 500.), 2., event, time).unwrap();
            }
            assert!(p.collection_interaction().is_some());
            if let Some(event) = cancel {
                dispatch(&mut p, &mut at, (400., 500.), 2., event, 40.).unwrap();
            }
            dispatch(&mut p, &mut at, (400., 500.), 2., InputEvent::ButtonUp, 50.).unwrap();
            assert_eq!(count(&p), if cancel.is_some() { "0" } else { "1" });
            assert_eq!(p.collection_interaction(), None);
        }
    }

    /// Escape abandons a press with the button still down, and a lost
    /// device has none down: a replacement waits for the first's release
    /// (LLP 1029.000.002 D9).
    #[test]
    fn escape_leaves_the_button_down_and_a_lost_device_does_not() {
        use exact_painted::presenter::ReloadResult;
        let next = contract::compile("component App\n  view\n    text \"next\"\n").unwrap();
        for (abandon, held) in [(press(1), true), (InputEvent::Cancel, false)] {
            let mut p = fixture();
            let mut at = Pointer::new((20., 40.));
            let mut send = |p: &mut Presenter<()>, event, time| {
                dispatch(p, &mut at, (400., 500.), 2., event, time).unwrap()
            };
            send(&mut p, InputEvent::MouseDown(std::time::UNIX_EPOCH), 0.);
            send(&mut p, abandon, 10.);
            let result = p.reload(&next.encode(), ()).unwrap();
            assert_eq!(result == ReloadResult::Held("pointer"), held);
            send(&mut p, InputEvent::ButtonUp, 20.);
            p.run_commands(|| ());
            assert_eq!(p.replacement_pending(), None);
        }
    }

    #[test]
    fn viewport_pointer_mapping_requires_the_acknowledged_extent_not_live_c() {
        let mut p = fixture();
        let a = p.display_frame().unwrap();
        assert!(p.display_complete(&a));
        assert!(p.resize(300., 300.).is_none());
        let b = p.display_frame().unwrap();
        let b_pixels = b.pixels().data().to_vec();
        assert!(p.resize(800., 1000.).is_none());
        let mut at = Pointer::new((20., 40.));
        let absolute = InputEvent::Absolute(Some(0.25), Some(0.5));
        assert!(dispatch(&mut p, &mut at, (800., 1000.), 1., absolute, 1.).is_err());
        assert_eq!(
            at.position,
            (20., 40.),
            "refusal must not reinterpret a pointer"
        );
        dispatch(&mut p, &mut at, (400., 500.), 1., absolute, 1.).unwrap();
        assert_eq!(at.position, (100., 250.));
        assert_eq!(b.pixels().data(), b_pixels);
        assert!(p.display_complete(&b));
        // No C paint between ACK and input. B's mapping is now authoritative.
        assert!(dispatch(&mut p, &mut at, (400., 500.), 1., absolute, 2.).is_err());
        assert_eq!(at.position, (100., 250.));
        dispatch(&mut p, &mut at, (300., 300.), 1., absolute, 2.).unwrap();
        assert_eq!(at.position, (75., 150.));
        assert!(p.dirty());
        assert_eq!(b.pixels().data(), b_pixels);
    }

    #[test]
    fn viewport_pointer_scale_or_missing_origin_witness_refuses_but_keys_cancel_continue() {
        let mut p = fixture();
        let a = p.display_frame().unwrap();
        assert!(p.display_complete(&a));
        let mut at = Pointer::new((20., 40.));
        for (extent, scale) in [((400., 500.), 2.), ((800., 1000.), 1.)] {
            assert!(dispatch(
                &mut p,
                &mut at,
                extent,
                scale,
                InputEvent::Motion(8., 4.),
                1.
            )
            .is_err());
            assert_eq!(at.position, (20., 40.));
            dispatch(&mut p, &mut at, extent, scale, press(45), 1.).unwrap();
            dispatch(&mut p, &mut at, extent, scale, InputEvent::Cancel, 1.).unwrap();
        }
        let b = p.display_frame().unwrap();
        p.reload(
            &contract::compile("component App\n  view\n    text \"new\"\n")
                .unwrap()
                .encode(),
            (),
        )
        .unwrap();
        assert!(dispatch(
            &mut p,
            &mut at,
            (400., 500.),
            1.,
            InputEvent::MouseDown(std::time::UNIX_EPOCH),
            2.
        )
        .is_err());
        assert!(p.display_complete(&b)); // Old runtime's buffer released only.
        assert!(dispatch(
            &mut p,
            &mut at,
            (400., 500.),
            1.,
            InputEvent::Motion(1., 1.),
            3.
        )
        .is_err());
        assert_eq!(at.position, (20., 40.));
        mismatched_physical_release_cancels_existing_hold_without_drop();
    }

    fn mismatched_physical_release_cancels_existing_hold_without_drop() {
        for (extent, scale) in [((800., 1000.), 1.), ((400., 500.), 2.)] {
            let mut p = fixture();
            let a = p.display_frame().unwrap();
            assert!(p.display_complete(&a));
            assert!(p.resize(300., 300.).is_none());
            let b = p.display_frame().unwrap();
            let b_pixels = b.pixels().data().to_vec();
            assert!(p.resize(800., 1000.).is_none());
            let mut at = Pointer::new((20., 40.));
            for (time, event) in [
                (1., InputEvent::MouseDown(std::time::UNIX_EPOCH)),
                (10., InputEvent::Motion(20., 0.)),
                (20., InputEvent::Motion(70., 0.)),
            ] {
                dispatch(&mut p, &mut at, (400., 500.), 1., event, time).unwrap();
            }
            assert!(
                p.collection_interaction().is_some(),
                "recognized hold pins its item"
            );
            assert!(p.contact_position().is_some());
            let certified_position = at.position;
            assert!(dispatch(&mut p, &mut at, extent, scale, InputEvent::ButtonUp, 30.).is_err());
            assert_eq!(
                at.position, certified_position,
                "refusal never guesses new coordinates"
            );
            assert_eq!(
                count(&p),
                "0",
                "cancellation must not execute a successful swipe"
            );
            assert!(p.contact_position().is_none());
            assert!(
                p.collection_interaction().is_none(),
                "physical UP must release its pin"
            );
            assert_eq!(b.pixels().data(), b_pixels);
            assert!(p.display_frame().is_none());
            assert!(p.display_complete(&b));
        }
    }
    #[test]
    fn header_height_relative_absolute_release_cancel_and_escape() {
        let plan = contract::compile(r#"component App
  state target = 180
  state count = 0
  state seen = 0
  action release(h: number, v: number)
    count = count + 1
    seen = h
    target = 360
  view
    box width=400 height=500
      column id="panel" testId="panel" position="absolute" bottom=0 width=400 height=target max-height="100%" box-sizing="border-box" transition="height -exact-spring(300,30,1)"
        box heightDragFor="panel" heightrelease=release height=40
          text "drag header"
      text `${count}` testId="count"
      text `${seen}` testId="seen"
"#).unwrap();
        for cancel in [None, Some(InputEvent::Cancel), Some(press(1))] {
            let mut p = Presenter::boot_with(
                &plan.encode(),
                (),
                (400., 500.),
                1.,
                PathBuf::from(concat!(env!("CARGO_MANIFEST_DIR"), "/../../apps/caltrain")),
                PainterChoice::Cpu,
            )
            .unwrap()
            .0;
            let mut at = Pointer::new((200., 340.));
            for (time, event) in [
                (0., InputEvent::MouseDown(std::time::UNIX_EPOCH)),
                (10., InputEvent::Motion(0., -20.)), // scale2 -> recognition y330
                (30., InputEvent::Absolute(None, Some(0.42))), // y210 -> height300
            ] {
                dispatch(&mut p, &mut at, (400., 500.), 2., event, time).unwrap();
            }
            let k = p.host().kernel();
            assert_eq!(
                k.node_by_key(k.find_by_test_id("panel")[0])
                    .unwrap()
                    .frame
                    .height,
                300.
            );
            if let Some(event) = cancel {
                dispatch(&mut p, &mut at, (400., 500.), 2., event, 40.).unwrap();
            }
            for time in [50., 60.] {
                dispatch(
                    &mut p,
                    &mut at,
                    (400., 500.),
                    2.,
                    InputEvent::ButtonUp,
                    time,
                )
                .unwrap();
            }
            assert_eq!(count(&p), if cancel.is_some() { "0" } else { "1" });
            assert!(p.collection_interaction().is_none());
            if cancel.is_none() {
                let k = p.host().kernel();
                assert_eq!(
                    k.node_by_key(k.find_by_test_id("seen")[0])
                        .unwrap()
                        .props
                        .str(PropId::Text),
                    Some("300")
                );
            }
        }
    }
}
