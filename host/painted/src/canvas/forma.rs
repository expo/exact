//! The canvas host's answer to Android's `ExactForma` (LLP 1121 D12).

use super::CanvasHost;
use exact_runner::DataSource;

impl<D: DataSource + Default> CanvasHost<D> {
    /// What `ExactForma(view)` asks: [`crate::Host::forma_json`].
    pub fn forma_json(&mut self, request: &str) -> String {
        self.p.host_mut().forma_json(request)
    }
}
