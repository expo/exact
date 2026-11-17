//! The first window to show a host that booted with nothing showing it
//! (LLP 1133.017 D7) gives its URL or share as the launch location.
use super::*;
use exact_kernel::PropValue;
use exact_runner::{DataError, Value};
use std::cell::Cell;
use std::rc::Rc;

/// A module whose image is loaded once `loaded` says so.
struct NoData {
    loaded: Rc<Cell<bool>>,
}
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn preload(&self) -> Result<bool, DataError> {
        Ok(self.loaded.get())
    }
}

/// `navigate` pushes, so a location that arrives as a navigation stacks on
/// the tab it finds, where a launch selects the location's own.
const APP: &str = r#"routes nav
  tab inbox "/"
    message "/m/:id"
  tab settings "/settings"
    pane "/settings/:pane"
    share "/share"
component App
  derive here = top(nav)
  derive files = sharedFiles(here, "files")
  action follow(url: string)
    nav = push(nav, url)
  view
    column navigationKey=`${here.id}` navigate=follow
      text `${nav.tab} ${stack(nav).map(e => e.name).join(",")}` testId="where"
      text `${here.url} ${files.files.length} ${files.pending}` testId="share"
"#;

fn text(p: &Presenter<NoData>, id: &str) -> String {
    let k = p.host.kernel();
    let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap();
    node.props
        .iter()
        .find_map(|(prop, v)| match v {
            PropValue::Str(s) if prop.name() == "text" => Some(s.to_string()),
            _ => None,
        })
        .unwrap()
}

/// A host as Android boots one for a wake: hidden, with no frame displayed,
/// its data module active once `loaded` says its image is.
fn unshown(loaded: &Rc<Cell<bool>>) -> Presenter<NoData> {
    let (mut p, error) = Presenter::boot_with(
        &contract::compile(APP).unwrap().encode(),
        NoData {
            loaded: loaded.clone(),
        },
        (400., 500.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    p.page_execution(true, false);
    p.activate_unshown();
    assert_eq!(p.data_state(), i32::from(loaded.get()));
    assert_eq!(text(&p, "where"), "inbox inbox");
    p
}

/// A share of one text file to the app's `/share`, and the file.
fn share(name: &str) -> (serde_json::Value, PathBuf) {
    let source = std::env::temp_dir().join(format!(
        "exact-painted-launch-{name}-{}.txt",
        std::process::id()
    ));
    std::fs::write(&source, "shared").unwrap();
    let request = serde_json::json!({
        "target": {
            "action": "/share",
            "params": {"files": [{"name": "files", "accept": ["text/plain"]}]},
        },
        "share": {"files": [{"path": source, "name": "note.txt"}]},
    });
    (request, source)
}

/// The first frame a window is given shows the launch location.
fn shown(p: &mut Presenter<NoData>) {
    p.page_execution(false, true);
    let frame = p.display_frame().unwrap();
    assert!(p.display_complete(&frame));
}

#[test]
fn a_url_given_to_an_unshown_host_is_its_launch_location() {
    let mut p = unshown(&Rc::new(Cell::new(true)));
    assert_eq!(p.launch_location("/settings/general", 0.), None);
    assert_eq!(text(&p, "where"), "settings settings,pane");
    shown(&mut p);
    assert_eq!(text(&p, "where"), "settings settings,pane");
    // The other tab is at its root, as a boot leaves it.
    let state: serde_json::Value =
        serde_json::from_str(&exact_runner::agent::state(p.host.runner())).unwrap();
    let tabs = state["slots"]["nav"]["tabs"].as_array().unwrap();
    let urls = |tab: &serde_json::Value| {
        tab["stack"]
            .as_array()
            .unwrap()
            .iter()
            .map(|entry| entry["url"].as_str().unwrap().to_owned())
            .collect::<Vec<_>>()
    };
    assert_eq!(urls(&tabs[0]), ["/"]);
    assert_eq!(urls(&tabs[1]), ["/settings", "/settings/general"]);
    assert_eq!(
        p.launch_location("/nowhere", 0.).as_deref(),
        Some("launch /nowhere: no route matches it")
    );
}

#[test]
fn a_share_given_to_an_unshown_host_is_its_launch_location() {
    let mut p = unshown(&Rc::new(Cell::new(true)));
    let (request, source) = share("share");
    assert_eq!(p.launch_share(&request, 0.), Ok(()));
    assert_eq!(text(&p, "where"), "settings settings,share");
    let route = text(&p, "share");
    assert!(
        route.starts_with("/share?") && route.ends_with(" 1 0"),
        "the share's route shows its file landed: {route}"
    );
    shown(&mut p);
    assert_eq!(text(&p, "where"), "settings settings,share");
    let _ = std::fs::remove_file(source);
}

/// A window made while the module's image still loads: the launch location
/// is the share's at once, and its file lands with the activation.
#[test]
fn a_share_given_before_the_data_module_is_active_lands_with_it() {
    let loaded = Rc::new(Cell::new(false));
    let mut p = unshown(&loaded);
    let (request, source) = share("pending");
    assert_eq!(p.launch_share(&request, 0.), Ok(()));
    assert_eq!(text(&p, "where"), "settings settings,share");
    assert!(text(&p, "share").ends_with(" 0 1"), "{}", text(&p, "share"));
    loaded.set(true);
    assert!(p.pump(0.).is_none());
    assert_eq!(p.data_state(), 1);
    assert!(text(&p, "share").ends_with(" 1 0"), "{}", text(&p, "share"));
    let claims = |p: &Presenter<NoData>| {
        let logs = exact_runner::agent::logs(p.host.runner(), 0).to_string();
        logs.matches("share: received").count()
    };
    assert_eq!(claims(&p), 1, "the claim is journaled once");
    let _ = std::fs::remove_file(source);
}
