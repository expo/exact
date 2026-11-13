//! What an outcome keeps until the UI takes it, a fetch's request and
//! headers as the transport takes and gives them, and how its failure is
//! worded: shared by the workers and the asynchronous core.
use super::{failed, MAX_HEADERS};
use exact_runner::{FailureKind, Outcome, Request};
use ibex2::stdlib::abort::AbortController;

pub(super) fn bounded_outcome(outcome: Outcome, limit: usize) -> Outcome {
    if retained(&outcome) > limit {
        failed(
            FailureKind::Refused,
            "native outcome exceeds retention limit",
        )
    } else {
        outcome
    }
}

/// The bytes an outcome keeps until the UI takes it.
pub(super) fn retained(outcome: &Outcome) -> usize {
    match outcome {
        Outcome::Response(r) => r
            .body
            .capacity()
            .saturating_add(
                r.headers
                    .capacity()
                    .saturating_mul(std::mem::size_of::<(String, String)>()),
            )
            .saturating_add(
                r.headers
                    .iter()
                    .map(|(k, v)| k.capacity().saturating_add(v.capacity()))
                    .sum::<usize>(),
            ),
        Outcome::Storage(b) => b.capacity(),
        Outcome::Message(_) | Outcome::Messages(_) => (outcome.messages().iter())
            .map(|m| {
                m.data
                    .capacity()
                    .saturating_add(m.event.capacity())
                    .saturating_add(m.id.capacity())
                    .saturating_add(m.binary.as_ref().map_or(0, Vec::capacity))
            })
            .fold(0, usize::saturating_add),
        Outcome::Closed { reason, .. } => reason.capacity(),
        Outcome::Surface(exact_runner::SurfaceOutcome::Captured(b)) => b.capacity(),
        Outcome::Surface(exact_runner::SurfaceOutcome::Restored) => 0,
        Outcome::Failed { message, .. } => message.capacity(),
    }
}

pub(super) fn fetch_failure(e: ibex2::boundary::HostError, abort: &AbortController) -> Outcome {
    match e {
        _ if abort.signal().aborted() => failed(FailureKind::Aborted, "native request aborted"),
        ibex2::boundary::HostError::Denied { capability } => failed(
            FailureKind::Refused,
            format!("outside the app's grants ({capability})"),
        ),
        // The origin a redirect led to, which the grants lack (podcast F5).
        ibex2::boundary::HostError::DeniedRedirect { capability, origin } => failed(
            FailureKind::Refused,
            format!("outside the app's grants ({capability}): redirected to {origin}"),
        ),
        // A tool's, a wake's or a package event's reach that could prompt
        // (LLP 1133.036 D6): never sent.
        ibex2::boundary::HostError::NotAllowed { reason } => {
            failed(FailureKind::Refused, format!("NotAllowedError: {reason}"))
        }
        e => failed(
            FailureKind::Network,
            e.to_string().chars().take(2048).collect::<String>(),
        ),
    }
}

/// The transport's request for a runner's.
pub(super) fn fetch_request(request: Request, forced: bool) -> ibex2::stdlib::fetch::Request {
    let mut req = ibex2::stdlib::fetch::Request::get(&request.url);
    req.method = request.method;
    req.redirect = match request.redirect {
        exact_runner::Redirect::Follow => ibex2::stdlib::fetch::RedirectMode::Follow,
        exact_runner::Redirect::Manual => ibex2::stdlib::fetch::RedirectMode::Manual,
        exact_runner::Redirect::Error => ibex2::stdlib::fetch::RedirectMode::Error,
    };
    for (k, v) in &request.headers {
        req.headers.append(k, v);
    }
    if forced {
        req.headers.set("cache-control", "no-cache");
    }
    let mode = if forced && request.cache != exact_runner::CacheMode::NoStore {
        exact_runner::CacheMode::Reload
    } else {
        request.cache
    };
    match mode {
        exact_runner::CacheMode::NoCache if !req.headers.has("cache-control") => {
            req.headers.set("cache-control", "max-age=0")
        }
        exact_runner::CacheMode::NoStore | exact_runner::CacheMode::Reload => {
            if !req.headers.has("pragma") {
                req.headers.set("pragma", "no-cache");
            }
            if !req.headers.has("cache-control") {
                req.headers.set("cache-control", "no-cache");
            }
        }
        _ => {}
    }
    if !request.body.is_empty() {
        req.body = Some(request.body);
    }
    req
}

pub(super) fn check_headers(
    r: ibex2::stdlib::fetch::StreamingResponse,
) -> Result<ibex2::stdlib::fetch::StreamingResponse, ibex2::boundary::HostError> {
    let headers = r.headers.entries();
    let bytes = headers.iter().try_fold(0usize, |n, (k, v)| {
        n.checked_add(k.len())?.checked_add(v.len())
    });
    if bytes.is_none_or(|n| n > MAX_HEADERS) || headers.len() > 1024 {
        return Err(ibex2::boundary::HostError::Failed(
            "HTTP headers exceed limit".into(),
        ));
    }
    Ok(r)
}

/// A count a measurement build sets (LLP 1120 §5's arms: `EXACT_HTTP_WORKERS`,
/// `EXACT_HTTP_IN_FLIGHT`), or `default`.
pub(super) const fn build_number(set: Option<&str>, default: usize) -> usize {
    let Some(text) = set else {
        return default;
    };
    let bytes = text.as_bytes();
    let (mut n, mut i): (usize, usize) = (0, 0);
    while i < bytes.len() {
        assert!(bytes[i].is_ascii_digit(), "a build count is digits");
        n = match n.checked_mul(10) {
            Some(tens) => match tens.checked_add((bytes[i] - b'0') as usize) {
                Some(next) => next,
                None => panic!("a build count past usize"),
            },
            None => panic!("a build count past usize"),
        };
        i += 1;
    }
    assert!(n > 0, "a build count is at least 1");
    n
}

/// A request's deadline: a thread that aborts the request when it passes,
/// and ends at once when the request finishes first (the sender drops).
#[cfg_attr(test, derive(Clone))]
pub(super) struct Deadline {
    passed: std::sync::Arc<std::sync::atomic::AtomicBool>,
    _done: std::sync::mpsc::Sender<()>,
    #[cfg(test)]
    abort: AbortController,
}

impl Deadline {
    pub(super) fn arm(ms: u32, abort: &AbortController) -> Result<Deadline, String> {
        let (done, wait) = std::sync::mpsc::channel::<()>();
        let passed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        #[cfg(test)]
        let controlled_abort = abort.clone();
        let (flag, abort) = (passed.clone(), abort.clone());
        std::thread::Builder::new()
            .name("exact-fetch-deadline".into())
            .spawn(move || {
                // A request the caller already aborted (a newer answer, an
                // unload) stays Aborted: the deadline did not end it.
                if let Err(std::sync::mpsc::RecvTimeoutError::Timeout) =
                    wait.recv_timeout(std::time::Duration::from_millis(ms.into()))
                {
                    if !abort.signal().aborted() {
                        flag.store(true, std::sync::atomic::Ordering::Release);
                        abort.abort();
                    }
                }
            })
            .map_err(|e| format!("the request's deadline could not be armed: {e}"))?;
        Ok(Deadline {
            passed,
            _done: done,
            #[cfg(test)]
            abort: controlled_abort,
        })
    }

    pub(super) fn passed(&self) -> bool {
        self.passed.load(std::sync::atomic::Ordering::Acquire)
    }

    #[cfg(test)]
    pub(super) fn expire(&self) {
        if !self.abort.signal().aborted() {
            self.passed
                .store(true, std::sync::atomic::Ordering::Release);
            self.abort.abort();
        }
    }
}

/// A WebSocket stream's idle timeout (LLP 1016.000 D11): a thread that
/// ends the stream when nothing has arrived for `ms`, measured from the
/// instant of each [`Quiet::heard`] and not from when the thread next runs.
///
/// The stream ends once, by whichever claims it first: the watch, which
/// then aborts the read, or its reader, with what it found
/// ([`Quiet::finish`]). So a close or a failure the reader already has is
/// never rewritten as a timeout while the socket is torn down, and the
/// watch never aborts a stream that has finished.
pub(super) struct Quiet {
    ms: u32,
    shared: std::sync::Arc<QuietState>,
    _done: std::sync::mpsc::Sender<()>,
}

struct QuietState {
    /// When something last arrived.
    heard: std::sync::Mutex<std::time::Instant>,
    /// [`ARMED`], then [`TIMED_OUT`] or [`FINISHED`], once.
    end: std::sync::atomic::AtomicU8,
}

const ARMED: u8 = 0;
const TIMED_OUT: u8 = 1;
const FINISHED: u8 = 2;

impl Quiet {
    pub(super) fn arm(ms: u32, abort: &AbortController) -> Result<Quiet, String> {
        use std::sync::atomic::Ordering;
        let (done, wait) = std::sync::mpsc::channel::<()>();
        let shared = std::sync::Arc::new(QuietState {
            heard: std::sync::Mutex::new(std::time::Instant::now()),
            end: std::sync::atomic::AtomicU8::new(ARMED),
        });
        let (state, abort) = (shared.clone(), abort.clone());
        let limit = std::time::Duration::from_millis(ms.into());
        std::thread::Builder::new()
            .name("exact-stream-quiet".into())
            .spawn(move || loop {
                // The time is read and the end claimed under the one lock an
                // arrival takes, so an arrival is either counted or comes
                // after the timeout, never between the two.
                let heard = state.heard.lock().unwrap();
                let left = (*heard + limit).saturating_duration_since(std::time::Instant::now());
                if left.is_zero() {
                    // A stream already let go stays Aborted: the quiet did
                    // not end it. The claim is made before the abort, so the
                    // reader the abort wakes finds it.
                    let claimed = !abort.signal().aborted()
                        && (state.end)
                            .compare_exchange(ARMED, TIMED_OUT, Ordering::AcqRel, Ordering::Acquire)
                            .is_ok();
                    drop(heard);
                    if claimed {
                        abort.abort();
                    }
                    return;
                }
                drop(heard);
                // Woken early only by the stream's end (the sender drops).
                if let Err(std::sync::mpsc::RecvTimeoutError::Disconnected) =
                    wait.recv_timeout(left)
                {
                    return;
                }
            })
            .map_err(|e| format!("the stream's idle timeout could not be armed: {e}"))?;
        Ok(Quiet {
            ms,
            shared,
            _done: done,
        })
    }

    /// Something arrived now: the opening, or a complete data message.
    pub(super) fn heard(&self) {
        *self.shared.heard.lock().unwrap() = std::time::Instant::now();
    }

    /// The stream's end: `end`, which its reader found, unless the watch
    /// ended it first, which is the timeout whatever the abort then produced
    /// (an aborted read, the peer's close). Call it before the socket is torn
    /// down.
    pub(super) fn finish(quiet: Option<&Quiet>, end: Outcome) -> Outcome {
        use std::sync::atomic::Ordering;
        let Some(q) = quiet else { return end };
        match (q.shared.end).compare_exchange(ARMED, FINISHED, Ordering::AcqRel, Ordering::Acquire)
        {
            Err(TIMED_OUT) => failed(
                FailureKind::Timeout,
                format!("nothing arrived for {} ms", q.ms),
            ),
            _ => end,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{failed, FailureKind, Outcome, Quiet};
    use ibex2::stdlib::abort::AbortController;
    use std::time::Duration;

    fn network() -> Outcome {
        failed(FailureKind::Network, "the socket closed (1006)")
    }

    /// LLP 1016.000 D11: the stream ends once. A reader that finishes first
    /// keeps what it found, however long its teardown takes, and the watch
    /// then aborts nothing; a watch that fires first is the timeout, whatever
    /// the reader's aborted read produced.
    #[test]
    fn a_quiet_watch_and_its_reader_settle_the_end_once() {
        let abort = AbortController::new();
        let quiet = Quiet::arm(400, &abort).unwrap();
        let end = Quiet::finish(Some(&quiet), network());
        assert!(
            matches!(
                end,
                Outcome::Failed {
                    kind: FailureKind::Network,
                    ..
                }
            ),
            "{end:?}"
        );
        std::thread::sleep(Duration::from_millis(600));
        assert!(
            !abort.signal().aborted(),
            "a finished stream is not aborted"
        );

        let abort = AbortController::new();
        let quiet = Quiet::arm(60, &abort).unwrap();
        std::thread::sleep(Duration::from_millis(150));
        assert!(abort.signal().aborted(), "the watch aborts the read");
        let end = Quiet::finish(Some(&quiet), network());
        assert!(
            matches!(&end, Outcome::Failed { kind: FailureKind::Timeout, message } if message == "nothing arrived for 60 ms"),
            "{end:?}"
        );

        // A stream let go first is not the watch's to end.
        let abort = AbortController::new();
        abort.abort();
        let quiet = Quiet::arm(20, &abort).unwrap();
        std::thread::sleep(Duration::from_millis(80));
        let end = Quiet::finish(Some(&quiet), network());
        assert!(
            matches!(
                end,
                Outcome::Failed {
                    kind: FailureKind::Network,
                    ..
                }
            ),
            "{end:?}"
        );
    }

    /// The time is counted from when something arrived, not from when the
    /// watch next ran.
    #[test]
    fn a_quiet_watch_counts_from_the_last_arrival() {
        let abort = AbortController::new();
        let quiet = Quiet::arm(1000, &abort).unwrap();
        std::thread::sleep(Duration::from_millis(600));
        quiet.heard();
        std::thread::sleep(Duration::from_millis(600));
        assert!(
            !abort.signal().aborted(),
            "1200 ms after arming, 600 ms after the arrival"
        );
        std::thread::sleep(Duration::from_millis(900));
        assert!(abort.signal().aborted(), "1500 ms after the arrival");
    }

    #[test]
    fn a_build_count_is_its_digits_or_the_default() {
        assert_eq!(super::build_number(None, 6), 6);
        assert_eq!(super::build_number(Some("7"), 6), 7);
        assert_eq!(super::build_number(Some("32"), 6), 32);
        for bad in ["", "0", "7a", "18446744073709551623"] {
            assert!(
                std::panic::catch_unwind(|| super::build_number(Some(bad), 6)).is_err(),
                "{bad}"
            );
        }
    }
}
