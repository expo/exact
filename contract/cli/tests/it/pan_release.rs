//! @ref LLP 1057 §10.6 phase 2 — `panrelease` completes `pan` with its
//! release velocity: two numbers appended, pan's units per second.
use crate::source::{boxed, Source};
use exact_plan::{EventKind, Value};
use exact_runner::{DataError, DataSource, Event, Runner};
struct Empty;
impl DataSource for Empty {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

const APP: &str = r#"component App
  state x = 0
  state vx = 0
  state vy = 0
  state released = 0
  action move(dx: number, dy: number)
    x = x + dx
  action release(scale: number, sx, sy)
    vx = sx * scale
    vy = sy * scale
    released = released + 1
  view
    box testId="card" pan=move panrelease=release(2) translate=`${x}px 0px`
"#;

fn boot() -> (Runner<Source>, exact_kernel::ViewId) {
    let plan = contract::compile(APP).unwrap();
    let runner = Runner::boot(
        plan,
        boxed(Empty),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let id = runner.kernel().find_by_test_id("card")[0];
    let id = runner.kernel().node_by_key(id).unwrap().id;
    (runner, id)
}

#[test]
fn panrelease_appends_the_release_velocity_after_curried_arguments() {
    let (mut runner, id) = boot();
    assert_eq!(
        runner.handlers_of(id),
        vec![EventKind::Pan, EventKind::Panrelease]
    );
    runner.dispatch(id, Event::Pan(30., 4.)).unwrap();
    runner
        .dispatch(id, Event::pan_release_payload("1200.5,-80").unwrap())
        .unwrap();
    let state = exact_runner::agent::state(&runner);
    assert!(state.contains("\"vx\":2401"), "{state}");
    assert!(state.contains("\"vy\":-160"), "{state}");
    assert!(state.contains("\"released\":1"), "{state}");
    // A cancelled contact releases at rest: the same event, zero velocity.
    runner.dispatch(id, Event::PanRelease(0., 0.)).unwrap();
    let state = exact_runner::agent::state(&runner);
    assert!(
        state.contains("\"vx\":0") && state.contains("\"released\":2"),
        "{state}"
    );
}

#[test]
fn panrelease_refuses_nonfinite_velocity_before_state_changes() {
    let (mut runner, id) = boot();
    let before = exact_runner::agent::state(&runner);
    for (vx, vy) in [(f64::NAN, 0.), (0., f64::INFINITY)] {
        assert!(runner.dispatch(id, Event::PanRelease(vx, vy)).is_err());
    }
    assert_eq!(exact_runner::agent::state(&runner), before);
    for payload in ["NaN,0", "0,inf", "1", "1,2,3", ""] {
        assert!(Event::pan_release_payload(payload).is_none(), "{payload}");
    }
    // A velocity is not a layout length: past f32's range is still finite.
    assert!(Event::pan_release_payload("1e39,0").is_some());
}

#[test]
fn panrelease_takes_two_numeric_payload_parameters() {
    for params in [
        "vx: number",
        "vx: string, vy: number",
        "a: number, b: number, c: number",
    ] {
        let src = format!(
            "component App\n  state x = 0\n  action done({params})\n    x = 1\n  view\n    box panrelease=done\n"
        );
        assert!(contract::compile(&src).is_err(), "{params}");
    }
    let untyped = "component App\n  state x = 0\n  action done(vx, vy)\n    x = vx + vy\n  view\n    box panrelease=done\n";
    contract::compile(untyped).unwrap();
}

#[test]
fn pinch_and_pinchrelease_append_scale_and_viewport_point_after_curried_arguments() {
    let source = r#"component App
  state result = 0
  action onPinch(base: number, scale, x, y)
    result = base + scale * 100 + x + y
  action onRelease(base: number, scale, x, y)
    result = base + scale * 1000 + x + y
  view
    scroll testId="target" height=100 pinch=onPinch(10) pinchrelease=onRelease(20)
"#;
    let plan = contract::compile(source).unwrap();
    let mut runner = Runner::boot(
        plan,
        boxed(Empty),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = runner.kernel().find_by_test_id("target")[0];
    let id = runner.kernel().node_by_key(key).unwrap().id;
    assert_eq!(
        runner.handlers_of(id),
        vec![EventKind::Pinch, EventKind::Pinchrelease]
    );
    for (kind, want) in [(67, 224), (68, 2034)] {
        runner
            .dispatch(id, Event::of_host_kind(kind, "2,5,9").unwrap())
            .unwrap();
        let state = exact_runner::agent::state(&runner);
        assert!(state.contains(&format!("\"result\":{want}")), "{state}");
        for bad in [
            "0,5,9", "-1,5,9", "inf,5,9", "NaN,5,9", "1,NaN,9", "2,3", "2,3,4,5",
        ] {
            assert!(Event::of_host_kind(kind, bad).is_err(), "{kind} {bad}");
        }
    }
    for binding in ["pinch", "pinchrelease"] {
        for bad in ["s, x", "s, x, y, z", "s: string, x: number, y: number"] {
            let src = format!(
                "component App\n  state heard = false\n  action report({bad})\n    heard = true\n  view\n    scroll height=100 {binding}=report\n"
            );
            assert!(contract::compile(&src).is_err(), "{binding} {bad}");
        }
    }
}
