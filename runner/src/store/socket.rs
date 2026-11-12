//! A Rust source's handle on a resource's two-way socket (LLP 1016.003):
//! it stages on the store as a TypeScript handle does, with D7's checks, in
//! the calls the runner names before it makes them.
use super::Store;
use crate::runner::DataError;
use crate::SocketOp;

/// D7's bounds on one call's operations: their count and payload, and a
/// socket's buffered bytes with what the call staged on it.
const OPS: usize = 256;
const BYTES: usize = 1 << 20;

/// A resource's two-way socket as a Rust source holds it (LLP 1016.003 D3):
/// the resource that owns it and the stream ticket it was opened on.
/// [`Store::socket`] hands it to the socket's mapper; keep it by the
/// resource's arguments. It goes stale when that stream ends, which is when
/// the resource is asked again or let go too: a stale handle is refused,
/// never sent on the resource's newer socket.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Socket {
    resource: usize,
    ticket: u64,
}

/// Which sockets the call being made may send on (LLP 1016.003 D4). The
/// runner sets it before a call into the data source; the module seam
/// carries it to a Rust module.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SocketCall {
    /// No socket: a resource's answer, a turn of an answer in flight, an
    /// overlay, a bake.
    #[default]
    None,
    /// A mutation's answer: any open socket of the module's resources.
    Mutation,
    /// A socket's mapper answering its `open` or a message: that socket.
    Mapper(Socket),
}

fn invalid(why: &str) -> DataError {
    DataError::Unavailable(format!("InvalidStateError: {why}"))
}

impl Socket {
    /// The socket of resource `resource` opened on stream ticket `ticket`.
    /// Sources take theirs from [`Store::socket`]; the module seam rebuilds
    /// them from these parts.
    pub fn new(resource: usize, ticket: u64) -> Socket {
        Socket { resource, ticket }
    }

    /// The index of the resource that owns the socket.
    pub fn resource(self) -> usize {
        self.resource
    }

    /// The stream ticket the socket was opened on.
    pub fn ticket(self) -> u64 {
        self.ticket
    }

    /// Stage a text frame.
    pub fn send_text(self, store: &mut Store, text: &str) -> Result<(), DataError> {
        self.send(store, SocketOp::Text(text.into()))
    }

    /// Stage a binary frame.
    pub fn send_binary(self, store: &mut Store, bytes: &[u8]) -> Result<(), DataError> {
        self.send(store, SocketOp::Binary(bytes.into()))
    }

    /// Stage a close: `code` 1000 or 3000–4999, `reason` at most 123 UTF-8
    /// bytes, as a browser takes them. It does not refresh the resource.
    pub fn close(self, store: &mut Store, code: u16, reason: &str) -> Result<(), DataError> {
        let reason = reason.to_string();
        self.send(store, SocketOp::Close { code, reason })
    }

    /// Stage `op`, a text or binary frame or a close, for the commit of the
    /// call being made; refused, and nothing staged, when the call may not
    /// send on this socket, the socket is not open (or this call closed it),
    /// or the call would pass D7's bounds. A mapper call that passes them
    /// fails its frame even when it ignores the error.
    pub fn send(self, store: &mut Store, op: SocketOp) -> Result<(), DataError> {
        if let SocketOp::Close { code, reason } = &op {
            if *code != 1000 && !(3000..=4999).contains(code) {
                let why = "InvalidAccessError: invalid socket close code";
                return Err(DataError::Unavailable(why.into()));
            }
            if reason.len() > 123 {
                let why = "SyntaxError: socket close reason exceeds 123 UTF-8 bytes";
                return Err(DataError::Unavailable(why.into()));
            }
        } else if !matches!(op, SocketOp::Text(_) | SocketOp::Binary(_)) {
            return Err(DataError::Unavailable(
                "a socket handle sends text, binary or a close".into(),
            ));
        }
        let kind = match store.socket_call {
            SocketCall::Mutation => "mutation",
            SocketCall::Mapper(own) if own == self => "mapper call",
            SocketCall::Mapper(_) => return Err(invalid("a mapper sends only on its own socket")),
            SocketCall::None => {
                return Err(invalid(
                    "a socket sends only from a mutation's answer or its own mapper",
                ))
            }
        };
        let closed = (store.sockets.iter())
            .any(|(i, op)| *i == self.resource && matches!(op, SocketOp::Close { .. }));
        if !self.is_open(store) || closed {
            return Err(invalid("the socket is not open"));
        }
        let staged: usize = store.sockets.iter().map(|(_, op)| op.size()).sum();
        let over = if store.sockets.len() >= OPS {
            Some(format!("socket {kind} exceeds 256 operations"))
        } else if staged + op.size() > BYTES {
            Some(format!("socket {kind} exceeds 1 MiB"))
        } else if self.buffered_amount(store) + op.size() as u64 > BYTES as u64 {
            Some("socket buffer exceeds 1 MiB".into())
        } else {
            None
        };
        if let Some(what) = over {
            let why = format!("QuotaExceededError: {what}");
            if let SocketCall::Mapper(_) = store.socket_call {
                store.socket_overload = Some(why.clone());
            }
            return Err(DataError::Unavailable(why));
        }
        store.sockets.push((self.resource, op));
        Ok(())
    }

    /// Whether the socket is open in this call's turn.
    pub fn is_open(self, store: &Store) -> bool {
        store.socket_open.contains(&(self.resource, self.ticket))
    }

    /// D7's `bufferedAmount`: the transport's bytes as the turn began, with
    /// what the call has staged on this socket; 0 when it is not open.
    pub fn buffered_amount(self, store: &Store) -> u64 {
        if !self.is_open(store) {
            return 0;
        }
        let transport = (store.socket_buffered.iter())
            .find(|(i, _)| *i == self.resource)
            .map_or(0, |(_, n)| *n);
        let staged = (store.sockets.iter())
            .filter(|(i, _)| *i == self.resource)
            .map(|(_, op)| op.size() as u64);
        transport + staged.sum::<u64>()
    }
}

impl Store {
    /// The socket of the frame being parsed: in a mapper call answering its
    /// own socket's `open` or a message, that socket's handle, else `None`
    /// (LLP 1016.003 D3).
    pub fn socket(&self) -> Option<Socket> {
        match self.socket_call {
            SocketCall::Mapper(own) => Some(own),
            _ => None,
        }
    }

    /// Which sockets the call about to be made may send on.
    pub fn socket_call(&self) -> SocketCall {
        self.socket_call
    }

    /// Name the call about to be made ([`SocketCall`]).
    pub fn set_socket_call(&mut self, call: SocketCall) {
        self.socket_call = call;
        self.socket_overload = None;
    }

    /// The call is over: no call is named, and a mapper call that passed
    /// D7's bounds fails, whatever it answered.
    pub fn end_socket_call<T>(&mut self, result: Result<T, DataError>) -> Result<T, DataError> {
        self.socket_call = SocketCall::None;
        match self.socket_overload.take() {
            Some(why) => Err(DataError::Unavailable(why)),
            None => result,
        }
    }

    /// The mapper call's overload, for the module seam to carry back.
    pub fn socket_overload(&self) -> Option<&str> {
        self.socket_overload.as_deref()
    }

    /// A mapper call's overload that a Rust module's own store recorded.
    pub fn note_socket_overload(&mut self, why: String) {
        self.socket_overload = Some(why);
    }

    /// Set the turn's open sockets: each one's resource index and stream
    /// ticket.
    pub fn set_socket_open(&mut self, open: Vec<(usize, u64)>) {
        self.socket_open = open;
    }

    /// The turn's open sockets (see [`Store::set_socket_open`]).
    pub fn socket_open(&self) -> &[(usize, u64)] {
        &self.socket_open
    }
}
