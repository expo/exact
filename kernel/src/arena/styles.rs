//! A node's style, shared with every node that holds an equal one, and the
//! sets the arena keeps of what styles hold.
use super::*;

impl NodeArena {
    /// A test's write in place, copied first when shared. Production writes
    /// go through [`set_style`](Self::set_style), which keeps an unset row
    /// at its initial value, as sharing assumes.
    #[cfg(test)]
    pub(crate) fn style_mut(&mut self, slot: u32) -> &mut StyleProps {
        Rc::make_mut(&mut self.styles[slot as usize])
    }

    /// Replace the slot's style, sharing an equal one's allocation.
    pub(crate) fn set_style(&mut self, slot: u32, style: StyleProps) {
        let style = self.shared.intern(style);
        self.set_shared_style(slot, style);
    }

    /// Patch a slot without copying a style another node already holds.
    pub(crate) fn patch_style(&mut self, slot: u32, patch: &StyleProps) {
        let style = self.shared.patch(&self.styles[slot as usize], patch);
        self.set_shared_style(slot, style);
    }

    /// Clear rows without copying a style another node already holds.
    pub(crate) fn clear_style(&mut self, slot: u32, mask: crate::StyleMask, written: bool) {
        let style = self
            .shared
            .clear(&self.styles[slot as usize], mask, written);
        self.set_shared_style(slot, style);
    }

    fn set_shared_style(&mut self, slot: u32, style: Rc<StyleProps>) {
        self.relative.update(slot, !style.relative.is_empty());
        if crate::fragment::is_multicol(&style) {
            self.frag.containers.insert(slot);
        } else {
            self.frag.containers.remove(slot);
        }
        // With no declaration and no owner's local insets, a move changes no length
        if style.mask.has(crate::StyleId::SafeAreaProperties) {
            self.declaring_slots.insert(slot);
        } else {
            self.declaring_slots.remove(slot);
        }
        self.styles[slot as usize] = style;
        self.refresh_sticky(slot);
    }

    /// Distinct styles the arena holds for its nodes, beyond the initial one.
    pub fn shared_style_count(&self) -> usize {
        self.shared.len()
    }
}
