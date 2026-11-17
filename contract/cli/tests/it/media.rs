//! A media element's commands by HTML's method names (podcast F8, F18):
//! `fastSeek(id, seconds)` reaches the host every time it runs, even to the
//! same time twice (the bound `currentTime` seeks only on a changed value),
//! and `load(id)` loads the source again.
use exact_runner::{DataError, DataSource, Runner, Value};

const APP: &str = r#"component App
  state hush = true
  action back
    fastSeek("player", 60)
  action retry
    load("player")
  view
    column
      button press=back testId="back"
        text "Back to 1:00"
      button press=retry testId="retry"
        text "Retry"
      audio "https://example.com/episode.mp3" id="player" paused=hush
"#;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.to_string()))
    }
}

#[test]
fn every_seek_and_load_is_its_own_command() {
    let mut r = Runner::boot(
        contract::compile(APP).unwrap(),
        NoData,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.take_commands();
    for action in ["back", "back", "retry"] {
        r.act(action, vec![]).unwrap();
    }
    let ran: Vec<_> = r
        .take_commands()
        .into_iter()
        .map(|c| (c.name, c.args))
        .collect();
    let seek = (
        "fastSeek".to_string(),
        vec![Value::str("player"), Value::Number(60.)],
    );
    let load = ("load".to_string(), vec![Value::str("player")]);
    assert_eq!(ran, [seek.clone(), seek, load]);
}

#[test]
fn the_commands_are_checked_by_name() {
    for (from, statement) in [
        (r#"fastSeek("player", 60)"#, r#"fastSeek("player")"#),
        (r#"fastSeek("player", 60)"#, r#"fastSeek(60, "player")"#),
        (r#"fastSeek("player", 60)"#, r#"fastSeek("player", "60")"#),
        (
            r#"fastSeek("player", 60)"#,
            r#"fastSeek("player", time=60)"#,
        ),
        (r#"load("player")"#, r#"load()"#),
        (r#"load("player")"#, r#"load("player", 1)"#),
        (r#"load("player")"#, r#"load(hush)"#),
    ] {
        let source = APP.replace(from, statement);
        let error = contract::compile(&source).unwrap_err().to_string();
        assert!(error.contains("type-media-command"), "{statement}: {error}");
    }
}

// @ref LLP 1042.000 D2 — a rendition's bound is spelt as HLS spells a
// variant's RESOLUTION; an `audio` has no picture to bound.
#[test]
fn a_resolution_is_width_x_height_on_a_video() {
    let video = |attrs: &str| {
        format!("component App\n  view\n    video \"https://example.com/a.m3u8\" {attrs}\n")
    };
    for good in [
        "preferredMaximumResolution=\"1920x1080\" startsOnFirstEligibleVariant=true",
        "preferredMaximumResolution=\"640x360\"",
    ] {
        contract::compile(&video(good)).unwrap_or_else(|e| panic!("{good}: {e}"));
    }
    for bad in [
        "1920",
        "1920x",
        "x1080",
        "0x1080",
        "1920X1080",
        "1920 x 1080",
        "-1x2",
        "1.5x2",
    ] {
        let error = contract::compile(&video(&format!("preferredMaximumResolution=\"{bad}\"")))
            .unwrap_err()
            .to_string();
        assert!(error.contains("lower-attr-value"), "{bad}: {error}");
    }
    let audio = APP.replace(
        "paused=hush",
        "paused=hush preferredMaximumResolution=\"640x360\"",
    );
    let error = contract::compile(&audio).unwrap_err().to_string();
    assert!(error.contains("lower-attr-tag"), "{error}");
}
