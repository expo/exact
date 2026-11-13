//! Keys and system Back for the Canvas host's reader (Android). The
//! reader's view takes the window's key events (a hardware keyboard's, the
//! IME's committed text sent as keys, a driver's injected ones) and hands
//! them here; they go where the display's keyboard goes on Linux
//! ([`Presenter::key`]): the focused node, as a keydown and then its default
//! action (a character typed into a field, Backspace, Enter).
//!
//! While an editable field holds the focus the reader shows the platform's
//! software keyboard: the presenter tells it so, as a request
//! (`requests.rs`, LLP 1115.001 D5). System Back goes where the presenter's order sends it
//! (LLP 1115.001 D6, [`crate::presenter::Presenter::system_back`]): the
//! navigation root's authored Back control, else its `navigate`, else the
//! runner's own `back`. The reader takes Back only while the presenter says
//! it has somewhere to go, so at the first screen the system's own Back
//! leaves the app; a Back that arrives and is not taken finishes the
//! activity, as it did.
//!
//! @ref LLP 1076 §3.3 (Exact plus Android Canvas)

#[cfg(target_os = "android")]
use super::CanvasHost;
#[cfg(target_os = "android")]
use exact_runner::DataSource;

#[cfg(target_os = "android")]
impl<D: DataSource + Default> CanvasHost<D> {
    /// A key typed at the focused node: a character, Enter (`'\n'`), or
    /// Backspace (`backspace`). Nothing when nothing holds the focus.
    pub fn key(&mut self, ch: Option<char>, backspace: bool) {
        let now = self.now();
        self.p.key(ch, backspace, now);
        self.force = true;
    }

    /// A gamepad's state (`Pads.kt`): to the canvas a key would reach,
    /// stamped when its event happened (`event_ms`, [`reported`]), as the
    /// Linux display stamps a pad's report, so a press and a release that
    /// reach the host together keep the time between them (LLP 1127.003 D2).
    pub fn pad(
        &mut self,
        index: u8,
        connected: bool,
        buttons: [f32; 17],
        axes: [f32; 4],
        event_ms: i64,
    ) {
        let now = self.now();
        // A drive's clock is its own, not the device's: there it is arrival.
        let at = match super::agent::clock(&self.p) {
            Some(_) => now,
            None => reported(event_ms, self.origin_ns, now),
        };
        self.p.hardware_pad(index, connected, buttons, axes, at);
        self.force = true;
    }

    /// A key by its web names, down and up: Tab, and a remote's D-pad arrows
    /// and Select (LLP 1115.001.003 D3). They take the Linux display's
    /// named-key path ([`crate::presenter::Presenter::hardware_key`]).
    /// Android tells Shift with the key, not as a press of its own, so it is
    /// held for this key alone, as a field's Shift+Tab holds it
    /// (`Presenter::reader_field_action`).
    pub fn named_key(&mut self, code: &str, key: &str, down: bool, repeat: bool, shift: bool) {
        let held = self.p.held & 1;
        self.p.hold_modifier("ShiftLeft", shift);
        self.p.hardware_key(code, key, down, repeat);
        self.p.hold_modifier("ShiftLeft", held != 0);
        self.force = true;
    }

    /// System Back, in the presenter's order (LLP 1115.001 D6). Whether the
    /// app took it; when it did not, it has nowhere to go back to and the
    /// activity goes.
    pub fn back(&mut self) -> bool {
        let now = self.now();
        let taken = self.p.system_back(now);
        self.force |= taken;
        taken
    }

    /// Predictive Back as it goes (Android 14's `OnBackAnimationCallback`):
    /// the presenter previews the pop or the sheet's close; Back itself
    /// commits it ([`CanvasHost::back`]).
    pub fn back_progress(&mut self, phase: i32, progress: f32, edge: i32) -> bool {
        let now = self.now();
        let previews = self.p.back_progress(phase, progress, edge, now);
        self.force = true;
        previews
    }

    /// A URL the activity was given while the app runs (`onNewIntent`, or
    /// an activity made again in a running process), as a link's location
    /// (LLP 1133.023 D15 item 3). The one at launch is `EXACT_LAUNCH_URL`.
    pub fn open(&mut self, href: &str) {
        let now = self.now();
        if let Some(error) = self.p.open_location(href, now) {
            eprintln!("exact: {error}");
        }
        self.force = true;
    }

    /// A share the activity was given while the app runs (`onNewIntent`,
    /// LLP 1133.024 D5): `{"target", "share"}`, its files descriptors the
    /// reader opened. The one at launch is `EXACT_SHARE`.
    pub fn share(&mut self, request: &str) {
        let now = self.now();
        let received = serde_json::from_str(request)
            .map_err(|e| e.to_string())
            .and_then(|q| self.p.receive_platform_share(&q, now));
        if let Err(error) = received {
            eprintln!("exact: share: {error}");
        }
        self.force = true;
    }
}

/// A reader event's `eventTime` on the host's clock, which reads zero at
/// `origin_ns`: `eventTime` is `SystemClock.uptimeMillis()`, which is
/// `CLOCK_MONOTONIC` in milliseconds, the clock `origin_ns` was read on
/// (`reader::monotonic_ns`). Never later than `now`, the host's clock at
/// arrival; `now` itself for 0, a send no event caused.
pub(super) fn reported(event_ms: i64, origin_ns: i64, now: f64) -> f64 {
    if event_ms <= 0 {
        return now;
    }
    (event_ms as f64 - origin_ns as f64 / 1e6).min(now)
}

#[cfg(test)]
mod tests {
    use super::reported;

    /// LLP 1127.003 D2: a 150 ms press and a 210 ms one that reach the host
    /// together, late, keep their lengths, on either side of a 200 ms
    /// tap/hold window.
    #[test]
    fn a_late_press_keeps_the_time_its_events_happened() {
        // The host started at 5 s of uptime; the events arrive at 9.5 s.
        let (origin, now) = (5_000_000_000, 4_500.);
        for span in [150, 210] {
            let down = reported(9_000, origin, now);
            let up = reported(9_000 + span, origin, now);
            assert_eq!((down, up - down), (4_000., span as f64));
        }
    }

    #[test]
    fn a_send_no_event_caused_is_its_arrival() {
        assert_eq!(reported(0, 5_000_000_000, 4_500.), 4_500.);
    }

    /// The reader and the host read the clock a moment apart: an event is
    /// never placed after its arrival.
    #[test]
    fn an_event_is_never_stamped_after_its_arrival() {
        assert_eq!(reported(9_600, 5_000_000_000, 4_500.), 4_500.);
    }
}
