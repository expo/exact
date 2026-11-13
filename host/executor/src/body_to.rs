//! A reply's body written to an app file (LLP 1108 D6 R2, ruled 2026-10-26):
//! `fetch(url, {exactBodyTo: "app:/…"})` from TypeScript, `Request::body_to`
//! from Rust. Before the request is sent, the worker admits `fs.write` on the
//! path and creates a staging file beside it through the app's pinned
//! directory handles (Ibex patch 19), so a denied path or a missing parent is
//! the request's failure and nothing is sent. A 2xx body goes to the staging
//! file a chunk at a time as it arrives, never held whole, and the staging
//! file replaces the target once the body is complete. A failure, an abort,
//! the deadline or a body over the bound drops the staging file, and the
//! target is left as it was. A reply that is not 2xx is read into the
//! response as any reply is, at most [`BODY_TO_HELD_BYTES`], and the file is
//! untouched. The bytes written travel as the response's [`BODY_WRITTEN`]
//! header.
//!
//! Memory and the lane: the request is charged what it can hold, a reply that
//! is not 2xx (`lib.rs` `reservation`), never the bound on what it
//! writes, so a large download neither waits for the lane's whole budget nor
//! is refused for it. An independent reply read into memory, written to no
//! file, goes into one buffer of its ceiling, allocated before the first
//! read: the lane charges it that ceiling once, where a growing buffer could
//! reach twice it.
//!
//! Time: the writes run on the worker between the transport's reads, so the
//! deadline and an abort stop the body at its next chunk; a write stalled on
//! the disk holds this request's worker until it returns.
use super::body;
use exact_runner::{
    FailureKind, HttpScheduling, Outcome, Request, BODY_TO_HELD_BYTES, BODY_WRITTEN,
    MAX_BODY_TO_BYTES,
};
use ibex2::boundary::HostError;
use ibex2::stdlib::app_fs::Staged;
use ibex2::stdlib::fetch::{over_limit, Body, Response, StreamingResponse};

/// What one read of the body takes before it is written.
const CHUNK: usize = 64 << 10;

/// The most of a reply `request` may read: its `max_response_bytes`, else
/// 1 GiB for a body written to a file and 64 MiB for any other.
pub(super) fn limit(request: &Request) -> usize {
    match request.http {
        HttpScheduling::Independent { max_response_bytes } => max_response_bytes as usize,
        HttpScheduling::Ordered if request.body_to.is_some() => MAX_BODY_TO_BYTES as usize,
        HttpScheduling::Ordered => super::MAX_BODY,
    }
}

/// The staging file for `request`'s body, made before the request is sent;
/// `None` for a request whose body is not written to a file.
pub(super) fn stage(
    roots: body::Roots<'_>,
    grants: &str,
    request: &Request,
) -> Result<Option<Staged>, Outcome> {
    let Some(path) = request.body_to.as_deref() else {
        return Ok(None);
    };
    if let Some(why) = request.body_to_refusal() {
        return Err(Outcome::Failed {
            kind: FailureKind::Refused,
            message: why.into(),
        });
    }
    let what = "exactBodyTo";
    let (granted, directories) =
        body::open_as(what, roots, grants, request.grants.as_deref(), path)?;
    directories
        .stage(&granted, path)
        .map(Some)
        .map_err(|e| body::named_as(what, path, e))
}

/// The response: with `staged`, a 2xx body written there and published, its
/// size in the [`BODY_WRITTEN`] header and no body; any other reply read into
/// memory, at most [`BODY_TO_HELD_BYTES`]. Without `staged`, the body read
/// into memory: `within` its ceiling, as the lane charged an independent
/// reply (`reads_within_ceiling`), or else as it grows. Past `limit` either
/// is [`over_limit`]`(limit)`, as a reply read into memory is.
pub(super) fn receive(
    mut r: StreamingResponse,
    staged: Option<Staged>,
    limit: usize,
    within: bool,
) -> Result<Response, HostError> {
    let (body, headers) = match staged {
        Some(mut staged) if r.ok() => {
            let (mut chunk, mut written) = (vec![0; CHUNK], 0u64);
            loop {
                let n = r.body.read(&mut chunk)?;
                if n == 0 {
                    break;
                }
                staged.write_all(&chunk[..n])?;
                written += n as u64;
            }
            staged.publish()?;
            let mut headers = r.headers;
            headers.set(BODY_WRITTEN, &written.to_string());
            (Vec::new(), headers)
        }
        // Not 2xx: the staging file is dropped, and the target left as it was.
        Some(_) => {
            let held = limit.min(BODY_TO_HELD_BYTES as usize);
            (read_within(&mut r.body, held, limit)?, r.headers)
        }
        None if within => (read_within(&mut r.body, limit, limit)?, r.headers),
        None => return r.collect(),
    };
    Ok(Response {
        status: r.status,
        status_text: r.status_text,
        headers,
        body,
        url: r.url,
        redirected: r.redirected,
    })
}

/// `body` read into one buffer of `cap` bytes, allocated once and never
/// grown, then trimmed to its length; one byte more is
/// [`over_limit`]`(limit)`.
fn read_within(body: &mut Body, cap: usize, limit: usize) -> Result<Vec<u8>, HostError> {
    let mut bytes = Vec::new();
    bytes
        .try_reserve_exact(cap)
        .map_err(|_| HostError::Failed("HTTP response allocation failed".into()))?;
    let mut filled = 0;
    loop {
        let at = filled;
        if at == cap {
            if body.read(&mut [0])? == 0 {
                break;
            }
            body.cancel();
            return Err(over_limit(limit));
        }
        // Keep the initialized tail after a short read, so the next read
        // initializes only the additional bytes it can fill.
        bytes.resize((at + CHUNK).min(cap), 0);
        let n = body.read(&mut bytes[at..])?;
        filled += n;
        if n == 0 {
            break;
        }
    }
    bytes.truncate(filled);
    bytes.shrink_to_fit();
    Ok(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ibex2::stdlib::fetch::BodySource;
    use std::sync::{atomic::AtomicUsize, atomic::Ordering, Arc};

    struct ShortReads {
        reads: usize,
        initialized: Arc<AtomicUsize>,
    }

    impl BodySource for ShortReads {
        fn read(&mut self, out: &mut [u8]) -> Result<usize, HostError> {
            self.initialized
                .fetch_add(out.iter().filter(|b| **b == 0).count(), Ordering::Relaxed);
            // Mark the entire window, including the tail a short read leaves,
            // so a later zero counts initialization of that byte again.
            out.fill(1);
            self.reads += 1;
            Ok(usize::from(self.reads <= 3))
        }
    }

    #[test]
    fn short_reads_initialize_no_more_than_the_buffer_capacity() {
        let cap = CHUNK * 2;
        let initialized = Arc::new(AtomicUsize::new(0));
        let source = ShortReads {
            reads: 0,
            initialized: initialized.clone(),
        };
        let mut body = Body::new(Box::new(source), cap, Default::default());
        assert_eq!(read_within(&mut body, cap, cap).unwrap(), [1; 3]);
        let initialized = initialized.load(Ordering::Relaxed);
        assert!(
            initialized <= cap,
            "initialized {initialized} bytes for {cap}"
        );
    }
}
