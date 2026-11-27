use super::*;
use exact_plan::Value;

#[test]
fn two_files_over_an_editor_drop_once_on_its_nearest_target_in_order() {
    let source = r#"component App
  state calls = 0
  state names = ""
  state enters = 0
  state leaves = 0
  action received(e: DragEvent)
    names = e.files.join("|")
    calls = calls + 1
  action outer(e: DragEvent)
    calls = calls + 100
  action entered(e: DragEvent)
    enters = enters + 1
  action left(e: DragEvent)
    leaves = leaves + 1
  view
    column width=300 height=200 drop=outer
      column width=220 height=120 drop=received dragenter=entered dragleave=left
        textarea testId="editor" value="original text" width=180 height=80
"#;
    let mut p = booted(source);
    let kernel = p.host.kernel();
    let field = kernel
        .node_by_key(kernel.find_by_test_id("editor")[0])
        .unwrap()
        .id;
    let rect = p.rect_of(field).unwrap();
    let point = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
    assert_eq!(p.hit(point.0, point.1), Some(field));
    let held = KeyModifiers::default();
    assert!(p.drag_over(point, held).is_some());
    assert!(p.drag_over(point, held).is_some());
    p.drag_left();
    assert!(p.drag_over(point, held).is_some());
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..");
    let files = [root.join("README.md"), root.join("docs/testing.md")];
    let reply = p
        .agent_drag(Some(field), &serde_json::json!({"drop":files}))
        .unwrap();
    let reply: serde_json::Value = serde_json::from_str(&reply).unwrap();
    assert_eq!(reply["drop"], 2);
    assert_eq!(reply["delivery"], "presenter");
    p.drag_left();
    let r = p.host.runner();
    assert_eq!(r.slot("calls"), Some(&Value::Number(1.)));
    assert_eq!(r.slot("enters"), Some(&Value::Number(2.)));
    assert_eq!(r.slot("leaves"), Some(&Value::Number(1.)));
    let docs: Vec<_> = r
        .slot("names")
        .and_then(Value::as_str)
        .unwrap()
        .split('|')
        .collect();
    assert_eq!(docs.len(), 2);
    for (doc, name) in docs.iter().zip(["/README.md", "/testing.md"]) {
        assert!(doc.starts_with("doc:/") && doc.ends_with(name), "{doc}");
    }
    assert_eq!(p.field_text(field), "original text");
}

#[test]
fn text_and_a_link_over_an_editor_drop_on_its_nearest_target() {
    let source = r#"component App
  state text = ""
  state url = ""
  state files = -1
  action received(e: DragEvent)
    text = e.text
    url = e.url
    files = e.files.length
  view
    column width=300 height=200 drop=received
      textarea testId="editor" value="original text" width=180 height=80
"#;
    let mut p = booted(source);
    let kernel = p.host.kernel();
    let field = kernel
        .node_by_key(kernel.find_by_test_id("editor")[0])
        .unwrap()
        .id;
    let empty = p.agent_drag(Some(field), &serde_json::json!({"drop": []}));
    assert!(empty
        .unwrap_err()
        .contains("requires a file path, --text or --url"));
    let request = serde_json::json!({"drop": [], "text": "Mount Fuji\nJapan", "url": "https://example.com/fuji"});
    p.agent_drag(Some(field), &request).unwrap();
    let r = p.host.runner();
    assert_eq!(r.slot("text"), Some(&Value::str("Mount Fuji\nJapan")));
    assert_eq!(r.slot("url"), Some(&Value::str("https://example.com/fuji")));
    assert_eq!(r.slot("files"), Some(&Value::Number(0.)));
    // Exact's fields take no drag on this host, so the text goes to the drop above
    assert_eq!(p.field_text(field), "original text");
}

/// A zone that counts its drops, enters and leaves, keeping the last drop's text.
fn zone() -> (Presenter<()>, (f32, f32), ViewId) {
    let source = r#"component App
  state drops = 0
  state enters = 0
  state leaves = 0
  state text = ""
  action received(e: DragEvent)
    drops = drops + 1
    text = e.text
  action entered(e: DragEvent)
    enters = enters + 1
  action left(e: DragEvent)
    leaves = leaves + 1
  view
    column width=300 height=200 testId="zone" drop=received dragenter=entered dragleave=left
      textarea testId="editor" value="original text" width=180 height=80
      textarea testId="off" value="off" disabled=true width=180 height=40
"#;
    let p = booted(source);
    let kernel = p.host.kernel();
    let field = kernel
        .node_by_key(kernel.find_by_test_id("editor")[0])
        .unwrap()
        .id;
    (p, (250., 150.), field)
}

fn counts(p: &Presenter<()>) -> [f64; 3] {
    let r = p.host.runner();
    ["drops", "enters", "leaves"].map(|k| match r.slot(k) {
        Some(Value::Number(n)) => *n,
        other => panic!("{k}: {other:?}"),
    })
}

fn text(text: &str) -> Dropped {
    Dropped {
        text: text.into(),
        ..Default::default()
    }
}

#[test]
fn a_pending_drop_holds_a_newer_drags_enter_until_it_lands() {
    let (mut p, point, _) = zone();
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let id = p.drop_released(point, held).unwrap();
    assert_eq!(
        counts(&p),
        [0., 1., 0.],
        "released: the zone neither drops nor leaves yet"
    );
    // A newer drag over the zone while the drop's items copy: its enter waits
    p.drag_over(point, held);
    assert_eq!(counts(&p), [0., 1., 0.]);
    assert!(p.drop_landed(id, &text("Fuji")).is_some());
    assert_eq!(
        counts(&p),
        [1., 2., 0.],
        "the drop, then the newer drag's enter"
    );
    assert_eq!(p.host.runner().slot("text"), Some(&Value::str("Fuji")));
    p.drag_left();
    assert_eq!(counts(&p), [1., 2., 1.]);
    assert_eq!(p.drop_landed(id, &text("again")), None, "a drop lands once");
}

#[test]
fn a_newer_drag_that_leaves_while_it_waits_sends_nothing() {
    let (mut p, point, _) = zone();
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let id = p.drop_released(point, held).unwrap();
    p.drag_over(point, held);
    p.drag_left();
    p.drop_landed(id, &text("Fuji"));
    assert_eq!(
        counts(&p),
        [1., 1., 0.],
        "the waiting enter was cancelled, and its leave with it"
    );
}

#[test]
fn a_newer_drag_that_drops_first_hears_its_waiting_enter_then_drops() {
    let (mut p, point, _) = zone();
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let first = p.drop_released(point, held).unwrap();
    p.drag_over(point, held);
    let second = p.drop_released(point, held).unwrap();
    assert_eq!(
        counts(&p),
        [0., 2., 0.],
        "the newer drag's enter, as it drops"
    );
    p.drop_landed(second, &text("second"));
    p.drop_landed(first, &text("first"));
    assert_eq!(counts(&p), [2., 2., 0.]);
    assert_eq!(p.host.runner().slot("text"), Some(&Value::str("first")));
}

/// A zone that logs its enters, leaves and drops, and that its first
/// drop moves away from where the drag was.
fn moving_zone() -> Presenter<()> {
    booted(
        r#"component App
  state log = ""
  state moved = false
  action received(e: DragEvent)
    log = `${log}drop ${e.text}, `
    moved = true
  action entered(e: DragEvent)
    log = `${log}enter, `
  action left(e: DragEvent)
    log = `${log}leave, `
  view
    column width=300 height=200 margin-top=(moved ? 400 : 0) testId="zone" drop=received dragenter=entered dragleave=left
"#,
    )
}

#[test]
fn a_drop_released_behind_another_takes_its_target_first_and_hears_its_enter_after() {
    let (mut p, point) = (moving_zone(), (150., 100.));
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let first = p.drop_released(point, held).unwrap();
    p.drag_over(point, held);
    let second = p.drop_released_behind(point, held, None).unwrap();
    assert_eq!(
        log(&p),
        "enter, ",
        "the second drag's enter waits for the first drop"
    );
    assert!(p.drop_landed(first, &text("first")).is_some());
    assert_eq!(log(&p), "enter, drop first, enter, ");
    assert!(
        p.drop_landed(second, &text("second")).is_some(),
        "the zone taken at the release, though the first drop moved it"
    );
    assert_eq!(log(&p), "enter, drop first, enter, drop second, ");
}

#[test]
fn a_failed_drop_leaves_before_one_released_behind_it_enters() {
    let (mut p, point) = (moving_zone(), (150., 100.));
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let first = p.drop_released(point, held).unwrap();
    p.drag_over(point, held);
    let second = p.drop_released_behind(point, held, None).unwrap();
    assert_eq!(p.drop_landed(first, &Dropped::default()), None);
    assert_eq!(log(&p), "enter, leave, enter, ");
    assert!(p.drop_landed(second, &text("second")).is_some());
    assert_eq!(log(&p), "enter, leave, enter, drop second, ");
}

#[test]
fn a_drop_whose_items_never_arrived_leaves_its_chain() {
    let (mut p, point, _) = zone();
    let held = KeyModifiers::default();
    p.drag_over(point, held);
    let id = p.drop_released(point, held).unwrap();
    assert_eq!(p.drop_landed(id, &Dropped::default()), None);
    assert_eq!(counts(&p), [0., 1., 1.]);
}

#[test]
fn a_field_keeps_text_and_links_and_gives_files_away() {
    /// A shell that hosts fields, as Android's reader does, measuring none.
    struct Shell;
    impl crate::fields::FieldMetrics for Shell {
        fn measure(
            &self,
            face: &crate::fields::FieldFace,
            _: &str,
            width: Option<f32>,
        ) -> [f32; 3] {
            [width.unwrap_or(0.), face.size, face.size]
        }
    }
    let _hosting = crate::fields::host_here(&Shell, 1.0);
    let (mut p, _, field) = zone();
    let rect = p.rect_of(field).unwrap();
    let over = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
    assert_eq!(
        p.drop_owner(field),
        2,
        "the zone above the field takes drops"
    );
    assert!(p.field_keeps(over, &text("Fuji")));
    assert!(p.field_keeps(
        over,
        &Dropped {
            url: "https://example.com".into(),
            ..Default::default()
        }
    ));
    assert!(!p.field_keeps(
        over,
        &Dropped {
            paths: vec!["/tmp/a.txt".into()],
            ..Default::default()
        }
    ));
    assert!(
        !p.field_keeps((250., 150.), &text("Fuji")),
        "no field there"
    );
    let kernel = p.host.kernel();
    let off = kernel
        .node_by_key(kernel.find_by_test_id("off")[0])
        .unwrap()
        .id;
    let rect = p.rect_of(off).unwrap();
    let disabled = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
    assert!(
        !p.field_keeps(disabled, &text("Fuji")),
        "a disabled field keeps no drag"
    );
    assert_eq!(
        p.field_drop(field, over),
        2,
        "the field, under a zone that takes drops"
    );
    assert_eq!(
        p.field_drop(off, disabled),
        -1,
        "a disabled field takes no drop"
    );
    assert_eq!(
        p.field_drop(field, (250., 150.)),
        -1,
        "the zone is painted there"
    );
}

/// A scroller filling the window, 400 points of content in a 200-point
/// port, whose last 100 points are a zone that hears `drop` and
/// `dragenter` when `listens`, else none; and the scroller's id.
fn below_the_fold(listens: bool) -> (Presenter<()>, ViewId) {
    let zone = if listens {
        " drop=received dragenter=entered"
    } else {
        ""
    };
    let mut p = booted(&format!(
        r#"component App
  state enters = 0
  state got = ""
  action entered(e: DragEvent)
    enters = enters + 1
  action received(e: DragEvent)
    got = e.text
  view
    scroll testId="list" width=300 height=200 overflow-y="scroll"
      box width=300 height=300
      box testId="zone" width=300 height=100{zone}
"#
    ));
    let list = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("list")[0])
        .unwrap()
        .id;
    p.frame();
    (p, list)
}

/// One display frame at `at` ms: the clock moves, then the motion step.
fn frame_at(p: &mut Presenter<()>, at: f64) {
    p.clock(at);
    p.tick(at);
    p.frame();
}

#[test]
fn a_drag_held_near_a_scrollers_edge_scrolls_it_until_a_zone_under_it_takes_the_drop() {
    let (mut p, list) = below_the_fold(true);
    let held = KeyModifiers::default();
    let point = (150., 190.);
    assert!(
        p.drag_over(point, held).is_none(),
        "nothing takes it above the fold"
    );
    assert_eq!(p.drag_edge(), Some((list, true, 720.)));
    assert!(p.needs_animation_frame());
    frame_at(&mut p, 1000.);
    assert_eq!(p.scroll_of(list).1, 0., "the first step is zero");
    frame_at(&mut p, 1016.);
    assert!((p.scroll_of(list).1 - 720. * 0.016).abs() < 0.001);
    frame_at(&mut p, 2000.);
    assert!(
        (p.scroll_of(list).1 - 720. * 0.048).abs() < 0.001,
        "a late frame catches up 32 ms at most"
    );
    let mut at = 2000.;
    while p.drag_edge().is_some() && at < 4000. {
        at += 16.;
        frame_at(&mut p, at);
    }
    assert_eq!(p.scroll_of(list).1, 200., "the drag scrolled to the end");
    assert!(p.drag_edge().is_none(), "and stops there");
    assert_eq!(
        p.host.runner().slot("enters"),
        Some(&Value::Number(1.)),
        "the zone came under the still drag"
    );
    let dropped = Dropped {
        text: "Mount Fuji".into(),
        ..Default::default()
    };
    assert!(p.dropped(point, held, &dropped).is_some());
    assert_eq!(p.host.runner().slot("got"), Some(&Value::str("Mount Fuji")));
    assert!(p.drag_edge().is_none());
}

#[test]
fn a_window_whose_nodes_hear_no_drag_does_not_scroll_under_one() {
    let (mut p, list) = below_the_fold(false);
    p.drag_over((150., 190.), KeyModifiers::default());
    assert!(p.drag_edge().is_none());
    frame_at(&mut p, 1000.);
    frame_at(&mut p, 1016.);
    assert_eq!(p.scroll_of(list).1, 0.);
}

#[test]
fn a_drag_scrolls_only_while_it_is_over_the_window_and_in_a_band() {
    let (mut p, list) = below_the_fold(true);
    let held = KeyModifiers::default();
    p.drag_over((150., 100.), held);
    assert!(p.drag_edge().is_none(), "the middle of the port");
    p.drag_over((150., 10.), held);
    assert!(
        p.drag_edge().is_none(),
        "the top band of a scroller at its start"
    );
    p.drag_over((150., 190.), held);
    frame_at(&mut p, 1000.);
    frame_at(&mut p, 1016.);
    let scrolled = p.scroll_of(list).1;
    assert!(scrolled > 0.);
    p.drag_over((150., 10.), held);
    assert_eq!(
        p.drag_edge(),
        Some((list, true, -720.)),
        "the top band once it can move up"
    );
    p.drag_left();
    assert!(p.drag_edge().is_none(), "a drag that left");
    frame_at(&mut p, 1032.);
    assert_eq!(p.scroll_of(list).1, scrolled);
}

#[test]
fn a_short_port_held_near_both_edges_scrolls_toward_the_nearer_and_stops_at_its_end() {
    let mut p = booted(
        r#"component App
  state n = 0
  action received(e: DragEvent)
    n = n + 1
  view
    column width=300 height=200 drop=received
      scroll testId="short" width=300 height=40 overflow-y="scroll"
        box width=300 height=100
"#,
    );
    let kernel = p.host.kernel();
    let short = kernel
        .node_by_key(kernel.find_by_test_id("short")[0])
        .unwrap()
        .id;
    p.frame();
    p.drag_over((150., 30.), KeyModifiers::default());
    assert_eq!(
        p.drag_edge(),
        Some((short, true, 720.)),
        "the bottom edge is nearer"
    );
    let mut at = 1000.;
    while at < 2000. {
        frame_at(&mut p, at);
        at += 16.;
    }
    assert_eq!(p.scroll_of(short).1, 60., "at its end");
    assert!(p.drag_edge().is_none(), "and not back toward the top");
    assert!(!p.needs_animation_frame());
}

#[test]
fn a_new_drag_catches_up_no_time_from_an_earlier_one() {
    let (mut p, list) = below_the_fold(true);
    let held = KeyModifiers::default();
    p.drag_over((150., 190.), held);
    frame_at(&mut p, 1000.);
    frame_at(&mut p, 1016.);
    let scrolled = p.scroll_of(list).1;
    p.drag_left();
    p.drag_over((150., 190.), held);
    frame_at(&mut p, 5000.);
    assert_eq!(
        p.scroll_of(list).1,
        scrolled,
        "the new drag's first step is zero"
    );
}

#[test]
fn a_commit_that_mounts_the_first_drop_under_a_still_drag_starts_its_scroll() {
    let mut p = booted(
        r#"component App
  state listens = false
  state n = 0
  action listen
    listens = true
  action received(e: DragEvent)
    n = n + 1
  view
    column width=300 height=200
      button testId="go" press=listen height=20
        text "go"
      scroll testId="list" width=300 height=180 overflow-y="scroll"
        box width=300 height=300
        if listens
          box width=300 height=100 drop=received
"#,
    );
    let kernel = p.host.kernel();
    let list = kernel
        .node_by_key(kernel.find_by_test_id("list")[0])
        .unwrap()
        .id;
    let go = kernel
        .node_by_key(kernel.find_by_test_id("go")[0])
        .unwrap()
        .id;
    p.frame();
    p.drag_over((150., 190.), KeyModifiers::default());
    assert!(p.drag_edge().is_none(), "no node hears a drag's events yet");
    assert!(!p.needs_animation_frame());
    p.tap(go).unwrap();
    assert!(
        p.needs_animation_frame(),
        "the commit asks to find the still drag again"
    );
    let mut at = 1000.;
    while p.scroll_of(list).1 == 0. && at < 1200. {
        frame_at(&mut p, at);
        at += 16.;
    }
    assert_eq!(p.drag_edge().map(|e| e.0), Some(list));
    assert!(p.scroll_of(list).1 > 0., "and it scrolls");
}

#[test]
fn a_scroller_a_commit_mounts_under_a_still_drag_scrolls_once_it_is_painted() {
    let mut p = booted(
        r#"component App
  state shown = false
  state n = 0
  action show
    shown = true
  action received(e: DragEvent)
    n = n + 1
  view
    column width=300 height=200 drop=received
      button testId="go" press=show height=20
        text "go"
      if shown
        scroll testId="list" width=300 height=180 overflow-y="scroll"
          box width=300 height=400
"#,
    );
    let go = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("go")[0])
        .unwrap()
        .id;
    p.frame();
    p.drag_over((150., 190.), KeyModifiers::default());
    assert!(p.drag_edge().is_none(), "no scroller under the drag yet");
    p.tap(go).unwrap();
    assert!(
        p.needs_animation_frame(),
        "the commit asks to find the still drag again"
    );
    let kernel = p.host.kernel();
    let list = kernel
        .node_by_key(kernel.find_by_test_id("list")[0])
        .unwrap()
        .id;
    let mut at = 1000.;
    while p.scroll_of(list).1 == 0. && at < 1200. {
        frame_at(&mut p, at);
        at += 16.;
    }
    assert_eq!(
        p.drag_edge().map(|e| e.0),
        Some(list),
        "the scroller mounted under it"
    );
    assert!(p.scroll_of(list).1 > 0., "scrolls");
}

#[test]
fn on_an_attached_display_the_drag_is_found_again_once_the_display_acknowledges_the_commit() {
    let mut p = booted(
        r#"component App
  state shown = false
  state n = 0
  action show
    shown = true
  action received(e: DragEvent)
    n = n + 1
  view
    column width=300 height=200 drop=received
      button testId="go" press=show height=20
        text "go"
      if shown
        scroll testId="list" width=300 height=180 overflow-y="scroll"
          box width=300 height=400
"#,
    );
    let go = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("go")[0])
        .unwrap()
        .id;
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
    p.drag_over((150., 190.), KeyModifiers::default());
    p.tap(go).unwrap();
    let kernel = p.host.kernel();
    let list = kernel
        .node_by_key(kernel.find_by_test_id("list")[0])
        .unwrap()
        .id;
    let tick = |p: &mut Presenter<()>, at: f64| {
        p.clock(at);
        p.tick(at);
    };
    tick(&mut p, 1000.);
    let submitted = p.display_frame().unwrap();
    tick(&mut p, 1016.);
    assert!(
        p.drag_edge().is_none(),
        "a frame only submitted shows the old boxes"
    );
    assert!(
        p.needs_animation_frame(),
        "so the request to look again stays"
    );
    assert!(p.display_complete(&submitted));
    tick(&mut p, 1032.);
    assert_eq!(
        p.drag_edge().map(|e| e.0),
        Some(list),
        "found once the display acknowledged it"
    );
}

#[test]
fn a_right_to_left_scroller_held_at_its_left_edge_scrolls_into_negative_offsets() {
    let mut p = booted(
        r#"component App
  state n = 0
  action received(e: DragEvent)
    n = n + 1
  view
    column width=300 height=200 drop=received
      row testId="rtl" direction="rtl" overflow-x="auto" width=300 height=100
        box width=200 height=100 flex-shrink=0
        box width=200 height=100 flex-shrink=0
"#,
    );
    let rtl = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("rtl")[0])
        .unwrap()
        .id;
    p.frame();
    assert_eq!(
        p.scroll_of(rtl).0,
        0.,
        "a right-to-left scroller starts at zero"
    );
    p.drag_over((10., 50.), KeyModifiers::default());
    assert_eq!(
        p.drag_edge(),
        Some((rtl, false, -720.)),
        "its left edge, toward its negative minimum"
    );
    frame_at(&mut p, 1000.);
    frame_at(&mut p, 1016.);
    assert!(p.scroll_of(rtl).0 < 0.);
}

#[test]
fn an_enter_that_takes_the_last_listener_away_stops_the_scroll() {
    let mut p = booted(
        r#"component App
  state listening = true
  action entered(e: DragEvent)
    listening = false
  action received(e: DragEvent)
    listening = false
  view
    scroll testId="list" width=300 height=200 overflow-y="scroll"
      box width=300 height=300
      if listening
        box width=300 height=900 drop=received dragenter=entered
      else
        box width=300 height=900
"#,
    );
    let list = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("list")[0])
        .unwrap()
        .id;
    p.frame();
    p.drag_over((150., 190.), KeyModifiers::default());
    let listening =
        |p: &Presenter<()>| p.host.runner().slot("listening") == Some(&Value::Bool(true));
    let mut at = 1000.;
    while listening(&p) && at < 3000. {
        let before = p.scroll_of(list).1;
        frame_at(&mut p, at);
        at += 16.;
        if !listening(&p) {
            assert_eq!(
                p.scroll_of(list).1,
                before,
                "the frame whose enter took the listener away takes no step"
            );
        }
    }
    assert!(!listening(&p), "the zone came under the drag");
    frame_at(&mut p, at);
    frame_at(&mut p, at + 16.);
    assert!(p.drag_edge().is_none());
}

#[test]
fn the_agents_dragenter_near_an_edge_scrolls_nothing() {
    let mut p = booted(
        r#"component App
  state n = 0
  action received(e: DragEvent)
    n = n + 1
  view
    scroll testId="list" width=300 height=200 overflow-y="scroll"
      box width=300 height=180
      box testId="edge" width=300 height=20 drop=received
      box width=300 height=300
"#,
    );
    let kernel = p.host.kernel();
    let list = kernel
        .node_by_key(kernel.find_by_test_id("list")[0])
        .unwrap()
        .id;
    let edge = kernel
        .node_by_key(kernel.find_by_test_id("edge")[0])
        .unwrap()
        .id;
    p.frame();
    p.agent_drag(
        Some(edge),
        &serde_json::json!({"dragenter": [], "text": "a"}),
    )
    .unwrap();
    assert!(p.drag_edge().is_none(), "the agent's drag holds still");
    frame_at(&mut p, 1000.);
    frame_at(&mut p, 1016.);
    assert_eq!(p.scroll_of(list).1, 0.);
    p.drag_over((150., 190.), KeyModifiers::default());
    assert_eq!(
        p.drag_edge(),
        Some((list, true, 720.)),
        "the platform's at the same point scrolls"
    );
}

#[test]
fn an_inner_scroller_at_its_end_leaves_the_drag_to_the_one_around_it() {
    let mut p = booted(
        r#"component App
  state n = 0
  action received(e: DragEvent)
    n = n + 1
  view
    scroll testId="outer" width=300 height=200 overflow-y="scroll" drop=received
      box width=300 height=100
      scroll testId="inner" width=300 height=100 overflow-y="scroll"
        box width=300 height=150
      box width=300 height=200
"#,
    );
    let id = |p: &Presenter<()>, test: &str| {
        p.host
            .kernel()
            .node_by_key(p.host.kernel().find_by_test_id(test)[0])
            .unwrap()
            .id
    };
    let (outer, inner) = (id(&p, "outer"), id(&p, "inner"));
    p.frame();
    let point = (150., 190.);
    p.drag_over(point, KeyModifiers::default());
    assert_eq!(
        p.drag_edge(),
        Some((inner, true, 720.)),
        "the innermost first"
    );
    let mut at = 1000.;
    while p.scroll_of(inner).1 < 50. && at < 3000. {
        frame_at(&mut p, at);
        at += 16.;
    }
    assert_eq!(p.scroll_of(inner).1, 50.);
    frame_at(&mut p, at);
    assert_eq!(
        p.drag_edge().map(|e| e.0),
        Some(outer),
        "then the one around it, from the same point"
    );
    frame_at(&mut p, at + 16.);
    assert!(p.scroll_of(outer).1 > 0.);
}

/// The app `source`, its first frame painted.
fn booted(source: &str) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (300., 200.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.frame();
    p
}

/// A zone that logs each event it hears: its kind, whether Shift was
/// held, and a drop's offsets and text.
fn logging_zone() -> (Presenter<()>, (f32, f32)) {
    let source = r#"component App
  state log = ""
  action received(e: DragEvent)
    log = `${log}drop ${e.shiftKey ? "S" : "-"} ${e.offsetX} ${e.offsetY} ${e.text};`
  action entered(e: DragEvent)
    log = `${log}enter ${e.shiftKey ? "S" : "-"};`
  action left(e: DragEvent)
    log = `${log}leave ${e.shiftKey ? "S" : "-"};`
  view
    column width=300 height=200 testId="zone" drop=received dragenter=entered dragleave=left
"#;
    (booted(source), (100., 50.))
}

fn log(p: &Presenter<()>) -> String {
    p.host
        .runner()
        .slot("log")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_owned()
}

const SHIFT: KeyModifiers = KeyModifiers {
    shift: true,
    ctrl: false,
    alt: false,
    meta: false,
};

#[test]
fn a_failed_drop_leaves_with_its_own_record_before_the_newer_drag_enters() {
    let (mut p, point) = logging_zone();
    p.drag_over(point, SHIFT);
    let id = p.drop_released(point, SHIFT).unwrap();
    p.drag_over((150., 60.), KeyModifiers::default());
    assert_eq!(log(&p), "enter S;", "the newer drag's enter waits");
    p.drop_landed(id, &Dropped::default());
    assert_eq!(
        log(&p),
        "enter S;leave S;enter -;",
        "the failed drop's leave, with its Shift, then the newer enter"
    );
}

#[test]
fn a_drop_hears_the_record_it_had_at_its_release() {
    let (mut p, point) = logging_zone();
    p.drag_over(point, SHIFT);
    let id = p.drop_released(point, SHIFT).unwrap();
    p.drag_over((150., 60.), KeyModifiers::default());
    p.drop_landed(id, &text("Fuji"));
    assert_eq!(log(&p), "enter S;drop S 100 50 Fuji;enter -;");
}

#[test]
fn a_newer_drag_a_field_takes_hears_its_waiting_enter_then_its_leave() {
    let (mut p, point) = logging_zone();
    p.drag_over(point, KeyModifiers::default());
    let id = p.drop_released(point, KeyModifiers::default()).unwrap();
    p.drag_dropped(point, SHIFT);
    assert_eq!(log(&p), "enter -;enter S;leave S;");
    p.drop_landed(id, &text("Fuji"));
    assert_eq!(
        log(&p),
        "enter -;enter S;leave S;drop - 100 50 Fuji;",
        "the drag is over: no enter after the drop"
    );
}

#[test]
fn a_drop_whose_target_was_renewed_leaves_instead() {
    let (mut p, point) = logging_zone();
    p.drag_over(point, KeyModifiers::default());
    let id = p.drop_released(point, KeyModifiers::default()).unwrap();
    let kernel = p.host.kernel();
    let zone = kernel
        .node_by_key(kernel.find_by_test_id("zone")[0])
        .unwrap()
        .id;
    p.drag_renewed(&[zone]);
    assert_eq!(p.drop_landed(id, &text("Fuji")), None);
    assert_eq!(
        log(&p),
        "enter -;",
        "a new mount heard no enter, so it hears no leave either"
    );
}

#[test]
fn a_new_plan_numbers_its_drops_after_the_old_ones() {
    let (mut p, point) = logging_zone();
    let old = p.drop_released(point, KeyModifiers::default()).unwrap();
    p.drag_in.reset();
    let new = p.drop_released(point, KeyModifiers::default()).unwrap();
    assert_ne!(old, new);
    assert_eq!(
        p.drop_landed(old, &text("late")),
        None,
        "the old plan's copy lands nowhere"
    );
    assert!(p.drop_landed(new, &text("Fuji")).is_some());
}

/// A zone, a button that disables itself in its own `dragenter`, in a
/// column that also takes drops; each counts its drops.
fn self_disabling_zone() -> (Presenter<()>, ViewId) {
    let source = r#"component App
  state off = false
  state outer = 0
  state inner = 0
  action toOuter(e: DragEvent)
    outer = outer + 1
  action toInner(e: DragEvent)
    inner = inner + 1
  action entered(e: DragEvent)
    off = true
  view
    column width=300 height=200 testId="outer" drop=toOuter
      button "Zone" testId="inner" width=200 height=100 disabled=off drop=toInner dragenter=entered
"#;
    let p = booted(source);
    let kernel = p.host.kernel();
    let outer = kernel
        .node_by_key(kernel.find_by_test_id("outer")[0])
        .unwrap()
        .id;
    (p, outer)
}

#[test]
fn an_enter_that_disables_the_target_at_the_release_does_not_move_the_drop() {
    let (mut p, _) = self_disabling_zone();
    let id = p
        .drop_released((100., 50.), KeyModifiers::default())
        .unwrap();
    assert_eq!(
        p.host.runner().slot("off"),
        Some(&Value::Bool(true)),
        "the zone's enter ran at the release"
    );
    assert_eq!(
        p.drop_landed(id, &text("Fuji")),
        None,
        "the zone it was released on takes no drop now"
    );
    let r = p.host.runner();
    assert_eq!(
        [r.slot("outer"), r.slot("inner")],
        [Some(&Value::Number(0.)), Some(&Value::Number(0.))]
    );
}

#[test]
fn a_hover_answers_with_the_target_after_its_enters_ran() {
    let (mut p, outer) = self_disabling_zone();
    assert_eq!(
            p.drag_over((100., 50.), KeyModifiers::default()),
            Some(outer),
            "the zone disabled itself as the drag came over it, so the column above takes the drop there"
        );
}

#[test]
fn an_enter_skipped_at_the_release_owes_no_leave_when_the_drop_fails() {
    let mut p = booted(
        r#"component App
  state off = false
  state log = ""
  action innerIn(e: DragEvent)
    off = true
    log = `${log}enter inner;`
  action innerOut(e: DragEvent)
    log = `${log}leave inner;`
  action outerIn(e: DragEvent)
    log = `${log}enter outer;`
  action outerOut(e: DragEvent)
    log = `${log}leave outer;`
  action received(e: DragEvent)
    log = `${log}drop;`
  view
    column width=300 height=200 disabled=off dragenter=outerIn dragleave=outerOut
      button "Zone" width=200 height=100 drop=received dragenter=innerIn dragleave=innerOut
"#,
    );
    let id = p
        .drop_released((100., 50.), KeyModifiers::default())
        .unwrap();
    assert_eq!(
        p.drop_landed(id, &Dropped::default()),
        None,
        "nothing arrived"
    );
    assert_eq!(
        log(&p),
        "enter inner;leave inner;",
        "the column was disabled before its enter, so it hears neither"
    );
}

#[test]
fn a_failed_drop_whose_leave_enables_a_zone_enters_the_drag_over_it() {
    let mut p = booted(
        r#"component App
  state off = true
  state log = ""
  action received(e: DragEvent)
    log = `${log}drop;`
  action left(e: DragEvent)
    off = false
    log = `${log}leave A;`
  action entered(e: DragEvent)
    log = `${log}enter B;`
  view
    column width=300 height=200
      button "A" width=200 height=100 drop=received dragleave=left
      button "B" width=200 height=100 disabled=off dragenter=entered
"#,
    );
    let id = p
        .drop_released((100., 50.), KeyModifiers::default())
        .unwrap();
    p.drag_over((100., 150.), KeyModifiers::default());
    assert_eq!(log(&p), "", "B is disabled, and A waits for its drop");
    assert_eq!(
        p.drop_landed(id, &Dropped::default()),
        None,
        "nothing arrived"
    );
    assert_eq!(
        log(&p),
        "leave A;enter B;",
        "A's leave enabled B, under the drag still over the window"
    );
}

#[test]
fn an_enter_to_a_node_hidden_at_the_release_owes_no_leave_when_the_drop_fails() {
    let mut p = booted(
        r#"component App
  state look = "flex"
  state log = ""
  action innerIn(e: DragEvent)
    look = "none"
    log = `${log}enter inner;`
  action innerOut(e: DragEvent)
    log = `${log}leave inner;`
  action outerIn(e: DragEvent)
    log = `${log}enter outer;`
  action outerOut(e: DragEvent)
    log = `${log}leave outer;`
  action shown(e: DragEvent)
    look = "flex"
  action received(e: DragEvent)
    log = `${log}drop;`
  view
    row width=300 height=200
      button "Show" width=100 height=100 dragenter=shown
      column width=150 height=200 display=look dragenter=outerIn dragleave=outerOut
        button "Zone" width=150 height=100 drop=received dragenter=innerIn dragleave=innerOut
"#,
    );
    let id = p
        .drop_released((150., 50.), KeyModifiers::default())
        .unwrap();
    assert_eq!(
        log(&p),
        "enter inner;",
        "the column was hidden before its enter"
    );
    p.drag_over((50., 50.), KeyModifiers::default());
    assert_eq!(
        p.drop_landed(id, &Dropped::default()),
        None,
        "nothing arrived"
    );
    assert_eq!(
        log(&p),
        "enter inner;leave inner;",
        "the column, shown again, hears no leave of the drop"
    );
}
