//! When the painted host checks a stream's head (LLP 1030.006 D5, LLP
//! 1030.003 D4): Apple's `ForegroundUpdateChecks`
//! (`host/apple/Sources/ExactUpdates/Updates.swift`) on the presenter's clock.
//! A check runs when the app comes to the foreground, at most once a minute,
//! and every `deploy.check.interval` seconds while it stays there; never in
//! the background. Under `deploy.activate: "immediate"` a staged generation
//! activates as soon as it is staged in the foreground, or at the next
//! foreground.

/// `deploy.check.interval`'s default, 15 minutes, as Apple's.
const DEFAULT_INTERVAL_MS: f64 = 15.0 * 60.0 * 1000.0;
/// The least time between two checks a return to the foreground starts.
const FOREGROUND_GAP_MS: f64 = 60_000.0;

/// What the cadence asks of the presenter now.
#[derive(Debug, Default, PartialEq, Eq)]
pub(super) struct Asks {
    pub check: bool,
    pub activate: bool,
}

#[derive(Debug)]
pub(super) struct Checks {
    interval_ms: f64,
    immediate: bool,
    foreground: bool,
    staged: bool,
    last_foreground_check: Option<f64>,
    due: Option<f64>,
}

impl Checks {
    /// The cadence `compat.json`'s `delivery` names: `activate` and
    /// `checkInterval` (seconds), as the bake records them.
    pub fn from_compat(compat: &str) -> Self {
        let delivery = serde_json::from_str::<serde_json::Value>(compat)
            .ok()
            .and_then(|v| v.get("delivery").cloned())
            .unwrap_or_default();
        let interval = delivery
            .get("checkInterval")
            .and_then(serde_json::Value::as_f64)
            .filter(|s| s.is_finite() && *s > 0.0);
        Self::new(
            interval.map(|s| s * 1000.0),
            delivery.get("activate").and_then(|a| a.as_str()) == Some("immediate"),
        )
    }

    fn new(interval_ms: Option<f64>, immediate: bool) -> Self {
        Self {
            interval_ms: interval_ms.unwrap_or(DEFAULT_INTERVAL_MS),
            immediate,
            foreground: false,
            staged: false,
            last_foreground_check: None,
            due: None,
        }
    }

    /// The app came to the foreground or left it at `now`.
    pub fn foreground(&mut self, foreground: bool, now: f64) -> Asks {
        if foreground == self.foreground {
            return Asks::default();
        }
        self.foreground = foreground;
        self.due = None;
        if !foreground {
            return Asks::default();
        }
        let check = self
            .last_foreground_check
            .is_none_or(|at| now - at >= FOREGROUND_GAP_MS);
        if check {
            self.last_foreground_check = Some(now);
        }
        self.due = Some(now + self.interval_ms);
        Asks {
            check,
            activate: self.take_staged(),
        }
    }

    /// Whether the periodic check is due at `now`; it is then due again an
    /// interval later.
    pub fn tick(&mut self, now: f64) -> bool {
        match self.due {
            Some(due) if self.foreground && now >= due => {
                self.due = Some(now + self.interval_ms);
                true
            }
            _ => false,
        }
    }

    /// When the next periodic check is due, on the presenter's clock.
    pub fn due(&self) -> Option<f64> {
        self.due.filter(|_| self.foreground)
    }

    /// A check finished with `line`: whether to activate what it staged.
    pub fn completed(&mut self, line: &str) -> bool {
        if line.starts_with("staged") {
            self.staged = true;
        }
        self.take_staged()
    }

    /// Automatic activation waits until the app is in the foreground.
    pub fn activation_allowed(&self) -> bool {
        !self.immediate || self.foreground
    }

    fn take_staged(&mut self) -> bool {
        let now = self.immediate && self.foreground && self.staged;
        if now {
            self.staged = false;
        }
        now
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CHECK: Asks = Asks {
        check: true,
        activate: false,
    };

    #[test]
    fn a_foreground_checks_at_most_once_a_minute_and_then_every_interval() {
        let mut c = Checks::new(Some(300_000.0), false);
        assert_eq!(c.foreground(true, 0.0), CHECK);
        assert_eq!(
            c.foreground(true, 1.0),
            Asks::default(),
            "already in the foreground"
        );
        assert!(!c.tick(299_999.0));
        assert!(c.tick(300_000.0));
        assert_eq!(c.due(), Some(600_000.0));
        c.foreground(false, 310_000.0);
        assert_eq!(c.due(), None, "no check in the background");
        assert!(!c.tick(700_000.0));
        // Back within a minute of the last foreground check: none.
        let mut c = Checks::new(None, false);
        c.foreground(true, 0.0);
        c.foreground(false, 10_000.0);
        assert_eq!(c.foreground(true, 59_999.0), Asks::default());
        assert_eq!(c.due(), Some(59_999.0 + DEFAULT_INTERVAL_MS));
        c.foreground(false, 60_000.0);
        assert_eq!(c.foreground(true, 61_000.0), CHECK);
    }

    #[test]
    fn immediate_activates_what_a_foreground_check_staged() {
        let mut c = Checks::new(None, true);
        c.foreground(true, 0.0);
        assert!(!c.completed("current"));
        assert!(c.completed("staged seq 2"));
        assert!(!c.completed("current"), "activated once");
        // Staged while in the background: activated at the next foreground.
        c.foreground(false, 1_000.0);
        assert!(!c.completed("staged seq 3"));
        assert_eq!(
            c.foreground(true, 2_000.0),
            Asks {
                check: false,
                activate: true
            }
        );
        // Without immediate, a staged generation waits for the app or a launch.
        let mut c = Checks::new(None, false);
        c.foreground(true, 0.0);
        assert!(!c.completed("staged seq 2"));
    }

    #[test]
    fn the_cadence_is_read_from_compat() {
        let c = Checks::from_compat(
            r#"{"id":"x","inputs":{},"delivery":{"activate":"immediate","checkInterval":60}}"#,
        );
        assert!(c.immediate);
        assert_eq!(c.interval_ms, 60_000.0);
        let c = Checks::from_compat(r#"{"delivery":{"activate":"next-launch"}}"#);
        assert!(!c.immediate);
        assert_eq!(c.interval_ms, DEFAULT_INTERVAL_MS);
    }
}

/// The cadence and the blessing through a presenter, on its clock.
#[cfg(test)]
mod presenter_tests {
    use crate::delivery::{Selection, Store};
    use crate::presenter::{PainterChoice, Presenter};
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    #[derive(Default)]
    struct Log {
        checks: u32,
        blessed: u32,
        lines: Vec<String>,
    }

    /// A store that counts checks and blessings and answers queued lines.
    struct Counted(Arc<Mutex<Log>>);

    impl Store for Counted {
        fn prepare_selected(&mut self) -> Option<Selection> {
            None
        }
        fn boot_started(&mut self) {}
        fn entry_refused(&mut self, _: &str, _: &str) {}
        fn selection_corrupt(&mut self, _: &str) {}
        fn boot_succeeded(&mut self) {
            self.0.lock().unwrap().blessed += 1;
        }
        fn take_note(&mut self) -> Option<String> {
            None
        }
        #[cfg(unix)]
        fn fd(&self) -> std::os::unix::io::RawFd {
            -1
        }
        fn check(&self) -> bool {
            self.0.lock().unwrap().checks += 1;
            true
        }
        fn take_line(&mut self) -> Option<String> {
            self.0.lock().unwrap().lines.pop()
        }
        fn prepare_activation(&self) -> Result<Option<Selection>, String> {
            Ok(None)
        }
        fn commit_activation(&mut self, _: Option<String>, _: u64) -> Result<(), String> {
            Ok(())
        }
        fn staged_stream_into(&self, _: &mut exact_runner::Delivery) {}
        fn status_into(&self, _: &mut exact_runner::Delivery) {}
    }

    fn presenter(delivery: &str) -> (Presenter<()>, Arc<Mutex<Log>>) {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan("component App\n  view\n    button \"Go\"\n"),
            (),
            (100., 100.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.compat = format!(r#"{{"id":"x","inputs":{{}},"delivery":{delivery}}}"#);
        let log = Arc::new(Mutex::new(Log::default()));
        p.set_updates(Some(Box::new(Counted(log.clone()))));
        assert!(p.clock(1_000.0).1.is_none());
        (p, log)
    }

    fn first_pixel(p: &mut Presenter<()>) {
        let _ = p.frame();
        p.first_pixel();
        assert!(p.first_pixel_told());
        p.update_launched();
    }

    #[test]
    fn the_first_frame_checks_once_and_the_interval_follows_while_shown() {
        let (mut p, log) = presenter(r#"{"activate":"next-launch","checkInterval":60}"#);
        let checks = |log: &Arc<Mutex<Log>>| log.lock().unwrap().checks;
        p.update_launched();
        p.poll_update();
        assert_eq!(checks(&log), 0, "none before first pixel");
        first_pixel(&mut p);
        assert_eq!(checks(&log), 1, "the launch's check");
        assert_eq!(
            log.lock().unwrap().blessed,
            1,
            "no reader: first pixel blesses"
        );
        first_pixel(&mut p);
        p.poll_update();
        assert_eq!(checks(&log), 1);
        assert_eq!(p.update_check_due(), Some(60_000.0));
        assert!(p.clock(61_000.0).1.is_none());
        p.poll_update();
        assert_eq!(checks(&log), 2, "the interval");
        p.page_execution(true, true);
        assert!(p.clock(200_000.0).1.is_none());
        p.poll_update();
        assert_eq!(checks(&log), 2, "none while hidden");
        p.page_execution(false, true);
        assert_eq!(checks(&log), 3, "back in the foreground");
    }

    #[test]
    fn a_reader_blesses_once_drawn_and_immediate_waits_for_a_held_touch() {
        let (mut p, log) = presenter(r#"{"activate":"immediate"}"#);
        p.set_reader_blesses();
        assert!(p.reader_drew(), "before first pixel: asked again");
        first_pixel(&mut p);
        assert_eq!(log.lock().unwrap().blessed, 0, "first pixel does not bless");
        assert!(!p.reader_drew());
        assert!(!p.reader_drew());
        assert_eq!(log.lock().unwrap().blessed, 1);
        log.lock().unwrap().lines.push("staged seq 2".into());
        p.pointer_down(10., 10., 1_000.0).unwrap();
        assert!(p.poll_update());
        assert!(p.pending_update);
        p.run_commands(|| ());
        assert!(p.pending_update, "a held touch defers the activation");
        p.pointer_up(10., 10., 1_000.0).unwrap();
        p.run_commands(|| ());
        assert!(!p.pending_update, "applied at the first turn with none");
    }
}
