//! Each production export answers its operation as the dispatcher does.
use super::*;
use exact_plan::Value;
use exact_runner::DataError;

#[derive(Clone, Default)]
struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const APP: &str = r#"component App
  state n = 0
  action add
    n = n + 1
  view
    column
      button press=add testId="add"
        text `${n}`
"#;

fn booted() -> Bridge<NoData> {
    let plan = contract::compile(APP).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, NoData, Hooks::none(), 400., 800.);
    bridge
}

type Export = fn(&mut Bridge<NoData>, usize) -> u32;

fn reply(bridge: &mut Bridge<NoData>, request: &str, f: Export) -> String {
    let len = bridge.input_write(request.as_bytes());
    let n = f(bridge, len);
    String::from_utf8(bridge.output_bytes(n as usize).to_vec()).unwrap()
}

#[test]
fn each_production_export_answers_as_the_dispatcher() {
    // The picker's and the app files' replies read the roots the last app to
    // configure storage left process-wide (`picker::ROOTS`), which parallel
    // tests change: the comparison runs alone in a child process.
    if std::env::var("EXACT_ANSWERS_PARITY").as_deref() != Ok("1") {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "abi::answers::tests::each_production_export_answers_as_the_dispatcher",
                "--test-threads=1",
            ])
            .env("EXACT_ANSWERS_PARITY", "1")
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&result.stdout);
        assert!(
            result.status.success() && out.contains("1 passed"),
            "{out}{}",
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    let view = {
        let bridge = booted();
        let kernel = bridge.host.as_ref().unwrap().runner().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id("add")[0])
            .unwrap()
            .id
    };
    let node = format!(r#"{{"op":"node","id":{view}}}"#);
    let planned = format!(r#"{{"op":"node","id":{view},"plan":true}}"#);
    let cases: [(&str, Export); 21] = [
        (r#"{"op":"outstanding"}"#, Bridge::outstanding),
        (r#"{"op":"background"}"#, Bridge::must_finish),
        (r#"{"op":"actions","drain":true}"#, Bridge::ended_actions),
        (r#"{"op":"declared"}"#, Bridge::declared),
        (r#"{"op":"tree"}"#, Bridge::tree),
        (r#"{"op":"state"}"#, Bridge::state),
        (&node, Bridge::node),
        (&planned, Bridge::node),
        (r#"{"op":"perf"}"#, Bridge::perf),
        (r#"{"op":"logs","since":0}"#, Bridge::logs),
        (r#"{"op":"tags"}"#, Bridge::tags),
        (r#"{"op":"picker","id":"none"}"#, Bridge::picker),
        (r#"{"op":"showPicker","id":"none"}"#, Bridge::show_picker),
        (r#"{"op":"pickedPath","name":"a.txt"}"#, Bridge::picked_path),
        (r#"{"op":"appRoots"}"#, Bridge::app_roots),
        (
            r#"{"op":"shareFile","path":"app:/data/none.txt","to":"/nonexistent/none.txt"}"#,
            Bridge::share_file,
        ),
        (r#"{"op":"keptDocuments","owner":1}"#, Bridge::documents),
        (
            r#"{"op":"frameRequest","node":1,"capability":"dialog","args":{}}"#,
            Bridge::frame_request,
        ),
        (r#"{"op":"called","id":1}"#, Bridge::tool),
        (r#"{"op":"call","args":}"#, Bridge::tool),
        (
            r#"{"op":"tap","ticket":7,"choice":"cancel"}"#,
            Bridge::answer_ticket,
        ),
    ];
    // An operation that changes what the runner holds (a drain, a hold, an
    // answer, a picked file's number) is compared on two bridges booted
    // alike; a read, on one.
    let changes = |request: &str| {
        [
            "actions",
            "showPicker",
            "pickedPath",
            "tap",
            "frameRequest",
            "called",
            "call",
        ]
        .iter()
        .any(|op| request.contains(&format!("\"op\":\"{op}\"")))
    };
    for (request, export) in cases {
        if changes(request) {
            let general = reply(&mut booted(), request, Bridge::agent);
            assert_eq!(reply(&mut booted(), request, export), general, "{request}");
        } else {
            let mut bridge = booted();
            let general = reply(&mut bridge, request, Bridge::agent);
            assert_eq!(reply(&mut bridge, request, export), general, "{request}");
        }
    }
}

/// A drag's open through the export's own call (`exact_share_file`'s
/// `shareOpen`, LLP 1094.000 D3): a booted app's granted file, opened from
/// the roots its activation pinned, answered as a descriptor the caller then
/// owns; a file no grant covers answers an error and none. Roots are
/// process-wide (`picker::ROOTS`), so it runs alone in a child, under a
/// scratch `HOME`.
#[cfg(unix)]
#[test]
fn the_share_export_opens_a_granted_app_file_for_a_drag() {
    use std::io::Read;
    use std::os::fd::FromRawFd;
    const CHILD: &str = "EXACT_SHARE_OPEN_EXPORT_TEST";
    if std::env::var_os(CHILD).is_none() {
        let home = crate::TempDir(
            std::env::temp_dir().join(format!("exact-share-open-{}", std::process::id())),
        );
        let _ = std::fs::remove_dir_all(&home);
        std::fs::create_dir_all(&home).unwrap();
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "abi::answers::tests::the_share_export_opens_a_granted_app_file_for_a_drag",
            ])
            .env(CHILD, "1")
            .env("HOME", &*home)
            .env_remove("EXACT_AGENT")
            .output()
            .unwrap();
        let out = String::from_utf8_lossy(&output.stdout);
        assert!(
            output.status.success() && out.contains("1 passed"),
            "{out}{}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }
    #[derive(Clone, Default)]
    struct Granted;
    impl DataSource for Granted {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn app_id(&self) -> &str {
            "test.exact.drag-open"
        }
        fn grants(&self) -> &str {
            "fs.read app:/data"
        }
    }
    let plan = contract::compile(APP).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Granted, Hooks::none(), 400., 800.);
    // Activation configures the app's storage and pins its roots, as after a
    // session's first pixel.
    bridge.data_ready();
    let (roots, _) = crate::picker::app_dirs("test.exact.drag-open")
        .unwrap()
        .unwrap();
    std::fs::write(roots[0].join("notes.txt"), b"the backup").unwrap();
    std::fs::write(roots[1].join("x"), b"x").unwrap();
    let mut ask = |request: &str| {
        let len = bridge.input_write(request.as_bytes());
        let n = bridge.share_file(len);
        String::from_utf8(bridge.output_bytes(n as usize).to_vec()).unwrap()
    };
    let answer = ask(r#"{"op":"shareOpen","path":"app:/data/notes.txt"}"#);
    let fd = exact_runner::json::field_num(&answer, "fd").unwrap_or_else(|| panic!("{answer}"));
    let mut text = String::new();
    // SAFETY: the reply's descriptor is newly opened and this test's alone.
    #[allow(unsafe_code)]
    let file = unsafe { std::fs::File::from_raw_fd(fd as i32) };
    { file }.read_to_string(&mut text).unwrap();
    assert_eq!(text, "the backup");
    let refused = ask(r#"{"op":"shareOpen","path":"app:/cache/x"}"#);
    assert!(
        refused.contains("\"error\"") && !refused.contains("\"fd\""),
        "{refused}"
    );
}
