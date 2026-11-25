//! LLP 1116.006 D2: `navigationSplit="placeholder"` marks a box in a stack's
//! root route as the detail pane's while nothing is selected. Only a wide
//! window's split shows it (the web shell, over its `display`); every other
//! host lays it out as no box, so a phone's list never draws it. On a route
//! it would hide the route, so it is refused there.
//!
//! LLP 1115.002 D4/stage 6d: chevrons are paint. The host reads the shared
//! list/press rule; the compiler must not invent an author opt-out.

use crate::source::{boxed, NoData};
use exact_kernel::{Display, Kernel, PropId};
use exact_plan::{BindingKind, Opcode, StrId};
use exact_runner::Runner;

/// A stack of two routes; `route` and `inner` are attributes of each route
/// and of a column inside it beside the route's text.
fn stack(route: &str, inner: &str) -> String {
    format!(
        "routes nav\n  home \"/\"\n    item \"/item\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` testId=`route-${{e.name}}` position=\"absolute\" inset=0 {route}\n          text e.name\n          column testId=`own-${{e.name}}` {inner}\n            text \"Pick one\"\n"
    )
}

#[test]
fn the_placeholder_reaches_the_host_and_is_no_box_where_no_split_shows_it() {
    let plan = contract::bake(
        contract::compile(&stack("", "navigationSplit=\"placeholder\" gap=8")).unwrap(),
        boxed(NoData),
    )
    .unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let own = k.node_by_key(k.find_by_test_id("own-home")[0]).unwrap();
    assert_eq!(own.props.str(PropId::NavigationSplit), Some("placeholder"));
    assert_eq!(own.style.display, Display::None);
    assert_eq!((own.frame.width, own.frame.height), (0.0, 0.0));
    // An unmarked box keeps its `column`.
    let plan = contract::bake(contract::compile(&stack("", "")).unwrap(), boxed(NoData)).unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    assert_ne!(
        k.node_by_key(k.find_by_test_id("own-home")[0])
            .unwrap()
            .style
            .display,
        Display::None
    );
}

#[test]
fn on_a_route_the_placeholder_is_refused() {
    let error = contract::compile(&stack("navigationSplit=\"placeholder\"", "")).unwrap_err();
    assert_eq!(error.id, "lower-attr-value", "{error}");
    assert!(
        error
            .message
            .contains("on a route, or on the navigation root, it would hide it"),
        "{error}"
    );
    // Each placeholder refusal ends with the whole rule, so a repair that follows it compiles
    assert!(
        error.message.contains("A placeholder is a plain box"),
        "{error}"
    );
    // Any other word is refused, naming the values.
    let error = contract::compile(&stack("", "navigationSplit=\"empty\"")).unwrap_err();
    assert!(
        error
            .message
            .contains("and a plain box that is its own child \"placeholder\""),
        "{error}"
    );
}

#[test]
fn the_likely_guesses_are_refused_with_why_and_split_asks() {
    // LLP 1127 D1: "auto" is each platform's default, and the value says how the stack splits, not a switch.
    for (attribute, says) in [
        (
            "navigationSplit=\"auto\"",
            "leave the attribute out for each host's own default",
        ),
        // A template with nothing computed in it says what its text says.
        (
            "navigationSplit=`auto`",
            "leave the attribute out for each host's own default",
        ),
        ("navigationSplit=true", "is no switch"),
        // An empty value says nothing; leave the attribute out instead.
        (
            "navigationSplit=\"\"",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
        (
            "navigationSplit=\"columns\"",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
        // A width is a whole number of px, as every host reads it.
        (
            "navigationSplit=\"0280\"",
            "in digits with no leading zero",
        ),
        (
            "navigationSplit=\"0x100\"",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
        (
            "navigationSplit=280.5",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
        // A template the compiler folds to a constant is that constant, as every attribute's is.
        (
            "navigationSplit=`${180 + 100.5}`",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
        // So is one of literals, a switch's word included.
        (
            "navigationSplit=`${true}`",
            "takes \"split\", a width (a whole number of px, 1 to 999999, in digits with no leading zero) or \"none\"",
        ),
    ] {
        let error = contract::compile(&stack(attribute, "")).unwrap_err();
        assert_eq!(error.id, "lower-attr-value", "{error}");
        assert!(error.message.contains(says), "{attribute}: {error}");
    }
    // With no list, "split" and a placeholder box in the root route each ask; unsaid, nothing splits.
    assert_eq!(asked("navigationSplit=\"split\"", ""), Some(true));
    assert_eq!(asked("", "navigationSplit=\"placeholder\""), Some(true));
    assert_eq!(asked("", "navigationSplit=`placeholder`"), Some(true));
    assert_eq!(asked("navigationSplit=`${200 + 100}`", ""), Some(true));
    // A computed value is read at run time: a word the hosts know asks, and any other says nothing.
    assert_eq!(
        asked(
            "navigationSplit=(e.name == \"home\" ? \"300\" : \"none\")",
            ""
        ),
        Some(true)
    );
    assert_eq!(
        asked(
            "navigationSplit=(e.name == \"home\" ? \"auto\" : \"none\")",
            ""
        ),
        None
    );
    assert_eq!(asked("", ""), None);
}

#[test]
fn a_placeholder_in_a_route_the_compiler_cannot_place_is_its_own_child() {
    // A keyed stack outside every root the compiler places, whose Back a host installs (as the painted tests' does), is
    // still a stack a host reads (LLP 1127 D1): a route's own placeholder there compiles, and one deeper is still refused.
    let wrap = |src: String| {
        let (head, body) = src.split_once("  view\n").unwrap();
        let body: String = body.lines().map(|line| format!("  {line}\n")).collect();
        format!("{head}  view\n    column\n      text \"Wrapper\"\n{body}")
            .replace(" navigationBack=\"back\"", "")
    };
    contract::compile(&wrap(stack("", "navigationSplit=\"placeholder\""))).unwrap();
    let deeper = wrap(stack("", "")).replace(
        "            text \"Pick one\"\n",
        "            column navigationSplit=\"placeholder\"\n              text \"Pick one\"\n",
    );
    assert_eq!(
        contract::compile(&deeper).unwrap_err().id,
        "lower-navigation-split"
    );
    // And one a `for` repeats there is refused for that
    let repeated = wrap(stack("", "")).replace(
        "\n            text e.name\n",
        "\n            text e.name\n            for n in [1] key=n\n              column navigationSplit=\"placeholder\"\n                text \"Pick\"\n",
    );
    let error = contract::compile(&repeated).unwrap_err();
    assert!(
        error.message.contains("a `for` inside the route repeats"),
        "{error}"
    );
    // Each placeholder refusal ends with the whole rule, so a repair that follows it compiles
    assert!(
        error.message.contains("A placeholder is a plain box"),
        "{error}"
    );
}

#[test]
fn a_placeholder_a_components_typed_prop_names_is_the_placeholder() {
    // A literal a typed prop passes keeps its type's wrapper, and is still the literal it is (LLP 1127 D1): a placeholder,
    // and a `none` popover, which makes none.
    let source = "routes nav\n  home \"/\"\n    item \"/item\"\ncomponent App\n  view\n    main navigationKey=`${top(nav).id}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}` testId=`route-${e.name}` position=\"absolute\" inset=0\n          text e.name\n          Pane(mode=\"placeholder\", layer=none)\ncomponent Pane\n  props\n    mode: enum<\"placeholder\", \"none\">\n    layer: option<string>\n  view\n    column navigationSplit=mode popover=layer\n      text \"Pick one\"\n";
    let plan = contract::bake(contract::compile(source).unwrap(), boxed(NoData)).unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let home = k
        .node_by_key(k.find_by_test_id("route-home")[0])
        .unwrap()
        .id;
    assert_eq!(
        r.stack_split(home, None).map(|split| split.asked),
        Some(true)
    );
}

/// Whether the stack of `stack(route, inner)` splits because its root route asks; `None` where it pushes.
fn asked(route: &str, inner: &str) -> Option<bool> {
    let plan = contract::bake(
        contract::compile(&stack(route, inner)).unwrap(),
        boxed(NoData),
    )
    .unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    let home = k
        .node_by_key(k.find_by_test_id("route-home")[0])
        .unwrap()
        .id;
    r.stack_split(home, None).map(|split| split.asked)
}

#[test]
fn a_button_in_the_placeholder_is_no_zero_size_button_to_the_bake() {
    // Laid out as no box at the lint's phone width, as the platform intends:
    // not the hidden button the bake refuses (it is pressed in the pane).
    let src = stack("", "navigationSplit=\"placeholder\"").replace(
        "            text \"Pick one\"\n",
        "            text \"Pick one\"\n            button \"Add\" press=add testId=\"add\"\n",
    );
    let src = src.replace(
        "  view\n",
        "  action add\n    nav = push(nav, \"/item\")\n  view\n",
    );
    let plan = contract::compile(&src).unwrap();
    contract::check(&plan).unwrap();
    contract::bake(plan, boxed(NoData)).unwrap();
}

/// A stack whose root route holds `rows`, with actions that select and that push.
fn rows(route: &str, rows: &str) -> String {
    let rows: String = rows.lines().map(|l| format!("          {l}\n")).collect();
    format!(
        "routes nav\n  home \"/\"\n    item \"/item\"\ncomponent App\n  state picked = \"\"\n  action pick(id: string)\n    picked = id\n  action open(id: string)\n    nav = push(nav, \"/item\")\n  action choose(id: string)\n    picked = id\n    open(id)\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` testId=`route-${{e.name}}` position=\"absolute\" inset=0 {route}\n{rows}"
    )
}

/// What the home route says for `navigationSplit` once lowered.
fn split_of(src: &str) -> Option<String> {
    let plan = contract::bake(
        contract::compile(src).unwrap_or_else(|e| panic!("{e}\n{src}")),
        boxed(NoData),
    )
    .unwrap();
    let r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id("route-home")[0])
        .unwrap()
        .props
        .str(PropId::NavigationSplit)
        .map(str::to_owned)
}

#[test]
fn decorative_chevrons_do_not_write_a_navigation_split_opt_out() {
    // event-rsvp (round 9): invitations that select into a panel, a chevron
    // on each, and a Create button elsewhere that does push.
    let decorative = [
        "column press=pick(\"a\") testId=\"row\"\n  text \"A\"\n  image \"symbol:forward-chevron\"",
        "button press=pick(\"a\")\n  text \"A\"\n  image \"symbol:forward-chevron\"",
        // Another spelling of the chevron is the same chevron.
        "button press=pick(\"a\")\n  text \"A\"\n  image \"symbol:chevron-right\"",
        "button press=pick(\"a\")\n  text \"A\"\n  image \"symbol:sf/chevron.forward\"",
        // A row nested in a box, under an `if`, beside a button that does push.
        "button press=open(\"x\")\n  text \"Create\"\ncolumn\n  if true\n    row press=pick(\"a\")\n      text \"A\"\n      image \"symbol:forward-chevron\"",
    ];
    for body in decorative {
        assert_eq!(split_of(&rows("", body)), None, "{body}");
    }
}

#[test]
fn a_link_without_a_chevron_beside_decoration_keeps_only_the_authors_split_choice() {
    let body = "list\n  link href=\"/item\"\n    text \"Item\"\n  button press=pick(\"a\")\n    text \"Decoration\"\n    image \"symbol:forward-chevron\"";
    assert_eq!(split_of(&rows("", body)), None);
    for (attribute, expected) in [
        ("navigationSplit=\"none\"", "none"),
        ("navigationSplit=280", "280"),
    ] {
        assert_eq!(split_of(&rows(attribute, body)).as_deref(), Some(expected));
    }
}

#[test]
fn a_row_that_navigates_keeps_the_hosts_split() {
    for body in [
        "button press=open(\"a\")\n  text \"A\"\n  image \"symbol:forward-chevron\"",
        // An action that selects and then pushes (through a call) pushes.
        "button press=choose(\"a\")\n  text \"A\"\n  image \"symbol:forward-chevron\"",
        "link href=\"/item\"\n  text \"A\"\n  image \"symbol:forward-chevron\"",
        // One row that pushes among rows that do not: the list's rows push.
        "button press=pick(\"a\")\n  text \"A\"\n  image \"symbol:forward-chevron\"\nbutton press=open(\"b\")\n  text \"B\"\n  image \"symbol:forward-chevron\"",
        // No chevron, or a chevron no pressable holds, is no list the rule reads.
        "button press=pick(\"a\")\n  text \"A\"",
        "row\n  text \"A\"\n  image \"symbol:forward-chevron\"",
    ] {
        assert_eq!(split_of(&rows("", body)), None, "{body}");
    }
    // What the author said wins, a width included.
    let body = "button press=pick(\"a\")\n  text \"A\"\n  image \"symbol:forward-chevron\"";
    assert_eq!(
        split_of(&rows("navigationSplit=\"280\"", body)).as_deref(),
        Some("280")
    );
}

#[test]
fn a_tab_split_is_literal_and_an_untabbed_stack_can_still_bind_it() {
    let tab = include_str!("../../../../scripts/fixtures/split-tabs.contract");
    for value in [
        "",
        r#"navigationSplit="none""#,
        r#"navigationSplit="split""#,
    ] {
        contract::compile(&tab.replace(r#"navigationSplit="split""#, value)).unwrap();
    }
    for value in [
        r#"(e.name == "library" ? "split" : "none")"#,
        "280",
        r#""280""#,
    ] {
        let error = contract::compile(&tab.replace(
            r#"navigationSplit="split""#,
            &format!("navigationSplit={value}"),
        ))
        .unwrap_err();
        assert_eq!(error.id, "lower-tab-split", "{error}");
        assert!(
            error.message.contains("by construction")
                && error.message.contains("two tabs or two routes")
        );
    }
    // A placeholder written on the tab's route is told where the box goes
    let error = contract::compile(&tab.replace(
        r#"navigationSplit="split""#,
        r#"navigationSplit="placeholder""#,
    ))
    .unwrap_err();
    assert_eq!(error.id, "lower-tab-split", "{error}");
    assert!(
        error.message.contains("A placeholder is a plain box"),
        "{error}"
    );
    // A literal a typed prop passes is still that literal, and a computed one is still refused
    let (head, tail) = tab.split_once("              else\n").unwrap();
    let tail = &tail[tail.find("      row role=\"tablist\"").unwrap()..];
    let typed = |mode: &str| {
        format!("{head}              else\n                Pane(route=`${{e.id}}`, mode={mode})\n{tail}component Pane\n  props\n    route: string\n    mode: enum<\"split\", \"none\">\n  view\n    column navigationKey=route navigationSplit=mode position=\"absolute\" inset=0\n      header\n        text \"Pane\" role=\"heading\" aria-level=1\n")
    };
    contract::compile(&typed("\"split\"")).unwrap();
    // And so is one a second component forwards into a template with text of its own
    let forwarded = typed("\"t\"")
        .replace("Pane(route=", "Outer(route=")
        .replace("mode: enum<\"split\", \"none\">", "mode: enum<\"t\", \"ce\">")
        .replace("navigationSplit=mode", "navigationSplit=`spli${mode}`")
        + "component Outer\n  props\n    route: string\n    mode: enum<\"t\", \"ce\">\n  view\n    Pane(route=route, mode=mode)\n";
    // Each is the literal the runner's scan of a tab's ask reads, not a text the app builds (LLP 1127 D19)
    let plan = contract::compile(&forwarded).unwrap();
    let splits = plan
        .bindings
        .iter()
        .filter(|b| b.kind == BindingKind::Prop && b.id == PropId::NavigationSplit as u16)
        .map(|b| match plan.read_code(plan.code(b.expr)).0 {
            [op, a, b, c, d, ret] if *op == Opcode::Str as u8 && *ret == Opcode::Return as u8 => {
                Some(
                    plan.str(StrId(u32::from_le_bytes([*a, *b, *c, *d])))
                        .to_owned(),
                )
            }
            _ => None,
        })
        .collect::<Vec<_>>();
    assert!(
        !splits.is_empty() && splits.iter().all(|s| s.as_deref() == Some("split")),
        "{splits:?}"
    );
    let error =
        contract::compile(&typed("(e.name == \"item\" ? \"split\" : \"none\")")).unwrap_err();
    assert_eq!(error.id, "lower-tab-split", "{error}");
    let bound = r#"navigationSplit=(e.name == "home" ? "300" : "none")"#;
    assert_eq!(split_of(&stack(bound, "")).as_deref(), Some("300"));
}
