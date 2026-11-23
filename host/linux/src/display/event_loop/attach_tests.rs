//! The attach carrier (LLP 1012.000 piece 7) in the display loop's own run,
//! headless: a person stand-in allows or refuses, and a driver speaks the
//! wire over the socket the marker names.
use super::*;
use crate::attach::{admitted, Attached, Listener, Person};
use exact_painted::presenter::PainterChoice;
use std::io::{BufRead, BufReader, Read, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::sync::mpsc::{channel, Receiver, Sender};

const DEVELOPMENT: &str = r#"{"embedded":{"trust":"development"}}"#;

struct Headless {
    close: UnixStream,
}

impl Output for Headless {
    fn refresh(&self) -> u32 {
        60
    }
    fn period_ms(&self) -> f64 {
        1000. / 60.
    }
    fn pending(&self) -> bool {
        false
    }
    fn sequence(&self) -> Option<u32> {
        None
    }
    #[cfg(target_os = "linux")]
    fn last_flip(&self) -> Option<Duration> {
        None
    }
    fn fd(&self) -> i32 {
        self.close.as_raw_fd()
    }
    fn submit(&mut self, frame: PanelFrame) -> Result<Completion, String> {
        Ok(Completion::Shown(frame))
    }
    fn ready(&mut self) -> Result<Completion, String> {
        Err("headless display closed".into())
    }
}

/// The loop's other events: a wake descriptor the person stand-in writes to as it answers.
struct Wake(UnixStream);

impl Events<()> for Wake {
    fn fds(&self) -> Vec<i32> {
        vec![self.0.as_raw_fd()]
    }
    fn idle(&self) -> bool {
        false
    }
    fn mirror(&self) -> bool {
        false
    }
    fn publish(&mut self, _: Arc<Pixmap>) {}
    fn turn(&mut self, _: &mut Presenter<()>, _: impl Fn() -> f64) {
        let _ = self.0.set_nonblocking(true);
        let mut drained = [0u8; 64];
        while matches!(self.0.read(&mut drained), Ok(n) if n > 0) {}
    }
    fn date_jumped(&self) -> bool {
        false
    }
    fn trace_requested(&self) -> bool {
        false
    }
}

/// The person: each prompt the app shows goes to the test, which answers that prompt; what the window shows goes
/// back to it too. An answer for a prompt no longer shown is no answer.
struct Stand {
    prompts: Sender<u64>,
    answers: Receiver<(u64, bool)>,
    asking: Option<u64>,
    shown: Sender<bool>,
}

impl Person for Stand {
    fn ask(&mut self, id: u64) {
        self.asking = Some(id);
        let _ = self.prompts.send(id);
    }
    fn answer(&mut self, id: u64) -> Option<bool> {
        while let Ok((prompt, allow)) = self.answers.try_recv() {
            if self.asking == Some(prompt) && prompt == id {
                return Some(allow);
            }
        }
        None
    }
    fn ended(&mut self, id: u64) {
        if self.asking == Some(id) {
            self.asking = None;
        }
    }
    fn attached(&mut self, on: bool) {
        let _ = self.shown.send(on);
    }
}

struct App {
    close: UnixStream,
    wake: UnixStream,
    answers: Sender<(u64, bool)>,
    prompts: Receiver<u64>,
    shown: Receiver<bool>,
    runtime: std::path::PathBuf,
    thread: Option<std::thread::JoinHandle<i32>>,
}

impl App {
    fn run() -> Self {
        let runtime =
            std::env::temp_dir().join(format!("exact-attach-test-{}", std::process::id()));
        std::fs::create_dir_all(&runtime).unwrap();
        std::fs::set_permissions(&runtime, std::fs::Permissions::from_mode(0o700)).unwrap();
        let plan = contract::compile(
            r#"component App
  state ticks = 0
  state secret = "hunter2"
  action tick
    ticks = ticks + 1
  view
    column
      text `${ticks}` testId="ticks"
      input type="password" value=secret testId="key"
      button press=tick testId="more"
        text "More"
"#,
        )
        .unwrap()
        .encode();
        let (close, closed) = UnixStream::pair().unwrap();
        let (wake, woken) = UnixStream::pair().unwrap();
        let (answers, answered) = channel();
        let (prompted, prompts) = channel();
        let (shown_to, shown) = channel();
        let dir = runtime.clone();
        let thread = std::thread::spawn(move || {
            let (p, error) = Presenter::boot_with(
                &plan,
                (),
                (200., 120.),
                1.,
                std::path::PathBuf::new(),
                PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            let mut config = Config::from_env(&plan, "{}");
            config.dev_plan = None;
            config.dev_url = None;
            let mut events = Attached {
                inner: Wake(woken),
                listener: Listener::open(Some(&dir), DEVELOPMENT, false, "com.example.attach")
                    .unwrap(),
                person: Stand {
                    prompts: prompted,
                    answers: answered,
                    asking: None,
                    shown: shown_to,
                },
            };
            run(
                &config,
                Instant::now(),
                p,
                &mut Headless { close: closed },
                &mut events,
                LaunchMarks::default(),
                || (),
            )
        });
        Self {
            close,
            wake,
            answers,
            prompts,
            shown,
            runtime,
            thread: Some(thread),
        }
    }

    fn marker(&self) -> serde_json::Value {
        let path = self
            .runtime
            .join("exact")
            .join(format!("exact-attach-{}.json", std::process::id()));
        for _ in 0..400 {
            if let Ok(text) = std::fs::read_to_string(&path) {
                return serde_json::from_str(&text).unwrap();
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("no marker at {}", path.display());
    }

    /// The prompt the app shows next.
    fn prompt(&self) -> u64 {
        self.prompts
            .recv_timeout(Duration::from_secs(5))
            .expect("a prompt")
    }

    /// The person answers prompt `id`, and the loop wakes to hear it, as a click on the app would wake it.
    fn person(&self, id: u64, allow: bool) {
        self.answers.send((id, allow)).unwrap();
        (&self.wake).write_all(&[1]).unwrap();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        let _ = self.close.write_all(&[1]);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
        let _ = std::fs::remove_dir_all(&self.runtime);
    }
}

struct Driver {
    stream: UnixStream,
    lines: BufReader<UnixStream>,
}

impl Driver {
    fn connect(marker: &serde_json::Value) -> Self {
        let stream = UnixStream::connect(marker["socket"].as_str().unwrap()).unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(10)))
            .unwrap();
        let lines = BufReader::new(stream.try_clone().unwrap());
        Self { stream, lines }
    }
    fn send(&mut self, line: &str) {
        self.stream
            .write_all(format!("{line}\n").as_bytes())
            .unwrap();
    }
    fn read(&mut self) -> Option<serde_json::Value> {
        let mut line = String::new();
        match self.lines.read_line(&mut line) {
            Ok(0) | Err(_) => None,
            Ok(_) => Some(serde_json::from_str(&line).unwrap()),
        }
    }
    fn ask(&mut self, line: &str) -> serde_json::Value {
        self.send(line);
        self.read().expect("a reply")
    }
    fn handshake(&mut self, marker: &serde_json::Value) -> serde_json::Value {
        self.ask(&serde_json::json!({"attach": 1, "nonce": marker["nonce"]}).to_string())
    }
}

#[test]
fn an_allowed_driver_reads_the_running_app_on_its_wall_clock_and_is_refused_the_rest() {
    let app = App::run();
    let marker = app.marker();
    assert_eq!(
        (marker["pid"].as_u64(), marker["bundle"].as_str()),
        (
            Some(u64::from(std::process::id())),
            Some("com.example.attach")
        )
    );
    let mode = |p: &std::path::Path| {
        std::os::unix::fs::MetadataExt::mode(&std::fs::symlink_metadata(p).unwrap()) & 0o777
    };
    assert_eq!(mode(&app.runtime.join("exact")), 0o700);
    assert_eq!(
        mode(std::path::Path::new(marker["socket"].as_str().unwrap())),
        0o600
    );

    // A wrong nonce names another listener.
    let mut stranger = Driver::connect(&marker);
    assert_eq!(
        stranger.ask(r#"{"attach":1,"nonce":"00000000000000000000000000000000"}"#)["refused"],
        "handshake"
    );

    // Nothing is read before the person allows it, and a request then ends the connection.
    let mut early = Driver::connect(&marker);
    assert_eq!(early.handshake(&marker)["waiting"], "approval");
    let _ = app.prompt();
    early.send(r#"{"request":1,"op":"tree"}"#);
    assert!(early.read().is_none());

    // An answer to a prompt whose driver hung up reaches no other driver.
    let mut gone = Driver::connect(&marker);
    assert_eq!(gone.handshake(&marker)["waiting"], "approval");
    let stale = app.prompt();
    drop(gone);
    (&app.wake).write_all(&[1]).unwrap();
    std::thread::sleep(Duration::from_millis(100));
    app.person(stale, true);
    let mut d = Driver::connect(&marker);
    assert_eq!(d.handshake(&marker)["waiting"], "approval");
    let prompt = app.prompt();
    assert_ne!(prompt, stale);
    d.stream
        .set_read_timeout(Some(Duration::from_millis(500)))
        .unwrap();
    assert!(d.read().is_none(), "the stale answer allowed nothing");
    d.stream
        .set_read_timeout(Some(Duration::from_secs(10)))
        .unwrap();
    app.person(prompt, true);
    let attached = d.read().unwrap();
    assert_eq!(attached["attached"], true);
    assert_eq!(
        attached["grants"],
        serde_json::json!({"read": true, "input": false})
    );
    assert!(
        app.shown.recv_timeout(Duration::from_secs(5)).unwrap(),
        "the window shows the attachment"
    );

    let tree = d.ask(r#"{"request":1,"op":"tree"}"#);
    assert_eq!(tree["request"], 1);
    assert!(
        tree.to_string().contains("ticks") && tree.to_string().contains("\"more\""),
        "{tree}"
    );
    let state = d.ask(r#"{"request":2,"op":"state"}"#);
    assert!(!state.to_string().contains("hunter2"), "masked: {state}");
    assert!(state.to_string().contains("•••"), "{state}");
    for (n, op) in [(3, "logs"), (4, "layout"), (5, "tags"), (6, "grants")] {
        let r = d.ask(&format!(r#"{{"request":{n},"op":"{op}"}}"#));
        assert_eq!(r["request"], n);
        assert!(r.get("error").is_none(), "{op}: {r}");
    }
    let picture = d.ask(r#"{"request":7,"op":"screenshot"}"#);
    assert_eq!(
        (picture["width"].as_u64(), picture["capture"].as_str()),
        (Some(200), Some("last-presented-frame")),
        "{picture}"
    );

    // The app's clock goes on by itself between reads: nothing seeks or settles it.
    let wall = |r: &serde_json::Value| {
        r["observation"]["wall"]
            .as_f64()
            .unwrap_or_else(|| panic!("{r}"))
    };
    let before = d.ask(r#"{"request":8,"op":"state"}"#);
    assert!(before["observation"]["clock"].is_number(), "{before}");
    std::thread::sleep(Duration::from_millis(400));
    let after = wall(&d.ask(r#"{"request":9,"op":"state"}"#));
    assert!(after >= wall(&before) + 300., "{} → {after}", wall(&before));

    for (line, code) in [
        (r#"{"request":10,"op":"clock","seek":100}"#, "live-clock"),
        (r#"{"request":11,"op":"clock","settle":true}"#, "live-clock"),
        (r#"{"request":12,"op":"state","reveal":true}"#, "masked"),
        (r#"{"request":13,"op":"tree","session":"main"}"#, "pinned"),
        (r#"{"request":14,"op":"tap","id":3}"#, "unsupported"),
        (
            r#"{"request":15,"op":"type","id":3,"text":"x"}"#,
            "unsupported",
        ),
        (r#"{"request":16,"op":"call","tool":"x"}"#, "unsupported"),
        (r#"{"request":17,"op":"quit"}"#, "unsupported"),
        (r#"{"request":18,"op":"perf","frames":true}"#, "unsupported"),
        (r#"{"request":19,"op":"tree","bogus":1}"#, "malformed"),
        (
            r#"{"request":20,"op":"layout","id":5000000000}"#,
            "malformed",
        ),
        (r#"{"request":21,"op":"tree","op":"quit"}"#, "malformed"),
    ] {
        assert_eq!(d.ask(line)["refused"], code, "{line}");
    }
    assert_eq!(
        d.ask(r#"{"request":22,"op":"tags"}"#)["request"],
        22,
        "a refusal leaves the connection reading"
    );
    // Requests sent together are each answered, in order, one a turn.
    d.send("{\"request\":23,\"op\":\"tags\"}\n{\"request\":24,\"op\":\"grants\"}\n{\"request\":25,\"op\":\"logs\"}");
    for n in 23..=25 {
        assert_eq!(d.read().unwrap()["request"], n);
    }

    // One driver at a time.
    let mut second = Driver::connect(&marker);
    second.send(&serde_json::json!({"attach": 1, "nonce": marker["nonce"]}).to_string());
    assert_eq!(second.read().unwrap()["refused"], "busy");

    // A hangup ends the attachment; the same process asks the next driver again, which the person may refuse.
    drop(d);
    assert!(
        !app.shown.recv_timeout(Duration::from_secs(5)).unwrap(),
        "the window stops showing it"
    );
    let mut again = Driver::connect(&app.marker());
    assert_eq!(again.handshake(&marker)["waiting"], "approval");
    app.person(app.prompt(), false);
    assert_eq!(again.read().unwrap()["refused"], "declined");
    let mut third = Driver::connect(&marker);
    assert_eq!(
        third.handshake(&marker)["pid"],
        std::process::id(),
        "the same process"
    );
    app.person(app.prompt(), true);
    assert_eq!(third.read().unwrap()["attached"], true);
    // A last request and the hangup together: the request is still answered before the connection ends.
    third.send(r#"{"request":1,"op":"tags"}"#);
    third.stream.shutdown(std::net::Shutdown::Write).unwrap();
    assert_eq!(third.read().unwrap()["request"], 1);
}

#[test]
fn only_a_development_build_not_owned_by_the_driver_listens_in_a_private_directory() {
    assert!(admitted(DEVELOPMENT, false).is_ok());
    assert!(admitted(DEVELOPMENT, true)
        .unwrap_err()
        .contains("driver owns"));
    assert!(admitted(r#"{"embedded":{"trust":"production"}}"#, false)
        .unwrap_err()
        .contains("production"));
    assert!(admitted(r#"{"embedded":{"trust":"tester"}}"#, false)
        .unwrap_err()
        .contains("tester"));
    assert!(admitted("{}", false)
        .unwrap_err()
        .contains("names no trust"));
    assert!(admitted("not json", false).is_err());
    assert!(Listener::open(None, DEVELOPMENT, false, "x")
        .err()
        .unwrap()
        .contains("XDG_RUNTIME_DIR"));
    // Admission comes first, and the runtime directory itself must be private.
    let open = std::env::temp_dir().join(format!("exact-attach-open-{}", std::process::id()));
    std::fs::create_dir_all(&open).unwrap();
    std::fs::set_permissions(&open, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(Listener::open(Some(&open), "{}", false, "x")
        .err()
        .unwrap()
        .contains("names no trust"));
    assert!(Listener::open(Some(&open), DEVELOPMENT, true, "x")
        .err()
        .unwrap()
        .contains("driver owns"));
    assert!(Listener::open(Some(&open), DEVELOPMENT, false, "x")
        .err()
        .unwrap()
        .contains("open to other users"));
    assert!(!open.join("exact").exists(), "nothing was made in it");
    let _ = std::fs::remove_dir_all(&open);
}
