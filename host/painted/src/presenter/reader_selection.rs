//! Reader selection uses UTF-16 offsets in the painted paragraph.
use super::*;
use crate::paint::inline_frame::selection::Shaped;
use exact_kernel::{NodeKey, UserSelect};
use std::ops::Range;
use std::rc::Rc;

mod mouse;
pub(super) use mouse::MouseSelection;

#[cfg(any(target_os = "android", test))]
pub(super) struct Selection {
    key: NodeKey,
    value: Rc<str>,
    range: Range<u32>,
    reports: BTreeMap<NodeKey, Report>,
}

#[derive(Default, PartialEq)]
struct Report {
    range: Range<u32>,
    text: String,
    rects: Vec<exact_runner::geometry::GeometryAnswer>,
}

struct Part {
    range: Range<u32>,
    atomic: Option<ViewId>,
}

impl<D: DataSource> Presenter<D> {
    fn selection_policy(&self, id: ViewId) -> Option<Option<ViewId>> {
        let kernel = self.host.kernel();
        let mut enabled = None;
        let mut atomic = None;
        let mut at = Some(id);
        while let Some(n) = at.and_then(|id| kernel.node(id)) {
            match n.style.user_select {
                UserSelect::All => {
                    atomic = Some(n.id);
                    enabled.get_or_insert(true);
                }
                UserSelect::Text | UserSelect::Contain => {
                    enabled.get_or_insert(true);
                }
                UserSelect::None => {
                    enabled.get_or_insert(false);
                }
                UserSelect::Auto => {}
            }
            if enabled.is_none()
                && (matches!(n.node_type, NodeType::Control | NodeType::TextInput)
                    || self
                        .host
                        .runner()
                        .handlers_of(n.id)
                        .contains(&EventKind::Press)
                    || n.props.str(PropId::Href).is_some()
                    || matches!(
                        n.props.str(PropId::AccessibilityRole),
                        Some(
                            "button"
                                | "link"
                                | "tab"
                                | "menuitem"
                                | "menuitemcheckbox"
                                | "menuitemradio"
                                | "option"
                                | "checkbox"
                                | "radio"
                                | "switch"
                                | "treeitem"
                                | "slider"
                                | "spinbutton"
                        )
                    ))
            {
                return None;
            }
            at = n.parent;
        }
        (enabled == Some(true)).then_some(atomic)
    }

    fn selection_parts(&self, shaped: &Shaped) -> Vec<Part> {
        let mut parts = Vec::<Part>::new();
        for (range, id) in &shaped.runs {
            let Some(atomic) = self.selection_policy(*id) else {
                continue;
            };
            if let Some(last) = parts
                .last_mut()
                .filter(|p| p.atomic == atomic && p.range.end == range.start)
            {
                last.range.end = range.end;
            } else {
                parts.push(Part {
                    range: range.clone(),
                    atomic,
                });
            }
        }
        parts
    }

    fn selection_reports(
        &self,
        paragraph: ViewId,
        shaped: &Shaped,
        selected: &[Range<u32>],
        mut marks: impl FnMut(Range<u32>) -> Vec<exact_runner::geometry::GeometryAnswer>,
    ) -> BTreeMap<NodeKey, Report> {
        let mut listeners = BTreeMap::<NodeKey, Range<u32>>::new();
        let runs = shaped.authored.as_ref().map_or(&shaped.runs, |a| &a.runs);
        let value = shaped
            .authored
            .as_ref()
            .map_or(shaped.value.as_ref(), |a| a.value.as_str());
        let source_offset = |n| {
            shaped
                .authored
                .as_ref()
                .map_or(n, |a| a.map.source_utf16(n as usize) as u32)
        };
        for (run, source) in runs {
            let mut at = Some(*source);
            while let Some(node) = at.and_then(|id| self.host.kernel().node(id)) {
                if self
                    .host
                    .runner()
                    .handlers_of(node.id)
                    .contains(&EventKind::Selectionchange)
                {
                    listeners
                        .entry(node.key)
                        .and_modify(|r| r.end = run.end)
                        .or_insert(run.clone());
                }
                if node.id == paragraph {
                    break;
                }
                at = node.parent;
            }
        }
        listeners
            .into_iter()
            .filter_map(|(key, own)| {
                let mut report = Report::default();
                for r in selected {
                    let start = source_offset(r.start).max(own.start);
                    let end = source_offset(r.end).min(own.end);
                    if start >= end {
                        continue;
                    }
                    if report.text.is_empty() {
                        report.range.start = start - own.start;
                    }
                    report.range.end = end - own.start;
                    let visible = |n| {
                        shaped
                            .authored
                            .as_ref()
                            .map_or(n, |a| a.map.collapsed_utf16(n as usize) as u32)
                    };
                    report.rects.extend(marks(visible(start)..visible(end)));
                    report
                        .text
                        .push_str(&value[field::byte_at(value, start)..field::byte_at(value, end)]);
                }
                (!report.text.is_empty()).then_some((key, report))
            })
            .collect()
    }

    #[cfg(any(target_os = "android", test))]
    fn replace_selection(&mut self, next: Option<Selection>) {
        let old = std::mem::replace(&mut self.reader_selection, next);
        let events = Self::changed_reports(
            old.as_ref().map(|s| &s.reports),
            self.reader_selection.as_ref().map(|s| &s.reports),
        );
        self.report_selection(events);
    }

    fn changed_reports(
        was: Option<&BTreeMap<NodeKey, Report>>,
        now: Option<&BTreeMap<NodeKey, Report>>,
    ) -> Vec<(NodeKey, Event)> {
        let keys: std::collections::BTreeSet<_> = was
            .into_iter()
            .chain(now)
            .flat_map(|m| m.keys().copied())
            .collect();
        let mut events = Vec::new();
        for key in keys {
            let before = was.and_then(|m| m.get(&key));
            let after = now.and_then(|m| m.get(&key));
            if before != after {
                let (text, start, end) = after.map_or((String::new(), 0, 0), |r| {
                    (r.text.clone(), r.range.start, r.range.end)
                });
                events.push((
                    key,
                    Event::SelectionChange {
                        text,
                        start: f64::from(start),
                        end: f64::from(end),
                        rects: after.map(|r| r.rects.clone()).unwrap_or_default(),
                    },
                ));
            }
        }
        events
    }

    fn report_selection(&mut self, events: Vec<(NodeKey, Event)>) {
        for (key, event) in events {
            if let Some(node) = self.host.kernel().node_by_key(key) {
                if let Some(e) = self
                    .host
                    .dispatch_at(node.id, event, self.host.now())
                    .or(self.after_commit())
                {
                    self.host.log(e);
                }
            }
        }
    }

    /// 0 long press, 1/2 move the leading/trailing handle, 3 select all,
    /// 4 read geometry, 5 read selected text, 6 dismiss.
    #[cfg(any(target_os = "android", test))]
    pub(crate) fn reader_select(
        &mut self,
        action: i32,
        x: f32,
        y: f32,
        scale: f32,
    ) -> Option<serde_json::Value> {
        if action == 6 {
            self.replace_selection(None);
            return None;
        }
        let id = if action == 0 {
            if self.context_menu_at(x, y).is_some() {
                return None;
            }
            let hit = self.hit(x, y)?;
            if self.brush.inline_text.contains_key(&hit) {
                hit
            } else {
                self.inline_block(hit)?
            }
        } else {
            let old = self.reader_selection.as_ref()?;
            self.host.kernel().node_by_key(old.key)?.id
        };
        self.boxes();
        if !self.alert_allows(id)
            || self.host.route_visibility(id).1
            || !self.display.allows(self.host.kernel(), id)
        {
            return None;
        }
        let b = self.boxes.iter().find(|b| b.id == id && !b.projected())?;
        let (vw, vh) = self.viewport();
        let clip = b.clip.unwrap_or((0., 0., vw, vh));
        let paragraph = self.brush.inline_text.get(&id)?.clone();
        let (key, shaped) = paragraph.selectable()?;
        if self.host.kernel().node(id)?.key != key {
            return None;
        }
        let parts = self.selection_parts(shaped);
        let old = self.reader_selection.as_ref().filter(|s| {
            s.key == key && (Rc::ptr_eq(&s.value, &shaped.value) || s.value == shaped.value)
        });
        let range = match action {
            0 => {
                let hit = paragraph.selection_hit(x, y, true)?;
                let part = parts.iter().find(|p| p.range.contains(&hit))?;
                if part.atomic.is_some() {
                    part.range.clone()
                } else {
                    let (start, end) = field_pointer::word_range(&shaped.value, hit);
                    start.max(part.range.start)..end.min(part.range.end)
                }
            }
            1 | 2 => {
                let r = &old?.range;
                let hit = paragraph.selection_hit(x, y, false)?;
                if action == 1 && hit < r.end {
                    hit..r.end
                } else if action == 2 && hit > r.start {
                    r.start..hit
                } else {
                    r.clone()
                }
            }
            3 => {
                old?;
                parts.first()?.range.start..parts.last()?.range.end
            }
            4 | 5 => old?.range.clone(),
            _ => return None,
        };
        let (range, ranges) = selected_ranges(&parts, range)?;
        if action == 5 {
            let mut text = String::new();
            for r in ranges {
                text.push_str(
                    &shaped.value[field::byte_at(&shaped.value, r.start)
                        ..field::byte_at(&shaped.value, r.end)],
                );
            }
            return Some(serde_json::Value::String(text));
        }
        let rect = |(x, y, w, h): Rect4| [x * scale, y * scale, (x + w) * scale, (y + h) * scale];
        let marks: Vec<_> = ranges
            .iter()
            .cloned()
            .flat_map(|r| paragraph.selection_marks(r, clip))
            .map(rect)
            .collect();
        let (start, start_rtl) = paragraph.selection_caret(range.start, false)?;
        let (end, end_rtl) = paragraph.selection_caret(range.end, true)?;
        if action != 4 {
            let reports = self.selection_reports(id, shaped, &ranges, |_| Vec::new());
            self.replace_selection(Some(Selection {
                key,
                value: shaped.value.clone(),
                range,
                reports,
            }));
        }
        Some(
            serde_json::json!({ "marks": marks, "start": rect(start), "end": rect(end), "startRtl": start_rtl, "endRtl": end_rtl, "clip": rect(clip) }),
        )
    }
}

fn selected_ranges(parts: &[Part], mut range: Range<u32>) -> Option<(Range<u32>, Vec<Range<u32>>)> {
    if range.is_empty() {
        return None;
    }
    let intersected = |p: &&Part| p.range.start < range.end && range.start < p.range.end;
    let first = parts.iter().find(intersected)?;
    let last = parts.iter().rfind(intersected)?;
    range.start = if first.atomic.is_some() {
        first.range.start
    } else {
        range.start.max(first.range.start)
    };
    range.end = if last.atomic.is_some() {
        last.range.end
    } else {
        range.end.min(last.range.end)
    };
    let ranges = parts
        .iter()
        .map(|p| range.start.max(p.range.start)..range.end.min(p.range.end))
        .filter(|r| !r.is_empty())
        .collect::<Vec<_>>();
    Some((range, ranges))
}
