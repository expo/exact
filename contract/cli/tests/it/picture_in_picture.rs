//! An image's picture in picture (LLP 1042 §9): `requestPictureInPicture(id)`
//! and `exitPictureInPicture()` reach the host, and the image hears
//! `enterpictureinpicture`, `leavepictureinpicture` and, with its reason,
//! `pictureinpictureerror` as the host reports them.
use crate::source::{boxed, NoData, Source};
use exact_runner::{Event, Runner, Value};

const APP: &str = r#"component App
  state floating = false
  state entries = 0
  state refusal = ""
  action float
    requestPictureInPicture("page")
  action unfloat
    exitPictureInPicture()
  action refused(why: string)
    refusal = why
  action entered
    floating = true
    entries = entries + 1
  action left
    floating = false
  view
    column
      button press=float testId="float"
        text "Float"
      image "app:/tmp/frame.jpg" id="page" testId="page" width=1 height=1 enterpictureinpicture=entered leavepictureinpicture=left pictureinpictureerror=refused
"#;

fn boot() -> Runner<Source> {
    Runner::boot(
        contract::compile(APP).unwrap(),
        boxed(NoData),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn the_request_is_a_command_naming_the_image() {
    let mut r = boot();
    r.take_commands();
    r.act("float", vec![]).unwrap();
    r.act("unfloat", vec![]).unwrap();
    let ran: Vec<_> = r
        .take_commands()
        .into_iter()
        .map(|c| (c.name, c.args))
        .collect();
    assert_eq!(
        ran,
        [
            (
                "requestPictureInPicture".to_string(),
                vec![Value::str("page")]
            ),
            ("exitPictureInPicture".to_string(), vec![])
        ]
    );
}

#[test]
fn the_image_hears_it_enter_and_leave() {
    let mut r = boot();
    let key = r.kernel().find_first_by_test_id("page").unwrap();
    let id = r.kernel().node_by_key(key).unwrap().id;
    for (wire, floating) in [
        ("enterpictureinpicture\n", true),
        ("leavepictureinpicture\n", false),
        ("enterpictureinpicture\n", true),
    ] {
        let event = Event::media_payload(wire).unwrap_or_else(|| panic!("{wire:?} refused"));
        r.dispatch(id, event).unwrap();
        assert_eq!(r.slot("floating"), Some(&Value::Bool(floating)), "{wire:?}");
    }
    assert_eq!(r.slot("entries"), Some(&Value::Number(2.0)));
    let refused = "picture in picture is not possible";
    let event = Event::media_payload(&format!("pictureinpictureerror\n{refused}")).unwrap();
    r.dispatch(id, event).unwrap();
    assert_eq!(r.slot("refusal"), Some(&Value::str(refused)));
}

#[test]
fn the_events_carry_nothing() {
    for wire in ["enterpictureinpicture\ntrue", "leavepictureinpicture\n1"] {
        assert!(Event::media_payload(wire).is_none(), "{wire:?}");
    }
}

#[test]
fn the_request_names_one_element() {
    for statement in [
        r#"requestPictureInPicture()"#,
        r#"requestPictureInPicture(1)"#,
        r#"requestPictureInPicture("page", "other")"#,
        r#"exitPictureInPicture("page")"#,
    ] {
        let source = APP.replace(r#"requestPictureInPicture("page")"#, statement);
        let error = contract::compile(&source).unwrap_err().to_string();
        assert!(error.contains("type-media-command"), "{statement}: {error}");
    }
}

#[test]
fn the_window_option_names_exacts_window() {
    let source = APP.replace(
        r#"requestPictureInPicture("page")"#,
        r#"requestPictureInPicture("page", presentation="window", title="Preview", width=480, height=320)"#,
    );
    let mut r = Runner::boot(
        contract::compile(&source).unwrap(),
        boxed(NoData),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.take_commands();
    r.act("float", vec![]).unwrap();
    let ran: Vec<_> = r.take_commands().into_iter().map(|c| c.args).collect();
    assert_eq!(
        ran,
        [vec![
            Value::str("page"),
            Value::str("window"),
            Value::str("Preview"),
            Value::Number(480.0),
            Value::Number(320.0)
        ]]
    );
    let bare = APP.replace(
        r#"requestPictureInPicture("page")"#,
        r#"requestPictureInPicture("page", presentation="panel")"#,
    );
    let mut r = Runner::boot(
        contract::compile(&bare).unwrap(),
        boxed(NoData),
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.take_commands();
    r.act("float", vec![]).unwrap();
    let ran: Vec<_> = r.take_commands().into_iter().map(|c| c.args).collect();
    assert_eq!(
        ran,
        [vec![
            Value::str("page"),
            Value::str("panel"),
            Value::Option(None),
            Value::Option(None),
            Value::Option(None)
        ]]
    );
}

#[test]
fn the_window_options_are_checked() {
    for statement in [
        r#"requestPictureInPicture("page", presentation="system")"#,
        r#"requestPictureInPicture("page", title="Preview")"#,
        r#"requestPictureInPicture("page", presentation="window", width="480")"#,
        r#"requestPictureInPicture("page", presentation="window", size=3)"#,
        r#"requestPictureInPicture("page", presentation="window", presentation="window")"#,
        r#"exitPictureInPicture(presentation="window")"#,
        r#"load("page", presentation="window")"#,
    ] {
        let source = APP.replace(r#"requestPictureInPicture("page")"#, statement);
        let error = contract::compile(&source).unwrap_err().to_string();
        assert!(error.contains("type-media-command"), "{statement}: {error}");
    }
}

#[test]
fn a_video_accepts_the_events_and_refuses_each_image_option_by_name() {
    let app = include_str!("../../../../scripts/fixtures/video-picture-in-picture.contract");
    let app = format!("{app}      image \"poster.png\" id=`poster-${{status}}`\n");
    contract::compile(&app).unwrap();
    for option in [
        "presentation=\"window\"",
        "title=\"Preview\"",
        "width=480",
        "height=320",
    ] {
        let source = app.replace("(\"clip\")", &format!("(\"clip\", {option})"));
        let error = contract::compile(&source).unwrap_err().to_string();
        assert!(
            error.contains("type-media-command")
                && error.contains(option.split('=').next().unwrap()),
            "{error}"
        );
    }
}

#[test]
fn a_removed_video_delivers_leave_in_its_original_scope() {
    let app = include_str!("../../../../scripts/fixtures/video-picture-in-picture.contract");
    for source in [
        app.to_owned(),
        app.replace(" enterpictureinpicture=entered", ""),
    ] {
        let mut runner = Runner::boot(
            contract::compile(&format!(
                "{source}      video \"other.mp4\" testId=\"other\" leavepictureinpicture=left\n"
            ))
            .unwrap(),
            boxed(NoData),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let key = runner.kernel().find_first_by_test_id("clip").unwrap();
        let view = runner.kernel().node_by_key(key).unwrap().id;
        runner
            .dispatch(
                view,
                Event::media_payload("enterpictureinpicture\n").unwrap(),
            )
            .unwrap();
        assert!(runner.expects_video_leave(view, key));
        runner.act("remove", vec![]).unwrap();
        assert!(runner.kernel().node_by_key(key).is_none());
        let next_key = runner.kernel().find_first_by_test_id("other").unwrap();
        let next = runner.kernel().node_by_key(next_key).unwrap().id;
        assert_ne!(view, next);
        runner
            .dispatch(
                next,
                Event::media_payload("enterpictureinpicture\n").unwrap(),
            )
            .unwrap();
        runner
            .dispatch(
                view,
                Event::media_payload("leavepictureinpicture\n").unwrap(),
            )
            .unwrap();
        assert_eq!(runner.slot("leaves"), Some(&Value::Number(1.0)));
        assert!(!runner.expects_video_leave(view, key));
        assert!(runner.expects_video_leave(next, next_key));
    }
}
