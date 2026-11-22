//! LLP 1115.000 D5.2: no navigation bar is something a route says.
//! `navigationBar` is a route's prop, `"none"` or `"auto"`; a literal with
//! any other word is refused, and so is the attribute anywhere but on a
//! route, where no host would read it.

use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Runner};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// A stack of two routes; `root`, `route` and `inner` are attributes of the
/// navigation root, of each route and of a box inside each.
fn stack(root: &str, route: &str, inner: &str) -> String {
    format!(
        "routes nav\n  home \"/\"\n    viewer \"/viewer\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}` {root}\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` testId=`route-${{e.name}}` position=\"absolute\" inset=0 {route}\n          column {inner}\n            text e.name\n"
    )
}

#[test]
fn a_route_says_it_has_no_bar_and_the_word_reaches_the_host() {
    let src = stack(
        "",
        "navigationBar=(e.name == \"viewer\" ? \"none\" : \"auto\")",
        "",
    );
    let plan = contract::bake(contract::compile(&src).unwrap(), NoData).unwrap();
    let r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/viewer",
    )
    .unwrap();
    let bar = |id: &str| {
        let k = r.kernel();
        k.node_by_key(k.find_by_test_id(id)[0])
            .unwrap()
            .props
            .str(PropId::NavigationBar)
            .map(str::to_string)
    };
    assert_eq!(bar("route-home").as_deref(), Some("auto"));
    assert_eq!(bar("route-viewer").as_deref(), Some("none"));
    // A literal on every route, and none at all (the default).
    for route in ["navigationBar=\"none\"", "navigationBar=\"auto\"", ""] {
        contract::compile(&stack("", route, "")).unwrap_or_else(|e| panic!("{route}: {e}"));
    }
}

#[test]
fn a_literal_outside_none_and_auto_is_refused() {
    for value in ["hidden", "false", "", "None", "none auto"] {
        let error =
            contract::compile(&stack("", &format!("navigationBar=\"{value}\""), "")).unwrap_err();
        assert_eq!(error.id, "lower-attr-value", "{value:?}: {error}");
        assert!(
            error.message.contains("`navigationBar` takes \"none\"")
                && error.message.contains("\"auto\""),
            "{value:?}: {error}"
        );
    }
}

#[test]
fn off_a_route_the_attribute_is_refused_naming_where_it_belongs() {
    // On the navigation root (a guess at "no bars in this app") and on a box
    // inside a screen, no host reads it: the screen would keep its bar.
    for (root, inner, on) in [
        ("navigationBar=\"none\"", "", "the navigation root"),
        (
            "",
            "navigationBar=\"none\"",
            "an element with no `navigationKey`",
        ),
    ] {
        let error = contract::compile(&stack(root, "", inner)).unwrap_err();
        assert_eq!(error.id, "lower-route-bar", "{error}");
        assert!(
            error.message.contains(on) && error.message.contains("belongs to a route"),
            "{error}"
        );
        assert_eq!(
            error.span.line,
            if inner.is_empty() { 6 } else { 9 },
            "{error}"
        );
    }
    // With no navigation root at all it is as misplaced.
    let error = contract::compile("component App\n  view\n    column navigationBar=\"none\"\n")
        .unwrap_err();
    assert_eq!(error.id, "lower-route-bar", "{error}");
}

/// A button that presses nothing and opens no popover is no item of the iOS
/// bar and is hidden with the header (euxy port 14's "● Clock" readout):
/// the warning says so and names a readout's `role="status"`.
#[test]
fn a_header_button_that_presses_nothing_is_named() {
    let header = "header\n            text \"Log\" role=\"heading\" aria-level=2\n            button aria-label=\"Clock\"\n              text \"● Clock\"";
    let src = stack("", "", "").replace("column \n", &format!("{header}\n          column\n"));
    let (result, warnings) = contract::compile_warned(&src);
    result.unwrap();
    let ids: Vec<_> = warnings
        .iter()
        .map(|w| (w.id.as_str(), w.span.line))
        .collect();
    assert_eq!(ids, [("lower-header-unplaced", 11)], "{src}");
    let says = &warnings[0].message;
    assert!(
        says.starts_with(
            "a `button` that presses nothing in this route's `header` is not shown on iOS"
        ),
        "{says}"
    );
    assert!(says.contains("give it `role=\"status\"`"), "{says}");
    assert!(
        says.contains("say `navigationBar=\"none\"` on the route's element and draw the row yourself (docs/recipes/chat-header.md"),
        "{says}"
    );
    // A disabled one is UIKit's disabled bar item, shown dimmed: no warning.
    let disabled = src.replace("button aria-label", "button disabled=true aria-label");
    let (result, warnings) = contract::compile_warned(&disabled);
    result.unwrap();
    assert!(warnings.is_empty(), "{warnings:?}");
    // One that says it is not disabled is still no item.
    let enabled = src.replace("button aria-label", "button disabled=false aria-label");
    let (result, warnings) = contract::compile_warned(&enabled);
    result.unwrap();
    assert_eq!(warnings.len(), 1, "{warnings:?}");
}

/// A symbol image in the heading's own flex row is drawn in the iOS title,
/// before or after its text (`HeaderTitle`): no warning names it. One in a
/// column, a picture, or one beside a text the title does not take is.
#[test]
fn a_symbol_beside_the_heading_in_its_row_is_the_titles() {
    let header = |row: &str, image: &str, extra: &str| {
        format!("header display=\"flex\" align-items=\"center\"\n            {row}\n              image \"{image}\"\n              text \"Detail\" role=\"heading\" aria-level=2{extra}")
    };
    let unplaced = |header: String| {
        let src = stack("", "", "").replace("column \n", &format!("{header}\n          column\n"));
        let (result, warnings) = contract::compile_warned(&src);
        result.unwrap();
        warnings
            .iter()
            .filter(|w| w.id == "lower-header-unplaced")
            .map(|w| w.span.line)
            .collect::<Vec<_>>()
    };
    assert!(unplaced(header("row align-items=\"center\" gap=6", "symbol:add", "")).is_empty());
    assert!(unplaced(header("box display=\"flex\"", "symbol:add", "")).is_empty());
    assert_eq!(unplaced(header("column", "symbol:add", "")), [11]);
    assert_eq!(unplaced(header("row", "photo.png", "")), [11]);
    let beside = "\n              text \"draft\"";
    assert_eq!(
        unplaced(header("row", "symbol:add", beside)),
        [11],
        "a text in the row is the subtitle"
    );
}

/// Issue #364: a route's first `header` that holds buttons and no heading is
/// the bar with no title, so the compiler says nothing of it; one that also
/// holds a text is not, and `lower-header-no-bar` names the text and both
/// repairs.
#[test]
fn a_header_of_buttons_alone_is_the_bar_and_one_with_a_text_is_named() {
    let route = |header: &str| {
        let header: String = header.lines().map(|l| format!("          {l}\n")).collect();
        format!("routes nav\n  home \"/\"\ncomponent App\n  state n = 0\n  action go\n    n = n + 1\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header\n{header}          scroll flex=1\n            text \"body\"\n")
    };
    for header in [
        "  button press=go aria-label=\"Add\"\n    image \"symbol:add\"",
        "  row\n    button press=go aria-label=\"Add\"\n      image \"symbol:add\"\n  view flex=1\n  text \"Live\" role=\"status\"",
    ] {
        assert_eq!(crate::warnings::warned(&route(header)), [], "{header}");
    }
    let src =
        route("  text \"Inbox\"\n  button press=go aria-label=\"Add\"\n    image \"symbol:add\"");
    assert_eq!(
        crate::warnings::warned(&src),
        [("lower-header-no-bar".to_string(), 11)]
    );
    let (_, warnings) = contract::compile_warned(&src);
    let m = &warnings[0].message;
    assert!(
        m.contains("holds no heading, and this text in it is not a bar item")
            && m.contains("one `text role=\"heading\"`")
            && m.contains("a header of buttons alone is the bar"),
        "{m}"
    );
    // A heading that presses is still the heading, as the kernel's bar
    // reads it: one is the title, the text after it its subtitle, and two
    // are two headings.
    let heading = "  text \"Inbox\" role=\"heading\" press=go\n";
    let subtitle = format!("{heading}  text \"3 unread\"\n");
    assert_eq!(crate::warnings::warned(&route(&subtitle)), []);
    let (_, warnings) = contract::compile_warned(&route(&heading.repeat(2)));
    assert_eq!(warnings.len(), 1, "{warnings:?}");
    assert!(
        warnings[0].message.contains("holds 2 headings"),
        "{}",
        warnings[0].message
    );
}

/// LLP 1141.004 D5, `header-item-left-out`: for each target the app ships
/// whose bar has no place for an element iOS hosts, that element, named with
/// the targets, in source order; information, not a problem.
#[test]
fn a_header_element_a_targets_bar_has_no_place_for_is_named() {
    let app = |header: &str| {
        format!("routes nav\n  home \"/\"\ncomponent App\n  state n = 0\n  action go\n    n = n + 1\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header display=\"flex\"\n            text \"Car\" role=\"heading\" aria-level=1\n{header}          text \"body\"\n")
    };
    let said = |name: &str, targets: &str, view: &str| -> Vec<String> {
        let dir =
            crate::scratch::Scratch::new(format!("exact2-left-out-{name}-{}", std::process::id()));
        std::fs::write(dir.join("app.json"), format!("{{\"app\": {{\"id\": \"dev.test.bar\", \"name\": \"Bar\"}}, \"host\": {{{targets}}}}}")).unwrap();
        std::fs::write(dir.join("app.contract"), view).unwrap();
        let (plan, warnings) = contract::compile_path_all_warned(&dir.at("app.contract"), false);
        plan.unwrap_or_else(|e| panic!("{e:?}"));
        warnings
            .iter()
            .filter(|w| w.id == "header-item-left-out")
            .map(|w| w.message.clone())
            .collect()
    };
    let all = "\"android\": {}, \"macos\": {}";
    // A readout of words the Mac draws, and a spinner it has no place for:
    // each left out on Android, and the spinner sends the Mac's header to
    // the page, said once.
    let words = app("            row role=\"status\" aria-label=\"Live\"\n              text \"Live\"\n            progress\n");
    assert_eq!(
        said("web", "\"web\": {}", &words),
        Vec::<String>::new(),
        "the web draws them"
    );
    let both = said("all", all, &words);
    assert_eq!(both.len(), 3, "{both:?}");
    assert!(
        both[0].starts_with("this readout")
            && both[0].contains("android")
            && !both[0].contains("macos"),
        "{}",
        both[0]
    );
    assert!(
        both[1].starts_with("this `progress`") && both[1].contains("android"),
        "{}",
        both[1]
    );
    assert!(
        both[2].starts_with("on macos") && both[2].contains("whole header is drawn in the page"),
        "{}",
        both[2]
    );
    // Inside an `if`, as the guide's spinner is: a readout holding a spinner,
    // left out on Android, and no face for the Mac's toolbar; a button whose
    // face iOS hosts is an action everywhere; what its author keeps in the
    // page is no item.
    let more = app("            if true\n              row role=\"status\" aria-label=\"Updating\"\n                progress\n            button press=go aria-label=\"Live\"\n              column width=6 height=6 background-color=\"#0a0\"\n              text \"Live\"\n            progress appearance=\"none\"\n");
    let said_more = said("more", all, &more);
    assert_eq!(said_more.len(), 2, "{said_more:?}");
    assert!(
        said_more[0].starts_with("this readout") && said_more[0].contains("android"),
        "{}",
        said_more[0]
    );
    assert!(
        said_more[1].starts_with("on macos")
            && said_more[1].contains("a readout holding more than words"),
        "{}",
        said_more[1]
    );
    // A text in a box after the heading is the Mac's window subtitle; a
    // `progress` that is itself the readout is no face for its toolbar.
    let boxed = said(
        "boxed",
        "\"macos\": {}",
        &app("            column\n              text \"On time\"\n"),
    );
    assert_eq!(boxed, Vec::<String>::new(), "the window's subtitle");
    let spun = said(
        "spun",
        "\"macos\": {}",
        &app("            progress role=\"status\" aria-label=\"Updating\"\n"),
    );
    assert_eq!(spun.len(), 1, "{spun:?}");
    assert!(
        spun[0].contains("a readout holding more than words"),
        "{}",
        spun[0]
    );
    // Arms of a choice never show together: a search field in each is no
    // second one, and a heading in each with a spinner after them is one
    // heading, the spinner still named.
    let arms = |name: &str, targets: &str, header: &str| -> Vec<(String, String)> {
        let dir =
            crate::scratch::Scratch::new(format!("exact2-arms-{name}-{}", std::process::id()));
        std::fs::write(dir.join("app.json"), format!("{{\"app\": {{\"id\": \"dev.test.bar\", \"name\": \"Bar\"}}, \"host\": {{{targets}}}}}")).unwrap();
        std::fs::write(dir.join("app.contract"), format!("routes nav\n  home \"/\"\ncomponent App\n  state q = \"\"\n  state on = true\n  action typed(v: string)\n    q = v\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header display=\"flex\"\n{header}          text \"body\"\n")).unwrap();
        let (plan, warnings) = contract::compile_path_all_warned(&dir.at("app.contract"), false);
        plan.unwrap_or_else(|e| panic!("{e:?}"));
        warnings
            .iter()
            .filter(|w| w.id.starts_with("lower-header") || w.id == "header-item-left-out")
            .map(|w| (w.id.to_string(), w.message.clone()))
            .collect()
    };
    let phones = "\"android\": {}";
    let searches = arms("search", phones, "            text \"Car\" role=\"heading\" aria-level=1\n            if on\n              input type=\"search\" value=q input=typed\n            else\n              input type=\"search\" value=q input=typed\n");
    assert_eq!(
        searches,
        Vec::<(String, String)>::new(),
        "one search field whichever arm shows"
    );
    let headed = arms("headed", phones, "            if on\n              text \"Car\" role=\"heading\" aria-level=1\n            else\n              text \"Van\" role=\"heading\" aria-level=1\n            progress\n");
    assert_eq!(headed.len(), 1, "{headed:?}");
    assert!(
        headed[0].0 == "header-item-left-out" && headed[0].1.starts_with("this `progress`"),
        "{headed:?}"
    );
    // A search field after one in each arm is the second whichever shows; a
    // subtitle in each arm is the Mac's one subtitle.
    let second = arms("second", phones, "            text \"Car\" role=\"heading\" aria-level=1\n            if on\n              input type=\"search\" value=q input=typed\n            else\n              input type=\"search\" value=q input=typed\n            input type=\"search\" value=q input=typed\n");
    assert_eq!(second.len(), 1, "{second:?}");
    assert!(
        second[0].0 == "lower-header-unplaced"
            && second[0].1.contains("a second `input type=\"search\"`"),
        "{second:?}"
    );
    let mac = "\"macos\": {}";
    let subtitled = arms("subtitled", mac, "            text \"Car\" role=\"heading\" aria-level=1\n            if on\n              text \"On time\"\n            else\n              text \"Late\"\n");
    assert_eq!(
        subtitled,
        Vec::<(String, String)>::new(),
        "one subtitle whichever arm shows"
    );
    let after = arms("after", mac, "            text \"Car\" role=\"heading\" aria-level=1\n            if on\n              text \"On time\"\n            else\n              text \"Late\"\n            text \"Platform 2\"\n");
    assert_eq!(after.len(), 1, "{after:?}");
    assert!(
        after[0]
            .1
            .contains("a text that is neither its heading nor the one after it"),
        "{after:?}"
    );
    let inside = arms("inside", phones, "            text \"Car\" role=\"heading\" aria-level=1\n            input type=\"search\" value=q input=typed\n            if on\n              input type=\"search\" value=q input=typed\n");
    assert_eq!(inside.len(), 1, "{inside:?}");
    assert!(inside[0].0 == "lower-header-unplaced", "{inside:?}");
    // The Mac's toolbar draws a symbol readout, and makes segments only of
    // two or more titled tabs.
    let symbol = arms("symbol", mac, "            text \"Car\" role=\"heading\" aria-level=1\n            image \"symbol:add\" role=\"status\" aria-label=\"New\"\n");
    assert_eq!(
        symbol,
        Vec::<(String, String)>::new(),
        "a symbol readout is drawn"
    );
    let tab = |n: usize| {
        "              button role=\"tab\" press=typed(\"a\")\n                text \"A\"\n"
            .repeat(n)
    };
    let tablist = |n: usize| {
        format!("            text \"Car\" role=\"heading\" aria-level=1\n            row role=\"tablist\"\n{}", tab(n))
    };
    assert_eq!(
        arms("tabs", mac, &tablist(2)),
        Vec::<(String, String)>::new(),
        "two titled tabs are segments"
    );
    let one = arms("tab", mac, &tablist(1));
    assert_eq!(one.len(), 1, "{one:?}");
    assert!(
        one[0].1.contains("a tablist that is not two or more tabs"),
        "{one:?}"
    );
    // What a title button its author keeps holds stays in the page, hidden
    // by nothing, a second search field in it too.
    let kept = arms("kept", phones, "            button press=typed(\"t\") appearance=\"none\"\n              text \"Car\" role=\"heading\" aria-level=1\n              progress\n              input type=\"search\" value=q input=typed\n              input type=\"search\" value=q input=typed\n");
    assert_eq!(kept, Vec::<(String, String)>::new(), "kept in the page");
    // A message about a place holds however the choices go: two search
    // fields after a heading in each arm, the second hidden; a search field
    // in each of two `if`s, which may never show together, given the benefit.
    let headed_twice = arms("headed-twice", phones, "            if on\n              text \"Car\" role=\"heading\" aria-level=1\n            else\n              text \"Van\" role=\"heading\" aria-level=1\n            input type=\"search\" value=q input=typed\n            input type=\"search\" value=q input=typed\n");
    assert_eq!(headed_twice.len(), 1, "{headed_twice:?}");
    assert!(
        headed_twice[0].0 == "lower-header-unplaced",
        "{headed_twice:?}"
    );
    let apart = arms("apart", phones, "            text \"Car\" role=\"heading\" aria-level=1\n            if on\n              input type=\"search\" value=q input=typed\n            if !on\n              input type=\"search\" value=q input=typed\n");
    assert_eq!(apart, Vec::<(String, String)>::new(), "the benefit");
    // The Mac's toolbar has one search field, and a tab's title is its words,
    // not its `aria-label`; words chosen by an `if` are given the benefit.
    let searched = arms("searched", mac, "            text \"Car\" role=\"heading\" aria-level=1\n            input type=\"search\" value=q input=typed\n            input type=\"search\" value=q input=typed\n");
    assert!(
        searched
            .iter()
            .any(|(id, m)| id == "header-item-left-out" && m.contains("a second search field")),
        "{searched:?}"
    );
    let blank = "              button role=\"tab\" press=typed(\"b\") aria-label=\"B\"\n                text \"\"\n";
    let blank = arms("blank", mac, &format!("{}{blank}", tablist(1)));
    assert_eq!(blank.len(), 1, "{blank:?}");
    assert!(
        blank[0]
            .1
            .contains("a tablist that is not two or more tabs"),
        "{blank:?}"
    );
    let chosen = "              button role=\"tab\" press=typed(\"b\")\n                if on\n                  text \"B\"\n                else\n                  text \"C\"\n";
    assert_eq!(
        arms("chosen", mac, &format!("{}{chosen}", tablist(1))),
        Vec::<(String, String)>::new(),
        "either title"
    );
    // Beside a tablist's segments a line of glyphs is an item of its own,
    // which Android leaves out.
    let lined = arms(
        "lined",
        phones,
        &format!(
            "{}            row\n              image \"symbol:add\"\n              text \"Muted\"\n",
            tablist(2)
        ),
    );
    assert_eq!(lined.len(), 1, "{lined:?}");
    assert!(
        lined[0].1.starts_with("this line of glyphs") && lined[0].1.contains("android"),
        "{lined:?}"
    );
    assert!(contract_lower::warning_rank::information(
        "header-item-left-out"
    ));
}

/// LLP 1141.004 D1: in an app that ships iOS, a route's header that is the
/// navigation bar holds the bar's closed set, and the compiler refuses the
/// rest (`lower-header-item`), naming the repair. An app that does not ship
/// iOS, or a header that is not the bar, is drawn as written elsewhere.
#[test]
fn an_element_the_ios_bar_has_no_place_for_is_refused() {
    let refused = |name: &str, targets: &str, header: &str| -> Vec<String> {
        let dir =
            crate::scratch::Scratch::new(format!("exact2-refused-{name}-{}", std::process::id()));
        std::fs::write(dir.join("app.json"), format!("{{\"app\": {{\"id\": \"dev.test.bar\", \"name\": \"Bar\"}}, \"host\": {{{targets}}}}}")).unwrap();
        std::fs::write(dir.join("app.contract"), format!("routes nav\n  home \"/\"\ncomponent App\n  state q = \"\"\n  action go\n    q = \"\"\n  action typed(v: string)\n    q = v\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header display=\"flex\"\n{header}          text \"body\"\n")).unwrap();
        let (plan, _) = contract::compile_path_all_warned(&dir.at("app.contract"), false);
        match plan {
            Ok(_) => Vec::new(),
            Err(errors) => errors
                .iter()
                .map(|e| {
                    assert_eq!(e.id, "lower-header-item", "{e:?}");
                    e.message.clone()
                })
                .collect(),
        }
    };
    let ios = "\"ios\": {}";
    let heading = "            text \"Car\" role=\"heading\" aria-level=1\n";
    // The closed set: a subtitle, a symbol button, a filled button of one
    // text, a link of a symbol, a badge, readouts of a dot and words and of a
    // spinner and words, one search field and one tablist of titled tabs.
    let closed = format!("{heading}            text \"On time\"\n            button press=go aria-label=\"Add\"\n              image \"symbol:add\" width=20 height=20\n            button press=go padding=6 background-color=\"-exact-fill\" color=\"-exact-system-red\"\n              text \"Stop\"\n            link href=\"/\" aria-label=\"Home\"\n              image \"symbol:add\"\n            button press=go aria-label=\"Me\"\n              column width=32 height=32 border-radius=16 background-color=\"#e3e3fe\"\n                text \"AB\" font-size=13 color=\"#3838f5\"\n            row role=\"status\" aria-label=\"Live\" gap=6\n              column width=7 height=7 border-radius=4 background-color=\"#0a0\"\n              text \"Live\"\n            row role=\"status\" aria-label=\"Updating\"\n              progress\n              text \"Updating\"\n            input type=\"search\" value=q input=typed\n            row role=\"tablist\"\n              button role=\"tab\" press=go\n                text \"All\"\n              button role=\"tab\" press=go\n                text \"Missed\"\n");
    assert_eq!(
        refused("closed", ios, &closed),
        Vec::<String>::new(),
        "the closed set"
    );
    // A plain box after the heading is read for its words: the subtitle.
    let boxed = format!("{heading}            column\n              text \"On time\"\n");
    assert_eq!(
        refused("boxed", ios, &boxed),
        Vec::<String>::new(),
        "the subtitle in a box"
    );
    // A line of a symbol and words beside a readout is the subtitle, the
    // readout its own item.
    let line = format!("{heading}            row\n              image \"symbol:add\"\n              text \"Muted\"\n              text \"Live\" role=\"status\"\n");
    assert_eq!(
        refused("line", ios, &line),
        Vec::<String>::new(),
        "the glyph line and a readout"
    );
    // Each other element is refused, the repair named.
    for (name, header, what) in [
        ("spinner", format!("{heading}            progress\n"), "this `progress`"),
        ("picture", format!("{heading}            image \"https://example.com/logo.png\" width=20 height=20\n"), "this `image`"),
        ("switch", format!("{heading}            input type=\"checkbox\" role=\"switch\" checked=true aria-label=\"Live\"\n"), "this `input`"),
        ("dot", format!("{heading}            button press=go aria-label=\"Live\"\n              column width=6 height=6 background-color=\"#0a0\"\n              text \"Live\"\n"), "this button, whose face holds more than one symbol and one text"),
        ("ink", format!("{heading}            button press=go\n              text \"Stop\" color=\"-exact-system-red\"\n"), "this button, whose face writes on its text more than where it sits"),
        ("bordered", format!("{heading}            button press=go aria-label=\"Me\"\n              column width=32 height=32 background-color=\"#e3e3fe\" border=\"1px solid #000\"\n                text \"AB\"\n"), "this button, whose face is a badge whose box writes more"),
        ("pressing", format!("{heading}            row role=\"status\" aria-label=\"Offline\"\n              text \"Offline\"\n              button press=go\n                text \"Retry\"\n"), "a button in a readout"),
        ("third", format!("{heading}            text \"On time\"\n            text \"Platform 2\"\n"), "this text"),
        ("pressed switch", format!("{heading}            input type=\"checkbox\" role=\"switch\" checked=true press=go aria-label=\"Alerts\"\n"), "this `input`, which presses"),
    ] {
        let said = refused(name, ios, &header);
        assert_eq!(said.len(), 1, "{name}: {said:?}");
        assert!(said[0].starts_with(what) && said[0].contains("has no place in the iOS navigation bar") && said[0].contains("below the `header`"), "{name}: {}", said[0]);
    }
    // Refused in an app that ships iOS and Android, an element is not also
    // named as one Android leaves out.
    let dir = crate::scratch::Scratch::new(format!("exact2-refused-both-{}", std::process::id()));
    std::fs::write(dir.join("app.json"), "{\"app\": {\"id\": \"dev.test.bar\", \"name\": \"Bar\"}, \"host\": {\"ios\": {}, \"android\": {}}}").unwrap();
    std::fs::write(dir.join("app.contract"), format!("routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}` width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n          header display=\"flex\"\n{heading}            progress\n          text \"body\"\n")).unwrap();
    let (plan, warnings) = contract::compile_path_all_warned(&dir.at("app.contract"), false);
    assert!(plan.is_err(), "refused");
    assert!(
        !warnings.iter().any(|w| w.id == "header-item-left-out"),
        "{warnings:?}"
    );
    // An app that does not ship iOS, and a header that is not the bar, draw
    // it as written.
    let picture =
        format!("{heading}            image \"https://example.com/logo.png\" width=20 height=20\n");
    assert_eq!(
        refused("android", "\"android\": {}, \"web\": {}", &picture),
        Vec::<String>::new()
    );
    let twice = format!(
        "{heading}{heading}            image \"https://example.com/logo.png\" width=20 height=20\n"
    );
    assert_eq!(
        refused("twice", ios, &twice),
        Vec::<String>::new(),
        "two headings: no bar"
    );
}

/// The refusal reads what an element writes, not a class's rows, and the
/// kernel leaves the root's `navigationBack` control to UIKit on a pushed
/// route. So a face styled by a class, a header its class takes out of the
/// bar, and the Back control are not refused; a class elsewhere excuses
/// nothing.
#[test]
fn a_class_and_the_back_control_are_given_the_benefit() {
    let refused = |name: &str, header: &str| -> usize {
        let dir =
            crate::scratch::Scratch::new(format!("exact2-benefit-{name}-{}", std::process::id()));
        std::fs::write(
            dir.join("app.json"),
            "{\"app\": {\"id\": \"dev.test.bar\", \"name\": \"Bar\"}, \"host\": {\"ios\": {}}}",
        )
        .unwrap();
        std::fs::write(dir.join("app.contract"), format!("routes nav\n  home \"/\"\nstyle Label\n  margin-left=4\nstyle Avatar\n  background-color=\"#e3e3fe\"\nstyle Site\n  appearance=\"none\"\nstyle Padded\n  padding=8\ncomponent App\n  state q = \"\"\n  action go\n    q = \"\"\n  view\n    main navigationKey=`${{top(nav).id}}` navigationBack=\"back\" width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}` position=\"absolute\" inset=0\n{header}          text \"body\"\n")).unwrap();
        match contract::compile_path_all_warned(&dir.at("app.contract"), false).0 {
            Ok(_) => 0,
            Err(errors) => {
                for e in &errors {
                    assert_eq!(e.id, "lower-header-item", "{name}: {e:?}");
                }
                errors.len()
            }
        }
    };
    let heading = "            text \"Car\" role=\"heading\" aria-level=1\n";
    let bar = "          header display=\"flex\"\n";
    for (name, header, count) in [
        ("label", format!("{bar}{heading}            button press=go\n              text \"Save\" class=Label\n"), 0),
        ("avatar", format!("{bar}{heading}            button press=go aria-label=\"Me\"\n              column class=Avatar width=32 height=32\n                text \"AB\"\n"), 0),
        ("site", format!("          header display=\"flex\" class=Site\n{heading}            progress\n"), 0),
        ("back", format!("{bar}            button id=\"back\" press=go\n              image \"symbol:back\"\n              text \"Back\" color=\"-exact-label\"\n{heading}"), 0),
        ("padded", format!("          header display=\"flex\" class=Padded\n{heading}            progress\n"), 1),
        ("beside", format!("{bar}{heading}            button press=go\n              text \"Save\" class=Label\n            button press=go\n              text \"Stop\" color=\"-exact-label\"\n"), 1),
    ] {
        assert_eq!(refused(name, &header), count, "{name}");
    }
    // The same app's terminal entry is no iOS bar.
    let dir =
        crate::scratch::Scratch::new(format!("exact2-benefit-terminal-{}", std::process::id()));
    std::fs::write(
        dir.join("app.json"),
        "{\"app\": {\"id\": \"dev.test.bar\", \"name\": \"Bar\"}, \"host\": {\"ios\": {}}}",
    )
    .unwrap();
    std::fs::write(dir.join("terminal.contract"), format!("routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${{top(nav).id}}`\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}`\n{bar}{heading}            progress\n          text \"body\"\n")).unwrap();
    if let Err(errors) = contract::compile_path_terminal(&dir.at("terminal.contract")) {
        assert!(
            errors.iter().all(|e| e.id != "lower-header-item"),
            "{errors:?}"
        );
    }
}
