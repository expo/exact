//! Launch writes reach each host on its owner thread (LLP 1162.000 D4).
use super::*;

type Wake = Arc<dyn Fn() + Send + Sync>;

/// One session's mark that a launch write changed its store.
#[derive(Default)]
pub struct StoreWatch {
    changed: AtomicBool,
    wake: Mutex<Option<Wake>>,
}
impl StoreWatch {
    pub(crate) fn pending(&self) -> bool {
        self.changed.load(Ordering::Acquire)
    }
    pub(crate) fn take_changed(&self) -> bool {
        self.changed.swap(false, Ordering::AcqRel)
    }
    pub(crate) fn listen(&self, wake: Wake) {
        *self.wake.lock().unwrap() = Some(wake.clone());
        if self.changed.load(Ordering::Acquire) {
            wake();
        }
    }
    fn changed(&self) {
        self.changed.store(true, Ordering::Release);
        let wake = self.wake.lock().unwrap().clone();
        if let Some(wake) = wake {
            wake();
        }
    }
}
impl LaunchPreloads {
    pub(crate) fn watch_store(&self, pending: impl FnOnce() -> bool) -> Arc<StoreWatch> {
        // Register before reading the snapshot, in the writers' order: a write is
        // either already persisted or marks this host, including during its read (D4).
        let _versions = self.0.invalidated.lock().unwrap();
        let watch = Arc::new(StoreWatch {
            changed: AtomicBool::new(pending()),
            wake: Mutex::new(None),
        });
        let mut hosts = self.0.hosts.lock().unwrap();
        hosts.retain(|host| host.strong_count() != 0);
        hosts.push(Arc::downgrade(&watch));
        watch
    }
    pub(super) fn store_written(&self) {
        let hosts: Vec<_> = self
            .0
            .hosts
            .lock()
            .unwrap()
            .iter()
            .filter_map(|host| host.upgrade())
            .collect();
        for host in hosts {
            host.changed();
        }
    }
}
