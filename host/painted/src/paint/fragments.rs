//! @ref LLP 1093 D7, D8, D12 — this host's part in multi-column layout. The
//! kernel publishes every box in a flow at its column's place and, for a box
//! that straddles columns, its union frame and fragments. A paragraph is laid
//! out once, at its unfragmented width, and painted in each fragment at the
//! fragment's offset, clipped to it, so the lines it shows are the ones the
//! kernel cut there. Column rules are rectangles centred in the gaps. A hit
//! or a tap sees fragments, never a union's gap.

use super::*;

impl Painter {
    /// A `Text` node: its runs' backgrounds, decorations and glyphs, once,
    /// or once in each of its fragments.
    /// `own`: the box's overflow clip, in its coordinates; `ancestors`: the
    /// clip its ancestors put on it, in the walk's.
    pub(super) fn text_node(
        &mut self,
        walk: &mut Walk<'_, '_>,
        node: &NodeRef<'_>,
        box_paint: &BoxPaint,
        geometry: &BoxGeometry,
        (rect, origin): (Rect4, (f32, f32)),
        (own, ancestors): (Option<Rect4>, Option<Rect4>),
        ts: Transform,
    ) -> Option<(f32, f32)> {
        // Two texts on one baseline keep it when their boxes snap.
        let mut content = geometry.content;
        content.0 = origin.0 + geometry.inset.0;
        content.1 = origin.1 + geometry.inset.1;
        let s = node.style;
        let kernel = walk.scene.kernel;
        let frags = kernel.fragments(node.key).unwrap_or(&[]);
        // The width its lines broke at: a fragmented box's own, not its
        // union's; a multi-column `text`'s column width. Layout's, where the
        // box drawn on whole pixels is a fraction of one narrower or wider.
        let laid = box_paint.content_width(node.frame.width);
        if let Some(item) = kernel.text_item_box(node) {
            content.0 += item.x;
        }
        let width = kernel
            .text_item_box(node)
            .map(|item| item.width)
            .unwrap_or_else(|| match frags.first() {
                Some(g) if kernel.columns(node.key).is_some() => g.width,
                Some(g) => g.width - (node.frame.width - laid),
                None => laid,
            });
        // The kernel measures a Text subtree as one paragraph. Inline
        // descendants deliberately have zero frames, not paint boxes.
        let build = || {
            let computed = node.computed_style(StyleMask::INHERITED);
            let mut spec = text_spec(&computed, "");
            // A fitted paragraph's line box is at its runs' scale (LLP 1116.001 D1.4).
            let strut = exact_kernel::TextStyle::from_style(&computed).scaled(node.fit_scale());
            spec.strut = Run::from_style("", strut);
            spec.shown(
                &node.text_runs(),
                exact_kernel::text::Markup::from_prop(node.props.str(PropId::Markup)),
            )
        };
        let paragraph = if let Some(stamp) = node.paragraph_stamp() {
            // @ref LLP 1043.000 §3 D7 — ordinary text takes the same
            // retained identity path; flow only adds exclusion geometry.
            self.text.borrow_mut().flow_identified(
                &stamp,
                width,
                &node
                    .flow_shapes()
                    .iter()
                    .map(|s| s.translate(-geometry.inset.0, -geometry.inset.1))
                    .collect::<Vec<_>>(),
                self.accepted_text.get(&node.key),
                build,
            )
        } else {
            let spec = build();
            (!spec.is_empty()).then(|| self.text.borrow_mut().paragraph(&spec, Some(width)))
        };
        let paragraph = paragraph?;
        let mut palette = Vec::new();
        text_palette(kernel, node, self.dark, walk.reveal, &mut palette);
        presented_text_colors(walk, node, &paragraph, &mut palette, self.dark);
        let shown_before = self.accepted_text.get(&node.key);
        let roll = (self.numerals)
            .roll(node.key, s.content_transition, shown_before, &paragraph)
            .filter(|_| frags.is_empty());
        if roll.is_some() {
            // Drawn anew each frame it rolls, never kept in a row.
            self.row_refuse();
        }
        walk.text.insert(node.key, paragraph.clone());
        // The canvas recorder leases the same paragraph the paint drew.
        self.row_text(node.key, &paragraph);
        // CSS `text-overflow: ellipsis` in a clipping box: an over-wide line
        // ends in "…" (LLP 1053 G5; paint only).
        let (ox, oy) = effective_overflow(node);
        let shown = (s.text_overflow == exact_kernel::TextOverflow::Ellipsis
            && ox != Overflow::Visible)
            .then(|| paragraph.ellipsized(width))
            .flatten()
            .unwrap_or_else(|| paragraph.clone());
        // An ellipsized line changes in place.
        let roll = roll.filter(|_| Rc::ptr_eq(&shown, &paragraph));
        let backgrounds = text_backgrounds(kernel, node, self.dark, walk.reveal);
        let groups = inline_opacity::Groups::capture(kernel, node.id, &palette, |id| {
            (walk.scene.presented)(id).opacity
        });
        let focus = walk
            .scene
            .focus
            .filter(|id| self.ringless != Some(*id) && !look::material())
            .and_then(|id| kernel.node(id))
            .filter(|n| {
                n.node_type == NodeType::Text
                    && (n.is_inline_run() || n.props.str(PropId::Href).is_some())
                    && n.props.bool(PropId::Disabled) != Some(true)
                    && !control::ringless(n)
            });
        let focus = focus.map(|focused| (focused.id, control::ring(&focused, self.dark)));
        let can_overflow = frags.is_empty() && (ox == Overflow::Visible || oy == Overflow::Visible);
        let mut baseline = None;
        let mut at = |painter: &mut Self, shown: &Paragraph, origin: (f32, f32)| {
            if can_overflow {
                painter.row_text_reach(shown, origin, geometry.content, ts);
            }
            if let Some((roll, _)) = &roll {
                // The outgoing number rolls out past the new one's lines.
                let (x, y, w, h) = roll.bounds();
                painter.row_reach(super::bbox(ts, (x + origin.0, y + origin.1, w, h)));
            }
            if baseline.is_none() {
                baseline = shown.baseline_at(origin);
            }
            groups.visit(&mut |paint| match paint {
                inline_opacity::Paint::Begin(index, opacity) => {
                    if opacity < 1. {
                        painter.backend.push_opacity(opacity);
                    }
                    // A background belongs to its declaring box's group,
                    // including the parts under a more transparent child.
                    let backgrounds: Vec<_> = backgrounds
                        .iter()
                        .map(|b| {
                            let b = b.as_ref()?;
                            let mut layers = std::iter::once(b)
                                .chain(b.outer.iter())
                                .filter(|b| groups.owner(kernel, b.id) == index);
                            let mut b = layers.next()?.clone();
                            b.outer = layers.cloned().collect();
                            Some(b)
                        })
                        .collect();
                    for paint in shown.inline_backgrounds(&backgrounds, &kernel.env_of(node.id)) {
                        // Vertical padding paints past the line box.
                        let (x, y, w, h) = paint.shape().rect;
                        painter.row_reach(super::bbox(ts, (x + origin.0, y + origin.1, w, h)));
                        inline::paint_inline(&mut *painter.backend, &paint, origin, ts);
                    }
                }
                inline_opacity::Paint::Text(range) => painter.text_paint(
                    node,
                    kernel,
                    shown,
                    &palette,
                    origin,
                    rect,
                    ts,
                    walk.reveal,
                    roll.as_ref(),
                    range,
                ),
                inline_opacity::Paint::End(index, opacity) => {
                    if let Some((focused, accent)) = &focus {
                        if groups.owner(kernel, *focused) == index {
                            for mut shape in
                                inline::focus_fragments(kernel, shown, &palette, *focused)
                            {
                                shape.rect.0 += origin.0;
                                shape.rect.1 += origin.1;
                                painter.field_ring(&shape, *accent, ts);
                            }
                        }
                    }
                    if opacity < 1. {
                        painter.backend.pop_opacity();
                    }
                }
            });
        };
        if frags.is_empty() {
            // The kernel places the anonymous text item.
            at(
                self,
                &shown,
                (content.0, content.1 + kernel.text_line_offset(node)),
            );
        }
        for g in frags {
            if g.lines.0 == g.lines.1 {
                continue;
            }
            let shown = shown.line_fragment(g.lines.0 as usize..g.lines.1 as usize);
            let column = Shape::rect((rect.0 + g.x, rect.1 + g.y, g.width, g.height));
            // A column past the box's own is drawn outside it, as far as
            // the clips around its lines let it. The box's own is cut before
            // it is transformed: it reaches a million points on a visible
            // axis, which a rotation would turn onto both.
            let kept = own.map_or(column.rect, |o| super::intersect(column.rect, o));
            let drawn = super::bbox(ts, kept);
            let drawn = ancestors.map_or(drawn, |c| super::intersect(drawn, c));
            if kept.2 > 0. && kept.3 > 0. && drawn.2 > 0. && drawn.3 > 0. {
                self.row_reach(drawn);
            }
            self.backend.push_clip(&column, ts);
            at(self, &shown, (content.0 + g.dx, content.1 + g.dy));
            self.backend.pop_clip();
        }
        if let Some(inline) = inline_frame::Captured::new(kernel, node, &paragraph, &palette) {
            let origin = (
                content.0,
                content.1
                    + if frags.is_empty() {
                        kernel.text_line_offset(node)
                    } else {
                        0.
                    },
            );
            let inline = inline.place(origin, frags, ts);
            self.highlight_reader(&inline);
            self.row_inline(node.id, &inline);
            walk.inline.insert(node.id, inline);
        }
        baseline
    }

    /// A multi-column container's `column-rule`: centred in each gap between
    /// two columns that both hold a box, as tall as the columns, under the
    /// columns' contents (D7). A rule wider than its gap moves no column.
    pub(super) fn column_rules(
        &mut self,
        kernel: &Kernel,
        node: &NodeRef<'_>,
        rect: Rect4,
        ts: Transform,
    ) {
        let s = node.style;
        let width = s.column_rule_width;
        if s.column_rule_style != exact_kernel::ColumnRuleStyle::Solid || width <= 0.0 {
            return;
        }
        let Some(columns) = kernel.columns(node.key) else {
            return;
        };
        let current = node
            .computed_style(StyleMask::of(StyleId::TextColor))
            .text_color;
        let color = rgba(s.column_rule_color.unwrap_or(current).resolve(self.dark));
        for pair in columns.columns.windows(2) {
            let [a, b] = [pair[0], pair[1]];
            if !(a.holds && b.holds) {
                continue;
            }
            // The gap's middle, whichever side the next column is on.
            let (left, right) = if b.x >= a.x {
                (a.x + a.width, b.x)
            } else {
                (b.x + b.width, a.x)
            };
            let x = rect.0 + (left + right) / 2.0 - width / 2.0;
            let shape = Shape::rect((x, rect.1 + a.y, width, a.height.max(b.height)));
            // A rule wider than its gap paints past the box.
            self.row_reach(super::bbox(ts, shape.rect));
            self.backend.fill(&shape, color, ts);
        }
    }
}

/// Whether a point (viewport points) inside a painted box hits it: a box
/// with fragments only inside one, never in its union's gap (D8).
pub(crate) fn hits(kernel: &Kernel, b: &PaintedBox, x: f32, y: f32) -> bool {
    let b = b.for_hit();
    let Some(node) = kernel.node(b.id) else {
        return true;
    };
    kernel
        .fragments(node.key)
        .is_none_or(|frags| frags.iter().any(|g| g.contains(x - b.rect.0, y - b.rect.1)))
}

/// Whether a point outside a painted box is in its hit slop
/// (`StyleProps::hit_slop`, Charlie, 2026-11-10): the box's own rectangle
/// grown in its own space, under the clips it was painted with. A box placed
/// by a perspective takes none.
pub(crate) fn slop_hits(kernel: &Kernel, b: &PaintedBox, x: f32, y: f32) -> bool {
    slopped(kernel, b).is_some_and(|b| b.contains(x, y))
}

/// Where `b`'s hit slop takes hits, before its clips; none without slop.
pub(crate) fn slop_bounds(kernel: &Kernel, b: &PaintedBox) -> Option<Rect4> {
    slopped(kernel, b).map(|b| b.rect)
}

/// `b` grown by its hit slop, for a hit test.
fn slopped(kernel: &Kernel, b: &PaintedBox) -> Option<PaintedBox> {
    let node = kernel.node(b.id)?;
    if !node.style.may_have_hit_slop() {
        return None;
    }
    let [t, r, bottom, l] = node.style.hit_slop(&kernel.env_of(node.id));
    if t + r + bottom + l <= 0.0 {
        return None;
    }
    let mut b = b.for_hit();
    if b.projective.is_some() {
        return None;
    }
    let grow = |q: Rect4| (q.0 - l, q.1 - t, q.2 + l + r, q.3 + t + bottom);
    match b.affine {
        Some((ts, local)) => {
            b.affine = Some((ts, grow(local)));
            b.rect = bbox(ts, grow(local));
        }
        None => b.rect = grow(b.rect),
    }
    Some(b)
}

/// Where the agent's `tap` aims on a box with fragments: its first
/// fragment's centre, not the union's, which may be a gap (D12).
pub(crate) fn center(kernel: &Kernel, b: &PaintedBox) -> Option<(f32, f32)> {
    let g = kernel.fragments(kernel.node(b.id)?.key)?.first()?;
    Some((
        b.rect.0 + g.x + g.width / 2.0,
        b.rect.1 + g.y + g.height / 2.0,
    ))
}
