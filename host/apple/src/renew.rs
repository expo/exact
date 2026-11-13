//! List rows the runner rebound to another item (LLP 1078), on the Apple host.
//!
//! A rebound row keeps its views and kernel nodes; the receipt names every
//! one of them `renewed`, and the kernel's motion forgets each and hears it
//! again as new. What this host keeps by view beyond the mirror goes as a
//! destroy would take it: an inline run's painted colour and opacity, a
//! running flight, an `svg`'s scene and a box's lowered animations (sent
//! again whole, as for a new view). The presenter is told `renew` before the
//! batch's props and style for the view, and resets what it keeps (a press,
//! a hover, a nested scroller's offset, a picture of another source:
//! `Mac/RenewMac.swift`). Presented values stay the engine's: it keeps a
//! renewed node at rest where it was and presents what the new item
//! changes, and presents one it forgot whole. A colour a transition was
//! presenting over the old item's style goes back to the style once the
//! kernel has forgotten the transition ([`Host::repaint_renewed`]).

use super::*;
use exact_kernel::motion::motion_node;

impl<D: DataSource> Host<D> {
    /// A receipt's renewed views, before its creates and updates reach the
    /// batch. Returns those whose paint was moving: their styles show the
    /// moving values until [`Host::repaint_renewed`] restates them.
    pub(super) fn renew_views(
        &mut self,
        receipt: &CommitReceipt,
        batch: &mut Batch,
    ) -> Vec<ViewId> {
        let mut painted = Vec::new();
        for key in &receipt.renewed {
            let Some(id) = self.keys.get(key).copied() else {
                continue;
            };
            // A run's moving colour or opacity is its paragraph's paint: the
            // paragraph is painted again without it.
            let colored = self.paint.runs.remove(&id).is_some();
            if self.paint.opacities.remove(&id).is_some() || colored {
                if let Some((owner, _)) = self.inline_runs.get(&id) {
                    self.dirty_paragraphs.insert(*owner);
                }
            }
            self.svg.renewed(id);
            self.end_flight_of(id);
            if !self.mirror.contains_key(&id) {
                continue;
            }
            if self
                .shown_paint(motion_node(*key))
                .0
                .iter()
                .any(Option::is_some)
            {
                painted.push(id);
            }
            batch.renew(id);
        }
        painted
    }

    /// After the commits' motion: each renewed view whose paint was moving
    /// shows its style's own colours again, unless a transition of the new
    /// item's moves them now.
    pub(super) fn repaint_renewed(&mut self, views: &[ViewId], batch: &mut Batch) {
        for view in views {
            self.present_colors(*view, batch, true);
        }
    }
}
