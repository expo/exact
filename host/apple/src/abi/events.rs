use super::{not_booted, Bridge};
use exact_runner::DataSource;

impl<D: DataSource> Bridge<D> {
    /// Move the clock: `mode` 0 fires every timer due (the wall clock), 1
    /// stops after a timer that sends (the agent's jump), 2 lands only the
    /// `then`s already armed, the clock unmoved (an agent's input's end).
    pub fn advance(&mut self, now_ms: f64, mode: u32) -> u32 {
        let out = self.host.as_mut().map_or_else(not_booted, |h| match mode {
            4 => h.advance_native_text(),
            2 | 3 => h.land_then(mode == 3),
            1 => h.advance_until_request(now_ms),
            _ => h.advance(now_ms),
        });
        self.emit(out)
    }
}
