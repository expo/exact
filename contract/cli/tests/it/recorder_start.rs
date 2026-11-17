//! Recorder's own `app.contract` when its recorder does not start: the
//! microphone it opened for the take is stopped and the refusal shown, a
//! retry records, and a successful start and stop still stop the track.
//! `@exact/media-capture` and `@exact/recording` are played by the native
//! slot under the agent; `startMediaRecorder` refuses as many times as asked.

use exact_runner::{Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

struct Granted;
impl exact_runner::DataSource for Granted {
    fn query(
        &mut self,
        source: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(source.into()))
    }
    fn answer(
        &mut self,
        _: &mut exact_runner::Store,
        _: &str,
        _: &[exact_runner::Value],
    ) -> Result<exact_runner::Answer, exact_runner::DataError> {
        // app.ts's takes stay pending; this drives the module seam.
        Ok(exact_runner::Answer::Later(exact_runner::Request::native(
            Vec::new(),
        )))
    }
    fn grants(&self) -> &str {
        // app.ts's lines.
        "device.microphone purpose.microphone\ndevice.speech-recognition purpose.transcribe\nfs.read app:/data/takes\nfs.write app:/data/takes\n"
    }
}

type Calls = Arc<Mutex<Vec<Json>>>;

fn track(id: &str, state: &str) -> Json {
    json!({ "id": id, "kind": "audio", "label": "Microphone", "enabled": true, "muted": false,
        "readyState": state, "settings": {} })
}

/// Recorder over the native slot; `refusals` starts are refused `NotSupportedError`.
fn boot(refusals: usize) -> (Runner<Granted>, Calls) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/recorder/app.contract");
    let (plan, _) = contract::compile_path_all(&path, false).unwrap_or_else(|e| panic!("{e:?}"));
    let mut r = Runner::boot(
        plan,
        Granted,
        exact_kernel::Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let calls: Calls = Arc::default();
    let seen = calls.clone();
    let refusals = Arc::new(AtomicUsize::new(refusals));
    let streams = AtomicUsize::new(0);
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            // app.ts's requests (`Granted`'s empty bodies) stay pending.
            let Ok(v) = serde_json::from_slice::<Json>(&body) else {
                return;
            };
            seen.lock().unwrap().push(v.clone());
            let answer = match v["send"].as_str() {
                Some("getUserMedia") => {
                    let n = streams.fetch_add(1, Ordering::SeqCst) + 1;
                    json!({ "id": format!("stream-{n}"), "active": true,
                        "tracks": [track(&format!("track-{n}"), "live")] })
                }
                Some("startMediaRecorder")
                    if refusals
                        .try_update(Ordering::SeqCst, Ordering::SeqCst, |n| n.checked_sub(1))
                        .is_ok() =>
                {
                    json!({ "error": "NotSupportedError" })
                }
                Some("startMediaRecorder") => json!({ "id": "recorder-1", "state": "recording",
                    "mimeType": "audio/wav", "path": "app:/data/takes/take-0.wav", "duration": 0 }),
                Some("stopMediaRecorder") => json!({ "path": "app:/data/takes/take-0.wav",
                    "mimeType": "audio/wav", "duration": 1000, "size": 1 }),
                Some("stopTrack") => track(v["args"]["track"].as_str().unwrap_or(""), "ended"),
                _ => json!({}),
            };
            reply.send(Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: answer.to_string().into_bytes(),
            }));
        })));
    r.bind_extensions(&[], &["media-capture", "recording", "speech-recognition"]);
    let granted = r#"{"op":"prefer","permission":{"microphone":"granted"}}"#;
    r.extension_agent(granted).unwrap().1.unwrap();
    (r, calls)
}

/// The host's part: each request run and each outcome fulfilled, until
/// nothing is left to do; one not answered yet stays pending.
fn pump(r: &mut Runner<Granted>) {
    for _ in 0..16 {
        // A landing's `then` and a change task run as the clock is advanced, which need not move.
        let ran = !r.advance(0.0).unwrap().is_empty();
        let outs = r.take_requests();
        if outs.is_empty() && !ran && r.apply_announced().0.is_empty() {
            return;
        }
        for out in outs {
            let outcome = match r.native_work(&out.request) {
                Dispatch::Run(Work::Now(work)) => work(),
                Dispatch::Run(Work::Later(work)) => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    work(Reply::new(move |o| {
                        let _ = tx.send(o);
                    }));
                    match rx.try_recv() {
                        Ok(outcome) => outcome,
                        Err(_) => continue,
                    }
                }
                _ => continue,
            };
            let _ = r.fulfill(out.ticket, outcome);
        }
    }
}

/// What a failed assertion shows: the journal and the mutations' slots.
fn why(r: &Runner<Granted>, calls: &Calls) -> String {
    let slots: Vec<_> = ["opened", "started", "trackId", "recorderId", "takeName"]
        .iter()
        .map(|n| format!("{n}={:?}", r.slot(n)))
        .collect();
    let sent: Vec<_> = (calls.lock().unwrap().iter())
        .filter_map(|c| c["send"].as_str().map(str::to_owned))
        .collect();
    format!(
        "{slots:?}\nsent {sent:?}\n{}",
        exact_runner::agent::handle(r, r#"{"op":"logs"}"#)
    )
}

fn stopped(calls: &Calls) -> Vec<String> {
    (calls.lock().unwrap().iter())
        .filter(|c| c["send"] == "stopTrack")
        .map(|c| c["args"]["track"].as_str().unwrap_or("").to_owned())
        .collect()
}

fn slot(r: &Runner<Granted>, name: &str) -> String {
    format!("{:?}", r.slot(name))
}

fn shown(r: &Runner<Granted>, test_id: &str) -> bool {
    let tree = exact_runner::agent::handle(r, r#"{"op":"tree"}"#);
    tree.contains(&format!("\"testId\":\"{test_id}\""))
}

/// The refused start's track is stopped, its identifiers cleared and the
/// refusal shown; the next press records on a track of its own.
#[test]
fn a_recorder_that_does_not_start_stops_its_track() {
    let (mut r, calls) = boot(1);
    pump(&mut r);
    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r);
    assert_eq!(stopped(&calls), ["track-1"], "{}", why(&r, &calls));
    assert_eq!(slot(&r, "trackId"), slot(&r, "recorderId"));
    assert_eq!(slot(&r, "takeName"), slot(&r, "recorderId"));
    assert!(shown(&r, "start-failed"), "{}", why(&r, &calls));

    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r);
    let recording = slot(&r, "recorderId").contains("recorder-1");
    assert!(
        recording && slot(&r, "trackId").contains("track-2"),
        "{}",
        why(&r, &calls)
    );
    assert_eq!(stopped(&calls), ["track-1"], "the retry's track records");
    assert!(!shown(&r, "start-failed"));
}

/// A take that starts and stops stops its track once, and only then.
#[test]
fn a_recorder_that_starts_keeps_its_track_until_stopped() {
    let (mut r, calls) = boot(0);
    pump(&mut r);
    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r);
    assert!(stopped(&calls).is_empty(), "{}", why(&r, &calls));
    assert!(
        slot(&r, "recorderId").contains("recorder-1"),
        "{}",
        why(&r, &calls)
    );
    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r);
    assert_eq!(stopped(&calls), ["track-1"], "{}", why(&r, &calls));
    assert!(!shown(&r, "start-failed"));
}
