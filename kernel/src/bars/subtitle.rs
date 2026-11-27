//! Supporting subtitle extraction for a header-shaped route (LLP 1115.001 D11, #1977).
//!
//! Plain-text extraction in the heading's title group (`shape.tap`, else the
//! outermost ancestor below `shape.header` holding no bar part, else
//! `shape.header`). Mirrors Apple's `HeaderTitle` title-group walk for plain
//! text with these explicit rules:
//! - skips `display: none`, popovers/dialogs, bar items/readouts/search/tablist,
//!   and independent buttons or links;
//! - skips filled boxes (non-transparent `background-color` in light or dark);
//! - skips passive glyph-line boxes (direct children are only passive `symbol:`
//!   images and texts, with at least one visible symbol and one visible
//!   non-blank text) rather than extracting only their first text fragment;
//! - checks computed `Visibility::Visible` on the candidate `Text` node itself
//!   so a `visibility: visible` child inside a `visibility: hidden` wrapper
//!   still qualifies.

use super::{HeaderShape, NodeType, PropId, StyleId};
use crate::generated::Display;
use crate::id::ViewId;
use crate::kernel::{Kernel, NodeRef};
use crate::ControlKind;

fn within(kernel: &Kernel, node: ViewId, ancestor: ViewId) -> bool {
    let mut at = Some(node);
    while let Some(id) = at {
        if id == ancestor {
            return true;
        }
        at = kernel.node(id).and_then(|n| n.parent);
    }
    false
}

fn is_layer(node: &NodeRef<'_>) -> bool {
    node.props.str(PropId::Popover).is_some()
        || node.props.str(PropId::SemanticTag) == Some("dialog")
}

fn is_control(kernel: &Kernel, node: &NodeRef<'_>, heading: ViewId) -> bool {
    if within(kernel, heading, node.id) {
        return false;
    }
    let role = node.props.str(PropId::AccessibilityRole);
    node.node_type == NodeType::Pressable
        || ControlKind::of(node.node_type, node.props) == Some(ControlKind::Button)
        || matches!(role, Some("button" | "link"))
        || node.props.link().is_some()
}

fn is_filled_box(node: &NodeRef<'_>) -> bool {
    node.node_type == NodeType::View
        && node
            .style
            .background_color
            .is_some_and(|bg| bg.resolve(false).a() != 0 || bg.resolve(true).a() != 0)
}

fn is_visible(node: &NodeRef<'_>) -> bool {
    node.computed_row(StyleId::Visibility, |s| s.visibility) == crate::Visibility::Visible
}

/// Whether `node` is a passive glyph-line box matching Apple's `HeaderTitle.line`:
/// its direct shown non-`apart` children are all passive `symbol:` images and
/// texts, with at least one visible symbol and at least one visible non-blank text.
fn is_glyph_line(kernel: &Kernel, node: ViewId, heading: ViewId, apart: &[ViewId]) -> bool {
    let Some(parent) = kernel.node(node) else {
        return false;
    };
    if parent.node_type == NodeType::Text
        || is_filled_box(&parent)
        || is_control(kernel, &parent, heading)
    {
        return false;
    }
    let mut has_symbol = false;
    let mut has_text = false;
    for child_id in parent.children() {
        let Some(child) = kernel.node(child_id) else {
            continue;
        };
        if child.style.display == Display::None || is_layer(&child) || apart.contains(&child_id) {
            continue;
        }
        if is_filled_box(&child) || is_control(kernel, &child, heading) {
            return false;
        }
        if child.node_type == NodeType::Image
            && child
                .props
                .str(PropId::ImageSource)
                .is_some_and(|s| s.starts_with("symbol:"))
        {
            if is_visible(&child) {
                has_symbol = true;
            }
        } else if child.node_type == NodeType::Text {
            if is_visible(&child) && kernel.bar_text(child_id).is_some() {
                has_text = true;
            }
        } else {
            return false;
        }
    }
    has_symbol && has_text
}

impl Kernel {
    /// The plain-text supporting subtitle of a header-shaped route
    /// (LLP 1115.001 D11, #1977): the first shown, visible, non-blank `text`
    /// after `shape.heading` inside its title group. Returns `None` when the
    /// heading is absent or blank, or when no eligible plain text follows it.
    pub fn header_subtitle(&self, shape: &HeaderShape<ViewId>) -> Option<ViewId> {
        let heading = shape.heading?;
        self.bar_text(heading)?;

        let mut apart =
            Vec::with_capacity(shape.leading.len() + shape.trailing.len() + shape.items.len() + 2);
        apart.extend_from_slice(&shape.leading);
        apart.extend_from_slice(&shape.trailing);
        apart.extend_from_slice(&shape.items);
        if let Some(search) = shape.search {
            apart.push(search);
        }
        if let Some(segments) = shape.segments {
            apart.push(segments);
        }

        let group = shape.tap.unwrap_or_else(|| {
            let mut chain = Vec::new();
            let mut at = self.node(heading).and_then(|n| n.parent);
            while let Some(id) = at {
                if id == shape.header {
                    break;
                }
                chain.push(id);
                at = self.node(id).and_then(|n| n.parent);
            }
            chain
                .into_iter()
                .rev()
                .find(|&ancestor| !apart.iter().any(|&p| within(self, p, ancestor)))
                .unwrap_or(shape.header)
        });

        fn walk(
            kernel: &Kernel,
            node: ViewId,
            heading: ViewId,
            apart: &[ViewId],
            passed: &mut bool,
        ) -> Option<ViewId> {
            for child_id in kernel.node(node)?.children() {
                let Some(child) = kernel.node(child_id) else {
                    continue;
                };
                if child.style.display == Display::None || is_layer(&child) {
                    continue;
                }
                if child_id == heading {
                    *passed = true;
                    continue;
                }
                if apart.contains(&child_id) || is_control(kernel, &child, heading) {
                    continue;
                }
                if *passed {
                    if child.node_type == NodeType::Text {
                        if is_visible(&child) && kernel.bar_text(child_id).is_some() {
                            return Some(child_id);
                        }
                        continue;
                    }
                    if matches!(child.node_type, NodeType::Image | NodeType::Svg)
                        || is_filled_box(&child)
                        || is_glyph_line(kernel, child_id, heading, apart)
                    {
                        continue;
                    }
                    if let Some(found) = walk(kernel, child_id, heading, apart, passed) {
                        return Some(found);
                    }
                    continue;
                }
                if within(kernel, heading, child_id) {
                    if let Some(found) = walk(kernel, child_id, heading, apart, passed) {
                        return Some(found);
                    }
                }
            }
            None
        }

        let mut passed = false;
        walk(self, group, heading, &apart, &mut passed)
    }
}
