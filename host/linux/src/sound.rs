//! Contract sounds cross Android's existing reader request channel. The runner
//! keeps its journal; the reader opens the optional AAudio arm at the first play.

use exact_runner::sound::SoundOp;
use serde_json::{json, Value};
use std::sync::Mutex;

fn agent() -> bool {
    std::env::var("EXACT_AGENT").is_ok_and(|v| v == "1")
}

pub(crate) struct Pending {
    enabled: bool,
    #[cfg(any(target_os = "android", test))]
    reset: bool,
    ops: Vec<SoundOp>,
}

impl Default for Pending {
    fn default() -> Self {
        Self {
            enabled: cfg!(target_os = "android") && !agent(),
            #[cfg(any(target_os = "android", test))]
            reset: true,
            ops: Vec::new(),
        }
    }
}

impl Pending {
    pub(crate) fn take(&mut self, ops: Vec<SoundOp>) {
        if self.enabled {
            self.ops.extend(ops);
        }
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn ready(&self) -> bool {
        self.enabled && (self.reset || !self.ops.is_empty())
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn batch(
        &mut self,
        files: Vec<String>,
        origin_ns: i64,
        submitted_ns: i64,
    ) -> String {
        // CLOCK_MONOTONIC, also used by AAudioStream_getTimestamp. Preserve
        // the origin across reloads; a late voice retains its attack.
        let time = |ms: f64| origin_ns as f64 / 1e9 + ms / 1000.0;
        let ops: Vec<_> = self
            .ops
            .drain(..)
            .filter_map(|op| match op {
                SoundOp::Play {
                    id,
                    sound,
                    at,
                    gain,
                    playback_rate,
                    pan,
                } => Some(json!({
                    "kind": 1, "id": id, "sound": sound, "at": time(at),
                    "gain": gain, "rate": playback_rate, "pan": pan,
                })),
                SoundOp::End { id, at } => Some(json!({"kind": 2, "id": id, "at": time(at)})),
                SoundOp::Set {
                    id,
                    at,
                    gain,
                    playback_rate,
                    pan,
                    ramp,
                } => Some(json!({
                    "kind": 4, "id": id, "at": time(at), "gain": gain,
                    "rate": playback_rate, "pan": pan, "ramp": ramp / 1000.0,
                })),
                // Timed haptics and MIDI remain outside the sound executor.
                SoundOp::Haptic { .. } | SoundOp::Unhaptic { .. } => None,
            })
            .collect();
        let reset = std::mem::take(&mut self.reset);
        json!({"files": files, "reset": reset, "submitted": submitted_ns as f64 / 1e9, "ops": ops})
            .to_string()
    }
}

static OUTPUT: Mutex<Option<Value>> = Mutex::new(None);

/// The main-thread sound owner reports without waiting for the runner's thread.
pub fn report(state: &str) {
    if let Ok(value @ Value::Object(_)) = serde_json::from_str(state) {
        *OUTPUT.lock().unwrap_or_else(|e| e.into_inner()) = Some(value);
    }
}

pub(crate) fn output_state(reply: String) -> String {
    if !cfg!(target_os = "android") || agent() {
        return reply;
    }
    let Ok(mut state) = serde_json::from_str::<Value>(&reply) else {
        return reply;
    };
    if let Some(sounds) = state.get_mut("sounds") {
        sounds["output"] = OUTPUT
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
            .unwrap_or_else(|| json!({"kind": "aaudio", "state": "closed", "skipped": 0}));
        state.to_string()
    } else {
        reply
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    #[test]
    fn android_hands_runner_voices_to_the_arm_on_the_monotonic_frame_clock() {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../web-js/conformance/sounds/app.contract");
        let plan = contract::compile_path_source(
            &path,
            r#"sound "assets/kick.wav"
component App
  action play
    playSound("assets/kick.wav", at=125, gain=0.5, playbackRate=1.5, pan=-0.25)
  action change
    setSounds(gain=0.75, pan=0.5, ramp=40)
  action stop
    stopSounds()
  view
    column
      button "Play" press=play testId="play"
      button "Change" press=change testId="change"
      button "Stop" press=stop testId="stop"
"#,
        )
        .unwrap();
        let (mut p, error) = Presenter::boot_with(
            &plan.encode(),
            (),
            (80., 40.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none());
        p.host.sound.enabled = true;
        let kernel = p.host.kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("play")[0])
            .unwrap()
            .id;
        assert!(p
            .host
            .dispatch_at(node, exact_runner::Event::Press, 0.0)
            .is_none());
        p.host.take_commands();
        let body: Value = serde_json::from_str(&p.host.sound.batch(
            vec!["assets/kick.wav".into()],
            10_000_000_000,
            10_000_000_000,
        ))
        .unwrap();
        let play = &body["ops"][0];
        assert_eq!(play["kind"], 1);
        assert_eq!(play["sound"], 0);
        assert_eq!(play["gain"], 0.5);
        assert_eq!(play["rate"], 1.5);
        assert_eq!(play["pan"], -0.25);
        // AAudio timestamp: frame 4800 reached the speaker at 10.1 s.
        // This voice belongs to frame 6000, 240 frames into buffer 5760.
        let frame = 4800.0 + (play["at"].as_f64().unwrap() - 10.1) * 48000.0;
        assert_eq!(frame.round() as i64, 6000);
        assert_eq!(frame.round() as i64 - 5760, 240);
        assert!(p
            .host
            .agent(r#"{"op":"state"}"#)
            .contains("assets/kick.wav"));
        assert!(!p.host.sound.ready());
        let kernel = p.host.kernel();
        let change = kernel
            .node_by_key(kernel.find_by_test_id("change")[0])
            .unwrap()
            .id;
        assert!(p
            .host
            .dispatch_at(change, exact_runner::Event::Press, 25.0)
            .is_none());
        p.host.take_commands();
        let changed: Value = serde_json::from_str(&p.host.sound.batch(
            vec!["assets/kick.wav".into()],
            10_000_000_000,
            10_025_000_000,
        ))
        .unwrap();
        let change = &changed["ops"][0];
        assert_eq!(change["kind"], 4);
        assert_eq!(change["id"], play["id"]);
        assert_eq!(change["at"], 10.025);
        assert_eq!(change["gain"], 0.75);
        assert_eq!(change["pan"], 0.5);
        assert!(
            change["rate"].is_null(),
            "an omitted rate keeps the voice's rate"
        );
        assert_eq!(change["ramp"], 0.04);
        let kernel = p.host.kernel();
        let stop = kernel
            .node_by_key(kernel.find_by_test_id("stop")[0])
            .unwrap()
            .id;
        assert!(p
            .host
            .dispatch_at(stop, exact_runner::Event::Press, 50.0)
            .is_none());
        p.host.take_commands();
        let stopped: Value = serde_json::from_str(&p.host.sound.batch(
            vec!["assets/kick.wav".into()],
            10_000_000_000,
            10_050_000_000,
        ))
        .unwrap();
        assert_eq!(stopped["ops"][0]["kind"], 2);
        assert_eq!(stopped["ops"][0]["id"], play["id"]);
        assert_eq!(
            stopped["ops"][0]["at"], play["at"],
            "a waiting voice is cancelled at its start"
        );
        assert_eq!(stopped["reset"], false);
    }
}
