//! Events from their hosts' UTF-8 payloads, and what an action receives of each.

use super::super::{media_session, MouseEvent, PointerEvent};
use super::*;

impl Event {
    /// Decode a DOM focus event's target id and optional related id. The
    /// byte length keeps arbitrary UTF-8 ids, including newlines, unambiguous.
    pub fn focus_payload(entering: bool, payload: &str) -> Option<Self> {
        let (len, rest) = payload.split_once(':')?;
        let len = len.parse::<usize>().ok()?;
        let target = rest.get(..len)?.to_owned();
        let related_target = match rest.get(len..)? {
            "0" => None,
            rest => Some(rest.strip_prefix('1')?.to_owned()),
        };
        Some(if entering {
            Self::Focusin {
                target,
                related_target,
            }
        } else {
            Self::Focusout {
                target,
                related_target,
            }
        })
    }

    /// A press as a host writes it ([`MouseEvent::parse`]): the
    /// modifiers held, after the pointer's point where there was one;
    /// `Press` for neither; `None` for a malformed line.
    pub fn press(payload: &str) -> Option<Self> {
        Some(Self::pressed(MouseEvent::parse(payload)?))
    }

    /// A press with `mouse`'s point and modifiers: `Press` for neither.
    pub fn pressed(mouse: MouseEvent) -> Self {
        if mouse == MouseEvent::default() {
            Self::Press
        } else {
            Self::PressWith(mouse)
        }
    }

    /// A key from its chord ([`KeyModifiers::split`]), with no code.
    pub fn key(chord: &str) -> Self {
        Self::Key(KeyboardEvent::chord(chord))
    }

    /// The DOM record this event offers its action as an optional last
    /// parameter, its fields in the compiler's order
    /// (`contract_types::event_record`, `contract/types/src/selection.rs`):
    /// `key`'s and `keyup`'s `KeyboardEvent`, `press`'s `MouseEvent`, the pointer's and
    /// `contextmenu`'s `PointerEvent`, `scroll`'s `ScrollEvent`, the
    /// clipboard's `ClipboardEvent`, `wheel`'s `WheelEvent` and the
    /// `DragEvent` of `drop`, `dragenter` and `dragleave`.
    pub fn record(&self) -> Option<Value> {
        match self {
            Event::Toggle { old_open, new_open } => Some(Value::record(vec![
                Value::str(if *old_open { "open" } else { "closed" }),
                Value::str(if *new_open { "open" } else { "closed" }),
            ])),
            Event::Key(k) | Event::Keycapture(k) | Event::Keyup(k) => Some(k.value()),
            Event::Pointerdown(p)
            | Event::Pointerup(p)
            | Event::Pointermove(p)
            | Event::ContextmenuAt(p) => Some(p.value()),
            // A menu asked for with no point (a keyboard's menu key): the
            // record DOM gives one, at the node's origin, no button held.
            Event::Contextmenu => Some(
                PointerEvent {
                    offset_x: 0.0,
                    offset_y: 0.0,
                    button: 0.0,
                    buttons: 0.0,
                    pressure: 0.0,
                    pointer_type: "mouse".into(),
                    pointer_id: 1.0,
                    client_x: 0.0,
                    client_y: 0.0,
                    held: KeyModifiers::default(),
                }
                .value(),
            ),
            Event::Wheel(w) => Some(w.value()),
            Event::Historyswipe(s) => Some(s.value()),
            Event::Scrolldragend(d) => Some(d.value()),
            Event::MidiMessage(m) => Some(m.value()),
            Event::Drop(d) | Event::Dragenter(d) | Event::Dragleave(d) => Some(d.value()),
            Event::FrameNavigation(_, n) => Some(n.value()),
            Event::FrameRecord(_, v) => Some((**v).clone()),
            Event::Atompress(a) => Some(a.value()),
            Event::Swipe(direction) => Some(Value::record(vec![Value::str(direction)])),
            Event::Resize(r) => Some(r.value()),
            Event::Intersect {
                is_intersecting,
                intersection_ratio,
            } => Some(Value::record(vec![
                Value::Bool(*is_intersecting),
                Value::Number(*intersection_ratio),
            ])),
            Event::Scroll(s) => Some(s.value(None)),
            Event::Clipboard(_, text, files) => Some(Value::record(vec![
                Value::str(text),
                Value::list(files.iter().map(|f| Value::str(f)).collect()),
            ])),
            Event::SelectionChange {
                text,
                start,
                end,
                rects,
            } => Some(Value::record(vec![
                Value::str(text),
                Value::Number(*start),
                Value::Number(*end),
                Value::list(rects.iter().map(|r| r.value()).collect()),
            ])),
            Event::Press => Some(MouseEvent::default().value()),
            Event::PressWith(mouse) => Some(mouse.value()),
            Event::Media(kind, payload) => media_session::record(*kind, payload),
            Event::Focusin {
                target,
                related_target,
            }
            | Event::Focusout {
                target,
                related_target,
            } => Some(Value::record(vec![
                Value::str(target),
                related_target
                    .as_ref()
                    .map_or(Value::NONE, |id| Value::some(Value::str(id))),
            ])),
            _ => None,
        }
    }

    /// Decode exactly four comma-separated geometry dimensions. Hosts validate
    /// binding identity/mapping before delivery; zero suspends physical admission.
    pub fn transform_geometry_payload(payload: &str) -> Option<Self> {
        let [box_width, box_height, port_width, port_height] = tuple(payload)?;
        let event = Self::TransformGeometry {
            box_width,
            box_height,
            port_width,
            port_height,
        };
        event.invalid_payload().is_none().then_some(event)
    }

    /// Decode exactly `x,y,scale,vx,vy,vscale`. Physical hosts separately check
    /// both tokens, all three binding keys, incarnation and geometry before time.
    pub fn transform_release_payload(payload: &str) -> Option<Self> {
        let [x, y, scale, vx, vy, vscale] = tuple(payload)?;
        let event = Self::TransformRelease {
            x,
            y,
            scale,
            vx,
            vy,
            vscale,
        };
        event.invalid_payload().is_none().then_some(event)
    }

    /// Decode two finite, layout-representable pan deltas; no clock on refusal.
    pub fn pan_payload(payload: &str) -> Option<Self> {
        let [x, y] = tuple(payload)?;
        (pixel(x) && pixel(y)).then_some(Self::Pan(x, y))
    }

    /// Decode exactly `vx,vy`: two finite velocities; no clock on refusal.
    pub fn pan_release_payload(payload: &str) -> Option<Self> {
        let [vx, vy] = tuple(payload)?;
        (vx.is_finite() && vy.is_finite()).then_some(Self::PanRelease(vx, vy))
    }

    pub(super) fn invalid_payload(&self) -> Option<&'static str> {
        match *self {
            Self::Pan(x, y) if !pixel(x) || !pixel(y) => Some("pan"),
            Self::PanRelease(vx, vy) if !vx.is_finite() || !vy.is_finite() => Some("panrelease"),
            Self::HeightRelease { height, velocity } if !valid_height_release(height, velocity) => {
                Some("heightrelease")
            }
            Self::TransformGeometry {
                box_width,
                box_height,
                port_width,
                port_height,
            } if ![box_width, box_height, port_width, port_height]
                .into_iter()
                .all(|v| pixel(v) && v >= 0.0) =>
            {
                Some("transformgeometry")
            }
            Self::TransformRelease {
                x,
                y,
                scale,
                vx,
                vy,
                vscale,
            } if !pixel(x)
                || !pixel(y)
                || !pixel(scale)
                || scale <= 0.0
                || (scale as f32) <= 0.0
                || ![vx, vy, vscale].into_iter().all(f64::is_finite) =>
            {
                Some("transformrelease")
            }
            _ => None,
        }
    }

    /// A checkbox's reported state, exactly `true` or `false` (LLP 1069.001
    /// D4), as the payload of an `input` or `change`.
    pub fn checked_payload(payload: &str) -> Option<ControlValue> {
        match payload {
            "true" => Some(ControlValue::Checked(true)),
            "false" => Some(ControlValue::Checked(false)),
            _ => None,
        }
    }

    /// Decode exactly `height,velocity`. Hosts must parse before advancing time.
    /// This synthesizes an event; physical delivery separately validates both
    /// binding generations and a live Height token before clock or action.
    pub fn height_release_payload(payload: &str) -> Option<Self> {
        let (height, velocity) = payload.split_once(',')?;
        let (height, velocity) = (
            exact_num::parse_f64(height).ok()?,
            exact_num::parse_f64(velocity).ok()?,
        );
        valid_height_release(height, velocity).then_some(Self::HeightRelease { height, velocity })
    }

    /// Decode the scroll event: `left,top,scrollWidth,scrollHeight,
    /// clientWidth,clientHeight`, finite CSS pixels, the extents not negative.
    pub fn scroll_payload(payload: &str) -> Option<Self> {
        let [left, top, width, height, client_width, client_height] = tuple::<6>(payload)?;
        ([left, top].iter().all(|v| v.is_finite())
            && [width, height, client_width, client_height]
                .iter()
                .all(|v| v.is_finite() && *v >= 0.0))
        .then_some(Self::Scroll(ScrollEvent {
            left,
            top,
            width,
            height,
            client_width,
            client_height,
        }))
    }
}

fn tuple<const N: usize>(payload: &str) -> Option<[f64; N]> {
    let mut parts = payload.split(',');
    let mut values = [0.0; N];
    for value in &mut values {
        *value = exact_num::parse_f64(parts.next()?).ok()?;
    }
    parts.next().is_none().then_some(values)
}

fn pixel(v: f64) -> bool {
    v.is_finite() && v.abs() <= f32::MAX as f64
}

fn valid_height_release(height: f64, velocity: f64) -> bool {
    height.is_finite() && (0.0..=f32::MAX as f64).contains(&height) && velocity.is_finite()
}
