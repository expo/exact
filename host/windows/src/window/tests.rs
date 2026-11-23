use super::*;
use winit::platform::windows::WindowAttributesExtWindows;

#[test]
fn dirty_redraws_respect_the_present_retry_and_one_paint_per_refresh() {
    let start = Instant::now();
    for hz in [60, 120, 144] {
        let period = Duration::from_secs_f64(1. / hz as f64);
        let next = start + period;
        let mut schedule = FrameSchedule {
            cadence: Some(next),
            retry: None,
            present: None,
        };
        for step in 0..100 {
            let now = start + period * step / 100;
            assert_eq!(schedule.redraw(now, true), Some(next));
            assert_eq!(schedule.admit(now), next);
        }
        // Refusal can arrive after the cadence tick. New dirty events cannot
        // shorten its deadline, and a clean picture must still retry there.
        let refused_until = next + period / 2;
        schedule.present = Some(refused_until);
        schedule.retry = Some(refused_until);
        for dirty in [false, true] {
            assert_eq!(schedule.redraw(next, dirty), Some(refused_until));
            assert_eq!(schedule.redraw(refused_until, dirty), Some(refused_until));
        }
        assert_eq!(schedule.admit(next), refused_until);
        assert_eq!(schedule.admit(refused_until), refused_until);
        schedule.present = None;
        schedule.retry = None;
        assert_eq!(schedule.redraw(refused_until, false), None);
    }
}

#[test]
fn recovery_backoff_keeps_idle_wakes_without_stalling_changed_content() {
    let now = Instant::now();
    let recovery = now + Duration::from_secs(1);
    let mut schedule = FrameSchedule {
        cadence: None,
        retry: Some(recovery),
        present: None,
    };
    assert_eq!(schedule.redraw(now, false), Some(recovery));
    assert_eq!(schedule.redraw(now, true), Some(now));
    assert_eq!(schedule.admit(now), now);
    let tick = now + Duration::from_millis(16);
    schedule.cadence = Some(tick);
    assert_eq!(schedule.redraw(now, true), Some(tick));
    assert_eq!(schedule.redraw(now, false), Some(recovery));
}

#[test]
fn an_early_image_or_window_redraw_survives_until_the_next_tick() {
    let now = Instant::now();
    let tick = now + Duration::from_millis(16);
    let schedule = FrameSchedule {
        cadence: Some(tick),
        retry: None,
        present: None,
    };
    let mut pending = false;
    // Image completion requests a redraw before poll_images marks the picture
    // dirty. Deferring that event must preserve the request, not just damage.
    assert!(schedule.defer(now, &mut pending));
    assert_eq!(schedule.redraw(now, pending), Some(tick));
    assert!(!schedule.defer(tick, &mut pending));
    assert_eq!(schedule.redraw(tick, pending), None);
}
fn present<D: DataSource>(p: &mut Presenter<D>, window: &exact_ink::device::d3d11::Window) {
    let limit = Instant::now() + Duration::from_secs(5);
    loop {
        assert!(p.window_frame(window.clone()).unwrap().is_none());
        if p.last_frame_succeeded() && !p.dirty() {
            break;
        }
        assert!(Instant::now() < limit, "window frame was not presented");
        // A full swap chain defers this frame to the carrier's retry deadline.
        let due = p
            .painter_retry_deadline()
            .expect("deferred present retries");
        std::thread::sleep(due.min(limit).saturating_duration_since(Instant::now()));
    }
}

#[test]
#[ignore = "native Windows desktop, system caret blink enabled, EXACT_INK_DEVICE=d3d11"]
#[allow(deprecated)]
fn direct_window_blinks_without_a_softbuffer_surface() {
    let events = EventLoop::<crate::accessibility::Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let window = Rc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_title("Ink caret regression")
                    .with_no_redirection_bitmap(true)
                    .with_inner_size(LogicalSize::new(160., 80.)),
            )
            .unwrap(),
    );
    let owned = exact_ink::device::d3d11::Window::from_owner(window.clone()).unwrap();
    let plan = contract::compile(
        "component App\n  view\n    input testId=\"field\" value=\"blink\" width=140 height=40\n",
    )
    .unwrap()
    .encode();
    let (mut p, warning) = Presenter::boot_with(
        &plan,
        (),
        (160., 80.),
        1.,
        Default::default(),
        exact_painted::presenter::PainterChoice::Ink,
    )
    .unwrap();
    assert!(warning.is_none());
    assert!(p.attach_painter_window(owned.clone(), [160, 80]).unwrap());
    let k = p.host().kernel();
    let field = k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id;
    p.tap(field).unwrap();
    present(&mut p, &owned);
    let started = Instant::now();
    let mut caret = crate::caret::Caret::default();
    caret.presented(owned.hwnd() as isize, p.caret_frame(), 1., None, started);
    let due = caret.deadline().expect("system caret blink enabled");
    let mut state = App {
        service_causes: 0,
        proxy: events.create_proxy(),
        accessibility: None,
        name: "Ink caret regression".into(),
        title: WindowTitle::default(),
        min_size: (1., 1.),
        config: Config::from_env(&plan, ""),
        window: Some(window),
        surface: None,
        preparing: None,
        jump_list_started: false,
        direct: true,
        painter: PainterBoot::windows::<true>("{}"),
        presenter: Some(p),
        timers: crate::timers::Timers::new(|_| {}),
        started,
        pointer: (0., 0.),
        pointer_inside: false,
        focused: true,
        cursor: CursorIcon::Default,
        buttons: 0,
        unheard: false,
        clicks: Default::default(),
        touches: Default::default(),
        scale: 1.,
        refit_after_scale: false,
        next_frame: started,
        redraw_pending: false,
        failed: false,
        minimized: false,
        occluded: false,
        frames: 1,
        measured_first_screen: false,
        measure_first_composition: false,
        paint_ms: 0.,
        present_ms: 0.,
        previous_frame: None,
        samples: Vec::new(),
        frame_logs: Vec::new(),
        frame_limit: None,
        first_tap: None,
        pads: Default::default(),
        pads_active: false,
        caret,
        tooltip: Default::default(),
        edit_menu: None,
        ime: Default::default(),
        appearance: Default::default(),
        modal: Default::default(),
        dialogs: Default::default(),
        desktop: crate::desktop::Desktop::install("Ink caret regression"),
        closing: false,
        keep_running: false,
        hidden: false,
        pending_show: None,
        reveal_awaited: None,
        instance: None,
        cold_documents: Vec::new(),
        tray: None,
        presents: Default::default(),
    };
    assert!(!state.presenter.as_ref().unwrap().dirty());
    state.blink(due).unwrap();
    assert!(!state.caret.visible());
    let next = state.caret.deadline().unwrap();
    assert!(next > due);
    assert!(state.presenter.as_ref().unwrap().dirty());
    present(state.presenter.as_mut().unwrap(), &owned);
    state.blink(due).unwrap();
    assert_eq!(state.caret.deadline(), Some(next));
    state.blink(next).unwrap();
    assert!(state.caret.visible());
    present(state.presenter.as_mut().unwrap(), &owned);
}
#[test]
#[ignore = "native Windows desktop with DirectComposition"]
#[allow(deprecated)]
fn hiding_a_resident_window_releases_its_painter_and_preserves_its_canvas_and_editor() {
    use exact_painted::presenter::PainterChoice;
    use exact_runner::{exact_canvas, DataError, Value};
    use winit::platform::pump_events::EventLoopExtPumpEvents;

    #[derive(Default)]
    struct Picture;
    impl DataSource for Picture {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("picture".into(), 0)]
        }
        fn draw_2d(
            &mut self,
            _: &str,
            _: &[Value],
            ctx: &exact_canvas::Context2d,
            frame: &exact_canvas::Frame,
        ) -> Result<bool, exact_canvas::DrawError> {
            let mut image = ctx
                .create_image_data_with_sw_and_sh(frame.width, frame.height)
                .unwrap();
            for pixel in image.data.as_chunks_mut::<4>().0 {
                *pixel = [24, 48, 96, 255];
            }
            ctx.put_image_data(&image, 0., 0.).unwrap();
            Ok(false)
        }
    }

    let size = [240u32, 120u32];
    let mut events = EventLoop::<crate::accessibility::Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let plan = contract::compile(
        r#"component App
  view
    main position="absolute" inset=0
      canvas surface=picture() position="absolute" inset=0 width="100%" height="100%"
      input testId="field" width=120 height=40
"#,
    )
    .unwrap()
    .encode();
    for (painter, name) in [(PainterChoice::Ink, "ink"), (PainterChoice::Cpu, "cpu")] {
        let direct = painter == PainterChoice::Ink;
        let window = Rc::new(
            events
                .create_window(
                    Window::default_attributes()
                        .with_title("Resident canvas test")
                        .with_no_redirection_bitmap(direct)
                        .with_inner_size(winit::dpi::PhysicalSize::new(size[0], size[1])),
                )
                .unwrap(),
        );
        let owned = exact_ink::device::d3d11::Window::from_owner(window.clone()).unwrap();
        let (mut p, warning) = Presenter::boot_with(
            &plan,
            Picture,
            (size[0] as f32, size[1] as f32),
            1.,
            Default::default(),
            painter,
        )
        .unwrap();
        assert!(warning.is_none(), "{warning:?}");
        assert_eq!(
            p.attach_painter_window(owned.clone(), size).unwrap(),
            direct
        );
        let k = p.host().kernel();
        let field = k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id;
        p.type_text(field, "resident").unwrap();
        p.reader_access(1., true);
        let editor_id = p.access_told().iter().find(|n| n.view == field).unwrap().id;
        let selection = exact_runner::FieldSelection {
            start: 1,
            end: 4,
            direction: exact_runner::SelectionDirection::Backward,
        };
        assert!(p.access_select(editor_id, selection, 0.));
        let mut app = test_app(events.create_proxy(), &plan, p);
        app.window = Some(window);
        app.keep_running = true;
        app.direct = direct;
        app.painter =
            PainterBoot::windows::<false>(&serde_json::json!({"painter": name}).to_string());
        let verify = |p: &mut Presenter<Picture>| {
            let editor = p
                .access_tree()
                .0
                .into_iter()
                .find(|n| n.view == field)
                .unwrap();
            assert_eq!(editor.value, "resident");
            assert_eq!(p.access_selection(editor_id), Some(selection));
            let pixels = p.frame();
            let offset = ((size[1] - 8) * size[0] + size[0] - 8) as usize * 4;
            assert_eq!(&pixels.data()[offset..offset + 4], &[24, 48, 96, 255]);
        };
        let p = app.presenter.as_mut().unwrap();
        if direct {
            present(p, &owned);
        }
        verify(p);
        let hwnd = owned.hwnd() as _;
        #[allow(unsafe_code)]
        let cloaked = || {
            use windows_sys::Win32::Graphics::Dwm::{
                DwmGetWindowAttribute, DWMWA_CLOAKED, DWM_CLOAKED_APP,
            };
            let mut flags = 0u32;
            let hr = unsafe {
                DwmGetWindowAttribute(
                    hwnd,
                    DWMWA_CLOAKED as u32,
                    (&mut flags as *mut u32).cast(),
                    size_of_val(&flags) as u32,
                )
            };
            assert!(hr >= 0, "{hr:#x}");
            flags & DWM_CLOAKED_APP != 0
        };
        for cancel_first in [true, false] {
            let hardware =
                app.presenter.as_ref().unwrap().painter.api.as_deref() == Some("direct D3D11");
            app.hide_by_app();
            assert_eq!(app.presenter.as_ref().unwrap().painter.name, "cpu");
            app.bring_forward();
            app.bring_forward();
            assert!(cloaked());
            #[allow(unsafe_code)]
            let foreground =
                unsafe { windows_sys::Win32::UI::WindowsAndMessaging::GetForegroundWindow() };
            assert_ne!(foreground, hwnd);
            if cancel_first {
                app.hide_by_app();
                assert!(!cloaked());
                assert_eq!(app.window.as_ref().unwrap().is_visible(), Some(false));
                app.bring_forward();
                assert!(cloaked());
            }
            let previous = app.frames;
            let deadline = Instant::now() + Duration::from_secs(5);
            while app.frames == previous {
                events.pump_app_events(
                    Some(deadline.saturating_duration_since(Instant::now())),
                    &mut app,
                );
                assert!(
                    !app.failed && Instant::now() < deadline,
                    "first window frame missing"
                );
            }
            assert!(!cloaked());
            assert_eq!(app.window.as_ref().unwrap().is_visible(), Some(true));
            let p = app.presenter.as_mut().unwrap();
            assert_eq!(p.painter.name, name);
            if hardware {
                assert_eq!(p.painter.api.as_deref(), Some("direct D3D11"));
                assert_eq!(
                    p.painter.device_ms, 0.,
                    "the hidden window's device reopened"
                );
            }
            verify(p);
        }
    }
}

#[test]
#[ignore = "native Windows desktop with DirectComposition"]
#[allow(deprecated)]
fn a_shown_window_reveals_the_still_its_canvas_asked_for() {
    use exact_painted::presenter::PainterChoice;
    use exact_runner::{exact_canvas, DataError, Value};
    use winit::platform::pump_events::EventLoopExtPumpEvents;

    #[derive(Default)]
    struct Still;
    impl DataSource for Still {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
        fn canvas_surfaces(&self) -> Vec<(String, usize)> {
            vec![("still".into(), 1)]
        }
        fn draw_2d(
            &mut self,
            _: &str,
            args: &[Value],
            ctx: &exact_canvas::Context2d,
            frame: &exact_canvas::Frame,
        ) -> Result<bool, exact_canvas::DrawError> {
            ctx.clear_rect(0., 0., frame.width as f64, frame.height as f64);
            if let Some(path) = args.first().and_then(|v| v.as_str()) {
                if !path.is_empty() {
                    ctx.draw_image_with_image_handle(path, 0., 0.)?;
                }
            }
            Ok(false)
        }
    }

    // A display-sized still written as an ordinary file, so it decodes.
    let size = [2560u32, 1440u32];
    let name = format!("reveal-still-{}.png", std::process::id());
    let disk = exact_painted::picker::roots()[2].join(&name);
    std::fs::create_dir_all(disk.parent().unwrap()).unwrap();
    let mut encoded = Vec::new();
    let mut encoder = png::Encoder::new(&mut encoded, size[0], size[1]);
    encoder.set_color(png::ColorType::Rgba);
    encoder.set_compression(png::Compression::Fast);
    let rows: Vec<u8> = (0..size[0] * size[1])
        .flat_map(|i| [(i % 251) as u8, 160, 40, 255])
        .collect();
    encoder
        .write_header()
        .unwrap()
        .write_image_data(&rows)
        .unwrap();
    std::fs::write(&disk, &encoded).unwrap();
    let path = format!("app:/tmp/{name}");

    let mut events = EventLoop::<crate::accessibility::Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let plan = contract::compile(
        r#"component App
  state path = ""
  action choose(value: string)
    path = value
  view
    main position="absolute" inset=0
      canvas surface=still(path) position="absolute" inset=0 width="100%" height="100%"
      input testId="path" value=path input=choose width=120 height=40
"#,
    )
    .unwrap()
    .encode();
    let window = Rc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_title("Reveal still test")
                    .with_no_redirection_bitmap(true)
                    .with_inner_size(winit::dpi::PhysicalSize::new(size[0], size[1])),
            )
            .unwrap(),
    );
    let owned = exact_ink::device::d3d11::Window::from_owner(window.clone()).unwrap();
    let (mut p, warning) = Presenter::boot_with(
        &plan,
        Still,
        (size[0] as f32, size[1] as f32),
        1.,
        Default::default(),
        PainterChoice::Ink,
    )
    .unwrap();
    assert!(warning.is_none(), "{warning:?}");
    p.attach_painter_window(owned.clone(), size).unwrap();
    let mut app = test_app(events.create_proxy(), &plan, p);
    app.window = Some(window);
    app.keep_running = true;
    app.direct = true;
    app.painter = PainterBoot::windows::<false>(r#"{"painter":"ink"}"#);
    present(app.presenter.as_mut().unwrap(), &owned);
    app.hide_by_app();
    // Hidden, the app points its canvas at the new still, as a capture's does.
    let p = app.presenter.as_mut().unwrap();
    let k = p.host().kernel();
    let field = k.node_by_key(k.find_by_test_id("path")[0]).unwrap().id;
    p.type_text(field, &path).unwrap();
    app.bring_forward();
    let previous = app.frames;
    let deadline = Instant::now() + Duration::from_secs(5);
    while app.frames == previous {
        events.pump_app_events(Some(Duration::from_millis(1)), &mut app);
        assert!(
            !app.failed && Instant::now() < deadline,
            "first window frame missing"
        );
    }
    assert_eq!(app.window.as_ref().unwrap().is_visible(), Some(true));
    assert_eq!(
        app.reveal_awaited,
        Some(false),
        "the revealed frame lacks the still its canvas asked for"
    );
    let p = app.presenter.as_mut().unwrap();
    let pixels = p.frame();
    let _ = std::fs::remove_file(&disk);
    let at = (size[1] / 2) * size[0] + 300;
    assert_eq!(
        &pixels.data()[at as usize * 4..][..4],
        &[(at % 251) as u8, 160, 40, 255]
    );
}
#[test]
fn text_without_a_physical_key_reaches_the_editor() {
    use winit::keyboard::{Key, KeyCode, NamedKey, NativeKey, NativeKeyCode, PhysicalKey};
    let packet = PhysicalKey::Unidentified(NativeKeyCode::Windows(0));
    for text in ["a", "é", "界", "🦀"] {
        assert_eq!(
            key_names(packet, &Key::Character(text.into()), Some(text)),
            ("Unidentified".into(), text.into())
        );
        assert_eq!(
            key_names(
                packet,
                &Key::Unidentified(NativeKey::Windows(0xe7)),
                Some(text)
            ),
            ("Unidentified".into(), text.into())
        );
    }
    assert_eq!(
        key_names(
            PhysicalKey::Code(KeyCode::Enter),
            &Key::Named(NamedKey::Enter),
            Some("\r")
        ),
        ("Enter".into(), "Enter".into())
    );
    assert_eq!(
        key_names(
            PhysicalKey::Code(KeyCode::SuperLeft),
            &Key::Named(NamedKey::Meta),
            None
        ),
        ("MetaLeft".into(), "Meta".into())
    );
}
#[test]
fn frame_cadence_does_not_accumulate_timer_latency() {
    let start = Instant::now();
    for hz in [60, 120, 144] {
        let period = Duration::from_secs_f64(1.0 / hz as f64);
        let mut deadline = start;
        for frame in 1..=10_000 {
            deadline = next_frame_deadline(deadline, deadline + Duration::from_micros(350), period);
            assert_eq!(deadline, start + period * frame);
        }
    }
}
#[test]
fn early_redraws_preserve_cadence_and_stalls_resume_without_catch_up() {
    let start = Instant::now();
    let period = Duration::from_millis(10);
    let deadline = start + period;
    assert_eq!(next_frame_deadline(deadline, start, period), deadline);
    let late = deadline + Duration::from_secs(5);
    assert_eq!(next_frame_deadline(deadline, late, period), late + period);
    assert_eq!(
        next_frame_deadline(deadline, deadline + period, period),
        deadline + period * 2
    );
}

pub(super) fn windowless_app(
    proxy: EventLoopProxy<crate::accessibility::Event>,
    plan: &[u8],
) -> App<()> {
    let (p, _) = Presenter::boot_with(
        plan,
        (),
        (160., 80.),
        1.,
        Default::default(),
        exact_painted::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    test_app(proxy, plan, p)
}

fn test_app<D: DataSource>(
    proxy: EventLoopProxy<crate::accessibility::Event>,
    plan: &[u8],
    p: Presenter<D>,
) -> App<D> {
    let started = Instant::now();
    App {
        service_causes: 0,
        proxy,
        accessibility: None,
        name: "Frame scheduling regression".into(),
        title: WindowTitle::default(),
        min_size: (1., 1.),
        config: Config::from_env(plan, ""),
        window: None,
        surface: None,
        preparing: None,
        jump_list_started: false,
        direct: true,
        painter: PainterBoot::windows::<true>("{}"),
        presenter: Some(p),
        timers: crate::timers::Timers::new(|_| {}),
        started,
        pointer: (0., 0.),
        pointer_inside: false,
        focused: true,
        cursor: CursorIcon::Default,
        buttons: 0,
        unheard: false,
        clicks: Default::default(),
        touches: Default::default(),
        scale: 1.,
        refit_after_scale: false,
        next_frame: started,
        redraw_pending: false,
        failed: false,
        minimized: false,
        occluded: false,
        frames: 1,
        measured_first_screen: false,
        measure_first_composition: false,
        paint_ms: 0.,
        present_ms: 0.,
        previous_frame: None,
        samples: Vec::new(),
        frame_logs: Vec::new(),
        frame_limit: None,
        first_tap: None,
        pads: Default::default(),
        pads_active: false,
        caret: Default::default(),
        tooltip: Default::default(),
        edit_menu: None,
        ime: Default::default(),
        appearance: Default::default(),
        modal: Default::default(),
        dialogs: Default::default(),
        desktop: crate::desktop::Desktop::install("Frame scheduling regression"),
        closing: false,
        keep_running: false,
        hidden: false,
        pending_show: None,
        reveal_awaited: None,
        instance: None,
        cold_documents: Vec::new(),
        tray: None,
        presents: Default::default(),
    }
}

#[allow(deprecated)]
pub(crate) fn run_carrier_tests(events: EventLoop<crate::accessibility::Event>) {
    let plan = contract::compile("component App\n  view\n    text \"idle\"\n")
        .unwrap()
        .encode();
    let mut app = windowless_app(events.create_proxy(), &plan);
    let mut checked = false;
    events.run(|event, events| {
        if !matches!(event, winit::event::Event::AboutToWait) { return; }
        let now = Instant::now();
        let retry = now + Duration::from_secs(1);
        app.presenter.as_mut().unwrap().frame();
        assert!(!app.presenter.as_ref().unwrap().dirty());
        app.wait(events, |_| (Some(retry), Some(retry)));
        assert_eq!(events.control_flow(), ControlFlow::WaitUntil(retry));
        app.redraw_pending = true;
        app.wait(events, |_| (Some(retry), Some(retry)));
        assert_eq!(events.control_flow(), ControlFlow::WaitUntil(retry));
        assert!(app.redraw_pending);
        app.next_frame = now + Duration::from_millis(500);
        app.frame_limit = Some(10);
        app.wait(events, |_| (None, None));
        assert_eq!(events.control_flow(), ControlFlow::WaitUntil(app.next_frame));
        app.frame_limit = None;
        app.redraw_pending = false;
        app.about_to_wait(events);
        assert_eq!(events.control_flow(), ControlFlow::Wait);
        // A runner wake must win over the presenter's later retry.
        let timed = contract::compile("component App\n  state n = 0\n  action bump\n    n = n + 1\n  task timer mount\n    every(500, bump)\n  view\n    text \"idle\"\n").unwrap().encode();
        let (mut p, _) = Presenter::boot_with(&timed, (), (160., 80.), 1., Default::default(),
            exact_painted::presenter::PainterChoice::Cpu).unwrap();
        p.frame();
        let wake = p.wake_due_ms().unwrap();
        app.presenter = Some(p);
        app.wait(events, |_| (Some(retry), Some(retry)));
        assert_eq!(events.control_flow(), ControlFlow::WaitUntil(app.started + Duration::from_secs_f64(wake / 1000.)));
        file_drop::tests::shell_selection_over_an_editor(events, app.proxy.clone()).unwrap();
        document_title_reaches_the_window_without_drawing(events, app.proxy.clone());
        checked = true;
        events.exit();
    }).unwrap();
    assert!(checked);
}

fn document_title_reaches_the_window_without_drawing(
    events: &ActiveEventLoop,
    proxy: EventLoopProxy<crate::accessibility::Event>,
) {
    let plan = contract::compile(
        r#"component App
  state step = 0
  state timed = false
  derive title = timed ? "Timed document" : step == 0 ? "First document" : step == 1 ? "Second document 📄" : ""
  derive body = timed ? "Updated text" : "Initial text"
  action next
    step = (step + 1) % 4
  action rename
    timed = true
  task timer mount
    after(60000, rename)
  view
    column
      if step != 3
        head title=title
      button "Next" press=next testId="next"
      text body
"#,
    )
    .unwrap()
    .encode();
    let mut app = windowless_app(proxy.clone(), &plan);
    let initial_frames = app.frames;
    let window = Rc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_title(&app.name)
                    .with_visible(false)
                    .with_inner_size(LogicalSize::new(160., 80.)),
            )
            .unwrap(),
    );
    app.window = Some(window.clone());
    app.hidden = true;
    let mut access = crate::accessibility::Access::new(events, &window, proxy);
    let p = app.presenter.as_mut().unwrap();
    access.event(
        p,
        &window,
        &app.name,
        accesskit_winit::WindowEvent::InitialTreeRequested,
    );
    app.accessibility = Some(access);
    let kernel = p.host().kernel();
    let next = kernel
        .node_by_key(kernel.find_by_test_id("next")[0])
        .unwrap()
        .id;
    let titles = [
        Some("First document"),
        Some("Second document 📄"),
        Some(""),
        None,
        Some("First document"),
    ];
    for (step, title) in titles.into_iter().enumerate() {
        if step > 0 {
            let p = app.presenter.as_mut().unwrap();
            assert_eq!(
                p.host_mut()
                    .dispatch_at(next, exact_runner::Event::Press, 0.),
                None
            );
        }
        app.wait(events, |_| (None, None));
        let title = title.unwrap_or(&app.name);
        assert_eq!(window.title(), title);
        assert_eq!(
            app.accessibility.as_ref().unwrap().root_label(),
            Some(title)
        );
    }
    assert_eq!(app.frames, initial_frames);

    app.started -= Duration::from_secs(60);
    app.wait(events, |_| (None, None));
    let p = app.presenter.as_mut().unwrap();
    let access = app.accessibility.as_mut().unwrap();
    assert_eq!(window.title(), "Timed document");
    assert_eq!(access.root_label(), Some("Timed document"));
    assert!(p.access_told().iter().any(|n| n.name == "Initial text"));
    assert!(p.dirty());

    p.frame();
    access.sync(p, &window, "Timed document");
    assert!(p.access_told().iter().any(|n| n.name == "Updated text"));
    assert_eq!(access.root_label(), Some("Timed document"));
}

#[test]
fn newly_drawable_windows_discard_occlusion_deadlines_but_keep_cadence() {
    let now = Instant::now();
    let period = Duration::from_secs_f64(1. / 60.);
    for event in [
        WindowEvent::Occluded(false),
        WindowEvent::Focused(true),
        WindowEvent::Resized(winit::dpi::PhysicalSize::new(800, 800)),
    ] {
        assert!(presentation_resumes(&event));
        let schedule = FrameSchedule {
            cadence: Some(now + period),
            retry: None,
            present: None,
        };
        assert_eq!(schedule.admit(now), now + period);
        assert_eq!(schedule.redraw(now, true), Some(now + period));
    }
    assert!(!presentation_resumes(&WindowEvent::Focused(false)));
    assert!(!presentation_resumes(&WindowEvent::Occluded(true)));
}

/// A login item's hidden launch (LLP 1133.025 D6): the window's composition
/// takes no present while the window is not visible, so a drawn frame never
/// succeeds, and the frame painted to pixels is its first pixel instead.
#[test]
#[ignore = "native Windows desktop, EXACT_INK_DEVICE=d3d11"]
#[allow(deprecated)]
fn a_hidden_window_reaches_first_pixel_without_a_present() {
    let events = EventLoop::<crate::accessibility::Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let window = Rc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_title("Hidden launch regression")
                    .with_visible(false)
                    .with_no_redirection_bitmap(true)
                    .with_inner_size(LogicalSize::new(160., 80.)),
            )
            .unwrap(),
    );
    let owned = exact_ink::device::d3d11::Window::from_owner(window.clone()).unwrap();
    let plan = contract::compile("component App\n  view\n    text \"Hidden\"\n")
        .unwrap()
        .encode();
    let (mut p, warning) = Presenter::boot_with(
        &plan,
        (),
        (160., 80.),
        1.,
        Default::default(),
        exact_painted::presenter::PainterChoice::Ink,
    )
    .unwrap();
    assert!(warning.is_none());
    assert!(p.attach_painter_window(owned.clone(), [160, 80]).unwrap());
    let _ = p.window_frame(owned).unwrap();
    p.first_pixel();
    assert!(!p.first_pixel_told(), "a hidden window took a present");
    p.first_pixel_unshown();
    assert!(p.first_pixel_told());
}

/// A close in the same message batch as a keystroke in Windows' editor
/// (EXACT_INK_RICHEDIT=1): the edit is still the editor's when the close
/// arrives, and `beforeunload` must see it, or an app asks nothing and the
/// edit is lost.
#[test]
#[ignore = "native Windows desktop, EXACT_INK_DEVICE=d3d11, isolated process"]
#[allow(deprecated)]
fn beforeunload_sees_an_edit_the_native_editor_still_holds() {
    use windows_sys::Win32::UI::WindowsAndMessaging::{MSG, WM_CHAR, WM_LBUTTONDOWN};
    std::env::set_var("EXACT_INK_RICHEDIT", "1");
    let events = EventLoop::<crate::accessibility::Event>::with_user_event()
        .with_any_thread(true)
        .build()
        .unwrap();
    let window = Rc::new(
        events
            .create_window(
                Window::default_attributes()
                    .with_title("Unload regression")
                    .with_no_redirection_bitmap(true)
                    .with_inner_size(LogicalSize::new(160., 80.)),
            )
            .unwrap(),
    );
    let owned = exact_ink::device::d3d11::Window::from_owner(window.clone()).unwrap();
    let plan = contract::compile(
        "component App\n  state draft = \"saved\"\n  state dirty = false\n  action type(v)\n    draft = v\n    dirty = true\n  action ask\n    if dirty\n      preventDefault()\n  view\n    main beforeunload=ask\n      input testId=\"field\" value=draft input=type width=140 height=40\n",
    )
    .unwrap()
    .encode();
    let (mut p, warning) = Presenter::boot_with(
        &plan,
        (),
        (160., 80.),
        1.,
        Default::default(),
        exact_painted::presenter::PainterChoice::Ink,
    )
    .unwrap();
    assert!(warning.is_none());
    assert!(p.attach_painter_window(owned.clone(), [160, 80]).unwrap());
    let limit = Instant::now() + Duration::from_secs(5);
    while !(p.last_frame_succeeded() && !p.dirty()) {
        assert!(
            Instant::now() < limit,
            "the field's frame was not presented"
        );
        let _ = p.window_frame(owned.clone()).unwrap();
        std::thread::sleep(Duration::from_millis(16));
    }
    let hook = exact_ink::device::d3d11::fields::message_hook();
    for (kind, wparam, lparam) in [
        (WM_LBUTTONDOWN, 1, (10 << 16) | 10),
        (WM_CHAR, 'x' as usize, 1),
    ] {
        let message = MSG {
            hwnd: owned.hwnd() as _,
            message: kind,
            wParam: wparam,
            lParam: lparam,
            ..Default::default()
        };
        hook(&message as *const MSG as *const std::ffi::c_void);
    }
    // No turn ran between the keystroke and the close.
    assert!(!p.before_unload(p.host().now()), "the handler saw the edit");
}
