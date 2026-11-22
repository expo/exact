//! Kept rows moved rather than walked: their recordings through a scroll,
//! and their hits and texts in a walk that paints nothing.
use super::*;

/// A presenter over `source` at 100 by 100 points, its rows kept by [`Keeps`].
fn kept(source: &str) -> (Presenter<()>, Rc<RefCell<Heard>>) {
    kept_in(source, (100., 100.), false)
}

/// A presenter over `source` at `size`, its rows kept by [`Keeps`] in the
/// viewport's coordinates, or their own when `local`, as Canvas keeps them.
fn kept_in(source: &str, size: (f32, f32), local: bool) -> (Presenter<()>, Rc<RefCell<Heard>>) {
    let (p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        size,
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    keep(p, local)
}

fn keep(mut p: Presenter<()>, local: bool) -> (Presenter<()>, Rc<RefCell<Heard>>) {
    let heard = Rc::new(RefCell::new(Heard {
        viewport_row_coordinates: !local,
        ..Heard::default()
    }));
    p.reader_replaced(Box::new(Keeps {
        heard: heard.clone(),
        row: 0,
    }));
    (p, heard)
}

fn view_id(p: &Presenter<()>, test_id: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

fn near(a: crate::paint::FieldText, b: crate::paint::FieldText) -> bool {
    (a.rect.0 - b.rect.0).abs() < 0.001
        && (a.rect.1 - b.rect.1).abs() < 0.001
        && (a.baseline - b.baseline).abs() < 0.001
}

fn act(p: &mut Presenter<()>, action: &str) {
    let receipt = p.host.runner_mut().act(action, vec![]).unwrap();
    let at_ms = p.host.now();
    assert_eq!(
        p.host
            .commit(&[exact_runner::Timed { at_ms, receipt }], None),
        None
    );
    assert_eq!(p.after_commit(), None);
}

#[test]
fn a_steady_scroll_records_no_row_again() {
    let rows = "        box width=100 height=50 flex-shrink=0\n".repeat(16);
    let (mut p, heard) = kept(&format!(
        "component App\n  view\n    scroll testId=\"port\" width=100 height=100\n      column\n{rows}"
    ));
    let port = view_id(&p, "port");
    p.frame();
    let recorded = heard.borrow().begun.len();
    // Each row travels further than the viewport from where it was recorded.
    for step in 1..=60 {
        p.scroll_to(port, (0., step as f32 * 10.));
        p.frame();
    }
    assert_eq!(heard.borrow().begun.len(), recorded);
}

#[test]
fn distant_rows_with_hit_slop_keep_their_recordings_after_a_long_jump() {
    let rows =
        "        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n".repeat(120);
    let (mut p, heard) = kept(&format!(
        "component App\n  view\n    scroll testId=\"port\" width=100 height=100\n      column\n{rows}"
    ));
    let port = view_id(&p, "port");
    p.frame();
    let recorded = heard.borrow().begun.len();
    assert_eq!(recorded, 120);
    p.scroll_to(port, (0., 5000.));
    p.frame();
    // Only the rows whose drawing or slop can be hit at the new position.
    let again = heard.borrow().begun.len() - recorded;
    assert!(again > 0 && again < 10, "{again} rows recorded again");
}

#[test]
fn a_buttons_title_is_where_a_fresh_walk_puts_it_from_a_kept_row() {
    let source = r#"component App
  view
    scroll testId="port" width=100 height=100
      column
        box width=100 height=40 flex-shrink=0
        button "Go" testId="go" width=100 height=30 flex-shrink=0
        box width=100 height=400 flex-shrink=0
"#;
    let boot = || {
        let (p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(source),
            (),
            (100., 100.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        p
    };
    let mut fresh = boot();
    let (port, go) = (view_id(&fresh, "port"), view_id(&fresh, "go"));
    let title = |p: &mut Presenter<()>| p.field_texts(go).and_then(|t| t.label).expect("the title");
    fresh.frame();
    let at_rest = title(&mut fresh);
    fresh.scroll_to(port, (0., 0.25));
    let moved = title(&mut fresh);
    for local in [false, true] {
        let (mut p, heard) = keep(boot(), local);
        p.frame();
        assert!(near(title(&mut p), at_rest), "recorded, local {local}");
        heard.borrow_mut().geometry_without_drawing = true;
        let replayed = p.brush.geometry_rows_replayed();
        p.scroll_to(port, (0., 0.25));
        let got = title(&mut p);
        assert!(
            p.brush.geometry_rows_replayed() > replayed,
            "the row is moved"
        );
        assert!(
            near(got, moved),
            "moved, local {local}: {got:?} against {moved:?}"
        );
    }
}

#[test]
fn a_material_selects_value_moves_with_its_kept_row() {
    if !crate::presenter::control::material_tests::material_process() {
        return;
    }
    let (mut p, heard) = kept(
        r#"component App
  state pick = "a"
  action choose(v: string)
    pick = v
  view
    scroll testId="port" width=100 height=100
      column
        select value=pick change=choose testId="pick"
          option "Alpha" value="a"
          option "Beta" value="b"
        box width=100 height=400 flex-shrink=0
"#,
    );
    let (port, pick) = (view_id(&p, "port"), view_id(&p, "pick"));
    p.frame();
    let at_rest = p
        .field_texts(pick)
        .and_then(|t| t.value)
        .expect("the value");
    heard.borrow_mut().geometry_without_drawing = true;
    let replayed = p.brush.geometry_rows_replayed();
    p.scroll_to(port, (0., 0.25));
    let moved = p
        .field_texts(pick)
        .and_then(|t| t.value)
        .expect("the moved value");
    assert!(
        p.brush.geometry_rows_replayed() > replayed,
        "the row is moved"
    );
    assert!((moved.rect.1 - (at_rest.rect.1 - 0.25)).abs() < 0.001);
}

#[test]
fn a_row_whose_hit_slop_rounds_to_the_port_edge_is_walked_after_a_long_jump() {
    // Recorded where `f32` steps by 1/16, then by 4: the child's top rounds
    // to the row's, and its slop to the port's top or above it after the
    // jump, though a fresh walk's reaches 1/64 or 1/4 into the port.
    for (top, recorded, arrival) in [(0.015625, 1_000_000., 100.), (1.5, 40_000_000., 101.25)] {
        let (mut p, heard) = kept(&format!(
            r#"component App
  view
    scroll testId="port" width=100 height=100
      column
        box width=100 height=20 flex-shrink=0 overflow="visible"
          box testId="child" position="relative" top={top} width=100 height=20 overflow="visible" -exact-hit-slop-bottom=80
        box width=100 height={} flex-shrink=0 pointer-events="none"
"#,
            recorded + 100.
        ));
        let (port, child) = (view_id(&p, "port"), view_id(&p, "child"));
        assert_eq!(p.scroll_to(port, (0., recorded)), Some((0., recorded)));
        p.frame();
        heard.borrow_mut().geometry_without_drawing = true;
        assert_eq!(p.scroll_to(port, (0., arrival)), Some((0., arrival)));
        assert_eq!(p.hit(50., 0.), Some(child), "recorded at {recorded}");
    }
}

#[test]
fn a_long_jump_reads_hit_slop_that_env_changed() {
    let (mut p, heard) = kept(
        r#"component App
  view
    scroll testId="port" width=100 height=80
      column
        column height=20 flex-shrink=0
          box testId="target" width=100 height=20 flex-shrink=0 overflow="visible" -exact-hit-slop-bottom="env(safe-area-inset-bottom)"
        box width=100 height=1000100 flex-shrink=0 pointer-events="none"
        box width=100 height=20 flex-shrink=0
"#,
    );
    let (port, target) = (view_id(&p, "port"), view_id(&p, "target"));
    p.scroll_to(port, (0., 32_768.));
    p.frame();
    heard.borrow_mut().geometry_without_drawing = true;
    assert_eq!(p.set_safe_area([0., 0., 80., 0.]), None);
    p.scroll_to(port, (0., 80.9995));
    // The slop now ends at 19.0005; moved from 32,768 it would end at 19.
    assert_eq!(p.hit(5., 19.), Some(target));
}

#[test]
fn a_material_fields_label_and_value_are_where_a_fresh_walk_puts_them() {
    if !crate::presenter::control::material_tests::material_process() {
        return;
    }
    let source = r#"component App
  view
    scroll testId="port" width=100 height=100
      column
        box width=100 height=40 flex-shrink=0
        input testId="name" value="Ada" placeholder="Name" width=100 flex-shrink=0
        box width=100 height=400 flex-shrink=0
"#;
    let (mut fresh, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
        (),
        (100., 100.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    let name = view_id(&fresh, "name");
    fresh.frame();
    let want = fresh.field_texts(name).expect("its texts");
    for local in [false, true] {
        let (mut p, _) = kept_in(source, (100., 100.), local);
        p.frame();
        let got = p.field_texts(name).expect("its texts");
        assert!(
            near(got.label.unwrap(), want.label.unwrap()),
            "label, local {local}"
        );
        assert!(
            near(got.value.unwrap(), want.value.unwrap()),
            "value, local {local}"
        );
    }
}

#[test]
fn a_long_jump_walks_only_rows_its_port_shows() {
    let rows = "          box width=100 height=50 flex-shrink=0\n".repeat(120);
    let (mut p, heard) = kept_in(
        &format!(
            "component App\n  view\n    column\n      scroll testId=\"port\" width=100 height=100\n        column\n{rows}"
        ),
        (100., 1000.),
        false,
    );
    let port = view_id(&p, "port");
    p.frame();
    let recorded = heard.borrow().begun.len();
    p.scroll_to(port, (0., 5000.));
    p.frame();
    // Rows the window would show below the port are clipped by it.
    let again = heard.borrow().begun.len() - recorded;
    assert!(again > 0 && again < 6, "{again} rows recorded again");
}

#[test]
fn rows_a_layout_moves_whole_keep_their_hits_for_geometry_walks() {
    let (mut p, heard) = kept(
        r#"component App
  state tall = false
  action grow
    tall = true
  view
    scroll testId="port" width=100 height=100
      column
        box width=100 height=(tall ? 40 : 20) flex-shrink=0
        box width=100 height=30 flex-shrink=0
        box width=100 height=30 flex-shrink=0
        box width=100 height=30 flex-shrink=0
"#,
    );
    let port = view_id(&p, "port");
    p.frame();
    act(&mut p, "grow");
    p.frame();
    heard.borrow_mut().geometry_without_drawing = true;
    let replayed = p.brush.geometry_rows_replayed();
    p.scroll_to(port, (0., 0.25));
    p.boxes();
    // The three rows below moved whole: nothing in them moved within them.
    assert!(p.brush.geometry_rows_replayed() - replayed >= 3);
}

#[test]
fn rows_freed_at_a_geometry_walk_are_freed_by_the_backend_that_kept_them() {
    let rows = "        box width=100 height=50 flex-shrink=0\n".repeat(4);
    let source =
        format!("component App\n  view\n    scroll testId=\"port\" width=100 height=100\n      column\n{rows}");
    for whole_sections in [false, true] {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(&source),
            (),
            (100., 100.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        let heard = Rc::new(RefCell::new(Heard {
            viewport_row_coordinates: true,
            whole_sections,
            ..Heard::default()
        }));
        p.reader_replaced(Box::new(Keeps {
            heard: heard.clone(),
            row: 0,
        }));
        p.frame();
        let recorded: Vec<u32> = heard.borrow().begun.iter().map(|(id, _)| *id).collect();
        // Colours that resolve anew free every kept row at the next walk,
        // here one for a hit.
        p.brush.colors_changed();
        heard.borrow_mut().geometry_without_drawing = true;
        let port = view_id(&p, "port");
        p.scroll_to(port, (0., 1.));
        assert!(p.hit(5., 5.).is_some());
        heard.borrow_mut().geometry_without_drawing = false;
        p.frame();
        let freed = heard.borrow().freed.clone();
        assert!(
            recorded.iter().all(|id| freed.contains(id)),
            "whole sections {whole_sections}: {recorded:?} freed {freed:?}"
        );
    }
}
