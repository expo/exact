//! The HTML user agent margins and list indentation for native layout
//! (HTML §15.3.3 and §15.3.7). Web hosts use the browser's sheet.

use crate::arena::NodeArena;
use crate::{Direction, NodeType, PropId, StyleId};
use taffy::prelude::length;

fn list(tag: &str) -> bool {
    matches!(tag, "ul" | "ol")
}

pub(crate) fn styled(arena: &NodeArena, slot: u32) -> bool {
    matches!(
        (
            arena.node_type(slot),
            arena.props(slot).str(PropId::SemanticTag)
        ),
        (
            NodeType::View,
            Some("ul" | "ol" | "dl" | "dd" | "blockquote")
        ) | (NodeType::Text, Some("p"))
    )
}

pub(crate) fn apply(arena: &NodeArena, slot: u32, s: &mut taffy::style::Style) {
    if !styled(arena, slot) {
        return;
    }
    let tag = arena.props(slot).str(PropId::SemanticTag).unwrap();
    let mask = arena.style(slot).mask;
    let rtl = arena.computed_source(slot, StyleId::Direction).direction == Direction::Rtl;
    if tag == "dd" {
        if rtl && !mask.has(StyleId::MarginRight) {
            s.margin.right = length(40.);
        } else if !rtl && !mask.has(StyleId::MarginLeft) {
            s.margin.left = length(40.);
        }
        return;
    }
    if tag == "blockquote" {
        if !mask.has(StyleId::MarginLeft) {
            s.margin.left = length(40.);
        }
        if !mask.has(StyleId::MarginRight) {
            s.margin.right = length(40.);
        }
    }
    if list(tag) {
        if rtl && !mask.has(StyleId::PaddingRight) {
            s.padding.right = length(40.);
        } else if !rtl && !mask.has(StyleId::PaddingLeft) {
            s.padding.left = length(40.);
        }
        let mut ancestor = arena.parent(slot);
        while let Some(parent) = ancestor {
            if arena
                .props(parent)
                .str(PropId::SemanticTag)
                .is_some_and(list)
            {
                return;
            }
            ancestor = arena.parent(parent);
        }
    }
    let em = arena.computed_source(slot, StyleId::FontSize).font_size;
    if !mask.has(StyleId::MarginTop) {
        s.margin.top = length(em);
    }
    if !mask.has(StyleId::MarginBottom) {
        s.margin.bottom = length(em);
    }
}

impl<'a> crate::NodeRef<'a> {
    /// Outside markers in document order. A hidden item with a layout box
    /// still advances the counter. Removing its box removes its counter too.
    pub fn list_markers(&self) -> Option<impl Iterator<Item = (Self, Option<String>)> + 'a> {
        use crate::ListMarkerType;
        let tag = self.props.str(PropId::SemanticTag)?;
        if !list(tag) {
            return None;
        }
        let marker = if self.style.mask.has(StyleId::ListStyleType) {
            self.style.list_style_type
        } else if tag == "ol" {
            ListMarkerType::Decimal
        } else {
            let mut marker = ListMarkerType::Disc;
            let mut parent = self.arena.parent(self.slot);
            while let Some(slot) = parent {
                if self
                    .arena
                    .props(slot)
                    .str(PropId::SemanticTag)
                    .is_some_and(list)
                {
                    if marker == ListMarkerType::Circle {
                        marker = ListMarkerType::Square;
                        break;
                    }
                    marker = ListMarkerType::Circle;
                }
                parent = self.arena.parent(slot);
            }
            marker
        };
        let mut ordinal = 0;
        Some(self.child_nodes().filter_map(move |item| {
            if item.props.str(PropId::SemanticTag) != Some("li") {
                return None;
            }
            if item.style.mask.has(StyleId::Display) {
                return Some((item, None));
            }
            ordinal += 1;
            let kind = if item.style.mask.has(StyleId::ListStyleType) {
                item.style.list_style_type
            } else {
                marker
            };
            let label = match kind {
                ListMarkerType::Decimal => format!("{ordinal}."),
                ListMarkerType::Disc => "•".into(),
                ListMarkerType::Circle => "◦".into(),
                ListMarkerType::Square => "▪".into(),
                ListMarkerType::None => return Some((item, None)),
            };
            Some((item, Some(label)))
        }))
    }
}
