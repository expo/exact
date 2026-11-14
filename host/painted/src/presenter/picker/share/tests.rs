//! A share whose location is not delivered still lands its files, so no
//! token is left pending with nothing to finish it.
use super::*;
use exact_kernel::PropValue;
use exact_runner::share_in::PendingFile;
use exact_runner::{DataError, Value};
use std::cell::RefCell;

#[derive(Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
}

const APP: &str = r#"routes nav
  tab home "/"
    share "/share"
  notfound
component App
  derive here = top(nav)
  derive files = sharedFiles(here, "files")
  action follow(url: string)
    nav = go(nav, url)
  view
    column navigationKey=`${here.id}` navigate=follow
      text `${files.files.length} ${files.pending}` testId="counts"
"#;

fn counts(p: &Presenter<NoData>) -> String {
    let k = p.host.kernel();
    let node = k.node_by_key(k.find_by_test_id("counts")[0]).unwrap();
    node.props
        .iter()
        .find_map(|(prop, v)| match v {
            PropValue::Str(s) if prop.name() == "text" => Some(s.to_string()),
            _ => None,
        })
        .unwrap()
}

#[test]
fn a_share_whose_delivery_fails_still_lands_its_files() {
    let (mut p, error) = Presenter::boot_with(
        &contract::compile(APP).unwrap().encode(),
        NoData,
        (400., 500.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert_eq!(error, None);
    let source =
        std::env::temp_dir().join(format!("exact-painted-share-{}.txt", std::process::id()));
    std::fs::write(&source, "shared").unwrap();
    let target = ShareTarget {
        action: "/share".into(),
        title: None,
        text: None,
        url: None,
        files: vec![("files".into(), vec!["text/plain".into()])],
    };
    let share = Pending {
        files: vec![PendingFile {
            source: source.clone(),
            name: "note.txt".into(),
            mime: None,
            member: None,
            size: None,
        }],
        ..Pending::default()
    };
    let delivered = RefCell::new(String::new());
    let refused = p.receive(&target, share, Held::new(), |_, location| {
        *delivered.borrow_mut() = location.to_owned();
        Err("the route could not be opened".into())
    });
    assert_eq!(refused.unwrap_err(), "the route could not be opened");
    let root = p.host.kernel().roots()[0];
    let location = delivered.into_inner();
    assert!(p
        .host
        .dispatch_at(root, Event::Navigate(location), 0.)
        .is_none());
    assert_eq!(counts(&p), "1 0", "the file landed; none is pending");
    let _ = std::fs::remove_file(source);
}
