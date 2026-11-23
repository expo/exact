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

/// `exactPage().iframePartition` asks the platform only for a plan with an iframe.
#[test]
fn only_a_plan_with_a_frame_asks_whether_partitions_are_kept() {
    let boot = |source: &str| {
        let (p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (400., 800.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p
    };
    let mut plain = boot("component App\n  view\n    text \"no frame\"\n");
    plain.serve_frame_partitions(|| panic!("a plan without an iframe asked"));
    assert!(!plain.host.runner().page().iframe_partition);
    let mut framed = boot(APP);
    framed.serve_frame_partitions(|| true);
    assert!(framed.host.runner().page().iframe_partition);
}
