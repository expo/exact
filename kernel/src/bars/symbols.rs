//! The neutral bar's symbol boxes, shared by layout and painted hosts (LLP 1115.000 D4.15).
use super::*;
use crate::arena::NodeArena;
use crate::style::metrics::{self, Column};
use crate::style::relative::Unit;

impl Kernel {
    /// Resolve the symbols that occupy an entire bar item's face. Whether a
    /// route's header is a bar is `header_bar`'s, from what hears `press`, as
    /// the web marks `data-exact-bar`; its items are what the neutral sheet
    /// selects there, buttons without handlers too (`sheet_items`). Call
    /// before layout, after changes to the tree, listeners or root font size.
    pub fn project_bar_symbols(&mut self, pressed: &dyn Fn(ViewId) -> bool) {
        let mut wanted = Vec::new();
        if metrics::column() == Column::Neutral && self.has_prop(PropId::NavigationKey) {
            let routes = self.preorder_where(&self.roots(), |_, props| {
                props.str(PropId::NavigationKey).is_some()
            });
            let tree = Live {
                kernel: self,
                back: None,
                pressed,
            };
            let button = |id| {
                pressed(id)
                    || self.node(id).is_some_and(|node| {
                        let slot = node.key.index;
                        self.arena().is_button(slot) || self.arena().is_native_button(slot)
                    })
            };
            for route in routes {
                let Some(shape) = header_bar(&tree, route, false) else {
                    continue;
                };
                if site_header(&tree, shape.header) {
                    continue;
                }
                // The sheet selects nothing under a layer, a readout or an
                // `appearance: none` its author wrote, wherever it stands.
                let mut ancestor = self.node(shape.header);
                let mut excluded = false;
                while let Some(at) = ancestor {
                    excluded |=
                        tree.facts(at.id) & (fact::AUTHORED | fact::LAYER | fact::STATUS) != 0;
                    ancestor = at.parent.and_then(|p| self.node(p));
                }
                if excluded {
                    continue;
                }
                let mut items = Vec::new();
                sheet_items(&tree, &button, shape.header, &mut items);
                for item in &items {
                    let Some(node) = self.node(*item) else {
                        continue;
                    };
                    let native = crate::ControlKind::of(node.node_type, node.props)
                        == Some(crate::ControlKind::Button);
                    let children = node.children();
                    let symbol = |id| {
                        self.node(id)
                            .filter(|n| n.node_type == NodeType::Image)
                            .and_then(|n| n.props.str(PropId::ImageSource))
                            .and_then(|s| s.strip_prefix("symbol:"))
                            .filter(|s| {
                                !crate::Icon::parse(s).is_some_and(|icon| {
                                    icon.path.is_empty() && !icon.svg.is_empty()
                                })
                            })
                    };
                    if children.iter().any(|&id| {
                        symbol(id).is_none()
                            && !(native
                                && self.node(id).is_some_and(|n| {
                                    n.node_type == NodeType::Text
                                        && n.text_runs().iter().all(|run| run.text.is_empty())
                                }))
                    }) {
                        continue;
                    }
                    for child in children {
                        if let Some(role) = symbol(child) {
                            let size = self.arena().bar_symbol_metric(role);
                            wanted.push((self.node(child).unwrap().key.index, size));
                        }
                    }
                }
            }
        }
        self.replace_bar_symbols(wanted);
    }
}

/// The pressables under `node` in a bar's header that the neutral sheet
/// takes as items (neutral.css, LLP 1115.000 D4.15): every `button` and
/// every node that hears `press`, except what is in a popover or dialog, a
/// tablist, a readout, another item or what its author wrote
/// `appearance: none` on. A button around a heading is not an item: what
/// it holds is, as the title's pressable's is in `Walk::walk`.
fn sheet_items(tree: &Live, button: &dyn Fn(ViewId) -> bool, node: ViewId, out: &mut Vec<ViewId>) {
    for child in tree.children(node) {
        let facts = tree.facts(child);
        if facts & (fact::AUTHORED | fact::LAYER | fact::TABLIST) != 0 {
            continue;
        }
        if button(child) && !holds_heading(tree, child) {
            out.push(child);
        } else if button(child) || facts & fact::STATUS == 0 {
            sheet_items(tree, button, child, out);
        }
    }
}

impl NodeRef<'_> {
    /// The neutral bar's square for this symbol, over its authored size;
    /// `None` when it keeps its ordinary image sizing.
    pub fn bar_symbol_size(&self) -> Option<f32> {
        self.arena.bar_symbol_size(self.slot)
    }
}

impl NodeArena {
    pub(crate) fn bar_symbol_size(&self, slot: u32) -> Option<f32> {
        self.bar_symbols
            .binary_search_by_key(&slot, |(s, _)| *s)
            .ok()
            .map(|i| self.bar_symbols[i].1)
    }

    pub(crate) fn bar_symbol_metric(&self, role: &str) -> f32 {
        use crate::generated::metric;
        let scale =
            if crate::apple_symbol_name(role).is_empty() || crate::symbol_path(role).is_none() {
                1.0
            } else {
                metrics::number_in(metric::SYMBOL_SCALE, Column::Neutral).expect("symbol scale")
            };
        self.bar_symbol_font_size() * scale
    }

    fn bar_symbol_font_size(&self) -> f32 {
        let root = self.root_font_size();
        metrics::length_in(
            crate::generated::metric::BAR_SYMBOL_SIZE,
            Column::Neutral,
            root,
            root,
        )
        .expect("neutral bar symbol size")
    }

    pub(crate) fn bar_symbol_style(
        &self,
        slot: u32,
        style: &mut std::borrow::Cow<'_, crate::StyleProps>,
    ) {
        if self.bar_symbol_size(slot).is_none() {
            return;
        }
        // The sheet's font override also owns the image's em geometry.
        // Stored rows retain their authored font's lengths for when the face changes.
        let font = self.bar_symbol_font_size();
        for (id, unit, n) in self.style(slot).relative.iter() {
            if unit == Unit::Em && id != StyleId::FontSize {
                style.to_mut().set_resolved(id, n * font);
            }
        }
    }

    pub(crate) fn bar_symbol_layout(&self, slot: u32, style: &mut taffy::Style) {
        if let Some(size) = self.bar_symbol_size(slot) {
            style.size = taffy::Size {
                width: taffy::style::Dimension::length(size),
                height: taffy::style::Dimension::length(size),
            };
        }
    }
}
