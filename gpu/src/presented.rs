//! A canvas's `presented` fact (LLP 1009, amended 2026-11-12): whether a
//! complete frame of its current picture has been handed to the compositor.
//! The module answers it on every host and adds it to the record the surface
//! publishes, which the app reads as `exactSurface(name).presented`, so a
//! loader can stay over the canvas until there is a picture under it.
//!
//! The picture is current from the canvas's making, and again after a
//! restore, a lost device, a new target or a new [`crate::Surface::generation`]:
//! each makes the fact false until a frame drawn after it is presented. A
//! changed fact sends the surface's last record again; a frame that changes
//! nothing sends nothing, so presenting costs the app no commit per frame.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

/// One canvas's fact and the record it rides in.
pub(crate) struct Presented {
    /// Which picture is current: it moves when what the compositor was shown
    /// stops being this canvas's picture.
    epoch: u64,
    /// The latest epoch a frame of which reached the compositor; written by
    /// whatever thread hears so ([`Shown::shown`]).
    shown: Arc<AtomicU64>,
    /// The surface's generation at the last look.
    generation: u64,
    /// The surface's latest record, to send again when the fact changes.
    record: Option<String>,
    /// The fact the host was last given; `None` before anything was sent.
    told: Option<bool>,
}

impl Default for Presented {
    fn default() -> Self {
        Self {
            epoch: 1,
            shown: Default::default(),
            generation: 0,
            record: None,
            told: None,
        }
    }
}

/// A frame of a canvas's current picture, on its way to the compositor.
pub(crate) struct Shown {
    at: Arc<AtomicU64>,
    epoch: u64,
}

impl Shown {
    /// The frame has been handed to the compositor.
    pub(crate) fn shown(self) {
        self.at.fetch_max(self.epoch, Ordering::AcqRel);
    }
}

impl Presented {
    /// Whether a complete frame of the current picture has been presented.
    pub(crate) fn is(&self) -> bool {
        self.shown.load(Ordering::Acquire) >= self.epoch
    }

    /// What was presented is no longer the current picture.
    pub(crate) fn reset(&mut self) {
        self.epoch += 1;
    }

    /// Replace `target` with `make()` when it no longer `fits` (Android's
    /// buffers at a new size): what was presented was the old target's, so
    /// the picture starts over, as for any target let go of.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn refit<T>(
        &mut self,
        target: &mut Option<T>,
        fits: impl FnOnce(&T) -> bool,
        make: impl FnOnce() -> Option<T>,
    ) {
        if target.as_ref().is_some_and(|t| !fits(t)) {
            *target = make();
            self.reset();
        }
    }

    /// The surface says which picture it draws; a new one starts over.
    pub(crate) fn generation(&mut self, generation: u64) {
        if generation != self.generation {
            self.generation = generation;
            self.reset();
        }
    }

    /// The token for a complete frame just recorded, while the current
    /// picture has none presented.
    pub(crate) fn pending(&self) -> Option<Shown> {
        (!self.is()).then(|| Shown {
            at: self.shown.clone(),
            epoch: self.epoch,
        })
    }

    /// A complete frame the host itself puts on screen (a read-back canvas
    /// it composes into its own frame) has been shown. Not on the web, where
    /// every canvas presents through `gpu_flush`.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) fn now(&self) {
        self.shown.fetch_max(self.epoch, Ordering::AcqRel);
    }

    /// The record replaced by a restore is not sent again.
    pub(crate) fn forget_record(&mut self) {
        self.record = None;
    }

    /// The record for the host, if it has something new: the surface's
    /// `fresh` one, or its last again when the fact changed. Nothing before
    /// the surface published or a frame was presented.
    pub(crate) fn take(&mut self, fresh: Option<String>) -> Option<String> {
        let presented = self.is();
        let changed = fresh.is_some();
        if let Some(record) = fresh {
            self.record = Some(record);
        }
        if !changed && self.told.unwrap_or(false) == presented {
            return None;
        }
        self.told = Some(presented);
        Some(with_presented(self.record.as_deref(), presented))
    }
}

/// `record` with `presented` as its last member. The name is the module's: a
/// record that has its own `presented` is refused by the runner as a duplicate
/// key, as one that is not an object (passed as it is) is refused by name.
pub(crate) fn with_presented(record: Option<&str>, presented: bool) -> String {
    let record = record.map_or("{}", str::trim);
    let Some(body) = record
        .strip_prefix('{')
        .and_then(|r| r.strip_suffix('}'))
        .map(str::trim)
    else {
        return record.into();
    };
    if body.is_empty() {
        format!("{{\"presented\":{presented}}}")
    } else {
        format!("{{{body},\"presented\":{presented}}}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fact_rides_last_in_the_record_and_only_a_change_resends_it() {
        let mut p = Presented::default();
        assert_eq!(p.take(None), None, "nothing published, nothing presented");
        assert_eq!(
            p.take(Some(r#"{"score":1}"#.into())).as_deref(),
            Some(r#"{"score":1,"presented":false}"#)
        );
        assert_eq!(p.take(None), None);
        p.pending().unwrap().shown();
        assert!(p.pending().is_none());
        assert_eq!(
            p.take(None).as_deref(),
            Some(r#"{"score":1,"presented":true}"#)
        );
        assert_eq!(p.take(None), None, "later frames change nothing");
        p.generation(1);
        assert_eq!(
            p.take(None).as_deref(),
            Some(r#"{"score":1,"presented":false}"#)
        );
    }

    #[test]
    fn a_frame_of_an_earlier_picture_does_not_present_this_one() {
        let mut p = Presented::default();
        let late = p.pending().unwrap();
        p.reset();
        late.shown();
        assert!(!p.is());
        p.now();
        assert!(p.is());
        assert_eq!(p.take(None).as_deref(), Some(r#"{"presented":true}"#));
    }

    /// A presented canvas whose buffers are made again at a new size is not
    /// presented until a frame in the new ones is; the same size keeps them.
    #[test]
    fn buffers_made_again_at_a_new_size_start_the_picture_over() {
        let mut p = Presented::default();
        let mut ring = Some((4, 4));
        p.pending().unwrap().shown();
        p.refit(&mut ring, |r| *r == (4, 4), || panic!("same size"));
        assert!(p.is());
        p.refit(&mut ring, |r| *r == (8, 4), || Some((8, 4)));
        assert_eq!(ring, Some((8, 4)));
        assert!(!p.is(), "the new buffers hold no frame");
        p.pending().unwrap().shown();
        assert!(p.is());
    }

    #[test]
    fn a_non_object_passes_through_for_the_runner_to_refuse() {
        assert_eq!(
            with_presented(Some(r#"{"presented":7}"#), true),
            r#"{"presented":7,"presented":true}"#
        );
        assert_eq!(
            with_presented(Some(" {} "), false),
            r#"{"presented":false}"#
        );
        assert_eq!(with_presented(Some("[1]"), true), "[1]");
    }
}
