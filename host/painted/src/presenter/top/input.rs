//! Input delivered by a platform's top-layer window.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// Whether a press is down in the window of the surface that is open.
    pub(in crate::presenter) fn surface_pressed(&self) -> bool {
        let open = self.top.open.as_ref().map(|open| open.serial);
        self.surface_press.is_some() && self.surface_press == open
    }

    /// What the shell says happened in the window of surface `serial`
    /// ([`event`]): a touch at (`x`, `y`) of the surface, in points from
    /// its top left; a key; a dismissal; a refusal. What it says of a
    /// surface that is no longer the one open is dropped.
    pub fn surface_event(&mut self, serial: u32, what: i32, x: f32, y: f32, now_ms: f64) {
        // A press in the surface's own window reaches no `pointer_down`. Its
        // lift, its cancel or the window's end ends it; a key does not.
        match what {
            0 => self.surface_press = Some(serial),
            1 | 3 | event::DISMISSED | event::REFUSED if self.surface_press == Some(serial) => {
                self.surface_press = None
            }
            _ => {}
        }
        if self.top.open.as_ref().map(|open| open.serial) != Some(serial) {
            return;
        }
        let key = match what {
            event::ENTER => Some("Enter"),
            event::SPACE => Some(" "),
            event::TAB | event::TAB_BACK => Some("Tab"),
            event::UP => Some("ArrowUp"),
            event::DOWN => Some("ArrowDown"),
            event::LEFT => Some("ArrowLeft"),
            event::RIGHT => Some("ArrowRight"),
            _ => None,
        };
        // Record the input's modality before delivering it, as the page's
        // input does: the focus restored when the surface closes follows it.
        if let Some(key) = key {
            self.key_shows_ring(key);
        }
        match what {
            0 => self.touched(),
            2 if self.brush.winui_focus => self.brush.pointer_modality = true,
            _ => {}
        }
        match what {
            0..=3 => self.surface_touch(what, x, y, now_ms),
            event::DISMISSED => {
                let open = self.top.open.as_ref().unwrap();
                let (node, modal) = (open.node, open.modal);
                // The shell's window is already gone. HTML permits ignoring
                // cancelation when the UA cannot honor it; cancel still fires.
                if modal {
                    self.dialog_close_prevented(node, now_ms);
                }
                if self.top.open.as_ref().is_some_and(|o| o.serial == serial) {
                    self.close_surface(now_ms);
                }
            }
            event::REFUSED => {
                self.host.log(format!(
                    "the top layer's surface {serial} was refused: the shell could not make its window"
                ));
                self.close_surface(now_ms);
            }
            event::ENTER | event::SPACE => {
                self.surface_key("Enter", now_ms);
            }
            event::TAB | event::DOWN => self.surface_step(false),
            event::TAB_BACK | event::UP => self.surface_step(true),
            event::LEFT | event::RIGHT => self.surface_horizontal(what == event::LEFT, now_ms),
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::tests::{boot, tap};
    use super::*;

    /// A press in the surface's own window is down until that pointer
    /// lifts or is cancelled, or the surface goes: a key leaves it down,
    /// and a replacement waiting (LLP 1029.000.002 D9).
    #[test]
    fn a_key_leaves_a_surfaces_press_down() {
        let mut p = boot();
        p.serve_overlays();
        p.frame();
        tap(&mut p, "more").unwrap();
        let open = p.top.open.as_ref().unwrap();
        let (serial, (x, y, w, h)) = (open.serial, open.layout.targets[0].rect);
        let (x, y) = (x + w / 2., y + h / 2.);
        p.surface_event(serial, 0, x, y, 1.);
        p.surface_event(serial, event::DOWN, 0., 0., 2.);
        assert_eq!(p.interaction_hold(), Some("pointer"));
        p.surface_event(serial, 2, x, y, 3.);
        assert_eq!(p.interaction_hold(), Some("pointer"));
        p.surface_event(serial, 3, x, y, 4.);
        assert_eq!(p.interaction_hold(), None);
    }
}
