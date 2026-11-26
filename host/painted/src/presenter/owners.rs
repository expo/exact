//! @ref LLP 1123 D2, D17 — each route owner's update on the painted hosts, from the shell's own geometry. Activation
//! (stage 5) sends it in place of the routes' covers and local insets; until then only its tests call it.
//!
//! The shell owns every quantity. A route's band is the room its bars take of it and the window's unsafe sides
//! (`bars.rs`); a sheet's route's band is the sheet's drag handle, the system's bar under its foot and the window's
//! sides past its span; with no bars, the band is the window's insets, which are 0 on a desktop shell. The painted host
//! makes no native views and writes to none, and it has no native margins, layout regions, bar layout regions or chrome.
//! So an update carries no contributions, its native targets are unavailable, and those fields are absent. Android
//! reports the keyboard and the segments (A1, A3).
use super::Presenter;
use crate::navigation::{sheet_of, Presented};
use exact_kernel::{Kernel, OwnerUpdate, PropId, ViewId, WindowMap};
use exact_runner::DataSource;
use std::hash::{DefaultHasher, Hash, Hasher};

impl<D: DataSource> Presenter<D> {
    /// The update of each route in the first navigation root's stacks as of the last commit, as the shell lays them
    /// out: its band, its map into the window (a translation to where it lies), and its configuration's identity. A
    /// route that has left its stack has no update.
    pub(crate) fn owner_updates(&self) -> Vec<(ViewId, OwnerUpdate)> {
        let kernel = self.host.kernel();
        let roots = self.host.roots();
        let found = kernel.preorder_where(&roots, |_, props| {
            props.str(PropId::NavigationBack).is_some()
        });
        let Some(root) = found.first().copied() else {
            return Vec::new();
        };
        let env = kernel.arena().env();
        let bars = self.host.bars();
        let split = self.host.navigation.split.active.as_ref();
        let page = self.page_origin();
        let (_, stacks) = crate::navigation::stacks(kernel, root);
        let mut updates = Vec::new();
        for stack in &stacks {
            let kinds: Vec<Presented> = (stack.iter())
                .map(|r| {
                    kernel
                        .node(*r)
                        .map_or(Presented::Pushed, |n| Presented::of(&n))
                })
                .collect();
            for (index, route) in stack.iter().enumerate() {
                let Some(node) = kernel.node(*route) else {
                    continue;
                };
                let sheet = sheet_of(&kinds, index);
                let shown = sheet
                    .and_then(|at| kernel.node(stack[at]))
                    .and_then(|m| self.sheet_origin(m.key));
                let (x, y) = match (shown, split.filter(|s| s.block(*route))) {
                    (Some(at), _) => at,
                    // A detail's frame is local to its pane
                    (None, Some(s)) => (s.pane().0 + node.frame.x, s.pane().1 + node.frame.y),
                    (None, None) => in_page(kernel, *route),
                };
                let band = (bars.bands.get(route).copied())
                    .unwrap_or([env.top, env.right, env.bottom, env.left]);
                let barred = bars.routes.iter().any(|r| r.route == *route);
                let update = OwnerUpdate {
                    band: Some(band),
                    window_map: Some(WindowMap([1.0, 0.0, 0.0, 1.0, page.0 + x, page.1 + y])),
                    configuration: Some(self.configuration(barred, sheet.is_some())),
                    ..Default::default()
                };
                updates.push((*route, update));
            }
        }
        updates
    }

    /// The identity of a route's configuration, which an `observed-max` reader keeps its largest band for (D9): the
    /// window's size, where the page lies in it and the page's size, the room the shell's bars take, whether the route
    /// has a top bar and the navigation bar shows, and whether it is in a sheet.
    fn configuration(&self, barred: bool, sheeted: bool) -> u64 {
        let mut h = DefaultHasher::new();
        let window = self.window().map_or((0.0, 0.0), |w| w.size);
        let (above, below, foot) = self.host.bars().room.unwrap_or_default();
        let page = self.page_origin();
        for v in [
            window.0,
            window.1,
            page.0,
            page.1,
            self.viewport.0,
            self.viewport.1,
            above,
            below,
            foot,
        ] {
            v.to_bits().hash(&mut h);
        }
        (barred, self.host.bars().tabs.is_some(), sheeted).hash(&mut h);
        h.finish()
    }
}

/// Where the box `view` lies in the page, its frame's origin and its ancestors', which no scroller holds for a route.
fn in_page(kernel: &Kernel, view: ViewId) -> (f32, f32) {
    let arena = kernel.arena();
    let mut at = arena.slot_of(view);
    let (mut x, mut y) = (0.0, 0.0);
    while let Some(slot) = at {
        let f = arena.frame(slot);
        (x, y) = (x + f.x, y + f.y);
        at = arena.parent(slot);
    }
    (x, y)
}

#[cfg(test)]
#[path = "owners_tests.rs"]
mod tests;
