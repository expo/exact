//! @exact/window-management on the painted hosts (LLP 1178 D1, D2, D6),
//! composed by the desktop package builder (LLP 1133.022 D10 item 5): the
//! host's display service (`Context::displays`) in the interface's shapes.
//!
//! - The Linux host's one display is its KMS connector, or the headless
//!   buffer, at the host's scale; the pointer is the one the host draws.
//! - The Windows host's are its monitors, laid out into CSS pixels, and
//!   the system's pointer (`host/windows/src/displays.rs`), or with no
//!   window the headless buffer; its `windows/exact.json` names this file.
//! - Natively no permission guards the displays: the status is `granted`
//!   wherever the host reports them, and `available: false` elsewhere.
//! - The source watches the host's displays while this session reads it,
//!   and announces its topic at each change the host reports.

use exact_painted::desktop::{Screen, Screens};
use exact_painted::package::{json, Context, Json, Package, Reply};

const TOPIC: &str = "window-management.getScreenDetails";

/// A `ScreenDetailed`, and with `client` its `clientRect`.
fn screen(s: &Screen, client: bool) -> Json {
    let [left, top, width, height] = s.rect;
    let [avail_left, avail_top, avail_width, avail_height] = s.avail;
    let mut out = json!({
        "left": left, "top": top, "width": width, "height": height,
        "availLeft": avail_left, "availTop": avail_top,
        "availWidth": avail_width, "availHeight": avail_height,
        "devicePixelRatio": s.scale, "isPrimary": s.primary,
        "isInternal": s.internal, "label": s.label,
    });
    if client {
        out["clientRect"] = s
            .client
            .map(
                |[x, y, width, height]| json!({ "x": x, "y": y, "width": width, "height": height }),
            )
            .unwrap_or(Json::Null);
    }
    out
}

/// `ScreenDetails`.
fn details(s: &Screens) -> Json {
    let current = s.screens.get(s.current).or(s.screens.first());
    json!({
        "screens": s.screens.iter().map(|x| screen(x, false)).collect::<Vec<_>>(),
        "currentScreen": current.map(|x| screen(x, false)),
    })
}

/// The session's instance.
pub struct WindowManagement {
    context: Context,
    watching: bool,
}

impl WindowManagement {
    fn unwatch(&mut self) {
        if std::mem::take(&mut self.watching) {
            if let Some(displays) = self.context.displays() {
                displays.unwatch_displays(self.context.session());
            }
        }
    }
}

impl Package for WindowManagement {
    fn new(context: Context) -> Self {
        WindowManagement {
            context,
            watching: false,
        }
    }

    fn later(&mut self, name: &str, _args: &Json, _preferred: Option<&Json>, reply: Reply) {
        let displays = self.context.displays();
        match (name, displays) {
            ("windowManagementStatus", d) => reply.send(json!({
                "available": d.is_some(),
                "state": if d.is_some() { "granted" } else { "denied" },
            })),
            ("getScreenDetails", Some(d)) => {
                if !std::mem::replace(&mut self.watching, true) {
                    let notifier = self.context.notifier();
                    d.watch_displays(
                        self.context.session(),
                        Box::new(move || notifier.changed(TOPIC)),
                    );
                }
                let screens = d.screens();
                if screens.screens.is_empty() {
                    return reply.error("NotSupportedError", Some("no display"));
                }
                reply.send(details(&screens));
            }
            ("getPointerPosition", Some(d)) => match d.pointer() {
                Some(p) => {
                    let on = p.screens.screens.get(p.on).or(p.screens.screens.first());
                    reply.send(json!({
                        "screenX": p.at[0], "screenY": p.at[1],
                        "clientX": p.client.map(|c| c[0]), "clientY": p.client.map(|c| c[1]),
                        "screen": on.map(|s| screen(s, true)),
                    }))
                }
                None => reply.error("NotSupportedError", Some("no pointer")),
            },
            _ => reply.error("NotSupportedError", None),
        }
    }

    fn release(&mut self, name: &str, _args: Option<&Json>) {
        if name == "getScreenDetails" {
            self.unwatch();
        }
    }

    fn destroy(&mut self) {
        self.unwatch();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_screen_reads_by_the_specification_s_names() {
        let s = Screen {
            rect: [0., 0., 1280., 800.],
            avail: [0., 24., 1280., 776.],
            scale: 2.,
            primary: true,
            internal: true,
            label: "eDP-1".into(),
            client: Some([-10., 0., 1280., 800.]),
        };
        let all = details(&Screens {
            screens: vec![s.clone()],
            current: 0,
        });
        assert_eq!(all["currentScreen"]["availTop"], 24.);
        assert_eq!(all["screens"][0]["label"], "eDP-1");
        assert!(all["screens"][0].get("clientRect").is_none());
        assert_eq!(screen(&s, true)["clientRect"]["x"], -10.);
    }
}
