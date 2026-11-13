//! `harness --print --prompt "…"` (LLP 1101.003 §5), run as a script runs
//! it, on the scripted mock model: one prompt, the transcript written as it
//! settles, the exit status saying whether the turn ended well.

use std::process::{Command, Output, Stdio};

/// The harness on the mock, with no key from this machine's environment or
/// configuration.
fn harness(name: &str, model: &str, args: &[&str]) -> Output {
    let home = std::env::temp_dir().join(format!(
        "harness-command-line-{}-{name}",
        std::process::id()
    ));
    std::fs::create_dir_all(&home).unwrap();
    // The mock's tour reads a small file, then asks to run a command.
    std::fs::write(home.join("notes.txt"), "a small file\n").unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_harness"));
    for key in ["ANTHROPIC_API_KEY", "OPENAI_API_KEY", "OPENROUTER_API_KEY"] {
        command.env_remove(key);
    }
    command
        .env("HOME", &home)
        .env("EXACT_HARNESS_MODEL", model)
        .current_dir(&home)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .expect("runs")
}

fn text(bytes: &[u8]) -> String {
    String::from_utf8_lossy(bytes).into_owned()
}

/// A turn with no tool: the reply streams, the run settles, exit 0. The
/// mock's silent start (it thinks before its first word) holds the run, or
/// the reply would be missing.
#[test]
fn a_prompt_prints_its_reply_and_exits_zero() {
    let out = harness("long", "mock", &["--print", "--prompt", "long"]);
    let printed = text(&out.stdout);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}\n{printed}",
        text(&out.stderr)
    );
    assert!(printed.contains("› long"), "{printed}");
    assert!(printed.contains("⏺ mock"), "{printed}");
    assert!(!printed.contains('\x1b'), "a pipe takes plain text");
    // Nothing that waits for a person is drawn.
    assert!(!printed.contains("Ask anything"), "{printed}");
}

/// A tool that needs approval is refused, not awaited: the turn stops with
/// an error entry, the head says 503 and the run exits 1 (D8). The tool
/// rounds before it, and the gaps between them, held the run.
#[test]
fn a_tool_needing_approval_ends_the_run_with_one() {
    let out = harness("approval", "mock", &["--print", "--prompt", "look around"]);
    let printed = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{printed}");
    assert!(
        printed.contains("List(.)") && printed.contains("Bash(ls -la)"),
        "{printed}"
    );
    assert!(printed.contains("needs approval"), "{printed}");
    assert!(
        text(&out.stderr).contains("status 503 at /?prompt=look%20around"),
        "{}",
        text(&out.stderr)
    );
    let state: serde_json::Value = serde_json::from_slice(
        &harness(
            "approval-json",
            "mock",
            &["--json", "--prompt", "look around"],
        )
        .stdout,
    )
    .expect("one object");
    assert_eq!(
        state["exit"],
        serde_json::json!({"status": 1, "reason": "settled", "head": 503})
    );
    assert_eq!(state["slots"]["keyDraft"], "");
}

/// `--prompt` is model input: a slash command is asked of the model, never
/// run (D4).
#[test]
fn a_slash_prompt_is_asked_literally() {
    let out = harness("slash", "mock", &["--print", "--prompt", "/model gpt"]);
    let printed = text(&out.stdout);
    assert!(printed.contains("› /model gpt"), "{printed}");
    assert!(
        printed.contains("⏺ mock"),
        "the model answered it: {printed}"
    );
}

/// A model whose provider has no key: no turn starts, the error entry
/// fails the session, exit 1.
#[test]
fn a_failed_provider_route_exits_one() {
    let out = harness("route", "gpt-5.5", &["--print", "--prompt", "hi"]);
    let printed = text(&out.stdout);
    assert_eq!(out.status.code(), Some(1), "{printed}");
    assert!(printed.contains("Can't reach"), "{printed}");
}

/// A timeout mid-reply: `beforeunload` interrupts the turn, whose thread
/// leaves within the shutdown's second, and the run exits 124 (D2).
#[test]
fn a_timeout_interrupts_the_turn() {
    let out = harness(
        "timeout",
        "mock",
        &["--print", "--prompt", "long", "--timeout", "3"],
    );
    let printed = text(&out.stdout);
    assert_eq!(out.status.code(), Some(124), "{printed}");
    // A forced exit writes nothing more: the note says the shutdown ran.
    assert!(printed.contains("[interrupted]"), "{printed}");
}
