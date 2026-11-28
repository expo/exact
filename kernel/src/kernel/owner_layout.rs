//! LLP 1123's stage 5 layout behind one switch, `Kernel::set_owner_layout`: off on main until
//! every host reports its owners with its blocks, on in the tests of each host's wiring.
use super::Kernel;

impl Kernel {
    /// Lays out as LLP 1123's stage 5 does where `on`: an owner's root takes no padding from an
    /// edge cover, since its content root clears its band (D3), and is laid out in its own block,
    /// out of the walk that finds the scroller at a covered box's bottom; and a scroller's bars
    /// lie on its scrollport (D12). Off, main's layout. `var(--exact-safe-area-inset-*)` keeps
    /// main's meaning either way until stage 5 lowers it to the observation leaf. The next layout
    /// applies it.
    pub fn set_owner_layout(&mut self, on: bool) {
        if self.arena.observations.owner_layout == on {
            return;
        }
        self.arena.observations.owner_layout = on;
        self.restyle_scrollport_bars();
        // A content region lays out apart: its accepted publication is the old layout's
        if let Some(region) = &mut self.region {
            region.invalidate();
        }
        // Restyle every box: an owner's cover, and the walks for the scroller at a covered box's
        // bottom, change meaning with it (`cover.rs`)
        let live: Vec<u32> = self.arena.iter_live().collect();
        if let Some(layout) = self.layout.as_deref_mut() {
            super::cover::restyle(&self.arena, layout, &live);
        }
    }

    /// Whether the kernel lays out as stage 5 does ([`Kernel::set_owner_layout`]), which a host
    /// reads to report its owners with its blocks.
    pub fn owner_layout(&self) -> bool {
        self.arena.observations.owner_layout
    }
}

#[cfg(test)]
#[path = "owner_layout_tests.rs"]
mod tests;
