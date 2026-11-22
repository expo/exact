//! A batch says each navigation root's tabs when they change, so the
//! presenter reads the tabs of the tree it applies (LLP 1115.001 D11).
use super::Host;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::{DataError, DataSource, Event};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// Tabs under a navigation root, and another that comes once `more` is pressed.
const TABS: &str = r#"routes nav
  tab home "/"
  tab other "/other"
  tab third "/third"
component App
  state three = false
  action pick(name: string)
    nav = select(nav, name)
  action more
    three = true
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" display="flex" flex-direction="column" testId="root"
      column flex=1
        for t in nav.tabs key = t.name
          column role="tabpanel" id=`panel-${t.name}`
            for e in t.stack key = e.id
              column navigationKey=`${e.id}`
                text e.name
                button press=more testId=`more-${e.name}`
                  text "More"
      row role="tablist" testId="tabs"
        button role="tab" aria-controls="panel-home" press=pick("home") testId="home"
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other") testId="other"
          text "Other"
        if three
          button role="tab" aria-controls="panel-third" press=pick("third") testId="third"
            text "Third"
"#;

fn boot(source: &str) -> (Host<NoData>, String) {
    let plan = contract::compile(source).unwrap().encode();
    Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap()
}

fn view(host: &Host<NoData>, test_id: &str) -> u32 {
    let kernel = host.runner().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id
}

fn press(host: &mut Host<NoData>, test_id: &str) -> String {
    let id = view(host, test_id);
    let now = host.engine().now() * 1000.0;
    host.dispatch_at(id, Event::Press, now)
}

/// The batch's `rootTabs`, when it says them.
fn root_tabs(batch: &str) -> Option<serde_json::Value> {
    let v: serde_json::Value = serde_json::from_str(batch).unwrap();
    v.get("rootTabs").cloned()
}

#[test]
fn the_first_batch_says_the_roots_tabs_and_a_batch_that_leaves_them_says_nothing() {
    let (mut host, boot) = boot(TABS);
    let said = root_tabs(&boot).expect("the boot batch says them");
    let tabs = said.as_array().unwrap();
    assert_eq!(tabs.len(), 1, "{said}");
    assert_eq!(tabs[0]["root"], view(&host, "root"));
    assert_eq!(tabs[0]["tablist"], view(&host, "tabs"));
    assert_eq!(
        tabs[0]["tabs"],
        serde_json::json!([view(&host, "home"), view(&host, "other")])
    );
    assert_eq!(tabs[0]["panels"].as_array().unwrap().len(), 2);
    let picked = press(&mut host, "other");
    assert!(
        root_tabs(&picked).is_none(),
        "a selection changes no tab: {picked}"
    );
}

#[test]
fn a_tab_that_comes_is_said_in_the_batch_that_adds_it() {
    let (mut host, _) = boot(TABS);
    let more = press(&mut host, "more-home");
    let said = root_tabs(&more).expect("the batch that adds the tab says the tabs");
    assert_eq!(said[0]["tabs"].as_array().unwrap().len(), 3, "{said}");
    assert_eq!(said[0]["tabs"][2], view(&host, "third"));
}

#[test]
fn a_page_with_no_navigation_root_says_no_tabs_once() {
    let (mut host, boot) = boot("component App\n  state n = 0\n  action bump\n    n = n + 1\n  view\n    button press=bump testId=\"bump\"\n      text `${n}`\n");
    assert_eq!(root_tabs(&boot), Some(serde_json::json!([])));
    let bumped = press(&mut host, "bump");
    assert!(root_tabs(&bumped).is_none(), "{bumped}");
}

#[test]
fn the_roots_tabs_are_the_first_tablist_in_tree_order() {
    let (mut host, boot) = boot(include_str!(
        "../../../scripts/fixtures/root-tabs-order.contract"
    ));
    let said = root_tabs(&boot).expect("the boot batch says them");
    assert_eq!(said[0]["tablist"], view(&host, "first-tabs"), "{said}");
    let shown = press(&mut host, "show-home");
    let said = root_tabs(&shown).expect("the batch that adds a tablist ahead says the tabs");
    let (later, first) = (view(&host, "later-tabs"), view(&host, "first-tabs"));
    assert!(
        later > first,
        "the tablist made later has the higher view id"
    );
    assert_eq!(said[0]["tablist"], later, "the first in tree order: {said}");
}

#[test]
fn the_route_inspector_is_a_full_height_trailing_complementary_pane() {
    let source = r#"component App
  view
    main navigationKey="home" navigationBack="back" width=390 height=500
      row navigationKey="home" width=390 height=500 display="flex" flex-direction="row" testId="route"
        column flex=1
        aside testId="inspector" width=100 min-width="20%" max-width=160 box-sizing="border-box"
"#;
    let (host, _) = boot(source);
    let inspect = |host: &Host<NoData>| host.runner().route_inspector(view(host, "route"));
    let pane = inspect(&host).unwrap();
    assert_eq!(pane.view, view(&host, "inspector"));
    assert_eq!(
        (pane.width, pane.minimum, pane.maximum),
        (100.0, Some(78.0), Some(160.0))
    );

    for other in [
        source.replace("flex-direction=\"row\"", "flex-direction=\"column\""),
        source.replace("testId=\"inspector\"", "height=64 testId=\"inspector\""),
        source.replace(
            "testId=\"inspector\"",
            "position=\"absolute\" testId=\"inspector\"",
        ),
        source.replace(
            "testId=\"inspector\"",
            "role=\"navigation\" testId=\"inspector\"",
        ),
        format!("{source}        column width=20\n"),
        source.replace("        aside", "        column\n          aside"),
    ] {
        let (host, _) = boot(&other);
        assert_eq!(inspect(&host), None, "{other}");
    }
}

/// Two tabs whose tablist names its accessory by a computed id
/// (LLP 1075.003 §9.17); `{before}`, `{panel}` and `{after}` are the
/// elements written before the panels, in the home panel beside its routes,
/// and between the panels and the tablist.
fn accessory_app(before: &str, panel: &str, after: &str) -> String {
    format!(
        r#"routes nav
  tab home "/"
  tab other "/other"
component App
  state playing = false
  state hidden = false
  state name = "player"
  action pick(n: string)
    nav = select(nav, n)
  action play
    playing = true
  action hide
    hidden = true
  action unname
    name = ""
  view
    main navigationKey=`${{top(nav).id}}` navigationBack="back" display="flex" flex-direction="column" testId="root"
{before}
      column flex=1
        for t in nav.tabs key = t.name
          column role="tabpanel" id=`panel-${{t.name}}`
{panel}
            for e in t.stack key = e.id
              column navigationKey=`${{e.id}}`
                button press=play testId=`play-${{e.name}}`
                  text "Play"
                button press=hide testId=`hide-${{e.name}}`
                  text "Hide"
                button press=unname testId=`unname-${{e.name}}`
                  text "Unname"
{after}
      row role="tablist" testId="tabs" navigationTabAccessory=name
        button role="tab" aria-controls="panel-home" press=pick("home") testId="home"
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other") testId="other"
          text "Other"
"#
    )
}

const PLAYER: &str = r#"      if playing
        row id="player" testId="player" height=56 display=(hidden ? "none" : "flex")
          text "Now playing""#;

#[test]
fn the_tablist_names_its_accessory_once_the_element_is_mounted() {
    let (mut host, boot) = boot(&accessory_app("", "", PLAYER));
    let said = root_tabs(&boot).expect("the boot batch says the tabs");
    assert!(
        said[0].get("accessory").is_none(),
        "nothing is mounted: {said}"
    );
    let note = said[0]["accessoryNote"]
        .as_str()
        .expect("a name that resolves to nothing is noted");
    assert!(
        note.contains("names no element") && note.contains("navigationTabAccessory=\"\""),
        "{note}"
    );
    let played = press(&mut host, "play-home");
    let said = root_tabs(&played).expect("the batch that mounts the element says the tabs");
    assert_eq!(said[0]["accessory"], view(&host, "player"), "{said}");
    assert!(said[0].get("accessoryNote").is_none(), "{said}");
}

#[test]
fn an_accessory_with_no_box_or_an_empty_name_is_none_and_unnoted() {
    let (mut host, _) = boot(&accessory_app("", "", PLAYER));
    press(&mut host, "play-home");
    let hidden = press(&mut host, "hide-home");
    let said = root_tabs(&hidden).expect("display: none takes the accessory away");
    assert!(
        said[0].get("accessory").is_none() && said[0].get("accessoryNote").is_none(),
        "{said}"
    );
    let (mut host, _) = boot(&accessory_app("", "", PLAYER));
    let unnamed = press(&mut host, "unname-home");
    let said = root_tabs(&unnamed).expect("an empty name drops the note");
    assert!(
        said[0].get("accessory").is_none() && said[0].get("accessoryNote").is_none(),
        "{said}"
    );
}

#[test]
fn the_rule_takes_a_list_itself_a_caption_in_a_group_and_one_before_an_unlinked_panel() {
    let cases = [
        (
            "a list",
            "      list id=\"player\" testId=\"player\" height=56\n        row height=56\n          text \"Now playing\"",
        ),
        (
            "a caption in a radio group",
            "      row role=\"radiogroup\" aria-label=\"Speed\"\n        row id=\"player\" testId=\"player\" height=56\n          text \"Speed\"\n        button role=\"radio\" aria-checked=\"true\" press=play\n          text \"1x\"\n        button role=\"radio\" aria-checked=\"false\" press=play\n          text \"2x\"",
        ),
        (
            "before an unlinked panel",
            "      row id=\"player\" testId=\"player\" height=56\n        text \"Now playing\"\n      column role=\"tabpanel\" id=\"panel-unlinked\"",
        ),
    ];
    for (case, after) in cases {
        let (host, booted) = boot(&accessory_app("", "", after));
        let said = root_tabs(&booted).expect("the boot batch says the tabs");
        assert_eq!(
            said[0]["accessory"],
            view(&host, "player"),
            "{case}: {said}"
        );
    }
}

#[test]
fn a_navs_footer_holds_the_sidebars_rows_only_where_the_sidebar_may_show_it() {
    let tablist = r#"      row role="tablist" testId="tabs" navigationTabAccessory=name
        button role="tab" aria-controls="panel-home" press=pick("home") testId="home"
          text "Home"
        button role="tab" aria-controls="panel-other" press=pick("other") testId="other"
          text "Other"
"#;
    for (direction, sidebar) in [("column", true), ("row", false)] {
        let nav = format!(
            "      nav display=\"flex\" flex-direction=\"{direction}\"\n{}        footer\n          row id=\"player\" testId=\"player\" height=56\n            text \"Now playing\"\n",
            tablist.lines().map(|line| format!("  {line}\n")).collect::<String>()
        );
        let source = accessory_app("", "", "");
        assert!(source.contains(tablist), "the helper's tablist");
        let (host, booted) = boot(&source.replace(tablist, &nav));
        let said = root_tabs(&booted).expect("the boot batch says the tabs");
        if sidebar {
            let note = said[0]["accessoryNote"].as_str().unwrap_or_default();
            assert!(note.contains("footer"), "{direction}: {said}");
        } else {
            assert_eq!(
                said[0]["accessory"],
                view(&host, "player"),
                "{direction}: {said}"
            );
        }
    }
}

#[test]
fn the_rule_refuses_an_element_that_cannot_stand_beside_the_tab_bar() {
    let row = |extra: &str| {
        format!(
            "      row id=\"player\" testId=\"player\" height=56{extra}\n        text \"Now playing\""
        )
    };
    let in_panel = "            row id=\"player\" height=56\n              text \"Now playing\"";
    let cases = [
        ("before a tab panel the tablist links", accessory_app(&row(""), "", "")),
        // After a tab panel the tablist does not link, still before the linked ones
        (
            "before a tab panel the tablist links",
            accessory_app(
                &format!(
                    "      column role=\"tabpanel\" id=\"panel-unlinked\"\n{}",
                    row("")
                ),
                "",
                "",
            ),
        ),
        ("inside a tab panel", accessory_app("", in_panel, "")),
        ("popover", accessory_app("", "", &row(" popover=\"auto\""))),
        (
            "inside a `list`",
            accessory_app("", "", "      list\n        row id=\"player\" height=56"),
        ),
        (
            "more than one element",
            accessory_app("", "", &format!("{}\n{}", row(""), row(""))),
        ),
        (
            "a part of a control",
            accessory_app(
                "",
                "",
                "      svg viewBox=\"0 0 24 24\" width=24 height=24\n        path id=\"player\" d=\"M0 0L24 24\"",
            ),
        ),
        (
            "a part of a control",
            accessory_app(
                "",
                "",
                "      select value=\"a\" change=pick aria-label=\"Song\"\n        option \"Now playing\" id=\"player\" value=\"a\" display=\"block\"",
            ),
        ),
        (
            "a part of a control",
            accessory_app(
                "",
                "",
                "      row role=\"radiogroup\" aria-label=\"Speed\"\n        button role=\"radio\" aria-checked=\"true\" id=\"player\" press=play\n          text \"1x\"\n        button role=\"radio\" aria-checked=\"false\" press=play\n          text \"2x\"",
            ),
        ),
        (
            "a part of a control",
            accessory_app(
                "",
                "",
                "      row role=\"tablist\" aria-label=\"View\"\n        button role=\"tab\" aria-selected=\"true\" id=\"player\" press=play\n          text \"Queue\"\n        button role=\"tab\" aria-selected=\"false\" press=play\n          text \"Lyrics\"",
            ),
        ),
        // The tablist naming itself by its computed name
        (
            "is or holds the tablist",
            accessory_app("", "", "").replace(
                "row role=\"tablist\" testId=\"tabs\"",
                "row role=\"tablist\" testId=\"tabs\" id=\"player\"",
            ),
        ),
    ];
    for (reason, source) in cases {
        let (_, boot) = boot(&source);
        let said = root_tabs(&boot).expect("the boot batch says the tabs");
        assert!(said[0].get("accessory").is_none(), "{reason}: {said}");
        let note = said[0]["accessoryNote"].as_str().unwrap_or_default();
        assert!(
            note.contains(reason) && note.contains("shows no accessory"),
            "{reason}: {note}"
        );
    }
}
