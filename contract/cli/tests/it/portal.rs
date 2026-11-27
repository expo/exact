//! LLP 1013.002 D1, D3: `portal` is a string prop on the element it moves, and
//! the compiler refuses what it can prove no placement could do.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Kernel, PropId, ViewId};
use exact_plan::{BindingKind, Plan, Value};
use exact_runner::{Event, Runner};
use std::path::Path;

fn corpus(name: &str) -> String {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../corpus")
        .join(name);
    std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn refusal(view: &str) -> Option<String> {
    let source = format!("component App\n  state on = true\n  view\n    column\n{view}");
    contract::compile(&source).err().map(|e| e.id.to_string())
}

#[test]
fn both_fixtures_compile_with_one_portal_binding() {
    for name in ["portal.contract", "portal-flight.contract"] {
        let plan = contract::compile(&corpus(name)).unwrap();
        let portals = (plan.bindings.iter())
            .filter(|b| b.kind == BindingKind::Prop && b.id == PropId::Portal as u16)
            .count();
        assert_eq!(portals, 1, "{name}");
    }
}

#[test]
fn what_no_placement_could_move_is_refused() {
    for (view, id) in [
        // A list's row, a button's face, a paragraph's run, an svg's child.
        ("      list\n        for x in [1] key=x\n          box portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      button\n        text \"Go\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      p\n        strong \"Go\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      svg width=10 height=10\n        circle r=5 portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        // A dialog and a popover are presented, a route projected.
        ("      dialog portal=\"a\"\n        text \"Hi\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      box popover=\"auto\" id=\"m\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        // A destination that is a control, has a role, or is the mover itself or inside it.
        ("      button id=\"a\"\n        text \"Go\"\n      box portal=\"a\"\n", "lower-portal-target"),
        ("      box id=\"a\" role=\"group\"\n      box portal=\"a\"\n", "lower-portal-target"),
        ("      box portal=\"a\"\n        box id=\"a\"\n", "lower-portal-target"),
        ("      box id=\"a\" portal=\"a\"\n", "lower-portal-target"),
        // A route and a keyed destination.
        ("      column navigationKey=\"r\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      column id=\"a\" navigationKey=\"r\"\n      box portal=\"a\"\n", "lower-portal-target"),
        // An empty popover is a popover; a table's and a grouped list's parts are slots.
        ("      box popover=\"\" id=\"m\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      table\n        tr portal=\"a\"\n          td\n            text \"1\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      list\n        section\n          row portal=\"a\"\n            text \"1\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      box role=\"tab\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        ("      box role=\"dialog\" portal=\"a\"\n      box id=\"a\"\n", "lower-portal-mover"),
        // A computed id inside the mover names only its own descendant.
        ("      button id=\"a\"\n        text \"Go\"\n      box portal=\"a\"\n        box id=(on ? \"x\" : \"a\")\n", "lower-portal-target"),
    ] {
        assert_eq!(refusal(view).as_deref(), Some(id), "{view}");
    }
}

#[test]
fn an_app_root_cannot_move() {
    let source = "component App\n  view\n    column portal=\"a\"\n      box id=\"a\"\n";
    assert_eq!(
        contract::compile(source).unwrap_err().id,
        "lower-portal-mover"
    );
}

#[test]
fn what_placement_decides_compiles() {
    for view in [
        // A whole svg moves as one element; a video and a button may move.
        "      svg width=10 height=10 portal=\"a\"\n        circle r=5\n      box id=\"a\"\n",
        "      video \"m.mp4\" portal=\"a\"\n      box id=\"a\"\n",
        // A computed role or destination, a missing one, an empty one.
        "      box id=\"a\" role=(on ? \"group\" : \"\")\n      box portal=\"a\"\n",
        "      box id=\"a\"\n      box portal=(on ? \"a\" : \"\")\n",
        "      box portal=\"nowhere\"\n",
        "      box portal=\"\"\n",
        // A destination inside a route is an ordinary layout child.
        "      column navigationKey=\"r\"\n        box id=\"a\"\n      box portal=\"a\"\n        text \"Playing\"\n",
        // One carrier of the id can receive it; the runner hides the mover
        // when the first in declaration order cannot.
        "      button id=\"a\"\n        text \"Go\"\n      box id=\"a\"\n      box portal=\"a\"\n",
        // Each row's mover may name another row's box; an id in the other arm of
        // the mover's `if` is never beside it, so the mover hides.
        "      for x in [1, 2] key=x\n        box portal=\"p\"\n          if x == 1\n            box id=\"p\"\n",
        "      if on\n        box portal=\"a\"\n      else\n        button id=\"a\"\n          text \"Go\"\n",
        // HTML inside an svg's foreignObject is boxes again.
        "      svg width=10 height=10\n        foreignObject width=10 height=10\n          column\n            box portal=\"a\"\n      box id=\"a\"\n",
    ] {
        assert_eq!(refusal(view), None, "{view}");
    }
}

// Placement (LLP 1013.002 D1, D2): the runner shows the same live node where
// its `portal` says, hides it where nothing can receive it, and keeps its
// declaration for state, handlers and teardown.

fn boot(source: &str) -> Runner<Source> {
    let plan = contract::compile(source).unwrap();
    let plan = Plan::decode(&plan.encode()).unwrap();
    Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<Source>, test_id: &str) -> Option<ViewId> {
    let key = r.kernel().find_first_by_test_id(test_id)?;
    Some(r.kernel().node_by_key(key)?.id)
}

fn parent(r: &Runner<Source>, v: ViewId) -> Option<ViewId> {
    r.kernel().node(v).unwrap().parent
}

fn label(r: &Runner<Source>, test_id: &str) -> String {
    let k = r.kernel();
    let button = k.node(view(r, test_id).unwrap()).unwrap();
    let text = k.node(button.children()[0]).unwrap();
    text.props.str(PropId::Text).unwrap().to_owned()
}

fn press(r: &mut Runner<Source>, test_id: &str) {
    let v = view(r, test_id).unwrap();
    r.dispatch(v, Event::Press).unwrap();
}

#[test]
fn the_player_and_its_counter_move_between_routes_and_keep_their_state() {
    let mut r = boot(&corpus("portal.contract"));
    let (player, video) = (view(&r, "player").unwrap(), view(&r, "video").unwrap());
    assert_eq!(parent(&r, player), view(&r, "card"));
    press(&mut r, "count");
    assert_eq!(label(&r, "count"), "Count 1");
    press(&mut r, "open");
    assert_eq!(parent(&r, player), view(&r, "detail"));
    assert_eq!(view(&r, "video"), Some(video));
    // The press resolves at the declaration, beside the detail's own children.
    press(&mut r, "count");
    assert_eq!(label(&r, "count"), "Count 2");
    press(&mut r, "back");
    assert_eq!(parent(&r, player), view(&r, "card"));
    assert_eq!(label(&r, "count"), "Count 2");
    // Removing the declaration destroys what it shows; a new one is fresh.
    press(&mut r, "remove");
    assert!(r.kernel().node(player).is_none() && r.kernel().node(video).is_none());
    assert!(r
        .kernel()
        .node(view(&r, "card").unwrap())
        .unwrap()
        .children()
        .is_empty());
    press(&mut r, "remove");
    assert_ne!(view(&r, "player"), Some(player));
    assert_eq!(label(&r, "count"), "Count 0");
}

#[test]
fn the_flight_fixture_moves_the_player_into_each_new_destination() {
    let mut r = boot(&corpus("portal-flight.contract"));
    let player = view(&r, "player").unwrap();
    assert_eq!(parent(&r, player), view(&r, "card"));
    for (step, place) in ["detail", "card", "detail"].into_iter().enumerate() {
        press(&mut r, "toggle");
        assert_eq!(parent(&r, player), view(&r, place), "step {step}");
    }
    press(&mut r, "count");
    assert_eq!(label(&r, "count"), "Count 1");
}

const PLACES: &str = r#"component App
  state target = "a"
  state showA = true
  state alive = true
  action to(t: string)
    target = t
  action toggleA
    showA = not showA
  action toggle
    alive = not alive
  action both
    showA = not showA
    alive = not alive
  view
    column
      if showA
        box id="a" testId="a"
      box id="b" testId="b"
        text "B" testId="bt"
      if alive
        box portal=target testId="m"
          button press=toggle testId="mb"
            text "Hide"
"#;

#[test]
fn a_mover_parks_alive_and_returns_and_dies_once_with_its_declaration() {
    let mut r = boot(PLACES);
    let m = view(&r, "m").unwrap();
    let (a, b, bt) = (
        view(&r, "a").unwrap(),
        view(&r, "b").unwrap(),
        view(&r, "bt").unwrap(),
    );
    assert_eq!(parent(&r, m), Some(a));
    // Appended after the destination's own children.
    r.act("to", vec![Value::str("b")]).unwrap();
    assert_eq!(r.kernel().node(b).unwrap().children(), [bt, m]);
    // No destination: detached, alive, and not in any tree.
    r.act("to", vec![Value::str("nowhere")]).unwrap();
    assert_eq!(parent(&r, m), None);
    assert!(r.kernel().node(m).is_some());
    assert!(r.kernel().node(b).unwrap().children() == [bt]);
    // The destination dies: the mover survives, hidden, and returns with it.
    r.act("to", vec![Value::str("a")]).unwrap();
    assert_eq!(parent(&r, m), Some(a));
    r.act("toggleA", vec![]).unwrap();
    assert!(r.kernel().node(a).is_none());
    assert_eq!((r.kernel().node(m).is_some(), parent(&r, m)), (true, None));
    r.act("toggleA", vec![]).unwrap();
    assert_eq!(parent(&r, m), view(&r, "a"));
    // Its own handler runs from where it is shown.
    press(&mut r, "mb");
    assert!(r.kernel().node(m).is_none());
    assert!(r
        .kernel()
        .node(view(&r, "a").unwrap())
        .unwrap()
        .children()
        .is_empty());
    r.act("toggle", vec![]).unwrap();
    let m = view(&r, "m").unwrap();
    // Destination and declaration die in one commit, and return together.
    r.act("both", vec![]).unwrap();
    assert!(r.kernel().node(m).is_none() && view(&r, "a").is_none());
    r.act("both", vec![]).unwrap();
    assert_eq!(parent(&r, view(&r, "m").unwrap()), view(&r, "a"));
}

#[test]
fn cycles_self_targets_and_hidden_destinations_park_their_movers() {
    let mut r = boot(
        r#"component App
  state into = "y-slot"
  action to(t: string)
    into = t
  view
    column
      box id="x" portal=into testId="x"
        box id="x-slot" testId="xs"
      box id="y" portal="x-slot" testId="y"
        box id="y-slot" testId="ys"
      box id="z" portal="y-slot" testId="z"
"#,
    );
    let (x, y, z) = (
        view(&r, "x").unwrap(),
        view(&r, "y").unwrap(),
        view(&r, "z").unwrap(),
    );
    // x into y, y into x: both on the cycle park, and z, whose destination
    // is inside y, parks with it.
    assert_eq!(
        (parent(&r, x), parent(&r, y), parent(&r, z)),
        (None, None, None)
    );
    // x into its own descendant parks x, and with it the destinations in it.
    r.act("to", vec![Value::str("x-slot")]).unwrap();
    assert_eq!(
        (parent(&r, x), parent(&r, y), parent(&r, z)),
        (None, None, None)
    );
    // Back where it is declared: y goes into x, z into y.
    r.act("to", vec![Value::str("")]).unwrap();
    assert!(parent(&r, x).is_some());
    assert_eq!(parent(&r, y), view(&r, "xs"));
    assert_eq!(parent(&r, z), view(&r, "ys"));
}

#[test]
fn the_first_element_with_the_id_receives_and_a_later_one_takes_over() {
    let mut r = boot(
        r#"component App
  state first = true
  state bad = "g"
  action flip
    first = not first
  view
    column
      if first
        box id="d" testId="d1"
      box id="d" testId="d2"
      button role="group" id="g" testId="g"
        text "Group"
      box portal="d" testId="m"
      box portal=bad testId="n"
"#,
    );
    let (m, n) = (view(&r, "m").unwrap(), view(&r, "n").unwrap());
    assert_eq!(parent(&r, m), view(&r, "d1"));
    // A destination that cannot receive hides its mover.
    assert_eq!(parent(&r, n), None);
    r.act("flip", vec![]).unwrap();
    assert_eq!(parent(&r, m), view(&r, "d2"));
}

#[test]
fn a_row_moves_with_its_own_frame_and_keeps_its_place_through_a_reorder() {
    let mut r = boot(
        r#"component App
  state swapped = false
  state picked = ""
  action pick(x: string)
    picked = x
  action swap
    swapped = not swapped
  view
    column
      text picked testId="picked"
      box id="slot-p" testId="sp"
      box id="slot-q" testId="sq"
      for x in (swapped ? ["q", "p"] : ["p", "q"]) key=x
        box portal=`slot-${x}` testId=`m-${x}`
          button press=pick(x) testId=`b-${x}`
            text x
"#,
    );
    let (mp, mq) = (view(&r, "m-p").unwrap(), view(&r, "m-q").unwrap());
    assert_eq!(
        (parent(&r, mp), parent(&r, mq)),
        (view(&r, "sp"), view(&r, "sq"))
    );
    r.act("swap", vec![]).unwrap();
    assert_eq!(
        (parent(&r, mp), parent(&r, mq)),
        (view(&r, "sp"), view(&r, "sq"))
    );
    press(&mut r, "b-q");
    let picked = r.kernel().node(view(&r, "picked").unwrap()).unwrap();
    assert_eq!(picked.props.str(PropId::Text), Some("q"));
}

#[test]
fn movers_sharing_a_destination_follow_declaration_order_through_a_reorder() {
    let mut r = boot(
        r#"component App
  state swapped = false
  action swap
    swapped = not swapped
  view
    column
      box id="slot" testId="slot"
      for x in (swapped ? ["q", "p"] : ["p", "q"]) key=x
        box portal="slot" testId=`m-${x}`
"#,
    );
    let (p, q, slot) = (
        view(&r, "m-p").unwrap(),
        view(&r, "m-q").unwrap(),
        view(&r, "slot").unwrap(),
    );
    assert_eq!(r.kernel().node(slot).unwrap().children(), [p, q]);
    r.act("swap", vec![]).unwrap();
    assert_eq!(r.kernel().node(slot).unwrap().children(), [q, p]);
}

#[test]
fn a_virtualized_lists_rows_stay_its_own_beside_a_fixed_child_that_says_portal() {
    let r = boot(
        r#"component App
  view
    list virtualized=true appearance="none" height=200 testId="list"
      box portal="" testId="fixed"
      for x in [1, 2, 3] key=x
        text `${x}` testId=`row-${x}`
"#,
    );
    let list = r.kernel().node(view(&r, "list").unwrap()).unwrap();
    assert!(list.children().len() > 1, "{:?}", list.children());
    assert!(view(&r, "row-1").is_some_and(|v| r.kernel().node(v).unwrap().parent.is_some()));
}

#[test]
fn a_destination_takes_its_mover_once_its_computed_popover_is_none() {
    let mut r = boot(
        r#"component App
  state open = true
  state into = "d"
  action close
    open = false
  view
    column
      box id="d" testId="d" popover=(open ? some("manual") : none)
      box portal=into testId="m"
"#,
    );
    let m = view(&r, "m").unwrap();
    assert_eq!(parent(&r, m), None);
    r.act("close", vec![]).unwrap();
    assert_eq!(parent(&r, m), view(&r, "d"));
}

#[test]
fn a_whole_table_moves_and_a_long_chain_parks_past_the_depth_limit() {
    let mut r = boot(
        r#"component App
  view
    column
      box id="d" testId="d"
      table portal="d" testId="t"
        tr
          td
            text "1"
"#,
    );
    assert_eq!(parent(&r, view(&r, "t").unwrap()), view(&r, "d"));
    // 200 boxes, each shown in the next: the chain would nest 200 deep.
    let n = 200;
    let items: Vec<String> = (0..n).map(|i| i.to_string()).collect();
    r = boot(&format!(
        "component App\n  view\n    column\n      for i in [{}] key=i\n        box id=`m-${{i}}` testId=`m-${{i}}` portal=(i == {} ? \"\" : `m-${{i + 1}}`)\n",
        items.join(", "),
        n - 1
    ));
    let shown = (0..n)
        .filter(|i| parent(&r, view(&r, &format!("m-{i}")).unwrap()).is_some())
        .count();
    assert!(shown < n && shown > 100, "{shown} of {n} shown");
}

#[test]
fn a_route_headers_bar_item_is_refused_and_a_foreign_object_box_receives() {
    let source = r#"routes nav
  tab home "/"

component App
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}`
          header
            text "Home" role="heading"
            button portal="d"
              text "Done"
          box id="d"
"#;
    assert_eq!(
        contract::compile(source).unwrap_err().id,
        "lower-portal-mover"
    );
    let r = boot(
        "component App\n  view\n    column\n      svg width=10 height=10\n        foreignObject width=10 height=10\n          box id=\"d\" testId=\"d\"\n      box portal=\"d\" testId=\"m\"\n",
    );
    assert_eq!(parent(&r, view(&r, "m").unwrap()), view(&r, "d"));
}

#[test]
fn a_reload_keeps_a_mover_declared_before_its_destination() {
    let source = "component App\n  view\n    column\n      video \"m.mp4\" portal=\"d\" testId=\"v\"\n      box id=\"d\" testId=\"d\"\n";
    let r = boot(source);
    let v = view(&r, "v").unwrap();
    assert_eq!(parent(&r, v), view(&r, "d"));
    let plan = Plan::decode(&contract::compile(source).unwrap().encode()).unwrap();
    let again = Runner::boot_carrying(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        &r.carry(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(view(&again, "v"), Some(v));
    assert_eq!(parent(&again, v), view(&again, "d"));
}

#[test]
fn hidden_movers_are_journaled_in_declaration_order() {
    let movers: String = (0..8)
        .map(|i| format!("      box portal=gone testId=\"m{i}\"\n"))
        .collect();
    let r = boot(&format!(
        "component App\n  state gone = \"nowhere\"\n  view\n    column\n{movers}"
    ));
    let said: Vec<&str> = r.journal().filter(|l| l.contains("hides it")).collect();
    let expected: Vec<String> = (0..8)
        .map(|i| {
            let v = view(&r, &format!("m{i}")).unwrap();
            format!("view {v}: `portal=\"nowhere\"` hides it")
        })
        .collect();
    assert_eq!(said.len(), expected.len(), "{said:#?}");
    for (line, want) in said.iter().zip(&expected) {
        assert!(line.contains(want.as_str()), "{said:#?}");
    }
}

#[test]
fn removing_movers_shown_apart_emits_their_ops_in_declaration_order() {
    // Eight movers, each in its own destination, declared under one `if`.
    let places: String = (0..8)
        .map(|i| format!("      box id=\"d{i}\" testId=\"d{i}\"\n"))
        .collect();
    let movers: String = (0..8)
        .map(|i| format!("        box portal=\"d{i}\" testId=\"m{i}\"\n"))
        .collect();
    let mut r = boot(&format!(
        "component App\n  state alive = true\n  action kill\n    alive = false\n  view\n    column\n      button press=kill testId=\"kill\"\n        text \"Kill\"\n{places}      if alive\n       column\n{movers}"
    ));
    let movers: Vec<ViewId> = (0..8)
        .map(|i| view(&r, &format!("m{i}")).unwrap())
        .collect();
    let places: Vec<ViewId> = (0..8)
        .map(|i| view(&r, &format!("d{i}")).unwrap())
        .collect();
    for (m, d) in movers.iter().zip(&places) {
        assert_eq!(parent(&r, *m), Some(*d));
    }
    let k = |r: &Runner<Source>, v: ViewId| r.kernel().node(v).unwrap().key;
    let mover_keys: Vec<_> = movers.iter().map(|&m| k(&r, m)).collect();
    let place_keys: Vec<_> = places.iter().map(|&d| k(&r, d)).collect();
    let kill = view(&r, "kill").unwrap();
    let receipt = r.dispatch(kill, Event::Press).unwrap();
    let destroyed: Vec<_> = (receipt.destroyed.iter())
        .filter(|key| mover_keys.contains(key))
        .copied()
        .collect();
    assert_eq!(
        destroyed, mover_keys,
        "the movers' destroys in declaration order"
    );
    let mut touched: Vec<_> = (receipt.touched.iter())
        .filter(|key| place_keys.contains(key))
        .copied()
        .collect();
    touched.dedup();
    assert_eq!(
        touched, place_keys,
        "the destinations' lists in declaration order"
    );
}

#[test]
fn a_mover_shown_in_its_own_declaration_parent_names_no_declaration() {
    let r = boot("component App\n  view\n    column id=\"p\" testId=\"p\"\n      box portal=\"p\" testId=\"m\"\n");
    let m = view(&r, "m").unwrap();
    assert_eq!(parent(&r, m), view(&r, "p"));
    assert_eq!(r.declared_parent(m), None, "it bubbles as any node does");
    assert!(r.declared_parents().is_empty());
}

#[test]
fn a_mover_whose_parent_loses_holds_while_its_destination_unmounts_survives_and_reattaches() {
    let mut r = boot(
        r#"component App
  state as_tabs = false
  state show_dest = true
  action flip
    as_tabs = true
    show_dest = false
  action unflip
    as_tabs = false
    show_dest = true
  view
    column
      if show_dest
        box id="dest" testId="dest"
      column role=(as_tabs ? "tablist" : "") testId="p"
        box portal="dest" testId="e"
"#,
    );
    let dest = view(&r, "dest").unwrap();
    let p = view(&r, "p").unwrap();
    let e = view(&r, "e").unwrap();
    assert_eq!(parent(&r, e), Some(dest));
    r.act("flip", vec![]).unwrap();
    assert!(view(&r, "dest").is_none());
    assert!(r.kernel().node(e).is_some());
    assert_eq!(parent(&r, e), Some(p));
    assert_eq!(r.kernel().node(p).unwrap().children(), [e]);
    // Runner is healthy and moves `e` back once `p` holds again and `dest` returns.
    r.act("unflip", vec![]).unwrap();
    assert_eq!(parent(&r, e), view(&r, "dest"));
}
