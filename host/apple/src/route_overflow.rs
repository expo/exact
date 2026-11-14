//! A route's in-flow overflow at the window offer, after first pixel.
use super::*;
use exact_kernel::{BlockAxis, Display, PositionType};

#[derive(Default)]
pub(super) struct Journal {
    pub(super) dirty: bool,
    pub(super) settled: bool,
    pub(super) wake: Option<std::sync::Arc<dyn Fn() + Send + Sync>>,
    requested: bool,
    warned: BTreeSet<NodeKey>,
}

impl<D: DataSource> Host<D> {
    pub(super) fn request_route_overflow(&mut self) {
        if self.data_activated
            && self.route_overflow.dirty
            && self.route_overflow.settled
            && !self.route_overflow.requested
            && self.route_overflow_window().is_some()
        {
            self.route_overflow.requested = true;
            if let Some(wake) = &self.route_overflow.wake {
                wake();
            }
        }
    }

    pub(super) fn journal_route_overflow(&mut self) {
        self.route_overflow.requested = false;
        if !self.data_activated || !self.route_overflow.settled || !self.route_overflow.dirty {
            return;
        }
        let Some(window) = self.route_overflow_window() else {
            return;
        };
        self.route_overflow.dirty = false;
        let kernel = self.runner.kernel();
        self.route_overflow
            .warned
            .retain(|key| kernel.node_by_key(*key).is_some());
        let mut notes = Vec::new();
        for id in self.preorder() {
            let Some(route) = kernel.node(id) else {
                continue;
            };
            let Some(key) = route.props.str(PropId::NavigationKey) else {
                continue;
            };
            if route.style.display == Display::None || self.runner.inactive(id) {
                continue;
            }
            let mut ancestor = route.parent;
            let mut selected = false;
            while let Some(parent) = ancestor.and_then(|p| kernel.node(p)) {
                if let Some(top) = parent.props.str(PropId::NavigationKey) {
                    selected = top == key;
                    break;
                }
                ancestor = parent.parent;
            }
            if !selected {
                continue;
            }
            match kernel.host_block(id) {
                Some(block) if !matches!((block.width, block.height), (BlockAxis::Is(w), BlockAxis::Is(h)) if same_size((w, h), window)) =>
                {
                    continue;
                }
                // UIKit owns every route's block; a newly pushed route has
                // no native allocation until its controller is shown.
                None if cfg!(any(target_os = "ios", target_os = "tvos")) => continue,
                _ => {}
            }
            let bottom = flow_bottom(&route, kernel);
            let (_, _, _, padding) = kernel.layout_padding(route.key).unwrap_or_default();
            let (_, _, _, border) = kernel.resolved_border(route.key).unwrap_or_default();
            let outer = route.frame.y + route.frame.height;
            // Content inside the route's border box moves with the route, so
            // only content past it can be reached by scrolling an ancestor.
            if scrolls(&route)
                || bottom + padding <= outer - border + 0.5
                || (bottom > outer + 0.5 && self.reaches(&route, bottom))
            {
                self.route_overflow.warned.remove(&route.key);
            } else if self.route_overflow.warned.insert(route.key) {
                notes.push(format!(
                    "route-overflow: route {key:?} has in-flow content below its padding edge and nothing scrolls it; put the content in a scroll or list with flex=1 min-height=0, or give its container overflow-y=\"auto\" and a bounded height"
                ));
            }
        }
        for note in notes {
            self.runner.log(note);
        }
    }

    fn route_overflow_window(&self) -> Option<(f32, f32)> {
        // The native view reports the window before keyboard avoidance.
        let window = self.runner.kernel().env().screen?;
        (same_size(self.viewport, window) && self.runner.viewport().keyboard_height == 0.0)
            .then_some(window)
    }

    /// Whether the nearest scroller above the route's visible overflow, an
    /// ancestor's or the document's, scrolls as far as `bottom`. A clip, a
    /// native containing block or the top layer between them ends the
    /// search: these are the boundaries `content_size`, `emit_extents` and
    /// `document_extent` publish extents by.
    fn reaches(&self, route: &NodeRef<'_>, bottom: f32) -> bool {
        let kernel = self.runner.kernel();
        let mut id = route.id;
        while let Some(node) = kernel.node(id) {
            if id != route.id && scrolls(&node) {
                return bottom <= node.frame.y + layout::content_size(&node, kernel).1 + 0.5;
            }
            if kernel.host_block(id).is_some()
                || style::effective_overflow(&node) != (Overflow::Visible, Overflow::Visible)
                || node.props.str(PropId::SemanticTag) == Some("dialog")
                || node.props.str(PropId::Popover).is_some()
            {
                return false;
            }
            match node.parent {
                Some(parent) => id = parent,
                None => {
                    return !self.native_protected_id(id)
                        && bottom <= node.frame.y + layout::document_extent(&node, kernel).1 + 0.5;
                }
            }
        }
        false
    }
}

fn same_size(a: (f32, f32), b: (f32, f32)) -> bool {
    (a.0 - b.0).abs() <= 0.5 && (a.1 - b.1).abs() <= 0.5
}

fn scrolls(node: &NodeRef<'_>) -> bool {
    matches!(
        style::effective_overflow(node).1,
        Overflow::Auto | Overflow::Scroll
    )
}

fn flow_bottom(route: &NodeRef<'_>, kernel: &Kernel) -> f32 {
    let mut bottom = route.frame.y;
    let mut below = route.children();
    while let Some(id) = below.pop() {
        let Some(node) = kernel.node(id) else {
            continue;
        };
        if node.style.display == Display::None
            || node.layout_position() == PositionType::Absolute
            || kernel.host_block(id).is_some()
            || node.node_type.is_metadata()
            || node.node_type.is_svg_element()
        {
            continue;
        }
        // Frames precede transforms. A clipped or scrolling descendant owns
        // its overflow; its in-flow border box still occupies the route.
        bottom = bottom.max(node.frame.y + node.frame.height);
        if style::effective_overflow(&node).1 == Overflow::Visible {
            // A text leaf's anonymous item has no child node. Its layout
            // content includes the lines and the grid item's placement.
            if node.node_type == NodeType::Text && node.style.line_clamp == 0 {
                bottom = bottom.max(node.frame.y + node.content.1);
            }
            below.extend(node.children());
        }
    }
    bottom
}
