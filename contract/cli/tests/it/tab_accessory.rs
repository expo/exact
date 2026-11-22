//! @ref LLP 1075.003 §9.17 — the compiler refuses a `navigationTabAccessory`
//! it can prove no host shows, and leaves the rest to the kernel's rule.

/// Two tabs under a navigation root; `BEFORE`, `PANEL` and `AFTER` are what
/// is written before the panels, in the home panel beside its routes, and
/// between the panels and the tablist, and `LIST` the tablist's attributes.
const SOURCE: &str = r#"routes nav
  tab home "/"
  tab other "/other"
component App
  state playing = false
  action pick(n: string)
    nav = select(nav, n)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" display="flex" flex-direction="column"
BEFORE
      column flex=1
        for t in nav.tabs key = t.name
          column role="tabpanel" id=`panel-${t.name}`
PANEL
            for e in t.stack key = e.id
              column navigationKey=`${e.id}` id="route"
                text e.name
AFTER
      row role="tablist" LIST
        button role="tab" aria-controls="panel-home" press=pick("home")
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other")
          text "Other"
component Pane
  props
    name: string
  view
    row id=name
"#;

fn compile(
    list: &str,
    before: &str,
    panel: &str,
    after: &str,
) -> Result<(), contract::CompileError> {
    let source = SOURCE
        .replace("LIST", list)
        .replace("BEFORE", before)
        .replace("PANEL", panel)
        .replace("AFTER", after);
    contract::compile(&source).map(|_| ())
}

const NAMED: &str = "navigationTabAccessory=\"player\"";
const PLAYER: &str = "      row id=\"player\" height=56\n        text \"Now playing\"";

#[test]
fn a_row_after_the_panels_is_accepted_and_doubt_is_left_to_the_kernel() {
    let accepted = [
        (NAMED, "", "", PLAYER.to_string()),
        // An `if` may hold it back on purpose.
        (
            NAMED,
            "",
            "",
            "      if playing\n        row id=\"player\" height=56".to_string(),
        ),
        // A component use, or a computed id, may carry it.
        (NAMED, "", "", "      Pane(name=\"player\")".to_string()),
        (
            NAMED,
            "",
            "",
            "      row id=(playing ? \"player\" : \"idle\")".to_string(),
        ),
        // A computed popover may be none, and a panel under an `if` may not be there.
        (NAMED, "", "", "      row id=\"player\" height=56 popover=(playing ? some(\"manual\") : none)".to_string()),
        (NAMED, "", "", "      row id=\"player\" height=56\n        if playing\n          column role=\"tabpanel\" id=\"panel-extra\"".to_string()),
        // Not literal, or empty: nothing to check.
        (
            "navigationTabAccessory=(playing ? \"player\" : \"\")",
            "",
            "",
            String::new(),
        ),
        ("navigationTabAccessory=\"\"", "", "", String::new()),
    ];
    for (list, before, panel, after) in accepted {
        compile(list, before, panel, &after).unwrap_or_else(|e| panic!("{list} {after}: {e}"));
    }
}

#[test]
fn an_id_no_element_carries_outside_the_routes_is_refused() {
    let error = compile(NAMED, "", "", "      row id=\"playr\" height=56").unwrap_err();
    assert_eq!(error.id, "lower-tab-accessory", "{error}");
    assert!(
        error.message.contains("names no element")
            && error.message.contains("navigationTabAccessory=\"\""),
        "{error}"
    );
    // A route's id is a route's, which the rule refuses
    let error = compile("navigationTabAccessory=\"route\"", "", "", "").unwrap_err();
    assert!(error.message.contains("names a route"), "{error}");
    // A row in a route lies where the rule never looks
    let source = SOURCE
        .replace("LIST", NAMED)
        .replace(
            "                text e.name",
            "                row id=\"player\" height=56",
        )
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace("AFTER", "");
    let error = contract::compile(&source).unwrap_err();
    assert!(error.message.contains("names no element"), "{error}");
}

#[test]
fn each_place_the_kernel_refuses_is_refused_with_its_reason() {
    let in_panel = "            row id=\"player\" height=56";
    let cases = [
        ("inside a tab panel", "", in_panel, String::new()),
        (
            "popover",
            "",
            "",
            "      row id=\"player\" height=56 popover=\"auto\"".to_string(),
        ),
    ];
    for (reason, before, panel, after) in cases {
        let error = compile(NAMED, before, panel, &after).unwrap_err();
        assert_eq!(error.id, "lower-tab-accessory", "{reason}: {error}");
        assert!(
            error.message.contains(reason) && error.message.contains("shows no accessory"),
            "{reason}: {error}"
        );
    }
}

#[test]
fn the_attribute_off_the_root_tablist_is_refused() {
    // On a box that is not a tablist, whose name is not what to fix first
    let error = compile("", "", "", &format!("{PLAYER}\n      row {NAMED}")).unwrap_err();
    assert!(
        error.message.contains("is not a `role=\"tablist\"`"),
        "{error}"
    );
    let error = compile("", "", "", "      row navigationTabAccessory=\"playr\"").unwrap_err();
    assert!(
        error.message.contains("is not a `role=\"tablist\"`")
            && !error.message.contains("names no element"),
        "{error}"
    );
    // Outside every navigation root
    let error = contract::compile(&format!(
        "component App\n  view\n    row role=\"tablist\" {NAMED}\n"
    ))
    .unwrap_err();
    assert!(
        error.message.contains("outside every navigation root"),
        "{error}"
    );
}

#[test]
fn panels_whose_role_is_computed_leave_the_place_to_the_kernel() {
    let source = SOURCE
        .replace(
            "  state playing = false",
            "  state playing = false\n  state asPanels = true",
        )
        .replace(
            "column role=\"tabpanel\" id=`panel-${t.name}`",
            "column role=(asPanels ? \"tabpanel\" : \"group\") id=`panel-${t.name}`",
        )
        .replace("LIST", NAMED)
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace("AFTER", PLAYER);
    contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn a_for_that_repeats_the_tablist_leaves_the_named_one_to_the_kernel() {
    // The first strip links the panels and is the root's tablist; the second, `playback`, is another, which the
    // rule may take: the compiler cannot tell the iterations apart
    let source = r#"routes nav
  tab home "/"
  tab other "/other"
component App
  action pick(n: string)
    nav = select(nav, n)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" display="flex" flex-direction="column"
      column flex=1
        for t in nav.tabs key = t.name
          column role="tabpanel" id=`panel-${t.name}`
            for e in t.stack key = e.id
              column navigationKey=`${e.id}`
                text "Hi"
      for strip in ["main", "playback"] key = strip
        row role="tablist" id=strip navigationTabAccessory="playback"
          if strip == "main"
            button role="tab" aria-controls="panel-home" press=pick("home")
              text "Home"
            button role="tab" aria-controls="panel-other" press=pick("other")
              text "Other"
          else
            button press=pick("home")
              text "Play"
"#;
    contract::compile(source).unwrap_or_else(|e| panic!("{e}"));
    // Or a button in it, which lies in the tablist only on the first iteration
    let button = source
        .replace(
            "id=strip navigationTabAccessory=\"playback\"",
            "navigationTabAccessory=\"play\"",
        )
        .replace(
            "button press=pick(\"home\")\n              text \"Play\"",
            "button id=\"play\" press=pick(\"home\")\n              text \"Play\"",
        );
    assert!(button.contains("id=\"play\""), "the case is written");
    contract::compile(&button).unwrap_or_else(|e| panic!("button: {e}"));
    // A name no element can carry is refused all the same
    let error = contract::compile(&button.replace(
        "navigationTabAccessory=\"play\"",
        "navigationTabAccessory=\"plya\"",
    ))
    .unwrap_err();
    assert!(error.message.contains("names no element"), "{error}");
}

#[test]
fn a_for_that_holds_the_panels_and_the_player_leaves_their_order_to_the_kernel() {
    // The player's arm comes first in the source, and its item last, after both panels: by
    // a literal id or a computed one
    for id in ["\"player\"", "playerName"] {
        let source = format!(
            r#"routes nav
  tab home "/"
  tab other "/other"
component App
  state playerName = "player"
  action pick(n: string)
    nav = select(nav, n)
  view
    main navigationKey=`${{top(nav).id}}` navigationBack="back" display="flex" flex-direction="column"
      column flex=1
        for x in ["home", "other", "player"] key = x
          if x == "player"
            row id={id} height=56
          else
            column role="tabpanel" id=`panel-${{x}}`
      row role="tablist" {NAMED}
        button role="tab" aria-controls="panel-home" press=pick("home")
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other")
          text "Other"
"#
        );
        contract::compile(&source).unwrap_or_else(|e| panic!("{id}: {e}"));
    }
}

#[test]
fn a_navigation_root_that_is_a_tab_panel_gives_its_children_no_panel_of_its_own() {
    let source = SOURCE
        .replace("display=\"flex\" flex-direction=\"column\"\nBEFORE", "display=\"flex\" flex-direction=\"column\" role=\"tabpanel\" id=\"panel-root\"\nBEFORE")
        .replace("LIST", NAMED)
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace("AFTER", PLAYER);
    assert!(
        source.contains("id=\"panel-root\""),
        "the root says the role"
    );
    contract::compile(&source).unwrap_or_else(|e| panic!("{e}"));
}

#[test]
fn every_declaration_and_an_inline_run_are_checked() {
    // An `if` whose other arm names none does not hide a wrong name
    let arms = SOURCE.replace(
        "      row role=\"tablist\" LIST",
        "      if playing\n        row role=\"tablist\" navigationTabAccessory=\"\"\n          button role=\"tab\" aria-controls=\"panel-home\"\n            text \"Home\"\n      row role=\"tablist\" navigationTabAccessory=\"playr\"",
    );
    let arms = arms
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace("AFTER", PLAYER);
    let error = contract::compile(&arms).unwrap_err();
    assert!(error.message.contains("names no element"), "{error}");
    // A run inside a `text` has no box of its own
    let inline = SOURCE
        .replace("LIST", NAMED)
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace(
            "AFTER",
            "      text \"Now \"\n        text \"playing\" id=\"player\"",
        );
    let error = contract::compile(&inline).unwrap_err();
    assert!(error.message.contains("run inside a paragraph"), "{error}");
}

#[test]
fn a_player_in_a_navs_footer_is_left_to_the_kernel() {
    // Whether a root `nav`'s footer is the sidebar's depends on its layout, which the kernel's rule reads
    let footer = SOURCE
        .replace(
            "      row role=\"tablist\" LIST",
            &format!("      nav\n        row role=\"tablist\" {NAMED}"),
        )
        .replace(
            "        button role=\"tab\"",
            "          button role=\"tab\"",
        )
        .replace("          text \"Home\"", "            text \"Home\"")
        .replace(
            "          text \"Other\"",
            "            text \"Other\"\n        footer\n          row id=\"player\" height=56",
        )
        .replace("BEFORE", "")
        .replace("PANEL", "")
        .replace("AFTER", "");
    contract::compile(&footer).unwrap_or_else(|e| panic!("{e}"));
}

/// Two tabs whose panels are written out, with `BODY` in the panels' column.
fn written_out(body: &str, list: &str) -> String {
    format!(
        r#"routes nav
  tab home "/"
  tab other "/other"
component App
  action pick(n: string)
    nav = select(nav, n)
  view
    main navigationKey=`${{top(nav).id}}` navigationBack="back" display="flex" flex-direction="column"
      column flex=1
{body}
      row role="tablist" {list}
        button role="tab" aria-controls="panel-home" press=pick("home")
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other")
          text "Other"
"#
    )
}

#[test]
fn the_order_and_the_links_are_the_kernels_and_runs_rows_and_shapes_are_refused() {
    let home = "        column role=\"tabpanel\" id=\"panel-home\"";
    let other = "        column role=\"tabpanel\" id=\"panel-other\"";
    let player = "        row id=\"player\" height=56";
    // Where the player falls among the panels, and which panels the tablist links, the
    // kernel's rule decides on the live tree
    contract::compile(&written_out(&format!("{home}\n{player}\n{other}"), NAMED))
        .unwrap_or_else(|e| panic!("between: {e}"));
    contract::compile(&written_out(&format!("{player}\n{home}\n{other}"), NAMED))
        .unwrap_or_else(|e| panic!("before: {e}"));
    // After both it is accepted
    contract::compile(&written_out(&format!("{home}\n{other}\n{player}"), NAMED))
        .unwrap_or_else(|e| panic!("{e}"));
    // Whether a radio or a tab shows as a segment depends on its group's layout and look, which the kernel's rule reads
    contract::compile(&written_out(
        &format!("{home}\n{other}\n        row role=\"radiogroup\" aria-label=\"Speed\"\n          button role=\"radio\" aria-checked=\"true\" id=\"player\" press=pick(\"home\")\n            text \"1x\""),
        NAMED,
    ))
    .unwrap_or_else(|e| panic!("radio: {e}"));
    // A `list` is one box, which the accessory can be
    contract::compile(&written_out(
        &format!("{home}\n{other}\n        list id=\"player\" height=56\n          row height=56"),
        NAMED,
    ))
    .unwrap_or_else(|e| panic!("list: {e}"));
    // A run in `time`, a row in a `list`, a shape in an `svg` and an option are refused
    for (row, reason) in [
        (
            "        svg viewBox=\"0 0 24 24\" width=24 height=24\n          path id=\"player\" d=\"M0 0L24 24\"",
            "a part of a control",
        ),
        (
            "        select value=\"a\" change=pick aria-label=\"Song\"\n          option \"Now playing\" id=\"player\" value=\"a\" display=\"block\"",
            "a part of a control",
        ),
        (
            "        time datetime=0\n          text \"now\" id=\"player\"",
            "run inside a paragraph",
        ),
        (
            "        list\n          row id=\"player\" height=56",
            "inside a `list`",
        ),
    ] {
        let error =
            contract::compile(&written_out(&format!("{home}\n{other}\n{row}"), NAMED)).unwrap_err();
        assert!(error.message.contains(reason), "{reason}: {error}");
    }
}
