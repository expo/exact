//! Desktop mouse click counts and release targets.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// A desktop mouse down with the shell's platform click count.
    pub fn pointer_down_with_clicks(
        &mut self,
        x: f32,
        y: f32,
        now_ms: f64,
        clicks: u32,
    ) -> Result<bool, String> {
        let before = self.hit(x, y).and_then(|id| {
            let node = self.host.kernel().node(id)?;
            (node.node_type == NodeType::TextInput)
                .then(|| (node.key, self.field_text(id), self.field_selection(id)))
        });
        let result = self.pointer_down_counted(x, y, now_ms, now_ms, clicks);
        if result.is_ok() && !self.contact_is_direct() {
            let target = self
                .contact
                .as_ref()
                .filter(|c| {
                    c.focus_before_down.is_some()
                        && !c.focus_blocked
                        && c.canvas.is_none()
                        && c.retained.is_none()
                })
                .and_then(|c| c.hit);
            if let Some(key) = target {
                self.begin_field_drag(key, (x, y), clicks, before);
            }
        }
        result
    }
    pub(super) fn mouse_click_target(&mut self, x: f32, y: f32) -> Option<NodeKey> {
        if self.mouse_selection.moved
            || (self.mouse_selection.anchor_held() && self.contact_position() != Some((x, y)))
            || self
                .field_drag
                .as_ref()
                .is_some_and(|drag| drag.moved((x, y)))
        {
            return None;
        }
        let contact = self.contact.as_ref()?;
        if contact.canvas.is_some()
            || contact.retained.is_some()
            || contact.group
            || contact.panning
            || contact.hold.is_some()
        {
            return None;
        }
        let down = contact.hit?;
        let up = self.hit_after_pointerdown(x, y)?;
        if self.brush.region_blocks_action(up) {
            return None;
        }
        let kernel = self.host.kernel();
        // A removed down node cancels the click; there is no surviving-parent
        // fallback. Walk from the original node, including it, toward the root.
        let mut at = Some(kernel.node_by_key(down)?.id);
        while let Some(id) = at {
            if id == up || self.is_inside(up, id) {
                return kernel.node(id).map(|n| n.key);
            }
            at = kernel.node(id)?.parent;
        }
        None
    }
}
