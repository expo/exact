//! `perf frames` on the Android reader (LLP 1079.000 §6.2): the reader's
//! turns, ingests and window frames reach the two sources of
//! [`crate::frame_health::android::Reader`] here, on exact2's thread, and an
//! agent's request reads them. The display's period they share goes to the
//! GPU canvases too (`gpu_period`, LLP 1009), and each canvas's frame-rate
//! cap back to the reader (`Native.frameRates`).
//!
//! Collection runs only in a driver's launch (`EXACT_AGENT=1`), which a
//! production bake never is (LLP 1069.007 D2). Under the driver's clock a
//! frame is virtual and nothing is recorded; a capture lease lends the
//! display the clock ([`super::agent::lend`]) so its frames are real.

use super::{agent, reader::monotonic_ns, CanvasHost};
use crate::frame_health::android::{Lend, Reader};
use exact_runner::DataSource;
use std::cell::RefCell;

thread_local! {
    static READER: RefCell<Option<Reader>> = const { RefCell::new(None) };
}

fn with<T>(f: impl FnOnce(&mut Reader) -> T) -> T {
    READER.with_borrow_mut(|r| {
        f(
            r.get_or_insert_with(|| {
                Reader::new(std::env::var("EXACT_AGENT").as_deref() == Ok("1"))
            }),
        )
    })
}

/// The device a report names, told as the reader starts: before a host may run.
pub fn identity(identity: &str) {
    if let Ok(identity) = serde_json::from_str(identity) {
        with(|r| r.identity = identity);
    }
}

/// The display's refresh period, told where no host runs yet.
pub fn display_period(period: u64) {
    with(|r| r.remember_display_period(period));
}

impl<D: DataSource + Default> CanvasHost<D> {
    /// A Choreographer turn (`Native.frameTurn`).
    pub fn frame_turn(
        &mut self,
        arrival: u64,
        vsync: u64,
        deadline: u64,
        period: u64,
        finished: u64,
        publication: u64,
    ) {
        self.frame_clock(finished);
        let given = with(|r| r.turned(arrival, vsync, deadline, period, finished, publication));
        self.give_period(given);
    }

    /// The main thread ingested every publication through `applied`.
    pub fn frame_landed(&mut self, applied: u64, began: u64, ended: u64) {
        with(|r| r.landed(applied, began, ended));
    }

    /// No turn is asked for.
    pub fn frame_idle(&mut self) {
        with(Reader::idle);
    }

    /// The display's refresh period.
    pub fn frame_period(&mut self, period: u64) {
        let given = with(|r| r.display_period(period));
        self.give_period(given);
    }

    /// Window frames the reader drained, and the reports lost.
    pub fn window_frames(&mut self, words: &[i64], lost: u64) {
        self.frame_clock(monotonic_ns().max(0) as u64);
        with(|r| r.window_frames(words, lost));
    }

    /// The frame-rate caps of the GPU canvases that changed since last asked.
    pub fn frame_rates(&mut self) -> Option<Vec<i32>> {
        self.p.surfaces.frame_rates()
    }

    fn give_period(&mut self, ms: Option<f64>) {
        if let Some(ms) = ms {
            self.p.surfaces.period(ms);
        }
    }

    /// Whose clock the sources run under at `now`; a lease that ended gives
    /// the driver its clock back.
    fn frame_clock(&mut self, now: u64) {
        if agent::lent() && !with(|r| r.app.leased(now)) {
            agent::unlend(&mut self.p);
        }
        let virtual_clock = agent::clock(&self.p).is_some();
        with(|r| r.clock(virtual_clock, now));
    }

    /// A `perf` request with `frames`, answered without a turn; `None` for
    /// any other. A capture's reply carries the runner's clock, which the
    /// driver follows once a lease returns it.
    pub(super) fn frames_request(&mut self, line: &str) -> Option<String> {
        let q: serde_json::Value = serde_json::from_str(line).ok()?;
        if q["frames"] != true {
            return None;
        }
        let now = monotonic_ns().max(0) as u64;
        self.frame_clock(now);
        let (reply, lend) = with(|r| r.answer(&q, now));
        match lend {
            Lend::Wall => agent::lend(&mut self.p, self.started),
            Lend::Driver => agent::unlend(&mut self.p),
            Lend::Keep => {}
        }
        if !q["capture"].is_string() {
            return Some(reply);
        }
        self.frame_clock(now);
        let mut reply: serde_json::Value = serde_json::from_str(&reply).ok()?;
        // Capture starts once the lease has lent the clock: after the reply was made.
        if reply["capture"].is_boolean() {
            reply["capture"] = with(|r| r.app.capturing()).into();
        }
        reply["clock"] = self.p.host().now().into();
        reply["lent"] = agent::lent().into();
        Some(reply.to_string())
    }
}
