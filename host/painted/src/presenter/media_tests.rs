use super::super::*;

fn video(p: &Presenter<()>) -> ViewId {
    p.host
        .kernel()
        .node_by_key(p.host.kernel().find_by_test_id("video")[0])
        .unwrap()
        .id
}

#[test]
fn a_players_reports_reach_the_node_only_for_its_session_and_allocation() {
    let source = r#"component App
  state paused = false
  action stopped
    paused = true
  view
    column width=400 height=400
      text (paused ? "paused" : "playing") testId="label" height=24
      video src="a.mp4" paused=paused pause=stopped testId="video"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let id = video(&p);
    let key = p.host.kernel().node(id).unwrap().key;
    let session = p.brush.native_session;
    let label = |p: &Presenter<()>| {
        let label = p.host.kernel().find_by_test_id("label")[0];
        let node = p.host.kernel().node_by_key(label).unwrap();
        node.props.str(PropId::Text).unwrap().to_string()
    };
    // Before any report the row says why it never plays.
    assert!(p.media_json()[0]["state"]["unavailable"].is_string());

    // Another painter's session, an event the node does not handle, and a name
    // that is no media event are dropped.
    assert!(!p.reader_media(id, key, session + 1, "pause", "", "", 1.));
    assert!(!p.reader_media(id, key, session, "playing", "", "", 1.));
    assert_eq!(label(&p), "playing");

    assert!(p.reader_media(id, key, session, "pause", "", "", 2.));
    assert_eq!(label(&p), "paused");

    // A state report is the row the agent reads, and its size the node's natural size.
    let state = r#"{"currentTime":3,"duration":10,"paused":true,"playing":false,"playbackRate":1,"videoWidth":640,"videoHeight":360}"#;
    assert!(p.reader_media(id, key, session, "", "", state, 3.));
    let row = &p.media_json()[0]["state"];
    assert_eq!(row["currentTime"], 3.0);
    assert!(row["unavailable"].is_null());
    // The same size again lays nothing out.
    assert!(!p.reader_media(id, key, session, "", "", state, 4.));
}

/// Only a node that handles `timeupdate` has its player run the 4 Hz clock
/// (LLP 1042.001 D3): the painter names which.
#[test]
fn the_painter_names_the_videos_that_handle_timeupdate() {
    let source = r#"component App
  state at = 0.0
  action ticked(t: number)
    at = t
  view
    column width=400 height=400
      video src="a.mp4" timeupdate=ticked testId="timed"
      video src="b.mp4" testId="quiet"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let key = |p: &Presenter<()>, id| p.host.kernel().find_by_test_id(id)[0];
    p.sync_media(&mut Default::default());
    assert!(p.brush.media.timed.contains(&key(&p, "timed")));
    assert!(!p.brush.media.timed.contains(&key(&p, "quiet")));
}

/// `fastSeek`, `load` and `requestFullscreen` are counted in the node's
/// props (LLP 1042.001 §5):
/// a node that is no media element, or no node, refuses them.
#[test]
fn fast_seek_and_load_are_numbered_for_the_player() {
    let source = r#"component App
  view
    column width=400 height=400
      video src="a.mp4" id="clip" testId="video"
      text "Clip" id="label"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let key = p.host.kernel().find_by_test_id("video")[0];
    let id = |s: &str| exact_plan::Value::str(s);
    p.media_command("fastSeek", &[id("clip"), exact_plan::Value::Number(4.5)]);
    p.media_command("fastSeek", &[id("clip"), exact_plan::Value::Number(7.0)]);
    p.media_command("load", &[id("clip")]);
    p.media_command("requestFullscreen", &[id("clip")]);
    let asked = p.brush.media.commands[&key];
    assert_eq!(
        (asked.seeks, asked.seconds, asked.loads, asked.fullscreens),
        (2, 7.0, 1, 1)
    );
    assert_eq!(p.brush.media.asked, [key, key, key, key]);
    p.media_command("load", &[id("label")]);
    p.media_command("load", &[id("gone")]);
    assert_eq!(p.brush.media.commands.len(), 1);
    let journal = crate::agent::handle(&mut p, r#"{"op":"logs"}"#);
    assert!(
        journal.contains(r#"load \"label\" refused: not a video or audio"#),
        "{journal}"
    );
    assert!(
        journal.contains(r#"load \"gone\" refused: no live node with that id"#),
        "{journal}"
    );
}

/// A `controls` video is a Tab stop where its player draws the controller,
/// as a hand-built `PlayerView` is, and its props name it focused, so the
/// player takes the window's focus (LLP 1042.001 D4). Without `controls`, or
/// on a host with no player, it takes none.
#[test]
fn tab_stops_at_a_controls_video_where_a_player_draws_its_controller() {
    let source = r#"component App
  view
    column width=400 height=600
      button "Before" testId="before" height=30
      video src="a.mp4" controls=true testId="controls" height=100
      video src="b.mp4" testId="bare" height=100
      button "After" testId="after" height=30
"#;
    let boot = |players: bool| {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (400., 600.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.media.players = players;
        p
    };
    let key = |p: &Presenter<()>, id| p.host.kernel().find_by_test_id(id)[0];
    let id = |p: &Presenter<()>, name| p.host.kernel().node_by_key(key(p, name)).unwrap().id;
    let tab = |p: &mut Presenter<()>, at: f64| {
        assert_eq!(p.move_focus(false, at), None);
        p.sync_media(&mut Default::default());
        p.focus
    };

    let mut p = boot(true);
    assert_eq!(tab(&mut p, 1.), Some(id(&p, "before")));
    assert_eq!(tab(&mut p, 2.), Some(id(&p, "controls")));
    assert_eq!(p.brush.media.focused, Some(key(&p, "controls")));
    assert_eq!(tab(&mut p, 3.), Some(id(&p, "after")));
    assert_eq!(p.brush.media.focused, None);

    let mut p = boot(false);
    assert_eq!(tab(&mut p, 1.), Some(id(&p, "before")));
    assert_eq!(tab(&mut p, 2.), Some(id(&p, "after")));
}
