use super::*;
impl<D: DataSource> Bridge<D> {
    /// A name alone clears a surface; name NUL JSON publishes it, even if empty.
    pub fn surface_record(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("surface record: invalid UTF-8");
        };
        let (name, json) = text
            .split_once('\0')
            .map_or((text, None), |(name, json)| (name, Some(json)));
        let out = self
            .host
            .as_mut()
            .map_or_else(not_booted, |host| host.surface_record(name, json));
        self.emit(out)
    }

    /// Whether the current plan opts this canvas into HUD metrics.
    pub fn canvas_performance_requested(&self, len: usize) -> u32 {
        let Ok(name) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return 0;
        };
        u32::from(
            self.host
                .as_ref()
                .is_some_and(|h| h.runner().wants_canvas_performance(name)),
        )
    }
    /// Publish or clear a named canvas's ephemeral HUD metrics.
    pub fn canvas_performance_record(&mut self, len: usize) -> u32 {
        let Ok(text) = std::str::from_utf8(&self.input[..len.min(self.input.len())]) else {
            return self.refuse("canvas performance: invalid UTF-8");
        };
        let (name, json) = text
            .split_once('\0')
            .map_or((text, None), |(name, json)| (name, Some(json)));
        let out = self
            .host
            .as_mut()
            .map_or_else(not_booted, |h| h.canvas_performance_record(name, json));
        self.emit(out)
    }
}
