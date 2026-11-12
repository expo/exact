//! Which of a canvas's buffers each frame renders into, and which finished
//! frame the reader shows (`crate::buffers`, whose text says why).
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Buffers a canvas owns.
pub(crate) const SLOTS: usize = 6;

/// The slots' states, by index.
#[derive(Default)]
pub(crate) struct Order {
    /// The slot the next frame renders into.
    next: usize,
    /// Slots whose frames are submitted, with the GPU's word that each is
    /// done and whether the frame is published then: an incomplete frame's
    /// slot is busy until its commands have run, and is not shown.
    pending: Vec<(usize, Arc<AtomicBool>, bool)>,
    /// Slots whose frames are done and not yet published, oldest first.
    done: VecDeque<usize>,
    /// The published frame's slot, and the one published before it.
    latest: Option<usize>,
    previous: Option<usize>,
    /// Counts the frames published, so a reader knows a new one.
    serial: u32,
}

impl Order {
    /// The slot this frame renders into: the next one not on the GPU,
    /// waiting, shown or shown a frame ago; failing that, the oldest waiting
    /// frame's, which is dropped. `None` when every slot is on the GPU or
    /// shown: a frame then waits for the GPU (`Module::render_to_buffer`).
    pub(crate) fn target(&mut self) -> Option<usize> {
        let busy = |s: usize, o: &Order| {
            o.pending.iter().any(|&(p, _, _)| p == s)
                || o.done.contains(&s)
                || o.latest == Some(s)
                || o.previous == Some(s)
        };
        let slot = (0..SLOTS)
            .map(|k| (self.next + k) % SLOTS)
            .find(|&s| !busy(s, self))
            .or_else(|| self.done.pop_front())?;
        self.next = (slot + 1) % SLOTS;
        Some(slot)
    }

    /// `slot`'s frame was submitted; `done` is set when the GPU has run it,
    /// and the frame is then published if `publish`.
    pub(crate) fn submitted(&mut self, slot: usize, done: Arc<AtomicBool>, publish: bool) {
        self.pending.push((slot, done, publish));
    }

    /// Frames submitted that the GPU has not finished.
    pub(crate) fn in_flight(&self) -> usize {
        self.pending.len()
    }

    /// Take the frames the GPU has finished: the complete ones wait to be
    /// published, the others' slots are free.
    pub(crate) fn settle(&mut self) {
        while let Some((slot, done, publish)) = self.pending.first() {
            if !done.load(Ordering::Acquire) {
                break;
            }
            if *publish {
                self.done.push_back(*slot);
            }
            self.pending.remove(0);
        }
    }

    /// Publish the oldest finished frame, keeping at most one waiting behind
    /// it; the published frame's slot and serial (as [`Order::latest`]).
    pub(crate) fn take(&mut self) -> Option<(usize, u32)> {
        while self.done.len() > 2 {
            self.done.pop_front();
        }
        if let Some(slot) = self.done.pop_front() {
            self.previous = self.latest;
            self.latest = Some(slot);
            self.serial = self.serial.wrapping_add(1);
        }
        self.latest()
    }

    /// The newest published frame's slot and serial.
    pub(crate) fn latest(&self) -> Option<(usize, u32)> {
        Some((self.latest?, self.serial))
    }

    /// Whether a complete frame has not been published yet: on the GPU, or
    /// finished and waiting for [`Order::take`].
    pub(crate) fn unpublished(&self) -> bool {
        !self.done.is_empty() || self.pending.iter().any(|&(_, _, publish)| publish)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// An incomplete frame's buffer is the GPU's until its commands have
    /// run: no frame renders into it meanwhile, and it is never shown.
    #[test]
    fn an_incomplete_frame_keeps_its_buffer_until_done_and_is_not_shown() {
        let mut o = Order::default();
        let done = |o: &mut Order, slot, publish| {
            let flag = Arc::new(AtomicBool::new(false));
            o.submitted(slot, flag.clone(), publish);
            flag
        };
        let incomplete = o.target().unwrap();
        let gpu = done(&mut o, incomplete, false);
        assert_eq!(o.in_flight(), 1);
        for _ in 0..SLOTS * 2 {
            assert_ne!(
                o.target(),
                Some(incomplete),
                "rendered into while on the GPU"
            );
        }
        gpu.store(true, Ordering::Release);
        o.settle();
        assert_eq!(o.in_flight(), 0);
        assert_eq!(o.take(), None, "an incomplete frame is not published");
        // Its buffer is free again; a complete frame there is published.
        let slot = (0..SLOTS)
            .map(|_| o.target())
            .find(|&s| s == Some(incomplete));
        let complete = done(&mut o, slot.flatten().unwrap(), true);
        complete.store(true, Ordering::Release);
        o.settle();
        assert_eq!(o.take(), Some((incomplete, 1)));
    }

    /// A complete frame is unpublished from its submit until a `take`
    /// publishes it, through any number of calls before the GPU finishes;
    /// an incomplete one never is.
    #[test]
    fn a_complete_frame_is_unpublished_until_taken() {
        let mut o = Order::default();
        assert!(!o.unpublished());
        let slot = o.target().unwrap();
        let gpu = Arc::new(AtomicBool::new(false));
        o.submitted(slot, gpu.clone(), true);
        for _ in 0..3 {
            o.settle();
            assert_eq!(o.take(), None, "published before the GPU finished");
            assert!(o.unpublished(), "on the GPU");
        }
        gpu.store(true, Ordering::Release);
        o.settle();
        assert!(o.unpublished(), "finished and waiting");
        assert_eq!(o.take(), Some((slot, 1)));
        assert!(!o.unpublished());

        let slot = o.target().unwrap();
        let gpu = Arc::new(AtomicBool::new(false));
        o.submitted(slot, gpu.clone(), false);
        assert!(!o.unpublished(), "an incomplete frame is never published");
        gpu.store(true, Ordering::Release);
        o.settle();
        assert!(!o.unpublished());
    }

    /// Two frames finished in one display frame are published one a turn:
    /// the second stays unpublished after the first is taken.
    #[test]
    fn a_second_finished_frame_waits_for_the_next_take() {
        let mut o = Order::default();
        for _ in 0..2 {
            let slot = o.target().unwrap();
            o.submitted(slot, Arc::new(AtomicBool::new(true)), true);
        }
        o.settle();
        assert!(o.take().is_some());
        assert!(o.unpublished());
        assert!(o.take().is_some());
        assert!(!o.unpublished());
    }

    /// With every slot on the GPU or shown there is no slot to render into,
    /// rather than one in use; a finished frame waiting to be shown gives
    /// its slot up.
    #[test]
    fn no_frame_renders_into_a_slot_in_use() {
        let mut o = Order::default();
        let submit = |o: &mut Order, slot| {
            let flag = Arc::new(AtomicBool::new(false));
            o.submitted(slot, flag.clone(), true);
            flag
        };
        for _ in 0..2 {
            let slot = o.target().unwrap();
            submit(&mut o, slot).store(true, Ordering::Release);
            o.settle();
            o.take();
        }
        let mut busy = vec![o.previous.unwrap(), o.latest.unwrap()];
        let mut gpu = Vec::new();
        while let Some(slot) = o.target() {
            assert!(!busy.contains(&slot), "rendered into slot {slot}, in use");
            busy.push(slot);
            gpu.push(submit(&mut o, slot));
        }
        assert_eq!(o.in_flight(), SLOTS - 2, "every other slot on the GPU");
        gpu[0].store(true, Ordering::Release);
        o.settle();
        assert_eq!(o.target(), Some(busy[2]), "the finished frame is dropped");
    }
}
