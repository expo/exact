//! The trial layout's frozen scroll content (PROTO, rule 4: `Kernel::trial_clearance_frames`).

use super::LayoutTree;
use taffy::prelude::NodeId;

impl LayoutTree {
    /// PROTO (rule 4, `Kernel::trial_clearance_frames`): in a trial copy, detaches the content of each scroll
    /// container under `root` that keeps its settled layout when only the block-axis room changes: the container's
    /// block size comes from a length, a percentage or flex growth from a definite basis, it does not distribute
    /// its content along the block axis, and nothing in its content takes a size or an inset from a block-axis
    /// percentage, grows in a column, or is absolutely positioned. Returns their slots.
    pub(crate) fn freeze_scroll_content(&mut self, root: NodeId) -> Vec<u32> {
        use taffy::CompactLength;
        // A length a block-axis room can change: a percentage, or a calc that may hold one
        let definite = |raw: CompactLength| {
            matches!(
                raw.tag(),
                CompactLength::LENGTH_TAG | CompactLength::PERCENT_TAG
            )
        };
        let mut frozen = Vec::new();
        let mut stack = vec![root];
        while let Some(node) = stack.pop() {
            let Ok(style) = self.taffy.style(node) else {
                continue;
            };
            let children = self.taffy.children(node).unwrap_or_default();
            let scrolls = style.overflow.y == taffy::Overflow::Scroll
                && style.display != taffy::Display::None;
            let own_size = definite(style.size.height.into_raw())
                || (style.flex_grow > 0.0 && definite(style.flex_basis.into_raw()));
            let distributes = match style.display {
                taffy::Display::Flex => {
                    matches!(
                        style.flex_direction,
                        taffy::FlexDirection::Column | taffy::FlexDirection::ColumnReverse
                    ) && !(style.justify_content.is_none()
                        || style.justify_content == Some(taffy::JustifyContent::START)
                        || style.justify_content == Some(taffy::JustifyContent::FLEX_START))
                }
                taffy::Display::Grid => true,
                _ => false,
            };
            if scrolls && own_size && !distributes && self.content_is_settled(node) {
                if let Some(&slot) = self.slots.get(&node) {
                    frozen.push(slot);
                }
                self.set_children(node, &[]);
                continue;
            }
            stack.extend(children);
        }
        frozen
    }

    /// PROTO (rule 4): whether nothing under `node` takes its layout from `node`'s block size
    fn content_is_settled(&self, node: NodeId) -> bool {
        use taffy::CompactLength;
        let relative = |raw: CompactLength| {
            matches!(
                raw.tag(),
                CompactLength::PERCENT_TAG | CompactLength::CALC_TAG
            )
        };
        let mut stack = self.taffy.children(node).unwrap_or_default();
        let column = |n: NodeId| {
            self.taffy.style(n).is_ok_and(|s| {
                s.display == taffy::Display::Flex
                    && matches!(
                        s.flex_direction,
                        taffy::FlexDirection::Column | taffy::FlexDirection::ColumnReverse
                    )
            })
        };
        while let Some(n) = stack.pop() {
            let Ok(s) = self.taffy.style(n) else { continue };
            if s.position == taffy::Position::Absolute
                || relative(s.size.height.into_raw())
                || relative(s.min_size.height.into_raw())
                || relative(s.max_size.height.into_raw())
                || relative(s.inset.top.into_raw())
                || relative(s.inset.bottom.into_raw())
                || (s.flex_grow > 0.0 && self.taffy.parent(n).is_some_and(&column))
            {
                return false;
            }
            stack.extend(self.taffy.children(n).unwrap_or_default());
        }
        true
    }
}
