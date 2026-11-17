//! Read readiness is deterministic: scripted syscalls, the actual event path.
use super::*;
use drm::control::PageFlipEvent;
use std::cell::Cell;
use std::collections::VecDeque;
use std::io::{self, ErrorKind};
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

fn crtc(id: u32) -> crtc::Handle {
    NonZeroU32::new(id).unwrap().into()
}
fn flip(crtc_id: u32, sequence: u32) -> Event {
    Event::PageFlip(PageFlipEvent {
        frame: sequence,
        duration: Duration::from_millis(16),
        crtc: crtc(crtc_id),
    })
}
fn read_once(result: io::Result<Vec<Event>>) -> io::Result<Option<u32>> {
    read_timed(result).map(|r| r.map(|(sequence, _)| sequence))
}
fn read_timed(result: io::Result<Vec<Event>>) -> io::Result<Option<(u32, Duration)>> {
    let mut result = Some(result);
    receive_flip(crtc(7), || {
        result
            .take()
            .expect("one readable turn must perform at most one bounded DRM read")
    })
}

/// The kernel's flip timestamp is returned, not the time the loop read the
/// event: TTI uses it as the moment the frame was shown.
#[test]
fn matching_crtc_read_accepts_the_frame_sequence() {
    assert_eq!(
        read_timed(Ok(vec![flip(7, 30)])).unwrap(),
        Some((30, Duration::from_millis(16)))
    );
}

#[test]
fn wrong_crtc_does_not_complete_a_submitted_frame_or_request_another_read() {
    assert_eq!(read_once(Ok(vec![flip(8, 30)])).unwrap(), None);
}

#[test]
fn nonblocking_empty_and_interrupted_reads_leave_the_flip_pending() {
    for kind in [ErrorKind::WouldBlock, ErrorKind::Interrupted] {
        assert_eq!(read_once(Err(io::Error::from(kind))).unwrap(), None);
    }
}

#[test]
fn empty_event_batch_yields_to_existing_input_worker_and_timer_processing() {
    let reads = Cell::new(0);
    let mut scheduled = VecDeque::from([Ok(Vec::new()), Ok(vec![flip(7, 3)])]);
    let result = receive_flip(crtc(7), || {
        reads.set(reads.get() + 1);
        scheduled.pop_front().expect("bounded scripted events")
    })
    .unwrap();
    assert_eq!(
        result, None,
        "a readiness turn must not wait for a later flip"
    );
    assert_eq!(
        reads.get(),
        1,
        "leave the next read to the next FD-ready turn"
    );
    assert_eq!(scheduled.len(), 1);
}

#[test]
fn flip_completion_does_not_depend_on_sequence_order() {
    let mut slots = FlipState::default();
    slots.submit(frame(1), |_, _, _| Ok(())).unwrap();
    for sequence in [50, 49, 49, u32::MAX, 0, 1 << 31, 1] {
        slots.submit(frame(2), |_, _, _| Ok(())).unwrap();
        let completed = slots
            .ready(crtc(7), || Ok(vec![flip(7, sequence)]))
            .unwrap()
            .unwrap();
        assert_eq!(completed.data()[0], 2);
        assert!(!slots.pending());
        assert_eq!(slots.sequence, Some(sequence));
    }
}

#[test]
fn read_errors_propagate_without_retry() {
    let error = read_once(Err(io::Error::from(ErrorKind::BrokenPipe))).unwrap_err();
    assert_eq!(error.kind(), ErrorKind::BrokenPipe);
}

fn frame(value: u8) -> Arc<Pixmap> {
    let mut pixels = Pixmap::new(1, 1).unwrap();
    pixels.data_mut()[0] = value;
    Arc::new(pixels)
}

#[test]
fn two_buffers_have_one_pending_owner_and_neither_front_nor_pending_is_overwritten() {
    let mut slots = FlipState::default();
    let mut writes = Vec::new();
    let first = slots
        .submit(frame(1), |back, first, _| {
            writes.push((back, first));
            Ok(())
        })
        .unwrap()
        .unwrap();
    assert_eq!(slots.front, 1);
    assert!(!slots.pending());
    assert_eq!(first.data()[0], 1);
    assert!(slots
        .submit(frame(2), |back, first, _| {
            writes.push((back, first));
            Ok(())
        })
        .unwrap()
        .is_none());
    assert_eq!(slots.front, 1);
    assert!(slots.pending());
    assert!(slots
        .submit(frame(99), |_, _, _| panic!(
            "occupied slot must refuse BEFORE copy"
        ))
        .is_err());
    assert_eq!(writes, [(1, true), (0, false)]);
    let completed = slots
        .ready(crtc(7), || Ok(vec![flip(7, 21)]))
        .unwrap()
        .unwrap();
    assert_eq!(completed.data()[0], 2);
    assert_eq!(slots.front, 0);
    assert!(!slots.pending());
    slots
        .submit(frame(3), |back, first, _| {
            writes.push((back, first));
            Ok(())
        })
        .unwrap();
    assert_eq!(writes, [(1, true), (0, false), (1, false)]);
}

#[test]
fn copy_or_ioctl_failure_does_not_change_front_or_claim_a_submission() {
    let mut slots = FlipState::default();
    assert!(slots
        .submit(frame(1), |_, _, _| Err("set_crtc refusal".into()))
        .is_err());
    assert!(slots.first);
    assert_eq!(slots.front, 0);
    assert!(!slots.pending());
    slots.submit(frame(1), |_, _, _| Ok(())).unwrap();
    let rejected = frame(2);
    let weak = Arc::downgrade(&rejected);
    assert!(slots
        .submit(rejected, |back, first, _| {
            assert_eq!(back, 0);
            assert!(!first);
            Err("page_flip refusal after copy".into())
        })
        .is_err());
    assert!(weak.upgrade().is_none());
    assert_eq!(slots.front, 1);
    assert!(!slots.pending());
}

#[test]
fn no_event_wrong_crtc_and_read_errors_retain_the_exact_pending_pixels() {
    let mut slots = FlipState::default();
    slots.submit(frame(1), |_, _, _| Ok(())).unwrap();
    let submitted = frame(2);
    let weak = Arc::downgrade(&submitted);
    slots.submit(submitted, |_, _, _| Ok(())).unwrap();
    for read in [
        Ok(Vec::new()),
        Ok(vec![flip(8, 21)]),
        Err(io::Error::from(ErrorKind::WouldBlock)),
        Err(io::Error::from(ErrorKind::Interrupted)),
    ] {
        assert!(slots.ready(crtc(7), || read).unwrap().is_none());
        assert!(slots.pending());
        assert_eq!(slots.front, 1);
        assert_eq!(weak.upgrade().unwrap().data()[0], 2);
    }
    assert!(slots
        .ready(crtc(7), || Err(io::Error::from(ErrorKind::BrokenPipe)))
        .is_err());
    assert!(slots.pending());
    drop(slots);
    assert!(
        weak.upgrade().is_none(),
        "shutdown must not retain a pixmap history"
    );
}

#[test]
fn events_without_a_pending_flip_are_drained_without_changing_front() {
    let mut slots = FlipState::default();
    assert!(slots
        .ready(crtc(7), || Ok(vec![flip(7, 39)]))
        .unwrap()
        .is_none());
    assert_eq!(slots.front, 0);
    assert!(slots.first);
    assert_eq!(slots.sequence, None);
    assert_eq!(slots.at, None);
    slots.submit(frame(1), |_, _, _| Ok(())).unwrap();
    slots.submit(frame(2), |_, _, _| Ok(())).unwrap();
    slots
        .ready(crtc(7), || Ok(vec![flip(7, 40), flip(7, 40)]))
        .unwrap()
        .unwrap();
    assert!(slots
        .ready(crtc(7), || Ok(vec![flip(7, 41)]))
        .unwrap()
        .is_none());
    assert_eq!(slots.front, 0);
    assert!(!slots.pending());
    assert_eq!(slots.sequence, Some(40));
    assert_eq!(slots.at, Some(Duration::from_millis(16)));
    slots.submit(frame(3), |_, _, _| Ok(())).unwrap();
    assert!(slots.ready(crtc(7), || Ok(vec![])).unwrap().is_none());
    assert!(slots.pending());
    let third = slots
        .ready(crtc(7), || Ok(vec![flip(7, 41)]))
        .unwrap()
        .unwrap();
    assert_eq!(third.data()[0], 3);
    assert_eq!(slots.front, 1);
    assert!(!slots.pending());
}

#[test]
fn repeated_zero_sequences_complete_each_submitted_frame() {
    let mut slots = FlipState::default();
    let first = slots.submit(frame(1), |_, _, _| Ok(())).unwrap().unwrap();
    assert_eq!(first.data()[0], 1);
    for value in 2..=6 {
        assert!(slots
            .submit(frame(value), |_, _, _| Ok(()))
            .unwrap()
            .is_none());
        let at = Duration::from_millis(u64::from(value) * 16);
        let completed = slots
            .ready(crtc(7), || {
                Ok(vec![Event::PageFlip(PageFlipEvent {
                    frame: 0,
                    duration: at,
                    crtc: crtc(7),
                })])
            })
            .unwrap()
            .expect("a flip with no vblank counter must release the pending frame");
        assert_eq!(completed.data()[0], value);
        assert_eq!(slots.front, usize::from(value % 2));
        assert!(!slots.pending());
        assert_eq!(slots.sequence, Some(0));
        assert_eq!(slots.at, Some(at));
    }
}
