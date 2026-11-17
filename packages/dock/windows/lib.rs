//! @exact/dock on Windows (LLP 1133.025 D5): `requestAttention` flashes the
//! window's taskbar button through the host (`exact_painted::desktop`,
//! `FlashWindowEx`): until the window comes forward for a critical request,
//! three times otherwise, or `windows.count` times when the author says
//! (LLP 1133.025 D8). A hidden window has no taskbar button, so nothing is
//! asked, and the journal says why.

use exact_painted::package::{json, Context, Json, Package, Reply};

pub struct Dock {
    context: Context,
}

impl Package for Dock {
    fn new(context: Context) -> Self {
        Dock { context }
    }

    fn later(&mut self, name: &str, args: &Json, _preferred: Option<&Json>, reply: Reply) {
        let (Some(host), "requestAttention") = (self.context.desktop(), name) else {
            return reply.error("NotSupportedError", None);
        };
        let critical = args["critical"] == true;
        let count = match &args["windows"]["count"] {
            Json::Null => None,
            n => match n
                .as_f64()
                .filter(|n| n.fract() == 0.0 && (1.0..=u32::MAX as f64).contains(n))
            {
                Some(n) => Some(n as u32),
                None => {
                    return reply.error(
                        "TypeError",
                        Some("windows.count is a whole number of flashes, at least 1"),
                    )
                }
            },
        };
        match host.attention(critical, count) {
            Ok(()) => self.context.log(match count {
                Some(n) => format!("requestAttention: {n} flashes"),
                None if critical => "requestAttention: critical".into(),
                None => "requestAttention: informational".into(),
            }),
            Err(why) => self.context.log(format!("requestAttention: {why}")),
        }
        reply.send(json!({}));
    }
}
