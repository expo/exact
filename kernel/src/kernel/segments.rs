//! An owner's segments (@ref LLP 1123 D16): its host's split of the owner's
//! own active division regions by the rule that splits the document's (LLP
//! 1078), commonly the document's segments projected into the owner,
//! reported in the owner's coordinates, none where the owner lies on one
//! segment. A box reads its holder's, `--exact-viewport-segment-*`, and a box
//! outside every owner reads the document's.

use super::area::{holder, Holder};
use super::Kernel;
use crate::error::{KernelError, LayoutError};
use crate::style::env::SegmentGrid;
use crate::ViewId;

impl Kernel {
    /// Set the segments of the content owner whose root is `view` (LLP 1123
    /// D2's owner update, D16), or none (`None`) where it lies on one segment.
    /// Its readers of an owner segment are derived again; returns whether any
    /// was (a layout is owed then). Refused: a grid of one segment or past 16
    /// columns or rows, a count of rects that is not `cols × rows`, and a
    /// rect that is nonfinite or of negative size.
    pub fn set_owner_segments(
        &mut self,
        view: ViewId,
        segments: Option<SegmentGrid>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if let Some(why) = segments.as_ref().and_then(SegmentGrid::refusal) {
            return Err(LayoutError::OwnerSegments(why).into());
        }
        let store = &mut self.arena.observations.segments;
        if store.get(&slot).map(|g| &**g) == segments.as_ref() {
            return Ok(false);
        }
        match segments {
            Some(grid) => store.insert(slot, grid.into()),
            None => store.remove(&slot),
        };
        let held = match holder(&self.arena, slot) == Holder::Owner(slot) {
            true => self.segment_readers_through(slot),
            false => Vec::new(),
        };
        let readers = self.with_captures(held);
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// Derive again the readers of an owner segment whose holder is `slot`'s
    /// or `slot`, and each capture's readers below them, where a box's holder
    /// decides what they read (`owner::segments_differ`): after a host starts
    /// or stops placing a top-layer box, which keeps its holder and changes
    /// what that gives them (`owner::segments_held`).
    pub(super) fn segments_moved(&mut self, slot: u32) {
        if !super::owner::segments_differ(&self.arena) {
            return;
        }
        let readers = self.segment_readers_through(slot);
        let readers = self.with_captures(readers);
        self.restyle_readers(&readers);
    }

    /// The readers of an owner segment whose holder is `top`'s, or `top`
    /// itself.
    pub(super) fn segment_readers_through(&self, top: u32) -> Vec<u32> {
        let set = &self.arena.readers.segments;
        let all = set.iter();
        super::targets::held_among(&self.arena, top, set.len(), |s| set.contains(s), all)
    }
}
