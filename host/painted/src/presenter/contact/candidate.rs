//! A contact's candidates: the gestures a press could begin, innermost
//! first, then by kind on one node (LLP 1057.001 §1), a mouse's lift of a
//! node that drags out of the app last (LLP 1094.000 D13).
use super::*;
use crate::presenter::retained_action::RetainedContact;

#[derive(Clone, Copy)]
pub(in crate::presenter) enum Candidate {
    Arrange(exact_runner::ReorderBinding),
    /// A grip in a list with a `reorderGroup` (LLP 1094 D6).
    Group(exact_runner::ReorderBinding),
    Swipe(NodeKey),
    Pan(NodeKey),
    Transform(TransformDragBinding),
    Height {
        handle: NodeKey,
        target: NodeKey,
    },
    /// A mouse's lift of `source`, armed at the press with a digest of what
    /// it carried (LLP 1094.000 D13).
    Lift {
        source: NodeKey,
        carried: u64,
    },
}
impl Candidate {
    pub(super) fn node(&self) -> NodeKey {
        match *self {
            Candidate::Arrange(b) | Candidate::Group(b) => b.handle,
            Candidate::Transform(b) => b.handle,
            Candidate::Height { handle, .. } => handle,
            Candidate::Swipe(key) | Candidate::Pan(key) => key,
            Candidate::Lift { source, .. } => source,
        }
    }
    /// The candidate's own axis rule, at the slop (LLP 1057.001 §1).
    pub(super) fn accepts(&self, dx: f64, dy: f64) -> bool {
        match self {
            Candidate::Arrange(_) | Candidate::Height { .. } => dy.abs() > dx.abs(),
            Candidate::Swipe(_) => dx.abs() > dy.abs(),
            // A grouped row lifts in any direction (LLP 1094 D6).
            Candidate::Transform(_)
            | Candidate::Pan(_)
            | Candidate::Group(_)
            | Candidate::Lift { .. } => true,
        }
    }
}

impl<D: DataSource> Presenter<D> {
    /// Every candidate of a contact on `hit`: innermost first, then reorder >
    /// transform > height > pan > swipe > a mouse's lift on one node (LLP
    /// 1057.001 §1; LLP 1094.000 D8, D13).
    pub(in crate::presenter) fn candidates(&self, hit: NodeKey) -> Vec<Candidate> {
        let ranked = [
            self.arrange_candidate(hit),
            self.transform_candidate(hit),
            self.height_candidate(hit),
            self.pan_candidate(hit),
            self.swipe_candidate(hit).map(Candidate::Swipe),
            self.mouse_lift(hit)
                .filter(|_| !self.direct())
                .map(|(source, carried)| Candidate::Lift { source, carried }),
        ];
        let mut found: Vec<_> = ranked
            .into_iter()
            .enumerate()
            .filter_map(|(rank, c)| Some((self.depth(hit, c?.node())?, rank, c?)))
            .collect();
        found.sort_by_key(|&(depth, rank, _)| (depth, rank));
        found.into_iter().map(|(_, _, c)| c).collect()
    }
    pub(in crate::presenter) fn depth(&self, hit: NodeKey, candidate: NodeKey) -> Option<usize> {
        let kernel = self.host.kernel();
        let mut at = kernel.node_by_key(hit);
        let mut steps = 0;
        while let Some(node) = at {
            if node.key == candidate {
                return Some(steps);
            }
            at = node.parent.and_then(|id| kernel.node(id));
            steps += 1;
        }
        None
    }
    // @ref LLP 1043.000 §3 D8 — ordinary commits move layout, not a motion hold.
    // Nearest explicit pan handler owns one contact; an editor boundary stops
    // it. A press between keeps the contact only within the slop (the pan
    // that begins takes the press), as a draggable element hears a drag that
    // starts on a button inside it (LLP 1057.001 rule 3; kanban F6).
    pub(super) fn pan_candidate(&self, hit: NodeKey) -> Option<Candidate> {
        let mut at = self.host.kernel().node_by_key(hit).map(|n| n.id);
        while let Some(id) = at {
            let n = self.host.kernel().node(id)?;
            let handlers = self.host.runner().handlers_of(id);
            if self.input_live(n.key) && handlers.contains(&EventKind::Pan) {
                return Some(Candidate::Pan(n.key));
            }
            if n.node_type == NodeType::TextInput {
                return None;
            }
            at = n.parent;
        }
        None
    }

    /// Whether `candidate` may begin with a move of `dx` past the slop: a
    /// mouse's swipe takes only a direction it would start in, its node's
    /// ways or a displaced node's way back to rest, so another falls through
    /// to the candidates after it, a lift's among them (LLP 1094.000 D13).
    pub(super) fn starts_toward(
        &self,
        candidate: &Candidate,
        retained: Option<&RetainedContact>,
        dx: f64,
    ) -> bool {
        let Candidate::Swipe(key) = *candidate else {
            return true;
        };
        if self.direct() {
            return true;
        }
        let ways = match retained {
            Some(retained) => retained.swipe_ways(),
            None => self.swipe_ways(key),
        };
        let view = self.host.kernel().node_by_key(key).map(|n| n.id);
        let shown = view.map_or(0., |v| self.host.presented(v).translate.0 as f64);
        ways.starts(dx, shown)
    }

    /// Lift `contact`'s armed source. The host takes the drag from there and
    /// the contact ends (none); a lift the press no longer arms, or the host
    /// declines, leaves the contact with no candidate, as if none had armed.
    pub(super) fn lift_contact(&mut self, contact: Contact, now_ms: f64) -> Option<Contact> {
        let Some(Candidate::Lift { source, carried }) = contact.candidate else {
            return Some(contact);
        };
        let (hit, origin) = (contact.hit, contact.origin);
        // The lift cancels the contact the presenter holds
        self.contact = Some(contact);
        if self.mouse_lifts(hit, source, carried, origin, now_ms) {
            return None;
        }
        let mut contact = self.contact.take()?;
        contact.candidate = None;
        Some(contact)
    }
}
