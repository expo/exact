use super::{Host, Timed};
use exact_runner::DataSource;
impl<D: DataSource> Host<D> {
    /// Publish ephemeral HUD metrics independently of the world's saved state.
    pub fn canvas_performance_record(&mut self, name: &str, json: Option<&str>) -> String {
        let (receipts, error) = match self.runner.set_canvas_performance(name, json) {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => (vec![], None),
            Err(e) => (vec![], Some(format!("canvas performance: {e:?}"))),
        };
        self.commit(&receipts, error)
    }
}
