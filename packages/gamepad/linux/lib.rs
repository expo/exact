//! @exact/gamepad on Linux and Windows (LLP 1133.022 D4, D10 items 1 and 5,
//! D12), composed into the app's host crate by the Rust builder: the host's
//! one pad registry (`exact_painted::pads`), the evdev pads on Linux and
//! XInput's slots on Windows, through the package context.
//!
//! - The list is the registry's, by slot index, with each pad's family by
//!   its vendor (XInput's are Xbox-class) and D2's labels; `current` is the
//!   pad whose button last went down.
//! - `dual-rumble` is the registry's claim on the device: an `FF_RUMBLE`
//!   effect on Linux, `XInputSetState` on Windows; it answers `complete`
//!   after its span, or `preempted`. `trigger-rumble` and lights are `later`
//!   on both (D12): no pad lists them.
//! - A reader with `interval` reads each pad's buttons and axes as they are
//!   at its ask; the registry announces a pad's coming, going and use, not
//!   each reading.

use exact_painted::package::{json, Context, Json, Package, Reply};
use exact_painted::pads::{Pad, Pads};
use std::sync::{Arc, Mutex};

/// D2's labels by family; the interface's `tables.labels`.
const LABELS: [(&str, [&str; 17]); 4] = [
    (
        "xbox",
        [
            "A", "B", "X", "Y", "LB", "RB", "LT", "RT", "View", "Menu", "LS", "RS", "Up", "Down",
            "Left", "Right", "Xbox",
        ],
    ),
    (
        "playstation",
        [
            "Cross", "Circle", "Square", "Triangle", "L1", "R1", "L2", "R2", "Create", "Options",
            "L3", "R3", "Up", "Down", "Left", "Right", "PS",
        ],
    ),
    (
        "nintendo",
        [
            "B", "A", "Y", "X", "L", "R", "ZL", "ZR", "−", "+", "LS", "RS", "Up", "Down", "Left",
            "Right", "Home",
        ],
    ),
    (
        "generic",
        [
            "A", "B", "X", "Y", "L1", "R1", "L2", "R2", "Select", "Start", "L3", "R3", "Up",
            "Down", "Left", "Right", "Home",
        ],
    ),
];

/// The specification's recommended cap on a duration, ms.
const LONGEST: f64 = 5000.;

fn labels(family: &str) -> Vec<&'static str> {
    let (_, l) = LABELS
        .iter()
        .find(|(f, _)| *f == family)
        .unwrap_or(&LABELS[3]);
    l.to_vec()
}

/// A registry pad as the interface's `Gamepad`; with `read` (the host's clock at the ask), its reading, stamped then,
/// as on macOS and Android.
fn shape(pad: &Pad, read: Option<f64>) -> Json {
    let family = pad.identity.family();
    let effects: Vec<&str> = if pad.rumble {
        vec!["dual-rumble"]
    } else {
        vec![]
    };
    let mut out = json!({
        "index": pad.index, "id": pad.identity.id, "mapping": "standard", "family": family,
        "labels": labels(family), "effects": effects, "light": false,
        "buttons": null, "axes": null, "timestamp": null,
        "windows": null, "linux": null,
    });
    if let Some(now) = read {
        let r = &pad.reading;
        out["buttons"] = (r.buttons.iter())
            .map(|v| json!({ "pressed": *v >= 0.5, "touched": *v > 0., "value": v }))
            .collect();
        out["axes"] = json!(r.axes);
        out["timestamp"] = json!(now);
    }
    match pad.identity.user_index {
        Some(slot) => out["windows"] = json!({ "userIndex": slot }),
        None => {
            out["linux"] = json!({
                "driver": pad.identity.driver.clone().unwrap_or_default(),
                "vendor": pad.identity.vendor, "product": pad.identity.product,
            })
        }
    }
    out
}

pub struct Gamepad {
    context: Context,
    pads: Pads,
}

impl Package for Gamepad {
    fn new(context: Context) -> Self {
        let pads = context.pads();
        // The pads as last told: a change raises the events for those that came and went.
        let told: Arc<Mutex<Vec<Pad>>> = Arc::new(Mutex::new(pads.list().0));
        let notifier = context.notifier();
        pads.watch(move || {
            let (now, _) = exact_painted::pads::list();
            let mut was = told.lock().unwrap_or_else(|e| e.into_inner());
            let alike = |a: &Pad, b: &Pad| a.index == b.index && a.identity == b.identity;
            for gone in was.iter().filter(|p| !now.iter().any(|n| alike(n, p))) {
                notifier.emit("gamepaddisconnected", json!([shape(gone, None)]));
            }
            for came in now.iter().filter(|p| !was.iter().any(|w| alike(w, p))) {
                notifier.emit("gamepadconnected", json!([shape(came, None)]));
            }
            *was = now;
            notifier.changed("gamepad.getGamepads");
        });
        Gamepad { context, pads }
    }

    fn later(&mut self, name: &str, args: &Json, _preferred: Option<&Json>, reply: Reply) {
        let (pads, current) = self.pads.list();
        // A send without `index` is for the pad in use when it arrives (D2).
        let index = args["index"].as_f64().map(|i| i as u8).or(current);
        let named = index.filter(|i| pads.iter().any(|p| p.index == *i));
        match name {
            "getGamepads" => {
                let read = (!args["interval"].is_null()).then(|| self.context.now());
                let list: Vec<Json> = pads.iter().map(|p| shape(p, read)).collect();
                reply.send(json!({ "available": true, "gamepads": list, "current": current }))
            }
            "playEffect" => {
                let Some(index) = named else {
                    return reply.error("NotFoundError", None);
                };
                if args["type"] != "dual-rumble" {
                    return reply.error("NotSupportedError", None);
                }
                let mut duration = args["duration"].as_f64().unwrap_or(0.);
                if duration > LONGEST {
                    self.context.log(format!(
                        "gamepad.playEffect: duration {duration} capped at {LONGEST}"
                    ));
                    duration = LONGEST;
                }
                let delay = args["startDelay"].as_f64().unwrap_or(0.);
                let strong = args["strongMagnitude"].as_f64().unwrap_or(0.) as f32;
                let weak = args["weakMagnitude"].as_f64().unwrap_or(0.) as f32;
                let reply = Arc::new(Mutex::new(Some(reply)));
                let answer = reply.clone();
                let done = Box::new(move |how: &'static str| {
                    if let Some(r) = answer.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        r.send(json!({ "result": how }));
                    }
                });
                if let Err(code) =
                    self.pads
                        .rumble(index, strong, weak, delay, duration as u32, done)
                {
                    if let Some(r) = reply.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        r.error(code, None);
                    }
                }
            }
            "resetEffects" => match named.map(|i| self.pads.reset(i)) {
                Some(Ok(())) => reply.send(json!({ "result": "complete" })),
                _ => reply.error("NotFoundError", None),
            },
            "setLight" if args["color"].is_null() && args["index"].is_null() => {
                reply.send(json!({}))
            }
            "setLight" if named.is_none() => reply.error("NotFoundError", None),
            // No pad here has a light the host can set (D12).
            "setLight" => reply.error("NotSupportedError", None),
            _ => reply.error("NotSupportedError", None),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_painted::pads::Identity;

    /// A reading carries the time it was read, as on macOS and Android; a pad's coming and going carry none.
    #[test]
    fn a_reading_is_stamped_with_its_ask() {
        let pad = Pad {
            index: 0,
            identity: Identity::default(),
            reading: Default::default(),
            rumble: false,
        };
        let read = shape(&pad, Some(1234.5));
        assert_eq!(read["timestamp"], 1234.5);
        assert_eq!(read["buttons"].as_array().map(Vec::len), Some(17));
        let told = shape(&pad, None);
        assert!(told["timestamp"].is_null() && told["buttons"].is_null());
    }
}
