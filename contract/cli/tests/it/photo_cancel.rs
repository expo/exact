//! Photo Editor's own `app.contract` when the camera is left while a photo
//! is being taken: the photo that answers afterwards is dropped, so it
//! neither replaces the picture being edited nor pops a route opened since.
//! `@exact/media-capture` and `@exact/camera` are played by the native slot
//! under the agent; `takePhoto` answers only when the test says.

use exact_runner::{Dispatch, Outcome, Reply, Response, Runner, Work};
use serde_json::{json, Value as Json};
use std::path::Path;
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
        // app.ts's sources stay pending; this drives the module seam.
        Ok(exact_runner::Answer::Later(exact_runner::Request::native(
            Vec::new(),
        )))
    }
    fn grants(&self) -> &str {
        "device.camera purpose.camera\n"
    }
}

type Calls = Arc<Mutex<Vec<Json>>>;
type Held = Arc<Mutex<Vec<Reply>>>;
/// The requests whose work answers later, by ticket, across pumps.
type Waiting = Vec<(u64, std::sync::mpsc::Receiver<Outcome>)>;

fn reply(reply: Reply, answer: Json) {
    reply.send(Outcome::Response(Response {
        status: 200,
        headers: Vec::new(),
        body: answer.to_string().into_bytes(),
    }));
}

/// Photo Editor over the native slot; each `takePhoto` is held in `Held`.
fn boot() -> (Runner<Granted>, Calls, Held) {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../apps/photo-editor/app.contract");
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
    let held: Held = Arc::default();
    let (seen, hold) = (calls.clone(), held.clone());
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, answer: Reply| {
            // app.ts's requests (`Granted`'s empty bodies) stay pending.
            let Ok(v) = serde_json::from_slice::<Json>(&body) else {
                return;
            };
            seen.lock().unwrap().push(v.clone());
            let track = json!({ "id": "track-1", "kind": "video", "label": "Camera",
                "enabled": true, "muted": false, "readyState": "live", "settings": {} });
            match v["send"].as_str() {
                Some("takePhoto") => hold.lock().unwrap().push(answer),
                Some("getUserMedia") => reply(
                    answer,
                    json!({ "id": "stream-1", "active": true, "tracks": [track] }),
                ),
                Some("stopTrack") => reply(answer, json!(track)),
                _ => {}
            }
        })));
    r.bind_extensions(&[], &["camera", "media-capture"]);
    let granted = r#"{"op":"prefer","permission":{"camera":"granted"}}"#;
    r.extension_agent(granted).unwrap().1.unwrap();
    (r, calls, held)
}

/// The host's part: each request run and each outcome fulfilled, until
/// nothing is left to do; one not answered yet stays pending.
fn pump(r: &mut Runner<Granted>, waiting: &mut Waiting) {
    for _ in 0..16 {
        let ran = !r.advance(0.0).unwrap().is_empty();
        let outs = r.take_requests();
        let mut landed = false;
        waiting.retain(|(ticket, rx)| match rx.try_recv() {
            Ok(outcome) => {
                landed = true;
                let _ = r.fulfill(*ticket, outcome);
                false
            }
            Err(_) => true,
        });
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

fn top(r: &Runner<Granted>) -> String {
    let tree = exact_runner::agent::handle(r, r#"{"op":"tree"}"#);
    ["print", "camera"]
        .into_iter()
        .find(|id| tree.contains(&format!("\"testId\":\"{id}\"")))
        .unwrap_or("editor")
        .to_owned()
}

fn why(r: &Runner<Granted>, calls: &Calls) -> String {
    let slots: Vec<_> = ["shot", "photo", "track", "nav"]
        .iter()
        .map(|n| format!("{n}={:?}", r.slot(n)))
        .collect();
    let sent: Vec<_> = (calls.lock().unwrap().iter())
        .filter_map(|c| c["send"].as_str().map(str::to_owned))
        .collect();
    format!("{slots:?}\nsent {sent:?}")
}

fn photo() -> Json {
    json!({ "path": "app:/tmp/camera/1.jpg", "type": "image/jpeg", "size": 1, "width": 1, "height": 1 })
}

fn stops(calls: &Calls) -> usize {
    (calls.lock().unwrap().iter())
        .filter(|c| c["send"] == "stopTrack")
        .count()
}

/// The shutter pressed, the camera left by `leave`, the print sheet opened,
/// then the photo answered: the photo is dropped, so the sample stays the
/// picture, the print sheet stays open and the track is not stopped again.
fn left_while_taking(leave: &str, args: Vec<exact_runner::Value>) {
    let (mut r, calls, held) = boot();
    let mut w = Waiting::new();
    pump(&mut r, &mut w);
    r.act("takePicture", Vec::new()).unwrap();
    pump(&mut r, &mut w);
    assert_eq!(top(&r), "camera", "{}", why(&r, &calls));
    r.act("shutter", Vec::new()).unwrap();
    pump(&mut r, &mut w);
    assert_eq!(held.lock().unwrap().len(), 1, "{}", why(&r, &calls));
    r.act(leave, args).unwrap();
    pump(&mut r, &mut w);
    if top(&r) != "print" {
        r.act("orderPrint", Vec::new()).unwrap();
        pump(&mut r, &mut w);
    }
    assert_eq!(top(&r), "print", "{}", why(&r, &calls));
    let stopped = stops(&calls);

    reply(held.lock().unwrap().pop().unwrap(), photo());
    pump(&mut r, &mut w);
    assert!(
        format!("{:?}", r.slot("photo")).contains("assets/sample.jpg"),
        "{}",
        why(&r, &calls)
    );
    assert_eq!(top(&r), "print", "{}", why(&r, &calls));
    assert_eq!(stops(&calls), stopped, "{}", why(&r, &calls));
}

#[test]
fn a_photo_answered_after_cancel_is_dropped() {
    left_while_taking("cancel", Vec::new());
}

/// A sheet dismissed by a swipe runs `back`.
#[test]
fn a_photo_answered_after_the_camera_is_dismissed_is_dropped() {
    left_while_taking("back", Vec::new());
}

/// A link that leaves the camera neither cancels nor goes back.
#[test]
fn a_photo_answered_after_a_link_left_the_camera_is_dropped() {
    left_while_taking("follow", vec![exact_runner::Value::str("/print")]);
}

/// A photo answered while the camera is up becomes the picture and closes it.
#[test]
fn a_photo_answered_on_the_camera_is_edited() {
    let (mut r, calls, held) = boot();
    let mut w = Waiting::new();
    pump(&mut r, &mut w);
    r.act("takePicture", Vec::new()).unwrap();
    pump(&mut r, &mut w);
    r.act("shutter", Vec::new()).unwrap();
    pump(&mut r, &mut w);
    let taken = held.lock().unwrap().pop().unwrap();
    reply(taken, photo());
    pump(&mut r, &mut w);
    assert!(
        format!("{:?}", r.slot("photo")).contains("app:/tmp/camera/1.jpg"),
        "{}",
        why(&r, &calls)
    );
    assert_eq!(top(&r), "editor", "{}", why(&r, &calls));
}
