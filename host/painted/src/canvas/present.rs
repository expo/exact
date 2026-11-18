//! The reader's presents (LLP 1003.001 D9). A picture painted while a
//! present is wanted (something is held, or the page has not been
//! presented) gets a token the reader keeps with its stream; the frame
//! commit callback of the frame that draws it reports the token with the
//! vsync that frame reaches, and the picture's present starts what it
//! carried.
use crate::presenter::Carried;
use std::collections::VecDeque;

/// Pictures painted and not yet reported presented, oldest first.
#[derive(Default)]
pub(crate) struct Pictures {
    next: u64,
    painted: Option<u64>,
    waiting: VecDeque<(u64, Carried, bool)>,
}

/// The reader drops reports it cannot make (a frame never drawn); past this
/// many, the oldest waiting picture is forgotten, and the deadline releases
/// what only it carried.
const WAITING: usize = 16;

impl Pictures {
    /// A turn painted a picture carrying `carried`; `wanted`: its present
    /// is to be reported. `ready` belongs to this picture, not its acknowledgement's turn.
    pub(crate) fn painted(&mut self, carried: Carried, wanted: bool, ready: bool) {
        self.painted = None;
        if !wanted {
            return;
        }
        self.next += 1;
        if self.waiting.len() == WAITING {
            self.waiting.pop_front();
        }
        self.waiting.push_back((self.next, carried, ready));
        self.painted = Some(self.next);
    }

    /// The token of the picture the last turn painted, once; 0 for none.
    pub(crate) fn take(&mut self) -> u64 {
        self.painted.take().unwrap_or(0)
    }

    /// The picture `token` was presented: what it carried. It and every
    /// older picture are forgotten, the later present covering them.
    pub(crate) fn presented(
        &mut self,
        token: u64,
        at: f64,
        launch: &mut crate::launch_marks::LaunchMarks,
    ) -> Option<Carried> {
        let at_index = self.waiting.iter().position(|(t, _, _)| *t == token)?;
        let (_, carried, ready) = self.waiting[at_index];
        self.waiting.drain(..=at_index);
        if !launch.done() {
            launch.presented(at, ready, !self.waiting.is_empty());
        }
        launch.record_done();
        Some(carried)
    }
}

/// A `CLOCK_MONOTONIC` instant in ns (`System.nanoTime`, Choreographer's
/// vsyncs) on the loop's clock, `now_ms` being that clock now: the loop's
/// `Instant` reads `CLOCK_MONOTONIC` too.
#[cfg(target_os = "android")]
pub(crate) fn monotonic_ms(at_ns: i64, now_ms: f64) -> f64 {
    now_ms - (super::reader::monotonic_ns() - at_ns) as f64 / 1e6
}

/// Runners this thread boots from now on await their page's first present:
/// the reader's, outside a driver's launch, where each frame is presented at
/// its commit (LLP 1069.000 D2).
#[cfg(target_os = "android")]
pub(crate) fn await_present() {
    exact_runner::page::set_awaits_present(std::env::var("EXACT_AGENT").as_deref() != Ok("1"));
}

#[cfg(target_os = "android")]
impl<D: exact_runner::DataSource + Default> super::CanvasHost<D> {
    /// The token of the picture the last turn painted, for the reader to
    /// report when its frame is presented; 0 when no present is wanted.
    pub fn picture(&mut self) -> u64 {
        self.presents.take()
    }

    /// The picture `token` was presented at `at_ns` (`System.nanoTime`):
    /// whether a frame is now wanted.
    pub fn picture_presented(&mut self, token: u64, at_ns: i64) -> bool {
        let startup_at =
            crate::launch_marks::boottime() - (super::reader::monotonic_ns() - at_ns) as f64 / 1e9;
        let Some(carried) = self.presents.presented(token, startup_at, &mut self.launch) else {
            return false;
        };
        let at = monotonic_ms(at_ns, self.now());
        if let Some(e) = self.p.picture_presented(carried, at) {
            eprintln!("exact: {e}");
        }
        self.animating()
    }
}

#[cfg(test)]
mod tests {
    use super::Pictures;
    use crate::presenter::Presenter;

    #[test]
    fn a_report_takes_its_picture_and_every_older_one() {
        let (mut p, _) = Presenter::boot_with(
            &crate::test_plan("component App\n  view\n    column\n"),
            (),
            (100., 100.),
            1.,
            std::path::PathBuf::new(),
            crate::presenter::PainterChoice::Cpu,
        )
        .unwrap();
        let mut pictures = Pictures::default();
        let mut launch = crate::launch_marks::LaunchMarks::new();
        pictures.painted(p.picture_carried(), false, false);
        assert_eq!(pictures.take(), 0, "no present wanted");
        let carried = p.picture_carried();
        pictures.painted(carried, true, false);
        let first = pictures.take();
        assert_eq!(pictures.take(), 0, "taken once");
        pictures.painted(carried, true, false);
        let second = pictures.take();
        assert!(second > first);
        assert_eq!(
            pictures.presented(second, crate::launch_marks::boottime(), &mut launch),
            Some(carried)
        );
        assert_eq!(
            pictures.presented(first, crate::launch_marks::boottime(), &mut launch),
            None,
            "covered by the later present"
        );
    }

    #[test]
    fn startup_is_published_once_when_ready_pictures_overlap() {
        use crate::launch_marks::{boottime, LaunchMarks};
        use std::{sync::mpsc, time::Duration};
        const BARRIER: &str = "test.startup.overlap.barrier";
        let (mut p, _) = Presenter::boot_with(
            &crate::test_plan("component App\n  view\n    column\n"),
            (),
            (100., 100.),
            1.,
            std::path::PathBuf::new(),
            crate::presenter::PainterChoice::Cpu,
        )
        .unwrap();
        let (sent, received) = mpsc::channel();
        crate::journal::subscribe(move |event| {
            if event.kind == "startup" || event.kind == BARRIER {
                let _ = sent.send(event.clone());
            }
        });
        let mut launch = LaunchMarks::new();
        launch.launch_end();
        launch.activated();
        launch.recorded();
        let mut pictures = Pictures::default();
        pictures.painted(p.picture_carried(), true, true);
        let token = pictures.take();
        launch.turn(&[]);
        assert!(!launch.done());
        launch.recorded();
        pictures.painted(p.picture_carried(), true, true);
        let newer = pictures.take();
        launch.turn(&[]);
        assert!(!launch.done());
        pictures.presented(token, boottime(), &mut launch);
        launch.record_done();
        assert!(launch.done());
        assert_eq!(launch.timeout_ms(), None, "the reader can now go idle");
        let report = launch.report();
        assert_eq!(report["tti"], "settled");
        assert_eq!(
            report["metrics"]["timeToInteractive"],
            report["metrics"]["timeToFirstRender"]
        );
        let mut count = 0;
        let mut delivered_count = || {
            // Another thread may be draining; the barrier follows every prior event.
            crate::journal::record(BARRIER, Default::default());
            loop {
                let event = received.recv_timeout(Duration::from_secs(10)).unwrap();
                if event.kind == BARRIER {
                    return count;
                }
                if event.fields == report {
                    count += 1;
                }
            }
        };
        assert_eq!(delivered_count(), 1);
        pictures.presented(newer, boottime(), &mut launch);
        launch.record_done();
        assert_eq!(launch.report(), report);
        assert_eq!(delivered_count(), 1);
    }

    #[test]
    fn an_older_picture_cannot_settle_a_newer_loaded_frame() {
        use crate::launch_marks::{boottime, LaunchMarks};
        let (mut p, _) = Presenter::boot_with(
            &crate::test_plan("component App\n  view\n    column\n"),
            (),
            (100., 100.),
            1.,
            std::path::PathBuf::new(),
            crate::presenter::PainterChoice::Cpu,
        )
        .unwrap();
        let mut launch = LaunchMarks::new();
        launch.launch_end();
        launch.activated();
        let mut pictures = Pictures::default();
        launch.recorded();
        pictures.painted(p.picture_carried(), true, false);
        let placeholder = pictures.take();
        launch.turn(&["requests:forecast".into()]);
        launch.recorded();
        pictures.painted(p.picture_carried(), true, true);
        let loaded = pictures.take();
        launch.turn(&[]);
        let first_at = boottime();
        pictures.presented(placeholder, first_at, &mut launch);
        assert!(
            !launch.done(),
            "the placeholder's acknowledgement is still pending content"
        );
        launch.turn(&[]);
        assert!(
            !launch.done(),
            "the loaded frame has not reached the screen"
        );
        let at = boottime();
        pictures.presented(loaded, at, &mut launch);
        let report = launch.report();
        assert_eq!(report["tti"], "settled");
        let tti = report["metrics"]["timeToInteractive"].as_f64().unwrap();
        let ttr = report["metrics"]["timeToFirstRender"].as_f64().unwrap();
        assert!((tti - ttr - (at - first_at)).abs() < 1e-9);
    }
}
