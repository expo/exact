//! Kept rows moved rather than walked: their recordings through a scroll,
//! and their hits and titles in a walk that paints nothing.
use super::*;

/// A presenter over `source` at 100 by 100 points, its rows kept by [`Keeps`].
fn kept(source: &str) -> (Presenter<()>, Rc<RefCell<Heard>>) {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(source),
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
    let (mut p, heard) = kept(&format!(
        "component App\n  view\n    scroll testId=\"port\" width=100 height=100\n      column\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n        box width=100 height=50 flex-shrink=0 -exact-hit-slop-bottom=8\n"
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
fn a_buttons_title_moves_with_its_kept_row_in_a_geometry_walk() {
    let (mut p, heard) = kept(
        r#"component App
  view
    scroll testId="port" width=100 height=100
      column
        button "Go" testId="go" width=100 height=30 flex-shrink=0
        box width=100 height=400 flex-shrink=0
"#,
    );
    let (port, go) = (view_id(&p, "port"), view_id(&p, "go"));
    p.frame();
    let at_rest = p.field_texts(go).and_then(|t| t.label).expect("the title");
    heard.borrow_mut().geometry_without_drawing = true;
    let replayed = p.brush.geometry_rows_replayed();
    p.scroll_to(port, (0., 0.25));
    let moved = p
        .field_texts(go)
        .and_then(|t| t.label)
        .expect("the moved title");
    assert!(
        p.brush.geometry_rows_replayed() > replayed,
        "the row is moved"
    );
    assert!((moved.rect.1 - (at_rest.rect.1 - 0.25)).abs() < 0.001);
    assert!((moved.baseline - (at_rest.baseline - 0.25)).abs() < 0.001);
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
    let receipt = p.host.runner_mut().act("grow", vec![]).unwrap();
    let at_ms = p.host.now();
    assert_eq!(
        p.host
            .commit(&[exact_runner::Timed { at_ms, receipt }], None),
        None
    );
    assert_eq!(p.after_commit(), None);
    p.frame();
    heard.borrow_mut().geometry_without_drawing = true;
    let replayed = p.brush.geometry_rows_replayed();
    p.scroll_to(port, (0., 0.25));
    p.boxes();
    // The three rows below moved whole: nothing in them moved within them.
    assert!(p.brush.geometry_rows_replayed() - replayed >= 3);
}
