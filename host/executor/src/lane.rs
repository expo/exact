//! The independent lane's transport (LLP 1162 D5): one for its six workers,
//! the scoped requests they run and the launch's preloads, so a launch's
//! overlapping reads share one connection pool instead of each worker
//! opening its own. The ordered lane keeps the boot's, and a stream opens
//! its own (`stream.rs`).
use super::StreamHost;
use std::sync::{Arc, OnceLock};

pub struct Lane {
    host: StreamHost,
    /// Made at the first use, on the thread that endows from it: a worker's
    /// or a preload's, never the host's while it publishes its first frame
    /// (the machine, when on, endows when the core is made).
    made: OnceLock<ibex2::host::Host>,
}

impl Lane {
    pub(crate) fn new(host: StreamHost) -> Arc<Lane> {
        Arc::new(Lane {
            host,
            made: OnceLock::new(),
        })
    }

    /// Bindings with `grants` over the lane's one transport.
    pub(crate) fn endow(&self, grants: ibex2::grant::GrantSet) -> ibex2::host::Bindings {
        self.made.get_or_init(|| (self.host)()).endow(grants)
    }
}
