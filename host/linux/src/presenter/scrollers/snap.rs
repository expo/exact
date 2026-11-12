//! LLP 1008 §9: the reader chooses the nearest projected rest, on either axis.
use super::*;
use crate::paint::space::mul3;
use crate::placement::inverse;
use exact_kernel::{ScrollSnapAlign as Align, ScrollSnapType as Type};

impl<D: DataSource> Presenter<D> {
    pub(super) fn write_snaps(&mut self, s: &Scroller, scale: f32, out: &mut Vec<i32>) {
        let Some(node) = self.host.kernel().node(s.id) else {
            out.extend([0, 0]);
            return;
        };
        let vertical = match node.style.scroll_snap_type {
            Type::XMandatory => false,
            Type::YMandatory => true,
            Type::None => {
                out.extend([0, 0]);
                return;
            }
        };
        let [top, right, bottom, left] = node.style.border_widths();
        let mut areas = Vec::new();
        let mut walk = node.children();
        while let Some(id) = walk.pop() {
            let Some(n) = self.host.kernel().node(id) else {
                continue;
            };
            if n.style.display == exact_kernel::Display::None {
                continue;
            }
            if n.style.scroll_snap_align != Align::None {
                areas.push((id, n.style.scroll_snap_align));
            }
            // A nested container is an area of its parent, but owns its descendants.
            let (ox, oy) = effective_overflow(&n);
            let owns = |o| matches!(o, Overflow::Auto | Overflow::Scroll | Overflow::Hidden);
            if !owns(ox) && !owns(oy) {
                walk.extend(n.children());
            }
        }
        let header = out.len();
        out.extend([if vertical { Y } else { X }, 0]);
        let Some((port_map, port)) = self.reveal_geometry(s.id) else {
            return;
        };
        let Some(to_port) = inverse(port_map) else {
            return;
        };
        let port = port.scroll_rect();
        let (port_at, port_len, offset, maximum) = if vertical {
            (
                port.1 + top,
                (port.3 - top - bottom).max(0.0),
                s.offset.1,
                s.max.1,
            )
        } else {
            (
                port.0 + left,
                (port.2 - left - right).max(0.0),
                s.offset.0,
                s.max.0,
            )
        };
        for (id, align) in areas {
            let Some((mapping, b)) = self.reveal_geometry(id) else {
                continue;
            };
            let Some(rect) = b.scroll_bounds(&mul3(&to_port, &mapping)) else {
                continue;
            };
            let (at, size) = if vertical {
                (rect.1, rect.3)
            } else {
                (rect.0, rect.2)
            };
            let lo = at - port_at + offset;
            let wide = size >= port_len;
            let rest = match align {
                _ if wide => lo,
                Align::Center => lo + (size - port_len) / 2.0,
                Align::End => lo + size - port_len,
                _ => lo,
            };
            let from = rest.clamp(0.0, maximum);
            let to = if wide {
                (lo + size - port_len).clamp(from, maximum)
            } else {
                from
            };
            out.extend([bits(from * scale), bits(to * scale)]);
            out[header + 1] += 1;
        }
    }
}
