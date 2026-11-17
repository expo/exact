//! A drawn slider or spin button stepped by its author's keys, and a native
//! range whose author's keys may take its step.
use super::*;

/// A drawn slider (horizontal and vertical) and spin button, each stepped
/// by its own `key` handler, which also hears its `keyup`.
const DRAWN_RANGES: &str = r#"component App
  state level = 50
  state floor = 3
  state tilt = 0
  state heard = ""
  action stepLevel(k: string)
    heard = `${heard} ${k}`
    if k == "ArrowRight"
      level = level + 10
    if k == "ArrowLeft"
      level = level - 10
  action stepFloor(k: string)
    heard = `${heard} ${k}`
    if k == "ArrowUp"
      floor = floor + 1
    if k == "ArrowDown"
      floor = floor - 1
  action stepTilt(k: string)
    heard = `${heard} ${k}`
    if k == "ArrowUp"
      tilt = tilt + 1
  action up(k: string)
    heard = `${heard} up:${k}`
  view
    column width=320 height=400 align-items="flex-start"
      box role="slider" tabindex=0 aria-label="Level" aria-valuenow=level key=stepLevel keyup=up width=200 height=30
      box role="spinbutton" tabindex=0 aria-label="Floor" aria-valuenow=floor aria-valuemin=1 key=stepFloor width=100 height=30
      box role="slider" aria-orientation="vertical" tabindex=0 aria-label="Tilt" aria-valuenow=tilt aria-valuemin=-5 aria-valuemax=5 key=stepTilt width=30 height=100
      text heard testId="heard" width=300 height=20
"#;

/// A service steps a drawn slider or spin button as WebKit steps an ARIA
/// one: the author's key handler hears ArrowRight or ArrowLeft for a
/// horizontal slider, ArrowUp or ArrowDown for a vertical one and a spin
/// button, then its keyup; and the step is offered only short of a bound.
/// None is set to a value: only its author's keys move it.
#[test]
fn a_drawn_range_steps_by_its_authors_keys() {
    let mut p = boot(DRAWN_RANGES, (320., 400.), 0);
    let nodes = statement(&mut p).nodes;
    let flags = |nodes: &[Node], name: &str| named(nodes, name).flags;
    let both = flag::RANGE | flag::FORWARD | flag::BACKWARD;
    assert_eq!(flags(&nodes, "Level"), both);
    assert_eq!(
        flags(&nodes, "Floor"),
        both,
        "a spin button with no maximum goes up"
    );
    assert_eq!(flags(&nodes, "Tilt"), both);
    let roles = ["Level", "Floor", "Tilt"].map(|name| named(&nodes, name).role);
    assert_eq!(roles, [role::SLIDER, role::SPIN_BUTTON, role::SLIDER]);
    let of = |name: &str| named(&nodes, name).id;
    let heard = |p: &Presenter<Data>| {
        let kernel = p.host().kernel();
        let text = kernel.node(id(p, "heard")).unwrap().props.str(PropId::Text);
        text.unwrap_or_default().trim().to_string()
    };
    assert!(p.access_act(of("Level"), act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(heard(&p), "ArrowRight up:ArrowRight");
    assert_eq!(named(&statement(&mut p).nodes, "Level").range[2], 60.0);
    assert!(p.access_act(of("Level"), act::BACKWARD, 0.0, 0.0));
    assert!(p.access_act(of("Floor"), act::BACKWARD, 0.0, 0.0));
    assert!(p.access_act(of("Tilt"), act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(
        heard(&p),
        "ArrowRight up:ArrowRight ArrowLeft up:ArrowLeft ArrowDown ArrowUp"
    );
    let nodes = statement(&mut p).nodes;
    assert_eq!(named(&nodes, "Level").range[2], 50.0);
    assert_eq!(named(&nodes, "Floor").range[2], 2.0);
    assert_eq!(named(&nodes, "Tilt").range[2], 1.0);
    assert!(!p.access_act(named(&nodes, "Level").id, act::SET, 20.0, 0.0));
    assert!(p.access_act(named(&nodes, "Floor").id, act::BACKWARD, 0.0, 0.0));
    settle(&mut p);
    let nodes = statement(&mut p).nodes;
    assert_eq!(
        flags(&nodes, "Floor"),
        flag::RANGE | flag::FORWARD,
        "at its least"
    );
    assert!(!p.access_act(named(&nodes, "Floor").id, act::BACKWARD, 0.0, 0.0));
}

/// Two horizontal sliders, one of them `direction: ltr` and the other
/// `rtl`, under a document of `direction`, each saying the keys it hears.
fn sliders_in(direction: Direction) -> Presenter<Data> {
    const SLIDERS: &str = r#"component App
  state heard = ""
  action hear(k: string)
    heard = `${heard} ${k}`
  view
    column width=320 height=400 align-items="flex-start"
      box role="slider" tabindex=0 aria-label="Plain" aria-valuenow=50 key=hear width=200 height=30
      box role="slider" tabindex=0 aria-label="Ltr" aria-valuenow=50 key=hear direction="ltr" width=200 height=30
      box role="slider" tabindex=0 aria-label="Rtl" aria-valuenow=50 key=hear direction="rtl" width=200 height=30
      text heard testId="heard" width=300 height=20
"#;
    let launch = exact_runner::time::Launch {
        direction: Some(direction),
        ..exact_runner::time::Launch::NONE
    };
    exact_runner::time::launching_with(launch, || boot(SLIDERS, (320., 400.), 0))
}

/// A service's increment on a horizontal slider is ArrowRight where the
/// slider's own computed `direction` is `ltr` and ArrowLeft where it is
/// `rtl`, whatever the document's: the slider inherits the document's
/// unless its author says otherwise.
#[test]
fn a_drawn_sliders_increment_follows_its_own_direction() {
    for (document, plain) in [
        (Direction::Rtl, "ArrowLeft"),
        (Direction::Ltr, "ArrowRight"),
    ] {
        let mut p = sliders_in(document);
        assert_eq!(p.host().runner().direction(), document.name());
        let nodes = statement(&mut p).nodes;
        for name in ["Plain", "Ltr", "Rtl"] {
            assert!(p.access_act(named(&nodes, name).id, act::FORWARD, 0.0, 0.0));
        }
        settle(&mut p);
        let kernel = p.host().kernel();
        let heard = kernel
            .node(id(&p, "heard"))
            .unwrap()
            .props
            .str(PropId::Text);
        let heard = heard.unwrap_or_default().trim().to_string();
        assert_eq!(
            heard,
            format!("{plain} ArrowRight ArrowLeft"),
            "{document:?}"
        );
    }
}

/// A service's step of a native range is first its arrow key (LLP 1069.001,
/// amended 2026-11-16): a `key` handler that calls `preventDefault()` makes
/// the move, here 10 on a range whose drag is free (`step="any"`); one that
/// does not leaves the range its own step, as a range with no handler has
/// it. Setting a value sends no key.
#[test]
fn a_native_ranges_step_is_its_arrow_key_first() {
    const RANGES: &str = r#"component App
  state at = 40
  state plain = 40
  state heard = ""
  action seek(k: string)
    heard = `${heard} ${k}`
    if k == "ArrowRight"
      preventDefault()
      at = min(at + 10, 240)
    if k == "ArrowLeft"
      preventDefault()
      at = max(at - 10, 0)
  action up(k: string)
    heard = `${heard} up:${k}`
  action hear(k: string)
    heard = `${heard} ${k}`
  action setAt(v: number)
    at = v
  action setPlain(v: number)
    plain = v
  view
    column width=320 height=400 align-items="flex-start"
      input type="range" min=0 max=240 step="any" value=at key=seek keyup=up change=setAt aria-label="Seek"
      input type="range" min=0 max=240 step=5 value=plain key=hear change=setPlain aria-label="Plain"
      text heard testId="heard" width=300 height=20
"#;
    let mut p = boot(RANGES, (320., 400.), 0);
    let nodes = statement(&mut p).nodes;
    let (seek, plain) = (named(&nodes, "Seek").id, named(&nodes, "Plain").id);
    // The value each range shows once the step has landed.
    let shown = |p: &mut Presenter<Data>| {
        settle(p);
        let nodes = statement(p).nodes;
        (
            named(&nodes, "Seek").range[2],
            named(&nodes, "Plain").range[2],
        )
    };
    assert!(p.access_act(seek, act::FORWARD, 0.0, 0.0));
    assert_eq!(shown(&mut p).0, 50.0, "the handler's 10, not a twentieth");
    assert!(p.access_act(seek, act::BACKWARD, 0.0, 0.0));
    assert!(p.access_act(seek, act::BACKWARD, 0.0, 0.0));
    assert_eq!(shown(&mut p).0, 30.0);
    assert!(p.access_act(seek, act::SET, 123.0, 0.0));
    assert_eq!(shown(&mut p).0, 123.0, "a set lands where it is put");
    assert!(p.access_act(plain, act::FORWARD, 0.0, 0.0));
    assert_eq!(shown(&mut p).1, 45.0, "its own step: nothing prevented it");
    let kernel = p.host().kernel();
    let heard = kernel
        .node(id(&p, "heard"))
        .unwrap()
        .props
        .str(PropId::Text);
    assert_eq!(
        heard.unwrap_or_default().trim(),
        "ArrowRight up:ArrowRight ArrowLeft up:ArrowLeft ArrowLeft up:ArrowLeft ArrowRight"
    );
}

/// The text of the `heard` node: what the handlers said, in order.
fn heard(p: &Presenter<Data>) -> String {
    let kernel = p.host().kernel();
    let text = kernel.node(id(p, "heard")).unwrap().props.str(PropId::Text);
    text.unwrap_or_default().trim().to_string()
}

/// A step nothing prevented is the key's default action: it lands between
/// the keydown and the keyup, as a keyboard's arrow does, and starts from
/// the value and the bounds the keydown's handler left.
#[test]
fn an_unprevented_step_follows_the_keydown_and_what_it_changed() {
    const RANGE: &str = r#"component App
  state at = 40
  state top = 50
  state heard = ""
  action jump(k: string)
    heard = `${heard} ${k}`
    at = 60
    top = 100
  action up(k: string)
    heard = `${heard} up:${k}`
  action moved(v: number)
    heard = `${heard} input:${v}`
  action setAt(v: number)
    heard = `${heard} change:${v}`
    at = v
  view
    column width=320 height=400 align-items="flex-start"
      input type="range" min=0 max=top step=5 value=at key=jump keyup=up input=moved change=setAt aria-label="Jump"
      text heard testId="heard" width=300 height=20
"#;
    let mut p = boot(RANGE, (320., 400.), 0);
    let jump = named(&statement(&mut p).nodes, "Jump").id;
    assert!(p.access_act(jump, act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(
        heard(&p),
        "ArrowRight input:65 change:65 up:ArrowRight",
        "one step past the handler's 60, under its new maximum, before the keyup"
    );
    assert_eq!(named(&statement(&mut p).nodes, "Jump").range[2], 65.0);
}

/// A disabled range takes no step and sends no key, as a keyboard's arrow
/// reaches neither it nor what holds it; and one its keydown's handler
/// disables or makes inert does not step after it.
#[test]
fn a_disabled_native_range_takes_no_step_and_no_key() {
    const RANGES: &str = r#"component App
  state at = 40
  state late = 40
  state shut = false
  state still = 40
  state gone = false
  state heard = ""
  action seek(k: string)
    heard = `${heard} ${k}`
    preventDefault()
    at = at + 10
  action shutIt(k: string)
    heard = `${heard} ${k}`
    shut = true
  action goIt(k: string)
    heard = `${heard} inert:${k}`
    gone = true
  action outer(k: string)
    heard = `${heard} outer:${k}`
    preventDefault()
  action setAt(v: number)
    at = v
  action setLate(v: number)
    late = v
  action setStill(v: number)
    still = v
  view
    column width=320 height=400 align-items="flex-start"
      column key=outer
        input type="range" min=0 max=240 step=5 value=at disabled=true key=seek change=setAt aria-label="Off"
      input type="range" min=0 max=240 step=5 value=late disabled=shut key=shutIt change=setLate aria-label="Late"
      input type="range" min=0 max=240 step=5 value=still inert=gone key=goIt change=setStill aria-label="Still"
      text heard testId="heard" width=300 height=20
"#;
    let mut p = boot(RANGES, (320., 400.), 0);
    let nodes = statement(&mut p).nodes;
    let (off, late, still) = (
        named(&nodes, "Off"),
        named(&nodes, "Late"),
        named(&nodes, "Still"),
    );
    for action in [act::FORWARD, act::BACKWARD, act::SET] {
        assert!(!p.access_act(off.id, action, 100.0, 0.0), "{action}");
    }
    settle(&mut p);
    assert_eq!(heard(&p), "", "no key at it or above it");
    assert_eq!(p.range_value(off.view), Some(40.0));
    assert!(!p.access_act(late.id, act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(heard(&p), "ArrowRight");
    assert_eq!(p.range_value(late.view), Some(40.0));
    assert!(!p.access_act(still.id, act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(heard(&p), "ArrowRight inert:ArrowRight");
    assert_eq!(p.range_value(still.view), Some(40.0));
}

/// A step answers whether the range moved, as a platform slider's own does:
/// no at the end of the range, though a handler took the key, and yes where
/// the handler's move left the host's step nowhere to go.
#[test]
fn a_native_ranges_step_answers_whether_it_moved() {
    const RANGES: &str = r#"component App
  state at = 230
  state edge = 230
  action seek(k: string)
    if k == "ArrowRight"
      preventDefault()
      at = min(at + 10, 240)
  action toEnd(k: string)
    edge = 240
  action setAt(v: number)
    at = v
  action setEdge(v: number)
    edge = v
  view
    column width=320 height=400 align-items="flex-start"
      input type="range" min=0 max=240 step="any" value=at key=seek change=setAt aria-label="Seek"
      input type="range" min=0 max=240 step="any" value=edge key=toEnd change=setEdge aria-label="Edge"
"#;
    let mut p = boot(RANGES, (320., 400.), 0);
    let nodes = statement(&mut p).nodes;
    let (seek, edge) = (named(&nodes, "Seek"), named(&nodes, "Edge"));
    assert!(
        p.access_act(edge.id, act::FORWARD, 0.0, 0.0),
        "the handler moved it onto its maximum"
    );
    settle(&mut p);
    assert_eq!(p.range_value(edge.view), Some(240.0));
    assert!(p.access_act(seek.id, act::FORWARD, 0.0, 0.0));
    settle(&mut p);
    assert_eq!(p.range_value(seek.view), Some(240.0));
    assert!(
        !p.access_act(seek.id, act::FORWARD, 0.0, 0.0),
        "the handler took the key at the end of the range"
    );
    settle(&mut p);
    assert_eq!(p.range_value(seek.view), Some(240.0));
}
