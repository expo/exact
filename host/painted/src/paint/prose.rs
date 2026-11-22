//! An outside marker belongs to its item for clipping, opacity and transforms.

use super::*;

impl Painter {
    pub(super) fn prose_marker(
        &mut self,
        walk: &Walk<'_, '_>,
        item: &NodeRef<'_>,
        rect: Rect4,
        ts: Transform,
    ) {
        if item.props.str(PropId::SemanticTag) != Some("li") {
            return;
        }
        let Some(list) = item.parent.and_then(|id| walk.scene.kernel.node(id)) else {
            return;
        };
        let markers = self.markers.entry(list.id).or_insert_with(|| {
            list.list_markers()
                .into_iter()
                .flatten()
                .filter_map(|(item, text)| text.map(|t| (item.id, t)))
                .collect()
        });
        let Some(label) = markers.get(&item.id) else {
            return;
        };
        let style = item.computed_style(StyleMask::INHERITED);
        let p = self
            .text
            .borrow_mut()
            .paragraph(&text_spec(&style, label), None);
        let gap = style.font_size * 0.25;
        let x = if style.direction == exact_kernel::Direction::Rtl {
            rect.0 + rect.2 + gap
        } else {
            rect.0 - p.width - gap
        };
        let y = item
            .child_nodes()
            .find(|c| {
                c.style.display != Display::None
                    && c.layout_position() != exact_kernel::PositionType::Absolute
            })
            .map_or(rect.1, |child| rect.1 + child.frame.y - item.frame.y);
        let color = presented_color(walk, item, self.dark)
            .unwrap_or_else(|| rgba(item.text_color().resolve(self.dark)));
        // An outside marker is drawn beside the item's box, and its lines
        // where its indent puts them.
        let (lx, ly, lw, lh) = super::text_shadow::line_bounds(&p);
        self.row_reach(super::bbox(ts, (x + lx, y + ly, lw, lh)));
        self.backend.text(
            &mut self.text.borrow_mut(),
            &p,
            &[RunPaint {
                source: item.id,
                color,
            }],
            (x, y),
            ts,
        );
    }
}
