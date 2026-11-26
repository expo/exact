//! The page's layout for the geometry facts (LLP 1123 D6): the browser lays
//! the page out, so the glue says when a commit's batch has settled, and the
//! runner answers again each fact whose box that layout moved.

use super::*;

impl<D: DataSource> Host<D> {
    /// After the page laid out a commit's batch: the commit answering the
    /// geometry facts it moved, or nothing when it moved none.
    pub fn geometry_settled(&mut self) -> Option<String> {
        let (receipts, error) = match self.runner.refresh_geometry() {
            Ok(Some(receipt)) => (
                vec![Timed {
                    at_ms: self.now_ms,
                    receipt,
                }],
                None,
            ),
            Ok(None) => return None,
            Err(e) => (vec![], Some(format!("geometry facts: {e:?}"))),
        };
        Some(self.batch_for(&receipts, error.as_deref()))
    }
}
