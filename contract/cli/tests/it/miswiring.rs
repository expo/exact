//! LLP 1116.010 D5: a miswiring the compiler can see is a warning. A name
//! that points at nothing, or a key that never meets its partner, which a
//! host would refuse or ignore at run time; each names its repair, fails no
//! build, and what the compiler cannot see through is given the benefit.

use crate::scratch::Scratch;

/// The ids and lines of the warnings `src` earns, after checking that it
/// still compiles.
fn warned(src: &str) -> Vec<(String, u32)> {
    let (result, warnings) = contract::compile_warned(src);
    if let Err(e) = result {
        panic!("a warning never fails a build: {e}\n{src}");
    }
    warnings
        .iter()
        .map(|w| (w.id.clone(), w.span.line))
        .collect()
}

fn messages(src: &str) -> Vec<String> {
    contract::compile_warned(src)
        .1
        .into_iter()
        .map(|w| w.message)
        .collect()
}

const SWIPE: &str = "lower-swipe-wiring";

/// A list of rows, each a swipe `scroll`; `row` is what is in the scroll, `after` is
/// written beside it, in the row's `for` (lines 9 on).
fn rows(attrs: &str, row: &str, after: &str) -> String {
    let indent = |text: &str, by: usize| -> String {
        text.lines()
            .map(|l| format!("{}{l}\n", " ".repeat(by)))
            .collect()
    };
    format!(
        "component App\n  state items = [\"Milk\", \"Eggs\"]\n  action remove(item: string)\n    items = items.filter(i => i != item)\n  view\n    column\n      for i in items key=i\n        scroll {attrs} overflow-x=\"scroll\" overflow-y=\"hidden\" scroll-snap-type=\"x mandatory\"\n{}{}",
        indent(row, 10),
        indent(after, 8)
    )
}

const ROW: &str =
    "row id=`row-${i}` width=\"100%\" flex-shrink=0 scroll-snap-align=\"start\"\n  text i";
const DELETE: &str = "button id=`delete-${i}` destructive=true press=remove(i) aria-label=`Delete ${i}`\n  image \"symbol:delete\"";

#[test]
fn a_swipe_action_outside_its_scroll_is_named_with_its_repair() {
    // grocery-list's seven minutes: the delete button indented as the scroll's
    // sibling, not its child. iOS refused the row; the web drew the button
    // as a second line under each item.
    let src = rows(
        "swipeContent=`row-${i}` swipeTrailing=`delete-${i}`",
        ROW,
        DELETE,
    );
    assert_eq!(warned(&src), [(SWIPE.to_string(), 8)]);
    let m = &messages(&src)[0];
    assert!(
        m.contains("`swipeTrailing` names `delete-${i}`, but no element inside this `scroll` has that `id`")
            && m.contains("Put the button inside this `scroll`")
            && m.contains("docs/recipes/list-rows.md"),
        "{m}"
    );
    // A literal list, one name wrong, and the content's own name.
    let src = rows(
        "swipeContent=\"body\" swipeLeading=\"pin delete\" swipeTrailing=\"archive\"",
        "row id=\"body\" width=\"100%\"\n  text i\nbutton id=\"pin\" press=remove(i) aria-label=\"Pin\"\n  image \"symbol:pin\"\nbutton id=\"archive\" press=remove(i) aria-label=\"Archive\"\n  image \"symbol:filter\"",
        "",
    );
    let m = messages(&src);
    assert_eq!(m.len(), 1, "{m:?}");
    assert!(m[0].contains("`swipeLeading` names \"delete\""), "{m:?}");
    let src = rows(
        "swipeContent=\"row\" swipeTrailing=\"delete\"",
        "row id=\"body\"\n  text i\nbutton id=\"delete\" press=remove(i) aria-label=\"x\"\n  image \"symbol:delete\"",
        "",
    );
    assert!(
        messages(&src)[0].contains("`swipeContent` names \"row\", but no element inside"),
        "{:?}",
        messages(&src)
    );
}

#[test]
fn a_swipe_row_wired_inside_its_scroll_is_not_named() {
    // The recipe's row, with its action inside; a template and a literal
    // that could meet are one; and a computed id is the benefit.
    let inside = rows(
        "swipeContent=`row-${i}` swipeTrailing=`delete-${i}`",
        &format!("{ROW}\n{DELETE}"),
        "",
    );
    assert_eq!(warned(&inside), []);
    let literal = rows(
        "swipeContent=\"row\" swipeTrailing=\"delete\"",
        "row id=\"row\" width=\"100%\"\n  text i\nbutton id=\"delete\" press=remove(i) aria-label=\"x\"\n  image \"symbol:delete\"",
        "",
    );
    assert_eq!(warned(&literal), []);
    let computed = rows(
        "swipeContent=`row-${i}` swipeTrailing=`delete-${i}`",
        &format!("{ROW}\nbutton id=(i == \"Milk\" ? \"a\" : \"b\") press=remove(i) aria-label=\"x\"\n  image \"symbol:delete\""),
        "",
    );
    assert_eq!(warned(&computed), []);
    let mixed = rows(
        "swipeContent=\"row-Milk\" swipeTrailing=`delete-${i}`",
        &format!("{ROW}\n{DELETE}"),
        "",
    );
    assert_eq!(warned(&mixed), []);
    // A choice of actions, euxy's Reset on presets alone: each arm's names are
    // inside, and an empty arm is a row with no action that time.
    for trailing in [
        "(i == \"Milk\" ? `reset-${i} delete-${i}` : `delete-${i}`)",
        "(i == \"Milk\" ? `delete-${i}` : \"\")",
    ] {
        let chosen = rows(
            &format!("swipeContent=`row-${{i}}` swipeTrailing={trailing}"),
            &format!("{ROW}\nbutton id=`reset-${{i}}` press=remove(i) aria-label=\"Reset\"\n  image \"symbol:delete\"\n{DELETE}"),
            "",
        );
        assert_eq!(warned(&chosen), [], "{trailing}");
    }
}

#[test]
fn a_literal_swipe_name_must_fit_all_fixed_template_text() {
    for (content, id, matches) in [
        ("\"delete\"", "`delete-${i}`", false),
        ("\"tail\"", "`${i}-tail`", false),
        ("\"aba\"", "`ab${i}ba`", false),
        ("\"first-last\"", "`first-${i}-middle-${i}-last`", false),
        ("\"delete-Milk\"", "`delete-${i}`", true),
        ("\"Milk-tail\"", "`${i}-tail`", true),
        ("\"abba\"", "`ab${i}ba`", true),
        (
            "\"first-Milk-middle-Milk-last\"",
            "`first-${i}-middle-${i}-last`",
            true,
        ),
    ] {
        let src = rows(
            &format!("swipeContent={content} swipeTrailing=\"action\""),
            &format!("row id={id}\n  text i\nbutton id=\"action\" press=remove(i) aria-label=\"x\"\n  image \"symbol:delete\""),
            "",
        );
        let expected = if matches {
            vec![]
        } else {
            vec![(SWIPE.to_string(), 8)]
        };
        assert_eq!(warned(&src), expected, "{src}");
    }
}

#[test]
fn a_choice_of_swipe_actions_is_read_arm_by_arm() {
    let row = |trailing: &str| {
        rows(
            &format!("swipeContent=`row-${{i}}` swipeTrailing={trailing}"),
            &format!("{ROW}\n{DELETE}"),
            "",
        )
    };
    // An arm that names a button outside the scroll is refused when it is taken.
    let src = row("(i == \"Milk\" ? `reset-${i} delete-${i}` : `delete-${i}`)");
    assert_eq!(warned(&src), [(SWIPE.to_string(), 8)]);
    assert!(
        messages(&src)[0].contains("`swipeTrailing` names `reset-${i}`"),
        "{:?}",
        messages(&src)
    );
    // Arms that name nothing: no action on any row.
    let src = row("(i == \"Milk\" ? \"\" : \"\")");
    assert!(
        messages(&src)[0].contains("written without `swipeLeading` or `swipeTrailing`"),
        "{:?}",
        messages(&src)
    );
    // An action the compiler cannot read is the benefit of the doubt, and an
    // arm it cannot read hides nothing the other arm names.
    assert_eq!(warned(&row("i")), []);
    let src = row("(i == \"Milk\" ? \"orphan\" : i)");
    assert_eq!(warned(&src), [(SWIPE.to_string(), 8)]);
    assert!(
        messages(&src)[0].contains("`swipeTrailing` names \"orphan\""),
        "{:?}",
        messages(&src)
    );
    assert_eq!(warned(&row("(i == \"Milk\" ? \"\" : i)")), []);
}

#[test]
fn a_swipe_action_inside_the_content_or_without_the_other_is_named() {
    // The action in the content box: iOS refuses `… is inside swipeContent`.
    let src = rows(
        "swipeContent=`row-${i}` swipeTrailing=`delete-${i}`",
        "row id=`row-${i}` width=\"100%\"\n  text i\n  button id=`delete-${i}` press=remove(i) aria-label=\"x\"\n    image \"symbol:delete\"",
        "",
    );
    assert_eq!(warned(&src), [(SWIPE.to_string(), 8)]);
    assert!(
        messages(&src)[0].contains("sits inside the `swipeContent` element"),
        "{:?}",
        messages(&src)
    );
    // A content with no action, an action with no content: refused or ignored.
    let none = rows("swipeContent=`row-${i}`", ROW, "");
    assert!(
        messages(&none)[0].contains("written without `swipeLeading` or `swipeTrailing`"),
        "{:?}",
        messages(&none)
    );
    let orphan = rows(
        "swipeTrailing=`delete-${i}`",
        &format!("{ROW}\n{DELETE}"),
        "",
    );
    assert!(
        messages(&orphan)[0].contains("`swipeTrailing` is written without a `swipeContent`"),
        "{:?}",
        messages(&orphan)
    );
}

const KEY: &str = "lower-navigation-key";

/// A stack whose root is keyed `root`, its list route `home` and its pushed route `pushed`
/// (lines 8 and 11).
fn stack(root: &str, home: &str, pushed: &str) -> String {
    format!(
        "routes nav\n  rooms \"/\"\n    room \"/room/:id\"\ncomponent App\n  view\n    main navigationKey={root} navigationBack=\"\" width=\"100%\" height=\"100%\"\n      for e in stack(nav) key=e.id\n        if e.name == \"rooms\"\n          column navigationKey={home} position=\"absolute\" inset=0\n            text \"Rooms\"\n        if e.name == \"room\"\n          column navigationKey={pushed} position=\"absolute\" inset=0\n            text \"Room\"\n"
    )
}

#[test]
fn a_navigation_key_that_never_meets_its_route_is_named_with_its_repair() {
    // chat-rooms, round 12: `main navigationKey="rooms"` over routes keyed by id,
    // so a tap on a room pushed nothing that was ever shown (eight minutes).
    let src = stack("\"rooms\"", "\"rooms\"", "`${e.id}`");
    assert_eq!(warned(&src), [(KEY.to_string(), 6)]);
    let m = &messages(&src)[0];
    assert!(
        m.contains("the navigation root's `navigationKey` is the literal \"rooms\"")
            && m.contains("its routes are keyed by their entry's id (`${e.id}`)")
            && m.contains("navigationKey=`${top(nav).id}`"),
        "{m}"
    );
    // The same app's final wiring: a root of the visit's id over a route keyed by a
    // word, which the root never names ("0" at the stack's root; agent-pitfalls).
    let src = stack("`${top(nav).id}`", "\"rooms\"", "`${e.id}`");
    assert_eq!(warned(&src), [(KEY.to_string(), 9)]);
    let m = &messages(&src)[0];
    assert!(
        m.contains("this route's `navigationKey` is the literal \"rooms\"")
            && m.contains("`navigationKey \"0\" matches no route`")
            && m.contains("Key every route `navigationKey=`${e.id}``"),
        "{m}"
    );
}

#[test]
fn navigation_keys_that_meet_or_that_the_compiler_cannot_read_are_not_named() {
    for (case, root, home, pushed) in [
        // The patterns': every route by its entry, under the visit on top.
        ("by id", "`${top(nav).id}`", "`${e.id}`", "`${e.id}`"),
        // The `rest` pattern's: a choice names the list's own word while it is on top.
        (
            "a choice",
            "(top(nav).name == \"rooms\" ? \"rooms\" : `${top(nav).id}`)",
            "\"rooms\"",
            "`${e.id}`",
        ),
        // Words all through, or a key the compiler cannot read.
        ("words", "\"rooms\"", "\"rooms\"", "\"room\""),
        ("a computed root", "(e0)", "`${e.id}`", "`${e.id}`"),
        (
            "the stack's own first key",
            "`${top(nav).id}`",
            "\"0\"",
            "`${e.id}`",
        ),
    ] {
        let src = stack(root, home, pushed).replace("(e0)", "`${top(nav).name}`");
        assert_eq!(warned(&src), [], "{case}\n{src}");
    }
}

const SPLIT: &str = "lower-navigation-split";

#[test]
fn a_navigation_split_where_no_host_reads_it_is_named_with_its_repair() {
    // recipe-finder, round 12 (and seven apps of rounds 10 and 11): `navigationSplit`
    // on `main`, which the split reads on the root route only.
    let on_root = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
        "navigationBack=\"\"",
        "navigationBack=\"\" navigationSplit=360",
    );
    assert_eq!(warned(&on_root), [(SPLIT.to_string(), 6)]);
    let m = &messages(&on_root)[0];
    assert!(
        m.contains("`navigationSplit` is on the navigation root (this `main`)")
            && m.contains("it belongs to the stack's root route"),
        "{m}"
    );
    let on_box = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
        "text \"Rooms\"",
        "box navigationSplit=\"none\"\n              text \"Rooms\"",
    );
    assert_eq!(warned(&on_box), [(SPLIT.to_string(), 10)]);
    // A template the compiler folds to that word is that literal, as the compiler judges it.
    let folded_none = on_box.replace(
        "navigationSplit=\"none\"",
        "navigationSplit=`${\"n\" + \"one\"}`",
    );
    assert_eq!(warned(&folded_none), [(SPLIT.to_string(), 10)]);
    // So is one only the element's scope folds, as lowering folds it.
    let scoped_none = on_box.replace(
        "navigationSplit=\"none\"",
        "navigationSplit=`${(1 + 1 == 2) ? \"none\" : e.name}`",
    );
    assert_eq!(warned(&scoped_none), [(SPLIT.to_string(), 10)]);
    // Each use of a shared source is judged by its own value: the one folding to "none" warns, whichever comes first.
    let shared = "routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${top(nav).id}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}` position=\"absolute\" inset=0\n          Pane(mode=`${\"place\" + \"holder\"}`)\n          Pane(mode=`${\"n\" + \"one\"}`)\ncomponent Pane\n  props\n    mode: string\n  view\n    column navigationSplit=mode\n      text \"Pick one\"\n";
    let split: Vec<_> = warned(shared)
        .into_iter()
        .filter(|(id, _)| id == SPLIT)
        .collect();
    assert_eq!(split, [(SPLIT.to_string(), 14)]);
    // A computed value on a box that is no route is refused: one error, and no warning beside it
    let computed = "routes nav\n  home \"/\"\ncomponent App\n  state mode = \"none\"\n  view\n    main navigationKey=`${top(nav).id}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}` position=\"absolute\" inset=0\n          box navigationSplit=mode\n            text \"Rooms\"\n";
    let (result, notes) = contract::compile_warned(computed);
    assert_eq!(result.unwrap_err().id, SPLIT);
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // So is a switch on the navigation root, which the lowerer refuses anywhere
    let root_switch = computed
        .replace(
            "navigationBack=\"back\"",
            "navigationBack=\"back\" navigationSplit=true",
        )
        .replace("box navigationSplit=mode", "box");
    let (result, notes) = contract::compile_warned(&root_switch);
    assert_eq!(result.unwrap_err().id, "lower-attr-value");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // So is a value of the wrong type the lowerer refuses on the navigation root
    let root_bound = computed
        .replace("state mode = \"none\"", "state mode = true")
        .replace(
            "navigationBack=\"back\"",
            "navigationBack=\"back\" navigationSplit=mode",
        )
        .replace("box navigationSplit=mode", "box");
    let (result, notes) = contract::compile_warned(&root_bound);
    assert_eq!(result.unwrap_err().id, "lower-attr-type");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // So is a folded placeholder, which placement refuses on the navigation root
    let root_placeholder = computed
        .replace(
            "navigationBack=\"back\"",
            "navigationBack=\"back\" navigationSplit=`${\"place\" + \"holder\"}`",
        )
        .replace("box navigationSplit=mode", "box");
    let (result, notes) = contract::compile_warned(&root_placeholder);
    let error = result.unwrap_err();
    assert!(error.message.contains("own child"), "{error}");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // A build that refuses anything holds its placement warnings: they speak of a build that runs, as the next one
    // that compiles shows
    let instances = "routes nav\n  home \"/\"\ncomponent App\n  view\n    main navigationKey=`${top(nav).id}` navigationBack=\"back\"\n      for e in stack(nav) key=e.id\n        column navigationKey=`${e.id}` position=\"absolute\" inset=0\n          Pane(mode=\"none\")\n          Pane(mode=\"auto\")\ncomponent Pane\n  props\n    mode: string\n  view\n    box navigationSplit=mode\n      text \"Rooms\"\n";
    let (result, notes) = contract::compile_warned(instances);
    assert_eq!(result.unwrap_err().id, "lower-attr-value");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    let (result, notes) =
        contract::compile_warned(&instances.replace("mode=\"auto\"", "mode=\"none\""));
    result.unwrap();
    assert_eq!(
        notes.iter().filter(|note| note.id == SPLIT).count(),
        1,
        "{notes:?}"
    );
    // A native view's computed one is its own attribute check's, which names what the module's box takes
    let computed_native = "component A\n  state mode = \"none\"\n  view\n    ghostty-terminal testId=\"term\" width=320 height=200 navigationSplit=mode\n";
    assert_eq!(
        contract::compile(computed_native).unwrap_err().id,
        "lower-native-attr"
    );
    // So is an action as the value, refused where the value is
    let action = computed
        .replace("box navigationSplit=mode", "box")
        .replace(
            "navigationBack=\"back\"",
            "navigationBack=\"back\" navigationSplit=pick",
        )
        .replace("  view\n", "  action pick\n    mode = \"split\"\n  view\n");
    let (result, notes) = contract::compile_warned(&action);
    assert!(result.is_err());
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // So is a native view's, which the module never takes
    let native = "component A\n  view\n    ghostty-terminal testId=\"term\" width=320 height=200 navigationSplit=\"none\"\n";
    let (result, notes) = contract::compile_warned(native);
    assert_eq!(result.unwrap_err().id, "lower-native-attr");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // So is a width the lowerer does not take, there as anywhere
    let fraction = computed.replace("navigationSplit=mode", "navigationSplit=280.5");
    let (result, notes) = contract::compile_warned(&fraction);
    assert_eq!(result.unwrap_err().id, "lower-attr-value");
    assert!(notes.iter().all(|note| note.id != SPLIT), "{notes:?}");
    // A literal width is literal too, written as a number, as text or as a template with nothing computed in it.
    for width in ["280", "\"280\"", "`280`"] {
        let on_box = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
            "text \"Rooms\"",
            &format!("box navigationSplit={width}\n              text \"Rooms\""),
        );
        assert_eq!(warned(&on_box), [(SPLIT.to_string(), 10)], "{width}");
    }
    // A literal `true` is refused for what it says, not as a computed value.
    let on_box = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
        "text \"Rooms\"",
        "box navigationSplit=true\n              text \"Rooms\"",
    );
    let error = contract::compile(&on_box).unwrap_err();
    assert!(
        error.id == "lower-attr-value" && error.message.contains("is no switch"),
        "{error}"
    );
    // On the root route, and a placeholder box inside it, it is read.
    let on_route = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
        "column navigationKey=`${e.id}` position=\"absolute\" inset=0\n            text \"Rooms\"",
        "column navigationKey=`${e.id}` navigationSplit=\"split\" position=\"absolute\" inset=0\n            box navigationSplit=\"placeholder\"\n              text \"Pick one\"",
    );
    assert_eq!(warned(&on_route), []);
    // So is one the compiler folds to "placeholder", which it takes as written so
    let folded = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "box navigationSplit=`${\"place\" + \"holder\"}`",
    );
    assert_eq!(warned(&folded), []);
    // One that folds only with the app's scope is the lowerer's, which takes it as the placeholder
    let scoped = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "box navigationSplit=`${(1 + 1 == 2) ? \"placeholder\" : e.name}`",
    );
    assert_eq!(warned(&scoped), []);
    // A portal would move a placeholder where the split cannot lay it out.
    let portaled = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "box navigationSplit=\"placeholder\" portal=\"elsewhere\"",
    );
    let error = contract::compile(&portaled).unwrap_err();
    assert!(
        error.id == SPLIT && error.message.contains("`portal` moves it elsewhere"),
        "{error}"
    );
    // An empty portal moves nothing.
    contract::compile(&portaled.replace("portal=\"elsewhere\"", "portal=\"\"")).unwrap();
    // A popover shows only when opened, so it is no placeholder.
    let popover = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "box navigationSplit=\"placeholder\" popover=\"auto\" id=\"pick\"",
    );
    let error = contract::compile(&popover).unwrap_err();
    assert!(
        error.id == SPLIT && error.message.contains("shows only when opened"),
        "{error}"
    );
    // Only a plain box is a placeholder (`view` is `box`): a header is its route's bar.
    contract::compile(&on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "view navigationSplit=\"placeholder\"",
    ))
    .unwrap();
    let header = on_route.replace(
        "box navigationSplit=\"placeholder\"\n              text \"Pick one\"",
        "header navigationSplit=\"placeholder\"\n              h1 \"Pick one\"",
    );
    let error = contract::compile(&header).unwrap_err();
    assert!(
        error.id == SPLIT && error.message.contains("only a plain box can be"),
        "{error}"
    );
    // `popover=none` makes no popover.
    contract::compile(&popover.replace("popover=\"auto\"", "popover=none")).unwrap();
    // LLP 1127 D1: "none" beside the route's own placeholder box is admitted: "none" wins at run time.
    let both = on_route.replace("navigationSplit=\"split\"", "navigationSplit=\"none\"");
    contract::compile(&both).unwrap();
    // Off a route, a computed `navigationSplit` is no placeholder the compiler can place.
    let computed_box = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "box navigationSplit=(true ? \"placeholder\" : \"\")",
    );
    let error = contract::compile(&computed_box).unwrap_err();
    assert!(error.message.contains("only the literal"), "{error}");
    // A placeholder is a box: on a list no host shows it.
    let listed = on_route.replace(
        "box navigationSplit=\"placeholder\"",
        "list navigationSplit=\"placeholder\"",
    );
    let error = contract::compile(&listed).unwrap_err();
    assert!(error.message.contains("marks a box"), "{error}");
    // Only the route's own child is laid out over the detail pane: one inside another box is refused.
    let nested = on_route.replace(
        "            box navigationSplit=\"placeholder\"\n              text \"Pick one\"",
        "            column\n              box navigationSplit=\"placeholder\"\n                text \"Pick one\"",
    );
    assert_ne!(nested, on_route, "the fixture moved");
    let error = contract::compile(&nested).unwrap_err();
    assert!(
        error.message.contains("inside another box of its route"),
        "{error}"
    );
    // A box a `for` repeats is a row: the split shows one box, and a list mounts its rows where the split cannot lay them out.
    let repeated = on_route.replace(
        "            box navigationSplit=\"placeholder\"\n              text \"Pick one\"",
        "            for q in stack(nav) key=q.id\n              box navigationSplit=\"placeholder\"\n                text \"Pick one\"",
    );
    assert_ne!(repeated, on_route, "the fixture moved");
    let error = contract::compile(&repeated).unwrap_err();
    assert!(
        error.id == SPLIT && error.message.contains("a `for` inside the route repeats"),
        "{error}"
    );
    // A placeholder box outside every route is refused: no split shows it.
    let orphan = stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`").replace(
        "      for e in stack(nav)",
        "      box navigationSplit=\"placeholder\"\n        text \"Pick one\"\n      for e in stack(nav)",
    );
    assert_ne!(
        orphan,
        stack("`${top(nav).id}`", "`${e.id}`", "`${e.id}`"),
        "the fixture moved"
    );
    let error = contract::compile(&orphan).unwrap_err();
    assert!(error.message.contains("outside every route"), "{error}");
}

const IMAGE: &str = "lower-image-source";

/// The warnings of a file that uses a package part (`exact:content`), as a build prints them.
fn warned_file(name: &str, src: &str) -> Vec<(String, u32, String)> {
    let dir = Scratch::new(format!("exact-miswiring-{name}-{}", std::process::id()));
    let path = dir.join("app.contract");
    std::fs::write(&path, src).unwrap();
    let (result, warnings) = contract::compile_path_all_warned(&path, false);
    let _ = std::fs::remove_dir_all(&dir);
    if let Err(e) = result {
        panic!("a warning never fails a build: {e:?}\n{src}");
    }
    warnings
        .into_iter()
        .map(|w| (w.id.clone(), w.span.line, w.message))
        .collect()
}

#[test]
fn an_image_source_that_is_neither_a_symbol_nor_a_file_is_named_with_its_repair() {
    // habit-tracker, round 12: `EmptyState(symbol="sparkles")` drew a blank, and event-rsvp's
    // `symbol="calendar"` the same; the warning points at the call's own line.
    let src = "use EmptyState from \"exact:content\"\ncomponent App\n  view\n    column\n      EmptyState(symbol=\"sparkles\", title=\"Nothing\", message=\"Add one\")\n      image \"gear\"\n      image \"banner\"\n";
    let got = warned_file("blank", src);
    assert_eq!(
        got.iter()
            .map(|(id, line, _)| (id.as_str(), *line))
            .collect::<Vec<_>>(),
        [(IMAGE, 5), (IMAGE, 6), (IMAGE, 7)],
        "{got:?}"
    );
    assert!(
        got[0]
            .2
            .contains("this word is a symbol: write `symbol:sparkles`")
            && got[1].2.contains("write `symbol:settings`")
            && got[2].2.contains("a symbol is written `symbol:<role>`"),
        "{got:?}"
    );
    // A role, an Apple name, a file, a URL, a computed source: all drawn.
    let fine = "component App\n  state s = \"x\"\n  view\n    column\n      image \"symbol:heart\"\n      image \"symbol:sf/heart.text.square\"\n      image \"logo.png\"\n      image \"/img/logo\"\n      image \"https://example.com/a\"\n      image s\n";
    assert_eq!(warned(fine), []);
}

/// The euxy port: a deep link into another tab, `go` then `select`, left the
/// visit on the tab it left; the warning names `open`, which lands it.
#[test]
fn a_select_after_a_push_is_named_with_the_forms_that_land_the_visit() {
    let app = |body: &str| {
        format!("routes nav\n  tab today \"/\"\n  tab patterns \"/patterns\"\n    shared \"/patterns/open\"\ncomponent App\n  action follow(url: string)\n{body}  view\n    main navigationKey=`${{top(nav).id}}` navigate=follow\n      for e in stack(nav) key=e.id\n        column navigationKey=`${{e.id}}`\n          text e.name\n")
    };
    const SELECT: &str = "lower-select-after-push";
    let src = app("    let n = go(nav, url)\n    nav = url.startsWith(\"/patterns/\") ? select(n, \"patterns\") : n\n");
    assert_eq!(warned(&src), [(SELECT.to_string(), 8)]);
    let m = &messages(&src)[0];
    assert!(
        m.contains("this `select` does not land the visit on \"patterns\": where `go` pushes, it pushes onto the selected tab's stack, whatever tab declares the route, so the visit stays on the tab being left; where the location is already \"patterns\"'s top, `go` selects \"patterns\" itself and this `select` pops it to its root")
            && m.contains("write `open(nav, url)`, which puts the route's declared chain in place of that tab's stack"),
        "{m}"
    );
    let src = app("    nav = select(push(nav, url), \"patterns\")\n");
    assert_eq!(warned(&src), [(SELECT.to_string(), 7)]);
    // The app's own `push` is no router verb.
    let own = app("    nav = select(push(nav, url), \"patterns\")\n").replace(
        "component App\n",
        "fn push(r: Router, url: string): Router = r\ncomponent App\n",
    );
    assert_eq!(warned(&own), [], "{own}");
    for verb in ["go", "push"] {
        let src = app(&format!(
            "    let n = push(nav, url)\n    nav = select({verb}(n, top(n).url), \"patterns\")\n"
        ));
        assert_eq!(warned(&src), [(SELECT.to_string(), 8)], "{src}");
        let src = app(&format!(
            "    nav = select({verb}(nav, top(back(nav)).url), \"patterns\")\n"
        ));
        assert_eq!(warned(&src), [(SELECT.to_string(), 7)], "{src}");
        let src = app(&format!(
            "    nav = select({verb}(nav, top(nav).url), \"patterns\")\n"
        ))
        .replace(
            "component App\n",
            "shape Location\n  id: number\n  url: string\nfn top(r: Router): Location = Location(id=0, url=\"/patterns/open\")\ncomponent App\n",
        );
        assert_eq!(warned(&src), [(SELECT.to_string(), 11)], "{src}");
    }
    for body in [
        "    nav = select(go(nav, top(nav).url), \"patterns\")\n",
        "    nav = select(push(nav, top(nav).url), \"patterns\")\n",
        "    let n = go(nav, top(nav).url)\n    nav = select(n, \"patterns\")\n",
        "    let n = push(nav, top(nav).url)\n    nav = select(n, \"patterns\")\n",
        "    nav = go(select(nav, \"patterns\"), url)\n",
        "    nav = open(nav, url)\n",
        "    let n = back(nav)\n    nav = select(n, \"today\")\n",
        // A binder of the same name hides the pushed `let`.
        "    let n = go(nav, url)\n    nav = match some(nav) { case some(n) => select(n, \"patterns\"), case none => n }\n",
        "    let n = go(nav, url)\n    match some(nav)\n      case some(n)\n        nav = select(n, \"patterns\")\n      case none\n        nav = n\n",
        // A choice is pushed only when every arm is.
        "    let on = url.startsWith(\"/patterns/\")\n    let n = on ? go(nav, url) : nav\n    nav = on ? n : select(n, \"patterns\")\n",
        // A `let` in another block.
        "    if url == \"/\"\n      let n = go(nav, url)\n      nav = n\n    else\n      let n = nav\n      nav = select(n, \"patterns\")\n",
    ] {
        assert_eq!(warned(&app(body)), [], "{body}");
    }
}

/// The euxy port: a template location is refused naming the `path()` that
/// spells it, a query concatenated after it.
#[test]
fn a_template_location_names_its_path() {
    let app = |location: &str| {
        format!(
            "routes nav\n  home \"/\"\n    day \"/day/:key\"\n    open \"/open\"\ncomponent App\n  state k = \"a\"\n  state ks = [\"a\"]\n  action visit\n    nav = push(nav, {location})\n  view\n    button press=visit\n      text \"Go\"\n"
        )
    };
    let refusal = |location: &str| contract::compile(&app(location)).unwrap_err();
    for (location, write) in [
        ("`/day/${k}`", "path(\"day\", k)"),
        (
            "`/open?u=${encodeURIComponent(k)}`",
            "path(\"open\") + `?u=${encodeURIComponent(k)}`",
        ),
        // A method and `length` as the author writes them.
        (
            "`/day/${k.trim().toLowerCase()}`",
            "path(\"day\", k.trim().toLowerCase())",
        ),
        ("`/day/${ks.length}`", "path(\"day\", ks.length)"),
    ] {
        let e = refusal(location);
        assert_eq!(e.id, "route-template", "{e}");
        let quoted = match write.contains('`') {
            true => format!("write `` {write} ``"),
            false => format!("write `{write}`"),
        };
        assert!(e.message.ends_with(&quoted), "{e}");
        // The repair compiles.
        if let Err(e) = contract::compile(&app(write)) {
            panic!("{write}: {e}");
        }
    }
    let src = app("`/day/${k}`").replace("state k = \"a\"", "state k = true");
    let e = contract::compile(&src).unwrap_err();
    assert_eq!(e.id, "route-template", "{e}");
    assert!(
        e.message.ends_with("write `` path(\"day\", `${k}`) ``"),
        "{e}"
    );
    let repaired = src.replace("`/day/${k}`", "path(\"day\", `${k}`)");
    if let Err(e) = contract::compile(&repaired) {
        panic!("{repaired}: {e}");
    }
    let unconverted = src.replace("`/day/${k}`", "path(\"day\", k)");
    assert_eq!(
        contract::compile(&unconverted).unwrap_err().id,
        "type-argument"
    );
    // No route, or text a repair could not write back as it reads: the hint alone.
    for location in [
        "`/elsewhere/${k}`",
        "`/day/\\${k}`",
        "`/day/${k}?q=\\\\`",
        "`/day/${k}?q=\\``",
        "`/day/${\"a\\\"b\"}`",
    ] {
        let e = refusal(location);
        assert_eq!(e.id, "route-template", "{location}: {e}");
        assert!(
            e.message.ends_with("would make another location)"),
            "{location}: {e}"
        );
    }
}

const GEOMETRY: &str = "lower-geometry-id";

/// An app whose `probe` action reads `read`, with `view` under its root column.
fn geometry(read: &str, view: &str) -> String {
    let view: String = view.lines().map(|l| format!("      {l}\n")).collect();
    format!("component App\n  state h = 0\n  action probe\n    h = {read}.height\n  view\n    column\n{view}      button press=probe\n        text \"Probe\"\n")
}

/// #301: a geometry read whose literal id no element has answers
/// `unavailable` on every host; the build says so, and names the `testId` an
/// author most likely meant.
#[test]
fn a_geometry_read_of_an_id_no_element_has_is_named() {
    let view = "column testId=\"root\" width=\"100%\"\n  text \"Body\"";
    for read in [
        "frame(\"root\")",
        "measure(\"root\")",
        "rangeFrame(\"root\", 0, 1)",
    ] {
        let src = geometry(read, view);
        assert_eq!(warned(&src), [(GEOMETRY.to_string(), 4)], "{read}");
        let name = &read[..read.find('(').unwrap()];
        let m = &messages(&src)[0];
        assert!(
            m.starts_with(&format!(
                "`{name}(\"root\")` names no element: nothing in the view has `id=\"root\"`"
            )),
            "{m}"
        );
        assert!(
            m.contains("`testId=\"root\"` names an element for tests"),
            "{m}"
        );
        // The repair the warning names silences it.
        let repaired = src.replace("testId=\"root\"", "testId=\"root\" id=\"root\"");
        assert_eq!(warned(&repaired), [], "{repaired}");
    }
    // A misspelling of an id the view has is named.
    let src = geometry("frame(\"bya\")", "column id=\"bay\"");
    assert_eq!(warned(&src), [(GEOMETRY.to_string(), 4)]);
    assert!(messages(&src)[0].contains("did you mean `frame(\"bay\")`?"));
    // Nothing near: the repair alone.
    let src = geometry("frame(\"nowhere\")", "column id=\"bay\"");
    assert!(messages(&src)[0].ends_with("give the element to read `id=\"nowhere\"`"));
}

/// What the compiler cannot see through is given the benefit of the doubt: a
/// computed id anywhere, a computed read, a prop a caller fills.
#[test]
fn a_geometry_read_that_might_name_an_element_is_not_named() {
    for (read, view) in [
        ("frame(\"bay\")", "column id=\"bay\""),
        (
            "frame(\"row-a\")",
            "for k in [\"a\", \"b\"] key=k\n  row id=`row-${k}`\n    text k",
        ),
        (
            "frame(\"other\")",
            "for k in [\"a\", \"b\"] key=k\n  row id=k\n    text k",
        ),
        ("frame(`row-${h}`)", "column id=\"bay\""),
        (
            "frame(\"later\")",
            "if h > 0\n  column id=\"later\"\nelse\n  text \"none\"",
        ),
    ] {
        let src = geometry(read, view);
        assert_eq!(warned(&src), [], "{src}");
    }
}

/// The view is the expanded one: an id a child's view writes counts, and a
/// child's literal read is checked once however often the child is used. The
/// id a caller passes is a computed argument in the child's action.
#[test]
fn a_geometry_read_in_a_child_is_checked_against_the_whole_view() {
    let app = |read: &str| {
        format!("component App\n  view\n    column\n      Panel()\n      Probe(target=\"pane\")\n      Probe(target=\"pane\")\n\ncomponent Probe\n  props\n    target: string\n  state h = 0\n  action probe\n    h = {read}.height\n  view\n    button press=probe\n      text \"Probe\"\n\ncomponent Panel\n  view\n    column id=\"panel\"\n")
    };
    assert_eq!(warned(&app("frame(\"panel\")")), []);
    assert_eq!(warned(&app("frame(target)")), []);
    let src = app("frame(\"pane\")");
    assert_eq!(warned(&src), [(GEOMETRY.to_string(), 13)]);
    assert!(messages(&src)[0].contains("did you mean `frame(\"panel\")`?"));
}

const NAVIGATION_BACK: &str = "lower-navigation-back-id";

fn back_id(name: &str, view: &str) -> String {
    let view: String = view.lines().map(|l| format!("      {l}\n")).collect();
    format!("component App\n  state closed = 0\n  action back\n    closed = closed + 1\n  action close(v: number)\n    closed = v\n  view\n    main navigationKey=\"root\" {name}\n{view}")
}

fn back_warnings(src: &str) -> Vec<(String, u32)> {
    warned(src)
        .into_iter()
        .filter(|(id, _)| id == NAVIGATION_BACK)
        .collect()
}

#[test]
fn navigation_back_names_a_press_controls_id_not_an_action() {
    for press in ["back", "close(1)"] {
        let name = if press == "back" { "back" } else { "close" };
        let src = back_id(
            &format!("navigationBack=\"{name}\""),
            &format!("button press={press}\n  text \"Close\""),
        );
        assert_eq!(back_warnings(&src), [(NAVIGATION_BACK.to_string(), 8)]);
        let m = messages(&src)
            .into_iter()
            .find(|m| m.contains("names a control's `id`, not an action"))
            .unwrap();
        assert!(
            m.contains("no element in the view has that id")
                && m.contains("root's `navigate` handler")
                && m.contains("router's Back"),
            "{m}"
        );
        assert!(m.contains(&format!("id=\"{name}\"")), "{m}");
        let repaired = src.replace("button press=", &format!("button id=\"{name}\" press="));
        assert_eq!(back_warnings(&repaired), []);
    }
    // The warning does not depend on a namesake action or any press handler.
    for view in ["text \"Body\"", "button press=back\n  text \"Close\""] {
        let src = back_id("navigationBack=\"close\"", view);
        assert_eq!(
            back_warnings(&src),
            [(NAVIGATION_BACK.to_string(), 8)],
            "{src}"
        );
    }
    let src = back_id(
        "navigationBack=\"close\"",
        "button testId=\"close\" press=back\n  text \"Close\"",
    );
    assert!(messages(&src)
        .iter()
        .any(|m| m.contains("`testId=\"close\"`")));
    let src = back_id(
        "navigationBack=\"clsoe\"",
        "button id=\"close\" press=back\n  text \"Close\"",
    );
    assert!(messages(&src)
        .iter()
        .any(|m| m.contains("did you mean `navigationBack=\"close\"`?")));
}

#[test]
fn navigation_back_checks_only_provably_missing_controls() {
    for (name, view) in [
        ("", "text \"Body\""),
        ("navigationBack=\"\"", "text \"Body\""),
        ("navigationBack=`${closed}`", "text \"Body\""),
        (
            "navigationBack=\"close\"",
            "column id=\"close\"\n  text \"Body\"",
        ),
        (
            "navigationBack=\"close\"",
            "button id=\"close\" commandfor=\"ask\" command=\"show-modal\"\n  text \"Close\"\ndialog id=\"ask\"\n  text \"Confirm\"",
        ),
        (
            "navigationBack=\"close\"",
            "button id=\"close\" press=back\n  text \"Close\"",
        ),
        (
            "navigationBack=\"close\"",
            "button id=`${closed}` press=back\n  text \"Close\"",
        ),
        (
            "navigationBack=\"close\"",
            "button id=\"close\" disabled=true press=back\n  text \"Close\"",
        ),
        (
            "navigationBack=\"close\"",
            "if closed == 0\n  button id=\"close\" press=back\n    text \"Close\"",
        ),
    ] {
        let src = back_id(name, view);
        assert_eq!(back_warnings(&src), [], "{src}");
    }
    // A matching id only on another route is not proof of reachability.
    let src = back_id("navigationBack=\"close\"", "column navigationKey=\"home\"\n  button id=\"close\" press=back\n    text \"Close home\"\ncolumn navigationKey=\"detail\"\n  text \"Detail\"");
    assert_eq!(back_warnings(&src), []);
    // A computed id on any element could match the Back name.
    let src = back_id(
        "navigationBack=\"close\"",
        "column id=`${closed}`\n  text \"Body\"",
    );
    assert_eq!(back_warnings(&src), []);
    let src = "component App\n  view\n    main navigationKey=\"root\" navigationBack=\"close\"\n      Close()\ncomponent Close\n  state closed = false\n  action back\n    closed = true\n  view\n    button id=\"close\" press=back\n      text \"Close\"\n";
    assert_eq!(back_warnings(src), []);
    let src = src.replace(" id=\"close\"", "");
    assert_eq!(back_warnings(&src), [(NAVIGATION_BACK.to_string(), 3)]);
}
