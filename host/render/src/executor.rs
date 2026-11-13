//! One render's executor: the native core the Apple and Linux hosts share
//! (grant-checked transport, ordered and independent lanes, continuations on
//! its workers), with a wake the rendering thread waits on instead of a UI
//! loop. Dropping it aborts whatever is still in flight.
pub use exact_executor::pump;
use exact_executor::{Core, SharedHttpCache, StreamHost, WorkerBudget};
use exact_runner::{CompletionTiming, Outcome, RequestOut, Work};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Instant;
/// Each test thread shares a worker budget among its servers and renders. The
/// dev-dependency enables this for integration tests; production shares one process bound.
const BUDGET: WorkerBudget = if cfg!(any(test, feature = "test-budget")) {
    WorkerBudget::Thread
} else {
    WorkerBudget::Process
};

thread_local! {
    static HTTP_CACHE: std::cell::RefCell<Option<SharedHttpCache>> = const { std::cell::RefCell::new(None) };
}

pub(crate) fn set_http_cache(cache: SharedHttpCache) {
    HTTP_CACHE.with(|current| *current.borrow_mut() = Some(cache));
}

fn http_cache() -> SharedHttpCache {
    static BUILD: std::sync::OnceLock<SharedHttpCache> = std::sync::OnceLock::new();
    HTTP_CACHE
        .with(|current| current.borrow().clone())
        .unwrap_or_else(|| BUILD.get_or_init(SharedHttpCache::new).clone())
}

/// The native core with a condition variable for its wake.
pub struct Executor {
    core: Core,
    wake: Arc<(Mutex<bool>, Condvar)>,
}

impl Executor {
    /// Start the core's owners under `grants` (the render's, never more
    /// than the app's).
    pub fn start(grants: &str) -> Self {
        // @ref LLP 1048.000 D10 — a server, not an app on a device: its
        // fetches go over rustls on macOS too (Charlie, 2026-09-29), not
        // NSURLSession, whose per-call cost was ~30% of a RealWorld page's CPU.
        // `EXACT_RENDER_TRANSPORT=platform` keeps the platform's (a check
        // against the platform, or a way back).
        let platform = std::env::var("EXACT_RENDER_TRANSPORT").as_deref() == Ok("platform");
        let transport: StreamHost = if platform {
            Arc::new(ibex2::host::Host::new)
        } else {
            Arc::new(|| {
                let host = ibex2::host::Host::with_transport(Box::new(
                    ibex2::transport::RustlsHttpTransport::new(),
                ));
                // Under conformance's network (`EXACT_NET_PROXY`) a Mac's sockets are the
                // portable ones too, which take the run's proxy; the platform's do not
                if cfg!(target_vendor = "apple") && std::env::var_os("EXACT_NET_PROXY").is_some() {
                    host.with_socket_transport(Box::new(
                        ibex2::transport::websocket::TcpSocketTransport::new(),
                    ))
                } else {
                    host
                }
            })
        };
        let host = transport();
        let bindings = ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants))
            .ok()
            .map(|g| host.endow(g));
        let wake = Arc::new((Mutex::new(false), Condvar::new()));
        let signal = wake.clone();
        let core = Core::start_on(
            bindings,
            grants,
            Box::new(move || {
                // Called under the core's lock: only record and signal.
                let (woken, ready) = &*signal;
                *woken.lock().unwrap() = true;
                ready.notify_all();
            }),
            transport,
            BUDGET,
        );
        core.set_shared_http_cache(http_cache());
        Self { core, wake }
    }

    /// Where an app's files are, for a request whose body is one of them
    /// (LLP 1108 D6 R2). A render serves no app files and never sets them,
    /// so such a request is refused; an embedder that has them may.
    pub fn set_app_roots(&self, roots: [std::path::PathBuf; 3]) {
        self.core.set_app_roots(roots);
    }

    /// Admit work, or refuse it (a limit, the core retired).
    pub fn run(&self, request: RequestOut, work: Option<Work>) -> Result<(), &'static str> {
        self.core.run_owned(request, work)
    }

    /// Acknowledge the wake before draining a bounded group of replies.
    pub fn begin_pump(&self) {
        self.core.begin_pump();
    }

    /// Take the next ready completion; more ready outcomes wake again.
    pub fn drain(&self) -> Vec<(u64, Outcome, Option<CompletionTiming>)> {
        self.core.drain()
    }

    /// Lift the ordered lane's fence after a refusal, admitting the ordered
    /// requests held behind it; returns those refused at a limit.
    pub fn resume_ordered(&self) -> Vec<(u64, &'static str)> {
        self.core.resume_ordered()
    }

    /// No admitted ordered work is in flight or undrained.
    pub fn ordered_idle(&self) -> bool {
        self.core.ordered_idle()
    }

    /// Settle a re-ask (`Dispatch::Again`) in its ordered place, with no
    /// work (LLP 1041 §8.4, amended 2026-10-09); kept pending while the
    /// window is full. An error is terminal, for the host to record.
    pub fn again(&self, request: &RequestOut) -> Result<(), &'static str> {
        self.core.again(request.ticket)
    }
    /// Let go of the work for tickets the runner no longer holds.
    pub fn forget(&self, held: impl Fn(u64) -> bool) {
        self.core.forget(held);
    }
    /// The uploads let go that had begun to go out, for
    /// `Runner::uploads_started` (LLP 1016.000 D10).
    pub fn let_go_started(&self, held: impl Fn(u64) -> bool) -> Vec<u64> {
        self.core.let_go_started(held)
    }
    /// Cancel the requests the app aborted, whatever their method.
    pub fn cancel(&self, tickets: &[u64]) {
        self.core.cancel(tickets);
    }

    /// Wait until the core has news, or until `until`.
    pub fn wait(&self, until: Instant) {
        // A direct drain may have consumed the wake: rearm before a later
        // stream message must wake this waiter (LLP 1162 D3).
        self.core.begin_pump();
        let (woken, ready) = &*self.wake;
        let mut guard = woken.lock().unwrap();
        while !*guard {
            let now = Instant::now();
            if now >= until {
                return;
            }
            guard = ready.wait_timeout(guard, until - now).unwrap().0;
        }
        *guard = false;
    }
}
