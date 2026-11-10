//! `exactPage().shownRoutes` (LLP 1127 D8): the routes a person can see and use, which a commit that
//! moves the navigation and a host's report of how its columns show them change, answered again only
//! when they do.

use crate::{DataError, DataSource, Event, Runner};
use exact_kernel::{BlockAxis, BlockShown, HostBlock, Kernel};
use exact_plan::Value;

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const SOURCE: &str = r#"shape Page
  shownRoutes: string[]

routes nav
  home "/"
    item "/item"

component App
  resource page = exactPage() as shape Page
  action open
    nav = push(nav, "/item")
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0 testId="home"
            button press=open testId="open"
              text "open"
        if e.name == "item"
          column navigationKey=`${e.id}` position="absolute" inset=0
            text "item"
"#;

const TABS: &str = r#"shape Page
  shownRoutes: string[]

routes nav
  tab first "/"
  tab second "/second"

component App
  resource page = exactPage() as shape Page
  action pick(name: string)
    nav = select(nav, name)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
      for t in nav.tabs key=t.name
        column role="tabpanel" id=`panel-${t.name}`
          for e in t.stack key=e.id
            column navigationKey=`${e.id}`
              text e.name
      row role="tablist"
        button role="tab" aria-controls="panel-first" press=pick("first") testId="tab-first"
          text "First"
        button role="tab" aria-controls="panel-second" press=pick("second") testId="tab-second"
          text "Second"
"#;

fn boot(source: &str) -> Runner<NoData> {
    let plan = contract::compile(source).unwrap();
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn view(r: &Runner<NoData>, id: &str) -> u32 {
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id(id)[0]).unwrap().id
}

fn answered(r: &Runner<NoData>) -> Option<Value> {
    r.resource("page").cloned()
}

fn keys(keys: &[String]) -> Option<Value> {
    Some(Value::record(vec![Value::list(
        keys.iter().map(|k| Value::str(k)).collect(),
    )]))
}

#[test]
fn shown_routes_answer_the_routes_a_person_can_see() {
    let mut r = boot(SOURCE);
    let home = r.current_shown_routes();
    assert_eq!(home.len(), 1, "the stack's one route");
    assert_eq!(answered(&r), keys(&home));
    // Pushed, the item covers the list: it alone shows, once the runner is asked
    let open = view(&r, "open");
    r.dispatch(open, Event::Press).unwrap();
    let item = r.current_shown_routes();
    assert_eq!(item.len(), 1);
    assert_ne!(item, home);
    assert!(r.refresh_shown_routes().unwrap().is_some());
    assert_eq!(answered(&r), keys(&item));
    assert!(
        r.refresh_shown_routes().unwrap().is_none(),
        "the same routes commit nothing"
    );
    // A split's list beside the item (an unfolded Duo): both show, the list first
    // A split lays each column's route out in a block of its own and says how it shows it
    let list = view(&r, "home");
    let column = HostBlock {
        width: BlockAxis::Is(320.0),
        height: BlockAxis::Is(669.0),
    };
    r.kernel_mut().set_host_block(list, Some(column)).unwrap();
    r.kernel_mut()
        .set_host_block_shown(list, BlockShown::Beside);
    assert!(r.refresh_shown_routes().unwrap().is_some());
    let both = [home[0].clone(), item[0].clone()];
    assert_eq!(answered(&r), keys(&both));
    // Folded again, the item over the list
    r.kernel_mut()
        .set_host_block_shown(list, BlockShown::InStack);
    assert!(r.refresh_shown_routes().unwrap().is_some());
    assert_eq!(answered(&r), keys(&item));
}

/// A tab bar's routes are in its panels: the selected tab's route shows, and a tab selected
/// before stays built but does not.
#[test]
fn shown_routes_name_the_selected_tab_route() {
    let mut r = boot(TABS);
    let first = r.current_shown_routes();
    assert_eq!(first.len(), 1, "the first tab's route");
    assert_eq!(answered(&r), keys(&first));
    let second = view(&r, "tab-second");
    r.dispatch(second, Event::Press).unwrap();
    assert!(r.refresh_shown_routes().unwrap().is_some());
    let shown = r.current_shown_routes();
    assert_eq!(shown.len(), 1, "the second tab's route alone");
    assert_ne!(shown, first);
    assert_eq!(answered(&r), keys(&shown));
    let back = view(&r, "tab-first");
    r.dispatch(back, Event::Press).unwrap();
    assert!(r.refresh_shown_routes().unwrap().is_some());
    assert_eq!(answered(&r), keys(&first));
}
