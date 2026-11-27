//! LLP 1030.004 D1: a literal `target` is HTML's (empty, or one of its four
//! keywords in any case) or a window's name. A name that starts with `_` and
//! is not a keyword, and `window-<n>`, the agent's label for a window without
//! a name, are refused. D2: the page's `name` is the window's from the first
//! settle, and a window opened at a route launches rooted there.
use crate::source::{boxed, Source};
use exact_kernel::Kernel;
use exact_runner::time::{launching_with, Launch};
use exact_runner::{DataError, DataSource, Runner, Value};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("host fact reached data: {source}")
    }
}

fn boot(source: &str, launch: Launch, location: &str) -> Runner<Source> {
    let plan = contract::bake(contract::compile(source).unwrap(), boxed(NoData)).unwrap();
    launching_with(launch, || {
        Runner::boot(
            plan,
            boxed(NoData),
            Kernel::with_monospace(),
            Default::default(),
            location,
        )
    })
    .unwrap()
}

fn text(r: &Runner<Source>, id: &str) -> String {
    let key = r.kernel().find_by_test_id(id)[0];
    let node = r.kernel().node_by_key(key).unwrap();
    node.props
        .str(exact_kernel::PropId::Text)
        .unwrap_or("")
        .to_string()
}

fn named(name: Option<&str>, rooted: bool) -> Launch {
    Launch {
        window_name: name.map(String::from),
        rooted,
        ..Launch::NONE
    }
}

#[test]
fn a_target_is_a_keyword_or_a_window_name() {
    let compile = |t: &str| {
        contract::compile(&format!(
            "component A\n  view\n    link href=\"/x\" target=\"{t}\"\n      text \"x\"\n"
        ))
    };
    for t in [
        "",
        "_blank",
        "_self",
        "_parent",
        "_top",
        "_BLANK",
        "_Self",
        "compose",
        "settings",
        "main",
        "Window-2",
        "window-",
        "window-a",
        "inspector_2",
    ] {
        compile(t).unwrap_or_else(|e| panic!("{t:?}: {e}"));
    }
    for t in ["_foo", "_", "_blanks", "window-2", "window-10"] {
        let e = compile(t).expect_err(t).to_string();
        assert!(
            e.contains("`target` accepts") && e.contains(&format!("given \"{t}\"")),
            "{t:?}: {e}"
        );
    }
}

#[test]
fn a_panel_link_opens_a_named_secondary_window() {
    let compile = |declaration: &str| {
        contract::compile(&format!(
            "component A\n  state t = \"tools\"\n  view\n    {declaration}\n      text \"Go\"\n"
        ))
    };
    for target in ["t", "\"tools\"", "\"window-palette\""] {
        compile(&format!(
            "link href=\"/x\" target={target} windowKind=\"panel\""
        ))
        .unwrap();
    }
    for declaration in [
        "link href=\"/x\" target=\"main\" windowKind=\"panel\"",
        "link href=\"/x\" target=\"_blank\" windowKind=\"panel\"",
        "link href=\"/x\" windowKind=\"panel\"",
        "link target=t windowKind=\"panel\"",
        "link href=\"/x\" target=t windowKind=\"hud\"",
        "box href=\"/x\" target=t windowKind=\"panel\"",
    ] {
        let why = compile(declaration).unwrap_err().to_string();
        assert!(
            why.contains("windowKind") || why.contains("panel target"),
            "{declaration}: {why}"
        );
    }
}

/// A bound `target` is resolved at the press, so the compiler does not check it.
#[test]
fn a_bound_target_compiles() {
    contract::compile(
        "component A\n  state t = \"_foo\"\n  view\n    link href=\"/x\" target=t\n      text \"x\"\n",
    )
    .unwrap();
}

/// `name` is `main` unless the host names the window, and a task gated on
/// it is armed, or not, from the boot.
#[test]
fn the_pages_name_is_the_windows_from_the_first_settle() {
    const APP: &str = "shape Page\n  name: string\ncomponent App\n  state n = 0\n  resource page = exactPage() as shape Page\n  task poll when page.name == \"main\"\n    every(1000, step)\n  action step\n    n = n + 1\n  view\n    text page.name testId=\"name\"\n";
    let main = boot(APP, Launch::NONE, "/");
    assert_eq!(text(&main, "name"), "main");
    assert_eq!(main.timer_due_ms(), Some(1000.0));
    let compose = boot(APP, named(Some("compose"), false), "/");
    assert_eq!(text(&compose, "name"), "compose");
    assert_eq!(compose.timer_due_ms(), None);
    assert_eq!(text(&boot(APP, named(Some(""), false), "/"), "name"), "");
}

const ROUTES: &str = "routes nav\n  tab inbox \"/\"\n    message \"/m/:id\"\n    compose \"/compose\"\n  tab settings \"/settings\"\n    pane \"/settings/:pane\"\ncomponent App\n  action follow(url: string)\n    nav = go(nav, url)\n  action pick(tab: string)\n    nav = select(nav, tab)\n  action pop\n    nav = back(nav)\n  view\n    main navigationKey=`${top(nav).id}` navigate=follow\n      text `${nav.tab} ${stack(nav).map(e => e.name).join(\",\")}` testId=\"where\"\n";

/// A rooted launch shows its screen alone; a link pushes above it, Back
/// returns to it, `select` seeds another tab, and a retarget from elsewhere
/// makes another screen the only visit.
#[test]
fn a_window_opened_at_a_route_is_rooted_there() {
    let mut r = boot(ROUTES, named(Some("compose"), true), "/compose");
    assert_eq!(text(&r, "where"), "inbox compose");
    r.act("follow", vec![Value::str("/m/4")]).unwrap();
    assert_eq!(text(&r, "where"), "inbox compose,message");
    r.act("pop", vec![]).unwrap();
    assert_eq!(text(&r, "where"), "inbox compose");
    r.act("pop", vec![]).unwrap();
    assert_eq!(text(&r, "where"), "inbox compose", "no Back from the root");
    r.act("pick", vec![Value::str("settings")]).unwrap();
    assert_eq!(text(&r, "where"), "settings settings");
    r.act("pick", vec![Value::str("inbox")]).unwrap();
    assert_eq!(text(&r, "where"), "inbox compose");
    assert!(r.host_root("/settings/general").unwrap().is_some());
    assert_eq!(text(&r, "where"), "settings pane");
    assert!(r.host_root("/nowhere").unwrap().is_none());
    assert_eq!(r.journal().filter(|l| l.contains("host root")).count(), 1);
    // The same location unrooted is the whole app there.
    let whole = boot(ROUTES, named(Some("compose"), false), "/compose");
    assert_eq!(text(&whole, "where"), "inbox inbox,compose");
}

/// A launch on a booted app makes the router a boot at that location makes,
/// whatever it held, on visits no earlier one shares an id with.
#[test]
fn a_launch_on_a_booted_app_is_a_boot_at_its_location() {
    let mut r = boot(ROUTES, Launch::NONE, "/");
    r.act("follow", vec![Value::str("/compose")]).unwrap();
    r.act("pick", vec![Value::str("settings")]).unwrap();
    r.act("follow", vec![Value::str("/settings/general")])
        .unwrap();
    let ids = |r: &Runner<Source>| {
        let state: serde_json::Value =
            serde_json::from_str(&exact_runner::agent::state(r)).unwrap();
        let nav = &state["slots"]["nav"];
        let mut ids = Vec::new();
        for tab in nav["tabs"].as_array().unwrap() {
            for entry in tab["stack"].as_array().unwrap() {
                ids.push((
                    entry["id"].as_u64().unwrap(),
                    entry["url"].as_str().unwrap().to_owned(),
                ));
            }
        }
        ids
    };
    let before = ids(&r);
    assert!(r.host_launch("/m/4").unwrap().is_some());
    assert_eq!(text(&r, "where"), "inbox inbox,message");
    let cold = boot(ROUTES, Launch::NONE, "/m/4");
    let urls = |ids: Vec<(u64, String)>| ids.into_iter().map(|(_, url)| url).collect::<Vec<_>>();
    assert_eq!(urls(ids(&r)), urls(ids(&cold)));
    assert!(ids(&r)
        .iter()
        .all(|(id, _)| before.iter().all(|(old, _)| old != id)));
    r.act("pick", vec![Value::str("settings")]).unwrap();
    assert_eq!(text(&r, "where"), "settings settings");
    assert!(r.host_launch("/nowhere").unwrap().is_none());
    assert_eq!(text(&r, "where"), "settings settings");
    assert_eq!(r.journal().filter(|l| l.contains("host launch")).count(), 1);
    assert_eq!(
        r.journal()
            .filter(|l| l.contains("router launch refused"))
            .count(),
        1
    );
    // A window rooted at a screen launches rooted.
    let mut rooted = boot(ROUTES, named(Some("compose"), true), "/compose");
    assert!(rooted.host_launch("/settings/general").unwrap().is_some());
    assert_eq!(text(&rooted, "where"), "settings pane");
}

/// A dev reload carries a rooted window's router, a stack that starts at
/// its screen and only the tabs it selected, and launches rooted again
/// when the table no longer fits it.
#[test]
fn a_rooted_router_survives_a_reload() {
    let mut r = boot(ROUTES, named(Some("compose"), true), "/compose");
    r.act("follow", vec![Value::str("/m/4")]).unwrap();
    let carried = r.carry();
    let reload = |source: &str, rooted: bool| {
        let plan = contract::bake(contract::compile(source).unwrap(), boxed(NoData)).unwrap();
        launching_with(named(Some("compose"), rooted), || {
            Runner::boot_carrying(
                plan,
                boxed(NoData),
                Kernel::with_monospace(),
                &carried,
                Default::default(),
                "/compose",
            )
        })
        .unwrap()
    };
    assert_eq!(
        text(&reload(ROUTES, true), "where"),
        "inbox compose,message"
    );
    // Unrooted, the same value is no router this table makes: launched again at its top.
    assert_eq!(text(&reload(ROUTES, false), "where"), "inbox inbox,message");
    let renamed = ROUTES.replace("message \"/m/:id\"", "letter \"/m/:id\"");
    assert_eq!(text(&reload(&renamed, true), "where"), "inbox letter");
}
