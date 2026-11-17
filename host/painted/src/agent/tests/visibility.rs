//! Page facts under an agent drive do not suspend the process.
use super::*;

fn presenter(source: &str) -> Presenter<()> {
    let plan = contract::compile(source).unwrap();
    let (presenter, error) = Presenter::boot_with(
        &plan.encode(),
        (),
        (300.0, 300.0),
        1.0,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    presenter
}

fn json(presenter: &mut Presenter<()>, request: &str) -> serde_json::Value {
    serde_json::from_str(&handle(presenter, request)).unwrap()
}

#[test]
fn hidden_and_frozen_page_facts_leave_repeating_timers_running() {
    let mut p = presenter(
        r#"shape Page
  visibilityState: string
component App
  resource page = exactPage() as shape Page
  state ticks = 0
  state visibleRuns = 0
  action tick
    ticks = ticks + 1
  action ranVisible
    visibleRuns = visibleRuns + 1
  task beat mount
    every(100, tick)
  task whileVisible when page.visibilityState == "visible"
    after(1, ranVisible)
  view
    text `${ticks} ${visibleRuns}`
"#,
    );

    let hidden = json(
        &mut p,
        r#"{"op":"prefer","page":{"frozen":true,"visibility-state":"hidden"}}"#,
    );
    assert_eq!(hidden["page"]["frozen"], true);
    assert_eq!(hidden["page"]["visibility-state"], "hidden");
    let clock = json(&mut p, r#"{"op":"clock","to":350}"#);
    assert_eq!(clock["clock"], 350);
    let state = json(&mut p, r#"{"op":"state"}"#);
    assert_eq!(state["slots"]["ticks"], 3);
    assert_eq!(state["slots"]["visibleRuns"], 0);

    json(
        &mut p,
        r#"{"op":"prefer","page":{"visibility-state":"visible"}}"#,
    );
    json(&mut p, r#"{"op":"clock","to":400}"#);
    let state = json(&mut p, r#"{"op":"state"}"#);
    assert_eq!(state["slots"]["ticks"], 4);
    assert_eq!(state["slots"]["visibleRuns"], 1);
}

#[test]
fn a_timer_armed_by_hiding_runs_while_the_page_fact_is_frozen() {
    let mut p = presenter(
        r#"shape Page
  visibilityState: string
component App
  resource page = exactPage() as shape Page
  state wasHidden = false
  action markHidden
    wasHidden = true
  task hid when page.visibilityState == "hidden"
    after(1, markHidden)
  view
    text wasHidden.toString()
"#,
    );

    json(
        &mut p,
        r#"{"op":"prefer","page":{"frozen":true,"visibility-state":"hidden"}}"#,
    );
    json(&mut p, r#"{"op":"clock","to":1}"#);
    let state = json(&mut p, r#"{"op":"state"}"#);
    assert_eq!(state["slots"]["wasHidden"], true);
}
