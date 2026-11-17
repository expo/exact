//! The live table on the Apple hosts (LLP 1121 D13, stage 4): a batch or
//! snapshot from the dev stream applied in place, no plan, no restart.

use super::Host;
use exact_runner::{design::Edit, DataSource, LiveRefusal, Timed};

impl<D: DataSource> Host<D> {
    /// Apply a `forma-batch` or `forma-snapshot` frame: a batch whose
    /// `live` says `applied` (with `unsupported` when the wide colour table
    /// refused a colour), `stale` (ask for a snapshot), `structure` (a plan
    /// is coming) or `refused`.
    pub fn apply_live(&mut self, frame: &str) -> String {
        let Some(edit) = Edit::parse(frame) else {
            return exact_runner::agent::error("live: not a forma batch or snapshot");
        };
        let (mut batch, live, unsupported) = match self.runner.apply_live(&edit) {
            Ok(applied) => {
                // A scrub's frame shows with no transition (LLP 1121 D13).
                let _still = edit.scrub.is_some().then(exact_kernel::motion::still);
                let mut batch = crate::batch::Batch::new();
                if applied.roles {
                    self.roles_moved(&mut batch);
                }
                let receipts = [Timed {
                    at_ms: self.now_ms,
                    receipt: applied.receipt,
                }];
                (
                    self.commit_into(&receipts, None, batch),
                    "applied".to_string(),
                    applied.full,
                )
            }
            Err(refusal) => (
                self.commit(&[], None),
                match refusal {
                    LiveRefusal::Stale => "stale".to_string(),
                    LiveRefusal::Structure => "structure".to_string(),
                    LiveRefusal::Refused(why) => format!("refused: {why}"),
                },
                false,
            ),
        };
        batch.pop();
        batch.push_str(",\"live\":");
        exact_runner::agent::quote(&live, &mut batch);
        batch.push_str(&format!(",\"unsupported\":{unsupported},\"design\":"));
        exact_runner::agent::quote(&self.runner.design(), &mut batch);
        batch.push('}');
        batch
    }
}
