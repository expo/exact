//! D2: within one process, an address is held by one listener at a time,
//! by the rule between processes. Each address keeps its listeners in
//! order, the holder last. Without `takeover` a newcomer waits just below
//! the holder; with it, the newcomer holds and the holder is displaced,
//! keeping its connections. When the holder lets go, the listener now last
//! is told to take the address.
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

/// What another listener of the process asks of this one, and how to wake it.
pub struct Signal {
    /// A newer listener took the address over.
    pub displaced: AtomicBool,
    /// The holder let go: take the address.
    pub acquire: AtomicBool,
    /// The listener is closing.
    pub stop: AtomicBool,
    wake: Box<dyn Fn() + Send + Sync>,
}

impl Signal {
    /// A signal that calls `wake` when set.
    pub fn new(wake: impl Fn() + Send + Sync + 'static) -> Signal {
        Signal {
            displaced: AtomicBool::new(false),
            acquire: AtomicBool::new(false),
            stop: AtomicBool::new(false),
            wake: Box::new(wake),
        }
    }

    fn raise(&self, flag: &AtomicBool) {
        flag.store(true, Ordering::SeqCst);
        (self.wake)();
    }

    /// Ask the listener to close.
    pub fn stop(&self) {
        self.raise(&self.stop);
    }
}

type Holders = Vec<(u64, Arc<Signal>)>;

struct Entry {
    holders: Holders,
    /// Serializes same-process publish/unpublish/rebind operations on one path.
    /// Lock order: `Entry::publish` before `HOLDS`.
    publish: Arc<Mutex<()>>,
}

static HOLDS: Mutex<Vec<(PathBuf, Entry)>> = Mutex::new(Vec::new());
static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

/// A listener's place on its address.
pub struct Hold {
    path: PathBuf,
    id: u64,
    publish: Arc<Mutex<()>>,
}

/// What a newcomer does first.
#[derive(Debug, PartialEq, Eq)]
pub enum Start {
    /// Bind: nobody in the process holds it.
    Bind,
    /// Bind over the holder, which is displaced.
    TakeOver,
    /// Wait for the holder to let go.
    Wait,
}

/// Join the listeners of `path`.
pub fn enter(path: &Path, takeover: bool, signal: Arc<Signal>) -> (Hold, Start) {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut holds = HOLDS.lock().unwrap();
    let at = match holds.iter().position(|(p, _)| p == path) {
        Some(at) => at,
        None => {
            holds.push((
                path.to_path_buf(),
                Entry {
                    holders: Vec::new(),
                    publish: Arc::new(Mutex::new(())),
                },
            ));
            holds.len() - 1
        }
    };
    let entry = &mut holds[at].1;
    let publish = entry.publish.clone();
    let holders = &mut entry.holders;
    let start = match holders.last() {
        None => {
            holders.push((id, signal));
            Start::Bind
        }
        Some((_, holder)) if takeover => {
            holder.raise(&holder.displaced);
            holders.push((id, signal));
            Start::TakeOver
        }
        Some(_) => {
            holders.insert(holders.len() - 1, (id, signal));
            Start::Wait
        }
    };
    let hold = Hold {
        path: path.to_path_buf(),
        id,
        publish,
    };
    (hold, start)
}

impl Hold {
    /// Whether this listener holds its address in the process.
    pub fn holding(&self) -> bool {
        let holds = HOLDS.lock().unwrap();
        (holds.iter().find(|(p, _)| *p == self.path))
            .and_then(|(_, e)| e.holders.last())
            .is_some_and(|(id, _)| *id == self.id)
    }

    /// Run `f` under this path's in-process publication lock.
    pub fn with_publish<R>(&self, f: impl FnOnce() -> R) -> R {
        let _publish = self.publish.lock().unwrap();
        f()
    }

    /// Run `f` under this path's publication lock only if this listener still
    /// holds the address. Clears stale transition flags before checking
    /// [`Self::holding`] so a displacement after the check is never lost.
    pub fn publish_if_holding<R>(&self, signal: &Signal, f: impl FnOnce() -> R) -> Option<R> {
        let _publish = self.publish.lock().unwrap();
        signal.displaced.store(false, Ordering::SeqCst);
        signal.acquire.store(false, Ordering::SeqCst);
        if !self.holding() {
            return None;
        }
        Some(f())
    }

    /// Let go: when this listener held the address, the one now last takes it.
    pub fn leave(self) {
        let mut holds = HOLDS.lock().unwrap();
        let Some(at) = holds.iter().position(|(p, _)| *p == self.path) else {
            return;
        };
        let holders = &mut holds[at].1.holders;
        let held = holders.last().is_some_and(|(id, _)| *id == self.id);
        holders.retain(|(id, _)| *id != self.id);
        match holders.last() {
            Some((_, next)) if held => next.raise(&next.acquire),
            Some(_) => {}
            None => {
                holds.remove(at);
            }
        }
    }
}
