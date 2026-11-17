//! Recorder's own `app.contract` when a take's transcript stop answers late:
//! the next take cannot start until it has, and its answer, however late,
//! leaves the next take's recognition alone. The media and speech packages are
//! played by the native slot under the agent; `stopSpeechRecognition`'s
//! answers are held until the test releases them.

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
        "device.microphone purpose.microphone\ndevice.speech-recognition purpose.transcribe\nfs.read app:/data/takes\nfs.write app:/data/takes\n"
    }
}

/// The stops not answered yet: the recognition each names, and its reply.
type Held = Arc<Mutex<Vec<(String, Reply)>>>;

fn respond(reply: Reply, answer: Json) {
    reply.send(Outcome::Response(Response {
        status: 200,
        headers: Vec::new(),
        body: answer.to_string().into_bytes(),
    }));
}

fn boot() -> (Runner<Granted>, Held) {
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
    let held: Held = Arc::default();
    let stops = held.clone();
    let streams = AtomicUsize::new(0);
    let recognitions = AtomicUsize::new(0);
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            let Ok(v) = serde_json::from_slice::<Json>(&body) else {
                return;
            };
            let track = |id: &str, state: &str| {
                json!({ "id": id, "kind": "audio", "label": "Microphone", "enabled": true,
                    "muted": false, "readyState": state, "settings": {} })
            };
            // A resource's read names its `source`, a mutation's its `send`.
            let answer = match v["send"].as_str().or(v["source"].as_str()) {
                Some("getUserMedia") => {
                    let n = streams.fetch_add(1, Ordering::SeqCst) + 1;
                    json!({ "id": format!("stream-{n}"), "active": true,
                        "tracks": [track(&format!("track-{n}"), "live")] })
                }
                Some("startMediaRecorder") => json!({ "id": "recorder-1", "state": "recording",
                    "mimeType": "audio/wav", "path": "app:/data/takes/take-0.wav", "duration": 0 }),
                Some("stopMediaRecorder") => json!({ "path": "app:/data/takes/take-0.wav",
                    "mimeType": "audio/wav", "duration": 1000, "size": 1 }),
                Some("stopTrack") => track(v["args"]["track"].as_str().unwrap_or(""), "ended"),
                Some("speechRecognitionStatus") => json!({ "available": true, "state": "granted",
                    "inputs": ["track", "file"], "local": true }),
                Some("speechRecognitionAvailability") => json!({ "availability": "available" }),
                Some("startSpeechRecognition") => {
                    let n = recognitions.fetch_add(1, Ordering::SeqCst) + 1;
                    json!({ "id": format!("rec-{n}"), "leavesDevice": false, "finalCount": 0, "files": 0 })
                }
                Some("stopSpeechRecognition") => {
                    let id = v["args"]["recognition"].as_str().unwrap_or("").to_owned();
                    stops.lock().unwrap().push((id, reply));
                    return;
                }
                _ => json!({}),
            };
            respond(reply, answer);
        })));
    r.bind_extensions(&[], &["media-capture", "recording", "speech-recognition"]);
    let granted =
        r#"{"op":"prefer","permission":{"microphone":"granted","speech-recognition":"granted"}}"#;
    r.extension_agent(granted).unwrap().1.unwrap();
    (r, held)
}

/// Each request run and each outcome fulfilled, until nothing is left to do;
/// one not answered yet stays pending until a later pump finds its answer.
fn pump(r: &mut Runner<Granted>, waiting: &mut Vec<(u64, std::sync::mpsc::Receiver<Outcome>)>) {
    for _ in 0..16 {
        let ran = !r.advance(0.0).unwrap().is_empty();
        let mut landed = false;
        waiting.retain(|(ticket, rx)| match rx.try_recv() {
            Ok(outcome) => {
                let _ = r.fulfill(*ticket, outcome);
                landed = true;
                false
            }
            Err(_) => true,
        });
        let outs = r.take_requests();
        if outs.is_empty() && !ran && !landed && r.apply_announced().0.is_empty() {
            return;
        }
        for out in outs {
            match r.native_work(&out.request) {
                Dispatch::Run(Work::Now(work)) => {
                    let _ = r.fulfill(out.ticket, work());
                }
                Dispatch::Run(Work::Later(work)) => {
                    let (tx, rx) = std::sync::mpsc::channel();
                    work(Reply::new(move |o| {
                        let _ = tx.send(o);
                    }));
                    waiting.push((out.ticket, rx));
                }
                _ => {}
            }
        }
    }
}

fn slot(r: &Runner<Granted>, name: &str) -> String {
    format!("{:?}", r.slot(name))
}

fn toggle_disabled(r: &Runner<Granted>) -> bool {
    let tree: Json =
        serde_json::from_str(&exact_runner::agent::handle(r, r#"{"op":"tree"}"#)).unwrap();
    let nodes = tree["nodes"].as_array().cloned().unwrap_or_default();
    let toggle = nodes.iter().find(|n| n["props"]["testId"] == "toggle");
    toggle.is_some_and(|n| n["props"]["disabled"] == json!(true))
}

/// A take's transcript stop that answers after the next take began clears
/// nothing of that take's, and Record waits for the stop's answer.
#[test]
fn a_late_transcript_stop_leaves_the_next_take_its_recognition() {
    let (mut r, held) = boot();
    let mut waiting = Vec::new();
    pump(&mut r, &mut waiting);
    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r, &mut waiting);
    assert!(
        slot(&r, "recognitionId").contains("rec-1"),
        "{}",
        slot(&r, "recognitionId")
    );

    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r, &mut waiting);
    assert_eq!(held.lock().unwrap().len(), 1, "the stop is held");
    assert!(
        toggle_disabled(&r),
        "Record waits for the transcript's stop"
    );

    // A second take started anyway, as an action sent before the button disabled would.
    r.act("toggle", Vec::new()).unwrap();
    pump(&mut r, &mut waiting);
    assert!(
        slot(&r, "recognitionId").contains("rec-2"),
        "{}",
        slot(&r, "recognitionId")
    );

    let (id, reply) = held.lock().unwrap().remove(0);
    assert_eq!(id, "rec-1");
    respond(
        reply,
        json!({ "id": "rec-1", "leavesDevice": false, "finalCount": 1, "files": 1,
        "transcript": "app:/data/takes/take-1.transcript" }),
    );
    pump(&mut r, &mut waiting);
    assert!(
        slot(&r, "recognitionId").contains("rec-2"),
        "the late stop cleared the next take's recognition: {}",
        slot(&r, "recognitionId")
    );
}
