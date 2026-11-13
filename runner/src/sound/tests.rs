//! The voice table through a runner (LLP 1096 §5): retrigger, past starts,
//! groups by start time, stops, the bound, drops, a refused commit, reload,
//! the seek rule, no wake, and the record; a voice's playback rate and pan
//! (LLP 1125 D10), and changing them while it plays (LLP 1096 D14).

use crate::sound::{By, SoundOp, Voice};
use crate::{DataError, DataSource, Runner};
use exact_kernel::Kernel;
use exact_plan::{Plan, Value};
use std::path::PathBuf;

const SOURCE: &str = r#"sound "assets/kick.wav"
sound "assets/hat.wav"

shape Hit
  src: string
  at: number
  gain: number
  group: string

shape Tape
  src: string
  at: number
  gain: number
  group: string
  playbackRate: number
  pan: number

component Kit
  state n = 0
  state ahead = 0
  resource hits = hits(n) as shape Hit[]
  resource tapes = tapes(n) as shape Tape[]
  action three
    playSound("assets/kick.wav")
    playSound("assets/kick.wav")
    playSound("assets/kick.wav")
  action list
    playSounds(hits)
  action tapeList
    playSounds(tapes)
  action past
    playSound("assets/kick.wav", at=performanceNow() - 10)
  action computed(s: string, a: number, g: number)
    playSound(s, at=a == -1 ? 0 / 0 : a, gain=g == -2 ? 1 / 0 : g)
  action count(k: number)
    n = k
  action hat(ms: number)
    playSound("assets/hat.wav", at=performanceNow() + ms, group="hat")
  action open(ms: number)
    playSound("assets/kick.wav", at=performanceNow() + ms, group="hat")
  action stop
    stopSounds()
  action stopHats
    stopSounds(group="hat")
  action bad
    playSound("assets/kick.wav")
    n = 99
  action again
    reload()
  action tape(rate: number, pan: number)
    playSound("assets/kick.wav", playbackRate=rate == -1 ? 0 / 0 : rate, pan=pan == -9 ? 1 / 0 : pan)
  action slowHat(ms: number)
    playSound("assets/hat.wav", at=performanceNow() + ms, group="hat", playbackRate=0.5, pan=0.25)
  action kickIn
    playSound("assets/kick.wav", group="k")
  action glide(g: string, rate: number, ramp: number)
    setSounds(group=g, playbackRate=rate, ramp=ramp)
  action level(g: number)
    setSounds(gain=g, pan=-0.5)
  action playSet
    playSound("assets/kick.wav", group="ps")
    setSounds(group="ps", playbackRate=2, gain=0.5)
  action badSet
    setSounds(gain=0 / 0)
  action window
    playSounds(hits)
    ahead = performanceNow() + 100
  task clock mount
    every(25, window)
  view
    text "kit"
"#;

/// The kit's hits: `n` of them, 50 ms apart from the source's `at`, each in
/// group "g"; `99` answers a string, a shape refusal.
struct Kit {
    at: f64,
}

impl DataSource for Kit {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        let n = match args.first() {
            Some(Value::Number(n)) => *n,
            _ => 0.0,
        };
        // A tape: the kit's hit with a rate and a pan, 2 and 0.5 (LLP 1125 D10).
        if source == "tapes" {
            return Ok(Value::list(
                (0..n as usize)
                    .map(|i| {
                        Value::record(vec![
                            Value::str("assets/kick.wav"),
                            Value::Number(100.0 * i as f64),
                            Value::Number(1.0),
                            Value::str(""),
                            Value::Number(2.0),
                            Value::Number(0.5),
                        ])
                    })
                    .collect(),
            ));
        }
        assert_eq!(source, "hits");
        if n == 99.0 {
            return Ok(Value::str("not a list"));
        }
        Ok(Value::list(
            (0..n as usize)
                .map(|i| {
                    Value::record(vec![
                        Value::str("assets/hat.wav"),
                        Value::Number(self.at + 50.0 * i as f64),
                        Value::Number(1.0),
                        Value::str(""),
                    ])
                })
                .collect(),
        ))
    }
}

/// Write a 16-bit mono WAV of `ms` at 48 kHz.
fn wav(path: &std::path::Path, ms: u32) {
    let frames = 48 * ms;
    let data = frames * 2;
    let mut b = Vec::new();
    b.extend_from_slice(b"RIFF");
    b.extend_from_slice(&(36 + data).to_le_bytes());
    b.extend_from_slice(b"WAVEfmt ");
    b.extend_from_slice(&16u32.to_le_bytes());
    for x in [1u16, 1] {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b.extend_from_slice(&48_000u32.to_le_bytes());
    b.extend_from_slice(&96_000u32.to_le_bytes());
    for x in [2u16, 16] {
        b.extend_from_slice(&x.to_le_bytes());
    }
    b.extend_from_slice(b"data");
    b.extend_from_slice(&data.to_le_bytes());
    b.resize(b.len() + data as usize, 0);
    std::fs::write(path, b).unwrap();
}

/// The app's directory: the source and a 250 ms kick and 100 ms hat.
fn app(source: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU32, Ordering};
    static N: AtomicU32 = AtomicU32::new(0);
    let dir = std::env::temp_dir().join(format!(
        "exact-sound-{}-{}",
        std::process::id(),
        N.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir_all(dir.join("assets")).unwrap();
    wav(&dir.join("assets/kick.wav"), 250);
    wav(&dir.join("assets/hat.wav"), 100);
    std::fs::write(dir.join("app.contract"), source).unwrap();
    dir
}

/// The plan of `source` in an app directory with the kit's sounds, compiled once per source
/// for the whole binary: the tests boot fresh runners over a few sources many times.
fn compiled(source: &str) -> Plan {
    static PLANS: std::sync::Mutex<Vec<(String, Plan)>> = std::sync::Mutex::new(Vec::new());
    let mut plans = PLANS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, plan)) = plans.iter().find(|(s, _)| s == source) {
        return plan.clone();
    }
    let dir = app(source);
    let plan = contract::compile_path(&dir.join("app.contract")).unwrap();
    let _ = std::fs::remove_dir_all(&dir);
    plans.push((source.to_owned(), plan.clone()));
    plan
}

fn boot_with(source: &str, n: f64) -> Runner<Kit> {
    let mut r = Runner::boot(
        compiled(source),
        Kit { at: 0.0 },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    if n > 0.0 {
        r.act("count", vec![Value::Number(n)]).unwrap();
    }
    r
}

fn boot() -> Runner<Kit> {
    boot_with(SOURCE, 0.0)
}

fn voices(r: &Runner<Kit>) -> Vec<Voice> {
    r.sounds().voices().cloned().collect()
}

fn ends(r: &Runner<Kit>) -> Vec<(u64, f64, By)> {
    r.sounds().voices().map(|v| (v.id, v.ends, v.by)).collect()
}

fn journal(r: &Runner<Kit>) -> Vec<String> {
    r.journal()
        .filter(|l| l.contains("sound"))
        .map(|l| l.split_once(' ').map_or(l, |(_, rest)| rest).to_string())
        .collect()
}

#[test]
fn every_call_is_a_new_voice() {
    let mut r = boot();
    r.act("three", vec![]).unwrap();
    let v = voices(&r);
    assert_eq!(v.iter().map(|v| v.id).collect::<Vec<_>>(), [1, 2, 3]);
    assert!(v
        .iter()
        .all(|v| v.at == 0.0 && v.ends == 250.0 && v.by == By::End));
    // A list of three is three voices too, in list order.
    let mut r = boot_with(SOURCE, 3.0);
    r.act("list", vec![]).unwrap();
    let ats: Vec<f64> = voices(&r).iter().map(|v| v.at).collect();
    assert_eq!(ats, [0.0, 50.0, 100.0]);
    // The authored commands still reach the host, which skips them.
    let names: Vec<String> = r.take_commands().into_iter().map(|c| c.name).collect();
    assert_eq!(names, ["playSounds"]);
    let ops = r.take_sounds();
    assert_eq!(ops.len(), 3);
    assert!(
        matches!(ops[1], SoundOp::Play { id: 2, sound: 1, at, gain, playback_rate, pan, .. } if at == 50.0 && gain == 1.0 && playback_rate == 1.0 && pan == 0.0)
    );
}

#[test]
fn a_past_start_is_now_and_says_what_was_asked() {
    let mut r = boot();
    r.advance(1000.0).unwrap();
    r.act("past", vec![]).unwrap();
    assert_eq!(voices(&r).last().unwrap().at, 1000.0);
    let lines = journal(&r);
    assert!(
        lines
            .iter()
            .any(|l| l.ends_with("assets/kick.wav at 1000 gain 1 (asked 990)")),
        "{lines:?}"
    );
}

#[test]
fn a_group_cuts_by_start_time_whatever_the_order_of_calls() {
    let mut r = boot();
    // 100 ms ahead first, then 50 ms ahead: the earlier start is cut by the later.
    r.act("hat", vec![Value::Number(100.0)]).unwrap();
    r.act("hat", vec![Value::Number(50.0)]).unwrap();
    assert_eq!(ends(&r), [(1, 200.0, By::End), (2, 100.0, By::Group)]);
    // A tie goes to the later call: the earlier never sounds.
    r.act("hat", vec![Value::Number(100.0)]).unwrap();
    assert_eq!(ends(&r)[0], (1, 100.0, By::Cut));
    assert_eq!(ends(&r)[2], (3, 200.0, By::End));
    // Another source in the same group chokes it.
    r.act("open", vec![Value::Number(120.0)]).unwrap();
    assert_eq!(ends(&r)[2], (3, 120.0, By::Group));
    let lines = journal(&r);
    assert!(
        lines.contains(&"sound 2 ends at 100 (group hat)".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"sound 1 ends at 100 (group hat)".to_string()),
        "{lines:?}"
    );
    // The output hears each end that moved earlier, once.
    let ops = r.take_sounds();
    assert!(ops.contains(&SoundOp::End { id: 1, at: 100.0 }));
    assert!(ops.contains(&SoundOp::End { id: 3, at: 120.0 }));
}

#[test]
fn stop_ends_the_sounding_and_cancels_the_waiting() {
    let mut r = boot();
    r.act("three", vec![]).unwrap();
    r.act("hat", vec![Value::Number(500.0)]).unwrap();
    r.advance(100.0).unwrap();
    r.act("stopHats", vec![]).unwrap();
    assert_eq!(ends(&r)[3], (4, 500.0, By::Cancelled));
    assert_eq!(ends(&r)[0], (1, 250.0, By::End));
    r.act("stop", vec![]).unwrap();
    assert_eq!(ends(&r)[0], (1, 100.0, By::Stop));
    let lines = journal(&r);
    assert!(
        lines.contains(&"sounds stopped: 1 (group hat)".to_string()),
        "{lines:?}"
    );
    assert!(
        lines.contains(&"sounds stopped: 3".to_string()),
        "{lines:?}"
    );
    assert_eq!(r.sounds().live(100.0), 0);
}

#[test]
fn the_thirty_third_live_voice_is_dropped_and_an_ended_one_frees_its_place() {
    let mut r = boot();
    for _ in 0..10 {
        r.act("three", vec![]).unwrap();
    }
    r.act("past", vec![]).unwrap();
    r.act("past", vec![]).unwrap();
    assert_eq!(r.sounds().live(0.0), 32);
    r.act("past", vec![]).unwrap();
    assert_eq!(voices(&r).len(), 32);
    assert!(journal(&r).contains(
        &"sound dropped: 32 voices sound at its start (assets/kick.wav at 0)".to_string()
    ));
    r.advance(250.0).unwrap();
    r.act("past", vec![]).unwrap();
    assert_eq!(voices(&r).last().unwrap().id, 33);
}

#[test]
fn a_bad_call_is_dropped_and_the_commit_stands() {
    let mut r = boot();
    let call = |r: &mut Runner<Kit>, s: &str, a: f64, g: f64| {
        r.act(
            "computed",
            vec![Value::str(s), Value::Number(a), Value::Number(g)],
        )
        .unwrap();
    };
    call(&mut r, "assets/nope.wav", 0.0, 1.0);
    // NaN and an infinity, as a computation makes them.
    call(&mut r, "assets/kick.wav", -1.0, 1.0);
    call(&mut r, "assets/kick.wav", 0.0, -2.0);
    // A finite gain outside 0–1 is clamped, not dropped.
    call(&mut r, "assets/kick.wav", 0.0, 3.0);
    assert_eq!(voices(&r).len(), 1);
    assert_eq!(voices(&r)[0].gain, 1.0);
    let lines = journal(&r);
    for want in [
        "sound dropped: \"assets/nope.wav\" is not a declared sound",
        "sound dropped: at is not a number (assets/kick.wav)",
        "sound dropped: gain is not a number (assets/kick.wav)",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}: {lines:?}");
    }
}

#[test]
fn a_refused_commit_schedules_nothing() {
    let mut r = boot();
    assert!(r.act("bad", vec![]).is_err());
    assert!(voices(&r).is_empty());
    assert!(r.take_sounds().is_empty());
    assert!(
        journal(&r).iter().all(|l| !l.starts_with("sound ")),
        "{:?}",
        journal(&r)
    );
}

#[test]
fn reload_ends_every_live_voice() {
    let mut r = boot();
    r.act("three", vec![]).unwrap();
    r.act("hat", vec![Value::Number(500.0)]).unwrap();
    r.take_sounds();
    r.act("again", vec![]).unwrap();
    assert!(journal(&r).contains(&"sounds stopped: 4 (reload)".to_string()));
    let ops = r.take_sounds();
    assert_eq!(ops.len(), 4, "{ops:?}");
    assert!(ops.contains(&SoundOp::End { id: 4, at: 500.0 }));
    // A host's dev reload asks the same of the runner it replaces.
    r.act("three", vec![]).unwrap();
    r.end_sounds();
    assert_eq!(r.sounds().live(0.0), 0);
}

#[test]
fn one_long_seek_and_many_short_ones_give_one_table() {
    // The two runners are independent: seek them on their own threads. A
    // runner stays on its thread; what it heard and let go crosses back.
    let ((one, one_evicted), (many, many_evicted)) = std::thread::scope(|scope| {
        let one = scope.spawn(|| {
            let mut one = boot_with(SOURCE, 2.0);
            one.advance(60_000.0).unwrap();
            (voices(&one), one.sounds().evicted())
        });
        let mut many = boot_with(SOURCE, 2.0);
        for k in 1..=60 {
            many.advance(1_000.0 * k as f64).unwrap();
        }
        (
            one.join().unwrap(),
            (voices(&many), many.sounds().evicted()),
        )
    });
    assert_eq!(one, many);
    assert_eq!(one_evicted, many_evicted);
    // Each window's hits land at the commit's time: a timer's due time.
    assert_eq!(one.last().unwrap().at, 60_000.0);
}

#[test]
fn a_voice_is_not_a_timer() {
    let mut r = boot_with(
        &SOURCE.replace("  task clock mount\n    every(25, window)\n", ""),
        0.0,
    );
    assert_eq!(r.timer_due_ms(), None);
    r.act("hat", vec![Value::Number(5_000.0)]).unwrap();
    assert_eq!(r.timer_due_ms(), None);
}

#[test]
fn the_record_keeps_1024_voices_and_counts_what_it_let_go() {
    let mut r = boot_with(
        &SOURCE.replace("  task clock mount\n    every(25, window)\n", ""),
        0.0,
    );
    for k in 0..400 {
        r.advance(300.0 * k as f64).unwrap();
        r.act("three", vec![]).unwrap();
    }
    assert_eq!(voices(&r).len(), 1024);
    assert_eq!(r.sounds().evicted().0, 1200 - 1024);
    assert_eq!(voices(&r)[0].id, 1200 - 1024 + 1);
    let state = crate::agent::handle(&r, r#"{"op":"state","sounds":"all"}"#);
    assert!(state.contains("\"recorded\":1024"), "{}", &state[..200]);
    let shown = crate::agent::handle(&r, r#"{"op":"state"}"#);
    assert_eq!(shown.matches("\"src\":").count(), 64);
}

#[test]
fn a_playback_rate_moves_the_end_as_on_tape_and_pan_rides_along() {
    let mut r = boot();
    let tape = |r: &mut Runner<Kit>, rate: f64, pan: f64| {
        r.act("tape", vec![Value::Number(rate), Value::Number(pan)])
            .unwrap();
    };
    // The 250 ms kick at twice the rate lasts 125 ms; at half, 500.
    tape(&mut r, 2.0, -0.5);
    tape(&mut r, 0.5, 0.0);
    // A computed rate or pan outside its range is clamped before it is stored.
    tape(&mut r, 10.0, -3.0);
    tape(&mut r, 0.0, 2.0);
    let v = voices(&r);
    let got: Vec<(f64, f64, f64)> = v.iter().map(|v| (v.playback_rate, v.pan, v.ends)).collect();
    assert_eq!(
        got,
        [
            (2.0, -0.5, 125.0),
            (0.5, 0.0, 500.0),
            (4.0, -1.0, 62.5),
            (0.25, 1.0, 1000.0)
        ]
    );
    let lines = journal(&r);
    for want in [
        "sound 1 assets/kick.wav at 0 gain 1 playbackRate 2 pan -0.5",
        "sound 2 assets/kick.wav at 0 gain 1 playbackRate 0.5",
        "sound 3 assets/kick.wav at 0 gain 1 playbackRate 4 pan -1",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}: {lines:?}");
    }
    let ops = r.take_sounds();
    assert!(matches!(
        ops[0],
        SoundOp::Play { id: 1, playback_rate, pan, .. } if playback_rate == 2.0 && pan == -0.5
    ));
    // Non-finite: the voice is dropped and the commit stands.
    tape(&mut r, -1.0, 0.0);
    tape(&mut r, 1.0, -9.0);
    assert_eq!(voices(&r).len(), 4);
    let lines = journal(&r);
    for want in [
        "sound dropped: playbackRate is not a number (assets/kick.wav)",
        "sound dropped: pan is not a number (assets/kick.wav)",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}: {lines:?}");
    }
    // `state.sounds` names the rate and pan only when they are not 1 and 0.
    let state = crate::agent::handle(&r, r#"{"op":"state","sounds":"all"}"#);
    assert!(
        state.contains(r#""group":null,"playbackRate":2,"pan":-0.5,"ends":125,"#),
        "{state}"
    );
    assert!(
        state.contains(r#""group":null,"playbackRate":0.5,"ends":500,"#),
        "{state}"
    );
    // A hit's shape may carry the rate and pan after its group.
    let mut r = boot_with(SOURCE, 2.0);
    r.act("tapeList", vec![]).unwrap();
    let got: Vec<(f64, f64, f64)> = voices(&r)
        .iter()
        .map(|v| (v.playback_rate, v.pan, v.ends))
        .collect();
    assert_eq!(got, [(2.0, 0.5, 125.0), (2.0, 0.5, 225.0)]);
}

#[test]
fn a_group_cuts_at_the_end_a_rate_gives() {
    let mut r = boot();
    // The 100 ms hat at half the rate would ring to 200; the next hat at 150 cuts it.
    r.act("slowHat", vec![Value::Number(0.0)]).unwrap();
    assert_eq!(ends(&r), [(1, 200.0, By::End)]);
    r.act("hat", vec![Value::Number(150.0)]).unwrap();
    assert_eq!(ends(&r)[0], (1, 150.0, By::Group));
    // One at 250 starts after the slow hat's end: nothing to cut.
    r.act("slowHat", vec![Value::Number(250.0)]).unwrap();
    r.act("hat", vec![Value::Number(460.0)]).unwrap();
    assert_eq!(ends(&r)[2], (3, 450.0, By::End));
}

fn glide(r: &mut Runner<Kit>, group: &str, rate: f64, ramp: f64) {
    let args = vec![Value::str(group), Value::Number(rate), Value::Number(ramp)];
    r.act("glide", args).unwrap();
}

#[test]
fn a_rate_change_moves_the_end_by_what_is_left_of_the_file() {
    let mut r = boot();
    r.act("kickIn", vec![]).unwrap();
    r.take_sounds();
    // 100 ms into the 250 ms kick, half speed at once: 150 ms of file take 300.
    r.advance(100.0).unwrap();
    glide(&mut r, "k", 0.5, 0.0);
    assert_eq!(ends(&r), [(1, 400.0, By::End)]);
    let ops = r.take_sounds();
    assert_eq!(
        ops,
        [
            SoundOp::End { id: 1, at: 400.0 },
            SoundOp::Set {
                id: 1,
                at: 100.0,
                gain: None,
                playback_rate: Some(0.5),
                pan: None,
                ramp: 0.0
            },
        ]
    );
    // 100 ms later 150 ms of file has played; back to 2 over 100 ms. The
    // ramp would cover 125 ms of file and 100 are left, so the end is inside
    // it: 0.5·s + 0.0075·s² = 100.
    r.advance(200.0).unwrap();
    glide(&mut r, "k", 2.0, 100.0);
    let end = ends(&r)[0].1;
    let s = (-0.5 + (0.25f64 + 3.0).sqrt()) / 0.015;
    assert!((end - 200.0 - s).abs() < 1e-9, "{end}");
    let lines = journal(&r);
    for want in [
        "sounds set: 1 (group k) playbackRate 0.5 ramp 0",
        "sound 1 ends at 400 (end)",
        "sounds set: 1 (group k) playbackRate 2 ramp 100",
    ] {
        assert!(lines.iter().any(|l| l == want), "{want}: {lines:?}");
    }
    // Another group, or one that has ended, is not changed.
    glide(&mut r, "hat", 1.0, 0.0);
    r.advance(400.0).unwrap();
    glide(&mut r, "k", 1.0, 0.0);
    let lines = journal(&r);
    assert!(
        lines
            .iter()
            .any(|l| l == "sounds set: 0 (group hat) playbackRate 1 ramp 0"),
        "{lines:?}"
    );
    assert!(
        lines
            .iter()
            .any(|l| l == "sounds set: 0 (group k) playbackRate 1 ramp 0"),
        "{lines:?}"
    );
}

#[test]
fn a_change_reaches_what_waits_outright_and_what_the_commit_made_through_its_play() {
    let mut r = boot();
    r.act("hat", vec![Value::Number(500.0)]).unwrap();
    r.take_sounds();
    // A computed gain is clamped; the waiting hat takes both values outright.
    r.act("level", vec![Value::Number(2.0)]).unwrap();
    let v = &voices(&r)[0];
    assert_eq!((v.gain, v.pan, v.ends), (1.0, -0.5, 600.0));
    assert_eq!(
        r.take_sounds(),
        [SoundOp::Set {
            id: 1,
            at: 0.0,
            gain: Some(1.0),
            playback_rate: None,
            pan: Some(-0.5),
            ramp: 20.0
        }]
    );
    glide(&mut r, "hat", 0.5, 50.0);
    assert_eq!(
        ends(&r),
        [(1, 700.0, By::End)],
        "a voice not started has no ramp"
    );
    r.take_sounds();
    // One made in the same commit plays with the values: no change op.
    r.act("playSet", vec![]).unwrap();
    let v = voices(&r)[1].clone();
    assert_eq!((v.gain, v.playback_rate, v.ends), (0.5, 2.0, 125.0));
    let ops = r.take_sounds();
    assert!(
        matches!(
            ops[..],
            [SoundOp::Play { id: 2, gain, playback_rate, .. }] if gain == 0.5 && playback_rate == 2.0
        ),
        "{ops:?}"
    );
    // A non-finite value changes nothing, and the commit stands.
    r.act("badSet", vec![]).unwrap();
    assert!(journal(&r)
        .iter()
        .any(|l| l == "sounds not set: gain is not a number"));
    assert_eq!(voices(&r)[1].gain, 0.5);
}

#[test]
fn a_slower_voice_is_cut_by_the_next_in_its_group() {
    let mut r = boot();
    r.act("hat", vec![Value::Number(0.0)]).unwrap();
    r.act("hat", vec![Value::Number(150.0)]).unwrap();
    assert_eq!(ends(&r), [(1, 100.0, By::End), (2, 250.0, By::End)]);
    // 50 ms in, a quarter speed: 50 ms of file would take 200, past the next hat.
    r.advance(50.0).unwrap();
    glide(&mut r, "hat", 0.25, 0.0);
    assert_eq!(ends(&r), [(1, 150.0, By::Group), (2, 550.0, By::End)]);
    // Back to full speed: it ends on its own again, before the next hat.
    glide(&mut r, "hat", 1.0, 0.0);
    assert_eq!(ends(&r), [(1, 100.0, By::End), (2, 250.0, By::End)]);
}

#[test]
fn a_slower_voice_counts_against_the_bound_at_later_starts() {
    let mut r = boot();
    for _ in 0..10 {
        r.act("three", vec![]).unwrap();
    }
    r.act("kickIn", vec![]).unwrap();
    r.act("past", vec![]).unwrap();
    r.act("hat", vec![Value::Number(300.0)]).unwrap();
    assert_eq!(r.sounds().live(0.0), 33);
    // Every kick now rings to 1000, so 32 sound at the hat's start: it loses.
    glide(&mut r, "", 0.25, 0.0);
    assert_eq!(ends(&r)[32], (33, 300.0, By::Cancelled));
    assert!(journal(&r)
        .iter()
        .any(|l| l == "sound dropped: 32 voices sound at its start (assets/hat.wav at 300)"));
}
