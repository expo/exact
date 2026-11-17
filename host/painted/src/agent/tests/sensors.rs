//! A canvas sensor input under the substitute (LLP 1133.013 D6): every
//! seek of the clock posts each display frame's samples as it goes.
use super::*;

/// The app's data grants motion.
#[derive(Default)]
struct Granted;
impl DataSource for Granted {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "device.motion purpose.motion\n"
    }
}

/// An app beside the repository's `@exact/motion`, installed as `exact add`
/// would, whose button starts a gravity input to the canvas `world`.
fn tilting() -> (Presenter<Granted>, std::path::PathBuf) {
    let dir = std::env::temp_dir().join(format!("exact-hold-samples-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let from = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packages/motion");
    let to = dir.join("node_modules/@exact/motion");
    for sub in ["", "web", "apple", "android"] {
        std::fs::create_dir_all(to.join(sub)).unwrap();
        std::fs::copy(
            from.join(sub).join("exact.json"),
            to.join(sub).join("exact.json"),
        )
        .unwrap();
    }
    std::fs::copy(from.join("package.json"), to.join("package.json")).unwrap();
    let source = r#"use startSensorInput, SensorInput from "@exact/motion"

component App
  mutation input as shape SensorInput
  action tilt
    send input = startSensorInput(surface="world", sensor="gravity", frequency=60)
  view
    column
      button press=tilt testId="tilt"
        text "Tilt"
      box testId="pad" width=100 height=100
      canvas surface=world()
"#;
    std::fs::write(dir.join("app.contract"), source).unwrap();
    let (plan, _) = contract::compile_path_all(&dir.join("app.contract"), false)
        .unwrap_or_else(|e| panic!("{e:?}"));
    let (p, error) = Presenter::boot_with(
        &plan.encode(),
        Granted,
        (300., 300.),
        1.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    (p, dir)
}

/// A contact's hold seeks the clock as `clock` does, a frame at a time:
/// when it ends, no sample of its span is still owed to the canvas, so a
/// later seek does not post the whole span at once past the queue's bound.
#[test]
fn a_contact_hold_posts_each_frames_sensor_samples() {
    let (mut p, dir) = tilting();
    let id = |p: &Presenter<Granted>, test_id: &str| {
        let k = p.host().kernel();
        k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
    };
    let tilt = id(&p, "tilt");
    handle(&mut p, &format!(r#"{{"op":"tap","id":{tilt}}}"#));
    let reply = handle(&mut p, r#"{"op":"clock","settle":true}"#);
    assert!(p.host().runner().surface_sensor_inputs(), "{reply}");
    let pad = id(&p, "pad");
    let down = handle(
        &mut p,
        &format!(r#"{{"op":"tap","phase":"down","id":{pad}}}"#),
    );
    assert!(down.contains("\"contact\":true"), "{down}");
    let hold = handle(&mut p, r#"{"op":"tap","phase":"hold","ms":500}"#);
    assert!(!hold.contains("error"), "{hold}");
    let now = p.host().now();
    let owed = p.host_mut().runner_mut().surface_sensor_events(now);
    assert!(
        owed.is_empty(),
        "{} samples left to post after the hold",
        owed.len()
    );
    let _ = std::fs::remove_dir_all(&dir);
}
