//! A GPU surface's haptic messages (LLP 1009 D10), as `exact_gpu::haptics`
//! writes them: consumed, never dispatched to the app. A shell (Android's
//! reader) takes them as [`crate::presenter::Request::SurfaceHaptic`] and
//! plays them on the vibrator; with no shell (desktop Linux, Windows, a
//! headless run) nothing plays. `exact:audio` is consumed beside them: this
//! host configures no audio session.

use std::collections::BTreeSet;

/// What prefixes a haptic message (`exact_gpu::haptics::PREFIX`).
pub(crate) const PREFIX: &str = "exact:haptics ";
/// The stop of a surface's texture.
const STOP: &str = "{\"texture\":null}";

/// The messages waiting for the shell, by their canvas's view id: a
/// surface id is its module's, and two modules both count from 1.
#[derive(Default)]
pub(crate) struct Haptics {
    /// A shell takes them; set by `Presenter::serve_requests`.
    pub(crate) shell: bool,
    queued: Vec<(u32, String)>,
    /// Surfaces that sent one: a surface that goes stops its texture.
    seen: BTreeSet<u32>,
}

impl Haptics {
    /// Whether `message` is a haptic message, which the host consumes;
    /// queued for `surface` when a shell takes it. A texture's values
    /// replace those of one still queued for the same surface: the shell
    /// takes the queue after each turn, and only the last values count.
    pub(crate) fn take(&mut self, surface: u32, message: &str) -> bool {
        let Some(json) = message.strip_prefix(PREFIX) else {
            return false;
        };
        if !self.shell {
            return true;
        }
        self.seen.insert(surface);
        let texture = |j: &str| j.starts_with("{\"texture\":{");
        if texture(json) {
            if let Some(last) = self.queued.iter_mut().rev().find(|(s, _)| *s == surface) {
                if texture(&last.1) {
                    last.1 = json.to_string();
                    return true;
                }
            }
        }
        self.queued.push((surface, json.to_string()));
        true
    }

    /// `surface` is destroyed: its texture stops.
    pub(crate) fn gone(&mut self, surface: u32) {
        if self.seen.remove(&surface) {
            self.queued.push((surface, STOP.to_string()));
        }
    }

    /// What waits for the shell, in order.
    pub(crate) fn take_queued(&mut self) -> Vec<(u32, String)> {
        std::mem::take(&mut self.queued)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn messages_are_consumed_and_queued_only_for_a_shell() {
        let mut h = Haptics::default();
        assert!(!h.take(1, "hello"));
        assert!(h.take(1, "exact:haptics {\"texture\":null}"));
        assert!(h.take_queued().is_empty(), "no shell, nothing plays");
        h.shell = true;
        let texture =
            |i: f32| format!("exact:haptics {{\"texture\":{{\"intensity\":{i},\"sharpness\":0}}}}");
        assert!(h.take(1, &texture(0.25)));
        assert!(h.take(2, &texture(0.5)));
        assert!(h.take(1, &texture(0.75)));
        assert!(h.take(
            1,
            "exact:haptics {\"pattern\":{\"events\":[],\"curves\":[]}}"
        ));
        assert!(h.take(1, &texture(1.0)));
        h.gone(2);
        h.gone(3);
        let q = h.take_queued();
        assert_eq!(
            q,
            [
                (
                    1,
                    "{\"texture\":{\"intensity\":0.75,\"sharpness\":0}}".to_string()
                ),
                (
                    2,
                    "{\"texture\":{\"intensity\":0.5,\"sharpness\":0}}".to_string()
                ),
                (1, "{\"pattern\":{\"events\":[],\"curves\":[]}}".to_string()),
                (
                    1,
                    "{\"texture\":{\"intensity\":1,\"sharpness\":0}}".to_string()
                ),
                (2, STOP.to_string()),
            ]
        );
        h.gone(2);
        assert!(h.take_queued().is_empty(), "a surface stops once");
    }
}
