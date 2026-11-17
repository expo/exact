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
    waiting: VecDeque<(u64, Carried)>,
}

/// The reader drops reports it cannot make (a frame never drawn); past this
/// many, the oldest waiting picture is forgotten, and the deadline releases
/// what only it carried.
const WAITING: usize = 16;

impl Pictures {
    /// A turn painted a picture carrying `carried`; `wanted`: its present
    /// is to be reported.
    pub(crate) fn painted(&mut self, carried: Carried, wanted: bool) {
        self.painted = None;
        if !wanted {
            return;
        }
        self.next += 1;
        if self.waiting.len() == WAITING {
            self.waiting.pop_front();
        }
        self.waiting.push_back((self.next, carried));
        self.painted = Some(self.next);
    }

    /// The token of the picture the last turn painted, once; 0 for none.
    pub(crate) fn take(&mut self) -> u64 {
        self.painted.take().unwrap_or(0)
    }

    /// The picture `token` was presented: what it carried. It and every
    /// older picture are forgotten, the later present covering them.
    pub(crate) fn presented(&mut self, token: u64) -> Option<Carried> {
        let at = self.waiting.iter().position(|(t, _)| *t == token)?;
        let carried = self.waiting[at].1;
        self.waiting.drain(..=at);
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
        let Some(carried) = self.presents.presented(token) else {
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
        pictures.painted(p.picture_carried(), false);
        assert_eq!(pictures.take(), 0, "no present wanted");
        let carried = p.picture_carried();
        pictures.painted(carried, true);
        let first = pictures.take();
        assert_eq!(pictures.take(), 0, "taken once");
        pictures.painted(carried, true);
        let second = pictures.take();
        assert!(second > first);
        assert_eq!(pictures.presented(second), Some(carried));
        assert_eq!(
            pictures.presented(first),
            None,
            "covered by the later present"
        );
    }
}
