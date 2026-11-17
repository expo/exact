use super::*;
use exact_runner::{DataError, Value};

#[derive(Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

/// A 320 by 220 port (300 by 200 and 10 of padding) zooming a 200 by 150
/// child with a 5 margin. The `scroll` handler mirrors the zoom back into
/// what `scrollZoom` reads, as the gallery's viewer does.
const APP: &str = r#"component App
  state zoom = 1
  state high = 4
  state left = 0
  state scrolls = 0
  state ends = 0
  state presses = 0
  state last = 0
  action zoomTo(value: number)
    zoom = value
  action detail
    zoom = 2
    left = 110
  action narrow
    high = 2
  action zoomed(x: number, y: number, e: ScrollEvent)
    scrolls = scrolls + 1
    last = e.scrollZoom
    zoom = e.scrollZoom
    left = x
  action ended
    ends = ends + 1
  action pressed
    presses = presses + 1
  view
    column
      button "Two" press=zoomTo(2) testId="two"
      button "Detail" press=detail testId="detail"
      button "Narrow" press=narrow testId="narrow"
      button "Four" press=zoomTo(4) testId="four"
      scroll testId="port" width=300 height=200 padding=10 -exact-max-zoom=high scrollZoom=zoom scrollLeft=left scroll=zoomed scrollend=ended
        box testId="child" width=200 height=150 margin=5 press=pressed
"#;

fn boot() -> Presenter<NoData> {
    let (p, error) = Presenter::boot_with(
        &contract::compile(APP).unwrap().encode(),
        NoData,
        (400., 500.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    p
}

fn named(p: &Presenter<NoData>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn number(p: &Presenter<NoData>, name: &str) -> f64 {
    match p.host.runner().slot(name).unwrap() {
        Value::Number(n) => *n,
        value => panic!("{name}: {value:?}"),
    }
}

fn press(p: &mut Presenter<NoData>, name: &str) {
    let id = named(p, name);
    assert_eq!(p.host.dispatch_at(id, Event::Press, p.host.now()), None);
    assert_eq!(p.after_commit(), None);
    settle(p);
}

/// Frames presented and what they owe delivered: a write's `scroll`
/// reports once its offset is shown.
fn settle(p: &mut Presenter<NoData>) {
    for _ in 0..8 {
        assert_eq!(p.pump(p.host.now()), None);
        if p.dirty() {
            let _ = p.frame();
        }
    }
}

fn zoom(p: &Presenter<NoData>) -> f64 {
    p.reported_zoom(named(p, "port"))
}

/// The child's drawn box.
fn child(p: &mut Presenter<NoData>) -> Rect4 {
    let id = named(p, "child");
    p.rect_of(id).unwrap()
}

fn port(p: &mut Presenter<NoData>) -> Rect4 {
    let id = named(p, "port");
    p.rect_of(id).unwrap()
}

fn close(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() < 0.01 && (a.1 - b.1).abs() < 0.01
}

#[test]
fn the_child_is_centred_by_half_the_slack_and_scaled_with_its_extent() {
    let mut p = boot();
    let (port, c) = (port(&mut p), child(&mut p));
    // At 1 the content is 230 by 180 of a 320 by 220 port: centred.
    assert!(close((c.0 - port.0, c.1 - port.1), (15. + 45., 15. + 20.)));
    assert!(close((c.2, c.3), (200., 150.)));
    press(&mut p, "two");
    let c = child(&mut p);
    assert_eq!(zoom(&p), 2.);
    assert!(close((c.2, c.3), (400., 300.)));
    // The extent is the padding and the margins around the scaled child,
    // unscaled: 430 by 330.
    let id = named(&p, "port");
    let travel = p.scroll_range(id, &BTreeMap::new());
    assert!(close(travel, (110., 110.)));
}

#[test]
fn a_write_zooms_about_the_ports_centre() {
    let mut p = boot();
    let id = named(&p, "port");
    let port = port(&mut p);
    let centre = (port.0 + 160., port.1 + 110.);
    // The child's centre is under the port's centre at 1, and stays there.
    let c = child(&mut p);
    assert!(close((c.0 + c.2 / 2., c.1 + c.3 / 2.), centre));
    press(&mut p, "two");
    assert!(close(p.scroll_of(id), (55., 55.)));
    let c = child(&mut p);
    assert!(close((c.0 + c.2 / 2., c.1 + c.3 / 2.), centre));
    assert_eq!(number(&p, "last"), 2.);
}

#[test]
fn control_wheel_zooms_about_the_pointer_and_the_mirror_moves_nothing() {
    let mut p = boot();
    let id = named(&p, "port");
    let c = child(&mut p);
    // A point a quarter of the way into the child.
    let at = (c.0 + 50., c.1 + 37.5);
    p.hold_modifier("ControlLeft", true);
    p.wheel_at(at.0, at.1, 0., -100. * 2f32.ln());
    p.hold_modifier("ControlLeft", false);
    settle(&mut p);
    assert!((zoom(&p) - 2.).abs() < 1e-4);
    let c = child(&mut p);
    assert!(close((c.0 + 100., c.1 + 75.), at));
    // The handler wrote the zoom back: an echo, which moves nothing.
    assert!((number(&p, "zoom") - zoom(&p)).abs() < 1e-6);
    let offset = p.scroll_of(id);
    settle(&mut p);
    assert_eq!(p.scroll_of(id), offset);
    // Without `ctrl` the wheel scrolls, as before.
    p.wheel_at(at.0, at.1, 0., 20.);
    assert!(close(p.scroll_of(id), (offset.0, offset.1 + 20.)));
    assert!((zoom(&p) - 2.).abs() < 1e-4);
}

#[test]
fn a_pinch_holds_a_write_and_ends_in_one_scrollend() {
    let mut p = boot();
    let id = named(&p, "port");
    let port = port(&mut p);
    let centre = (port.0 + 160., port.1 + 110.);
    assert_eq!(p.clock(10.), (10., None));
    p.zoom_pinch_began(id);
    p.zoom_pinched(id, 1.25, centre, centre);
    p.zoom_pinched(id, 1.2, centre, (centre.0, centre.1 - 10.));
    settle(&mut p);
    assert!((zoom(&p) - 1.5).abs() < 1e-4);
    assert!(number(&p, "scrolls") >= 2.);
    // The pinch moved the content with the fingers: 10 up.
    let c = child(&mut p);
    assert!(close((c.0 + c.2 / 2., c.1 + c.3 / 2. + 10.), centre));
    // A write during the pinch waits for its end.
    press(&mut p, "two");
    assert!((zoom(&p) - 1.5).abs() < 1e-4);
    assert_eq!(p.clock(500.), (500., None));
    assert_eq!(number(&p, "ends"), 0.);
    p.zoom_pinch_ended(id);
    assert_eq!(p.after_commit(), None);
    assert_eq!(zoom(&p), 2.);
    assert_eq!(p.clock(1000.), (1000., None));
    assert_eq!(number(&p, "ends"), 1.);
}

#[test]
fn a_write_after_the_gesture_rests_is_not_an_echo() {
    let mut p = boot();
    let id = named(&p, "port");
    let port = port(&mut p);
    let centre = (port.0 + 160., port.1 + 110.);
    p.zoom_pinch_began(id);
    p.zoom_pinched(id, 2., centre, centre);
    p.zoom_pinched(id, 0.75, centre, centre);
    p.zoom_pinch_ended(id);
    settle(&mut p);
    assert!((zoom(&p) - 1.5).abs() < 1e-4);
    assert!((number(&p, "zoom") - 1.5).abs() < 1e-4);
    // 2 was reported during the pinch, but the pinch has rested.
    press(&mut p, "two");
    assert_eq!(zoom(&p), 2.);
}

#[test]
fn zoom_and_offset_written_together_are_one_move() {
    let mut p = boot();
    let id = named(&p, "port");
    press(&mut p, "detail");
    assert_eq!(zoom(&p), 2.);
    // The offset lands in the new zoom's coordinates: the far left edge.
    assert_eq!(p.scroll_of(id).0, 110.);
}

#[test]
fn a_narrowed_range_clamps_the_zoom() {
    let mut p = boot();
    press(&mut p, "four");
    assert_eq!(zoom(&p), 4.);
    press(&mut p, "narrow");
    assert_eq!(zoom(&p), 2.);
    assert_eq!(number(&p, "zoom"), 2.);
}

#[test]
fn a_touch_double_tap_zooms_and_a_single_tap_presses_late() {
    let mut p = boot();
    p.set_pointer_kind(PointerKind::Touch);
    let c = child(&mut p);
    let at = (c.0 + 120., c.1 + 90.);
    let tap = |p: &mut Presenter<NoData>, t: f64| {
        assert_eq!(p.clock(t), (t, None));
        p.pointer_down(at.0, at.1, t).unwrap();
        p.pointer_up(at.0, at.1, t + 50.).unwrap();
        settle(p);
    };
    // One tap presses once no second can come.
    tap(&mut p, 100.);
    assert_eq!(number(&p, "presses"), 0.);
    assert_eq!(p.clock(500.), (500., None));
    assert_eq!(number(&p, "presses"), 1.);
    // Two zoom about the tap, and press nothing.
    tap(&mut p, 1000.);
    tap(&mut p, 1200.);
    assert_eq!(zoom(&p), 2.);
    let zoomed = child(&mut p);
    assert!(close((zoomed.0 + 240., zoomed.1 + 180.), at));
    assert_eq!(p.clock(2000.), (2000., None));
    assert_eq!(number(&p, "presses"), 1.);
    // Two more return to the minimum.
    tap(&mut p, 3000.);
    tap(&mut p, 3200.);
    assert_eq!(zoom(&p), 1.);
    assert_eq!(p.clock(4000.), (4000., None));
    assert_eq!(number(&p, "presses"), 1.);
}

#[test]
fn a_mouse_press_never_waits() {
    let mut p = boot();
    let c = child(&mut p);
    assert_eq!(p.clock(100.), (100., None));
    p.pointer_down(c.0 + 20., c.1 + 20., 100.).unwrap();
    p.pointer_up(c.0 + 20., c.1 + 20., 150.).unwrap();
    assert_eq!(number(&p, "presses"), 1.);
}

#[test]
fn the_zoomed_child_is_hit_where_it_is_drawn() {
    let mut p = boot();
    let child_id = named(&p, "child");
    let port_id = named(&p, "port");
    let c = child(&mut p);
    // Left of the child at 1: the port.
    let beside = (c.0 - 20., c.1 + 20.);
    assert_eq!(p.hit(beside.0, beside.1), Some(port_id));
    press(&mut p, "two");
    assert_eq!(p.hit(beside.0, beside.1), Some(child_id));
}

#[test]
fn a_smooth_write_animates_on_the_displays_clock_and_ends_once() {
    let source = APP.replace(
        "scrollend=ended",
        "scrollend=ended scroll-behavior=\"smooth\"",
    );
    let (mut p, error) = Presenter::boot_with(
        &contract::compile(&source).unwrap().encode(),
        NoData,
        (400., 500.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    let wall = std::rc::Rc::new(std::cell::Cell::new(0.0));
    let read = wall.clone();
    p.set_live_clock(Some(std::rc::Rc::new(move || read.get())));
    let frame = |p: &mut Presenter<NoData>, at: f64| {
        wall.set(at);
        p.tick(at);
        assert_eq!(p.advance(at), None);
        if p.dirty() {
            let _ = p.frame();
        }
    };
    frame(&mut p, 0.);
    let id = named(&p, "two");
    assert_eq!(p.host.dispatch_at(id, Event::Press, 0.), None);
    assert_eq!(p.after_commit(), None);
    assert_eq!(zoom(&p), 1.);
    assert!(p.needs_animation_frame());
    frame(&mut p, 150.);
    let middle = zoom(&p);
    assert!(middle > 1. && middle < 2., "{middle}");
    assert_eq!(number(&p, "ends"), 0.);
    frame(&mut p, 300.);
    assert_eq!(zoom(&p), 2.);
    frame(&mut p, 316.);
    frame(&mut p, 500.);
    assert_eq!(number(&p, "ends"), 1.);
    assert!(!p.needs_animation_frame());
    // The mirrored zooms the animation reported moved nothing back.
    assert_eq!(number(&p, "zoom"), 2.);
}

#[test]
fn a_touch_on_a_zoomable_scroller_with_no_travel_names_it_to_the_reader() {
    let mut p = boot();
    let id = named(&p, "port");
    let c = child(&mut p);
    let words = p.reader_under(c.0 + 20., c.1 + 20., 1.);
    // One scroller under the touch: the port, on neither axis, zooming and
    // zoomed by a double tap there.
    assert_eq!(words[1], 1);
    assert_eq!(words[3], id as i32);
    assert_eq!(words[4] & (1 | 2 | 256 | 512), 256 | 512);
}
