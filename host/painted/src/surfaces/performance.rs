use super::*;
impl Abi {
    pub(super) fn performance(&self, id: u32) -> Option<Vec<u8>> {
        static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
        let now = START
            .get_or_init(std::time::Instant::now)
            .elapsed()
            .as_secs_f64()
            * 1000.0;
        // SAFETY: the checked module ABI takes the canvas and a monotonic clock.
        let length = unsafe {
            self.symbol::<unsafe extern "C" fn(u32, f64) -> u32>(b"gpu_performance")(id, now)
        };
        if length > 16 * 1024 {
            return None;
        }
        self.bytes(length)
    }

    pub(super) fn enable_performance(&self, id: u32) {
        // SAFETY: the checked ABI; this canvas belongs to the module.
        unsafe {
            self.symbol::<unsafe extern "C" fn(u32, bool) -> bool>(b"gpu_performance_reporting")(
                id, true,
            )
        };
    }
}

impl Surfaces {
    pub(crate) fn performance_due_in_ms(&self) -> Option<f64> {
        if self.hidden || self.interrupted || !self.canvases.values().any(|c| c.performance) {
            return None;
        }
        self.performance_due.map(|due| {
            due.saturating_duration_since(std::time::Instant::now())
                .as_secs_f64()
                * 1000.0
        })
    }
}
impl<D: DataSource> Presenter<D> {
    pub(crate) fn poll_canvas_performance(&mut self) -> Option<String> {
        if self.surfaces.performance_due_in_ms() != Some(0.0) {
            return None;
        }
        self.surfaces.performance_due =
            Some(std::time::Instant::now() + std::time::Duration::from_secs(1));
        let mut changed = false;
        for c in self.surfaces.canvases.values().filter(|c| c.performance) {
            let abi = &self.surfaces.abis[&c.artifact];
            if let Some(bytes) = abi.performance(c.id) {
                self.surfaces.error = self.surfaces.error.take().or(self
                    .host
                    .canvas_performance_record(&c.name, Some(&String::from_utf8_lossy(&bytes))));
                changed = true;
            }
        }
        changed.then(|| self.after_commit()).flatten()
    }
}
