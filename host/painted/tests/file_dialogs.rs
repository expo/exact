//! Windows' typed dialog handoff, exercised without opening native UI.
use exact_painted::{
    presenter::{
        file_dialog::{FileDialog, Outcome, StartFolder},
        PainterChoice,
    },
    Presenter,
};
use exact_plan::Value;
use exact_runner::file_pickers::Kind;
use std::path::PathBuf;

fn plan(start: &str) -> Vec<u8> {
    contract::compile(&format!(
        r#"component App
  state start = {}
  state chosen = ""
  state cancels = 0
  action changed(value)
    chosen = value
  action cancelled
    cancels = cancels + 1
  action open
    showOpenFilePicker("result", true, startIn=start)
  action directory
    showDirectoryPicker("result", startIn=start)
  action save
    showSaveFilePicker("result", "Shot.png", startIn=start)
  action export
    saveFile("result", text="saved bytes", suggestedName="export.txt", startIn=start)
  action import
    showPicker("attach")
  view
    column
      input id="result" display="none" change=changed cancel=cancelled
      input type="file" id="attach" display="none"
      button testId="open" press=open
        text "Open"
      button testId="directory" press=directory
        text "Choose folder"
      button testId="save" press=save
        text "Save as"
      button testId="export" press=export
        text "Export"
      button testId="import" press=import
        text "Import"
"#,
        serde_json::to_string(start).unwrap()
    ))
    .unwrap()
    .encode()
}

fn presenter(start: &str) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &plan(start),
        (),
        (400., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn tap(p: &mut Presenter<()>, target: &str) {
    let kernel = p.host().kernel();
    let id = kernel
        .node_by_key(kernel.find_by_test_id(target)[0])
        .unwrap()
        .id;
    p.tap(id).unwrap();
    p.run_commands(|| ());
}

fn take(p: &mut Presenter<()>) -> (u32, FileDialog) {
    let mut asks = p.take_file_dialogs();
    assert_eq!(
        asks.len(),
        1,
        "{:?}",
        p.host().runner().journal().collect::<Vec<_>>()
    );
    assert!(p.take_file_dialogs().is_empty());
    let (id, dialog, _) = asks.remove(0);
    (id, dialog)
}

#[test]
fn documents_exports_and_imports_take_their_routes_and_a_headless_export_refuses() {
    let mut p = presenter("screenshots");
    tap(&mut p, "export");
    assert!(p.take_file_dialogs().is_empty());
    assert_eq!(p.host().runner().slot("cancels"), Some(&Value::Number(1.)));
    p.serve_file_dialogs();
    let folder = std::env::temp_dir().join(format!("exact-dialog-route-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let source = folder.join("opened.txt");
    std::fs::write(&source, b"original").unwrap();
    for (target, command, paths) in [
        (
            "open",
            "showOpenFilePicker",
            vec![source.clone(), source.clone()],
        ),
        ("directory", "showDirectoryPicker", vec![folder.clone()]),
        ("save", "showSaveFilePicker", vec![folder.join("Shot.png")]),
    ] {
        tap(&mut p, target);
        let (id, request) = take(&mut p);
        assert_eq!(request.command, command);
        assert_eq!(request.picker.multiple, target == "open");
        assert_eq!(
            request.start_folder(),
            Ok(Some(StartFolder::Known("screenshots".into())))
        );
        if target == "save" {
            assert_eq!(request.picker.suggested_name, "Shot.png");
        }
        p.answer_file_dialog(id, Outcome::Chosen(paths.clone()));
        let docs = p.host().runner().slot("chosen").unwrap().as_str().unwrap();
        assert_eq!(docs.lines().count(), paths.len());
        for doc in docs.lines() {
            assert!(doc.starts_with("doc:/"), "{doc}");
        }
    }
    tap(&mut p, "export");
    let (id, request) = take(&mut p);
    assert_eq!(request.command, "saveFile");
    assert_eq!(request.picker.kind, Kind::Save);
    let target = folder.join("saved.txt");
    p.answer_file_dialog(id, Outcome::Chosen(vec![target.clone()]));
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    while p.host().runner().slot("chosen") != Some(&Value::str("saved.txt")) {
        assert!(p.pump(0.).is_none());
        assert!(
            std::time::Instant::now() < deadline,
            "export did not finish"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    assert_eq!(std::fs::read(&target).unwrap(), b"saved bytes");
    assert_eq!(
        p.host().runner().slot("chosen"),
        Some(&Value::str("saved.txt"))
    );
    for outcome in [Outcome::Cancelled, Outcome::Refused("no dialog".into())] {
        tap(&mut p, "directory");
        let (id, _) = take(&mut p);
        p.answer_file_dialog(id, outcome);
    }
    assert_eq!(p.host().runner().slot("cancels"), Some(&Value::Number(3.)));
    tap(&mut p, "import");
    let (_, request) = take(&mut p);
    assert_eq!(request.command, "picker");
    assert_eq!(request.picker.kind, Kind::Open);
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn start_in_accepts_names_and_held_documents_and_stale_answers_are_dropped() {
    let mut p = presenter("pictures");
    p.serve_file_dialogs();
    tap(&mut p, "open");
    let (id, mut request) = take(&mut p);
    for start in exact_runner::file_pickers::WELL_KNOWN {
        request.picker.start_in = Some(start.into());
        assert_eq!(
            request.start_folder(),
            Ok(Some(StartFolder::Known(start.into())))
        );
    }
    let folder = std::env::temp_dir().join(format!("exact-dialog-start-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let doc = exact_data::documents::mint(&folder, 0).unwrap();
    request.picker.start_in = Some(doc);
    assert_eq!(
        request.start_folder(),
        Ok(Some(StartFolder::Chosen(
            std::fs::canonicalize(&folder).unwrap()
        )))
    );
    request.picker.start_in = Some("doc:/999999999/x".into());
    assert_eq!(request.start_folder().unwrap_err(), "showOpenFilePicker: startIn: refused: doc:/999999999/x is not a document this session holds");
    p.reload(&plan("pictures"), ()).unwrap();
    p.answer_file_dialog(id, Outcome::Chosen(vec![folder.clone()]));
    assert_eq!(p.host().runner().slot("chosen"), Some(&Value::str("")));
    let mut unknown = presenter("unknown");
    unknown.serve_file_dialogs();
    tap(&mut unknown, "open");
    assert!(take(&mut unknown).1.picker.start_in.is_none());
    let logs = unknown
        .host()
        .runner()
        .journal()
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        logs.contains("showOpenFilePicker: startIn: refused: \"unknown\""),
        "{logs}"
    );
    std::fs::remove_dir_all(folder).unwrap();
}

#[test]
fn document_dialogs_offer_the_declared_types_and_a_bad_record_refuses() {
    use exact_painted::presenter::file_dialog::FileType;
    let mut p = presenter("documents");
    p.serve_file_dialogs();
    p.set_delivery_from_compat(
        r#"{"inputs":{"fileHandlers":[
          {"action":"/","name":"Documents","accept":{"application/pdf":[".pdf"]}},
          {"action":"/","accept":{"application/json":[]}},
          {"action":"/","name":"Folder","accept":{"inode/directory":[]}}
        ]}}"#,
    );
    let declared = vec![
        FileType {
            name: Some("Documents".into()),
            suffixes: vec![".pdf".into()],
            mimes: vec!["application/pdf".into()],
        },
        FileType {
            name: None,
            suffixes: Vec::new(),
            mimes: vec!["application/json".into()],
        },
    ];
    for (target, types) in [
        ("open", declared.clone()),
        ("save", declared),
        ("directory", Vec::new()),
        ("export", Vec::new()),
        ("import", Vec::new()),
    ] {
        tap(&mut p, target);
        let (id, request) = take(&mut p);
        assert_eq!(request.types, types, "{target}");
        p.answer_file_dialog(id, Outcome::Cancelled);
    }
    p.set_delivery_from_compat(
        r#"{"inputs":{"fileHandlers":[{"action":"/","accept":{"text/markdown":[".MD"]}}]}}"#,
    );
    let cancels = p.host().runner().slot("cancels").cloned();
    tap(&mut p, "open");
    assert!(p.take_file_dialogs().is_empty());
    assert_ne!(p.host().runner().slot("cancels").cloned(), cancels);
    let journal = p.host().runner().journal().collect::<Vec<_>>().join("\n");
    assert!(
        journal.contains("showOpenFilePicker: refused: file handler 0's extension 0 is invalid"),
        "{journal}"
    );
}
