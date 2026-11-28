//! A list row's press credit (LLP 1115.004 D16 item 3): the visit its own handler's commit opened or moved, whenever
//! that handler runs, for as long as the list stands and the route shows that visit at that location.
use super::regression::act;
use super::*;
use exact_runner::Value;

/// The test app with a computed ask, and a second pushing list before the first, shown on `second`
fn asks() -> String {
    APP.replace("\"SPLIT\"", "ask")
        .replace(
            "  state value",
            "  state ask = \"split\"\n  action setAsk(a: string)\n    ask = a\n  state second = false\n  action showSecond(on: bool)\n    second = on\n  state value",
        )
        .replace(
            "            list testId=\"rows\"\n",
            "            if second\n              list testId=\"rows2\"\n                button press=open(\"c\") height=48 testId=\"c\" aria-current=\"page\"\n                  text \"Gamma\"\n            list testId=\"rows\"\n",
        )
}

fn back_in(p: &Presenter<()>, route: ViewId) -> ViewId {
    p.host.kernel().preorder_where(&[route], |_, props| {
        props.str(PropId::TestId) == Some("back-item")
    })[0]
}

#[test]
fn a_row_press_keeps_its_credit_while_its_list_does_whatever_the_stack_asks() {
    let mut p = boot_source(1200., &asks());
    tap(&mut p, "a");
    tap(&mut p, "deeper-item");
    tap(&mut p, "b");
    let (pane, b) = (split(&p).detail.unwrap(), id(&p, "b"));
    assert_eq!((split(&p).base, split(&p).row), (Some(pane), Some(b)));
    // The stack stops asking and asks again: the list and the visit stand, so the press's credit does
    act(&mut p, "setAsk", vec![Value::str("none")]);
    assert!(p.host.navigation.split.active.is_none());
    act(&mut p, "setAsk", vec![Value::str("split")]);
    assert_eq!((split(&p).base, split(&p).row), (Some(pane), Some(b)));
    assert_eq!(cover(&p, back_in(&p, pane)), Some(HostCover::Whole));
    // An earlier list becomes the split's: it supplies the selected row, its current one, and the deeper detail,
    // which none of its rows opened, shows its Back
    act(&mut p, "showSecond", vec![Value::Bool(true)]);
    assert_eq!(split(&p).list, Some(id(&p, "rows2")));
    assert_eq!(split(&p).row, Some(id(&p, "c")));
    assert_ne!(split(&p).base, Some(pane));
    assert_eq!(cover(&p, back_in(&p, pane)), None);
    act(&mut p, "showSecond", vec![Value::Bool(false)]);
    assert_eq!((split(&p).base, split(&p).row), (Some(pane), Some(b)));
}

#[test]
fn a_press_credit_goes_with_its_visit_or_its_list() {
    // A replace while the stack does not ask moves the same visit, so its credit goes then
    let mut p = boot_source(1200., &asks());
    tap(&mut p, "a");
    let item = id(&p, "route-item");
    assert_eq!(split(&p).row, Some(id(&p, "a")));
    let (visit_id, _) = visit(p.host.runner(), item).unwrap();
    act(&mut p, "setAsk", vec![Value::str("none")]);
    assert_eq!(p.host.navigation.split.actions.len(), 1);
    assert_eq!(
        visit(p.host.runner(), item),
        Some((visit_id, "/item/a".into()))
    );
    act(&mut p, "open", vec![Value::str("b")]);
    assert_eq!(id(&p, "route-item"), item);
    assert_eq!(
        visit(p.host.runner(), item),
        Some((visit_id, "/item/b".into()))
    );
    assert!(p.host.navigation.split.actions.is_empty());
    act(&mut p, "setAsk", vec![Value::str("split")]);
    assert_eq!((split(&p).base, split(&p).row), (Some(item), None));
    // A list mounted again is a new list, which marks the row the app says is current, not the old list's press
    let mut source = asks()
        .replace(
            "  state value",
            "  state listed = true\n  action list(on: bool)\n    listed = on\n  state value",
        )
        .replace("testId=\"b\"\n", "testId=\"b\" aria-current=\"page\"\n");
    let start = source.find("            list testId=\"rows\"\n").unwrap();
    let end = source[start..]
        .find("            box navigationSplit")
        .unwrap()
        + start;
    let block: String = source[start..end]
        .lines()
        .map(|l| format!("  {l}\n"))
        .collect();
    source.replace_range(start..end, &format!("            if listed\n{block}"));
    let mut p = boot_source(1200., &source);
    tap(&mut p, "a");
    assert_eq!(split(&p).row, Some(id(&p, "a")));
    act(&mut p, "list", vec![Value::Bool(false)]);
    assert!(p.host.navigation.split.actions.is_empty());
    act(&mut p, "list", vec![Value::Bool(true)]);
    assert_eq!(split(&p).row, Some(id(&p, "b")));
}

#[test]
fn a_back_marker_inside_the_lists_route_is_no_stack_of_its_own() {
    // A wrapper inside Home's route that names a Back action, with no navigationKey, is a navigation root with no
    // stack: Alpha's detail is credited in Home's
    let mut source = APP.replace("SPLIT", "");
    let start = source.find("            list testId=\"rows\"\n").unwrap();
    let end = source[start..]
        .find("            box navigationSplit")
        .unwrap()
        + start;
    let block: String = source[start..end]
        .lines()
        .map(|l| format!("  {l}\n"))
        .collect();
    source.replace_range(
        start..end,
        &format!("            column navigationBack=\"back\" testId=\"marker\"\n{block}"),
    );
    let mut p = boot_source(1200., &source);
    assert!(p.host.kernel().find_by_test_id("marker").len() == 1);
    tap(&mut p, "a");
    let item = id(&p, "route-item");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(item), Some(id(&p, "a")))
    );
}

#[test]
fn a_row_whose_press_selects_another_tab_leaves_that_tabs_selection() {
    let source = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
component App
  action tab(name: string)
    nav = select(nav, name)
  action open(id: string)
    nav = push(nav, path("item", id))
  action follow(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow width="100%" height="100%" display="flex" flex-direction="column"
      column flex=1 min-height=0 position="relative"
        for t in nav.tabs key=t.name
          column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
            for e in t.stack key=e.id
              column navigationKey=`${e.id}` position="absolute" inset=0 testId=`route-${e.name}`
                if e.name == "home"
                  list testId="rows"
                    button press=open("a") height=48 testId="a"
                      text "Alpha"
                if e.name == "item"
                  header
                    button id="back" press=back testId="back-item"
                      text "Back"
                    h1 "Item"
                if e.name == "other"
                  list testId="other-rows"
                    button press=tab("home") height=48 testId="to-home"
                      text "Home"
      row role="tablist" height=60 flex-shrink=0 testId="tabs"
        for t in nav.tabs key=t.name
          button role="tab" aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=tab(t.name) testId=`tab-${t.name}`
            text t.name
"#;
    let mut p = boot_source(1200., source);
    tap(&mut p, "a");
    let item = id(&p, "route-item");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(item), Some(id(&p, "a")))
    );
    tap(&mut p, "tab-other");
    // The other tab's row selects the first tab, whose detail Alpha's press opened
    tap(&mut p, "to-home");
    assert_eq!(split(&p).root, id(&p, "route-home"));
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(item), Some(id(&p, "a")))
    );
    // Alpha's press waits behind a change action, and a switch to the other tab waits behind it: one advance runs
    // both handlers, so Alpha's detail opens in its own tab's stack while the other tab shows, and is Alpha's there
    let held = source.replace(
        "component App\n",
        "component App\n  state armed = false\n  state input = 0\n  state seen = 0\n  action arm\n    armed = true\n  action tick\n    input = input + 1\n  action apply\n    seen = input\n  task timed when armed\n    after(50, tick)\n  task sync key=input\n    change(apply)\n",
    );
    let mut p = boot_source(1200., &held);
    act(&mut p, "arm", vec![]);
    let at = p.host.now() + 100.;
    let (a, other) = (id(&p, "a"), id(&p, "tab-other"));
    p.dispatch_press(a, at, true);
    p.dispatch_press(other, at + 1., true);
    assert_eq!(
        p.host.navigation.split.presses.len(),
        1,
        "Alpha's press waits"
    );
    settle(&mut p, at + 1.);
    assert_eq!(
        p.host.shown_stack().and_then(|s| s.first()).copied(),
        Some(id(&p, "route-other"))
    );
    tap(&mut p, "to-home");
    let item = id(&p, "route-item");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(item), Some(id(&p, "a")))
    );
    // Alpha's press also renders Home's tab control anew, in another branch with the same id: Home still names
    // Alpha's panel, so the detail is Alpha's
    let tabs = r#"        for t in nav.tabs key=t.name
          button role="tab" aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=tab(t.name) testId=`tab-${t.name}`
            text t.name
"#;
    let renewed = source
        .replace(
            "  action open(id: string)\n    nav = push(nav, path(\"item\", id))\n",
            "  state badged = false\n  action open(id: string)\n    nav = push(nav, path(\"item\", id))\n    badged = true\n",
        )
        .replace(
            tabs,
            r#"        for t in nav.tabs key=t.name
          if badged and t.name == "home"
            button role="tab" id=`tab-${t.name}` aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=tab(t.name) testId=`tab-${t.name}`
              text "home, new"
          else
            button role="tab" id=`tab-${t.name}` aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=tab(t.name) testId=`tab-${t.name}`
              text t.name
"#,
        );
    assert!(source.contains(tabs) && renewed.contains("badged = true"));
    // Alpha's press also gives Other's control Home's id: Home's control, the same element with the same id, still
    // names Alpha's panel
    let clashing = source
        .replace(
            "  action open(id: string)\n    nav = push(nav, path(\"item\", id))\n",
            "  state clash = false\n  action open(id: string)\n    nav = push(nav, path(\"item\", id))\n    clash = true\n",
        )
        .replace(
            "          button role=\"tab\" aria-controls",
            "          button role=\"tab\" id=(clash and t.name == \"other\" ? \"tab-home\" : `tab-${t.name}`) aria-controls",
        );
    assert!(clashing.contains("clash = true") && clashing.contains("id=(clash"));
    let mut p = boot_source(1200., &clashing);
    tap(&mut p, "a");
    assert_eq!(p.host.kernel().find_by_id("tab-home").len(), 2);
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(id(&p, "route-item")), Some(id(&p, "a")))
    );
    let mut p = boot_source(1200., &renewed);
    let key = |p: &Presenter<()>| p.host.kernel().node(id(p, "tab-home")).map(|n| n.key);
    let before = key(&p);
    tap(&mut p, "a");
    assert_ne!(key(&p), before, "Home's control is mounted anew");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(id(&p, "route-item")), Some(id(&p, "a")))
    );
}

#[test]
fn a_press_earns_no_credit_in_another_tab_through_a_root_route_every_tab_shares() {
    // One root route view and its list stand for whichever tab shows, the view's navigationKey following the selected
    // stack's root: Cross opens a visit in Other, which is no visit of the tab Cross's list stood in
    let head = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
    thing "/thing/:id"
component App
  action tab(name: string)
    nav = select(nav, name)
  action open(id: string)
    nav = push(nav, path("item", id))
  action cross
    nav = push(select(nav, "other"), path("thing", "b"))
  action follow(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow width="100%" height="100%" display="flex" flex-direction="column"
"#;
    let stack = r#"for e, i in stack(nav) key=(i == 0 ? "root" : `${e.id}`)
  column navigationKey=`${e.id}` navigationSplit=(i == 0 ? "split" : "") position="absolute" inset=0 testId=(i == 0 ? "route-root" : `route-${e.name}`)
    if i == 0
      list testId="rows"
        button press=open("a") height=48 testId="a"
          text "Alpha"
        button press=cross height=48 testId="cross"
          text "Cross"
    if i > 0
      header
        button id="back" press=back testId=`back-${e.name}`
          text "Back"
        h1 "Item"
"#;
    let indent = |text: &str, depth: usize| -> String {
        text.lines()
            .map(|l| format!("{}{l}\n", " ".repeat(depth)))
            .collect()
    };
    let tablist = |controls: &str| {
        format!(
            "      row role=\"tablist\" height=60 flex-shrink=0\n        for t in nav.tabs key=t.name\n          button role=\"tab\" aria-controls={controls} aria-selected=(nav.tab == t.name) press=tab(t.name) testId=`tab-${{t.name}}`\n            text t.name\n"
        )
    };
    let panel = |id: &str| {
        format!("        column role=\"tabpanel\" id={id} position=\"absolute\" inset=0\n")
    };
    let open = "      column flex=1 min-height=0 position=\"relative\"\n";
    // A tab's routes split by construction, with no ask
    let tabbed = stack.replace(" navigationSplit=(i == 0 ? \"split\" : \"\")", "");
    let stack = |tabs: bool| if tabs { tabbed.as_str() } else { stack };
    let sources = [
        // No panels
        format!("{head}{}", indent(stack(false), 6)),
        // The one panel the selected tab names, `panel-${nav.tab}`
        format!(
            "{head}{open}{}{}{}",
            panel("`panel-${nav.tab}`"),
            indent(stack(true), 10),
            tablist("`panel-${t.name}`")
        ),
        // A panel the selected tab names and a spare one the others name: Cross hands the first to Other's tab
        format!(
            "{head}{open}{}{}{}{}",
            panel("\"active\""),
            indent(stack(true), 10),
            panel("\"spare\""),
            tablist("(nav.tab == t.name ? \"active\" : \"spare\")")
        ),
        // The one panel the selected tab names, with one id that both tabs' controls share, which tells them apart no
        // more than no id
        format!(
            "{head}{open}{}{}{}",
            panel("`panel-${nav.tab}`"),
            indent(stack(true), 10),
            tablist("`panel-${t.name}`").replace("role=\"tab\" ", "role=\"tab\" id=\"tab\" ")
        ),
        // Two panels, with one id that both tabs' controls share
        format!(
            "{head}{open}{}{}{}{}",
            panel("\"active\""),
            indent(stack(true), 10),
            panel("\"spare\""),
            tablist("(nav.tab == t.name ? \"active\" : \"spare\")")
                .replace("role=\"tab\" ", "role=\"tab\" id=\"tab\" ")
        ),
        // The same panels, each named by one control kept by its place, whose id follows the tab it stands for
        format!(
            "{head}{open}{}{}{}{}",
            panel("\"active\""),
            indent(stack(true), 10),
            panel("\"spare\""),
            "      row role=\"tablist\" height=60 flex-shrink=0\n        button role=\"tab\" id=(nav.tab == \"home\" ? \"tab-home\" : \"tab-other\") aria-controls=\"active\" aria-selected=\"true\" press=tab(nav.tab)\n          text nav.tab\n        button role=\"tab\" id=(nav.tab == \"home\" ? \"tab-other\" : \"tab-home\") aria-controls=\"spare\" aria-selected=\"false\" press=tab(nav.tab == \"home\" ? \"other\" : \"home\")\n          text \"Spare\"\n"
        ),
    ];
    for source in &sources {
        let mut p = boot_source(1200., source);
        tap(&mut p, "a");
        assert_eq!(
            (split(&p).base, split(&p).row),
            (Some(id(&p, "route-item")), Some(id(&p, "a"))),
            "{source}"
        );
        let shared = (id(&p, "route-root"), id(&p, "rows"));
        tap(&mut p, "cross");
        assert_eq!((id(&p, "route-root"), id(&p, "rows")), shared, "{source}");
        assert_eq!(
            (split(&p).base, split(&p).row),
            (Some(id(&p, "route-thing")), None),
            "{source}"
        );
    }
}

#[test]
fn a_press_earns_no_credit_for_a_visit_it_returned_to_or_one_a_timer_opened() {
    // B's press goes back to Alpha's detail and stops the split: Alpha's visit is as it was, mounted behind the deeper
    // route or not, so B is credited with nothing
    let retreat = asks()
        .replace(
            "  action back\n",
            "  action retreat\n    nav = back(nav)\n    ask = \"none\"\n  action back\n",
        )
        .replace(
            "button press=open(\"b\") height=48 testId=\"b\"",
            "button press=retreat height=48 testId=\"b\"",
        );
    let only_top = retreat.replace(
        "      for e in stack(nav) key=e.id",
        "      for e in stack(nav).filter(x => x.name == \"home\" or x.id == top(nav).id) key=e.id",
    );
    for (source, alpha) in [(&retreat, true), (&only_top, false)] {
        let mut p = boot_source(1200., source);
        tap(&mut p, "a");
        tap(&mut p, "deeper-item");
        tap(&mut p, "b");
        assert!(p.host.navigation.split.active.is_none());
        act(&mut p, "setAsk", vec![Value::str("split")]);
        let item = id(&p, "route-item");
        // Alpha's credit stands where its route stayed mounted, and went with its view where it did not
        let row = alpha.then(|| id(&p, "a"));
        assert_eq!(
            (split(&p).base, split(&p).row),
            (Some(item), row),
            "Alpha mounted: {alpha}"
        );
    }
    // A timer that falls due as a row is pressed runs first in the press's dispatch, opening an unlinked detail and
    // stopping the split; the row's handler then leaves the router as it was, so the row is credited with nothing
    let timed = asks()
        .replace(
            "  state value",
            "  state armed = false\n  action arm\n    armed = true\n  action fire\n    nav = push(nav, \"/item/timer\")\n    ask = \"none\"\n  action stay\n    nav = nav\n  action moveIt\n    nav = replace(nav, path(\"item\", \"b\"))\n  task timed when armed\n    after(100, fire)\n  state value",
        )
        .replace(
            "testId=\"b\"\n                text \"Beta\"\n",
            "testId=\"b\"\n                text \"Beta\"\n              button press=stay height=48 testId=\"stay\"\n                text \"Stay\"\n              button press=moveIt height=48 testId=\"moves\"\n                text \"Move\"\n",
        );
    // A press that navigates nowhere takes no credit from an earlier commit's navigation
    let mut p = boot_source(1200., &timed);
    tap(&mut p, "a");
    let stay = id(&p, "stay");
    let at = p.host.now() + 1.;
    p.dispatch_press(stay, at, true);
    assert_eq!(split(&p).row, Some(id(&p, "a")));
    let mut p = boot_source(1200., &timed);
    act(&mut p, "arm", vec![]);
    let stay = id(&p, "stay");
    let at = p.host.now() + 200.;
    p.dispatch_press(stay, at, true);
    assert!(p.host.navigation.split.active.is_none());
    act(&mut p, "setAsk", vec![Value::str("split")]);
    assert_eq!(split(&p).row, None);
    assert_eq!(split(&p).base, split(&p).detail);
    // A row whose handler moves the visit the timer opened is that visit's row, at its new location
    let mut p = boot_source(1200., &timed);
    act(&mut p, "arm", vec![]);
    let moves = id(&p, "moves");
    let at = p.host.now() + 200.;
    p.dispatch_press(moves, at, true);
    act(&mut p, "setAsk", vec![Value::str("split")]);
    let item = id(&p, "route-item");
    assert_eq!(
        visit(p.host.runner(), item).map(|v| v.1),
        Some("/item/b".into())
    );
    assert_eq!((split(&p).base, split(&p).row), (Some(item), Some(moves)));
}

#[test]
fn a_press_credit_is_its_own_visit_and_its_own_commits() {
    // One view shows the top detail, keyed by its visit: the app opening a new visit at the same location keeps the
    // view and the location, but the visit is not the one Alpha's press opened, so its credit goes
    let fixed = r#"routes nav
  home "/"
    item "/item/:id"
component App
  state ask = "split"
  action setAsk(a: string)
    ask = a
  action open(id: string)
    nav = push(nav, path("item", id))
  action renew
    nav = push(back(nav), "/item/a")
  action back
    nav = back(nav)
  action follow(url: string)
    nav = push(nav, url)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav).filter(x => x.name == "home") key=e.id
        column navigationKey=`${e.id}` navigationSplit=ask position="absolute" inset=0 testId="route-home"
          list testId="rows"
            button press=open("a") height=48 testId="a"
              text "Alpha"
            button press=renew height=48 testId="renews"
              text "Renew"
      if top(nav).name == "item"
        column navigationKey=`${top(nav).id}` position="absolute" inset=0 testId="route-item"
          header
            button id="back" press=back testId="back-item"
              text "Back"
            h1 "Item"
"#;
    let mut p = boot_source(1200., fixed);
    tap(&mut p, "a");
    let item = id(&p, "route-item");
    let (first, _) = visit(p.host.runner(), item).unwrap();
    assert_eq!(split(&p).row, Some(id(&p, "a")));
    act(&mut p, "setAsk", vec![Value::str("none")]);
    act(&mut p, "renew", vec![]);
    assert_eq!(id(&p, "route-item"), item);
    let (renewed, location) = visit(p.host.runner(), item).unwrap();
    assert_ne!(renewed, first);
    assert_eq!(location, "/item/a");
    assert!(p.host.navigation.split.actions.is_empty());
    act(&mut p, "setAsk", vec![Value::str("split")]);
    assert_eq!((split(&p).base, split(&p).row), (Some(item), None));
    // A row whose own press makes that new visit is its row
    let renews = id(&p, "renews");
    let at = p.host.now() + 1.;
    p.dispatch_press(renews, at, true);
    let (newest, _) = visit(p.host.runner(), item).unwrap();
    assert_ne!(newest, renewed);
    assert_eq!((split(&p).base, split(&p).row), (Some(item), Some(renews)));
}

/// A change action keyed by `input`, which a timer moves, yields before a press that arrives behind it: that press
/// waits, held, and its handler runs in a later advance or another press's dispatch
const HELD: &str = r#"routes nav
  home "/"
    item "/item/:id"
component App
  state ask = "split"
  state armed = false
  state input = 0
  state seen = 0
  action setAsk(a: string)
    ask = a
  action arm
    armed = true
  action tick
    input = input + 1
  action apply
    seen = input
  action open(id: string)
    nav = push(nav, path("item", id))
  action openOff
    nav = push(nav, "/item/o")
    ask = "none"
  action stay
    nav = nav
  action back
    nav = back(nav)
  action follow(url: string)
    nav = push(nav, url)
  task timed when armed
    after(50, tick)
  task sync key=input
    change(apply)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` navigationSplit=(e.name == "home" ? ask : "") position="absolute" inset=0 testId=`route-${e.id}`
          if e.name == "home"
            list testId="rows"
              button press=open("a") height=48 testId="a"
                text "Alpha"
              button press=open("b") height=48 testId="b"
                text "Beta"
              button press=openOff height=48 testId="off"
                text "Off"
              button press=stay height=48 testId="stay" aria-current="page"
                text "Stay"
          if e.name == "item"
            header
              button id="back" press=back testId="back-item"
                text "Back"
              h1 "Item"
"#;

/// A press of `row` that waits behind the change action a timer makes due, still waiting for its own commit
fn held_press(p: &mut Presenter<()>, row: &str) -> f64 {
    act(p, "arm", vec![]);
    let at = p.host.now() + 100.;
    let row = id(p, row);
    p.dispatch_press(row, at, true);
    assert_eq!(
        p.host.navigation.split.presses.len(),
        1,
        "the press waits for its own commit"
    );
    at
}

/// Advance, committing each turn as the display loop does, until the change action the press waits behind has run and
/// its receipt is applied: the press still waits, for the next input's dispatch to run it
fn apply_change(p: &mut Presenter<()>, at: f64) {
    for _ in 0..4 {
        if p.host.runner().slot("seen") == Some(&Value::Number(1.)) {
            break;
        }
        assert_eq!(p.host.advance(at), None);
        assert_eq!(p.after_commit(), None);
    }
    assert_eq!(p.host.runner().slot("seen"), Some(&Value::Number(1.)));
    assert_eq!(
        p.host.navigation.split.presses.len(),
        1,
        "the press still waits"
    );
}

/// Advance from `at`, committing each turn as the display loop does, until every press has had its own commit
fn settle(p: &mut Presenter<()>, at: f64) {
    for step in 1..8 {
        if p.host.navigation.split.presses.is_empty() {
            return;
        }
        assert_eq!(p.host.advance(at + step as f64), None);
        assert_eq!(p.after_commit(), None);
    }
    panic!("a press still waits for its own commit");
}

#[test]
fn a_held_press_earns_the_credit_its_own_handler_commits() {
    // Alone: its handler runs in a later advance, and its row is the detail's
    let mut p = boot_source(1200., HELD);
    let at = held_press(&mut p, "a");
    settle(&mut p, at);
    assert_eq!(split(&p).row, Some(id(&p, "a")));
    // A staged renewal of its row while it waits, as the host's commit processing hands one when the runner recycles
    // rows (`State::renewed`): the detail is still the press's, so the app's current row is not marked, and no row is
    let mut p = boot_source(1200., HELD);
    let at = held_press(&mut p, "a");
    let key = p.host.kernel().node(id(&p, "a")).unwrap().key;
    let mut state = std::mem::take(&mut p.host.navigation.split);
    state.renewed(p.host.kernel(), &[key]);
    p.host.navigation.split = state;
    settle(&mut p, at);
    assert_eq!((split(&p).base, split(&p).row), (split(&p).detail, None));
    // Once the change action's receipt is applied, another press runs it in its own dispatch, then itself: each earns
    // what its own handler committed, at that dispatch
    for (first, second, credited) in [
        ("stay", "b", "b"),
        ("a", "stay", "a"),
        ("off", "stay", "off"),
    ] {
        let mut p = boot_source(1200., HELD);
        let at = held_press(&mut p, first);
        apply_change(&mut p, at);
        let second = id(&p, second);
        p.dispatch_press(second, at + 1., true);
        assert!(p.host.navigation.split.presses.is_empty(), "both ran");
        let credited = Some(id(&p, credited));
        if p.host.navigation.split.active.is_some() {
            assert_eq!(split(&p).row, credited, "{first} then {second:?}");
            act(&mut p, "setAsk", vec![Value::str("none")]);
        }
        // The stack stops asking and asks again: the credit stands
        act(&mut p, "setAsk", vec![Value::str("split")]);
        assert_eq!(
            split(&p).row,
            credited,
            "{first} then {second:?}, asked again"
        );
    }
    // Both open a detail, in B's dispatch: the later is the base's row, and Back finds the earlier's
    let mut p = boot_source(1200., HELD);
    let at = held_press(&mut p, "a");
    apply_change(&mut p, at);
    let b = id(&p, "b");
    p.dispatch_press(b, at + 1., true);
    assert!(p.host.navigation.split.presses.is_empty(), "both ran");
    assert_eq!(split(&p).row, Some(id(&p, "b")));
    let back = back_in(&p, split(&p).base.unwrap());
    let now = p.host.now() + 1.;
    p.dispatch_press(back, now, true);
    assert_eq!(split(&p).row, Some(id(&p, "a")));
}

#[test]
fn a_list_route_the_app_keys_credits_a_press_that_selects_its_own_tab_again() {
    // The list route's navigationKey is the app's own, no visit id, so the press takes the selected tab; Alpha selects
    // that tab again and opens its detail there
    let source = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
component App
  action open(id: string)
    nav = push(select(nav, "home"), path("item", id))
  action follow(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=(top(nav).name == "home" ? "home-list" : `${top(nav).id}`) navigationBack="back" navigate=follow width="100%" height="100%"
      for e, i in stack(nav) key=(i == 0 ? "home-list" : `${e.id}`)
        column navigationKey=(i == 0 ? "home-list" : `${e.id}`) navigationSplit=(i == 0 ? "split" : "") position="absolute" inset=0 testId=(i == 0 ? "route-root" : `route-${e.name}`)
          if i == 0
            list testId="rows"
              button press=open("a") height=48 testId="a"
                text "Alpha"
          if i > 0
            header
              button id="back" press=back testId=`back-${e.name}`
                text "Back"
              h1 "Item"
"#;
    let mut p = boot_source(1200., source);
    tap(&mut p, "a");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(id(&p, "route-item")), Some(id(&p, "a")))
    );
}

#[test]
fn a_press_in_a_tab_panel_is_credited_in_that_panel_whatever_the_router_selects() {
    // Home's list route is the app's own key, and the navigation root keeps showing Home's panel while Away selects
    // Other in the router: Bravo's press is made in Home's panel, and its detail opens there, so it is credited
    // whether Alpha's detail shows, stays mounted above the list parked on it, or was never opened
    let source = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
component App
  state shown = "home-list"
  state parked = false
  action open(id: string)
    nav = push(select(nav, "home"), path("item", id))
    parked = false
  action sync
    shown = (nav.tab == "home" ? (top(nav).name == "home" ? "home-list" : `${top(nav).id}`) : shown)
  task follow key=`${top(nav).id}`
    change(sync)
  action away
    nav = select(nav, "other")
  action tab(name: string)
    nav = select(nav, name)
  action follow(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=(parked ? "home-list" : shown) navigationBack="back" navigate=follow width="100%" height="100%" display="flex" flex-direction="column"
      column flex=1 min-height=0 position="relative"
        for t in nav.tabs key=t.name
          column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
            for e, i in t.stack key=e.id
              column navigationKey=(i == 0 && t.name == "home" ? "home-list" : `${e.id}`) position="absolute" inset=0 testId=`route-${e.name}`
                if e.name == "home"
                  list testId="rows"
                    button press=open("a") height=48 testId="a"
                      text "Alpha"
                    button press=open("b") height=48 testId="b"
                      text "Bravo"
                    button press=away height=48 testId="away"
                      text "Away"
                if e.name == "item"
                  header
                    button id="back" press=back testId=`back-${e.id}`
                      text "Back"
                    h1 "Item"
      row role="tablist" height=60 flex-shrink=0 testId="tabs"
        for t in nav.tabs key=t.name
          button role="tab" aria-controls=`panel-${t.name}` aria-selected=(t.name == "home") press=tab(t.name) testId=`tab-${t.name}`
            text t.name
"#;
    for (parked, alpha) in [(false, true), (true, true), (true, false)] {
        let source = if parked {
            source.replace(
                "  action away\n    nav = select(nav, \"other\")\n",
                "  action away\n    nav = select(nav, \"other\")\n    parked = true\n",
            )
        } else {
            source.to_owned()
        };
        let mut p = boot_source(1200., &source);
        // Each tap, then the advances that run the app's change task, which moves `shown` to Home's top
        let press = |p: &mut Presenter<()>, name: &str| {
            tap(p, name);
            for _ in 0..4 {
                let at = p.host.now() + 1.;
                assert_eq!(p.host.advance(at), None);
                assert_eq!(p.after_commit(), None);
            }
        };
        if alpha {
            press(&mut p, "a");
            assert_eq!(split(&p).row, Some(id(&p, "a")));
        }
        press(&mut p, "away");
        assert_eq!(p.host.runner().selected_tab().as_deref(), Some("other"));
        press(&mut p, "b");
        assert_eq!(
            split(&p).row,
            Some(id(&p, "b")),
            "parked: {parked}, Alpha first: {alpha}"
        );
    }
}

#[test]
fn a_list_route_keyed_by_a_number_in_one_stack_takes_the_selected_tab() {
    // One stack shows the tab the router selects: a root route the app keys "0", the number of Home's first visit, is
    // no visit to read, so Alpha's press made while Other shows is Other's, and its detail opens there
    let source = r#"routes nav
  tab home "/"
    item "/item/:id"
  tab other "/other"
    thing "/thing/:id"
component App
  action open(id: string)
    nav = push(nav, path("item", id))
  action cross
    nav = push(select(nav, "other"), path("thing", "b"))
  action follow(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=(top(nav).name == "home" or top(nav).name == "other" ? "0" : `${top(nav).id}`) navigationBack="back" navigate=follow width="100%" height="100%"
      for e, i in stack(nav) key=(i == 0 ? "root" : `${e.id}`)
        column navigationKey=(i == 0 ? "0" : `${e.id}`) navigationSplit=(i == 0 ? "split" : "") position="absolute" inset=0 testId=(i == 0 ? "route-root" : `route-${e.name}`)
          if i == 0
            list testId="rows"
              button press=open("a") height=48 testId="a"
                text "Alpha"
              button press=cross height=48 testId="cross"
                text "Cross"
          if i > 0
            header
              button id="back" press=back testId=`back-${e.name}`
                text "Back"
              h1 "Item"
"#;
    let mut p = boot_source(1200., source);
    tap(&mut p, "cross");
    assert_eq!(p.host.runner().selected_tab().as_deref(), Some("other"));
    tap(&mut p, "a");
    assert_eq!(
        (split(&p).base, split(&p).row),
        (Some(id(&p, "route-item")), Some(id(&p, "a")))
    );
}
