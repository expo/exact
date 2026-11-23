//! A `portal` mover's events bubble through its declaration, not the box it
//! is shown in, and a focused node in a mover that hides loses the focus
//! once (LLP 1013.002 D1, D5).
use super::*;
use exact_runner::{DataError, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

const APP: &str = r#"component App
  state at = "a"
  state log = ""
  action note(s: string)
    log = `${log}${s}`
  action heard(s: string, key: string)
    log = `${log}${s}`
  action hide
    at = "nowhere"
  view
    column width=400 height=400
      column pointerdown=note("A") press=note("Q") key=heard("k") testId="shown"
        box id="a" width=200 height=100
      column pointerdown=note("D") press=note("P") key=heard("K")
        column portal=at testId="mover" width=100 height=50
          button testId="inner" focus=note("+") blur=note("-") width=80 height=30
            text "inner"
      button press=hide testId="hide"
        text "Hide"
"#;

fn named(p: &Presenter<NoData>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn log(p: &Presenter<NoData>) -> String {
    (p.host.runner().slot("log").unwrap().as_str().unwrap()).to_owned()
}

fn presenter() -> Presenter<NoData> {
    let (p, error) = Presenter::boot_with(
        &contract::compile(APP).unwrap().encode(),
        NoData,
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    p
}

#[test]
fn a_moved_buttons_pointer_and_press_reach_its_declaration() {
    let mut p = presenter();
    let shown = named(&p, "shown");
    let mover = named(&p, "mover");
    assert!(
        p.is_inside(mover, shown),
        "the mover is shown in the first column"
    );
    p.tap(named(&p, "inner")).unwrap();
    assert_eq!(
        log(&p),
        "D+P",
        "the declaration's pointerdown and press, never the card's"
    );
}

#[test]
fn a_mouse_click_on_a_moved_button_presses_its_declaration() {
    let mut p = presenter();
    // A mouse's press resolves through kernel ancestry (`mouse_press_target`).
    let inner = named(&p, "inner");
    p.press_hit_after_down(inner, (0., 0.), 0., Some(None), true);
    assert!(p.after_commit().is_none());
    assert_eq!(log(&p), "P");
}

#[test]
fn a_key_at_a_moved_button_reaches_its_declaration() {
    let mut p = presenter();
    p.key_event_at(
        Some(named(&p, "inner")),
        EventKind::Key,
        "a",
        "KeyA",
        false,
        0.,
    );
    assert_eq!(log(&p), "K");
}

#[test]
fn a_focused_node_in_a_mover_that_hides_loses_the_focus_once() {
    let mut p = presenter();
    let inner = named(&p, "inner");
    assert!(p.set_focus(Some(inner), 0.).is_none());
    assert_eq!(log(&p), "+");
    let hide = named(&p, "hide");
    assert!(p.host.dispatch_at(hide, Event::Press, 0.).is_none());
    assert!(p.after_commit().is_none());
    assert_eq!(p.focus, None, "a hidden mover keeps no focus");
    assert_eq!(log(&p), "+-");
    assert!(!p.focusable(inner), "nor takes it again while hidden");
}
