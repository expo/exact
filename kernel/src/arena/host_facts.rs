//! What the host says of a box: what its container covers of it (LLP
//! 1075.003 §3.5), and the margins the platform's lists give their rows.

use super::NodeArena;

impl NodeArena {
    /// What a host container covers of a box, when it has said.
    pub fn cover(&self, slot: u32) -> Option<crate::kernel::HostCover> {
        self.covers.get(slot)
    }

    pub(crate) fn set_cover(&mut self, slot: u32, cover: Option<crate::kernel::HostCover>) {
        self.covers.set(slot, cover);
    }

    /// The margins the platform's list gives the grouped list `slot`.
    pub(crate) fn grouped_margins(&self, slot: u32) -> Option<crate::grouped::GroupedMargins> {
        self.grouped_margins
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, m)| *m)
    }

    pub(crate) fn has_grouped_margins(&self) -> bool {
        !self.grouped_margins.is_empty()
    }

    /// The margin the platform's cell gives the cells list `slot`'s rows.
    pub(crate) fn list_margins(&self, slot: u32) -> Option<[f32; 2]> {
        self.list_margins
            .iter()
            .find(|(s, _)| *s == slot)
            .map(|(_, m)| *m)
    }

    pub(crate) fn set_list_margins(&mut self, slot: u32, margins: Option<[f32; 2]>) {
        self.list_margins.retain(|(s, _)| *s != slot);
        self.list_margins.extend(margins.map(|m| (slot, m)));
    }

    pub(crate) fn set_grouped_margins(
        &mut self,
        slot: u32,
        margins: Option<crate::grouped::GroupedMargins>,
    ) {
        self.grouped_margins.retain(|(s, _)| *s != slot);
        self.grouped_margins.extend(margins.map(|m| (slot, m)));
    }
}
