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
