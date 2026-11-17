//! Input delivered by a platform's top-layer window.
use super::*;

impl<D: DataSource> Presenter<D> {
    /// What the shell says happened in the window of surface `serial`
    /// ([`event`]): a touch at (`x`, `y`) of the surface, in points from
    /// its top left; a key; a dismissal; a refusal. What it says of a
    /// surface that is no longer the one open is dropped.
    pub fn surface_event(&mut self, serial: u32, what: i32, x: f32, y: f32, now_ms: f64) {
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
