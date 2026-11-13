//! Failed store writes remain owed until persisted or superseded by a peer's.
//! Report each pending name once, even when later commits retry it.
use super::*;

impl<D: DataSource> Host<D> {
    /// Persist after the runner commits. I/O failures belong in the journal;
    /// they cannot refuse work the runner has already committed.
    pub(super) fn persist(&mut self) {
        let mut changed = self.runner.data().take_storage_changed();
        if let Some(store) = &self.secrets {
            store.retry_failed();
        }
        for line in self.kept_failures() {
            self.store_failure(line);
        }
        let writes = self.runner.take_store_writes();
        if self.secrets.is_none() {
            if changed {
                crate::store::widget_changed(self.runner.data_ref().app_id());
            }
            return;
        }
        for write in writes {
            if !write.name.starts_with(exact_runner::Store::KEPT)
                && !write.name.starts_with(exact_runner::Store::STATE)
            {
                if let Some(preloads) = &self.preloads {
                    preloads.drop_secret(&write.name);
                }
            }
            self.pending_store_stamps.remove(&write.name);
            self.reported_store_failures.remove(&write.name);
            if write.durable {
                self.pending_store_writes
                    .retain(|pending| pending.name != write.name);
                self.pending_store_bases.remove(&write.name);
                changed = true;
                continue;
            }
            if let Some(stamp) = self
                .secrets
                .as_ref()
                .and_then(|store| store.stamp(&write.name))
            {
                self.pending_store_stamps.insert(write.name.clone(), stamp);
            }
            if let Some(pending) = self
                .pending_store_writes
                .iter_mut()
                .find(|w| w.name == write.name)
            {
                *pending = write;
            } else {
                self.pending_store_writes.push(write);
            }
        }
        let mut disk = None;
        for write in std::mem::take(&mut self.pending_store_writes) {
            let stamp = self.pending_store_stamps.remove(&write.name);
            let store = self.secrets.as_ref().expect("checked above");
            if let Err(error) = store.write_stamped(&write, stamp.as_ref()) {
                if let Some(stamp) = stamp {
                    self.pending_store_stamps.insert(write.name.clone(), stamp);
                }
                if !self.pending_store_bases.contains_key(&write.name) {
                    // The disk's bytes at the first failure: a refresh that
                    // reads them again finds no peer write since.
                    let read = disk.get_or_insert_with(|| store.snapshot(Vec::new()));
                    if read.complete {
                        let base = read.values.iter().find(|(name, _)| name == &write.name);
                        self.pending_store_bases
                            .insert(write.name.clone(), base.map(|(_, value)| value.clone()));
                    }
                }
                if self.reported_store_failures.insert(write.name.clone()) {
                    self.store_failure(format!("store {} failed: {error}", write.name));
                }
                self.pending_store_writes.push(write);
            } else {
                self.reported_store_failures.remove(&write.name);
                self.pending_store_bases.remove(&write.name);
                changed = true;
            }
        }
        if changed {
            crate::store::widget_changed(self.runner.data_ref().app_id());
        }
    }

    fn store_failure(&mut self, line: String) {
        self.runner.log(line);
    }
}
