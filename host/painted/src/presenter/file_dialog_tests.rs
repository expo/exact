use super::*;
use exact_plan::Value;

fn presenter() -> (Presenter<()>, std::sync::mpsc::Receiver<()>) {
    let plan = crate::test_plan(
        r#"component App
  state result = ""
  state files: Picked[] = []
  action changed(value: string)
    result = value
  action imported(value: Picked[])
    files = value
  view
    column
      input id="result" change=changed
      input id="files" type="file" multiple=true change=imported
"#,
    );
    let mut p = Presenter::boot_with(
        &plan,
        (),
        (400., 300.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap()
    .0;
    let (wake, woken) = std::sync::mpsc::channel();
    p.executor = crate::executor::Executor::start_notifying("", move || {
        let _ = wake.send(());
    });
    (p, woken)
}

fn directory() -> PathBuf {
    static NEXT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);
    let path = std::env::current_dir()
        .unwrap()
        .join("target/507-tmp")
        .join(format!(
            "dialog-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed),
        ));
    std::fs::create_dir_all(&path).unwrap();
    path
}

fn request_save(p: &mut Presenter<()>, from: &str, text: Option<&str>) {
    let view = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_id("result")[0])
        .unwrap()
        .id;
    let save = SaveFile {
        id: "result".into(),
        from: from.into(),
        suggested_name: "export.txt".into(),
        text: text.map(str::to_owned),
        start_in: None,
    };
    p.queue_file_dialog(
        FileDialog {
            command: "saveFile",
            picker: Picker {
                kind: Kind::Save,
                id: "result".into(),
                multiple: false,
                suggested_name: "export.txt".into(),
                start_in: None,
            },
        },
        view,
        Route::Export(save),
    );
}

fn queue_save(p: &mut Presenter<()>, from: &str, text: Option<&str>) -> u32 {
    request_save(p, from, text);
    p.take_file_dialogs().pop().unwrap().0
}

fn settle(p: &mut Presenter<()>, woken: &std::sync::mpsc::Receiver<()>, id: u32) {
    while p.requests.waiting.contains_key(&id) {
        woken
            .recv_timeout(std::time::Duration::from_secs(5))
            .expect("file work did not finish");
        assert!(p.pump(0.).is_none());
    }
}

fn same_file(hard_link: bool) {
    let (mut p, _) = presenter();
    let dir = directory();
    let from = format!(
        "app:/tmp/507-same-{}.txt",
        dir.file_name().unwrap().to_str().unwrap()
    );
    let source = crate::picker::resolve(&from).unwrap();
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"keep these bytes").unwrap();
    let alias = source.with_extension("alias");
    let to = if hard_link {
        std::fs::hard_link(&source, &alias).unwrap();
        &alias
    } else {
        &source
    };
    let view = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_id("result")[0])
        .unwrap()
        .id;
    p.save_chosen(view, to, &from, None);
    assert_eq!(std::fs::read(&source).unwrap(), b"keep these bytes");
    if hard_link {
        std::fs::remove_file(alias).unwrap();
    }
    std::fs::remove_file(source).unwrap();
    std::fs::remove_dir(dir).unwrap();
}

#[test]
fn exporting_to_the_source_preserves_its_bytes() {
    same_file(false);
}
#[test]
fn exporting_to_a_hard_link_preserves_its_bytes() {
    same_file(true);
}

#[cfg(unix)]
#[test]
fn overwriting_an_export_preserves_the_destination_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = directory();
    let to = dir.join("saved.txt");
    std::fs::write(&to, b"old bytes that must be truncated").unwrap();
    std::fs::set_permissions(&to, std::fs::Permissions::from_mode(0o640)).unwrap();
    Export::Text("new".into()).write(&to).unwrap();
    assert_eq!(std::fs::read(&to).unwrap(), b"new");
    assert_eq!(
        std::fs::metadata(&to).unwrap().permissions().mode() & 0o777,
        0o640
    );
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn exporting_through_a_symlink_writes_its_target() {
    let dir = directory();
    let target = dir.join("target.txt");
    let to = dir.join("saved.txt");
    std::fs::write(&target, b"old bytes that must be truncated").unwrap();
    std::os::unix::fs::symlink(&target, &to).unwrap();
    Export::Text("new".into()).write(&to).unwrap();
    assert_eq!(std::fs::read(&target).unwrap(), b"new");
    assert!(std::fs::symlink_metadata(&to).unwrap().is_symlink());
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn shutdown_drains_a_blocked_export_and_reports_failures() {
    use std::io::Write;
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let (mut p, _) = presenter();
    let dir = directory();
    let to = dir.join("saved.txt");
    let id = queue_save(&mut p, "", Some("unused"));
    let (source, mut sender) = UnixStream::pair().unwrap();
    let requests::Waiting::FileDialog(wait) = p.requests.waiting.get_mut(&id).unwrap() else {
        panic!("no dialog");
    };
    wait.source = Some(Export::File(std::fs::File::from(OwnedFd::from(source))));
    p.answer_file_dialog(id, Outcome::Chosen(vec![to.clone()]));
    p.picks_replaced();
    let (returned, return_seen) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        // Keep the source blocked until shutdown either returns or waits for its worker.
        let returned_early = return_seen
            .recv_timeout(std::time::Duration::from_millis(200))
            .is_ok();
        sender.write_all(b"accepted export").unwrap();
        returned_early
    });
    let failures = p.drain_file_exports();
    returned.send(()).ok();
    let returned_early = writer.join().unwrap();
    assert!(!returned_early, "shutdown abandoned an accepted export");
    assert!(failures.is_empty(), "{failures:?}");
    assert_eq!(std::fs::read(&to).unwrap(), b"accepted export");
    assert!(!p.requests.waiting.contains_key(&id));

    let id = queue_save(&mut p, "", Some("cannot save"));
    p.answer_file_dialog(id, Outcome::Chosen(vec![dir.join("absent/saved.txt")]));
    let failures = p.drain_file_exports();
    assert_eq!(failures.len(), 1);
    assert!(failures[0].starts_with("saveFile: refused:"));
    assert!(p
        .host
        .runner()
        .journal()
        .any(|line| line.contains(&failures[0])));
    assert!(!p.requests.waiting.contains_key(&id));
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn export_keeps_the_file_opened_before_the_dialog() {
    let (mut p, woken) = presenter();
    let dir = directory();
    let from = format!(
        "app:/tmp/507-snapshot-{}.txt",
        dir.file_name().unwrap().to_str().unwrap()
    );
    let source = crate::picker::resolve(&from).unwrap();
    std::fs::create_dir_all(source.parent().unwrap()).unwrap();
    std::fs::write(&source, b"original").unwrap();
    let id = queue_save(&mut p, &from, None);
    let old = source.with_extension("old");
    std::fs::rename(&source, &old).unwrap();
    std::fs::write(&source, b"replacement").unwrap();
    let to = dir.join("saved.txt");
    p.answer_file_dialog(id, Outcome::Chosen(vec![to.clone()]));
    settle(&mut p, &woken, id);
    assert_eq!(std::fs::read(&to).unwrap(), b"original");
    std::fs::remove_file(source).unwrap();
    std::fs::remove_file(old).unwrap();
    std::fs::remove_dir_all(dir).unwrap();
}

// APFS refuses this name, so the selection is synthetic on a Mac.
#[cfg(unix)]
fn non_unicode(dir: &std::path::Path) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    dir.join(std::ffi::OsString::from_vec(b"bad-\xff.txt".to_vec()))
}
#[cfg(windows)]
fn non_unicode(dir: &std::path::Path) -> PathBuf {
    use std::os::windows::ffi::OsStringExt;
    dir.join(std::ffi::OsString::from_wide(&[
        0xd800, 0x2e, 0x74, 0x78, 0x74,
    ]))
}

#[test]
fn a_non_unicode_import_keeps_other_files_and_reports_the_refusal() {
    let (mut p, woken) = presenter();
    let dir = directory();
    let good = dir.join("good.txt");
    let bad = non_unicode(&dir);
    std::fs::write(&good, b"good").unwrap();
    let view = p.host.runner().picker("files").unwrap().view;
    p.borrowed_files_chosen(77, view, vec![good.clone(), bad.clone()]);
    settle(&mut p, &woken, 77);
    let files = p.host.runner().slot("files").unwrap();
    let Value::List(files) = files else {
        panic!("{files:?}")
    };
    assert_eq!(files.len(), 1);
    assert!(p
        .host
        .runner()
        .journal()
        .any(|l| l.contains("picker: refused:") && l.contains("Unicode")));
    assert_eq!(std::fs::read(good).unwrap(), b"good");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn a_non_unicode_document_is_reported_without_dropping_other_documents() {
    let (mut p, _) = presenter();
    let dir = directory();
    let good = dir.join("good.txt");
    let bad = non_unicode(&dir);
    std::fs::write(&good, b"good").unwrap();
    let view = p
        .host
        .kernel()
        .node_by_key(p.host.kernel().find_by_id("result")[0])
        .unwrap()
        .id;
    p.documents_chosen(
        view,
        "showOpenFilePicker",
        [good.as_path(), bad.as_path()].into_iter(),
    );
    assert!(p
        .host
        .runner()
        .slot("result")
        .unwrap()
        .as_str()
        .unwrap()
        .ends_with("/good.txt"));
    assert!(p
        .host
        .runner()
        .journal()
        .any(|l| l.contains("showOpenFilePicker: refused:") && l.contains("Unicode")));
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn a_blocked_export_leaves_the_presenter_thread_free() {
    let (mut p, woken) = presenter();
    let dir = directory();
    use std::io::Write;
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let to = dir.join("saved.txt");
    let id = queue_save(&mut p, "", Some("unused"));
    let (source, mut sender) = UnixStream::pair().unwrap();
    let requests::Waiting::FileDialog(wait) = p.requests.waiting.get_mut(&id).unwrap() else {
        panic!("no dialog");
    };
    wait.source = Some(Export::File(std::fs::File::from(OwnedFd::from(source))));
    let (returned, return_seen) = std::sync::mpsc::channel();
    let writer = std::thread::spawn(move || {
        let free = return_seen
            .recv_timeout(std::time::Duration::from_secs(2))
            .is_ok();
        sender.write_all(b"export bytes").unwrap();
        free
    });
    p.answer_file_dialog(id, Outcome::Chosen(vec![to.clone()]));
    returned.send(()).ok();
    assert!(p.pump(0.).is_none());
    let free = writer.join().unwrap();
    settle(&mut p, &woken, id);
    assert!(free, "export I/O blocked the presenter's thread");
    assert_eq!(std::fs::read(to).unwrap(), b"export bytes");
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn retirement_reaches_dialogs_already_taken_by_the_window() {
    for mode in ["replaced", "renewed", "cancelled"] {
        let (mut p, _) = presenter();
        let mut views = Vec::new();
        for name in ["result", "files"] {
            let view = p
                .host
                .kernel()
                .node_by_key(p.host.kernel().find_by_id(name)[0])
                .unwrap()
                .id;
            views.push(view);
            p.queue_file_dialog(
                FileDialog {
                    command: "showOpenFilePicker",
                    picker: Picker {
                        kind: Kind::Open,
                        id: name.into(),
                        multiple: false,
                        suggested_name: String::new(),
                        start_in: None,
                    },
                },
                view,
                Route::Document(Kind::Open),
            );
        }
        let dialogs = p.take_file_dialogs();
        assert_eq!(dialogs.len(), 2);
        assert!(dialogs.iter().all(|(_, _, live)| live.upgrade().is_some()));
        match mode {
            "replaced" => p.picks_replaced(),
            "renewed" => p.picks_renewed(&views),
            _ => {
                for (id, _, _) in &dialogs {
                    assert!(p.answer_request(*id, None));
                }
            }
        }
        for (id, _, live) in dialogs {
            assert!(!p.requests.waiting.contains_key(&id));
            assert!(
                live.upgrade().is_none(),
                "{mode}: {id} still belongs to the window"
            );
        }
    }
}

#[test]
fn a_non_unicode_export_creates_and_truncates_nothing() {
    let (mut p, woken) = presenter();
    let dir = directory();
    let to = non_unicode(&dir);
    for existing in [false, true] {
        // APFS cannot create this name; NTFS and Linux can.
        if existing && std::fs::write(&to, b"keep").is_err() {
            continue;
        }
        let id = queue_save(&mut p, "", Some("replacement"));
        p.answer_file_dialog(id, Outcome::Chosen(vec![to.clone()]));
        settle(&mut p, &woken, id);
        assert!(p
            .host
            .runner()
            .journal()
            .any(|line| line.ends_with(" saveFile: refused: a chosen path is not Unicode")));
        assert_eq!(p.host.runner().slot("result").unwrap().as_str(), Some(""));
        if existing {
            assert_eq!(std::fs::read(&to).unwrap(), b"keep");
        } else {
            assert!(!to.exists());
        }
    }
    std::fs::remove_dir_all(dir).unwrap();
}

#[cfg(unix)]
#[test]
fn overlapping_exports_publish_whole_files_and_keep_the_view_busy() {
    use std::io::Write;
    use std::os::{fd::OwnedFd, unix::net::UnixStream};
    let (mut p, woken) = presenter();
    let dir = directory();
    let to = dir.join("saved.txt");
    let id = queue_save(&mut p, "", Some("unused"));
    let (source, mut sender) = UnixStream::pair().unwrap();
    let requests::Waiting::FileDialog(wait) = p.requests.waiting.get_mut(&id).unwrap() else {
        panic!("no dialog");
    };
    wait.source = Some(Export::File(std::fs::File::from(OwnedFd::from(source))));
    p.answer_file_dialog(id, Outcome::Chosen(vec![to.clone()]));
    sender.write_all(b"first prefix").unwrap();
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
    // The open stream pauses the first copy after its prefix reaches disk.
    while !std::fs::read_dir(&dir)
        .unwrap()
        .any(|entry| entry.unwrap().metadata().unwrap().len() >= 12)
    {
        assert!(
            std::time::Instant::now() < deadline,
            "first copy did not start"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }
    request_save(&mut p, "", Some("second"));
    let dialogs = p.take_file_dialogs();
    // Another view or window can export to the same destination.
    let (copied, copy_seen) = std::sync::mpsc::channel();
    let second_to = to.clone();
    let second = std::thread::spawn(move || {
        let result = Export::Text("second".into()).write(&second_to);
        copied.send(()).unwrap();
        result
    });
    let interleaved = copy_seen
        .recv_timeout(std::time::Duration::from_millis(200))
        .is_ok();
    sender.write_all(b" and suffix").unwrap();
    drop(sender);
    settle(&mut p, &woken, id);
    second.join().unwrap().unwrap();
    assert!(
        !interleaved,
        "another view wrote while the first export was copying"
    );
    assert_eq!(std::fs::read(&to).unwrap(), b"second");
    assert!(
        dialogs.is_empty(),
        "the exporting view admitted another dialog"
    );
    assert_eq!(std::fs::read_dir(&dir).unwrap().count(), 1);
    std::fs::remove_dir_all(dir).unwrap();
}
