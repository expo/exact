//! The real harness screen boots against this data module (LLP 1101.002
//! P2): `terminal.contract`, compiled as the terminal host compiles it,
//! takes `session(mode)` and `animation(...)` through the shape check.

use exact_kernel::{Kernel, PropId};
use exact_runner::{DataSource, Event, Runner};
use exact_terminal::host::{Host, Mode};
use exact_terminal::vt::Vt;
use harness_data::shapes::{ContractValue, Session};
use harness_data::{Harness, Options};

#[test]
fn the_real_screen_boots_and_reads_the_session() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../terminal.contract");
    let plan = contract::compile_path_terminal(&path).unwrap_or_else(|errors| {
        panic!(
            "terminal.contract does not compile: {}",
            errors
                .iter()
                .map(|e| e.to_string())
                .collect::<Vec<_>>()
                .join("\n")
        )
    });
    let dir = std::env::temp_dir().join(format!("exact-harness-screen-{}", std::process::id()));
    let harness = Harness::with_options(Options::offline(Some(dir.clone())));
    let mut r = Runner::boot(
        plan,
        harness,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap_or_else(|e| panic!("the screen refused to boot: {e:?}"));
    r.data_ready().unwrap();
    let session = Session::from_value(r.resource("session").expect("a session resource"))
        .expect("the session is a Session");
    assert_eq!(session.entries[0].kind, "banner");
    assert!(session.models.iter().any(|m| m.id == "mock"));
    let prompt = r.kernel().find_by_test_id("prompt")[0];
    let prompt = r.kernel().node_by_key(prompt).unwrap().id;
    r.dispatch(prompt, Event::Input("/".into())).unwrap();
    let props = &r.kernel().node(prompt).unwrap().props;
    assert_eq!(props.str(PropId::AccessibilityRole), Some("combobox"));
    assert_eq!(
        props.str(PropId::AccessibilityControls),
        Some("completions")
    );
    assert_eq!(props.bool(PropId::AccessibilityExpanded), Some(true));
    assert_eq!(
        props.str(PropId::AccessibilityActiveDescendant),
        Some("complete-/help")
    );
    r.dispatch(prompt, Event::key("ArrowDown")).unwrap();
    assert_eq!(
        r.kernel()
            .node(prompt)
            .unwrap()
            .props
            .str(PropId::AccessibilityActiveDescendant),
        Some("complete-/model")
    );
    let option = r.kernel().find_by_test_id("complete-/model")[0];
    let props = &r.kernel().node_by_key(option).unwrap().props;
    assert_eq!(props.str(PropId::Id), Some("complete-/model"));
    assert_eq!(props.str(PropId::AccessibilityRole), Some("option"));
    assert_eq!(props.bool(PropId::AccessibilitySelected), Some(true));
    assert_eq!(
        props.get(PropId::TabIndex).and_then(|v| v.as_int()),
        Some(-1)
    );
    assert_eq!(props.bool(PropId::RetainFocus), Some(true));
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn an_inline_burst_reaches_scrollback_in_bounded_batches() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../terminal.contract");
    let plan = contract::compile_path_terminal(&path).unwrap();
    let mut harness = Harness::with_options(Options::offline(None));
    let mut host = Host::boot(plan, harness.clone(), Mode::Inline, 100, 30).unwrap();
    host.listen(std::sync::Arc::new(|| {}));
    let start = std::time::Instant::now();
    while Session::from_value(&harness.session())
        .unwrap()
        .entries
        .iter()
        .any(|entry| entry.kind == "banner" && entry.busy)
    {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        host.announced();
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    let mut vt = Vt::new(&mut host);
    vt.render(&mut host);
    harness
        .query("submit", &[exact_plan::Value::str("/stress 95")])
        .unwrap();
    host.announced();
    let mut frames = 0;
    while !Session::from_value(&harness.session())
        .unwrap()
        .entries
        .is_empty()
    {
        assert!(host.transcript().len() <= 32, "mounted the whole burst");
        vt.render(&mut host);
        let due = host.timer_due_ms().expect("printed entries can retire");
        host.tick(due);
        host.announced();
        frames += 1;
        assert!(frames < 20, "the burst stopped draining");
    }
    vt.render(&mut host);
    assert!(frames >= 3, "the burst used more than one mounted batch");
    let text = vt.text(true);
    let mut cursor = 0;
    for i in 0..95 {
        let marker = if i % 9 == 4 {
            format!("Bash(seq {i})")
        } else if i % 11 == 0 {
            format!("Prompt number {i}\n")
        } else {
            format!("#{i} ")
        };
        assert_eq!(
            text.matches(&marker).count(),
            1,
            "{marker}: missing or repeated"
        );
        cursor += text[cursor..].find(&marker).expect("entries stay in order") + marker.len();
    }
    assert_eq!(vt.unanswered(), 0);
}

/// The busy line's figures, `(seconds, tokens)`, as `vt` shows them.
fn busy_line(vt: &Vt) -> Option<(String, String)> {
    let text = vt.text(false);
    let line = text.lines().find(|l| l.ends_with("esc to interrupt)"))?;
    let (_, figures) = line.rsplit_once(" (")?;
    let (seconds, rest) = figures.split_once(" · ↓ ")?;
    let (tokens, _) = rest.split_once(" tokens")?;
    Some((seconds.to_string(), tokens.to_string()))
}

#[test]
fn each_turn_counts_its_own_seconds_and_tokens() {
    use exact_terminal::host::Key;
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../terminal.contract");
    let plan = contract::compile_path_terminal(&path).unwrap();
    let mut harness = Harness::with_options(Options::offline(None));
    let mut host = Host::boot(plan, harness.clone(), Mode::Inline, 100, 30).unwrap();
    host.listen(std::sync::Arc::new(|| {}));
    let mut vt = Vt::new(&mut host);
    let session = |h: &Harness| Session::from_value(&h.session()).unwrap();
    // The prompt has the focus; a key reaches what a drawn frame showed.
    let send = |host: &mut Host<Harness>, vt: &mut Vt, now: f64, text: &str| {
        host.tick(now);
        vt.render(host);
        host.read_at = host.frames;
        for c in text.chars() {
            host.key(Key::Char(c));
        }
        host.key(Key::Named("Enter"));
        host.announced();
    };
    let mut now = 0.0;
    // The first turn runs 2.6 s on the app's clock and writes its answer,
    // then is interrupted.
    send(&mut host, &mut vt, now, "hello");
    assert!(session(&harness).busy, "the first turn started");
    while now < 2600.0 {
        now += 100.0;
        host.tick(now);
        host.announced();
    }
    vt.render(&mut host);
    assert_eq!(busy_line(&vt).map(|l| l.0).as_deref(), Some("2s"));
    let start = std::time::Instant::now();
    while session(&harness).tokens_out == 0.0 {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    harness.query("interrupt", &[]).unwrap();
    while session(&harness).busy {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(1));
    }
    // What the data module changed lands at the next announcement.
    while busy_line(&vt).is_some() || host.by_test_id("approval").is_some() {
        assert!(start.elapsed() < std::time::Duration::from_secs(10));
        std::thread::sleep(std::time::Duration::from_millis(1));
        host.announced();
        vt.render(&mut host);
    }
    // The second counts from its own send, whatever the first took.
    now += 5000.0;
    send(&mut host, &mut vt, now, "again");
    assert!(session(&harness).busy, "the second turn started");
    for _ in 0..3 {
        now += 100.0;
        host.tick(now);
        host.announced();
    }
    vt.render(&mut host);
    assert_eq!(busy_line(&vt), Some(("0s".into(), "0".into())));
}
