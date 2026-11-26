//! @ref LLP 1123 D13 — the painted presenter's executor for a box's reach (`crate::safe_area`): tests set it, and
//! activation (stage 5) sets it from `Kernel::box_reach` after each layout.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// Set or clear (`None`) how far the box `view` reaches past its laid-out border box, top right bottom left in
    /// points: its background, border and children's clip are painted, and its hits taken, over its border box outset
    /// by it, while its layout, its children and what reads its geometry keep the laid-out box. Whether it changed; a
    /// change is painted in full.
    pub fn set_box_reach(&mut self, view: ViewId, reach: Option<[f32; 4]>) -> bool {
        let Some(key) = self.host.kernel().node(view).map(|n| n.key) else {
            return false;
        };
        // Taken out of the host while the host's kernel says which boxes are live
        let mut area = std::mem::take(&mut self.host.safe_area);
        let changed = area.set_reach(self.host.kernel(), key, reach);
        self.host.safe_area = area;
        self.dirty |= changed;
        changed
    }

    /// Set or release (`None`) the scroller `view`'s claim, its reach and its content insets (D13, D14), each top right
    /// bottom left in points: its scrollport and the frame that takes its hits reach, its decoration keeps its box, its
    /// range is its content and insets past the reached scrollport, and its content is painted at the displacement
    /// `ScrollClaim::displacement` gives its CSS offset. The CSS offset stays as it was, clamped to the new range.
    /// Whether it changed; a change is painted in full.
    pub fn set_scroll_claim(&mut self, view: ViewId, claim: Option<([f32; 4], [f32; 4])>) -> bool {
        let Some(key) = self.host.kernel().node(view).map(|n| n.key) else {
            return false;
        };
        let claim = claim.map(|(reach, insets)| crate::safe_area::ScrollClaim { reach, insets });
        let mut area = std::mem::take(&mut self.host.safe_area);
        let changed = area.set_scroll_claim(self.host.kernel(), key, claim);
        self.host.safe_area = area;
        if changed {
            self.dirty = true;
            self.clamp_scroll();
        }
        changed
    }

    /// The scroll offsets the painter places content at: each claimed scroller's displacement of its CSS offset in
    /// `kept`, and `kept`'s for every other; `None` while no scroller is claimed, when they are `kept`.
    pub(crate) fn displaced_scroll(
        &self,
        kept: &BTreeMap<ViewId, (f32, f32)>,
    ) -> Option<BTreeMap<ViewId, (f32, f32)>> {
        let kernel = self.host.kernel();
        let mut claims = self.host.safe_area.scroll_claims().peekable();
        claims.peek()?;
        let mut shown = kept.clone();
        for (id, claim) in claims.filter_map(|(key, c)| Some((kernel.node_by_key(key)?.id, c))) {
            let at = kept.get(&id).copied().unwrap_or_default();
            shown.insert(id, claim.displacement(at));
        }
        Some(shown)
    }

    /// Drop the reach of the views `renewed`, which a commit made new.
    pub(super) fn renew_reach(&mut self, renewed: &[ViewId]) {
        let mut area = std::mem::take(&mut self.host.safe_area);
        area.renewed(self.host.kernel(), renewed);
        self.host.safe_area = area;
    }
}

#[cfg(test)]
#[path = "reach_tests.rs"]
mod tests;
