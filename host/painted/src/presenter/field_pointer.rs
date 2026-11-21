//! Mouse selection uses the field's painted glyph positions and UTF-16 offsets.
use super::*;
use crate::text::field::Cursor;
use exact_kernel::NodeKey;
use exact_runner::{FieldSelection, SelectionDirection};
use field::{byte_at, utf16_len};
use unicode_segmentation::UnicodeSegmentation;

#[derive(Clone, Copy)]
pub(super) enum Unit {
    Character,
    Word,
    Paragraph,
    Password,
}

impl Unit {
    pub(super) fn for_clicks(clicks: u32) -> Self {
        match clicks {
            0 | 1 => Self::Character,
            2 => Self::Word,
            _ => Self::Paragraph,
        }
    }
}

pub(super) struct Drag {
    key: NodeKey,
    value: String,
    anchor: (u32, u32),
    unit: Unit,
    origin: (f32, f32),
    point: (f32, f32),
    moved: bool,
    edge_clock: Option<f64>,
    scroll: Option<(f32, f32)>,
}

impl Drag {
    pub(super) fn take_scroll(&mut self, key: NodeKey, value: &str) -> Option<(f32, f32)> {
        let next = self.scroll.take()?;
        (self.key == key && self.value == value).then_some(next)
    }

    pub(super) fn moved(&self, point: (f32, f32)) -> bool {
        self.moved || point != self.origin
    }
}

pub(super) fn range(value: &str, at: u32, unit: Unit) -> (u32, u32) {
    // The painted geometry supplies a valid UTF-16 insertion offset.
    match unit {
        Unit::Character => (at, at),
        Unit::Password => (0, utf16_len(value)),
        Unit::Paragraph => {
            let byte = byte_at(value, at);
            let mut start = 0;
            for (i, sep) in value.match_indices(crate::text::PARAGRAPH_BREAKS) {
                let end = i + sep.len();
                // CRLF is one paragraph break.
                if sep == "\r" && value[end..].starts_with('\n') {
                    continue;
                }
                if byte < end {
                    return (utf16_len(&value[..start]), utf16_len(&value[..end]));
                }
                start = end;
            }
            (utf16_len(&value[..start]), utf16_len(value))
        }
        Unit::Word => {
            #[cfg(target_os = "windows")]
            if let Some(range) = super::word::range(value, at) {
                return range;
            }
            let byte = byte_at(value, at);
            value
                .split_word_bound_indices()
                .find(|(start, text)| byte < start + text.len())
                .map_or((at, at), |(start, text)| {
                    (
                        utf16_len(&value[..start]),
                        utf16_len(&value[..start + text.len()]),
                    )
                })
        }
    }
}

#[cfg(any(target_os = "android", test))]
pub(super) fn word_range(value: &str, at: u32) -> (u32, u32) {
    range(value, at.min(utf16_len(value)), Unit::Word)
}

fn selection(anchor: (u32, u32), current: (u32, u32)) -> FieldSelection {
    let (from, to) = if current.0 < anchor.0 {
        (anchor.1, current.0)
    } else {
        (anchor.0, current.1.max(anchor.1))
    };
    FieldSelection {
        start: from.min(to),
        end: from.max(to),
        direction: match to.cmp(&from) {
            std::cmp::Ordering::Less => SelectionDirection::Backward,
            std::cmp::Ordering::Equal => SelectionDirection::None,
            std::cmp::Ordering::Greater => SelectionDirection::Forward,
        },
    }
}

impl<D: DataSource> Presenter<D> {
    pub(super) fn begin_field_drag(
        &mut self,
        key: NodeKey,
        point: (f32, f32),
        clicks: u32,
        before: Option<(NodeKey, String, FieldSelection)>,
    ) {
        let Some(node) = self.host.kernel().node_by_key(key).filter(|n| {
            n.node_type == NodeType::TextInput
                && !crate::fields::hosts(n)
                && self.focus == Some(n.id)
                && self.input_live(key)
                && self.focus_refusal(n.id).is_none()
                && !self.pointerdown_prevents_focus(n.id)
        }) else {
            return;
        };
        let id = node.id;
        let unit = if clicks > 1 && node.props.str(PropId::Type) == Some("password") {
            Unit::Password
        } else {
            Unit::for_clicks(clicks)
        };
        let value = self.field_text(id);
        let Some(cursor) = self.field_pointer_hit(id, &value, point, unit, None) else {
            return;
        };
        let current = range(&value, cursor.offset, unit);
        let anchor = before
            .filter(|(old_key, old_value, _)| {
                *old_key == key && *old_value == value && self.modifiers().shift
            })
            .map_or(current, |(_, _, mark)| {
                let at = if mark.direction == SelectionDirection::Backward {
                    mark.end
                } else {
                    mark.start
                };
                (at, at)
            });
        self.mark_pointer_selection(id, &value, selection(anchor, current), cursor.upstream);
        self.field_drag = Some(Drag {
            key,
            value,
            anchor,
            unit,
            origin: point,
            point,
            moved: false,
            edge_clock: None,
            scroll: None,
        });
    }

    fn field_pointer_hit(
        &mut self,
        id: ViewId,
        value: &str,
        point: (f32, f32),
        unit: Unit,
        autoscroll: Option<(f32, f32)>,
    ) -> Option<Cursor> {
        self.boxes();
        let point = self
            .boxes
            .iter()
            .find(|b| b.id == id)?
            .field_point(point.0, point.1);
        let field = self.current_field(id, value)?;
        let (x, y) = field.selection_point(point, autoscroll)?;
        Some(match unit {
            Unit::Character => field.geometry.hit(x, y),
            _ => Cursor {
                offset: field.geometry.hit_character(x, y),
                upstream: false,
            },
        })
    }

    fn dragged_field(&self, drag: &Drag) -> Option<ViewId> {
        let id = self.host.kernel().node_by_key(drag.key)?.id;
        (self.focus == Some(id)
            && self.input_live(drag.key)
            && self.focus_refusal(id).is_none()
            && self.field_text_ref(id) == drag.value)
            .then_some(id)
    }

    fn extend_field_selection(
        &mut self,
        id: ViewId,
        drag: &mut Drag,
        scroll: (f32, f32),
    ) -> Option<()> {
        let cursor =
            self.field_pointer_hit(id, &drag.value, drag.point, drag.unit, Some(scroll))?;
        let current = range(&drag.value, cursor.offset, drag.unit);
        self.mark_pointer_selection(
            id,
            &drag.value,
            selection(drag.anchor, current),
            cursor.upstream,
        );
        drag.scroll = Some(scroll);
        self.field_reveal = true;
        Some(())
    }

    pub(super) fn drag_field(&mut self, x: f32, y: f32) -> bool {
        let Some(mut drag) = self.field_drag.take() else {
            return false;
        };
        let Some(id) = self.dragged_field(&drag) else {
            return false;
        };
        if drag.point != (x, y) {
            drag.point = (x, y);
            drag.moved = true;
            // A mouse sample selects at the visible edge. Only a frame advances the scroll.
            let scroll = drag.scroll.unwrap_or_else(|| self.scroll_of(id));
            if self.extend_field_selection(id, &mut drag, scroll).is_none() {
                return false;
            }
        }
        self.field_drag = Some(drag);
        true
    }

    pub(super) fn field_drag_edge(&self) -> Option<(f32, f32)> {
        let drag = self.field_drag.as_ref().filter(|d| d.moved)?;
        let id = self.host.kernel().node_by_key(drag.key)?.id;
        let b = self.boxes.iter().find(|b| b.id == id)?;
        let field = self.brush.field_text.get(&id)?;
        let inverse = crate::placement::inverse(b.scroll_mapping()?)?;
        let (px, py) = crate::placement::map(&inverse, drag.point.0, drag.point.1);
        let (x, y, w, h) = b.scroll_rect();
        let scroll = drag.scroll.unwrap_or_else(|| self.scroll_of(id));
        let speed = |distance: f32, offset: f32, max: f32| {
            if (distance < 0. && offset > 0.) || (distance > 0. && offset < max) {
                distance.clamp(-32., 32.) * (720. / 32.)
            } else {
                0.
            }
        };
        let dx = px - px.clamp(x, x + w);
        let vx = speed(
            if field.scrolls_from_right() { -dx } else { dx },
            scroll.0,
            field.scroll_max.0,
        );
        let vy = speed(py - py.clamp(y, y + h), scroll.1, field.scroll_max.1);
        (vx != 0. || vy != 0.).then_some((vx, vy))
    }

    pub(super) fn tick_field_drag(&mut self, now: f64) {
        let Some(velocity) = self.field_drag_edge() else {
            if let Some(drag) = &mut self.field_drag {
                drag.edge_clock = None;
            }
            return;
        };
        let mut drag = self.field_drag.take().unwrap();
        let Some(id) = self.dragged_field(&drag) else {
            return;
        };
        let dt = drag
            .edge_clock
            .replace(now)
            .map_or(0., |last| ((now - last) / 1000.).clamp(0., 0.05) as f32);
        let old = drag.scroll.unwrap_or_else(|| self.scroll_of(id));
        let max = self.brush.field_text[&id].scroll_max;
        let next = (
            (old.0 + velocity.0 * dt).clamp(0., max.0),
            (old.1 + velocity.1 * dt).clamp(0., max.1),
        );
        if old != next && self.extend_field_selection(id, &mut drag, next).is_none() {
            return;
        }
        self.field_drag = Some(drag);
    }
}

#[cfg(test)]
mod tests;
