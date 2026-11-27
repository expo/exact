//! A driver's scroll steps and the reader's touches (LLP 1076 §3.3): a
//! driver's scroll moves the feed (a finger's is `scroll.rs`), and a touch
//! is the presenter's pointer at the view's pixels. Each touch says what touched (LLP 1115.001 D7.4): this entry is
//! where a finger is told from a mouse, which desktop Linux's own inputs
//! (evdev, VNC) and the agent remain.

use super::{CanvasHost, Section};
use crate::presenter::PointerKind;
use exact_runner::DataSource;

impl<D: DataSource + Default> CanvasHost<D> {
    /// A driver from outside (a benchmark's, a test's) scrolls the feed by
    /// `dy` pixels: the scroller a finger last moved, else what is under
    /// the viewport's center.
    pub fn scroll(&mut self, dy: f32) {
        let (x, y) = (self.viewport.0 / 2.0, self.viewport.1 / 2.0);
        let _s = Section::begin(c"exact scroll");
        self.driven();
        self.scroll_hold.note_scroll(self.prefetching);
        self.scrolled_at = self.now();
        self.travel.scrolled(dy / self.scale, self.scrolled_at);
        self.p.hold_collections(self.prefetching);
        // The feed: the scroller the first wheel at the centre took, moved
        // directly after (a nested list under the centre would take it).
        let moved = self
            .feed
            .is_some_and(|id| self.p.scroll_by(id, dy / self.scale));
        if !moved && self.feed.is_none() {
            self.p.wheel_at(x, y, 0.0, dy / self.scale);
            self.feed = self.p.last_wheel();
        }
        self.p.hold_collections(false);
    }

    /// Selection commands use the reader's pixel coordinates and return visible handle
    /// geometry, or the selected text for a clipboard or share action.
    #[cfg(target_os = "android")]
    pub fn reader_selection(&mut self, action: i32, x: f32, y: f32) -> Option<String> {
        let reply = self
            .p
            .reader_select(action, x / self.scale, y / self.scale, self.scale)?;
        Some(match reply {
            serde_json::Value::String(text) => text,
            geometry => geometry.to_string(),
        })
    }

    /// A touch (0 down, 1 up, 2 move, 3 cancel) at pixels, by Android's
    /// tool type `tool`: a mouse (3) is a mouse, a stylus or its eraser (2,
    /// 4) a pen, and anything else a finger. Action 4 is no new touch: the
    /// one that is down has been held still for the system's long-press
    /// time, which the reader measures; what names a context menu there
    /// takes it ([`crate::presenter::Presenter::long_press`]), and the
    /// touch then presses nothing when it lifts. `at_ns`: when it happened,
    /// the `MotionEvent`'s time (`CLOCK_MONOTONIC`, `SystemClock`'s
    /// uptime), or -1 for one the reader made itself.
    pub fn touch(
        &mut self,
        action: i32,
        x: f32,
        y: f32,
        tool: i32,
        at_ns: i64,
    ) -> Option<[i32; 4]> {
        let _s = Section::begin(c"exact touch");
        self.scroll_hold.touch(action);
        self.p.set_pointer_kind(match tool {
            3 => PointerKind::Mouse,
            2 | 4 => PointerKind::Pen,
            _ => PointerKind::Touch,
        });
        self.force |= action != 2;
        if action != 2 && self.painted_once {
            self.p.slice_collections(self.feed, Some(0), 0.0);
            self.responding = true;
            self.lead = false;
            self.travel.stopped();
        }
        let now = self.now();
        let at = self.input_clock.android(action == 0, at_ns, now);
        let (x, y) = (x / self.scale, y / self.scale);
        let r = if matches!(action, 1 | 2) {
            self.p
                .transform_finger((u64::MAX, (x, y)), None, action, now, at)
        } else {
            Ok(false)
        }
        .and_then(|took| {
            if took {
                Ok(())
            } else {
                self.p.reader_touch(action, x, y, now, at)
            }
        });
        self.touched(action, x, y, now, r);
        if action == 1 {
            if let Some((id, velocity, [kx, ky])) = self.p.take_height_scroll_release() {
                return Some([
                    id as i32,
                    (velocity * self.scale).to_bits() as i32,
                    kx.to_bits() as i32,
                    ky.to_bits() as i32,
                ]);
            }
        }
        None
    }

    /// Whether a touch is down on the reader's view, said where Android
    /// delivers it and before the reader routes it
    /// ([`crate::presenter::Presenter::set_touching`]).
    pub fn touching(&mut self, on: bool) -> bool {
        self.p.set_touching(on)
    }

    /// A finger past the touch's first (0 down, 1 up, 2 move, 3 cancel) at
    /// pixels, by Android's pointer `id`: the canvas holding the touch takes
    /// it first. A second finger can join a photo hold. Other recognizers
    /// keep their first contact, and an ordinary pending press is cancelled.
    pub fn finger(
        &mut self,
        action: i32,
        id: i32,
        x: f32,
        y: f32,
        tool: i32,
        at_ns: i64,
        primary: (f32, f32),
    ) {
        let _s = Section::begin(c"exact finger");
        let now = self.now();
        let taken = self
            .p
            .canvas_finger(id, action, x / self.scale, y / self.scale, now);
        if taken {
            return;
        }
        let pos = (x / self.scale, y / self.scale);
        let event = self.input_clock.android(false, at_ns, now);
        let id = id as u64;
        let first = (u64::MAX, (primary.0 / self.scale, primary.1 / self.scale));
        let peer = primary.0.is_finite().then_some(first);
        let result = if action == 0 && tool != 1 {
            Ok(self.p.tracking_transform_pinch())
        } else {
            self.p
                .transform_finger((id, pos), peer, action, now, event)
                .and_then(|took| {
                    if !took && action == 0 && peer.is_some() {
                        self.p.begin_transform_pinch(first, id, pos, event)
                    } else {
                        Ok(took)
                    }
                })
        };
        match result {
            Ok(false) if action == 0 => {
                self.touch(3, x, y, 0, -1);
            }
            Err(e) => self.p.host_mut().log(e),
            _ => {}
        }
    }
}
