//! The geometry facts (@ref LLP 1123 D6): `exactGeometry` answers from the layout just
//! committed, a later layout's change reaches it in a later commit, and a chain of commits it
//! causes, from a layout no delivery made through its own commits' layouts, stops at D1's bound
//! of eight, its answer holding until a layout with another cause.

use exact_kernel::{Kernel, Offer};
use exact_plan::Value;
use exact_runner::{agent, DataError, DataSource, Event, Runner, RunnerError};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("queried {name}")
    }
}

const SHAPES: &str = "shape Rect
  x: number
  y: number
  width: number
  height: number
shape Insets
  top: number
  right: number
  bottom: number
  left: number
shape Observed
  safeArea: option<Insets>
  layoutMargins: option<Insets>
  bar: option<Insets>
shape Margins
  layoutMargins: option<Rect>
shape Hero
  available: bool
  generation: number
  frame: option<Rect>
  observations: option<Observed>
  scroll: option<Rect>
  margins: option<Margins>
";

/// A runner for `component` under [`SHAPES`].
fn boot(component: &str) -> Result<Runner<NoData>, RunnerError> {
    Runner::boot(
        contract::compile(&format!("{SHAPES}{component}")).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
}

/// A plan whose `hero` is `width` wide and `attrs` besides, reading its own geometry.
fn hero(width: &str, attrs: &str) -> Runner<NoData> {
    boot(&format!(
        "component App
  state grows = 0
  state seen = 0
  action grow
    grows = grows + 1
  action fit(w: number, h: number, r: DOMRectReadOnly)
    seen = w
  resource g = exactGeometry(\"hero\") as shape Hero
  derive last = match g.frame {{ case some(f) => f.width, case none => 0 }}
  view
    column
      column id=\"hero\" width={width} height=40 {attrs}
"
    ))
    .unwrap()
}

fn lay_out(r: &mut Runner<NoData>) {
    lay_out_at(r, 400.0);
}

fn lay_out_at(r: &mut Runner<NoData>, width: f32) {
    let root = r.roots()[0];
    r.kernel_mut()
        .compute_layout(root, Offer::definite(width, 800.0))
        .unwrap();
}

/// A resource's answer as its fields, a `Hero`'s in [`SHAPES`]' order.
fn fields(r: &Runner<NoData>, resource: &str) -> Vec<Value> {
    match r.shown_resource(resource) {
        Some(Value::Record(fields)) => fields.to_vec(),
        other => panic!("{resource} is a record: {other:?}"),
    }
}

fn number(v: &Value) -> f64 {
    match v {
        Value::Number(n) => *n,
        other => panic!("a number: {other:?}"),
    }
}

fn some(v: &Value) -> Vec<Value> {
    match v {
        Value::Option(Some(inner)) => match &**inner {
            Value::Record(fields) => fields.to_vec(),
            other => panic!("a record: {other:?}"),
        },
        other => panic!("a value: {other:?}"),
    }
}

/// The text of the node whose `testId` is `test_id`.
fn text(r: &Runner<NoData>, test_id: &str) -> Option<String> {
    let tree = agent::tree(r);
    let key = format!("\"testId\":\"{test_id}\"");
    let node = tree.split("{\"id\":").find(|n| n.contains(&key))?;
    let at = node.find("\"text\":\"")? + 8;
    Some(node[at..].split('"').next()?.to_string())
}

/// The `g` the app shows: its frame's width, its generation, and the top of its safe area.
fn shown(r: &Runner<NoData>) -> (f64, f64, f64) {
    let g = fields(r, "g");
    let safe_area = some(&some(&g[3])[0]);
    (
        number(&some(&g[2])[2]),
        number(&g[1]),
        number(&safe_area[0]),
    )
}

#[test]
fn a_layout_reaches_the_fact_in_a_later_commit() {
    // The layout makes `hero` 120 wide, which the next delivery answers in a commit of its own
    // with the safe area every box observes, read or not, and a quantity only a host has as
    // `none`; a delivery with nothing changed commits nothing
    let mut r = hero("120", "");
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    assert_eq!(shown(&r), (120.0, 1.0, 0.0));
    assert_eq!(
        fields(&r, "g")[4],
        Value::NONE,
        "no host reports a scroller yet"
    );
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_none());
}

#[test]
fn a_route_bar_reaches_a_geometry_reader_at_the_next_layout() {
    use exact_kernel::{BlockAxis, HostBlock};
    let mut r = hero("120", "");
    let slot = r.kernel().find_by_id("hero")[0].index;
    let view = r.kernel().arena().local_id(slot);
    let block = HostBlock {
        tactic: Default::default(),
        width: BlockAxis::Is(120.0),
        height: BlockAxis::Is(40.0),
    };
    r.kernel_mut().set_host_block(view, Some(block)).unwrap();
    for width in [0.0, 69.0, 0.0] {
        assert!(!r
            .kernel_mut()
            .set_route_bar_insets(view, [0.0, width])
            .unwrap());
        lay_out(&mut r);
        assert!(r.refresh_geometry().unwrap().is_some());
        let sides = some(&some(&fields(&r, "g")[3])[2]);
        assert_eq!(sides, [0.0, width as f64, 0.0, 0.0].map(Value::Number));
    }
}

#[test]
fn a_band_reported_before_its_layout_reaches_the_fact_after_it() {
    // A session band of 20 owes no layout, since no style reads it, and reaches `hero`'s safe
    // area and, where no host reports margins, its layout margins at the next layout: a
    // delivery before that layout commits nothing, which would pair the band with the last
    // layout's frames
    let mut r = hero("120", "");
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    let owed = r.kernel_mut().set_session_band([20.0, 0.0, 0.0, 0.0]);
    assert!(!owed.unwrap());
    assert!(r.refresh_geometry().unwrap().is_none());
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    assert_eq!(shown(&r), (120.0, 2.0, 20.0));
    let margins = some(&some(&fields(&r, "g")[5])[0]);
    assert_eq!(margins, [0.0, 20.0, 120.0, 20.0].map(Value::Number));
}

#[test]
fn each_external_layout_starts_a_chain() {
    // `hero` is half the window: each width in turn is a chain of one commit, none held at the
    // bound, whether the next delivery finds that commit laid out and nothing changed (the
    // first ten) or finds no layout (the next ten), as after a commit a host did not lay out
    // because it moved no node
    let mut r = hero("\"50%\"", "");
    for (i, width) in (0..20).map(|i| (i, 300.0 + 10.0 * i as f32)) {
        lay_out_at(&mut r, width);
        assert!(r.refresh_geometry().unwrap().is_some(), "at {width}");
        if i < 10 {
            lay_out_at(&mut r, width);
        }
        assert!(
            r.refresh_geometry().unwrap().is_none(),
            "settled at {width}"
        );
        assert_eq!(shown(&r).0, f64::from(width) / 2.0);
    }
}

#[test]
fn a_fact_that_moves_its_own_layout_holds_at_the_bound() {
    // `hero` is one wider than it last read, so every delivery moves it again: the chain stops
    // after eight commits, its answer held, and a layout with another cause releases the hold
    let mut r = hero("(last + 1)", "");
    for _ in 0..8 {
        lay_out(&mut r);
        assert!(r.refresh_geometry().unwrap().is_some());
    }
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_none(), "the ninth is held");
    let held = shown(&r).0;
    r.act("grow", vec![]).unwrap();
    lay_out(&mut r);
    assert!(
        r.refresh_geometry().unwrap().is_some(),
        "another cause releases the hold"
    );
    assert!(shown(&r).0 > held);
}

#[test]
fn a_chain_holds_through_the_resize_events_its_layouts_raise() {
    // As above, with a `resize` handler on `hero` that commits at each of its layouts, as a
    // host delivers resize events before the facts: those commits are the chain's, not another
    // cause, so it still stops after eight
    let mut r = hero("(last + 1)", "resize=fit");
    let mut commits = 0;
    for _ in 0..20 {
        lay_out(&mut r);
        for (view, rect, _) in r.resize_due(0) {
            r.resize_delivered(view, rect);
            r.dispatch(view, Event::Resize(rect)).unwrap();
        }
        lay_out(&mut r);
        match r.refresh_geometry().unwrap() {
            Some(_) => commits += 1,
            None => break,
        }
    }
    assert_eq!(commits, 8);
}

#[test]
fn another_facts_answer_is_no_change_to_a_fact() {
    // `a` widens and is answered again at a new generation; `b`, unchanged, keeps the one it
    // was answered at, so the next delivery finds nothing to answer
    let mut r = boot(
        "component App
  state wide = false
  action widen
    wide = true
  resource a = exactGeometry(\"a\") as shape Hero
  resource b = exactGeometry(\"b\") as shape Hero
  view
    column
      column id=\"a\" width=(wide ? 200 : 100) height=10
      column id=\"b\" width=50 height=10
",
    )
    .unwrap();
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    r.act("widen", vec![]).unwrap();
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_none(), "b is unchanged");
    assert_eq!(number(&fields(&r, "a")[1]), 2.0);
    assert_eq!(number(&fields(&r, "b")[1]), 1.0);
}

#[test]
fn a_box_layout_did_not_place_is_unavailable() {
    // Under `display: none` `hero` has no box: unavailable, with no frame, before and after
    // the layout, so the layout changes nothing
    let mut r = hero("120", "display=\"none\"");
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_none());
    let g = fields(&r, "g");
    assert_eq!((&g[0], &g[2]), (&Value::Bool(false), &Value::NONE));
}

#[test]
fn a_node_space_starts_at_that_nodes_border_box() {
    // `outer` sits at (5, 7) and `hero` 10 in from its left and 20 down, so in `outer`'s space
    // `hero`'s frame starts at (10, 20); the window's is the host's to place, `none` here; a
    // space the source does not have is refused, naming the ones it has
    let plan = |space: &str| {
        format!(
            "component App
  resource g = exactGeometry(\"hero\", {space}) as shape Hero
  view
    column padding-left=5 padding-top=7
      column id=\"outer\" padding-left=10 padding-top=20
        column id=\"hero\" width=50 height=40
"
        )
    };
    let frame = |space: &str| {
        let mut r = boot(&plan(space)).unwrap();
        lay_out(&mut r);
        r.refresh_geometry().unwrap();
        fields(&r, "g")[2].clone()
    };
    let outer = some(&frame("\"node\", \"outer\""));
    assert_eq!(&outer[..2], &[Value::Number(10.0), Value::Number(20.0)]);
    assert_eq!(frame("\"window\""), Value::NONE);
    let refused = boot(&plan("\"page\""))
        .err()
        .expect("an unknown space is refused");
    let RunnerError::Data {
        error: DataError::BadArguments(message),
        ..
    } = &refused
    else {
        panic!("a bad argument: {refused:?}")
    };
    assert!(
        message.contains(r#""owner", "window" or "node""#),
        "{message}"
    );
}

#[test]
fn the_unsettled_list_counts_what_did_not_settle() {
    // After a layout that settled everything the list is available and empty; before any
    // layout the kernel measured nothing, so it is unavailable
    let mut r = boot(
        "shape Unsettled
  available: bool
  count: number
  ids: string[]
component App
  resource u = exactSafeAreaUnsettled() as shape Unsettled
  view
    column
",
    )
    .unwrap();
    assert_eq!(fields(&r, "u")[0], Value::Bool(false));
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    let u = fields(&r, "u");
    assert_eq!((&u[0], number(&u[1])), (&Value::Bool(true), 0.0));
}

#[test]
fn a_childs_fact_answers_with_its_instance_and_ends_with_it() {
    // A fact a child declares is its instance's own request: unavailable until the layout
    // after the render that made the instance, answered after that layout, and asked nothing
    // once a render destroys the instance, whose node is then gone
    let mut r = boot(
        "component App
  state open = false
  action toggle
    open = !open
  view
    column
      if open
        Card()
component Card
  resource g = exactGeometry(\"card\") as shape Hero
  derive width = match g.frame { case some(f) => f.width, case none => 0 }
  view
    column id=\"card\" width=120 height=40
      text `${width}` testId=\"width\"
",
    )
    .unwrap();
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_none());
    r.act("toggle", vec![]).unwrap();
    assert_eq!(text(&r, "width").as_deref(), Some("0"));
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    assert_eq!(text(&r, "width").as_deref(), Some("120"));
    r.act("toggle", vec![]).unwrap();
    lay_out(&mut r);
    assert!(
        r.refresh_geometry().unwrap().is_none(),
        "the instance's request ended"
    );
}

#[test]
fn the_native_chain_answers_the_targets_its_holder_reported() {
    // A box outside every owner reads the session view's targets: unavailable until they are
    // reported, then, at the next layout, the chain by number and the relations by name, each
    // with its kinds, identity, generation, frame in its window and raw insets, `none` while
    // the target is unavailable
    use exact_kernel::style::targets::{ChainEntry, Kind, NativeTarget, NativeTargets, Relation};
    let mut r = boot(
        "shape Target
  number: option<number>
  relation: option<string>
  kinds: string[]
  id: number
  generation: number
  frame: option<Rect>
  insets: option<Insets>
shape Chain
  available: bool
  generation: number
  entries: option<Target[]>
component App
  resource c = exactNativeChain(\"hero\") as shape Chain
  view
    column id=\"hero\"
",
    )
    .unwrap();
    lay_out(&mut r);
    assert_eq!(fields(&r, "c")[0], Value::Bool(false));
    let controller = NativeTarget {
        id: 4,
        generation: 2,
        insets: Some([62.0, 0.0, 34.0, 0.0]),
        frame: Some([0.0, 0.0, 402.0, 874.0]),
    };
    let window = NativeTarget {
        id: 5,
        generation: 1,
        insets: None,
        frame: None,
    };
    let targets = NativeTargets {
        chain: vec![ChainEntry {
            target: controller,
            kinds: Kind::Navigation.bit(),
        }],
        relations: vec![(Relation::Window, window)],
    };
    r.kernel_mut().set_session_targets(targets).unwrap();
    lay_out(&mut r);
    assert!(r.refresh_geometry().unwrap().is_some());
    let c = fields(&r, "c");
    assert_eq!(c[0], Value::Bool(true));
    let Value::Option(Some(entries)) = &c[2] else {
        panic!("entries: {:?}", c[2])
    };
    let Value::List(entries) = &**entries else {
        panic!("a list: {entries:?}")
    };
    let entry = |i: usize| match &entries[i] {
        Value::Record(fields) => fields.to_vec(),
        other => panic!("a record: {other:?}"),
    };
    let (first, second) = (entry(0), entry(1));
    assert_eq!(first[0], Value::some(Value::Number(1.0)));
    assert_eq!(first[1], Value::Option(None));
    assert_eq!(first[2], Value::list(vec![Value::str("navigation")]));
    assert_eq!((number(&first[3]), number(&first[4])), (4.0, 2.0));
    assert_eq!(number(&some(&first[5])[2]), 402.0);
    assert_eq!(number(&some(&first[6])[0]), 62.0);
    assert_eq!(second[0], Value::Option(None));
    assert_eq!(second[1], Value::some(Value::str("window")));
    assert_eq!(
        (&second[5], &second[6]),
        (&Value::Option(None), &Value::Option(None))
    );
}

#[test]
fn an_owners_chrome_reaches_its_boxes_geometry() {
    // The chrome an owner's host reports reaches a box in the owner at the next layout, each
    // quantity the host does not have `none`; outside every owner there is none
    use exact_kernel::{BlockAxis, HostBlock, OwnerChrome, OwnerUpdate, VerticalBarEdge};
    let mut r = boot(
        "shape Chrome
  verticalBarEdge: option<string>
  sidebarAvailable: option<bool>
  overscan: option<string>
  auxiliaryTopLeftArea: option<Rect>
  auxiliaryTopRightArea: option<Rect>
shape G
  available: bool
  chrome: option<Chrome>
component App
  resource g = exactGeometry(\"card\") as shape G
  resource o = exactGeometry(\"outside\") as shape G
  view
    column
      column id=\"outside\" height=10
      column id=\"route\" width=120 height=40
        column id=\"card\" height=10
",
    )
    .unwrap();
    let slot = r.kernel().find_by_id("route")[0].index;
    let route = r.kernel().arena().local_id(slot);
    let block = HostBlock {
        tactic: Default::default(),
        width: BlockAxis::Is(120.0),
        height: BlockAxis::Is(40.0),
    };
    r.kernel_mut().set_host_block(route, Some(block)).unwrap();
    lay_out(&mut r);
    r.refresh_geometry().unwrap();
    assert_eq!(fields(&r, "g")[1], Value::Option(None));
    let update = OwnerUpdate {
        chrome: Some(OwnerChrome {
            vertical_bar_edge: Some(VerticalBarEdge::Trailing),
            sidebar_available: None,
        }),
        ..OwnerUpdate::default()
    };
    assert!(!r.kernel_mut().update_owner(route, update).unwrap());
    lay_out(&mut r);
    r.refresh_geometry().unwrap();
    let chrome = some(&fields(&r, "g")[1]);
    assert_eq!(chrome[0], Value::some(Value::str("trailing")));
    assert!(chrome[1..].iter().all(|v| *v == Value::Option(None)));
    assert_eq!(fields(&r, "o")[1], Value::Option(None));
}
