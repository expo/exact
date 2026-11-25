//! `bake-geometry-feedback` (LLP 1123 D6): the bake refuses a path from a geometry fact to what
//! moves the geometry it reads, directly, through a branch, and through derives, actions and
//! state, and leaves a path to content, or through an action the person runs, alone.

const SHAPES: &str = "shape Rect
  x: number
  y: number
  width: number
  height: number
shape G
  available: bool
  frame: option<Rect>
";

/// A route reading `exactGeometry("hero")` as `g`, with `decls` and an inline title over `body`,
/// each line of either at its depth.
fn route(decls: &str, body: &str) -> String {
    let indent =
        |text: &str, by: &str| -> String { text.lines().map(|l| format!("{by}{l}\n")).collect() };
    format!(
        "{SHAPES}routes nav\n  home \"/\"\n  detail \"/detail\"\ncomponent App\n  resource g = exactGeometry(\"hero\") as shape G\n  derive wide = match g.frame {{ case some(f) => f.width > 400, case none => false }}\n{}  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header\n            text \"Inbox\" role=\"heading\" aria-level=2\n          column id=\"hero\"\n{}",
        indent(decls, "  "),
        indent(body, "            "),
    )
}

/// The first refusal's message, or none when the plan compiles.
fn fenced(src: &str) -> Option<String> {
    let e = contract::compile(src).err()?;
    assert_eq!(e.id, "bake-geometry-feedback", "{}", e.message);
    Some(e.message)
}

/// Assert that `src` is refused with a message that starts with `start`.
fn refused(src: &str, start: &str) {
    let message = fenced(src).unwrap_or_else(|| panic!("compiles: {start}"));
    assert!(message.starts_with(start), "{message}");
}

#[test]
fn a_fact_that_decides_content_compiles() {
    // A width, a text and a node under a branch follow the fact, as does a popover's invoker,
    // which presents only when pressed. A press focuses a field or pushes a route by the fact,
    // as a UIKit app chooses a popover or a sheet by its width, and a scroll hides the bar by
    // it: the person runs each once per event, so none feeds back
    let decls = "state bar = \"auto\"\naction edit\n  if wide\n    focus(\"name\")\n  else\n    nav = push(nav, \"detail\")\naction scrolled(left: number, top: number)\n  if wide\n    bar = \"none\"";
    let body = "text `${wide}` width=(wide ? 200 : 100)\nif wide\n  text \"Narrow\"\n  button popovertarget=\"tips\"\n    text \"Tips\"\ninput aria-label=\"Name\" id=\"name\"\nbutton press=edit\n  text \"Edit\"\nlist scroll=scrolled height=100\n  text \"row\"\ncolumn popover=\"auto\" id=\"tips\"\n  text \"Tip\"";
    let src = route(decls, body).replace("inset=0\n", "inset=0 navigationBar=bar\n");
    assert_eq!(fenced(&src), None);
}

#[test]
fn a_fact_that_sets_the_routes_bar_is_refused_naming_what_holds_it() {
    let bar = route("", "").replace(
        "inset=0\n",
        "inset=0 navigationBar=(wide ? \"none\" : \"auto\")\n",
    );
    refused(
        &bar,
        "`navigationBar` follows `wide`, which holds the geometry fact `g`, so a route's bar",
    );
    // The bar's title under a branch on the fact, and a branch before the header, which
    // decides whether the header is the route's first child
    let title = route("", "").replace(
        "          header\n            text \"Inbox\"",
        "          header\n            if wide\n              text \"Inbox\"",
    );
    refused(&title, "`text` is under a branch on `wide`");
    let before = route("", "").replace(
        "          header\n",
        "          if wide\n            text \"Wide\"\n          header\n",
    );
    refused(
        &before,
        "`if` branches on `wide`, which holds the geometry fact `g`, so the route's bar",
    );
}

#[test]
fn a_layout_event_carries_the_fact_through_an_action_and_state() {
    // The view reads the fact, so `resize` runs on it: its action's focus and the state it
    // writes, which a scroller's offset reads, are both refused
    let decls = "state top = 0\naction fit(w: number, h: number, r: DOMRectReadOnly)\n  top = h\n  focus(\"name\")";
    let body = "text `${wide}` resize=fit\ninput aria-label=\"Name\" id=\"name\"\nlist scrollTop=top height=100\n  text \"row\"";
    let message = fenced(&route(decls, body)).expect("refused");
    assert!(
        message.contains("`focus(…)` is in action `fit`, which runs on `wide`"),
        "{message}"
    );
    assert!(
        message.contains("`resize` runs `fit` at every layout"),
        "{message}"
    );
    let scrolled = route(decls.replace("\n  focus(\"name\")", "").as_str(), body);
    refused(
        &scrolled,
        "`scrollTop` follows `top`, which holds the geometry fact `g` through action `fit`",
    );
}

#[test]
fn an_action_that_runs_without_the_person_carries_the_fact() {
    // A task gated on the fact pushes a route or reveals a row; a sequence's `ended=` action,
    // which runs without the person whatever its gate, pushes one by the fact; and an image's
    // `load`, which fires as its picture arrives, runs on it
    let task =
        "action open\n  nav = push(nav, \"detail\")\ntask opening when wide\n  after(1, open)";
    refused(
        &route(task, ""),
        "`nav = …` is in action `open`, which runs on `wide`",
    );
    let reveal =
        "action open\n  scrollIntoView(\"hero\")\ntask opening when wide\n  after(1, open)";
    refused(
        &route(reveal, ""),
        "`scrollIntoView(…)` is in action `open`",
    );
    let sequence = "state on = true\naction open\n  if wide\n    nav = push(nav, \"detail\")\nsequence chime when on\n  play([], bpm=120, ended=open)";
    refused(
        &route(sequence, ""),
        "`nav = …` is under a branch on `wide`",
    );
    let load = "state bar = \"auto\"\naction hide\n  bar = \"none\"";
    let src = route(load, "if wide\n  image \"a.png\" load=hide")
        .replace("inset=0\n", "inset=0 navigationBar=bar\n");
    refused(
        &src,
        "`navigationBar` follows `bar`, which holds the geometry fact `g` through action `hide`",
    );
}

#[test]
fn a_childs_fact_is_named_as_written_and_its_props_carry_it() {
    // The inliner lifts a child's resource as `g#1`; the refusal names it as the child wrote it
    let own = format!(
        "{SHAPES}component App\n  view\n    column\n      Card()\ncomponent Card\n  resource g = exactGeometry(\"card\") as shape G\n  view\n    column id=\"card\"\n      input aria-label=\"Name\" autofocus=g.available\n"
    );
    refused(&own, "`autofocus` follows the geometry fact `g`, so focus");
    // A child's task reads the fact through a prop its lifted action captures
    let prop = format!(
        "{SHAPES}component App\n  resource g = exactGeometry(\"hero\") as shape G\n  view\n    column id=\"hero\"\n      Pager(on=g.available)\ncomponent Pager\n  props\n    on: bool\n  action tick\n    if on\n      focus(\"name\")\n  task ticking mount\n    every(1000, tick)\n  view\n    input aria-label=\"Name\" id=\"name\"\n"
    );
    refused(
        &prop,
        "`focus(…)` is under a branch on the geometry fact `g`",
    );
}

#[test]
fn a_row_keyed_by_the_fact_is_made_again_with_it() {
    // Each route's row is keyed by the fact, so the route is made again when it moves
    let keyed = route("", "").replace("key=e.id", "key=`${e.id}:${wide}`");
    refused(&keyed, "`column` is under a branch on `wide`, which holds the geometry fact `g`, so a route's bar or navigation");
}

#[test]
fn a_value_under_a_branch_or_a_display_that_follows_the_fact_is_read() {
    // Under a branch on the fact a scroller's offset still follows it; a `display` that follows
    // it decides whether an orientation lock holds, as a branch would
    let under = route(
        "",
        "if g.available\n  list scrollTop=(wide ? 100 : 0) height=100\n    text \"row\"",
    );
    refused(&under, "`scrollTop` follows `wide`");
    let shown = route(
        "",
        "column orientationlock=\"landscape\" display=(wide ? \"flex\" : \"none\")",
    );
    refused(&shown, "`column` is under a branch on `wide`, which holds the geometry fact `g`, so the window's fitting or orientation");
}

#[test]
fn a_name_an_action_fed_is_traced_to_it_through_a_derive() {
    // `more` pages the list at every layout, and the bar's title reads a derive of what it wrote
    let decls = "state page = 1\nderive label = `${page} pages`\naction more\n  page = page + 1";
    let src = route(
        decls,
        "text `${wide}`\nlist virtualized=true reachend=more height=100\n  for i in range(10) key=i\n    text \"row\"",
    )
    .replace(
        "text \"Inbox\" role=\"heading\"",
        "text label role=\"heading\"",
    );
    let message = fenced(&src).expect("refused");
    assert!(
        message.starts_with(
            "`text` follows `label`, which holds the geometry fact `g` through action `more`"
        ),
        "{message}"
    );
    assert!(
        message.contains("`reachend` runs `more` at every layout"),
        "{message}"
    );
}

#[test]
fn an_iframes_capture_runs_without_the_person() {
    // A capture answers a command, not the person
    let decls = "action shot(e: FrameCapture)\n  if wide\n    focus(\"name\")";
    let body =
        "iframe src=\"https://example.com\" capture=shot\ninput aria-label=\"Name\" id=\"name\"";
    refused(
        &route(decls, body),
        "`focus(…)` is under a branch on `wide`",
    );
}

#[test]
fn a_fact_that_shapes_a_popover_or_fits_the_window_is_refused() {
    // A popover's arrows move the popover, and aspect fit the window's fitting (LLP 1123 D7, D16)
    let popover = route("", "").replace(
        "inset=0\n",
        "inset=0 navigationPresentation=\"popover\" navigationPopoverAnchor=\"hero\" navigationPopoverArrows=(wide ? \"up\" : \"down\")\n",
    );
    refused(&popover, "`navigationPopoverArrows` follows `wide`");
    let fit = format!(
        "{SHAPES}component App\n  resource g = exactGeometry(\"hero\") as shape G\n  derive wide = match g.frame {{ case some(f) => f.width > 400, case none => false }}\n  view\n    column viewport-fit=\"cover\" -exact-safe-area-aspect-fit=(wide ? \"16 / 9\" : \"none\") width=\"100%\" height=\"100%\"\n      column id=\"hero\"\n"
    );
    refused(&fit, "`-exact-safe-area-aspect-fit` follows `wide`");
}
