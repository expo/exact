//! A drag out of the app (LLP 1094.000): `draggable` and `dataTransfer=`
//! lower to props the host holds before the gesture (D1, D3), and the runner
//! rewrites a commit's files and `url` to what the host carries, clearing a
//! prop left with nothing and journaling each refusal as the prop is
//! written (D2).

use exact_kernel::{Kernel, PropId};
use exact_plan::Value;
use exact_runner::page::Platform;
use exact_runner::{DataError, DataSource, Event, Runner};
use std::path::Path;

struct Grants(&'static str);

impl DataSource for Grants {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        self.0
    }
}

fn boot(platform: Platform, grants: &'static str) -> Runner<Grants> {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/drag-out.contract");
    boot_source(&std::fs::read_to_string(path).unwrap(), platform, grants)
}

fn boot_source(src: &str, platform: Platform, grants: &'static str) -> Runner<Grants> {
    let plan = contract::compile(src).unwrap();
    let plan = contract::bake(plan, Grants(grants)).unwrap();
    platform.boot(|| {
        Runner::boot(
            plan,
            Grants(grants),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    })
}

fn view(r: &Runner<Grants>, test_id: &str) -> u32 {
    let key = r.kernel().find_by_test_id(test_id)[0];
    r.kernel().node_by_key(key).unwrap().id
}

/// `(draggable, text, url, files)` of a node, as the host reads them.
fn drag(r: &Runner<Grants>, test_id: &str) -> (Option<bool>, String, String, String) {
    let key = r.kernel().find_by_test_id(test_id)[0];
    let props = r.kernel().node_by_key(key).unwrap().props;
    let text = |p| props.str(p).unwrap_or_default().to_string();
    (
        props.bool(PropId::Draggable),
        text(PropId::DataTransferText),
        text(PropId::DataTransferUrl),
        text(PropId::DataTransferFiles),
    )
}

/// The prop a node holds, or none where the runner cleared it.
fn held(r: &Runner<Grants>, test_id: &str, prop: PropId) -> Option<String> {
    let key = r.kernel().find_by_test_id(test_id)[0];
    let props = r.kernel().node_by_key(key).unwrap().props;
    props.str(prop).map(str::to_string)
}

/// The journal's drag refusals, without their clock stamps.
fn refusals(r: &Runner<Grants>) -> Vec<String> {
    r.journal()
        .filter_map(|l| l.find("drag: refused").map(|at| l[at..].to_string()))
        .collect()
}

#[test]
fn each_argument_binds_its_prop_and_a_bound_draggable_follows_state() {
    let mut r = boot(Platform::Ios, "fs.read app:/data\n");
    let card = drag(&r, "card-1016");
    assert_eq!(
        card,
        (Some(true), "".into(), "".into(), "assets/1016.jpg".into())
    );
    let row = (
        "Mount Fuji".to_string(),
        "https://example.com/landmarks/1016".to_string(),
    );
    assert_eq!(
        drag(&r, "row"),
        (Some(true), row.0.clone(), row.1.clone(), "".into())
    );
    assert_eq!(drag(&r, "export").3, "app:/data/export.pdf");
    r.dispatch(view(&r, "edit"), Event::Press).unwrap();
    assert_eq!(drag(&r, "row"), (Some(false), row.0, row.1, "".into()));
    assert!(refusals(&r).is_empty(), "{:?}", refusals(&r));
    let plan = contract::compile(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../corpus/drag-out.contract"),
        )
        .unwrap(),
    )
    .unwrap();
    let still = contract::compile("component A\n  view\n    text \"a\"\n").unwrap();
    use exact_runner::uses::{uses, Capability};
    assert!(uses(&plan).has(Capability::DragOut) && !uses(&still).has(Capability::DragOut));
}

#[test]
fn the_runner_admits_files_by_the_grants_and_the_host_reach_and_journals_a_refusal_as_it_writes_the_prop(
) {
    // No grant covers the export: refused on every host, the shipped file kept, the emptied prop cleared.
    let mut r = boot(Platform::Macos, "");
    assert_eq!(held(&r, "export", PropId::DataTransferFiles), None);
    assert_eq!(drag(&r, "card-1016").3, "assets/1016.jpg");
    assert_eq!(
        refusals(&r),
        ["drag: refused: app:/data/export.pdf is outside the app's fs.read grants"]
    );
    // A commit that leaves the export's files alone journals nothing again.
    r.dispatch(view(&r, "edit"), Event::Press).unwrap();
    assert_eq!(refusals(&r).len(), 1);
    // The browser cannot read an `app:/` file inside `dragstart`, granted or not.
    let web = boot(Platform::Web, "fs.read app:/data\n");
    assert_eq!(drag(&web, "export").3, "");
    assert_eq!(
        refusals(&web),
        ["drag: refused: app:/data/export.pdf: an app:/ file cannot be dragged on the web"]
    );
    // A host with no drag admits what the rule admits, so `tree` shows the declaration.
    let linux = boot(Platform::Linux, "fs.read app:/data\n");
    assert_eq!(drag(&linux, "export").3, "app:/data/export.pdf");
}

#[test]
fn bound_entries_and_urls_are_refused_by_the_same_rules() {
    let src = "component A\n  state files = [\"assets/a.jpg\", \"img/x.jpg\", \"app:/data/../x.pdf\"]\n  state link = \"/card/1\"\n  view\n    column draggable=true dataTransfer=DataTransfer(files=files, url=link) testId=\"odd\"\n      text \"a\"\n";
    let r = boot_source(src, Platform::Ios, "fs.read app:/data\n");
    assert_eq!(
        held(&r, "odd", PropId::DataTransferFiles).as_deref(),
        Some("assets/a.jpg")
    );
    assert_eq!(held(&r, "odd", PropId::DataTransferUrl), None);
    assert_eq!(
        refusals(&r),
        [
            "drag: refused: img/x.jpg is neither a shipped assets/ file nor an app:/ path",
            "drag: refused: app:/data/../x.pdf is not a plain app:/ path",
            "drag: refused: /card/1 is not an absolute http: or https: URL",
        ]
    );
}

#[test]
fn each_refusal_names_what_to_write_instead() {
    let node = |attrs: &str| {
        format!("component A\n  state n = 0\n  state dx = 0\n  action go\n    n = 1\n  action moved(x: number, y: number)\n    dx = x\n  view\n    column {attrs}\n      text \"a\"\n")
    };
    let refused = |src: &str| contract::compile(src).err().map(|e| (e.id, e.message));
    for (attrs, id, says) in [
        (
            "draggable=true dataTransfer=DataTransfer(title=\"a\")",
            "type-data-transfer",
            "has no argument `title`",
        ),
        (
            "draggable=true dataTransfer=DataTransfer()",
            "type-data-transfer",
            "needs `text=`, `url=` or `files=`",
        ),
        (
            "draggable=true dataTransfer=DataTransfer(files=[])",
            "type-data-transfer",
            "`files=[]` carries nothing",
        ),
        (
            "draggable=true dataTransfer=DataTransfer(url=\"/card/1\")",
            "type-data-transfer",
            "another app cannot open this app's path",
        ),
        (
            "draggable=true dataTransfer=DataTransfer(files=[\"https://example.com/a.jpg\"])",
            "type-data-transfer",
            "a remote image's address goes in `url=`",
        ),
        (
            "draggable=true dataTransfer=\"a\"",
            "type-data-transfer",
            "`dataTransfer` takes",
        ),
        (
            "draggable=true dataTransfer=DataTransfer(files=[(true ? \"assets/a.jpg\" : \"#b\")])",
            "type-data-transfer",
            "a picture of an element is `share`'s",
        ),
        ("drag=go", "lower-handler-later", "`drag` is not heard yet"),
        (
            "dragend=go",
            "lower-handler-later",
            "`dragend` is not heard yet",
        ),
    ] {
        let (got, message) = refused(&node(attrs)).unwrap_or_default();
        assert!(
            got == id && message.contains(says),
            "{attrs}: {got} {message}"
        );
    }
    let input =
        "component A\n  view\n    input draggable=true dataTransfer=DataTransfer(text=\"a\")\n";
    let (id, message) = refused(input).unwrap();
    assert_eq!(id, "lower-attr-tag");
    assert!(
        message.contains("a text field drags its own selected text"),
        "{message}"
    );
    assert!(refused(&node("draggable=false")).is_none());
    // `draggable=false` drags nothing, so a gesture beside it claims nothing.
    assert!(refused(&node("draggable=false pan=moved")).is_none());
    let within = |body: &str| format!("component A\n  view\n    {body}\n");
    for (body, id, says) in [
        ("scroll draggable=true dataTransfer=DataTransfer(text=\"a\")\n      text \"a\"", "lower-attr-tag", "a scroller's drags scroll it"),
        ("text\n      text \"a\"\n      image \"assets/a.png\" draggable=true dataTransfer=DataTransfer(text=\"a\")", "lower-attr-tag", "a run of text is part of its paragraph"),
        ("list appearance=\"auto\"\n      section\n        header\n          link href=\"/a\" draggable=true dataTransfer=DataTransfer(text=\"a\")\n            text \"a\"\n        column\n          text \"r\"", "lower-draggable-grouped", "nothing inside them drags out"),
    ] {
        let (got, message) = refused(&within(body)).unwrap_or_default();
        assert!(got == id && message.contains(says), "{body}: {got} {message}");
    }
    // A component is inlined before a grouped list's sections are read: the first component is the root.
    let component = "component A\n  view\n    list appearance=\"auto\"\n      section\n        column\n          Row()\ncomponent Row\n  view\n    link href=\"/a\" draggable=true dataTransfer=DataTransfer(text=\"a\")\n      text \"a\"\n";
    assert_eq!(
        refused(component).unwrap_or_default().0,
        "lower-draggable-grouped"
    );
    assert!(refused(&within("list appearance=\"auto\"\n      section\n        column\n          link href=\"/a\" draggable=false\n            text \"a\"")).is_none());
    // A literal shipped file the app does not ship, checked against its assets.
    let dir = std::env::temp_dir().join(format!("exact-contract-drag-{}", std::process::id()));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    std::fs::write(dir.join("assets/here.jpg"), b"x").unwrap();
    let files = "draggable=true dataTransfer=DataTransfer(files=[\"assets/here.jpg\", \"assets/gone.jpg\"])";
    std::fs::write(dir.join("app.contract"), node(files)).unwrap();
    let e = contract::compile_path(&dir.join("app.contract")).unwrap_err();
    assert_eq!((e.id.as_str(), e.message.as_str()), ("lower-draggable-unreadable", "`assets/gone.jpg` is not a file this app ships. Name a file under `assets/`, as an `image` would"));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn expect_drag_reads_each_form_and_names_them_when_it_cannot() {
    let src = "test \"drag\"\n  expect drag \"card\" has \"image/jpeg\" name \"1016-thumb.jpg\"\n  expect drag \"row\" has \"text/plain\" == \"Mount Fuji\"\n  expect drag \"card\" missing \"text/plain\"\n  expect drag \"off\" none\n";
    let json = contract::tests_json(&contract::tests(src).unwrap());
    for want in [
        r#"{"op":"expect-drag","target":"card","present":true,"kind":"image/jpeg","name":"1016-thumb.jpg","line":2}"#,
        r#"{"op":"expect-drag","target":"row","present":true,"kind":"text/plain","value":"Mount Fuji","line":3}"#,
        r#"{"op":"expect-drag","target":"card","present":false,"kind":"text/plain","line":4}"#,
        r#"{"op":"expect-drag","target":"off","present":false,"line":5}"#,
    ] {
        assert!(json.contains(want), "{want}\n{json}");
    }
    for (bad, id) in [
        ("\"card\"", "syntax-expected-step"),
        ("\"card\" carries \"text/plain\"", "syntax-expected-step"),
        ("card none", "syntax-expected-string"),
    ] {
        let error = contract::tests(&format!("test \"drag\"\n  expect drag {bad}\n")).unwrap_err();
        assert_eq!(error.id, id, "{bad}");
    }
    // `missing` takes no value: what follows is not a step.
    assert!(contract::tests(
        "test \"drag\"\n  expect drag \"card\" missing \"text/plain\" == \"a\"\n"
    )
    .is_err());
}
