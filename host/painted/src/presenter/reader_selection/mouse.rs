//! A desktop drag keeps its anchor as the pointer crosses paragraphs. Only
//! paragraphs in the selected interval need glyph geometry.
use super::*;
use crate::paint::inline_frame::{selection::Highlight, PaintedInline};
use crate::presenter::field_pointer::Unit;
use exact_runner::geometry::GeometryAnswer;

#[derive(Clone)]
struct Anchor {
    key: NodeKey,
    value: Rc<str>,
    offsets: Range<u32>,
    unit: Unit,
}

#[derive(Default)]
pub(in crate::presenter) struct MouseSelection {
    anchor: Option<Anchor>,
    pub(in crate::presenter) moved: bool,
    reports: BTreeMap<NodeKey, Report>,
    order: Vec<NodeKey>,
}

impl MouseSelection {
    pub(in crate::presenter) fn anchor_held(&self) -> bool {
        self.anchor.is_some()
    }

    pub(in crate::presenter) fn release(&mut self) {
        self.anchor = None;
        self.moved = false;
    }
}

impl<D: DataSource> Presenter<D> {
    fn reader_paragraph(&self, hit: ViewId) -> Option<ViewId> {
        let id = if self.brush.inline_text.contains_key(&hit) {
            hit
        } else {
            self.inline_block(hit)?
        };
        self.selectable_paragraph(id)?;
        Some(id)
    }

    fn selectable_paragraph(&self, id: ViewId) -> Option<&Rc<PaintedInline>> {
        let p = self.brush.inline_text.get(&id)?;
        let key = p.selection_key()?;
        (self.host.kernel().node(id)?.key == key
            && self.input_live(key)
            && self.display.allows(self.host.kernel(), id))
        .then_some(p)
    }

    fn reader_anchor(&self, hit: NodeKey, x: f32, y: f32, unit: Unit) -> Option<Anchor> {
        let id = self.reader_paragraph(self.host.kernel().node_by_key(hit)?.id)?;
        if self.boxes.iter().find(|b| b.id == id)?.projected() {
            return None;
        }
        let p = &self.brush.inline_text[&id];
        let (key, shaped) = p.selectable()?;
        let character = p.selection_hit(x, y, true)?;
        let parts = self.selection_parts(shaped);
        let part = parts.iter().find(|p| p.range.contains(&character))?;
        let offsets = match unit {
            Unit::Paragraph => {
                let (start, end) = field_pointer::range(&shaped.value, character, unit);
                start..end
            }
            _ if part.atomic.is_some() => part.range.clone(),
            Unit::Word => {
                let (start, end) = field_pointer::range(&shaped.value, character, unit);
                start.max(part.range.start)..end.min(part.range.end)
            }
            _ => {
                let at = p
                    .selection_hit(x, y, false)?
                    .clamp(part.range.start, part.range.end);
                at..at
            }
        };
        Some(Anchor {
            key,
            value: shaped.value.clone(),
            offsets,
            unit,
        })
    }

    pub(in crate::presenter) fn begin_reader_drag(
        &mut self,
        hit: Option<NodeKey>,
        x: f32,
        y: f32,
        clicks: u32,
    ) {
        if crate::PLATFORM != exact_runner::page::Platform::Linux || self.contact_is_direct() {
            return;
        }
        let anchor = hit.and_then(|key| self.reader_anchor(key, x, y, Unit::for_clicks(clicks)));
        self.mouse_selection.anchor = anchor.clone();
        self.mouse_selection.moved = false;
        if let Some(a) = anchor.filter(|a| !a.offsets.is_empty()) {
            self.select_to(&a, a.key, a.offsets.clone());
        } else {
            self.apply_mouse_selection(Default::default(), Vec::new(), Default::default());
        }
    }

    fn nearest_paragraph(&self, x: f32, y: f32) -> Option<ViewId> {
        // Compare painted bounds first. Hitting every shaped paragraph here makes
        // dragging across a short subject line pay for the entire message body.
        self.boxes
            .iter()
            .rev()
            .filter_map(|b| {
                if b.projected() {
                    return None;
                }
                self.selectable_paragraph(b.id)?;
                let (left, top, w, h) = b
                    .clip
                    .map_or(b.rect, |c| crate::paint::intersect(b.rect, c));
                if w <= 0. || h <= 0. {
                    return None;
                }
                let distance = (x - x.clamp(left, left + w)).hypot(y - y.clamp(top, top + h));
                Some((b.id, distance))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1))
            .map(|(id, _)| id)
    }

    pub(in crate::presenter) fn drag_reader(
        &mut self,
        x: f32,
        y: f32,
        previous: (f32, f32),
    ) -> bool {
        let Some(anchor) = self.mouse_selection.anchor.clone() else {
            return false;
        };
        if previous != (x, y) {
            self.mouse_selection.moved = true;
            let target = self
                .hit(x, y)
                .and_then(|hit| self.reader_paragraph(hit))
                .filter(|id| self.boxes.iter().any(|b| b.id == *id && !b.projected()))
                .or_else(|| self.nearest_paragraph(x, y));
            if let Some(p) = target.and_then(|id| self.brush.inline_text.get(&id)) {
                let character = !matches!(anchor.unit, Unit::Character);
                if let (Some((key, shaped)), Some(hit)) =
                    (p.selectable(), p.selection_hit(x, y, character))
                {
                    let (start, end) = field_pointer::range(&shaped.value, hit, anchor.unit);
                    self.select_to(&anchor, key, start..end);
                }
            }
        }
        true
    }

    fn paragraph_ranges(
        &self,
        anchor: &Anchor,
        end: NodeKey,
        target: Range<u32>,
    ) -> Vec<(ViewId, Range<u32>)> {
        let kernel = self.host.kernel();
        let Some(start) = kernel.node_by_key(anchor.key).filter(|n| {
            self.selectable_paragraph(n.id)
                .and_then(|p| p.selectable())
                .is_some_and(|(_, s)| s.value == anchor.value)
        }) else {
            return Vec::new();
        };
        if end == anchor.key {
            return vec![(
                start.id,
                anchor.offsets.start.min(target.start)..anchor.offsets.end.max(target.end),
            )];
        }
        let Some(end) = kernel.node_by_key(end) else {
            return Vec::new();
        };
        let ids = kernel.preorder_where(&kernel.roots(), |ty, _| ty == NodeType::Text);
        let Some(a) = ids.iter().position(|id| *id == start.id) else {
            return Vec::new();
        };
        let Some(b) = ids.iter().position(|id| *id == end.id) else {
            return Vec::new();
        };
        let (first, last, from, to) = if a < b {
            (a, b, anchor.offsets.start, target.end)
        } else {
            (b, a, target.start, anchor.offsets.end)
        };
        let mut ranges: Vec<_> = ids[first..=last]
            .iter()
            .map(|&id| (id, 0..u32::MAX))
            .collect();
        ranges[0].1.start = from;
        ranges[last - first].1.end = to;
        ranges
    }

    fn select_to(&mut self, anchor: &Anchor, end: NodeKey, target: Range<u32>) {
        let mut marks = BTreeMap::new();
        let mut reports = BTreeMap::new();
        let mut order = Vec::new();
        for (id, range) in self.paragraph_ranges(anchor, end, target) {
            let Some(p) = self.selectable_paragraph(id) else {
                continue;
            };
            let (key, shaped) = p.selectable().unwrap();
            if let Some((_, ranges)) = selected_ranges(&self.selection_parts(shaped), range) {
                let (w, h) = self.viewport();
                let clip = self
                    .boxes
                    .iter()
                    .find(|b| b.id == id)
                    .and_then(|b| b.clip)
                    .unwrap_or((0., 0., w, h));
                reports.extend(self.selection_reports(id, shaped, &ranges, |r| {
                    p.selection_marks(r, clip)
                        .into_iter()
                        .map(|(x, y, w, h)| GeometryAnswer {
                            x: x.into(),
                            y: y.into(),
                            width: w.into(),
                            height: h.into(),
                            ..Default::default()
                        })
                        .collect()
                }));
                marks.insert(
                    key,
                    Highlight {
                        value: shaped.value.clone(),
                        ranges,
                    },
                );
                order.push(key);
            }
        }
        self.apply_mouse_selection(marks, order, reports);
    }

    fn apply_mouse_selection(
        &mut self,
        marks: BTreeMap<NodeKey, Highlight>,
        order: Vec<NodeKey>,
        reports: BTreeMap<NodeKey, Report>,
    ) {
        let old = &self.brush.reader_highlights;
        if *old != marks {
            for key in old.keys().chain(marks.keys()) {
                if old.get(key) != marks.get(key) {
                    self.host.row_dirty.node(*key);
                }
            }
            let dirty = self.host.row_dirty.take_changes();
            self.brush.rows_dirty(self.host.kernel(), dirty);
            self.host.flow_damage.repaint();
            self.brush.reader_highlights = marks;
            self.dirty = true;
        }
        self.mouse_selection.order = order;
        let old = std::mem::replace(&mut self.mouse_selection.reports, reports);
        let events = Self::changed_reports(Some(&old), Some(&self.mouse_selection.reports));
        self.report_selection(events);
    }

    pub(in crate::presenter) fn copy_reader_key(&mut self, name: &str) -> bool {
        if self.mouse_selection.order.is_empty()
            || self.clipboard.is_none()
            || !name.eq_ignore_ascii_case("c")
        {
            return false;
        }
        let m = self.modifiers();
        if !m.ctrl
            || m.shift
            || m.alt
            || m.meta
            || self
                .focus
                .and_then(|id| self.host.kernel().node(id))
                .is_some_and(|n| n.node_type == NodeType::TextInput)
        {
            return false;
        }
        self.boxes();
        let mut text = String::new();
        for key in &self.mouse_selection.order {
            let Some((_, shaped)) = self
                .host
                .kernel()
                .node_by_key(*key)
                .and_then(|n| self.selectable_paragraph(n.id))
                .and_then(|p| p.selectable())
            else {
                return false;
            };
            let mark = &self.brush.reader_highlights[key];
            if mark.value != shaped.value {
                return false;
            }
            if !text.is_empty() {
                text.push('\n');
            }
            for range in &mark.ranges {
                text.push_str(
                    &shaped.value[field::byte_at(&shaped.value, range.start)
                        ..field::byte_at(&shaped.value, range.end)],
                );
            }
        }
        if let Err(e) = self.clipboard.as_mut().unwrap().write_text(&text) {
            self.host.log(e);
        }
        true
    }
}
