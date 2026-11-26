//! Scroll snap on the Linux family (LLP 1008 §9): the offsets a scroller's
//! areas rest at, as the reader is told them, and the rule that picks one.
use super::*;

fn boot(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 600.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    for _ in 0..8 {
        assert!(p.pump(p.host.now()).is_none());
        if p.dirty() {
            let _ = p.frame();
        }
    }
    p
}

fn id(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn rests(snap: &Snap) -> Vec<(f32, f32, bool)> {
    let mut v: Vec<_> = snap.areas.iter().map(|a| (a.from, a.to, a.stop)).collect();
    v.sort_by(|a, b| a.0.total_cmp(&b.0));
    v
}

const SNAPPERS: &str = r#"component App
  view
    column width=400 height=600
      scroll testId="rail" width=400 height=100 overflow-x="scroll" overflow-y="hidden" scroll-snap-type="x mandatory"
        row
          for i in [0, 1, 2, 3] key=i
            view width=300 height=100 flex-shrink=0 scroll-snap-align="center" scroll-snap-stop=(i == 2 ? "always" : "normal")
      scroll testId="pager" width=400 height=200 scroll-snap-type="y mandatory" scroll-padding-top=20
        column
          for i in [0, 1, 2] key=i
            view width=400 height=180 flex-shrink=0 scroll-snap-align="start none" scroll-margin-top=(i == 1 ? 10 : 0)
          view width=400 height=500 flex-shrink=0 scroll-snap-align="start"
      scroll testId="near" width=400 height=100 overflow-x="scroll" overflow-y="hidden" scroll-snap-type="x"
        row
          for i in [0, 1, 2] key=i
            view width=200 height=100 flex-shrink=0 scroll-snap-align="end"
              scroll width=200 height=100 overflow-x="scroll"
                row
                  view width=300 height=100 flex-shrink=0 scroll-snap-align="start"
"#;

#[test]
fn a_scroller_tells_the_offsets_its_areas_rest_at_on_each_axis_it_snaps() {
    let mut p = boot(SNAPPERS);
    let (rail, pager, near) = (id(&p, "rail"), id(&p, "pager"), id(&p, "near"));
    // Each 300-wide card's centre at the 400-wide port's; the first and the
    // last are held by the ends (travel 800).
    let x = p.snap(rail, false).unwrap();
    assert!(x.mandatory);
    assert_eq!(x.port, 400.);
    assert_eq!(
        rests(&x),
        vec![
            (0., 0., false),
            (250., 250., false),
            (550., 550., true),
            (800., 800., false)
        ]
    );
    assert_eq!(p.snap(rail, true), None, "it snaps along x alone");
    // A page's start at the snapport's, 20 below the port's top
    // (`scroll-padding-top`); the second page's area grows 10 above it, past
    // the snapport, so it rests anywhere from 150 to 160; the
    // last, taller than the 180 snapport, rests anywhere it covers it, to
    // the travel's end (840).
    let y = p.snap(pager, true).unwrap();
    assert_eq!(y.port, 180.);
    assert_eq!(
        rests(&y),
        vec![
            (0., 0., false),
            (150., 160., false),
            (340., 340., false),
            (520., 840., false)
        ]
    );
    // `proximity`, and a nested scroller keeps its own areas.
    let near = p.snap(near, false).unwrap();
    assert!(!near.mandatory);
    assert_eq!(
        rests(&near),
        vec![(0., 0., false), (0., 0., false), (200., 200., false)]
    );
}

#[test]
fn percentage_scroll_padding_rests_at_the_browsers_offsets() {
    for (axis, main, flow, first, last) in [
        ("x", "width", "row", "left", "right"),
        ("y", "height", "column", "top", "bottom"),
    ] {
        for (trail, center, end) in [("10%", 288., 516.), ("6px", 277., 494.)] {
            let mut p = boot(&format!(
                r#"component App
  view
    scroll testId="port" width=300 height=300 box-sizing="border-box" border-style="solid" overflow-{axis}="scroll" scroll-snap-type="{axis} mandatory" border-{first}-width=8 border-{last}-width=12 scroll-padding-{first}="10%" scroll-padding-{last}="{trail}"
      {flow} {main}=1200
        view {main}=88
        view {main}=80 scroll-snap-align="start"
        view {main}=220
        view {main}=80 scroll-snap-align="center"
        view {main}=220
        view {main}=80 scroll-snap-align="end"
"#
            ));
            let port = id(&p, "port");
            assert_eq!(
                rests(&p.snap(port, axis == "y").unwrap()),
                [60., center, end].map(|v| (v, v, false)),
                "{axis} {trail}"
            );
        }
    }
}

/// LLP 1123 D9: a scroll padding and a scroll margin that read the safe area
/// where the box is take its owner's inset, not the document's 0: the same
/// rests as `scroll-padding-top=20` and a 10-point margin.
#[test]
fn a_snap_padding_and_margin_read_the_boxes_own_inset() {
    let source = SNAPPERS
        .replace(
            "scroll-padding-top=20",
            r#"scroll-padding-top="var(--exact-safe-area-inset-top)""#,
        )
        .replace(
            "scroll-margin-top=(i == 1 ? 10 : 0)",
            r#"scroll-margin-top=(i == 1 ? "calc(var(--exact-safe-area-inset-top) - 10px)" : "0px")"#,
        );
    let mut p = boot(&source);
    let pager = id(&p, "pager");
    let kernel = p.host.runner_mut().kernel_mut();
    let root = kernel.roots()[0];
    let local = Some([20.0, 0.0, 0.0, 0.0]);
    kernel.set_local_insets(root, local).unwrap();
    let y = p.snap(pager, true).unwrap();
    let want = vec![
        (0., 0., false),
        (150., 160., false),
        (340., 340., false),
        (520., 840., false),
    ];
    assert_eq!(rests(&y), want);
}

#[test]
fn the_reader_is_told_each_scrollers_snap_in_its_pixels() {
    let mut p = boot(SNAPPERS);
    let (rail, pager) = (id(&p, "rail"), id(&p, "pager"));
    let words = p.reader_snaps(&[rail, pager, PAGE], 2.);
    let f = |w: i32| f32::from_bits(w as u32);
    // The rail: x, mandatory; a 400 port (800 px) and four areas; no y.
    assert_eq!(words[0], 1 | 4);
    assert_eq!((f(words[1]), words[2]), (800., 4));
    let rail_end = 3 + 3 * 4;
    assert_eq!((f(words[rail_end]), words[rail_end + 1]), (0., 0));
    let pager_at = rail_end + 2;
    assert_eq!(words[pager_at], 2 | 4);
    assert_eq!((f(words[pager_at + 1]), words[pager_at + 2]), (0., 0));
    assert_eq!((f(words[pager_at + 3]), words[pager_at + 4]), (360., 4));
    let page_at = pager_at + 5 + 3 * 4;
    assert_eq!(&words[page_at..], &[0, 0, 0, 0, 0]);
}

#[test]
fn a_rest_is_the_nearest_position_short_of_an_always_one() {
    let at = |from: f32, stop: bool| SnapArea {
        from,
        to: from,
        stop,
        id: 0,
    };
    let cards = [
        at(0., false),
        at(250., false),
        at(550., false),
        at(800., false),
    ];
    assert_eq!(rest(&cards, 600., 250., true, 400.), Some(550.));
    assert_eq!(
        rest(&cards, 400., 250., true, 400.),
        Some(250.),
        "the smaller of two as near"
    );
    let always = cards.map(|a| SnapArea { stop: true, ..a });
    assert_eq!(rest(&always, 780., 250., true, 400.), Some(550.));
    assert_eq!(rest(&always, 10., 550., true, 400.), Some(250.));
    let long = [
        at(0., false),
        SnapArea {
            from: 300.,
            to: 900.,
            stop: true,
            id: 0,
        },
        at(1200., false),
    ];
    assert_eq!(rest(&long, 640., 400., true, 400.), Some(640.));
    assert_eq!(
        rest(&long, 1150., 400., true, 400.),
        Some(900.),
        "left at its end"
    );
    assert_eq!(rest(&cards, 380., 250., false, 400.), Some(250.));
    assert_eq!(
        rest(&cards, 400., 250., false, 400.),
        None,
        "`proximity`: too far"
    );
    assert_eq!(rest(&[], 400., 250., true, 400.), None);
}

#[test]
fn a_flick_reaches_at_least_the_next_position_from_where_its_drag_began() {
    let at = |from: f32| SnapArea {
        from,
        to: from,
        stop: false,
        id: 0,
    };
    let cards = [at(0.), at(300.), at(600.), at(900.)];
    // A short quick flick projected under half a card goes on to the next.
    assert_eq!(flicked(&cards, 120., 80., (0., 1.), true, 300.), Some(300.));
    assert_eq!(flicked(&cards, 120., 80., (0., 0.), true, 300.), Some(0.));
    // Counted from where the drag began: one that already passed a card adds none.
    assert_eq!(
        flicked(&cards, 420., 400., (0., 1.), true, 300.),
        Some(300.)
    );
    assert_eq!(
        flicked(&cards, 560., 580., (600., -1.), true, 300.),
        Some(300.)
    );
    assert_eq!(
        flicked(&cards, 1500., 80., (0., 1.), true, 300.),
        Some(900.)
    );
    let always = cards.map(|a| SnapArea { stop: true, ..a });
    assert_eq!(
        flicked(&always, 1500., 80., (0., 1.), true, 300.),
        Some(300.)
    );
}

#[test]
fn a_wheel_notch_moves_at_once_and_finer_ticks_scroll_freely_then_settle() {
    let mut p = boot(SNAPPERS);
    let rail = id(&p, "rail");
    let mut seen = Vec::new();
    for _ in 0..4 {
        p.wheel(rail, 40., 0.).unwrap();
        seen.push(p.scroll_of(rail).0);
    }
    // A notch a position, the `always` card at 550 included; the last at the end.
    assert_eq!(seen, [250., 550., 800., 800.]);
    // Finer ticks scroll freely and settle once the wheel goes quiet.
    p.scroll_to(rail, (0., 0.));
    let start = p.host.now();
    let mut seen = Vec::new();
    for i in 0..8 {
        p.advance(start + i as f64 * 10.);
        p.wheel(rail, 5., 0.).unwrap();
        seen.push(p.scroll_of(rail).0);
    }
    assert_eq!(seen, [5., 10., 15., 20., 25., 30., 35., 40.]);
    let due = p.wake_due_ms().expect("quiet settle timer armed");
    assert_eq!(due, start + 70. + 100.);
    // A fast frame (>= notch) mid-gesture keeps scrolling freely until quiet.
    p.advance(start + 90.);
    p.wheel(rail, 48., 0.).unwrap();
    assert_eq!(p.scroll_of(rail).0, 88.);
    p.advance(start + 190.);
    assert_eq!(p.scroll_of(rail).0, 250.);
    // A scroll from elsewhere starts over from where it put it, and
    // a notch back goes to the position before it.
    p.scroll_to(rail, (500., 0.));
    p.wheel(rail, -40., 0.).unwrap();
    assert_eq!(p.scroll_of(rail).0, 250.);
}

#[test]
fn fine_wheel_ticks_settle_by_proximity_reversal_glide_and_scrollend() {
    let source = r#"component App
  state ends = 0
  state wide = false
  action ended
    ends = ends + 1
  action toggle
    wide = !wide
  view
    column width=400 height=900
      button testId="toggle" press=toggle width=(wide ? 120 : 100) height=20
      scroll testId="near" width=400 height=400 scroll-snap-type="y" scrollend=ended
        column
          for i in [0, 1, 2, 3, 4] key=i
            view width=400 height=400 flex-shrink=0 scroll-snap-align="start"
      scroll testId="pager" width=400 height=400 scroll-snap-type="y mandatory" scrollend=ended
        column
          for i in [0, 1, 2, 3, 4] key=i
            view width=400 height=400 flex-shrink=0 scroll-snap-align="start"
"#;
    let mut p = boot(source);
    let (near, pager) = (id(&p, "near"), id(&p, "pager"));
    let mut t = p.host.now();

    // Proximity: 60 fine ticks (240 pt) scroll freely and stay free outside 1/3 port.
    for _ in 0..60 {
        t += 8.;
        p.advance(t);
        p.wheel(near, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(near).1, 240.);
    if !cfg!(windows) {
        assert_eq!(
            p.host.runner().slot("ends"),
            Some(&exact_runner::Value::Number(0.))
        );
    }
    t += 100.;
    p.advance(t);
    assert_eq!(p.scroll_of(near).1, 240.);
    if !cfg!(windows) {
        assert_eq!(
            p.host.runner().slot("ends"),
            Some(&exact_runner::Value::Number(1.))
        );
    }

    // Proximity: scrolling to 300 (within 400/3 of 400) settles to 400.
    p.scroll_to(near, (0., 0.));
    for _ in 0..75 {
        t += 8.;
        p.advance(t);
        p.wheel(near, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(near).1, 300.);
    t += 100.;
    p.advance(t);
    assert_eq!(p.scroll_of(near).1, 400.);

    // Proximity: slow 1-line mouse notches (16 pt, >100 ms apart) advance without snapping back.
    p.scroll_to(near, (0., 0.));
    let mut slow = Vec::new();
    for _ in 0..6 {
        t += 200.;
        p.advance(t);
        p.wheel(near, 0., 16.).unwrap();
        t += 100.;
        p.advance(t);
        slow.push(p.scroll_of(near).1);
    }
    assert_eq!(slow, [16., 32., 48., 64., 80., 96.]);

    // Mandatory: 60 fine ticks (240 pt) settle once to 400 and hold resnap until quiet.
    for _ in 0..60 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager).1, 240.);
    p.tap(id(&p, "toggle")).unwrap();
    p.frame();
    assert_eq!(
        p.scroll_of(pager).1,
        240.,
        "layout commit must not snap mid-gesture"
    );
    let ends_before = p.host.runner().slot("ends").cloned();
    t += 100.;
    p.advance(t);
    assert_eq!(p.scroll_of(pager).1, 400.);
    if !cfg!(windows) {
        let Some(exact_runner::Value::Number(before)) = ends_before else {
            panic!()
        };
        assert_eq!(
            p.host.runner().slot("ends"),
            Some(&exact_runner::Value::Number(before + 1.)),
            "exactly one scrollend after settle"
        );
    }

    // Mandatory: reversing direction back toward start cancels forward flick.
    p.scroll_to(pager, (0., 0.));
    for _ in 0..15 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    for _ in 0..13 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., -4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager).1, 8.);
    t += 100.;
    p.advance(t);
    assert_eq!(p.scroll_of(pager).1, 0.);

    // Live clock: settle glides smoothly and a new wheel tick catches the glide
    // without emitting an extra scrollend.
    let clock = std::rc::Rc::new(std::cell::Cell::new(t + 5000.));
    let reader = clock.clone();
    p.set_live_clock(Some(std::rc::Rc::new(move || reader.get())));
    p.scroll_to(pager, (0., 0.));
    // Idle live clock: wheel_now uses live time even without a prior advance().
    let mut live_t = t + 5000.;
    for _ in 0..30 {
        live_t += 8.;
        clock.set(live_t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager).1, 120.);
    let due = p.wheel_snap_due().unwrap();
    assert!(
        (due - (live_t + 100.)).abs() < 0.01,
        "due uses live clock: {due} vs {}",
        live_t + 100.
    );
    p.advance(live_t + 50.);
    assert_eq!(
        p.scroll_of(pager).1,
        120.,
        "does not settle before quiet window"
    );
    let ends_before_glide = p.host.runner().slot("ends").cloned();
    live_t += 100.;
    clock.set(live_t);
    p.advance(live_t);
    assert!(p.needs_animation_frame(), "settle started a smooth glide");
    p.tick(live_t + 150.);
    let mid = p.scroll_of(pager).1;
    assert!(mid > 120. && mid < 400., "gliding smoothly: {mid}");
    // Catching the glide with a fine wheel tick stops it immediately without a premature scrollend.
    live_t += 160.;
    clock.set(live_t);
    p.wheel(pager, 0., -4.).unwrap();
    assert!(!p.gliding(pager));
    assert!((p.scroll_of(pager).1 - (mid - 4.)).abs() < 0.01);
    if !cfg!(windows) {
        assert_eq!(
            p.host.runner().slot("ends"),
            ends_before_glide.as_ref(),
            "catching glide emits no premature scrollend"
        );
    }
    // After quiet + glide finishes, exactly one scrollend fires for the combined gesture.
    live_t += 100.;
    clock.set(live_t);
    p.advance(live_t);
    live_t += 400.;
    clock.set(live_t);
    p.tick(live_t);
    p.advance(live_t);
    if !cfg!(windows) {
        let Some(exact_runner::Value::Number(before)) = ends_before_glide else {
            panic!()
        };
        assert_eq!(
            p.host.runner().slot("ends"),
            Some(&exact_runner::Value::Number(before + 1.)),
            "exactly one scrollend after caught glide settles"
        );
    }
}

#[test]
fn fine_wheel_snap_edge_cases_perpendicular_resize_and_preemption() {
    let mut p = boot(
        r#"component App
  state ends = 0
  state h = 400
  action ended
    ends = ends + 1
  action resize_pages
    h = 500
  view
    column width=400 height=600
      button testId="grow" press=resize_pages
        text "grow"
      scroll testId="pager" width=400 height=400 overflow-x="scroll" overflow-y="scroll" scroll-snap-type="y mandatory" scrollend=ended
        column width=800
          for i in [0, 1, 2] key=i
            view width=800 height=h flex-shrink=0 scroll-snap-align="start"
"#,
    );
    let pager = id(&p, "pager");
    let mut t = p.host.now();

    // 1. Perpendicular unsnapped X tick preserves the open Y snap gesture.
    for _ in 0..30 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager), (0., 120.));
    t += 8.;
    p.advance(t);
    p.wheel(pager, 20., 0.).unwrap();
    assert_eq!(p.scroll_of(pager), (20., 120.));
    assert!(
        p.wheel_snap_pending(pager),
        "X tick on unsnapped axis keeps Y snap pending"
    );
    t += 100.;
    p.advance(t);
    assert_eq!(p.scroll_of(pager), (20., 400.));

    // 2. No-move settle updates snap_layout so a later layout change follows the chosen page.
    p.scroll_to(pager, (0., 400.));
    p.frame();
    // Fine ticks away and back to 400 (net target == at == 400).
    for _ in 0..5 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    for _ in 0..5 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., -4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager).1, 400.);
    t += 100.;
    p.advance(t);
    assert!(!p.wheel_snap_pending(pager));
    assert_eq!(p.scroll_of(pager).1, 400.);
    // Growing pages to 500 follows page 1 -> 500.
    p.tap(id(&p, "grow")).unwrap();
    present(&mut p);
    assert_eq!(p.scroll_of(pager).1, 500.);

    // 3. Pointer hold / reader hold preempts pending wheel snap so settle does not yank it.
    p.scroll_to(pager, (0., 0.));
    present(&mut p);
    let y = p.box_of(pager).unwrap().rect.1 + 20.;
    p.reader_under(50., y, 1.);
    assert!(p.reader_knows(pager));
    for _ in 0..30 {
        t += 8.;
        p.advance(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    assert_eq!(p.scroll_of(pager).1, 120.);
    p.reader_drag_began(pager);
    assert!(!p.wheel_snap_pending(pager));
    t += 200.;
    p.advance(t);
    assert_eq!(
        p.scroll_of(pager).1,
        120.,
        "reader hold preempted wheel settle"
    );
    p.reader_drag_ended(pager, (0., 0.), None, false);

    // 4. Resize during a live-clock settle glide holds scrollend until resnap lands.
    let clock = std::rc::Rc::new(std::cell::Cell::new(t));
    let reader = clock.clone();
    p.set_live_clock(Some(std::rc::Rc::new(move || reader.get())));
    p.scroll_to(pager, (0., 0.));
    p.advance(t);
    for _ in 0..40 {
        t += 8.;
        clock.set(t);
        p.wheel(pager, 0., 4.).unwrap();
    }
    let ends_before = p.host.runner().slot("ends").cloned();
    t += 100.;
    clock.set(t);
    p.advance(t);
    assert!(p.gliding(pager));
    // Resize height during glide: page 1 moves from 500 to 500 (or viewport resize invalidates stamp).
    p.resize(400., 500.);
    t += 400.;
    clock.set(t);
    p.tick(t);
    assert!(!p.gliding(pager));
    if !cfg!(windows) {
        // Resnap is pending until the resized frame paints.
        if p.snap_resnap_pending(pager) {
            p.advance(t);
            assert_eq!(p.host.runner().slot("ends"), ends_before.as_ref());
        }
    }
    present(&mut p);
    p.advance(t);
    if !cfg!(windows) {
        let Some(exact_runner::Value::Number(before)) = ends_before else {
            panic!()
        };
        assert_eq!(
            p.host.runner().slot("ends"),
            Some(&exact_runner::Value::Number(before + 1.))
        );
    }
}

#[test]
fn a_layout_change_keeps_a_scroller_on_the_area_it_rested_on() {
    let mut p = boot(
        r#"component App
  state w = 300
  action wider
    w = 350
  view
    column width=400 height=600
      button testId="wider" press=wider
        text "wider"
      scroll testId="rail" width=400 height=100 overflow-x="scroll" overflow-y="hidden" scroll-snap-type="x mandatory"
        row
          for i in [0, 1, 2, 3] key=i
            view width=w height=100 flex-shrink=0 scroll-snap-align="start"
"#,
    );
    let rail = id(&p, "rail");
    p.wheel(rail, 40., 0.).unwrap();
    assert_eq!(p.scroll_of(rail).0, 300.);
    // The cards widen: the second now rests at 350.
    p.tap(id(&p, "wider")).unwrap();
    for _ in 0..8 {
        assert!(p.pump(p.host.now()).is_none());
        if p.dirty() {
            let _ = p.frame();
        }
    }
    assert_eq!(p.scroll_of(rail).0, 350.);
}

#[test]
fn a_virtualized_list_adds_the_rows_it_has_not_mounted() {
    let mut p = boot(
        r#"component App
  view
    list testId="strip" virtualized=true display="flex" width=300 height=100 overflow-x="scroll" overflow-y="hidden" estimated-item-width=150 scroll-snap-type="x mandatory"
      for n in range(1000) key=n
        view width=150 height=100 flex-shrink=0 scroll-snap-align="start"
"#,
    );
    let strip = id(&p, "strip");
    let snap = p.snap(strip, false).unwrap();
    let mut at: Vec<f32> = snap.areas.iter().map(|a| a.from).collect();
    at.sort_by(f32::total_cmp);
    at.dedup();
    // Every card from the first to the bounded window's end (eight ports and
    // one past the offset), mounted or not.
    assert!(at.len() > 16, "{} areas", at.len());
    assert!(
        at.iter().enumerate().all(|(i, &x)| x == i as f32 * 150.),
        "{at:?} {:?}",
        snap.areas
    );
}

#[test]
fn a_release_far_past_the_told_rows_reads_the_rows_around_its_projection() {
    let mut p = boot(
        r#"component App
  view
    list testId="strip" virtualized=true display="flex" width=300 height=100 overflow-x="scroll" overflow-y="hidden" estimated-item-width=150 scroll-snap-type="x mandatory"
      for n in range(1000) key=n
        view width=150 height=100 flex-shrink=0 scroll-snap-align="start" scroll-snap-stop=(n == 100 || n == 2 ? "always" : "normal")
"#,
    );
    let strip = id(&p, "strip");
    // The touch's rows carry their `always`, an unmounted one's too, and say
    // the scroller is a list.
    let told = p.snap(strip, false).unwrap();
    assert!(told.areas.iter().any(|a| a.from == 300. && a.stop));
    assert_eq!(p.reader_snaps(&[strip], 1.)[0] & 8, 8);
    // A release at 900 projected to 30,000: the rows there, and the first
    // `always` row past the release on the way (card 100, at 15,000).
    let words = p.reader_snaps_around(strip, false, 1800., 60_000., 2.);
    let f = |w: i32| f32::from_bits(w as u32) / 2.;
    let areas: Vec<(f32, bool)> = (0..words[0] as usize)
        .map(|k| (f(words[1 + 3 * k]), words[3 + 3 * k] != 0))
        .collect();
    assert!(areas.contains(&(30_000., false)), "{areas:?}");
    assert!(areas.contains(&(15_000., true)), "{areas:?}");
    let snaps: Vec<SnapArea> = areas
        .iter()
        .map(|&(from, stop)| SnapArea {
            from,
            to: from,
            stop,
            id: 0,
        })
        .collect();
    assert_eq!(
        flicked(&snaps, 30_000., 900., (900., 1.), true, 300.),
        Some(15_000.)
    );
}

/// LLP 1123 D9: `scrollIntoView` keeps the scroll margin that reads the
/// safe area where the target is: its owner's 30 below it, not the
/// document's 0.
#[test]
fn a_reveal_keeps_a_margin_that_reads_the_targets_inset() {
    let mut p = boot(
        r#"component App
  view
    column width=400 height=600
      scroll testId="list" width=400 height=200
        column
          view width=400 height=500 flex-shrink=0
          view id="target" testId="target" width=400 height=40 flex-shrink=0 scroll-margin-bottom="var(--exact-safe-area-inset-bottom)"
          view width=400 height=500 flex-shrink=0
"#,
    );
    let kernel = p.host.runner_mut().kernel_mut();
    let root = kernel.roots()[0];
    let local = Some([0.0, 0.0, 30.0, 0.0]);
    kernel.set_local_insets(root, local).unwrap();
    let (list, target) = (id(&p, "list"), id(&p, "target"));
    let (name, end) = (
        exact_plan::Value::str("target"),
        exact_plan::Value::str("end"),
    );
    p.scroll_element_into_view(&[name, end]);
    let port = p.box_of(list).unwrap().rect;
    let shown = p.box_of(target).unwrap().rect;
    assert_eq!(
        shown.1 + shown.3,
        port.1 + port.3 - 30.0,
        "{shown:?} in {port:?}"
    );
}

const READER_PAGER: &str = r#"component App
  state inserted = false
  action insert
    inserted = true
  view
    main width="100%" height="100%"
      scroll testId="pager" overflow-x="scroll" overflow-y="hidden" scroll-snap-type="x mandatory" display="flex" width="100%" height=200
        if inserted
          box width="100%" flex-shrink=0 scroll-snap-align="start"
        for i in [1, 2, 3] key=i
          box width="100%" height=200 flex-shrink=0 scroll-snap-align="start"
            text "Page"
      button testId="insert" press=insert
        text "Insert"
"#;

fn present(p: &mut Presenter<()>) {
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
}

fn reader_pager(p: &mut Presenter<()>) -> ViewId {
    present(p);
    let pager = id(p, "pager");
    p.reader_under(50., 50., 1.);
    p.reader_snaps(&[pager], 1.);
    assert!(p.reader_knows(pager));
    pager
}

#[test]
fn the_first_resized_picture_and_reader_statement_keep_the_same_page() {
    for (page, start, end) in [
        (1., 400., 900.),
        (1., 900., 400.),
        (2., 400., 900.),
        (2., 900., 400.),
    ] {
        let mut p = boot(READER_PAGER);
        assert_eq!(p.resize(start, 600.), None);
        let pager = reader_pager(&mut p);
        p.reader_drag_began(pager);
        let from = page * start;
        assert_eq!(p.reader_scrolled(pager, (from, 0.), 0), Some((from, 0.)));
        p.reader_drag_ended(pager, (0., 0.), Some((from, 0.)), false);
        present(&mut p);
        p.reader_scrollers(&[], 1.);
        assert_eq!(p.resize(end, 600.), None);
        present(&mut p);
        let expected = page * end;
        assert_eq!(
            p.box_of(pager).unwrap().scroll,
            Some((expected, 0.)),
            "first frame, {from} -> {expected}"
        );
        let words = p.reader_scrollers(&[], 1.).unwrap();
        let row = words
            .chunks_exact(9)
            .find(|s| s[0] == pager as i32)
            .unwrap();
        assert_eq!(f32::from_bits(row[3] as u32), expected);
    }
}

#[test]
fn proximity_follows_a_page_on_insertion_only_if_it_was_snapped() {
    for (start, end) in [(400., 800.), (180., 180.)] {
        let mut p = boot(&READER_PAGER.replace("x mandatory", "x proximity"));
        let pager = reader_pager(&mut p);
        p.reader_drag_began(pager);
        p.reader_scrolled(pager, (start, 0.), 0);
        p.reader_drag_ended(pager, (0., 0.), Some((start, 0.)), false);
        present(&mut p);
        p.tap(id(&p, "insert")).unwrap();
        present(&mut p);
        assert_eq!(p.box_of(pager).unwrap().scroll, Some((end, 0.)));
    }
}

#[test]
fn a_resize_during_reader_motion_waits_for_its_landing() {
    for resize_before_lift in [true, false] {
        let mut p = boot(READER_PAGER);
        let pager = reader_pager(&mut p);
        p.reader_drag_began(pager);
        p.reader_scrolled(pager, (160., 0.), 0);
        if !resize_before_lift {
            p.reader_drag_ended(pager, (400., 0.), Some((400., 0.)), true);
        }
        p.resize(900., 600.);
        present(&mut p);
        assert_eq!(p.scroll_of(pager).0, 160.);
        if resize_before_lift {
            p.reader_drag_ended(pager, (400., 0.), Some((400., 0.)), true);
        }
        assert_eq!(p.reader_scrolled(pager, (240., 0.), 0), Some((240., 0.)));
        present(&mut p);
        assert_eq!(p.scroll_of(pager).0, 400.);
        p.reader_fling_ended(pager);
        assert!(p.dirty(), "the deferred snap owes a picture");
        present(&mut p);
        assert_eq!(p.box_of(pager).unwrap().scroll, Some((900., 0.)));
    }
}

#[test]
fn a_nested_pager_resnaps_while_its_parent_is_dragging() {
    let mut p = boot(
        r#"component App
  view
    scroll testId="outer" width="100%" height="100%" scroll-snap-type="y mandatory"
      scroll testId="inner" width="100%" height="100%" overflow-x="scroll" overflow-y="hidden" display="flex" scroll-snap-align="start" scroll-snap-type="x proximity"
        for i in [1, 2, 3] key=i
          box width="100%" height=100 flex-shrink=0 scroll-snap-align="start"
      box width="100%" height="100%" scroll-snap-align="start"
"#,
    );
    present(&mut p);
    let (outer, inner) = (id(&p, "outer"), id(&p, "inner"));
    p.reader_under(50., 50., 1.);
    p.reader_snaps(&[inner, outer], 1.);
    p.reader_drag_began(inner);
    p.reader_scrolled(inner, (400., 0.), 0);
    p.reader_drag_ended(inner, (0., 0.), None, false);
    p.reader_drag_began(outer);
    p.reader_scrolled(outer, (0., 240.), 0);
    p.resize(800., 900.);
    present(&mut p);
    assert_eq!(p.scroll_of(inner), (800., 0.));
    assert_eq!(p.scroll_of(outer), (0., 240.));
    p.reader_scrolled(outer, (0., 360.), 0);
    p.reader_drag_ended(outer, (0., 0.), None, false);
    present(&mut p);
    assert_eq!(p.scroll_of(outer), (0., 900.));
}

#[test]
fn a_fling_whose_old_target_is_past_the_new_end_keeps_its_area() {
    let mut p = boot(READER_PAGER);
    p.resize(900., 400.);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (450., 0.), 0);
    p.snap_destination(pager, false, 900., 900.);
    p.reader_drag_ended(pager, (500., 0.), Some((900., 0.)), true);
    p.resize(400., 600.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager).0, 450.);
    // The reader runs the fling up to its new limit.
    p.reader_scrolled(pager, (350., 0.), 0);
    p.reader_fling_ended(pager);
    present(&mut p);
    assert_eq!(p.box_of(pager).unwrap().scroll, Some((400., 0.)));
}

#[test]
fn a_layout_snap_waits_for_a_programmatic_scroll_on_its_axis() {
    let mut p = boot(READER_PAGER);
    let pager = reader_pager(&mut p);
    let start = p.host.now();
    p.begin_glide(pager, (0., 0.), (400., 0.));
    p.step_glides(start + 100.);
    let middle = p.scroll_of(pager);
    assert!(middle.0 > 0. && middle.0 < 400.);
    p.resize(900., 600.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), middle);
    p.step_glides(start + 500.);
    present(&mut p);
    assert_eq!(p.box_of(pager).unwrap().scroll, Some((900., 0.)));
}

#[test]
fn a_finger_that_catches_a_settle_uses_the_resized_pages() {
    let mut p = boot(READER_PAGER);
    p.resize(800., 600.);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (300., 0.), 0);
    p.snap_destination(pager, false, 800., 800.);
    p.reader_drag_ended(pager, (500., 0.), Some((800., 0.)), true);
    p.resize(400., 600.);
    present(&mut p);
    p.reader_fling_ended(pager);
    p.reader_under(50., 50., 1.);
    p.reader_snaps(&[pager], 1.);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (500., 0.), 0);
    p.reader_drag_ended(pager, (0., 0.), None, false);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (800., 0.));
}

#[test]
fn a_clamp_keeps_both_a_free_proximity_offset_and_a_snap_identity() {
    let mut p = boot(&READER_PAGER.replace("x mandatory", "x proximity"));
    p.resize(800., 600.);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (1200., 0.));
    p.resize(400., 600.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (800., 0.));
    let a = area(900., 1800., SnapAlign::Start, false, 900., (0., 1800.));
    assert_eq!(
        resnap(&[SnapArea { id: 2, ..a }], (2, 900.), 800., true, 900.),
        Some(900.)
    );
}

#[test]
fn a_glide_cut_short_by_the_new_range_keeps_its_page() {
    for ticks in [&[150., 200., 250., 300., 400.][..], &[400.][..]] {
        let mut p = boot(READER_PAGER);
        p.resize(900., 600.);
        let pager = reader_pager(&mut p);
        let start = p.host.now();
        p.begin_glide(pager, (0., 0.), (900., 0.));
        p.step_glides(start + 100.);
        p.resize(400., 600.);
        present(&mut p);
        for &t in ticks {
            p.step_glides(start + t);
            p.pump(start + t);
            present(&mut p);
        }
        assert_eq!(p.scroll_of(pager), (400., 0.));
    }
}

#[test]
fn a_hidden_pager_does_not_request_geometry_for_every_picture() {
    let mut p = boot(&READER_PAGER.replace(
        r#"display="flex""#,
        r#"display=(inserted ? "none" : "flex")"#,
    ));
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (400., 0.));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert!(!p.snap_probe_needed());
}

#[test]
fn a_tap_after_a_resized_fling_leaves_its_landing_due() {
    let mut p = boot(READER_PAGER);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (160., 0.), 0);
    p.reader_drag_ended(pager, (400., 0.), Some((400., 0.)), true);
    p.resize(900., 600.);
    present(&mut p);
    p.reader_scrolled(pager, (240., 0.), 0);
    p.reader_fling_ended(pager);
    p.reader_under(50., 50., 1.);
    p.reader_snaps(&[pager], 1.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (900., 0.));
}

#[test]
fn mandatory_recovers_a_free_offset_and_a_deleted_end_page() {
    let source = READER_PAGER
        .replace("if inserted", "if false")
        .replace(
            "for i in [1, 2, 3] key=i",
            "for i in (inserted ? [1, 2] : [1, 2, 3]) key=i",
        )
        .replace(
            "      button testId=",
            "        box width=100 height=200 flex-shrink=0\n      button testId=",
        );
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (800., 0.));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (400., 0.));
    p.resize(914., 600.);
    present(&mut p);
    p.scroll_to(pager, (712., 0.));
    p.resize(411., 600.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (411., 0.));
}

#[test]
fn layout_of_a_sibling_does_not_probe_a_resting_pager() {
    let source = READER_PAGER
        .replace("if inserted", "if false")
        .replace("press=insert", "press=insert width=(inserted ? 102 : 100)");
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (400., 0.));
    p.tap(id(&p, "insert")).unwrap();
    assert!(!p.snap_probe_needed());
    assert_eq!(p.snap_layout.scanned, Some(p.host.kernel().layout_stamp()));
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (400., 0.));
}

#[test]
fn a_deferred_wheel_finishes_before_the_pager_follows_its_area() {
    let mut p = boot(READER_PAGER);
    let pager = id(&p, "pager");
    p.set_deferred_collections(true);
    p.wheel_at(50., 50., 40., 0.);
    assert_eq!(p.scroll_of(pager), (400., 0.));
    p.resize(900., 600.);
    p.frame();
    assert_eq!(p.scroll_of(pager), (400., 0.));
    p.run_deferred_collections();
    p.frame();
    assert_eq!(p.scroll_of(pager), (900., 0.));
}

#[test]
fn resizing_a_flex_sibling_moves_the_snapped_page() {
    let source = READER_PAGER
        .replace("if inserted", "if false")
        .replace("main width=", "main display=\"flex\" width=")
        .replace(
            "display=\"flex\" width=\"100%\" height=200",
            "display=\"flex\" flex=1 min-width=0 height=200",
        )
        .replace(
            "press=insert",
            "press=insert width=(inserted ? 300 : 100) flex-shrink=0",
        );
    let mut p = boot(&source);
    p.resize(900., 600.);
    let pager = reader_pager(&mut p);
    assert_eq!(p.box_of(pager).unwrap().rect.2, 800.);
    p.scroll_to(pager, (800., 0.));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert_eq!(p.box_of(pager).unwrap().rect.2, 600.);
    assert_eq!(p.scroll_of(pager), (600., 0.));
}

#[test]
fn an_app_scroll_in_the_new_layout_supersedes_the_old_fling() {
    let source = READER_PAGER.replace("if inserted", "if false").replace(
        "testId=\"pager\"",
        "testId=\"pager\" scrollLeft=(inserted ? 800 : 0)",
    );
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (160., 0.), 0);
    p.reader_drag_ended(pager, (400., 0.), Some((400., 0.)), true);
    p.resize(800., 600.);
    present(&mut p);
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    p.reader_fling_ended(pager);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (800., 0.));
}

#[test]
fn enabling_mandatory_from_proximity_snaps_a_free_offset() {
    let source = READER_PAGER.replace("if inserted", "if false").replace(
        "\"x mandatory\"",
        "(inserted ? \"x mandatory\" : \"x proximity\")",
    );
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (180., 0.));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (0., 0.));
}

#[test]
fn a_settle_remembers_its_page_across_two_resizes_or_a_fractional_pixel() {
    for (width, scale, landing, resized) in [
        (900., 1., 800., true),
        (900., 2.625, 800., true),
        (402., 0.75, 302. / 0.75, false),
    ] {
        let mut p = boot(&READER_PAGER.replace("x mandatory", "x proximity"));
        p.resize(width, 600.);
        let pager = reader_pager(&mut p);
        p.reader_snaps(&[pager], scale);
        p.reader_drag_began(pager);
        p.reader_scrolled(pager, (160., 0.), 0);
        if resized {
            p.resize(400., 600.);
            present(&mut p);
        }
        p.reader_snap_target(pager, false, width * scale, landing * scale, scale);
        p.reader_drag_ended(pager, (400., 0.), Some((landing, 0.)), true);
        if resized {
            p.resize(500., 600.);
            present(&mut p);
        }
        p.reader_scrolled(pager, (landing - 160., 0.), 0);
        p.reader_fling_ended(pager);
        present(&mut p);
        p.resize(500., 600.);
        present(&mut p);
        assert_eq!(p.scroll_of(pager), (500., 0.));
    }
}

#[test]
fn an_untouched_pager_follows_its_first_page_when_content_is_prepended() {
    for kind in ["x mandatory", "x proximity"] {
        let mut p = boot(&READER_PAGER.replace("x mandatory", kind));
        let pager = id(&p, "pager");
        assert!(p.snap_layout.axes.contains_key(&(pager, false)));
        p.tap(id(&p, "insert")).unwrap();
        present(&mut p);
        assert_eq!(p.scroll_of(pager), (400., 0.));
        p.resize(900., 600.);
        present(&mut p);
        assert_eq!(p.scroll_of(pager), (900., 0.));
    }
}

#[test]
fn a_backward_glide_follows_its_destination_after_a_clamp() {
    let mut p = boot(READER_PAGER);
    p.resize(800., 600.);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (1600., 0.));
    let start = p.host.now();
    p.begin_glide(pager, (1600., 0.), (800., 0.));
    p.step_glides(start + 20.);
    assert!(p.scroll_of(pager).0 > 1001.);
    p.resize(500., 600.);
    present(&mut p);
    p.step_glides(start + 400.);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (500., 0.));
}

#[test]
fn a_release_on_the_new_end_uses_the_area_from_its_touch() {
    let mut p = boot(READER_PAGER);
    p.resize(800., 600.);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (1200., 0.), 0);
    p.resize(400., 600.);
    present(&mut p);
    p.reader_snap_target(pager, false, 800., 800., 1.);
    p.reader_drag_ended(pager, (0., 0.), None, false);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (400., 0.));
}

#[test]
fn a_perpendicular_drag_preserves_the_other_axis_pending_rest() {
    let source = READER_PAGER
        .replace("overflow-y=\"hidden\"", "overflow-y=\"scroll\"")
        .replace("height=200 flex-shrink=0", "height=800 flex-shrink=0");
    let mut p = boot(&source);
    let pager = reader_pager(&mut p);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (160., 0.), 0);
    p.reader_drag_ended(pager, (400., 0.), Some((400., 0.)), true);
    p.resize(900., 600.);
    present(&mut p);
    p.reader_scrolled(pager, (240., 0.), 0);
    p.reader_fling_ended(pager);
    p.reader_under(50., 50., 1.);
    p.reader_snaps(&[pager], 1.);
    p.reader_drag_began(pager);
    p.reader_scrolled(pager, (0., 80.), 0);
    p.reader_drag_ended(pager, (0., 0.), None, false);
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (900., 80.));
}

#[test]
fn enabling_mandatory_snap_settles_the_first_picture() {
    let source = READER_PAGER
        .replace("if inserted", "if false")
        .replace("\"x mandatory\"", "(inserted ? \"x mandatory\" : \"none\")");
    let mut p = boot(&source);
    assert_eq!(p.snap_layout.probe_paints, 0);
    let pager = reader_pager(&mut p);
    p.scroll_to(pager, (260., 0.));
    present(&mut p);
    assert_eq!(p.scroll_of(pager), (260., 0.));
    p.tap(id(&p, "insert")).unwrap();
    present(&mut p);
    assert_eq!(p.box_of(pager).unwrap().scroll, Some((400., 0.)));
    let words = p.reader_scrollers(&[], 1.).unwrap();
    let row = words
        .chunks_exact(9)
        .find(|s| s[0] == pager as i32)
        .unwrap();
    assert_eq!(f32::from_bits(row[3] as u32), 400.);
    assert!(!p.snap_probe_needed());
}

#[test]
fn enabling_mandatory_with_a_scroll_request_settles_the_request() {
    for bound in [false, true] {
        let mut source = READER_PAGER
            .replace("if inserted", "if false")
            .replace("\"x mandatory\"", "(inserted ? \"x mandatory\" : \"none\")");
        if bound {
            source = source.replace(
                "testId=\"pager\"",
                "testId=\"pager\" scrollLeft=(inserted ? 260 : 0)",
            );
        }
        let mut p = boot(&source);
        let pager = reader_pager(&mut p);
        p.tap(id(&p, "insert")).unwrap();
        if !bound {
            p.scroll_to(pager, (260., 0.));
        }
        present(&mut p);
        assert_eq!(p.scroll_of(pager), (400., 0.), "bound={bound}");
        assert!(p.pending_model_scroll(pager).is_none());
        assert!(!p.snap_probe_needed());
    }
}

#[path = "snap_change_tests.rs"]
mod changes;
