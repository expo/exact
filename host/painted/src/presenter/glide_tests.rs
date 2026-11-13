//! A smooth element reveal glides while the display owns the clock, owns
//! its scroller until it lands (the end-follow waits), and ends once.
use super::super::{PainterChoice, Presenter};
use exact_kernel::ViewId;
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Event};
use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;

#[derive(Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

/// Rows 50 tall in a port 100 tall: the end is at `50 n - 100`.
const APP: &str = r#"component App
  state n = 10
  state following = true
  state ends = 0
  action up
    scrollIntoView("row-0", behavior="smooth")
  action down
    following = true
    scrollIntoView(`row-${n - 1}`, block="end", behavior="smooth")
  action jump
    scrollIntoView("row-0", behavior="instant")
  action away
    following = false
  action add
    n = n + 2
  action ended
    ends = ends + 1
  view
    column
      button "Up" press=up testId="up"
      button "Down" press=down testId="down"
      button "Jump" press=jump testId="jump"
      button "Away" press=away testId="away"
      button "Add" press=add testId="add"
      scroll testId="port" width=200 height=100 scrollFollowEnd=following scrollend=ended
        for i in range(n) key=i
          box id=`row-${i}` width=200 height=50 flex-shrink=0
"#;

struct Drive {
    p: Presenter<NoData>,
    wall: Rc<Cell<f64>>,
}

impl Drive {
    fn boot() -> Self {
        let (mut p, error) = Presenter::boot_with(
            &contract::compile(APP).unwrap().encode(),
            NoData,
            (400., 500.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert_eq!(error, None);
        let wall = Rc::new(Cell::new(0.0));
        let read = wall.clone();
        p.set_live_clock(Some(Rc::new(move || read.get())));
        let mut d = Drive { p, wall };
        d.frame(0.);
        d
    }
    fn id(&self, name: &str) -> ViewId {
        let k = self.p.host.kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    }
    fn top(&self) -> f32 {
        self.p.scroll_of(self.id("port")).1
    }
    fn number(&self, name: &str) -> f64 {
        match self.p.host.runner().slot(name).unwrap() {
            Value::Number(n) => *n,
            value => panic!("{name}: {value:?}"),
        }
    }
    fn press(&mut self, name: &str) {
        let (id, at) = (self.id(name), self.wall.get());
        assert_eq!(self.p.host.dispatch_at(id, Event::Press, at), None);
        assert_eq!(self.p.after_commit(), None);
        self.p.run_commands(NoData::default);
    }
    /// A display frame at `at` ms: the motion tick, then the timers.
    fn frame(&mut self, at: f64) {
        self.wall.set(at);
        self.p.tick(at);
        assert_eq!(self.p.advance(at), None);
    }
}

#[test]
fn a_smooth_reveal_glides_on_the_displays_clock_and_ends_once() {
    let mut d = Drive::boot();
    // Following its end, at it.
    assert_eq!(d.top(), 400.);
    d.press("up");
    assert_eq!(d.top(), 400., "it starts from where the port was");
    assert!(d.p.needs_animation_frame());
    d.frame(100.);
    let early = d.top();
    d.frame(150.);
    let middle = d.top();
    assert!(
        early < 400. && middle < early && middle > 0.,
        "{early} {middle}"
    );
    assert_eq!(d.number("ends"), 0.);
    d.frame(300.);
    assert_eq!(d.top(), 0.);
    d.frame(316.);
    assert_eq!(d.number("ends"), 1., "one end, where it rests");
    assert!(!d.p.needs_animation_frame());
    d.frame(500.);
    assert_eq!(d.number("ends"), 1.);
}

#[test]
fn the_end_follow_waits_for_a_glide_and_resumes_only_where_it_rests_at_the_end() {
    let mut d = Drive::boot();
    // Away from a followed end: rows arriving mid-glide are not followed.
    d.press("up");
    d.frame(100.);
    d.press("add");
    let at = d.top();
    assert!(at > 0. && at < 400., "{at}");
    d.frame(200.);
    assert!(d.top() < at, "the glide goes on up");
    d.frame(400.);
    assert_eq!(d.top(), 0.);
    d.press("add");
    assert_eq!(d.top(), 0., "it rests at the start: not following");

    // The app turns following on with a glide to the last row: no jump to
    // the end first, and once it rests there the end is followed.
    d.press("away");
    d.press("down");
    assert_eq!(d.top(), 0., "no jump ahead of the glide");
    d.frame(550.);
    assert!(d.top() > 0. && d.top() < 600., "{}", d.top());
    d.frame(800.);
    assert_eq!(d.top(), 600.);
    d.press("add");
    d.frame(816.);
    assert_eq!(d.top(), 700., "at the end, the end is followed");
}

#[test]
fn another_move_of_the_port_stops_a_glide_where_it_is() {
    let mut d = Drive::boot();
    d.press("up");
    d.frame(100.);
    let port = d.id("port");
    // The person's scroll (a key, a scrollbar) takes the port over.
    assert_eq!(d.p.scroll_to(port, (0., 123.)), Some((0., 123.)));
    d.frame(200.);
    assert_eq!(d.top(), 123.);
    assert!(!d.p.needs_animation_frame());
    // An instant request replaces a glide at once.
    d.press("up");
    d.frame(250.);
    d.press("jump");
    assert_eq!(d.top(), 0.);
    d.frame(600.);
    assert_eq!(d.top(), 0.);
}

#[test]
fn reduced_motion_and_a_clock_the_display_does_not_own_land_at_once() {
    let mut d = Drive::boot();
    d.p.set_live_clock(None);
    d.press("up");
    assert_eq!(d.top(), 0.);
    let mut d = Drive::boot();
    let mut preferences = d.p.host().runner().viewport().preferences;
    preferences.reduced_motion = true;
    d.p.set_preferences(preferences);
    d.press("up");
    assert_eq!(d.top(), 0.);
}

/// Fifty rows 100 tall in a port 100 tall, and a smooth `scrollIntoView`
/// of row 30: a virtualized list's smooth correction (LLP 1070.000 §11).
fn list(live: bool) -> (Presenter<NoData>, Rc<Cell<f64>>, ViewId) {
    let ids = (0..50)
        .map(|i| i.to_string())
        .collect::<Vec<_>>()
        .join(", ");
    let source = format!(
        r#"component App
  state ids = [{ids}]
  action go
    scrollIntoView("feed", 30, block="start", behavior="smooth")
  view
    column
      button "go" press=go testId="go"
      list id="feed" virtualized=true testId="feed" width=200 height=100 estimated-item-height=100
        for i in ids key=i
          box height=100
"#
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
    let wall = Rc::new(Cell::new(0.0));
    if live {
        let read = wall.clone();
        p.set_live_clock(Some(Rc::new(move || read.get())));
    }
    settle(&mut p);
    let k = p.host.kernel();
    let feed = k.node_by_key(k.find_by_test_id("feed")[0]).unwrap().id;
    (p, wall, feed)
}
fn settle(p: &mut Presenter<NoData>) {
    for _ in 0..16 {
        assert_eq!(p.pump(p.host.now()), None);
        if p.dirty() {
            let _ = p.frame();
        }
    }
}
fn request(p: &Presenter<NoData>) -> String {
    p.host.runner().into_view_json()
}

#[test]
fn a_lists_smooth_correction_glides_reports_its_destination_and_lands() {
    let (mut p, wall, feed) = list(true);
    let go = {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id("go")[0]).unwrap().id
    };
    p.tap(go).unwrap();
    p.run_commands(NoData::default);
    settle(&mut p);
    assert_eq!(p.scroll_of(feed).1, 0., "it starts from where the port was");
    assert!(p.gliding(feed));
    assert!(
        request(&p).contains(r#""status":"pending""#),
        "{}",
        request(&p)
    );
    wall.set(150.);
    p.tick(150.);
    settle(&mut p);
    let middle = p.scroll_of(feed).1;
    assert!(middle > 0. && middle < 3000., "{middle}");
    assert!(
        request(&p).contains(r#""status":"pending""#),
        "{}",
        request(&p)
    );
    wall.set(300.);
    p.tick(300.);
    settle(&mut p);
    assert_eq!(p.scroll_of(feed).1, 3000.);
    assert!(!p.gliding(feed));
    assert!(
        request(&p).contains(r#""status":"done""#),
        "{}",
        request(&p)
    );
}

#[test]
fn a_lists_smooth_correction_lands_at_once_on_a_clock_the_display_does_not_own() {
    let (mut p, _, feed) = list(false);
    let go = {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id("go")[0]).unwrap().id
    };
    p.tap(go).unwrap();
    p.run_commands(NoData::default);
    settle(&mut p);
    assert_eq!(p.scroll_of(feed).1, 3000.);
    assert!(
        request(&p).contains(r#""status":"done""#),
        "{}",
        request(&p)
    );
}

/// The reader's wheel during a list's glide ends it where the reader puts
/// the port: the report after says where that is, not where the glide was
/// headed, and no longer that the host animates, so the request ends there
/// and the window is built around the port.
#[test]
fn a_reader_interrupting_a_lists_glide_reports_where_the_port_is() {
    let (mut p, wall, feed) = list(true);
    let go = {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id("go")[0]).unwrap().id
    };
    p.tap(go).unwrap();
    p.run_commands(NoData::default);
    settle(&mut p);
    wall.set(150.);
    p.tick(150.);
    settle(&mut p);
    assert!(p.gliding(feed));
    p.wheel(feed, 0., -40.).unwrap();
    settle(&mut p);
    assert!(!p.gliding(feed), "the reader's move ends it");
    let at = p.scroll_of(feed).1;
    assert!(at > 0. && at < 3000., "{at}");
    let c = p.host.collection(feed).unwrap();
    let row = (at / 100.).floor() as usize;
    assert!(
        c.rows.iter().any(|r| r.index == row),
        "the window is built where the port is ({at}): {:?}",
        c.rows.iter().map(|r| r.index).collect::<Vec<_>>()
    );
    assert!(
        request(&p).contains(r#""status":"cancelled""#),
        "{}",
        request(&p)
    );
    wall.set(400.);
    p.tick(400.);
    settle(&mut p);
    assert_eq!(p.scroll_of(feed).1, at, "nothing moves it after");
}

/// A reveal inside an open modal box glides its scroller as anywhere else;
/// only the page under the box stays put.
#[test]
fn a_reveal_inside_an_open_modal_glides() {
    let source = r#"component App
  action reveal
    scrollIntoView("deep", behavior="smooth")
  view
    column width="100%" height="100%"
      button "Open" testId="open" commandfor="box" command="show-modal" height=30
      dialog id="box" testId="box" width=220 height=180 overflow-y="auto"
        button "Reveal" press=reveal testId="reveal" height=30 flex-shrink=0
        box height=1000 flex-shrink=0
        box id="deep" height=30 flex-shrink=0
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        NoData,
        (480., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let wall = Rc::new(Cell::new(0.0));
    let read = wall.clone();
    p.set_live_clock(Some(Rc::new(move || read.get())));
    p.serve_overlays();
    p.frame();
    let named = |p: &Presenter<NoData>, name: &str| {
        let k = p.host.kernel();
        k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
    };
    p.tap(named(&p, "open")).unwrap();
    p.run_commands(NoData::default);
    p.frame();
    assert!(p.alert_inert(), "the box is open and modal");
    let modal = named(&p, "box");
    p.tap(named(&p, "reveal")).unwrap();
    p.run_commands(NoData::default);
    assert_eq!(
        p.scroll_of(modal).1,
        0.,
        "no jump: it starts from where it was"
    );
    assert!(p.gliding(modal));
    wall.set(150.);
    p.tick(150.);
    let middle = p.scroll_of(modal).1;
    wall.set(300.);
    p.tick(300.);
    let end = p.scroll_of(modal).1;
    assert!(middle > 0. && middle < end && end > 800., "{middle} {end}");
    assert!(!p.gliding(modal));
}
