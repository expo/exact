//! A WebSocket client off Apple (RFC 6455): TCP from the standard
//! library, TLS from the same rustls and trust store as `RustlsHttpTransport`,
//! and the same environment proxy (an HTTP CONNECT tunnel; see `tunnel`).
//! One readiness-driven I/O pump owns each socket and rustls connection. It
//! multiplexes the bounded outbound queue with requested receives, so control
//! frames remain live while a large data message drains.
//! @ref LLP 0057#3-the-boundary — the platform owns the socket and TLS

use crate::boundary::HostError;
use crate::stdlib::abort::{AbortRegistration, AbortSignal};
use crate::stdlib::websocket::{
    Event, Incoming, Message, MessageSender, MessageSource, SocketTransport,
};
use mio::event::Source;
use mio::net::TcpStream as MioTcpStream;
use mio::{Events, Interest, Poll, Token, Waker};
use std::io::{Read, Write};
use std::net::{Shutdown, TcpStream as StdTcpStream, ToSocketAddrs};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    mpsc, Arc, Mutex,
};
use std::time::{Duration, Instant};

const MAX_HEAD: usize = 16 << 10;
/// How long a proxy has to answer CONNECT, its request written and its whole
/// answer read: a deadline, not a per-read timeout a slow drip could renew.
const CONNECT_BUDGET: Duration = Duration::from_secs(15);
const FRAGMENT: usize = 16 << 10;
const MAX_OUTBOUND_BYTES: usize = 16 << 20;
const MAX_OUTBOUND_MESSAGES: usize = 256;
// @ref LLP 0059.000#312-websocket--delegating-capability-bearing-author-required — data and control frames share one bounded command channel
const MAX_OUTBOUND_COMMANDS: usize = MAX_OUTBOUND_MESSAGES + 16;
const MAX_READ_FRAMES_PER_TURN: usize = 32;
const SOCKET_TOKEN: Token = Token(0);
const WAKE_TOKEN: Token = Token(1);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum AdmissionState {
    Open,
    Closing,
    Closed,
}

/// Plaintext `ws:` and rustls `wss:`, one connection per socket. The trust
/// store loads on the first `wss:` open, never at construction (a host that
/// opens no socket pays nothing at boot).
#[derive(Default)]
pub struct TcpSocketTransport {
    tls: std::sync::OnceLock<Arc<rustls::ClientConfig>>,
    #[cfg(test)]
    proxy: TestProxy,
    #[cfg(test)]
    connect_budget: Option<Duration>,
    #[cfg(test)]
    writer_gate: Option<Arc<(Mutex<bool>, std::sync::Condvar)>>,
    #[cfg(test)]
    pump_observer: Option<Arc<PumpObserver>>,
    #[cfg(test)]
    zero_socket_writes: bool,
}

/// Where a test transport's proxy comes from: none (the default, so a
/// developer's own proxy settings never reach a loopback fixture), the
/// environment (as in production), or a given one.
#[cfg(test)]
#[derive(Clone, Default)]
enum TestProxy {
    #[default]
    None,
    Environment,
    Given(ureq::Proxy),
}

#[cfg(test)]
#[derive(Default)]
struct PumpObserver {
    parked: std::sync::atomic::AtomicBool,
    receiving: std::sync::atomic::AtomicBool,
    exited: std::sync::atomic::AtomicBool,
    close_received: std::sync::atomic::AtomicBool,
    terminal_draining: std::sync::atomic::AtomicBool,
    returns: AtomicUsize,
    network_bytes: AtomicUsize,
    last_write_progress: Mutex<Option<Instant>>,
    write_blocked: std::sync::atomic::AtomicBool,
    peer_fin: std::sync::atomic::AtomicBool,
    send_state: std::sync::OnceLock<Arc<Mutex<SendState>>>,
}

impl TcpSocketTransport {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(test)]
    pub(crate) fn with_tls(config: Arc<rustls::ClientConfig>) -> Self {
        let tls = std::sync::OnceLock::new();
        tls.set(config).expect("a fresh TLS configuration");
        Self {
            tls,
            proxy: TestProxy::None,
            connect_budget: None,
            writer_gate: None,
            pump_observer: None,
            zero_socket_writes: false,
        }
    }

    #[cfg(test)]
    fn with_writer_gate(gate: Arc<(Mutex<bool>, std::sync::Condvar)>) -> Self {
        Self {
            tls: std::sync::OnceLock::new(),
            proxy: TestProxy::None,
            connect_budget: None,
            writer_gate: Some(gate),
            pump_observer: None,
            zero_socket_writes: false,
        }
    }

    #[cfg(test)]
    fn with_pump_observer(observer: Arc<PumpObserver>) -> Self {
        Self {
            tls: std::sync::OnceLock::new(),
            proxy: TestProxy::None,
            connect_budget: None,
            writer_gate: None,
            pump_observer: Some(observer),
            zero_socket_writes: false,
        }
    }

    #[cfg(test)]
    fn with_tls_and_pump_observer(
        config: Arc<rustls::ClientConfig>,
        observer: Arc<PumpObserver>,
    ) -> Self {
        let tls = std::sync::OnceLock::new();
        tls.set(config).expect("a fresh TLS configuration");
        Self {
            tls,
            proxy: TestProxy::None,
            connect_budget: None,
            writer_gate: None,
            pump_observer: Some(observer),
            zero_socket_writes: false,
        }
    }

    #[cfg(test)]
    fn with_tls_zero_socket_writes(config: Arc<rustls::ClientConfig>) -> Self {
        let tls = std::sync::OnceLock::new();
        tls.set(config).expect("a fresh TLS configuration");
        Self {
            tls,
            proxy: TestProxy::None,
            connect_budget: None,
            writer_gate: None,
            pump_observer: None,
            zero_socket_writes: true,
        }
    }

    /// A transport whose proxy comes from `proxy`, with `NO_PROXY` and the
    /// protocol applied as to the environment's, and a CONNECT that must be
    /// answered within `connect_budget` (default `CONNECT_BUDGET`).
    #[cfg(test)]
    fn with_tls_and_proxy(
        config: Arc<rustls::ClientConfig>,
        proxy: TestProxy,
        connect_budget: Option<Duration>,
    ) -> Self {
        let mut transport = Self::with_tls(config);
        transport.proxy = proxy;
        transport.connect_budget = connect_budget;
        transport
    }

    fn tls(&self) -> Arc<rustls::ClientConfig> {
        self.tls.get_or_init(super::tls::socket_config).clone()
    }

    // @ref LLP 0059.000#312-websocket--delegating-capability-bearing-author-required — the portable transport traverses fetch's environment proxy
    /// The proxy a socket to `host:port` goes through: the environment's, as
    /// fetch's ureq agent reads it, unless `NO_PROXY` names the host. One the
    /// socket cannot speak (`https://`, SOCKS) fails the open rather than
    /// being skipped.
    fn proxy_for(&self, host: &str, port: u16) -> Result<Option<ureq::Proxy>, HostError> {
        #[cfg(test)]
        let proxy = match &self.proxy {
            TestProxy::None => None,
            TestProxy::Environment => ureq::Proxy::try_from_env(),
            TestProxy::Given(proxy) => Some(proxy.clone()),
        };
        #[cfg(not(test))]
        let proxy = ureq::Proxy::try_from_env();
        let Some(proxy) = proxy else {
            return Ok(None);
        };
        // The target's authority alone: NO_PROXY names hosts, and a path too long for `http::Uri` cannot defeat it
        if format!("http://{host}:{port}/")
            .parse::<ureq::http::Uri>()
            .is_ok_and(|uri| proxy.is_no_proxy(&uri))
        {
            return Ok(None);
        }
        if proxy.protocol() != ureq::ProxyProtocol::Http {
            return Err(failed(format!(
                "the environment's proxy {} is not an http:// CONNECT proxy, the only kind a \
                 WebSocket tunnels through; use one or name the host in NO_PROXY",
                named(&proxy)
            )));
        }
        Ok(Some(proxy))
    }

    fn connect_budget(&self) -> Duration {
        #[cfg(test)]
        if let Some(budget) = self.connect_budget {
            return budget;
        }
        CONNECT_BUDGET
    }
}

fn failed(what: impl std::fmt::Display) -> HostError {
    HostError::Failed(format!("the socket did not open: {what}"))
}

enum HandshakeWire {
    Plain(StdTcpStream),
    Tls {
        stream: Box<rustls::StreamOwned<rustls::ClientConnection, StdTcpStream>>,
    },
}

impl HandshakeWire {
    fn into_nonblocking(self, #[cfg(test)] zero_socket_writes: bool) -> std::io::Result<Wire> {
        let convert = |tcp: StdTcpStream| {
            tcp.set_read_timeout(None)?;
            tcp.set_write_timeout(None)?;
            tcp.set_nonblocking(true)?;
            Ok::<_, std::io::Error>(PumpSocket {
                tcp: MioTcpStream::from_std(tcp),
                #[cfg(test)]
                zero_writes: zero_socket_writes,
            })
        };
        match self {
            Self::Plain(tcp) => convert(tcp).map(Wire::Plain),
            Self::Tls { stream } => {
                let rustls::StreamOwned { conn, sock } = *stream;
                Ok(Wire::Tls {
                    conn: Box::new(conn),
                    sock: convert(sock)?,
                })
            }
        }
    }
}

struct PumpSocket {
    tcp: MioTcpStream,
    #[cfg(test)]
    zero_writes: bool,
}

impl Read for PumpSocket {
    fn read(&mut self, bytes: &mut [u8]) -> std::io::Result<usize> {
        self.tcp.read(bytes)
    }
}

impl Write for PumpSocket {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        #[cfg(test)]
        if self.zero_writes && !bytes.is_empty() {
            return Ok(0);
        }
        self.tcp.write(bytes)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.tcp.flush()
    }
}

impl Source for PumpSocket {
    fn register(
        &mut self,
        registry: &mio::Registry,
        token: Token,
        interests: Interest,
    ) -> std::io::Result<()> {
        self.tcp.register(registry, token, interests)
    }

    fn reregister(
        &mut self,
        registry: &mio::Registry,
        token: Token,
        interests: Interest,
    ) -> std::io::Result<()> {
        self.tcp.reregister(registry, token, interests)
    }

    fn deregister(&mut self, registry: &mio::Registry) -> std::io::Result<()> {
        self.tcp.deregister(registry)
    }
}

impl Read for HandshakeWire {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.read(out),
            Self::Tls { stream } => stream.read(out),
        }
    }
}
impl Write for HandshakeWire {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        match self {
            Self::Plain(s) => s.write(bytes),
            Self::Tls { stream } => stream.write(bytes),
        }
    }
    fn flush(&mut self) -> std::io::Result<()> {
        match self {
            Self::Plain(s) => s.flush(),
            Self::Tls { stream } => stream.flush(),
        }
    }
}

enum Wire {
    Plain(PumpSocket),
    Tls {
        conn: Box<rustls::ClientConnection>,
        sock: PumpSocket,
    },
}

impl Wire {
    fn tcp_mut(&mut self) -> &mut PumpSocket {
        match self {
            Self::Plain(tcp) => tcp,
            Self::Tls { sock, .. } => sock,
        }
    }
}

impl SocketTransport for TcpSocketTransport {
    fn connect(
        &self,
        url: &url::Url,
        max_message: usize,
        signal: &AbortSignal,
    ) -> Result<Box<dyn MessageSource>, HostError> {
        self.connect_with_protocols(url, max_message, signal, &[])
    }

    fn connect_with_protocols(
        &self,
        url: &url::Url,
        max_message: usize,
        signal: &AbortSignal,
        protocols: &[String],
    ) -> Result<Box<dyn MessageSource>, HostError> {
        self.open_socket(url, max_message, signal, protocols)
            .map(|socket| Box::new(socket) as Box<dyn MessageSource>)
    }
}

impl TcpSocketTransport {
    fn open_socket(
        &self,
        url: &url::Url,
        max_message: usize,
        signal: &AbortSignal,
        protocols: &[String],
    ) -> Result<Socket, HostError> {
        if url.scheme() == "wss" {
            super::tls::ensure_available()?;
        }
        let host = url.host_str().ok_or_else(|| failed("no host"))?;
        let port = url
            .port_or_known_default()
            .ok_or_else(|| failed("no port"))?;
        // Through a proxy the connection is the proxy's, and the proxy resolves
        // the target, as fetch's CONNECT does (ureq's default for HTTP proxies)
        let proxy = self.proxy_for(host, port)?;
        let (dial_host, dial_port) = match proxy.as_ref() {
            Some(proxy) => (proxy.host(), proxy.port()),
            None => (host, port),
        };
        // The system lookup on a bounded helper, so an abort returns at once
        // even while the resolver hangs (`lookup`).
        let name = dial_host
            .trim_start_matches('[')
            .trim_end_matches(']')
            .to_string();
        let found = super::lookup::off_thread(signal, None, move || {
            (name.as_str(), dial_port)
                .to_socket_addrs()
                .map(|addrs| addrs.collect::<Vec<_>>())
        });
        let addrs: Vec<_> = match found {
            Ok(addrs) => addrs.map_err(failed)?,
            Err(super::lookup::Unanswered::Failed(why)) => return Err(failed(why)),
            Err(_) => return Err(signal.check().err().unwrap_or_else(|| failed("aborted"))),
        };
        signal.check()?;
        // Establish both readiness resources before a server can observe a TCP
        // connection or successful WebSocket upgrade. The Waker is mio's
        // platform primitive (EVFILT_USER/eventfd/IOCP), not a second socket.
        let poll = Poll::new().map_err(failed)?;
        let wake = Arc::new(Waker::new(poll.registry(), WAKE_TOKEN).map_err(failed)?);
        let mut last = failed("no resolved address");
        let mut tcp = None;
        for address in addrs {
            signal.check()?;
            match super::rustls_http::connect_socket(address, Duration::from_secs(10), signal) {
                Ok(s) => {
                    tcp = Some(s);
                    break;
                }
                Err(e) => last = failed(e),
            }
        }
        let mut tcp = tcp.ok_or(last)?;
        tcp.set_nodelay(true).map_err(failed)?;
        // Aborting wakes Poll even when the connection is completely idle.
        // Shutdown is retained as a second, idempotent way to make socket I/O
        // observe cancellation if it races the wake.
        let registration = {
            let socket = tcp.try_clone().map_err(failed)?;
            let wake = Arc::clone(&wake);
            signal.register(move || {
                let _ = socket.shutdown(Shutdown::Both);
                let _ = wake.wake();
            })
        };
        signal.check()?;
        let shutdown = tcp.try_clone().map_err(failed)?;
        tcp.set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(failed)?;
        if let Some(proxy) = proxy.as_ref() {
            tunnel(&mut tcp, proxy, host, port, self.connect_budget())
                .map_err(|e| signal.check().err().unwrap_or(e))?;
        }
        let mut wire = if url.scheme() == "wss" {
            let name = rustls::pki_types::ServerName::try_from(
                host.trim_start_matches('[')
                    .trim_end_matches(']')
                    .to_string(),
            )
            .map_err(failed)?;
            let tls = rustls::ClientConnection::new(self.tls(), name).map_err(failed)?;
            HandshakeWire::Tls {
                stream: Box::new(rustls::StreamOwned::new(tls, tcp)),
            }
        } else {
            HandshakeWire::Plain(tcp)
        };
        let (buffered, protocol) =
            handshake(&mut wire, url, protocols).map_err(|e| match signal.check() {
                Err(aborted) => aborted,
                Ok(()) => e,
            })?;
        // The connector (including Windows' select + SO_ERROR path) has
        // already proved this socket connected. Convert that exact retained
        // handle after the bounded blocking handshake; mio does not reconnect.
        let mut wire = wire
            .into_nonblocking(
                #[cfg(test)]
                self.zero_socket_writes,
            )
            .map_err(failed)?;
        poll.registry()
            .register(wire.tcp_mut(), SOCKET_TOKEN, Interest::READABLE)
            .map_err(failed)?;
        let buffered_amount = Arc::new(AtomicUsize::new(0));
        let command_count = Arc::new(AtomicUsize::new(0));
        let send_state = Arc::new(Mutex::new(SendState {
            admission: AdmissionState::Open,
            queued_bytes: 0,
            queued_messages: 0,
            terminal: None,
            graceful_teardown: false,
            close_flushed: Arc::default(),
            #[cfg(test)]
            history: Vec::new(),
        }));
        let (commands, outgoing) = mpsc::sync_channel(MAX_OUTBOUND_COMMANDS);
        let (requests, incoming) = mpsc::sync_channel(1);
        let sender = Arc::new(TcpSender {
            commands,
            wake: Arc::clone(&wake),
            command_count: Arc::clone(&command_count),
            buffered: Arc::clone(&buffered_amount),
            state: Arc::clone(&send_state),
            shutdown: shutdown.try_clone().map_err(failed)?,
        });
        let pump_shutdown = shutdown.try_clone().map_err(failed)?;
        let pump_signal = signal.clone();
        #[cfg(test)]
        let writer_gate = self.writer_gate.clone();
        #[cfg(test)]
        let pump_observer = self.pump_observer.clone();
        let pump_wake = Arc::clone(&wake);
        std::thread::spawn(move || {
            pump_loop(
                wire,
                buffered,
                max_message,
                outgoing,
                incoming,
                poll,
                pump_wake,
                pump_shutdown,
                buffered_amount,
                command_count,
                send_state,
                pump_signal,
                #[cfg(test)]
                writer_gate,
                #[cfg(test)]
                pump_observer,
            )
        });
        Ok(Socket {
            requests,
            wake,
            shutdown,
            signal: signal.clone(),
            _registration: registration,
            sender,
            protocol,
        })
    }
}

/// A proxy as an error names it: its scheme, host and port, never the
/// credentials its URL may carry.
fn named(proxy: &ureq::Proxy) -> String {
    let scheme = proxy.uri().scheme_str().unwrap_or("http");
    format!("{scheme}://{}:{}", proxy.host(), proxy.port())
}

/// Opens an HTTP CONNECT tunnel to `host:port` through `proxy` on `tcp`, as
/// ureq's `ConnectProxyConnector` does for fetch, with Basic credentials from
/// the proxy's URL. Every write and read checks one deadline, `budget` from
/// now. Informational (1xx) answers are passed over, and any 2xx final answer
/// opens the tunnel (RFC 9110 §9.3.6, §15.2; this ureq takes only 200). The
/// answer is read a byte at a time, so no tunnel byte is consumed with it. An
/// error gives its status code, never its reason phrase, which a proxy may
/// fill with anything, the credentials it was sent included.
fn tunnel(
    tcp: &mut StdTcpStream,
    proxy: &ureq::Proxy,
    host: &str,
    port: u16,
    budget: Duration,
) -> Result<(), HostError> {
    use base64::Engine as _;
    let target = format!("{host}:{port}");
    let through = |what: String| failed(format!("the proxy {} {what}", named(proxy)));
    let deadline = Instant::now() + budget;
    let remaining = || {
        let left = deadline.saturating_duration_since(Instant::now());
        (!left.is_zero()).then_some(left).ok_or_else(|| {
            through(format!(
                "did not answer CONNECT {target} within {} s",
                budget.as_secs_f64()
            ))
        })
    };
    let mut head = format!("CONNECT {target} HTTP/1.1\r\nHost: {target}\r\n");
    if proxy.username().is_some() || proxy.password().is_some() {
        let credentials = format!(
            "{}:{}",
            proxy.username().unwrap_or_default(),
            proxy.password().unwrap_or_default()
        );
        let encoded = base64::engine::general_purpose::STANDARD.encode(credentials);
        head.push_str(&format!("Proxy-Authorization: Basic {encoded}\r\n"));
    }
    head.push_str("\r\n");
    let mut unsent = head.as_bytes();
    while !unsent.is_empty() {
        tcp.set_write_timeout(Some(remaining()?)).map_err(failed)?;
        match tcp.write(unsent) {
            Ok(0) => return Err(through(format!("closed before CONNECT {target} was sent"))),
            Ok(sent) => unsent = &unsent[sent..],
            Err(e) if is_retry(&e) => {}
            Err(e) => return Err(through(format!("was not sent CONNECT {target}: {e}"))),
        }
    }
    let status = loop {
        let mut answer = Vec::with_capacity(256);
        let mut byte = [0u8; 1];
        while !answer.ends_with(b"\r\n\r\n") {
            if answer.len() >= MAX_HEAD {
                return Err(through(format!(
                    "answered CONNECT {target} with a head over {MAX_HEAD} bytes"
                )));
            }
            tcp.set_read_timeout(Some(remaining()?)).map_err(failed)?;
            match tcp.read(&mut byte) {
                Ok(0) => return Err(through(format!("closed before answering CONNECT {target}"))),
                Ok(_) => answer.push(byte[0]),
                Err(e) if is_retry(&e) => {}
                Err(e) => return Err(through(format!("did not answer CONNECT {target}: {e}"))),
            }
        }
        // 101 would switch protocols, which a CONNECT answer cannot
        match connect_status(&answer) {
            Some(status @ 100..=199) if status != 101 => continue,
            status => break status,
        }
    };
    // The upgrade that follows keeps the open's own timeouts
    tcp.set_write_timeout(None).map_err(failed)?;
    tcp.set_read_timeout(Some(Duration::from_secs(15)))
        .map_err(failed)?;
    match status {
        Some(200..=299) => Ok(()),
        Some(status) => Err(through(format!(
            "refused CONNECT {target} with status {status}"
        ))),
        None => Err(through(format!(
            "answered CONNECT {target} with a malformed head"
        ))),
    }
}

/// Whether a deadline-bound loop tries an I/O call again: an interrupted
/// call, or a timeout, on which the deadline itself then decides.
fn is_retry(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::Interrupted
            | std::io::ErrorKind::WouldBlock
            | std::io::ErrorKind::TimedOut
    )
}

/// The status of a CONNECT answer's head, ending in its blank line, or `None`
/// when it breaks RFC 9112's grammar (§4–5): an `HTTP/1.0` or `HTTP/1.1`
/// status line with a three-digit code and an optional reason, then header
/// lines of a token, a colon and a value, a folded line continuing the one
/// before. A reason or value may hold any octet but a control other than tab
/// (obs-text included). A status line ending at its code, without the space
/// before an empty reason, is taken, as proxies send it.
fn connect_status(head: &[u8]) -> Option<u16> {
    // Every line ends in CRLF: a bare LF leaves a line without its CR, and a
    // bare CR is a control, which no part below admits
    let mut lines = head
        .strip_suffix(b"\n\r\n")?
        .split(|b| *b == b'\n')
        .map(|line| line.strip_suffix(b"\r"));
    let status_line = lines.next()??;
    let (code, reason) = status_line
        .strip_prefix(b"HTTP/1.1 ")
        .or_else(|| status_line.strip_prefix(b"HTTP/1.0 "))?
        .split_at_checked(3)?;
    let text = |octets: &[u8]| {
        octets
            .iter()
            .all(|&b| b == b'\t' || (b >= 0x20 && b != 0x7f))
    };
    let token = |b: &u8| b.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(b);
    if !code.iter().all(u8::is_ascii_digit)
        || !(reason.is_empty() || reason.starts_with(b" ") && text(reason))
    {
        return None;
    }
    let mut first = true;
    for line in lines {
        let line = line?;
        let well_formed = match line.first() {
            Some(b' ' | b'\t') => !first && text(line),
            _ => line.iter().position(|b| *b == b':').is_some_and(|colon| {
                colon > 0 && line[..colon].iter().all(token) && text(&line[colon + 1..])
            }),
        };
        if !well_formed {
            return None;
        }
        first = false;
    }
    std::str::from_utf8(code).ok()?.parse().ok()
}

/// Send the opening handshake and check the answer. Returns bytes read past
/// the head (the first frames, if the server was quick).
fn handshake(
    wire: &mut HandshakeWire,
    url: &url::Url,
    protocols: &[String],
) -> Result<(Vec<u8>, String), HostError> {
    let mut nonce = [0u8; 16];
    getrandom::getrandom(&mut nonce).map_err(failed)?;
    use base64::Engine as _;
    let key = base64::engine::general_purpose::STANDARD.encode(nonce);
    let serialized_host = match url.host() {
        Some(url::Host::Ipv6(address)) => format!("[{address}]"),
        Some(url::Host::Ipv4(address)) => address.to_string(),
        Some(url::Host::Domain(domain)) => domain.to_string(),
        None => String::new(),
    };
    let host = match url.port() {
        Some(port) => format!("{serialized_host}:{port}"),
        None => serialized_host,
    };
    let target = match url.query() {
        Some(q) => format!("{}?{q}", url.path()),
        None => url.path().to_string(),
    };
    let protocols_header = if protocols.is_empty() {
        String::new()
    } else {
        format!("Sec-WebSocket-Protocol: {}\r\n", protocols.join(", "))
    };
    let head = format!(
        "GET {target} HTTP/1.1\r\nHost: {host}\r\nUpgrade: websocket\r\nConnection: Upgrade\r\n\
         Sec-WebSocket-Key: {key}\r\nSec-WebSocket-Version: 13\r\n{protocols_header}\r\n"
    );
    wire.write_all(head.as_bytes()).map_err(failed)?;
    wire.flush().map_err(failed)?;
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 4096];
    let end = loop {
        if let Some(at) = bytes.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
        if bytes.len() > MAX_HEAD {
            return Err(failed("the handshake's headers exceed 16 KiB"));
        }
        match wire.read(&mut chunk).map_err(failed)? {
            0 => return Err(failed("the connection closed during the handshake")),
            n => bytes.extend_from_slice(&chunk[..n]),
        }
    };
    let text = String::from_utf8_lossy(&bytes[..end]).into_owned();
    let mut lines = text.split("\r\n");
    let status = lines
        .next()
        .and_then(|l| l.split(' ').nth(1))
        .and_then(|s| s.parse::<u16>().ok())
        .ok_or_else(|| failed("not an HTTP answer"))?;
    if status != 101 {
        return Err(failed(format!("HTTP {status}")));
    }
    let header = |name: &str| {
        text.split("\r\n").skip(1).find_map(|l| {
            let (k, v) = l.split_once(':')?;
            k.trim()
                .eq_ignore_ascii_case(name)
                .then(|| v.trim().to_string())
        })
    };
    let expected = crate::stdlib::websocket::accept_key(&key);
    let upgrade = header("upgrade").is_some_and(|v| v.eq_ignore_ascii_case("websocket"));
    let connection = header("connection").is_some_and(|v| {
        v.split(',')
            .any(|t| t.trim().eq_ignore_ascii_case("upgrade"))
    });
    if !upgrade || !connection || header("sec-websocket-accept").as_deref() != Some(&expected) {
        return Err(failed("the server's handshake was not a WebSocket upgrade"));
    }
    if header("sec-websocket-extensions").is_some() {
        return Err(failed("the server chose an extension never offered"));
    }
    let selected = header("sec-websocket-protocol").unwrap_or_default();
    if !selected.is_empty() && !protocols.iter().any(|value| value == &selected) {
        return Err(failed("the server chose a subprotocol never offered"));
    }
    Ok((bytes[end..].to_vec(), selected))
}

struct Socket {
    requests: mpsc::SyncSender<ReceiveRequest>,
    wake: Arc<Waker>,
    shutdown: StdTcpStream,
    signal: AbortSignal,
    _registration: AbortRegistration,
    sender: Arc<TcpSender>,
    protocol: String,
}

fn protocol(what: &str) -> HostError {
    HostError::Failed(format!("the socket broke the protocol: {what}"))
}

enum Command {
    Data {
        opcode: u8,
        payload: Vec<u8>,
        accounted: usize,
    },
    Control {
        opcode: u8,
        payload: Vec<u8>,
    },
}

struct TcpSender {
    commands: mpsc::SyncSender<Command>,
    wake: Arc<Waker>,
    command_count: Arc<AtomicUsize>,
    buffered: Arc<AtomicUsize>,
    state: Arc<Mutex<SendState>>,
    shutdown: StdTcpStream,
}

struct SendState {
    admission: AdmissionState,
    queued_bytes: usize,
    queued_messages: usize,
    terminal: Option<TerminalLatch>,
    /// The pump has handed its final frame to TCP (or is already
    /// half-closing) and owns a graceful TCP teardown from here on.
    graceful_teardown: bool,
    close_flushed: Arc<(Mutex<bool>, std::sync::Condvar)>,
    #[cfg(test)]
    history: Vec<TerminalStep>,
}

/// Test-only record of terminal-latch transitions, in the order the pump made
/// them under the send-state lock.
#[cfg(test)]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TerminalStep {
    LatchedDeferred,
    LatchedDeliverable,
    CloseSent,
    Deliverable,
}

struct TerminalLatch {
    result: Result<Received, HostError>,
    delivered: bool,
    deliverable: bool,
    peer_close: bool,
}

enum CommandQueueError {
    Full,
    Closed,
    Wake(std::io::Error),
}

impl CommandQueueError {
    fn host_error(self) -> HostError {
        match self {
            Self::Full => HostError::Failed("the socket's outbound command queue is full".into()),
            Self::Closed => HostError::Failed("the socket is closed".into()),
            Self::Wake(error) => {
                HostError::Failed(format!("the socket wake notification failed: {error}"))
            }
        }
    }
}

impl TcpSender {
    fn queue(&self, state: &mut SendState, command: Command) -> Result<(), CommandQueueError> {
        if self
            .command_count
            .try_update(Ordering::AcqRel, Ordering::Acquire, |count| {
                (count < MAX_OUTBOUND_COMMANDS).then_some(count + 1)
            })
            .is_err()
        {
            state.admission = AdmissionState::Closed;
            let _ = self.shutdown.shutdown(Shutdown::Both);
            let _ = self.wake.wake();
            return Err(CommandQueueError::Full);
        }
        match self.commands.try_send(command) {
            Ok(()) => {
                if let Err(error) = self.wake.wake() {
                    state.admission = AdmissionState::Closed;
                    let _ = self.shutdown.shutdown(Shutdown::Both);
                    Err(CommandQueueError::Wake(error))
                } else {
                    Ok(())
                }
            }
            Err(mpsc::TrySendError::Full(_)) => {
                self.command_count.fetch_sub(1, Ordering::AcqRel);
                // A peer that does not read can prevent even a close frame
                // from draining. Fail abruptly instead of adding an unbounded
                // control-frame escape hatch beside the data quotas.
                state.admission = AdmissionState::Closed;
                let _ = self.shutdown.shutdown(Shutdown::Both);
                let _ = self.wake.wake();
                Err(CommandQueueError::Full)
            }
            Err(mpsc::TrySendError::Disconnected(_)) => {
                self.command_count.fetch_sub(1, Ordering::AcqRel);
                state.admission = AdmissionState::Closed;
                let _ = self.shutdown.shutdown(Shutdown::Both);
                let _ = self.wake.wake();
                Err(CommandQueueError::Closed)
            }
        }
    }

    fn enqueue(&self, opcode: u8, payload: &[u8]) -> Result<(), HostError> {
        saturating_add(&self.buffered, payload.len());
        let mut state = self.state.lock().expect("WebSocket sender poisoned");
        if state.admission != AdmissionState::Open {
            return Ok(());
        }
        if state.queued_bytes.saturating_add(payload.len()) > MAX_OUTBOUND_BYTES
            || state.queued_messages >= MAX_OUTBOUND_MESSAGES
        {
            // WHATWG says a full implementation buffer flags the socket as
            // full and closes the connection. An abrupt local failure keeps
            // memory bounded when even a close frame could sit behind a
            // non-reading peer.
            state.admission = AdmissionState::Closed;
            drop(state);
            let _ = self.shutdown.shutdown(Shutdown::Both);
            let _ = self.wake.wake();
            return Ok(());
        }
        let queued = self.queue(
            &mut state,
            Command::Data {
                opcode,
                payload: payload.to_vec(),
                accounted: payload.len(),
            },
        );
        if let Err(error) = queued {
            return match error {
                // As with the byte/message quota above, the send which finds
                // a full implementation buffer fails the connection without
                // synchronously throwing into script.
                CommandQueueError::Full => Ok(()),
                CommandQueueError::Closed | CommandQueueError::Wake(_) => Err(error.host_error()),
            };
        }
        state.queued_bytes = state.queued_bytes.saturating_add(payload.len());
        state.queued_messages += 1;
        Ok(())
    }

    fn mark_closed(&self) {
        self.state
            .lock()
            .expect("WebSocket sender poisoned")
            .admission = AdmissionState::Closed;
        let _ = self.wake.wake();
    }

    fn admission(&self) -> AdmissionState {
        self.state
            .lock()
            .expect("WebSocket sender poisoned")
            .admission
    }
}

impl MessageSender for TcpSender {
    fn send_text(&self, text: &str) -> Result<(), HostError> {
        self.enqueue(0x1, text.as_bytes())
    }

    fn send_binary(&self, bytes: &[u8]) -> Result<(), HostError> {
        self.enqueue(0x2, bytes)
    }

    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), HostError> {
        let mut state = self.state.lock().expect("WebSocket sender poisoned");
        if state.admission != AdmissionState::Open {
            return Ok(());
        }
        state.admission = AdmissionState::Closing;
        let mut payload = Vec::new();
        if let Some(code) = code {
            payload.extend_from_slice(&code.to_be_bytes());
            payload.extend_from_slice(reason.as_bytes());
        }
        self.queue(
            &mut state,
            Command::Control {
                opcode: 0x8,
                payload,
            },
        )
        .map_err(CommandQueueError::host_error)
    }

    fn wait_close(&self, timeout: Duration) -> bool {
        let progress = self
            .state
            .lock()
            .expect("WebSocket sender poisoned")
            .close_flushed
            .clone();
        let (lock, changed) = &*progress;
        let (flushed, _) = changed
            .wait_timeout_while(lock.lock().unwrap(), timeout, |done| !*done)
            .unwrap();
        *flushed
    }

    fn buffered_amount(&self) -> usize {
        self.buffered.load(Ordering::Acquire)
    }

    fn buffered_messages(&self) -> usize {
        self.state
            .lock()
            .expect("WebSocket sender poisoned")
            .queued_messages
    }
}

fn saturating_add(amount: &AtomicUsize, bytes: usize) {
    let _ = amount.try_update(Ordering::AcqRel, Ordering::Acquire, |current| {
        Some(current.saturating_add(bytes))
    });
}

/// How long one frame may wait for a peer that is not reading before the
/// connection fails. Short in unit tests so the stall path is exercised.
#[cfg(not(test))]
const WRITE_STALL_TIMEOUT: Duration = Duration::from_secs(15);
#[cfg(test)]
const WRITE_STALL_TIMEOUT: Duration = Duration::from_millis(500);

struct ReceiveRequest {
    binary_payload: bool,
    reply: mpsc::SyncSender<Result<Received, HostError>>,
}

struct ReceiveOperation {
    binary_payload: bool,
    message: Option<(u8, Vec<u8>)>,
}

struct Input {
    bytes: Vec<u8>,
    at: usize,
}

impl Input {
    fn new(bytes: Vec<u8>) -> Self {
        Self { bytes, at: 0 }
    }

    fn available(&self) -> &[u8] {
        &self.bytes[self.at..]
    }

    fn append(&mut self, bytes: &[u8]) {
        if self.at == self.bytes.len() {
            self.bytes.clear();
            self.at = 0;
        } else if self.at > 0 && self.at >= self.bytes.len() / 2 {
            self.bytes.drain(..self.at);
            self.at = 0;
        }
        self.bytes.extend_from_slice(bytes);
    }

    fn take(&mut self, count: usize) -> Vec<u8> {
        let start = self.at;
        self.at += count;
        self.bytes[start..start + count].to_vec()
    }

    fn discard(&mut self, count: usize) {
        self.at += count;
    }
}

struct ActiveData {
    opcode: u8,
    payload: Vec<u8>,
    accounted: usize,
    next: usize,
}

impl ActiveData {
    fn frame(&mut self) -> std::io::Result<(Vec<u8>, bool)> {
        if self.payload.is_empty() {
            return masked_frame(true, self.opcode, &[]).map(|frame| (frame, true));
        }
        let start = self.next;
        let end = start.saturating_add(FRAGMENT).min(self.payload.len());
        self.next = end;
        let final_fragment = end == self.payload.len();
        masked_frame(
            final_fragment,
            if start == 0 { self.opcode } else { 0 },
            &self.payload[start..end],
        )
        .map(|frame| (frame, final_fragment))
    }
}

enum FrameKind {
    Data { final_fragment: bool },
    Control { opcode: u8 },
}

struct OutgoingFrame {
    bytes: Vec<u8>,
    at: usize,
    kind: FrameKind,
}

struct Control {
    opcode: u8,
    payload: Vec<u8>,
}

enum Parse {
    NeedData(usize),
    Received(Received),
    Yield,
}

enum ControlParse {
    NeedData(usize),
    Data,
    Progress,
    Closed(Received),
    Yield,
}

enum ReadProgress {
    Bytes,
    Eof,
    Blocked,
}

enum WriteProgress {
    Blocked,
    Retry,
    Network(usize),
    Eof,
}

impl Wire {
    fn wants_write(&self) -> bool {
        match self {
            Self::Plain(_) => false,
            Self::Tls { conn, .. } => conn.wants_write(),
        }
    }

    fn read_into(&mut self, input: &mut Input, wanted: usize) -> std::io::Result<ReadProgress> {
        let mut bytes = [0; 16 << 10];
        let wanted = wanted.min(bytes.len());
        match self {
            Self::Plain(tcp) => match tcp.read(&mut bytes[..wanted]) {
                Ok(0) => Ok(ReadProgress::Eof),
                Ok(count) => {
                    input.append(&bytes[..count]);
                    Ok(ReadProgress::Bytes)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Ok(ReadProgress::Blocked)
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    Ok(ReadProgress::Bytes)
                }
                Err(error) => Err(error),
            },
            Self::Tls { conn, sock } => match conn.reader().read(&mut bytes[..wanted]) {
                Ok(0) => Ok(ReadProgress::Eof),
                Ok(count) => {
                    input.append(&bytes[..count]);
                    Ok(ReadProgress::Bytes)
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    match conn.read_tls(sock) {
                        Ok(0) => Ok(ReadProgress::Eof),
                        Ok(_) => {
                            conn.process_new_packets().map_err(|error| {
                                std::io::Error::new(std::io::ErrorKind::InvalidData, error)
                            })?;
                            Ok(ReadProgress::Bytes)
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                            Ok(ReadProgress::Blocked)
                        }
                        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                            Ok(ReadProgress::Bytes)
                        }
                        Err(error) => Err(error),
                    }
                }
                Err(error) => Err(error),
            },
        }
    }

    fn write_frame(&mut self, frame: &mut OutgoingFrame) -> std::io::Result<WriteProgress> {
        match self {
            Self::Plain(_) if frame.at == frame.bytes.len() => Ok(WriteProgress::Blocked),
            Self::Plain(tcp) => match tcp.write(&frame.bytes[frame.at..]) {
                Ok(0) => Err(std::io::ErrorKind::WriteZero.into()),
                Ok(count) => {
                    frame.at += count;
                    Ok(WriteProgress::Network(count))
                }
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    Ok(WriteProgress::Blocked)
                }
                Err(error) if error.kind() == std::io::ErrorKind::Interrupted => {
                    Ok(WriteProgress::Retry)
                }
                Err(error) => Err(error),
            },
            Self::Tls { conn, sock } => {
                if conn.wants_write() {
                    return write_tls_progress(conn, sock);
                }
                if frame.at == frame.bytes.len() {
                    return Ok(WriteProgress::Blocked);
                }
                let count = conn.writer().write(&frame.bytes[frame.at..])?;
                if count == 0 {
                    return Err(std::io::ErrorKind::WriteZero.into());
                }
                frame.at += count;
                Ok(WriteProgress::Retry)
            }
        }
    }

    fn frame_flushed(&self, frame: &OutgoingFrame) -> bool {
        frame.at == frame.bytes.len() && !self.wants_write()
    }

    fn write_tls_pending(&mut self) -> std::io::Result<WriteProgress> {
        let Self::Tls { conn, sock } = self else {
            return Ok(WriteProgress::Blocked);
        };
        write_tls_progress(conn, sock)
    }
}

fn write_tls_progress(
    connection: &mut rustls::ClientConnection,
    socket: &mut impl Write,
) -> std::io::Result<WriteProgress> {
    match connection.write_tls(socket) {
        Ok(0) => Ok(WriteProgress::Eof),
        Ok(count) => Ok(WriteProgress::Network(count)),
        Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => Ok(WriteProgress::Blocked),
        Err(error) if error.kind() == std::io::ErrorKind::Interrupted => Ok(WriteProgress::Retry),
        Err(error) => Err(error),
    }
}

#[allow(clippy::too_many_arguments)]
fn parse_available(
    operation: &mut ReceiveOperation,
    input: &mut Input,
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
    state: &Mutex<SendState>,
    limit: usize,
    close_started: bool,
    frames_left: &mut usize,
) -> Result<Parse, HostError> {
    loop {
        if *frames_left == 0 {
            return Ok(Parse::Yield);
        }
        let bytes = input.available();
        if bytes.len() < 2 {
            return Ok(Parse::NeedData(2 - bytes.len()));
        }
        let (fin, opcode) = (bytes[0] & 0x80 != 0, bytes[0] & 0x0f);
        if bytes[0] & 0x70 != 0 || bytes[1] & 0x80 != 0 {
            return Err(protocol("reserved bits, or a masked server frame"));
        }
        let (head, length) = match bytes[1] & 0x7f {
            126 if bytes.len() < 4 => return Ok(Parse::NeedData(4 - bytes.len())),
            126 => (4, u16::from_be_bytes([bytes[2], bytes[3]]) as u64),
            127 if bytes.len() < 10 => return Ok(Parse::NeedData(10 - bytes.len())),
            127 => (10, u64::from_be_bytes(bytes[2..10].try_into().unwrap())),
            length => (2, length as u64),
        };
        if opcode >= 8 && (!fin || length > 125) {
            return Err(protocol("a fragmented or long control frame"));
        }
        // Preserve the receive-only API exact2 already consumes: the first
        // binary frame's declared length returns before its payload is read or
        // allocated. Event consumers use the payload-bearing path below.
        if opcode == 0x2 && operation.message.is_none() && !operation.binary_payload {
            input.discard(head);
            return Ok(Parse::Received(Received::BinaryLength(length as usize)));
        }
        if opcode < 8 {
            match (opcode, &operation.message) {
                (0x1, None) | (0x2, None) => {}
                (0x0, Some(_)) => {}
                _ => return Err(protocol("a frame out of sequence")),
            }
            let so_far = operation
                .message
                .as_ref()
                .map_or(0, |(_, message)| message.len()) as u64;
            if so_far.saturating_add(length) > limit as u64 {
                input.discard(head);
                let mut send = state.lock().expect("WebSocket sender poisoned");
                if !close_started {
                    replace_controls_with_close(
                        controls,
                        command_count,
                        1009u16.to_be_bytes().to_vec(),
                    )?;
                    send.admission = AdmissionState::Closing;
                }
                return Ok(Parse::Received(Received::TooLarge));
            }
        }
        let Ok(length) = usize::try_from(length) else {
            return Err(protocol("a frame length that does not fit this platform"));
        };
        let frame_length = head.saturating_add(length);
        if input.available().len() < frame_length {
            return Ok(Parse::NeedData(frame_length - input.available().len()));
        }
        input.discard(head);
        let payload = input.take(length);
        *frames_left -= 1;
        if opcode >= 8 {
            match opcode {
                0x8 => {
                    let (code, reason) = parse_close_payload(&payload)?;
                    let echo = if code == 1005 {
                        vec![]
                    } else {
                        payload.clone()
                    };
                    let mut send = state.lock().expect("WebSocket sender poisoned");
                    if !close_started {
                        replace_controls_with_close(controls, command_count, echo)?;
                    }
                    send.admission = AdmissionState::Closed;
                    return Ok(Parse::Received(Received::Closed { code, reason }));
                }
                0x9 => {
                    if !close_started {
                        reserve_control(command_count)?;
                        controls.push_back(Control {
                            opcode: 0xA,
                            payload,
                        });
                    }
                }
                0xA => {}
                _ => return Err(protocol("an unknown control opcode")),
            }
            continue;
        }
        if matches!(opcode, 0x1 | 0x2) {
            operation.message = Some((opcode, Vec::new()));
        }
        let (_, whole) = operation.message.as_mut().unwrap();
        whole.extend_from_slice(&payload);
        if fin {
            let (kind, bytes) = operation.message.take().unwrap();
            let received = if kind == 0x1 {
                Received::Text(
                    String::from_utf8(bytes)
                        .map_err(|_| protocol("a text message that is not UTF-8"))?,
                )
            } else {
                Received::Binary(bytes)
            };
            return Ok(Parse::Received(received));
        }
    }
}

fn parse_control_prefix(
    input: &mut Input,
    controls: &mut std::collections::VecDeque<Control>,
    command_count: &AtomicUsize,
    state: &Mutex<SendState>,
    close_started: bool,
    frames_left: &mut usize,
) -> Result<ControlParse, HostError> {
    let mut progressed = false;
    loop {
        if *frames_left == 0 {
            return Ok(ControlParse::Yield);
        }
        let bytes = input.available();
        if bytes.len() < 2 {
            return Ok(if progressed {
                ControlParse::Progress
            } else {
                ControlParse::NeedData(2 - bytes.len())
            });
        }
        let (fin, opcode) = (bytes[0] & 0x80 != 0, bytes[0] & 0x0f);
        if opcode < 8 {
            return Ok(if progressed {
                ControlParse::Progress
            } else {
                ControlParse::Data
            });
        }
        if bytes[0] & 0x70 != 0 || bytes[1] & 0x80 != 0 {
            return Err(protocol("reserved bits, or a masked server frame"));
        }
        let (head, length): (usize, usize) = match bytes[1] & 0x7f {
            126 if bytes.len() < 4 => return Ok(ControlParse::NeedData(4 - bytes.len())),
            126 => (4, u16::from_be_bytes([bytes[2], bytes[3]]) as usize),
            127 if bytes.len() < 10 => return Ok(ControlParse::NeedData(10 - bytes.len())),
            127 => {
                let length = u64::from_be_bytes(bytes[2..10].try_into().unwrap());
                let length = usize::try_from(length)
                    .map_err(|_| protocol("a frame length that does not fit this platform"))?;
                (10, length)
            }
            length => (2, length as usize),
        };
        if !fin || length > 125 {
            return Err(protocol("a fragmented or long control frame"));
        }
        let frame_length = head.saturating_add(length);
        if bytes.len() < frame_length {
            return Ok(ControlParse::NeedData(frame_length - bytes.len()));
        }
        input.discard(head);
        let payload = input.take(length);
        *frames_left -= 1;
        progressed = true;
        match opcode {
            0x8 => {
                let (code, reason) = parse_close_payload(&payload)?;
                let echo = if code == 1005 { Vec::new() } else { payload };
                let mut send = state.lock().expect("WebSocket sender poisoned");
                if !close_started {
                    replace_controls_with_close(controls, command_count, echo)?;
                }
                send.admission = AdmissionState::Closed;
                return Ok(ControlParse::Closed(Received::Closed { code, reason }));
            }
            0x9 => {
                if !close_started {
                    reserve_control(command_count)?;
                    controls.push_back(Control {
                        opcode: 0xA,
                        payload,
                    });
                }
            }
            0xA => {}
            _ => return Err(protocol("an unknown control opcode")),
        }
    }
}

#[path = "websocket_pump.rs"]
mod pump;
use pump::*;

#[cfg(any(test, feature = "test-support"))]
#[path = "websocket_tests.rs"]
pub mod tests;
