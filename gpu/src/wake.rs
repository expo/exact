//! Thread-safe redraw wakes for resting GPU surfaces (LLP 1009 D12).
//!
//! A surface whose [`crate::Surface::render`] returned `false` receives no
//! further frames until inputs or host state change, unless it signals
//! [`SurfaceWaker::wake`] from any thread (for example when a background
//! worker finishes preparing data).
//!
//! State machine per surface instance:
//! - `IDLE (0) -> WOKEN (1)`: first wake fires the host notification once.
//! - `WOKEN (1) -> WOKEN (1)`: coalesced; no redundant host notification,
//!   while the `AcqRel` RMW preserves happens-before for concurrent writers.
//! - `WOKEN (1) -> IDLE (0)`: claimed by the module immediately before
//!   `Surface::render`; any wake arriving during `render` re-arms `WOKEN`.
//! - `* -> DEAD (2)`: latched on surface destroy or module drop; further
//!   wakes are silent no-ops and can never resurrect the surface.

use std::sync::{
    atomic::{AtomicU8, Ordering},
    Arc,
};

const IDLE: u8 = 0;
const WOKEN: u8 = 1;
const DEAD: u8 = 2;

/// Handle given to [`crate::Surface::set_waker`] to request a frame from any
/// thread when a resting surface has new work to render (LLP 1009 D12).
///
/// Cloning shares the same underlying wake state. Calling [`Self::wake`] after
/// the surface is destroyed or the module unloads is a safe no-op.
#[derive(Clone, Debug)]
pub struct SurfaceWaker {
    state: Arc<AtomicU8>,
}

impl SurfaceWaker {
    /// Request a frame on this surface's host thread.
    ///
    /// Multiple wakes before the next render coalesce into one host
    /// notification. Under a seekable agent clock, the pending wake settles
    /// on the next clock advance rather than spinning on wall time.
    pub fn wake(&self) {
        // `IDLE (0) < WOKEN (1) < DEAD (2)`: `fetch_max(WOKEN, AcqRel)` sets
        // `WOKEN` from `IDLE`, keeps `WOKEN` with a release-sequence RMW for
        // coalesced writers, and never overwrites `DEAD`.
        if self.state.fetch_max(WOKEN, Ordering::AcqRel) == IDLE {
            notify();
        }
    }
}

/// Per-instance wake state owned by [`crate::Instance`].
pub(crate) struct WakeState {
    state: Arc<AtomicU8>,
}

impl WakeState {
    pub(crate) fn new() -> (Self, SurfaceWaker) {
        let state = Arc::new(AtomicU8::new(IDLE));
        (
            Self {
                state: Arc::clone(&state),
            },
            SurfaceWaker { state },
        )
    }

    pub(crate) fn is_woken(&self) -> bool {
        self.state.load(Ordering::Acquire) == WOKEN
    }

    /// Consume a pending wake right before `Surface::render`.
    pub(crate) fn take(&self) -> bool {
        self.state
            .compare_exchange(WOKEN, IDLE, Ordering::AcqRel, Ordering::Acquire)
            .is_ok()
    }

    /// Re-arm the local wake latch without re-notifying the host when an
    /// early check consumed a wake before a fall-through render path.
    pub(crate) fn rearm(&self) {
        let _ = self
            .state
            .compare_exchange(IDLE, WOKEN, Ordering::AcqRel, Ordering::Acquire);
    }

    /// Permanently silence this instance's waker on destroy or module drop.
    pub(crate) fn kill(&self) {
        self.state.store(DEAD, Ordering::Release);
    }
}

#[cfg(not(target_arch = "wasm32"))]
mod host {
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NOTIFY: AtomicUsize = AtomicUsize::new(0);
    static IN_FLIGHT: AtomicUsize = AtomicUsize::new(0);

    struct InFlightGuard;
    impl Drop for InFlightGuard {
        fn drop(&mut self) {
            IN_FLIGHT.fetch_sub(1, Ordering::SeqCst);
        }
    }

    pub(super) fn notify() {
        IN_FLIGHT.fetch_add(1, Ordering::SeqCst);
        let _guard = InFlightGuard;
        let address = NOTIFY.load(Ordering::SeqCst);
        if address != 0 {
            // SAFETY: only `on_wake` stores here, and only an `extern "C" fn()`.
            let callback: extern "C" fn() = unsafe { std::mem::transmute(address) };
            callback();
        }
    }

    /// Register (or, with `None`, remove and drain) the presenter's wake
    /// callback (`gpu_on_wake`).
    pub(crate) fn on_wake(callback: Option<extern "C" fn()>) {
        NOTIFY.store(callback.map_or(0, |f| f as usize), Ordering::SeqCst);
        if callback.is_none() {
            while IN_FLIGHT.load(Ordering::SeqCst) != 0 {
                std::hint::spin_loop();
            }
        }
    }

    /// Clear and drain the wake callback before tearing down the module.
    pub(crate) fn unload() {
        on_wake(None);
    }

    #[cfg(test)]
    pub(super) fn notify_registered() -> bool {
        NOTIFY.load(Ordering::SeqCst) != 0
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use host::{on_wake, unload};

#[cfg(not(target_arch = "wasm32"))]
fn notify() {
    host::notify();
}

#[cfg(target_arch = "wasm32")]
fn notify() {
    crate::web::notify_wake();
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Barrier, Mutex,
    };

    static TEST_LOCK: Mutex<()> = Mutex::new(());
    static HITS: AtomicUsize = AtomicUsize::new(0);

    struct ResetGuard;
    impl Drop for ResetGuard {
        fn drop(&mut self) {
            on_wake(None);
        }
    }

    extern "C" fn count_hit() {
        HITS.fetch_add(1, Ordering::SeqCst);
    }

    #[test]
    fn coalesces_repeated_wakes_until_taken() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = ResetGuard;
        HITS.store(0, Ordering::SeqCst);
        on_wake(Some(count_hit));

        let (state, waker) = WakeState::new();
        assert!(!state.is_woken());
        for _ in 0..10_000 {
            waker.wake();
        }
        assert!(state.is_woken());
        assert_eq!(HITS.load(Ordering::SeqCst), 1);

        assert!(state.take());
        assert!(!state.is_woken());
        assert!(!state.take());

        waker.wake();
        waker.wake();
        assert!(state.is_woken());
        assert_eq!(HITS.load(Ordering::SeqCst), 2);
    }

    #[test]
    fn kill_silences_further_wakes_permanently() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = ResetGuard;
        HITS.store(0, Ordering::SeqCst);
        on_wake(Some(count_hit));

        let (state, waker) = WakeState::new();
        let clone = waker.clone();
        state.kill();
        waker.wake();
        clone.wake();
        assert!(!state.is_woken());
        assert!(!state.take());
        assert_eq!(HITS.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn coalesced_writer_happens_before_take() {
        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = ResetGuard;
        on_wake(None);

        for _ in 0..64 {
            let (state, waker) = WakeState::new();
            let value = Arc::new(AtomicUsize::new(0));
            let published = Arc::new(std::sync::atomic::AtomicBool::new(false));
            // Pre-arm WOKEN so the worker takes the coalesced `WOKEN -> WOKEN` path.
            waker.wake();
            let w = waker.clone();
            let v = Arc::clone(&value);
            let done = Arc::clone(&published);
            let t = std::thread::spawn(move || {
                v.store(42, Ordering::Relaxed);
                w.wake();
                done.store(true, Ordering::Release);
            });
            while !published.load(Ordering::Acquire) {
                std::hint::spin_loop();
            }
            // Take without joining first: synchronization comes solely from the
            // AcqRel fetch_max -> AcqRel compare_exchange release sequence.
            assert!(state.take());
            assert_eq!(value.load(Ordering::Relaxed), 42);
            t.join().unwrap();
        }
    }

    #[test]
    fn clearing_callback_waits_for_in_flight_notification() {
        static ENTERED: Mutex<Option<Arc<Barrier>>> = Mutex::new(None);
        static RELEASE: Mutex<Option<Arc<Barrier>>> = Mutex::new(None);

        extern "C" fn slow_cb() {
            if let Some(b) = ENTERED.lock().unwrap().clone() {
                b.wait();
            }
            if let Some(b) = RELEASE.lock().unwrap().clone() {
                b.wait();
            }
        }

        let _lock = TEST_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let _reset = ResetGuard;
        let entered = Arc::new(Barrier::new(2));
        let release = Arc::new(Barrier::new(2));
        let clearer_started = Arc::new(Barrier::new(2));
        *ENTERED.lock().unwrap() = Some(Arc::clone(&entered));
        *RELEASE.lock().unwrap() = Some(Arc::clone(&release));
        on_wake(Some(slow_cb));

        let (_state, waker) = WakeState::new();
        let caller = std::thread::spawn(move || waker.wake());
        entered.wait();

        let cleared = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let cleared_flag = Arc::clone(&cleared);
        let started = Arc::clone(&clearer_started);
        let clearer = std::thread::spawn(move || {
            started.wait();
            on_wake(None);
            cleared_flag.store(true, Ordering::SeqCst);
        });
        clearer_started.wait();
        // Wait until on_wake(None) has cleared NOTIFY and entered the IN_FLIGHT drain loop.
        while host::notify_registered() {
            std::hint::spin_loop();
        }
        assert!(
            !cleared.load(Ordering::SeqCst),
            "on_wake(None) returned while a wake callback was still running"
        );
        release.wait();
        caller.join().unwrap();
        clearer.join().unwrap();
        assert!(cleared.load(Ordering::SeqCst));
        *ENTERED.lock().unwrap() = None;
        *RELEASE.lock().unwrap() = None;
    }
}
