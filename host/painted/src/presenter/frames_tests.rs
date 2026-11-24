//! Each frame command this host has not built is the frame's refusal, never a quiet drop.
use super::super::{PainterChoice, Presenter};
use std::path::PathBuf;

const APP: &str = r#"component App
  state heard = ""
  action failed(e: FrameNavigation)
    heard = heard + e.error + ":" + (e.partitioned ? "p" : "u") + " "
  action shot(e: FrameCapture)
    heard = heard + "capture " + e.error + " " + e.path + " "
  action go
    navigate("guest", "https://example.com/")
    traverseHistory("guest", -1)
    reload("guest")
    reload("guest", ignoreCache=true)
    stopLoading("guest")
    captureScreenshot("guest", "app:/tmp/shot.png")
    openDevTools("guest")
  view
    column
      iframe id="guest" src="https://example.com/" partition="persist:a" navigateerror=failed capture=shot width=300 height=180
      button press=go testId="go" height=30
        text "Go"
      text heard testId="heard"
"#;

#[test]
fn every_frame_command_is_refused_at_the_frame() {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kernel = p.host().kernel();
    let go = kernel
        .node_by_key(kernel.find_by_test_id("go")[0])
        .unwrap()
        .id;
    p.tap(go).unwrap();
    p.run_commands(|| ());
    let tree = crate::agent::handle(&mut p, r#"{"op":"tree"}"#);
    let expected = "NotSupportedError:u NotSupportedError:u NotSupportedError:u NotSupportedError:u NotSupportedError:u capture NotSupportedError app:/tmp/shot.png";
    assert!(tree.contains(expected), "{tree}");
    let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
    assert!(
        journal.contains(
            "openDevTools: iframe \\\"guest\\\": this host does not yet open a frame's inspector"
        ),
        "{journal}"
    );
    assert!(!journal.contains("unknown command"), "{journal}");
}

/// The reader's record reaches only the frame allocation and session it was sent for; a frame
/// that does not listen hears nothing, and nothing is logged as an error.
#[test]
fn a_reader_record_reaches_only_its_frame() {
    const FRAMES: &str = r#"component App
  state heard = ""
  action failed(e: FrameNavigation)
    heard = heard + e.error
  view
    column
      iframe id="heard" src="https://example.com/" partition="persist:a" navigateerror=failed width=300 height=100 testId="heard-frame"
      iframe id="deaf" src="https://example.com/" partition="persist:b" width=300 height=100 testId="deaf-frame"
      text heard testId="heard"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(FRAMES),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = |p: &Presenter<()>, test_id: &str| {
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id(test_id)[0])
            .unwrap();
        (node.id, node.key)
    };
    let ((heard, key), (deaf, deaf_key)) = (frame(&p, "heard-frame"), frame(&p, "deaf-frame"));
    let session = p.brush.native_session;
    let record = r#"{"error":"NotSupportedError","partitioned":false}"#;
    let stale = exact_kernel::NodeKey {
        generation: key.generation.wrapping_add(1),
        ..key
    };
    assert!(!p.reader_frame_event(heard, stale, session, "navigateerror", record));
    assert!(!p.reader_frame_event(heard, key, session.wrapping_add(1), "navigateerror", record));
    assert!(p.reader_frame_event(deaf, deaf_key, session, "navigateerror", record));
    assert!(p.reader_frame_event(heard, key, session, "navigateerror", record));
    let tree = crate::agent::handle(&mut p, r#"{"op":"tree"}"#);
    assert!(tree.contains("\"NotSupportedError\""), "{tree}");
    let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
    assert!(!journal.contains("NoHandler"), "{journal}");
}

/// `exactPage().iframePartition` is asked once, by the first plan with an
/// iframe, at boot or in a live update, and holds from that runner's first
/// settlement: a child's state seeded from it reads the device's answer.
#[test]
fn whether_partitions_are_kept_is_asked_once_by_the_first_plan_with_a_frame() {
    const PLAIN: &str = "component App\n  view\n    text \"no frame\"\n";
    const FRAMED: &str = "shape Facts\n  iframePartition: bool\n\ncomponent App\n  resource page = exactPage() as shape Facts\n  view\n    column\n      Seen(kept=page.iframePartition)\n      iframe id=\"guest\" src=\"https://example.com/\" partition=\"persist:a\"\n\ncomponent Seen\n  props\n    kept: bool\n  state seen = kept\n  view\n    text `${seen}` testId=\"seen\"\n";
    fn kept_page(plan: &exact_plan::Plan) -> exact_runner::Page {
        super::with_partitions(Default::default(), plan, || Some(true))
    }
    fn never_asked(plan: &exact_plan::Plan) -> exact_runner::Page {
        super::with_partitions(Default::default(), plan, || {
            panic!("a plan without an iframe asked")
        })
    }
    let boot = |source: &str, page: fn(&exact_plan::Plan) -> exact_runner::Page| {
        let launch = exact_runner::time::Launch {
            page: Some(exact_runner::time::PageFacts::new(page)),
            ..exact_runner::time::Launch::NONE
        };
        let (mut p, error) = exact_runner::time::launching_with(launch, || {
            Presenter::boot_with(
                &crate::test_plan(source),
                (),
                (400., 800.),
                1.,
                PathBuf::new(),
                PainterChoice::Cpu,
            )
        })
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_frame_partitions(|| Some(true));
        p
    };
    let kept = |p: &Presenter<()>| p.host.runner().page().iframe_partition;
    let seen = |p: &Presenter<()>| {
        let state: serde_json::Value =
            serde_json::from_str(&exact_runner::agent::state(p.host.runner())).unwrap();
        let slots = state["slots"].as_object().unwrap();
        slots
            .iter()
            .any(|(name, value)| name.starts_with("seen#") && value == true)
    };
    assert!(!kept(&boot(PLAIN, never_asked)));
    let p = boot(FRAMED, kept_page);
    assert!(kept(&p) && seen(&p));
    // A live update that brings the first iframe asks before its first settlement.
    let mut p = boot(PLAIN, never_asked);
    p.reload(&crate::test_plan(FRAMED), ()).unwrap();
    assert!(kept(&p) && seen(&p));
    // What a drive prefers holds: an iframe gone and back again, or the first to come.
    let mut p = boot(FRAMED, kept_page);
    let preferred = exact_runner::Page {
        iframe_partition: false,
        ..p.host.runner().page()
    };
    assert!(p.prefer_page(preferred, true).is_none());
    p.reload(&crate::test_plan(PLAIN), ()).unwrap();
    p.reload(&crate::test_plan(FRAMED), ()).unwrap();
    assert!(!kept(&p));
    // A platform that could not answer is asked again.
    static ANSWERS: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
    fn probe() -> Option<bool> {
        ANSWERS
            .load(std::sync::atomic::Ordering::Relaxed)
            .then_some(true)
    }
    let mut p = boot(FRAMED, |plan| {
        super::with_partitions(Default::default(), plan, probe)
    });
    p.serve_frame_partitions(probe);
    assert!(!kept(&p));
    ANSWERS.store(true, std::sync::atomic::Ordering::Relaxed);
    p.reload(&crate::test_plan(FRAMED), ()).unwrap();
    assert!(kept(&p));
    let mut p = boot(PLAIN, never_asked);
    assert!(p.prefer_page(p.host.runner().page(), true).is_none());
    p.reload(&crate::test_plan(FRAMED), ()).unwrap();
    assert!(!kept(&p));
}

/// Where the reader keeps partitions, the four commands that move a frame
/// reach a partitioned frame's reader in its props, numbered; an
/// unpartitioned frame refuses them as Apple's does, and a capture or the
/// inspector is still refused.
#[test]
fn a_kept_partition_takes_its_commands_in_its_props() {
    const TWO: &str = r#"component App
  state heard = ""
  state part = "persist:a"
  action failed(e: FrameNavigation)
    heard = heard + e.error + " " + e.message + "; "
  action go
    navigate("guest", "https://example.com/b")
    traverseHistory("guest", -1)
    reload("guest", ignoreCache=true)
    stopLoading("guest")
    captureScreenshot("guest", "app:/tmp/shot.png")
    reload("plain")
  action move
    part = "persist:b"
  view
    column
      iframe id="guest" src="https://example.com/" partition=part navigateerror=failed width=300 height=100 testId="guest"
      iframe id="plain" src="https://example.com/" navigateerror=failed width=300 height=100
      button press=go testId="go" height=30
        text "Go"
      button press=move testId="move" height=30
        text "Move"
      text heard testId="heard"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(TWO),
        (),
        (400., 800.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kept = exact_runner::Page {
        iframe_partition: true,
        ..p.host.runner().page()
    };
    assert!(p.prefer_page(kept, true).is_none());
    let node = |p: &Presenter<()>, test_id: &str| {
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id(test_id)[0])
            .unwrap();
        (node.id, node.key)
    };
    p.tap(node(&p, "go").0).unwrap();
    p.run_commands(|| ());
    let key = node(&p, "guest").1;
    let sent = &p.brush.media.frames[&key];
    assert_eq!(
        serde_json::Value::from(sent.list.iter().cloned().collect::<Vec<_>>()),
        serde_json::json!([
            [1, "navigate", "https://example.com/b"],
            [2, "traverseHistory", -1.0],
            [3, "reload", true],
            [4, "stopLoading"]
        ])
    );
    let tree = crate::agent::handle(&mut p, r#"{"op":"tree"}"#);
    assert!(
        tree.contains("reload: iframe plain has no partition"),
        "{tree}"
    );
    let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
    assert!(
        journal.contains("captureScreenshot: iframe \\\"guest\\\": this host does not yet capture"),
        "{journal}"
    );
    // A new partition is a new web view: it runs none of the old ones.
    p.tap(node(&p, "move").0).unwrap();
    p.tap(node(&p, "go").0).unwrap();
    p.run_commands(|| ());
    let sent = &p.brush.media.frames[&key];
    assert_eq!((sent.partition.as_str(), sent.list.len()), ("persist:b", 4));
    assert_eq!(sent.list[0][0], 5);
}

#[test]
fn a_frame_keeps_its_newest_commands() {
    let mut c = crate::paint::native::FrameCommands::default();
    for _ in 0..20 {
        c.push("persist:a", "stopLoading", &[]);
    }
    assert_eq!(c.list.len(), crate::paint::native::FrameCommands::KEPT);
    assert_eq!(
        (c.list[0][0].as_u64(), c.list[15][0].as_u64()),
        (Some(5), Some(20))
    );
}
