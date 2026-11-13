//! A surface goes when its canvas does, and every surface when the host
//! is replaced.
use super::*;

impl Surfaces {
    /// Destroy canvas `view`'s surface; an owner's record goes with it.
    /// Whether a record went.
    pub(super) fn retire<D: DataSource>(&mut self, view: u32, host: &mut Host<D>) -> bool {
        let Some(c) = self.canvases.remove(&view) else {
            return false;
        };
        let abi = &self.abis[&c.artifact];
        self.edits_admitted |=
            abi.agent(c.id, &json!({"op":"state"}))["world"]["developmentEdits"] == true;
        abi.destroy(c.id);
        self.haptics.gone(view);
        if c.owner {
            self.error = self.error.take().or(host.surface_record(&c.name, None));
            if c.performance {
                self.error = self
                    .error
                    .take()
                    .or(host.canvas_performance_record(&c.name, None));
            }
        }
        c.owner
    }

    /// The host was replaced (a reload, an activated update) and its runner
    /// numbers views from 1 again, so a view id no longer names the canvas
    /// that made its surface. Every surface goes, as the Apple presenter's
    /// `reset` and the web's `gpu.reset` have it; the new tree's
    /// canvases make their own at the next sync.
    pub(crate) fn replaced<D: DataSource>(&mut self, host: &mut Host<D>) {
        self.captures.clear();
        let views: Vec<_> = self.canvases.keys().copied().collect();
        for view in views {
            self.retire(view, host);
        }
        self.pointer = None;
        self.aux_canvas = None;
        #[cfg(target_os = "android")]
        self.windows_replaced();
    }
}
