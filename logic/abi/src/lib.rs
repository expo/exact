//! Versioned, bounded data-only module seam. No Rust allocation or layout crosses it.
//! @ref LLP 1029.000 — Rust logic replacement after first pixel.

use exact_plan::{
    bytes::{Reader, Writer},
    Plan, Value,
};
use exact_runner::{
    Answer, CacheMode, DataError, DataSource, FailureKind, HttpScheduling, LongPoll, Outcome,
    Overlaid, Redirect, Request, Response, Socket, SocketCall, SocketOp, Store, SurfaceOutcome,
    SurfaceRequest, Write,
};

pub mod draw;

/// Seam version, independent of the plan format version.
pub const ABI: u32 = 9;
/// Maximum request or response bytes, enforced on both sides.
pub const MAX_MESSAGE: usize = exact_runner::MAX_HOST_WORK_BYTES;

fn error(e: impl std::fmt::Debug) -> String {
    format!("logic ABI: {e:?}")
}
fn bytes(w: &mut Writer, data: &[u8]) {
    w.u32(data.len() as u32);
    w.bytes(data);
}
fn read_bytes<'a>(r: &mut Reader<'a>) -> Result<&'a [u8], String> {
    let n = r.count().map_err(error)?;
    r.bytes(n).map_err(error)
}
fn pairs(w: &mut Writer, data: &[(String, String)]) {
    w.u32(data.len() as u32);
    for (a, b) in data {
        w.string(a);
        w.string(b);
    }
}
fn read_pairs(r: &mut Reader<'_>) -> Result<Vec<(String, String)>, String> {
    let n = r.count().map_err(error)?;
    (0..n)
        .map(|_| Ok((r.string().map_err(error)?, r.string().map_err(error)?)))
        .collect()
}
// A store value is length-prefixed bytes, as a string is
fn store_value(w: &mut Writer, value: &[u8]) {
    w.u32(value.len() as u32);
    w.bytes(value);
}
fn read_store_value(r: &mut Reader<'_>) -> Result<Vec<u8>, String> {
    let n = r.count().map_err(error)?;
    Ok(r.bytes(n).map_err(error)?.to_vec())
}
fn store_pairs(w: &mut Writer, data: &[(String, Vec<u8>)]) {
    w.u32(data.len() as u32);
    for (name, value) in data {
        w.string(name);
        store_value(w, value);
    }
}
fn read_store_pairs(r: &mut Reader<'_>) -> Result<Vec<(String, Vec<u8>)>, String> {
    let n = r.count().map_err(error)?;
    (0..n)
        .map(|_| Ok((r.string().map_err(error)?, read_store_value(r)?)))
        .collect()
}
fn finish(w: Writer) -> Result<Vec<u8>, String> {
    if w.len() > MAX_MESSAGE {
        Err("logic ABI message too large".into())
    } else {
        Ok(w.into_vec())
    }
}
fn header(op: u8) -> Writer {
    let mut w = Writer::default();
    w.u32(ABI);
    w.u8(op);
    w
}
fn reader(data: &[u8]) -> Result<Reader<'_>, String> {
    if data.len() > MAX_MESSAGE {
        return Err("logic ABI message too large".into());
    }
    let mut r = Reader::new(data);
    if r.u32().map_err(error)? != ABI {
        return Err("logic ABI version differs".into());
    }
    Ok(r)
}
fn end(r: &Reader<'_>) -> Result<(), String> {
    if r.is_empty() {
        Ok(())
    } else {
        Err("logic ABI trailing bytes".into())
    }
}

/// Read-only metadata request; creating a session and dispatching it is post-pixel.
pub fn metadata_request() -> Vec<u8> {
    header(0).into_vec()
}
/// Bind the plan's declared source shapes before activation.
pub fn bind_request(plan: &[u8]) -> Result<Vec<u8>, String> {
    let mut w = header(1);
    bytes(&mut w, plan);
    finish(w)
}
/// Activate the app's data implementation.
pub fn activate_request() -> Vec<u8> {
    header(2).into_vec()
}
/// Encode an answer or parse operation with a grant-filtered snapshot.
pub fn call_request(
    store: &Store,
    source: &str,
    args: &[Value],
    outcome: Option<&Outcome>,
) -> Result<Vec<u8>, String> {
    let mut w = header(if outcome.is_some() { 4 } else { 3 });
    w.string(source);
    Value::list(args.to_vec()).encode(&mut w);
    w.u32(store.granted().len() as u32);
    for name in store.granted() {
        w.string(name);
    }
    let snapshot = store.call_snapshot(store.granted());
    store_pairs(&mut w, &snapshot);
    if let Some(outcome) = outcome {
        encode_outcome(&mut w, outcome);
    }
    encode_socket_turn(&mut w, store);
    finish(w)
}

/// The call's socket turn (LLP 1016.003 D5): which sockets it may send on,
/// and each open socket's resource, ticket and buffered bytes.
fn encode_socket_turn(w: &mut Writer, store: &Store) {
    match store.socket_call() {
        SocketCall::None => w.u8(0),
        SocketCall::Mutation => w.u8(1),
        SocketCall::Mapper(own) => {
            w.u8(2);
            w.u32(own.resource() as u32);
            w.u64(own.ticket());
        }
    }
    let open = store.socket_open();
    w.u32(open.len() as u32);
    for &(resource, ticket) in open {
        let buffered = (store.socket_buffered().iter())
            .find(|(i, _)| *i == resource)
            .map_or(0, |(_, n)| *n);
        w.u32(resource as u32);
        w.u64(ticket);
        w.u64(buffered);
    }
}
fn read_socket_turn(r: &mut Reader<'_>, store: &mut Store) -> Result<(), String> {
    let call = match r.u8().map_err(error)? {
        0 => SocketCall::None,
        1 => SocketCall::Mutation,
        2 => {
            let resource = r.u32().map_err(error)? as usize;
            SocketCall::Mapper(Socket::new(resource, r.u64().map_err(error)?))
        }
        _ => return Err("invalid socket call".into()),
    };
    let n = r.count().map_err(error)?;
    let (mut open, mut buffered) = (Vec::with_capacity(n), Vec::with_capacity(n));
    for _ in 0..n {
        let resource = r.u32().map_err(error)? as usize;
        open.push((resource, r.u64().map_err(error)?));
        buffered.push((resource, r.u64().map_err(error)?));
    }
    store.set_socket_call(call);
    store.set_socket_open(open);
    store.set_socket_buffered(buffered);
    Ok(())
}

/// What a call staged on sockets, by resource, and a mapper's overload.
fn encode_socket_ops(w: &mut Writer, store: &mut Store) {
    // A socket handle stages nothing else (`Socket::send`); an operation a
    // source staged itself on the store's lower door fails the call.
    let (ops, other): (Vec<_>, Vec<_>) = (store.take_sockets().into_iter()).partition(|(_, op)| {
        matches!(
            op,
            SocketOp::Text(_) | SocketOp::Binary(_) | SocketOp::Close { .. }
        )
    });
    if !other.is_empty() {
        store.note_socket_overload(
            "a Rust module's socket operation is text, binary or a close".into(),
        );
    }
    w.u32(ops.len() as u32);
    for (resource, op) in ops {
        w.u32(resource as u32);
        match op {
            SocketOp::Text(text) => {
                w.u8(0);
                w.string(&text);
            }
            SocketOp::Binary(data) => {
                w.u8(1);
                bytes(w, &data);
            }
            SocketOp::Close { code, reason } => {
                w.u8(2);
                w.u16(code);
                w.string(&reason);
            }
            _ => unreachable!("partitioned above"),
        }
    }
    w.u8(u8::from(store.socket_overload().is_some()));
    w.string(store.socket_overload().unwrap_or(""));
}
#[allow(clippy::type_complexity)]
fn read_socket_ops(r: &mut Reader<'_>) -> Result<(Vec<(usize, SocketOp)>, Option<String>), String> {
    let n = r.count().map_err(error)?;
    let mut ops = Vec::with_capacity(n);
    for _ in 0..n {
        let resource = r.u32().map_err(error)? as usize;
        let op = match r.u8().map_err(error)? {
            0 => SocketOp::Text(r.string().map_err(error)?),
            1 => SocketOp::Binary(read_bytes(r)?.to_vec()),
            2 => SocketOp::Close {
                code: r.u16().map_err(error)?,
                reason: r.string().map_err(error)?,
            },
            _ => return Err("invalid socket operation".into()),
        };
        ops.push((resource, op));
    }
    let overload = match (r.u8().map_err(error)?, r.string().map_err(error)?) {
        (0, _) => None,
        (1, why) => Some(why),
        _ => return Err("invalid socket overload".into()),
    };
    Ok((ops, overload))
}

/// Encode an overlay operation (additive, as later outcome tags were): the
/// resource's answer and the writes laid over it. No store crosses it.
pub fn overlay_request(
    source: &str,
    args: &[Value],
    answer: &Value,
    writes: &[Write<'_>],
) -> Result<Vec<u8>, String> {
    let mut w = header(5);
    w.string(source);
    Value::list(args.to_vec()).encode(&mut w);
    answer.encode(&mut w);
    w.u32(writes.len() as u32);
    for write in writes {
        w.u64(write.id);
        w.string(write.mutation);
        w.string(write.source);
        Value::list(write.args.to_vec()).encode(&mut w);
        match write.reply {
            Some(reply) => {
                w.u8(1);
                reply.encode(&mut w);
            }
            None => w.u8(0),
        }
        w.u8(u8::from(write.answered));
    }
    finish(w)
}

/// Decode an overlay reply: the value shown and the answered writes it
/// keeps, or none (the answer shows).
pub fn overlay_reply(data: &[u8]) -> Result<Option<Overlaid>, DataError> {
    let decode = || -> Result<Result<Option<Overlaid>, DataError>, String> {
        let mut r = reader(data)?;
        if r.u8().map_err(error)? != 3 {
            return Err("expected overlay reply".into());
        }
        let result = match r.u8().map_err(error)? {
            0 => Ok(None),
            1 => {
                let value = Value::decode(&mut r).map_err(error)?;
                let count = r.count().map_err(error)?;
                let keep = (0..count)
                    .map(|_| r.u64().map_err(error))
                    .collect::<Result<Vec<_>, _>>()?;
                Ok(Some(Overlaid { value, keep }))
            }
            2 => Err(DataError::Unavailable(r.string().map_err(error)?)),
            3 => Err(DataError::UnknownSource(r.string().map_err(error)?)),
            _ => return Err("invalid overlay tag".into()),
        };
        end(&r)?;
        Ok(result)
    };
    decode().map_err(DataError::Interface)?
}

fn encode_outcome(w: &mut Writer, outcome: &Outcome) {
    match outcome {
        Outcome::Storage(payload) => {
            w.u8(5);
            bytes(w, payload);
        }
        Outcome::Surface(SurfaceOutcome::Captured(payload)) => {
            w.u8(6);
            bytes(w, payload);
        }
        Outcome::Surface(SurfaceOutcome::Restored) => w.u8(7),
        Outcome::Response(r) => {
            w.u8(0);
            w.u16(r.status);
            pairs(w, &r.headers);
            bytes(w, &r.body);
        }
        // One message of a stream (LLP 1016.000): tag 8, or 12 for a
        // socket's binary frame (LLP 1016.003 D5).
        Outcome::Message(m) => {
            w.u8(if m.binary.is_some() { 12 } else { 8 });
            w.string(&m.event);
            w.string(&m.id);
            match &m.binary {
                Some(data) => bytes(w, data),
                None => w.string(&m.data),
            }
            w.u32(m.coalesced);
            w.string(&m.protocol);
        }
        // The runner parses a batch one message at a time: none crosses.
        Outcome::Messages(_) => {
            w.u8(3);
            w.string("a source parses a stream's messages one at a time");
        }
        Outcome::Closed {
            code,
            reason,
            was_clean,
        } => {
            w.u8(11);
            w.u16(*code);
            w.string(reason);
            w.u8(u8::from(*was_clean));
        }
        Outcome::Failed { kind, message } => {
            w.u8(match kind {
                FailureKind::Network => 1,
                FailureKind::Refused => 2,
                FailureKind::Unsupported => 3,
                FailureKind::Aborted => 4,
                // Additive, as tags 5–8 were; a Rust module never asks for a
                // deadline across this seam (below), so it never sees one.
                FailureKind::Timeout => 9,
                FailureKind::NotFound => 10,
            });
            w.string(message);
        }
    }
}
fn read_outcome(r: &mut Reader<'_>) -> Result<Outcome, String> {
    let tag = r.u8().map_err(error)?;
    if tag == 0 {
        return Ok(Outcome::Response(Response {
            status: r.u16().map_err(error)?,
            headers: read_pairs(r)?,
            body: read_bytes(r)?.to_vec(),
        }));
    }
    if tag == 5 {
        return Ok(Outcome::Storage(read_bytes(r)?.to_vec()));
    }
    if tag == 6 {
        return Ok(Outcome::Surface(SurfaceOutcome::Captured(
            read_bytes(r)?.to_vec(),
        )));
    }
    if tag == 7 {
        return Ok(Outcome::Surface(SurfaceOutcome::Restored));
    }
    if tag == 8 || tag == 12 {
        let (event, id) = (r.string().map_err(error)?, r.string().map_err(error)?);
        let (data, binary) = match tag {
            8 => (r.string().map_err(error)?, None),
            _ => (String::new(), Some(read_bytes(r)?.to_vec())),
        };
        return Ok(Outcome::Message(exact_runner::Message {
            event,
            id,
            data,
            coalesced: r.u32().map_err(error)?,
            protocol: r.string().map_err(error)?.into(),
            binary,
        }));
    }
    if tag == 11 {
        return Ok(Outcome::Closed {
            code: r.u16().map_err(error)?,
            reason: r.string().map_err(error)?,
            was_clean: match r.u8().map_err(error)? {
                0 => false,
                1 => true,
                _ => return Err("invalid close".into()),
            },
        });
    }
    let kind = match tag {
        1 => FailureKind::Network,
        2 => FailureKind::Refused,
        3 => FailureKind::Unsupported,
        4 => FailureKind::Aborted,
        9 => FailureKind::Timeout,
        10 => FailureKind::NotFound,
        _ => return Err("invalid outcome tag".into()),
    };
    Ok(Outcome::Failed {
        kind,
        message: r.string().map_err(error)?,
    })
}
fn encode_result(w: &mut Writer, result: Result<Answer, DataError>) {
    match result {
        Ok(Answer::Later(r)) if r.prefetch_refusal().is_some() => encode_result(
            w,
            Err(DataError::Unavailable(r.prefetch_refusal().unwrap().into())),
        ),
        Ok(Answer::Later(r)) if r.preload_refusal().is_some() => encode_result(
            w,
            Err(DataError::Unavailable(r.preload_refusal().unwrap().into())),
        ),
        // A long poll that is no stream's independent GET, or with bad terms (LLP
        // 1016.000 D8), is refused before it crosses, ahead of every other kind.
        Ok(Answer::Later(r))
            if r.long_poll.is_some()
                && (r.long_poll_refusal().is_some()
                    || r.storage.is_some()
                    || r.http == HttpScheduling::Ordered) =>
        {
            let why = r
                .long_poll_refusal()
                .unwrap_or("exactLongPoll: a long poll is a stream's independent HTTP request")
                .to_string();
            encode_result(w, Err(DataError::Unavailable(why)))
        }
        Ok(Answer::Later(r))
            if (r.storage.is_some() || r.continuation.is_some() || r.surface.is_some())
                && r.http != HttpScheduling::Ordered =>
        {
            encode_result(
                w,
                Err(DataError::Unavailable(
                    "independent scheduling is HTTP-only".into(),
                )),
            );
        }
        // The request encoding has no deadline: refused rather than dropped,
        // so a Rust module's timeout never silently becomes none.
        Ok(Answer::Later(r)) if r.timeout_ms.is_some() => encode_result(
            w,
            Err(DataError::Unavailable(
                "a request timeout cannot cross the Rust module seam".into(),
            )),
        ),
        // Nor a stream's idle timeout (LLP 1016.000 D11).
        Ok(Answer::Later(r)) if r.idle_timeout_ms.is_some() => encode_result(
            w,
            Err(DataError::Unavailable(
                "an idle timeout cannot cross the Rust module seam".into(),
            )),
        ),
        // Nor a held mark (LLP 1016.000 D9): refused, never silently waited on.
        Ok(Answer::Later(r)) if r.held => encode_result(
            w,
            Err(DataError::Unavailable(
                "a held request cannot cross the Rust module seam yet".into(),
            )),
        ),
        // Nor an upload's progress: refused, never sent as a plain stream.
        Ok(Answer::Later(r)) if r.upload_progress => encode_result(
            w,
            Err(DataError::Unavailable(
                "an upload's progress cannot cross the Rust module seam yet".into(),
            )),
        ),
        // Nor has it a file body (LLP 1108 D6 R2): refused, never sent empty.
        Ok(Answer::Later(r)) if r.body_from.is_some() => encode_result(
            w,
            Err(DataError::Unavailable(
                "exactBodyFrom cannot cross the Rust module seam".into(),
            )),
        ),
        // Nor a body written to a file: refused, never read into memory instead.
        Ok(Answer::Later(r)) if r.body_to.is_some() => encode_result(
            w,
            Err(DataError::Unavailable(
                "exactBodyTo cannot cross the Rust module seam".into(),
            )),
        ),
        // A two-way socket crosses as a WebSocket alone (LLP 1016.003 D5),
        // and a listener (LLP 1133.005) not at all, socket mark or none.
        Ok(Answer::Later(r))
            if r.listen.is_some() || (r.socket && (!r.stream || r.is_process())) =>
        {
            encode_result(
                w,
                Err(DataError::Unavailable(
                    "a listener's or a child's socket cannot cross the Rust module seam".into(),
                )),
            )
        }
        // Nor a byte stream: refused, never read as events instead.
        Ok(Answer::Later(r)) if r.bytes => encode_result(
            w,
            Err(DataError::Unavailable(
                "a byte stream cannot cross the Rust module seam yet".into(),
            )),
        ),
        Ok(Answer::Now(v)) => {
            w.u8(0);
            v.encode(w);
        }
        Ok(Answer::Later(r))
            if r.continuation.is_none()
                && r.storage.is_none()
                && r.surface.is_some()
                && r.method == "GET"
                && r.url.is_empty()
                && r.headers.is_empty()
                && r.body.is_empty() =>
        {
            match r.surface.as_deref().unwrap() {
                SurfaceRequest::Capture { name } => {
                    w.u8(7);
                    w.string(name);
                }
                SurfaceRequest::Restore {
                    name,
                    bytes: payload,
                } => {
                    w.u8(8);
                    w.string(name);
                    bytes(w, payload);
                }
            }
            w.u8(u8::from(r.grants.is_some()));
            w.string(r.grants.as_deref().unwrap_or(""));
        }
        Ok(Answer::Later(r))
            if r.continuation.is_none() && r.surface.is_none() && r.storage.is_some() =>
        {
            w.u8(5);
            bytes(w, r.storage.as_ref().unwrap());
            w.u8(u8::from(r.grants.is_some()));
            w.string(r.grants.as_deref().unwrap_or(""));
        }
        // The seam's HTTP kinds carry no redirect mode yet: a module asking
        // for one is refused, never silently followed.
        Ok(Answer::Later(r)) if r.redirect != Redirect::Follow => encode_result(
            w,
            Err(DataError::Unavailable(
                "a redirect mode cannot cross the Rust module seam yet".into(),
            )),
        ),
        Ok(Answer::Later(r))
            if r.continuation.is_none() && r.surface.is_none() && r.storage.is_none() =>
        {
            // Additive HTTP result kind. Older hosts reject tag 6 before
            // executing effects; it must never silently decode as ordered.
            // A stream (tag 9) is always independent (LLP 1016.000); a long
            // poll (tag 10, D8) is a stream with its terms after the body; a
            // two-way socket (tag 11, LLP 1016.003) is a stream.
            // Prefetch wraps the HTTP result and its other markers (LLP 1162.000 D5).
            if r.prefetch {
                w.u8(14);
            }
            if let Some(name) = &r.preload {
                w.u8(13);
                w.string(name);
            }
            if !r.socket && r.cache != CacheMode::Unspecified {
                w.u8(12);
                w.string(r.cache.name());
            }
            let poll = r.long_poll.as_ref().filter(|_| r.stream);
            match r.http {
                HttpScheduling::Ordered => w.u8(1),
                HttpScheduling::Independent { max_response_bytes } => {
                    w.u8(match (r.stream, poll) {
                        (true, Some(_)) => 10,
                        (true, None) if r.socket => 11,
                        (true, None) => 9,
                        _ => 6,
                    });
                    w.u32(max_response_bytes);
                }
            }
            w.u8(u8::from(r.grants.is_some()));
            w.string(r.grants.as_deref().unwrap_or(""));
            w.string(&r.method);
            w.string(&r.url);
            pairs(w, &r.headers);
            bytes(w, &r.body);
            if r.stream {
                w.u32(r.protocols.len() as u32);
                for name in &r.protocols {
                    w.string(name);
                }
            }
            if let (Some(p), HttpScheduling::Independent { .. }) = (poll, r.http) {
                w.string(&p.cursor);
                w.string(&p.from);
                w.u8(u8::from(p.same_cursor_event));
            }
        }
        Ok(Answer::Later(r)) if r.continuation.is_some() => encode_result(
            w,
            Err(DataError::Unavailable(
                "executor-local continuations cannot cross the Rust module seam".into(),
            )),
        ),
        Ok(Answer::Later(_)) => encode_result(
            w,
            Err(DataError::Unavailable(
                "request combines multiple host-work kinds".into(),
            )),
        ),
        Err(e) => {
            let (tag, message) = match e {
                DataError::UnknownSource(s) => (2, s),
                DataError::BadArguments(s) => (3, s),
                // A coded failure's class is the TypeScript seam's and the
                // runner's (LLP 1109 D3): a Rust source's own is `error`.
                // (an overrun is exact-js's; a Rust module's seam never makes one)
                DataError::Unavailable(s)
                | DataError::Interface(s)
                | DataError::DeferredAtBake { message: s, .. }
                | DataError::Overrun(s)
                | DataError::Failed(_, s, _) => (4, s),
            };
            w.u8(tag);
            w.string(&message);
        }
    }
}
fn read_result(r: &mut Reader<'_>) -> Result<Result<Answer, DataError>, String> {
    let mut tag = r.u8().map_err(error)?;
    let prefetch = tag == 14;
    if prefetch {
        tag = r.u8().map_err(error)?;
        if !matches!(tag, 1 | 12 | 13) {
            return Err("invalid prefetch HTTP result tag".into());
        }
    }
    let preload = if tag == 13 {
        let name = r.string().map_err(error)?;
        tag = r.u8().map_err(error)?;
        if !matches!(tag, 1 | 6 | 9..=12) {
            return Err("invalid preload HTTP result tag".into());
        }
        Some(name)
    } else {
        None
    };
    let mut answer = match tag {
        0 => Ok(Answer::Now(Value::decode(r).map_err(error)?)),
        1 | 6 | 9..=11 => Ok(Answer::Later(read_http(r, tag)?)),
        12 => {
            let cache = CacheMode::parse(Some(&r.string().map_err(error)?))?;
            let tag = r.u8().map_err(error)?;
            if !matches!(tag, 1 | 6 | 9 | 10) {
                return Err("invalid cached HTTP result tag".into());
            }
            let mut request = read_http(r, tag)?;
            request.cache = cache;
            Ok(Answer::Later(request))
        }
        5 => {
            let payload = read_bytes(r)?.to_vec();
            let present = r.u8().map_err(error)?;
            let scope = r.string().map_err(error)?;
            let mut request = Request::storage(payload);
            request.grants = match present {
                0 => None,
                1 => Some(scope),
                _ => return Err("invalid grant scope tag".into()),
            };
            Ok(Answer::Later(request))
        }
        tag @ (7 | 8) => {
            let name = r.string().map_err(error)?;
            let payload = if tag == 8 {
                Some(read_bytes(r)?.to_vec())
            } else {
                None
            };
            let present = r.u8().map_err(error)?;
            let scope = r.string().map_err(error)?;
            let mut request = match payload {
                Some(bytes) => Request::restore_surface(name, bytes),
                None => Request::capture_surface(name),
            };
            request.grants = match present {
                0 => None,
                1 => Some(scope),
                _ => return Err("invalid grant scope tag".into()),
            };
            Ok(Answer::Later(request))
        }
        2 => Err(DataError::UnknownSource(r.string().map_err(error)?)),
        3 => Err(DataError::BadArguments(r.string().map_err(error)?)),
        4 => Err(DataError::Unavailable(r.string().map_err(error)?)),
        _ => return Err("invalid result tag".into()),
    };
    if let (Some(name), Ok(Answer::Later(request))) = (preload, &mut answer) {
        request.preload = Some(name.into_boxed_str());
        if let Some(why) = request.preload_refusal() {
            return Err(why.into());
        }
    }
    if prefetch {
        let Ok(Answer::Later(request)) = &mut answer else {
            return Err("invalid prefetch HTTP result".into());
        };
        request.prefetch = true;
        if let Some(why) = request.prefetch_refusal() {
            return Err(why.into());
        }
    }
    Ok(answer)
}

/// An HTTP result's request (tags 1, 6 and 9–11): a stream's, a long poll's
/// terms after its body (LLP 1016.000 D8), a two-way socket's (LLP 1016.003).
fn read_http(r: &mut Reader<'_>, tag: u8) -> Result<Request, String> {
    let mut request = Request {
        stream: tag >= 9,
        socket: tag == 11,
        bytes: false,
        upload_progress: false,
        long_poll: None,
        timeout_ms: None,
        idle_timeout_ms: None,
        protocols: Vec::new(),
        body_from: None,
        body_to: None,
        redirect: Redirect::Follow,
        cache: Default::default(),
        preload: None,
        prefetch: false,
        held: false,
        reach: Default::default(),
        listen: None,
        due_ms: None,
        wake_ms: None,
        // A Rust source names no module fetch (the TypeScript prelude does).
        fetch: None,
        http: if tag != 1 {
            let limit = r.u32().map_err(error)?;
            if limit == 0 || limit > 64 << 20 {
                return Err("invalid independent HTTP response limit".into());
            }
            HttpScheduling::Independent {
                max_response_bytes: limit,
            }
        } else {
            HttpScheduling::Ordered
        },
        continuation: None,
        storage: None,
        surface: None,
        grants: {
            let present = r.u8().map_err(error)?;
            let s = r.string().map_err(error)?;
            match present {
                0 => None,
                1 => Some(s),
                _ => return Err("invalid grant scope tag".into()),
            }
        },
        method: r.string().map_err(error)?,
        url: r.string().map_err(error)?,
        headers: read_pairs(r)?,
        body: read_bytes(r)?.to_vec(),
    };
    if tag >= 9 {
        for _ in 0..r.count().map_err(error)? {
            request.protocols.push(r.string().map_err(error)?);
        }
        request.check_protocols()?;
    }
    if tag == 10 {
        let (cursor, from) = (r.string().map_err(error)?, r.string().map_err(error)?);
        let same = r.u8().map_err(error)?;
        if same > 1 {
            return Err("invalid long poll terms".into());
        }
        request.long_poll = Some(Box::new(LongPoll {
            cursor,
            from,
            same_cursor_event: same == 1,
        }));
        if let Some(why) = request.long_poll_refusal() {
            return Err(why.into());
        }
    }
    Ok(request)
}

/// Decode identity exported by the actual instantiated module.
pub fn metadata_reply(data: &[u8]) -> Result<(String, String), String> {
    let mut r = reader(data)?;
    if r.u8().map_err(error)? != 0 {
        return Err("expected metadata reply".into());
    }
    let result = (r.string().map_err(error)?, r.string().map_err(error)?);
    end(&r)?;
    Ok(result)
}
/// Decode a bind or activate acknowledgement.
pub fn unit_reply(data: &[u8]) -> Result<(), String> {
    let mut r = reader(data)?;
    if r.u8().map_err(error)? != 1 {
        return Err("expected unit reply".into());
    }
    match read_result(&mut r)? {
        Ok(Answer::Now(Value::Unit)) => {}
        other => return Err(error(other)),
    };
    end(&r)
}
/// Validate the complete reply before applying its ordered, grant-checked writes.
/// An observed read remains observed even when the module returns a data error.
pub fn call_reply(data: &[u8], store: &mut Store) -> Result<Answer, DataError> {
    let decode = || -> Result<_, String> {
        let mut r = reader(data)?;
        if r.u8().map_err(error)? != 2 {
            return Err("expected call reply".into());
        }
        let observed = match r.u8().map_err(error)? {
            0 => false,
            1 => true,
            _ => return Err("invalid read flag".into()),
        };
        let count = r.count().map_err(error)?;
        let mut writes = Vec::new();
        for _ in 0..count {
            let name = r.string().map_err(error)?;
            if name.starts_with(Store::KEPT) || !store.granted().contains(&name) {
                return Err(format!("module wrote ungranted secret {name}"));
            }
            let value = match r.u8().map_err(error)? {
                0 => None,
                1 => Some(r.string().map_err(error)?),
                _ => return Err("invalid write tag".into()),
            };
            writes.push((name, value));
        }
        let sockets = read_socket_ops(&mut r)?;
        let result = read_result(&mut r)?;
        end(&r)?;
        Ok((observed, writes, sockets, result))
    };
    let (observed, writes, (ops, overload), result) = decode().map_err(DataError::Unavailable)?;
    if observed {
        store.observe_external_read();
    }
    for (name, value) in writes {
        match value {
            Some(value) => store.set(&name, &value)?,
            None => store.forget(&name)?,
        };
    }
    // What the module staged, through the same checks on the turn the
    // runner set (LLP 1016.003 D5); a socket it named is open here too.
    for (resource, op) in ops {
        let open = store.socket_open().iter().find(|(i, _)| *i == resource);
        let ticket = open.map_or(u64::MAX, |(_, t)| *t);
        Socket::new(resource, ticket).send(store, op)?;
    }
    if let Some(why) = overload {
        store.note_socket_overload(why);
    }
    result
}

/// A module-owned session; used by the export macro, useful for codec fixtures.
pub struct Session<D> {
    data: D,
    output: Vec<u8>,
}
impl<D: DataSource> Session<D> {
    /// Construct one data source for one host session.
    pub fn new(data: D) -> Self {
        Self {
            data,
            output: Vec::new(),
        }
    }
    /// Borrow the last output until the next dispatch.
    pub fn output(&self) -> &[u8] {
        &self.output
    }
    /// Dispatch one bounded byte request; no filesystem host capabilities exist.
    pub fn dispatch(&mut self, input: &[u8]) -> Result<(), String> {
        let mut r = reader(input)?;
        let op = r.u8().map_err(error)?;
        let mut w = header(match op {
            0 => 0,
            1 | 2 => 1,
            3 | 4 => 2,
            5 => 3,
            _ => return Err("unknown logic operation".into()),
        });
        match op {
            0 => {
                end(&r)?;
                w.string(self.data.app_id());
                w.string(self.data.grants());
            }
            1 => {
                let plan = Plan::decode(read_bytes(&mut r)?).map_err(error)?;
                end(&r)?;
                self.data.bind(&plan);
                encode_result(&mut w, Ok(Answer::Now(Value::Unit)));
            }
            2 => {
                end(&r)?;
                encode_result(
                    &mut w,
                    self.data.activate().map(|_| Answer::Now(Value::Unit)),
                );
            }
            3 | 4 => {
                let source = r.string().map_err(error)?;
                let Value::List(args) = Value::decode(&mut r).map_err(error)? else {
                    return Err("arguments must be a list".into());
                };
                let count = r.count().map_err(error)?;
                let scope = (0..count)
                    .map(|_| {
                        r.string()
                            .map(|name| format!("secret.keep {name}"))
                            .map_err(error)
                    })
                    .collect::<Result<Vec<_>, _>>()?
                    .join("\n");
                let mut store = Store::module_mirror(self.data.grants(), read_store_pairs(&mut r)?);
                let outcome = if op == 4 {
                    Some(read_outcome(&mut r)?)
                } else {
                    None
                };
                read_socket_turn(&mut r, &mut store)?;
                end(&r)?;
                let result = store.with_grants(&scope, |store| {
                    if let Some(outcome) = outcome {
                        self.data.parse(store, &source, &args, outcome)
                    } else {
                        self.data.answer(store, &source, &args)
                    }
                });
                // An answer that read a locked presence name with `get` fails,
                // as it does in process (LLP 1133.010 D5).
                let result = match store.take_locked_read() {
                    Some(name) => Err(exact_runner::StoreError::Locked(name).into()),
                    None => result,
                };
                w.u8(u8::from(store.reads() > 0));
                let writes = store.take_writes();
                w.u32(writes.len() as u32);
                for write in writes {
                    w.string(&write.name);
                    w.u8(u8::from(write.value.is_some()));
                    if let Some(value) = write.value {
                        store_value(&mut w, &value);
                    }
                }
                encode_socket_ops(&mut w, &mut store);
                encode_result(&mut w, result);
            }
            5 => {
                let source = r.string().map_err(error)?;
                let Value::List(args) = Value::decode(&mut r).map_err(error)? else {
                    return Err("arguments must be a list".into());
                };
                let answer = Value::decode(&mut r).map_err(error)?;
                let count = r.count().map_err(error)?;
                let mut decoded = Vec::with_capacity(count);
                for _ in 0..count {
                    let id = r.u64().map_err(error)?;
                    let mutation = r.string().map_err(error)?;
                    let source = r.string().map_err(error)?;
                    let Value::List(args) = Value::decode(&mut r).map_err(error)? else {
                        return Err("arguments must be a list".into());
                    };
                    let reply = match r.u8().map_err(error)? {
                        0 => None,
                        1 => Some(Value::decode(&mut r).map_err(error)?),
                        _ => return Err("invalid reply tag".into()),
                    };
                    let answered = r.u8().map_err(error)? == 1;
                    decoded.push((id, mutation, source, args, reply, answered));
                }
                end(&r)?;
                let writes: Vec<Write<'_>> = decoded
                    .iter()
                    .map(|(id, mutation, source, args, reply, answered)| Write {
                        id: *id,
                        mutation,
                        source,
                        args,
                        reply: reply.as_ref(),
                        answered: *answered,
                    })
                    .collect();
                match self.data.overlay(&source, &args, &answer, &writes) {
                    Ok(None) => w.u8(0),
                    Ok(Some(o)) => {
                        w.u8(1);
                        o.value.encode(&mut w);
                        w.u32(o.keep.len() as u32);
                        for id in o.keep {
                            w.u64(id);
                        }
                    }
                    // A source this module does not answer: a mixed app's
                    // other half may overlay it.
                    Err(DataError::UnknownSource(name)) => {
                        w.u8(3);
                        w.string(&name);
                    }
                    Err(e) => {
                        w.u8(2);
                        w.string(&format!("{e:?}"));
                    }
                }
            }
            _ => unreachable!(),
        }
        self.output = finish(w)?;
        Ok(())
    }
}

/// Export this app's data-only module as a native cdylib or standalone wasm.
/// `$new` constructs fresh state per session. Call only on the session's thread.
/// Input allocations must be freed by this module's deallocator, output is borrowed.
#[macro_export]
macro_rules! export {
    ($data:ty, $new:expr) => {
        $crate::export!(@exports $data, $new, false);
    };
    // Declares that all observable state lives in host inputs/Store/requests.
    // Construction, binding, activation and destruction must be effect-free;
    // parse must depend only on its explicit arguments/outcome. Derived caches
    // may be rebuilt. This is an author contract, not a proof by the runtime.
    ($data:ty, $new:expr, stateless) => {
        $crate::export!(@exports $data, $new, true);
    };
    (@exports $data:ty, $new:expr, $stateless:expr) => {
        #[no_mangle]
        pub extern "C" fn exact_logic_stateless() -> u32 {
            u32::from($stateless)
        }
        #[no_mangle]
        pub extern "C" fn exact_logic_abi() -> u32 {
            $crate::ABI
        }
        #[no_mangle]
        pub extern "C" fn exact_logic_create() -> usize {
            Box::into_raw(Box::new($crate::Session::<$data>::new($new))) as usize
        }
        #[no_mangle]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn exact_logic_destroy(session: usize) {
            if session != 0 {
                drop(Box::from_raw(session as *mut $crate::Session<$data>));
            }
        }
        #[no_mangle]
        pub extern "C" fn exact_logic_alloc(len: u32) -> usize {
            if len as usize > $crate::MAX_MESSAGE {
                return 0;
            }
            Box::into_raw(vec![0u8; len as usize].into_boxed_slice()) as *mut u8 as usize
        }
        #[no_mangle]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn exact_logic_dealloc(ptr: usize, len: u32) {
            if ptr != 0 {
                drop(Box::from_raw(std::ptr::slice_from_raw_parts_mut(
                    ptr as *mut u8,
                    len as usize,
                )));
            }
        }
        #[no_mangle]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn exact_logic_call(session: usize, ptr: usize, len: u32) -> u32 {
            if session == 0 || ptr == 0 || len as usize > $crate::MAX_MESSAGE {
                return 1;
            }
            let input = std::slice::from_raw_parts(ptr as *const u8, len as usize);
            u32::from(
                (&mut *(session as *mut $crate::Session<$data>))
                    .dispatch(input)
                    .is_err(),
            )
        }
        #[no_mangle]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn exact_logic_output(session: usize) -> usize {
            (&*(session as *const $crate::Session<$data>))
                .output()
                .as_ptr() as usize
        }
        #[no_mangle]
        #[allow(clippy::missing_safety_doc)]
        pub unsafe extern "C" fn exact_logic_output_len(session: usize) -> u32 {
            (&*(session as *const $crate::Session<$data>))
                .output()
                .len() as u32
        }
    };
}

#[cfg(test)]
mod socket_tests;
#[cfg(test)]
mod tests;
