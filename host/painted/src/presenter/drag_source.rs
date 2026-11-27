//! A drag of a node of the app, heard at its source (LLP 1094.000 D16), on
//! the hosts the presenter asks to lift: Android's `startDragAndDrop` and
//! Linux's Wayland source. The presenter numbers each lift it asks for; the
//! host says when the platform began it, and when it ended. The node hears
//! `dragstart` then, before the app's own drop side hears the drag, and
//! `dragend` once the app's own drop of it, if any, is delivered or failed.
//!
//! Each lift is kept by its number until its end is heard, as a drop of it
//! may still load when the next lift begins. A drop is a lift's only when
//! the host's drop side knows it as that lift's (Android's local state,
//! Linux's offer of the lift's own type).
use super::*;
use exact_kernel::NodeKey;
use exact_runner::{DropEffect, DropEvent, KeyModifiers};

/// A lift the presenter asked a host for, which the platform may have begun.
pub(super) struct Lifting {
    /// The lift's number, by which the host names it.
    pub(super) id: u32,
    /// The source node, and the point in its box where the drag began.
    pub(super) key: NodeKey,
    offsets: (f64, f64),
    /// Whether the platform began it, and the node heard `dragstart`.
    begun: bool,
    /// The app's own drop of the drag while it loads, which holds its end.
    dropping: Option<u32>,
    /// Whether the app's own drop was delivered: the outcome then, over the
    /// platform's, which comes before the drop's items arrive.
    delivered: Option<bool>,
    /// The platform's end, held for that drop: whether a destination took
    /// it, and the keys held.
    ended: Option<(bool, KeyModifiers)>,
    /// A commit renewed the source for another item (LLP 1078), which
    /// hears nothing of this drag.
    pub(super) renewed: bool,
}

impl<D: exact_runner::DataSource> Presenter<D> {
    /// A lift of `source`, held at `at`, is asked for: its number, which the
    /// host names it by when the platform begins and ends it, or refuses it
    /// (`lift_ended` of a lift not begun).
    pub(in crate::presenter) fn lift_asked(&mut self, source: ViewId, at: (f32, f32)) -> u32 {
        self.drag_in.lifts = self.drag_in.lifts.wrapping_add(1);
        let id = self.drag_in.lifts;
        let key = self.host.kernel().node(source).map(|node| node.key);
        let offsets = self.pointer_record(source, at.0, at.1, -1, 0);
        // Each lift stays until its host says the platform began and ended it, or refused it
        if let Some((key, record)) = key.zip(offsets) {
            self.drag_in.lifting.push(Lifting {
                id,
                key,
                offsets: (record.offset_x, record.offset_y),
                begun: false,
                dropping: None,
                delivered: None,
                ended: None,
                renewed: false,
            });
        }
        id
    }

    fn lift_mut(&mut self, lift: u32) -> Option<&mut Lifting> {
        self.drag_in.lifting.iter_mut().find(|s| s.id == lift)
    }

    /// The platform began lift `lift`: its node hears `dragstart`, with the
    /// keys `held` (LLP 1094.000 D16). The host says so outside the
    /// platform's own callback, and before its drop side reports the drag.
    pub fn lift_began(&mut self, lift: u32, held: KeyModifiers) {
        let Some(source) = self.lift_mut(lift).filter(|s| !s.begun) else {
            return;
        };
        source.begun = true;
        let (key, offsets, renewed) = (source.key, source.offsets, source.renewed);
        if !renewed {
            self.source_event(key, offsets, None, held);
        }
    }

    /// The platform ended lift `lift`, a destination taking it or not
    /// (`taken`): its node hears `dragend` now, or once the app's own drop
    /// of it lands. A lift the platform never began ends with no event.
    pub fn lift_ended(&mut self, lift: u32, taken: bool, held: KeyModifiers) {
        let Some(source) = self.lift_mut(lift) else {
            return;
        };
        if !source.begun {
            self.drag_in.lifting.retain(|s| s.id != lift);
        } else if source.dropping.is_some() {
            source.ended = Some((taken, held));
        } else {
            self.lift_finished(lift, taken, held);
        }
    }

    /// Drop `drop` is of lift `lift`, as the host's drop side knows it: the
    /// lift's end waits for it to land.
    pub(in crate::presenter) fn lift_drops(&mut self, drop: u32, lift: Option<u32>) {
        if let Some(source) = lift
            .and_then(|lift| self.lift_mut(lift))
            .filter(|s| s.begun)
        {
            source.dropping = Some(drop);
        }
    }

    /// Drop `drop` landed, `delivered` or not: a lift's end that waited for
    /// it is heard.
    pub(in crate::presenter) fn lift_landed(&mut self, drop: u32, delivered: bool) {
        let Some(source) = self
            .drag_in
            .lifting
            .iter_mut()
            .find(|s| s.dropping == Some(drop))
        else {
            return;
        };
        source.dropping = None;
        source.delivered = Some(delivered);
        if let Some((taken, held)) = source.ended {
            let lift = source.id;
            self.lift_finished(lift, taken, held);
        }
    }

    /// Lift `lift`'s node hears `dragend`, its record retired first, so a
    /// `dragend` action that lifts again asks for another.
    fn lift_finished(&mut self, lift: u32, taken: bool, held: KeyModifiers) {
        let Some(at) = self.drag_in.lifting.iter().position(|s| s.id == lift) else {
            return;
        };
        let source = self.drag_in.lifting.remove(at);
        if source.renewed {
            return;
        }
        let effect = match source.delivered.unwrap_or(taken) {
            true => DropEffect::Copy,
            false => DropEffect::None,
        };
        self.source_event(source.key, source.offsets, Some(effect), held);
    }

    /// The node `key`, while it is mounted, hears its drag's start, or its
    /// end with what the drag did (`end`).
    fn source_event(
        &mut self,
        key: NodeKey,
        offsets: (f64, f64),
        end: Option<DropEffect>,
        held: KeyModifiers,
    ) {
        let Some(id) = self.host.kernel().node_by_key(key).map(|node| node.id) else {
            return;
        };
        let data = DropEvent {
            offset_x: offsets.0,
            offset_y: offsets.1,
            files: Vec::new(),
            text: String::new(),
            url: String::new(),
            drop_effect: end.unwrap_or_default(),
            held,
        };
        let (kind, event) = match end {
            None => (EventKind::Dragstart, Event::Dragstart(data)),
            Some(_) => (EventKind::Dragend, Event::Dragend(data)),
        };
        if let Some(error) = self.pointer_dispatch(id, kind, event, self.pointer_now()) {
            self.host.log(error);
        }
    }
}
