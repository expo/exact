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
