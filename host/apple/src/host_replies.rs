use super::*;

impl<D: DataSource> Host<D> {
    /// Fulfill each reply as its own transaction, then present one batch
    /// (LLP 1162 D3). A returned error ends fulfillment; prior transactions stand.
    pub fn fulfill_all(
        &mut self,
        outcomes: impl IntoIterator<Item = crate::executor::pump::Completion>,
        now_ms: f64,
    ) -> String {
        let mut outcomes = outcomes.into_iter();
        self.fulfill_ready(|_| outcomes.next(), now_ms)
    }

    pub(crate) fn fulfill_ready(
        &mut self,
        mut next: impl FnMut(&mut Runner<D>) -> Option<crate::executor::pump::Completion>,
        now_ms: f64,
    ) -> String {
        self.now_ms = now_ms.max(self.now_ms);
        // Sample the presented batch's authored scroll before this pump changes geometry.
        let (batch, intersect_error) = self.intersect_round(Batch::new(), None);
        // Announced topics first: what the device said before these replies.
        let (announced, failed) = self.runner.apply_announced();
        let mut error = failed.map(|e| format!("{e:?}"));
        let mut receipts: Vec<Timed> = announced
            .into_iter()
            .map(|receipt| Timed {
                at_ms: self.now_ms,
                receipt,
            })
            .collect();
        while let Some((ticket, outcome, timing)) = next(&mut self.runner) {
            let preload = self.preloads.as_ref().and_then(|p| p.fulfilled(ticket));
            match self.runner.fulfill_measured(ticket, outcome, timing) {
                Ok(Some(receipt)) => {
                    self.preload_content.extend(preload);
                    receipts.push(Timed {
                        at_ms: self.now_ms,
                        receipt,
                    });
                }
                Ok(None) => {}
                Err(e) => {
                    error = Some(format!("{e:?}"));
                    break;
                }
            }
        }
        self.commit_into(&receipts, error.or(intersect_error), batch)
    }
}
