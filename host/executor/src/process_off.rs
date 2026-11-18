//! Desktop processes where no host starts programs (iOS, tvOS, Android,
//! Windows until LLP 1133.007's stage 3): every process request answers
//! `Unsupported` (§8), and none of the supervision is linked, so an app
//! there carries no process code (§14).
use super::{complete, failed, Core, Shared};
use exact_runner::{FailureKind, Request, SocketOp};
use ibex2::stdlib::abort::AbortController;
use std::sync::{Arc, Condvar};

#[cfg(windows)]
#[path = "folders_windows.rs"]
mod folders;

pub(crate) struct Processes {
    pub(crate) drained: Condvar,
    #[cfg(windows)]
    roots: std::sync::OnceLock<ibex2::stdlib::fs::Roots>,
}

impl Processes {
    pub(crate) fn new(_budget: super::WorkerBudget) -> Processes {
        Processes {
            drained: Condvar::new(),
            #[cfg(windows)]
            roots: Default::default(),
        }
    }
}

pub(super) fn run(
    shared: &Arc<Shared>,
    ticket: u64,
    _request: Request,
    _grants: &str,
    _host: &dyn Fn() -> ibex2::host::Host,
    _abort: AbortController,
) {
    let why = "this host starts no programs (LLP 1133.007 §8)";
    complete(shared, ticket, failed(FailureKind::Unsupported, why));
}

/// No child is running, so a batch naming one names an ended incarnation.
pub(crate) fn check(_shared: &Shared, ops: &[(u64, &SocketOp)]) -> Result<(), String> {
    match ops.is_empty() {
        true => Ok(()),
        false => Err("InvalidStateError: the child's incarnation has ended".into()),
    }
}

pub(crate) fn send(_shared: &Shared, _ops: &[(u64, &SocketOp)]) -> Vec<(u64, u64, u64)> {
    Vec::new()
}

pub(crate) fn queued(_shared: &Shared, _ticket: u64) -> Option<u64> {
    None
}

pub(crate) fn quit(_shared: &Shared) {}

pub(crate) fn launched(_shared: &Arc<Shared>) {}

/// A one-shot answers at once here, so it needs no room.
pub(crate) fn admit_one_shot(
    _shared: &Shared,
    _ticket: u64,
    _ceiling: usize,
) -> Result<(), &'static str> {
    Ok(())
}

pub(crate) fn release_one_shot(_shared: &Shared, _ticket: u64) {}

impl Core {
    /// This target starts no programs. Windows installs its storage resolver.
    pub fn allow_processes(&self) {
        #[cfg(windows)]
        {
            let roots = self.shared.processes.roots.get_or_init(folders::from_env);
            ibex2::stdlib::fs::set_roots(roots.clone());
        }
    }

    /// None: there is no process work to keep a clock for.
    pub fn work_clock(&self) -> Option<Arc<dyn exact_runner::WorkClock>> {
        None
    }
}
