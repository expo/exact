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
