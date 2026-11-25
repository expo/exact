//! Drags from other apps over the window (LLP 1094.001 D8). Wayland's own
//! data device, on winit's connection, says where a drag is and what it
//! offers by MIME type; the window takes a drop as a copy, and reads the
//! drop's items from the pipes the source writes them to.
//!
//! A listening thread owns the data devices, their offers and every request
//! to the compositor about them, and reads each drop's pipes in the same
//! loop, as a toolkit's main loop would. It waits on the connection itself,
//! because winit's loop goes back to sleep when none of the events it read
//! were its own queue's. The event loop tells the presenter what the
//! listener heard and sends back the presenter's answers.
#![allow(unsafe_code)]

use exact_painted::presenter::Presenter;
use exact_painted::Dropped;
use exact_runner::DataSource;
use smithay_client_toolkit::reexports::client::{
    backend::{Backend, ObjectId, WaylandError},
    event_created_child,
    globals::{registry_queue_init, GlobalListContents},
    protocol::{
        wl_data_device::{self, WlDataDevice},
        wl_data_device_manager::{DndAction, WlDataDeviceManager},
        wl_data_offer::{self, WlDataOffer},
        wl_registry::{self, WlRegistry},
        wl_seat::WlSeat,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, EventQueue, Proxy, QueueHandle, WEnum,
};
use std::ffi::OsString;
use std::io::{ErrorKind, PipeReader, PipeWriter, Read, Write};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::Mutex;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::Window;

/// The URI list a file manager offers its files as (RFC 2483).
const URI_LIST: &str = "text/uri-list";
/// The link a browser offers with its title, as Firefox and Chromium do and
/// as GTK and Qt read it: UTF-16, the URL, a line break and the title.
const MOZ_URL: &str = "text/x-moz-url";
/// The plain text types a drag may offer, the first offered taken: UTF-8
/// by name, then X11's name for it, then text in no named charset.
const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];
/// How long after the drop its items may take to arrive: what a source has
/// not finished sending by then is not delivered, so a stalled source cannot
/// hold the drop pending (D2 holds a newer drag's enters for it).
const DEADLINE: Duration = Duration::from_secs(10);
/// The most a drop's type may send; a type that sends more is not
/// delivered.
const LARGEST: usize = 64 << 20;

/// The event loop's side of drags: the drag over the window as the
/// presenter knows it, and the way to the listener.
pub(super) struct Drops {
    hover: Option<Hover>,
    commands: Sender<Command>,
    wake: Option<PipeWriter>,
    listener: Option<JoinHandle<()>>,
}

/// The drag over the window as the presenter knows it. The listener's
/// events reach the event loop in order, so the hover is always the
/// current drag's; its number tells the listener which drag an answer is
/// for, as the listener may have moved on by the time an answer arrives.
struct Hover {
    number: u64,
    point: (f32, f32),
    takes: Option<bool>,
}

/// What the listener heard, in order: a drag at a point, the drag gone, a
/// drop, and later the drop's items, each by the drag's number.
pub(in crate::window) enum Heard {
    Over {
        number: u64,
        point: (f32, f32),
    },
    Left,
    Dropped {
        number: u64,
        point: (f32, f32),
    },
    Landed {
        number: u64,
        id: u32,
        dropped: Dropped,
    },
}

/// The presenter's answers: whether a node under drag `number` takes the
/// drop, the drop the presenter made of it (none: nothing took it), and
/// whether the drop was delivered once its items arrived.
enum Command {
    Takes { number: u64, takes: bool },
    Released { number: u64, id: Option<u32> },
    Landed { number: u64, delivered: bool },
}

impl Drops {
    /// The listener for `window`'s seats, on winit's connection; none when
    /// the window is not Wayland's or the compositor shares no data.
    pub(super) fn new(window: &Window, proxy: EventLoopProxy<super::Event>) -> Option<Self> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // Winit's display outlives the listener: `App::exiting` drops Drops, which joins the listener, before the
        // event loop ends, and the builds that ship abort on a panic rather than unwind past it
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let surface =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
                .ok()?;
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<Listener>(&connection).ok()?;
        let handle = queue.handle();
        let manager = globals
            .bind::<WlDataDeviceManager, _, _>(&handle, 1..=3, ())
            .ok()?;
        let (commands, heard) = std::sync::mpsc::channel();
        let mut listener = Listener {
            surface,
            manager,
            seats: Vec::new(),
            drag: None,
            released: Vec::new(),
            drags: 0,
            proxy,
            commands: heard,
        };
        globals.contents().with_list(|list| {
            for global in list.iter().filter(|g| g.interface == "wl_seat") {
                listener.add_seat(globals.registry(), global.name, global.version, &handle);
            }
        });
        // A full wake pipe is a wake already pending, so neither end ever blocks
        let (woken, wake) = std::io::pipe().ok()?;
        (nonblocking(&woken) && nonblocking(&wake)).then_some(())?;
        let listener = std::thread::Builder::new()
            .name("exact-drags".into())
            .spawn(move || listener.listen(&connection, queue, &woken))
            .ok()?;
        Some(Self {
            hover: None,
            commands,
            wake: Some(wake),
            listener: Some(listener),
        })
    }

    /// Tell the presenter what the listener heard, and send back its answer.
    pub(super) fn heard<D: DataSource>(&mut self, p: &mut Presenter<D>, heard: Heard) {
        match heard {
            Heard::Over { number, point } => {
                let takes = self
                    .hover
                    .take()
                    .filter(|h| h.number == number)
                    .and_then(|h| h.takes);
                self.hover = Some(Hover {
                    number,
                    point,
                    takes,
                });
                self.answer(p);
            }
            Heard::Left => {
                self.hover = None;
                p.drag_left();
            }
            Heard::Dropped { number, point } => {
                self.hover = None;
                let id = p.drop_released(point, p.modifiers());
                self.send(Command::Released { number, id });
            }
            Heard::Landed {
                number,
                id,
                dropped,
            } => {
                let delivered = p.drop_landed(id, &dropped).is_some();
                self.send(Command::Landed { number, delivered });
            }
        }
    }

    /// Ask the presenter again whether a node under a drag that has not
    /// moved takes the drop, as a commit may have changed it.
    pub(super) fn turn<D: DataSource>(&mut self, p: &mut Presenter<D>) {
        if self.hover.is_some() {
            self.answer(p);
        }
    }

    /// The drag is where it is: the presenter hears it, and the listener
    /// hears whether a node there takes the drop when that changed.
    fn answer<D: DataSource>(&mut self, p: &mut Presenter<D>) {
        let Some(hover) = self.hover.as_ref() else {
            return;
        };
        let (number, point) = (hover.number, hover.point);
        let takes = p.drag_over(point, p.modifiers()).is_some();
        if let Some(hover) = self.hover.as_mut().filter(|h| h.takes != Some(takes)) {
            hover.takes = Some(takes);
            self.send(Command::Takes { number, takes });
        }
    }

    fn send(&mut self, command: Command) {
        if self.commands.send(command).is_err() {
            return;
        }
        if let Some(wake) = self.wake.as_mut() {
            while let Err(e) = wake.write(&[1]) {
                if e.kind() != ErrorKind::Interrupted {
                    break;
                }
            }
        }
    }
}

impl Drop for Drops {
    fn drop(&mut self) {
        // The listener hears the wake pipe close, ends its offers and reads, and leaves the connection
        drop(self.wake.take());
        if let Some(listener) = self.listener.take() {
            let _ = listener.join();
        }
    }
}

/// The listening thread's state: the data devices, one a seat, the drag
/// over the window, and the drops released to the presenter.
struct Listener {
    surface: ObjectId,
    manager: WlDataDeviceManager,
    seats: Vec<(u32, WlSeat, WlDataDevice)>,
    drag: Option<Drag>,
    released: Vec<Released>,
    drags: u64,
    proxy: EventLoopProxy<super::Event>,
    commands: Receiver<Command>,
}

/// A drag the window follows, on the device it entered by. `accepted` is
/// the type the window last accepted it as, which decides whether its drop
/// may be finished.
struct Drag {
    number: u64,
    device: ObjectId,
    offer: WlDataOffer,
    serial: u32,
    wanted: Wanted,
    point: (f32, f32),
    accepted: Option<String>,
}

/// A drop the presenter was told of, kept until it is delivered or
/// refused: when its items are due, counted from Wayland's drop, and the
/// items while they arrive.
struct Released {
    drag: Drag,
    deadline: Instant,
    reading: Option<Reading>,
}

/// Drop `id`'s items arriving through a pipe a type, each type at most
/// `largest` bytes.
struct Reading {
    id: u32,
    pipes: [Option<PipeReader>; 3],
    bytes: [Vec<u8>; 3],
    largest: usize,
}

/// The types the window reads from an offer, each spelled as offered: its
/// URI list, its titled link and its text.
#[derive(Default, PartialEq, Debug)]
struct Wanted {
    uris: Option<String>,
    link: Option<String>,
    text: Option<String>,
}

/// An offer's MIME types, as the compositor lists them before its enter,
/// and the action the compositor last chose for it.
#[derive(Default)]
struct Offered {
    types: Mutex<Vec<String>>,
    action: AtomicU32,
}

impl Listener {
    /// The listening thread, until the wake pipe closes or the connection
    /// fails: every offer it holds then ends unfinished.
    fn listen(mut self, connection: &Connection, mut queue: EventQueue<Self>, woken: &PipeReader) {
        self.run(connection, &mut queue, woken);
        for drag in self
            .drag
            .take()
            .into_iter()
            .chain(self.released.drain(..).map(|r| r.drag))
        {
            drag.offer.destroy();
        }
        // A finish the window sent as it closed is retried for up to a second while the socket is full
        let until = Instant::now() + Duration::from_secs(1);
        while let Err(WaylandError::Io(e)) = connection.flush() {
            let left = until.saturating_duration_since(Instant::now()).as_millis() as i32;
            if e.kind() != ErrorKind::WouldBlock || left == 0 {
                break;
            }
            let mut fd = poll_for(connection.backend().poll_fd().as_raw_fd(), libc::POLLOUT);
            unsafe { libc::poll(&mut fd, 1, left) };
        }
    }

    fn run(&mut self, connection: &Connection, queue: &mut EventQueue<Self>, woken: &PipeReader) {
        loop {
            if queue.dispatch_pending(self).is_err() {
                return;
            }
            self.obey();
            let blocked = match connection.flush() {
                Ok(()) => false,
                Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => true,
                Err(_) => return,
            };
            // A read is prepared before the poll, as threads that share a libwayland socket must; leaving it
            // unread cancels it
            let Some(guard) = queue.prepare_read() else {
                continue;
            };
            let display = libc::POLLIN | if blocked { libc::POLLOUT } else { 0 };
            let mut fds = vec![
                poll_for(guard.connection_fd().as_raw_fd(), display),
                poll_for(woken.as_raw_fd(), libc::POLLIN),
            ];
            let pipes: Vec<(usize, usize, i32)> = self.pipes().collect();
            fds.extend(pipes.iter().map(|&(.., fd)| poll_for(fd, libc::POLLIN)));
            let now = Instant::now();
            let wait = self
                .released
                .iter()
                .filter(|r| r.reading.is_some())
                .map(|r| {
                    r.deadline
                        .saturating_duration_since(now)
                        .as_millis()
                        .min(i32::MAX as u128) as i32
                })
                .min()
                .unwrap_or(-1);
            let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, wait) };
            if ready < 0 {
                if std::io::Error::last_os_error().kind() == ErrorKind::Interrupted {
                    continue;
                }
                return;
            }
            if fds[0].revents & (libc::POLLERR | libc::POLLHUP | libc::POLLNVAL) != 0 {
                return;
            }
            // The display's read is resolved first, so winit's thread never waits on this one's pipes
            if fds[0].revents & libc::POLLIN != 0 {
                match guard.read() {
                    Ok(_) => {}
                    Err(WaylandError::Io(e)) if e.kind() == ErrorKind::WouldBlock => {}
                    Err(_) => return,
                }
            } else {
                drop(guard);
            }
            // The answers the event loop sent before the window closed still count
            if fds[1].revents != 0 && !woke(woken) {
                self.obey();
                return;
            }
            // A drop past its deadline lands before its pipes are read, so nothing arrives after it
            self.land();
            for (fd, &(r, k, _)) in fds[2..].iter().zip(&pipes) {
                if fd.revents == 0 {
                    continue;
                }
                let released = &mut self.released[r];
                if let Some(reading) = released.reading.as_mut() {
                    reading.read(k, released.deadline);
                }
            }
            self.land();
        }
    }

    fn add_seat(
        &mut self,
        registry: &WlRegistry,
        name: u32,
        version: u32,
        handle: &QueueHandle<Self>,
    ) {
        let seat = registry.bind::<WlSeat, _, _>(name, version.min(7), handle, ());
        let device = self.manager.get_data_device(&seat, handle, ());
        self.seats.push((name, seat, device));
    }

    fn tell(&self, heard: Heard) {
        let _ = self.proxy.send_event(super::Event::Drag(heard));
    }

    /// The drag over the window ended without a drop.
    fn left(&mut self) {
        if let Some(drag) = self.drag.take() {
            drag.offer.destroy();
            self.tell(Heard::Left);
        }
    }

    /// Each open pipe, by its drop's place in `released` and its type's,
    /// with its descriptor.
    fn pipes(&self) -> impl Iterator<Item = (usize, usize, i32)> + '_ {
        self.released.iter().enumerate().flat_map(|(r, released)| {
            let open = released
                .reading
                .iter()
                .flat_map(|reading| reading.pipes.iter().enumerate());
            open.filter_map(move |(k, pipe)| Some((r, k, pipe.as_ref()?.as_raw_fd())))
        })
    }

    /// Hand the event loop each drop whose items have all arrived, or
    /// whose deadline passed with what had.
    fn land(&mut self) {
        let now = Instant::now();
        for released in &mut self.released {
            let due = now >= released.deadline;
            let Some(reading) = released.reading.take_if(|r| due || r.complete()) else {
                continue;
            };
            let (id, dropped) = reading.items();
            let number = released.drag.number;
            let _ = self.proxy.send_event(super::Event::Drag(Heard::Landed {
                number,
                id,
                dropped,
            }));
        }
    }

    /// Act on the presenter's answers.
    fn obey(&mut self) {
        while let Ok(command) = self.commands.try_recv() {
            match command {
                Command::Takes { number, takes } => {
                    if let Some(drag) = self.drag.as_mut().filter(|d| d.number == number) {
                        let mime = takes
                            .then(|| drag.wanted.first())
                            .flatten()
                            .map(str::to_owned);
                        drag.offer.accept(drag.serial, mime.clone());
                        drag.accepted = mime;
                    }
                }
                Command::Released { number, id } => {
                    let Some(at) = self.released.iter().position(|r| r.drag.number == number)
                    else {
                        continue;
                    };
                    match id {
                        Some(id) => {
                            let released = &mut self.released[at];
                            released.reading = Some(Reading::start(id, &released.drag));
                        }
                        None => self.released.remove(at).drag.offer.destroy(),
                    }
                }
                Command::Landed { number, delivered } => {
                    let Some(at) = self.released.iter().position(|r| r.drag.number == number)
                    else {
                        continue;
                    };
                    let offer = self.released.remove(at).drag.offer;
                    // Finishing tells the source the drop succeeded; an offer left unfinished is a cancelled drop
                    if delivered && offer.version() >= 3 {
                        offer.finish();
                    }
                    offer.destroy();
                }
            }
        }
    }
}

impl Reading {
    /// Ask the source for `drag`'s types for drop `id`. A type whose pipe
    /// cannot be made arrives empty.
    fn start(id: u32, drag: &Drag) -> Self {
        let Wanted { uris, link, text } = &drag.wanted;
        let pipes = [uris, link, text].map(|mime| {
            let (reader, writer) = std::io::pipe().ok()?;
            nonblocking(&reader).then_some(())?;
            drag.offer.receive(mime.clone()?, writer.as_fd());
            // The source holds its own copy of the write end, so the read ends when the source closes it
            drop(writer);
            Some(reader)
        });
        Reading {
            id,
            pipes,
            bytes: Default::default(),
            largest: LARGEST,
        }
    }

    /// Whether every type has arrived or given up.
    fn complete(&self) -> bool {
        self.pipes.iter().all(Option::is_none)
    }

    /// The drop's number and items: a type still arriving gives nothing.
    fn items(self) -> (u32, Dropped) {
        let Reading {
            id,
            pipes,
            mut bytes,
            ..
        } = self;
        for (pipe, bytes) in pipes.iter().zip(&mut bytes) {
            if pipe.is_some() {
                bytes.clear();
            }
        }
        let [uris, link, text] = bytes;
        (id, items(&uris, &link, &text))
    }

    /// Read some of what type `k`'s pipe holds, a bounded batch so other
    /// pipes and the display are heard between: its end closes the pipe, and
    /// an end after `deadline`, an error or more than `largest` bytes closes
    /// it with nothing.
    fn read(&mut self, k: usize, deadline: Instant) {
        let Some(pipe) = self.pipes[k].as_mut() else {
            return;
        };
        let mut chunk = [0; 1 << 16];
        for _ in 0..16 {
            match pipe.read(&mut chunk) {
                Ok(0) => {
                    self.pipes[k] = None;
                    if Instant::now() >= deadline {
                        self.bytes[k].clear();
                    }
                    return;
                }
                Ok(n) if self.bytes[k].len() + n <= self.largest => {
                    self.bytes[k].extend_from_slice(&chunk[..n])
                }
                Err(e) if e.kind() == ErrorKind::Interrupted => {}
                Err(e) if e.kind() == ErrorKind::WouldBlock => return,
                _ => {
                    self.pipes[k] = None;
                    self.bytes[k].clear();
                    return;
                }
            }
        }
    }
}

/// Whether the wake pipe is still open, its pending wakes read.
fn woke(woken: &PipeReader) -> bool {
    let mut bytes = [0; 64];
    loop {
        match (&*woken).read(&mut bytes) {
            Ok(0) => return false,
            Ok(_) => {}
            Err(e) if e.kind() == ErrorKind::Interrupted => {}
            Err(e) => return e.kind() == ErrorKind::WouldBlock,
        }
    }
}

fn poll_for(fd: i32, events: libc::c_short) -> libc::pollfd {
    libc::pollfd {
        fd,
        events,
        revents: 0,
    }
}

/// Make `pipe`'s end not block (`O_NONBLOCK`); whether it took.
fn nonblocking(pipe: &impl AsRawFd) -> bool {
    let fd = pipe.as_raw_fd();
    let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
    flags >= 0 && unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } >= 0
}

impl Wanted {
    /// What an offer of `types` gives the window, each type as offered:
    /// its URI list, its titled link and the first of its text types; none
    /// when it offers none of them.
    fn of(types: &[String]) -> Option<Self> {
        let find = |t: &str| {
            types
                .iter()
                .find(|offered| offered.eq_ignore_ascii_case(t))
                .cloned()
        };
        let wanted = Wanted {
            uris: find(URI_LIST),
            link: find(MOZ_URL),
            text: TEXTS.into_iter().find_map(find),
        };
        (wanted != Wanted::default()).then_some(wanted)
    }

    /// The type the window accepts while a node under the drag takes the
    /// drop.
    fn first(&self) -> Option<&str> {
        [&self.uris, &self.link, &self.text]
            .into_iter()
            .flatten()
            .next()
            .map(String::as_str)
    }
}

/// The items a drop's URI list (RFC 2483), titled link and text give: each
/// local `file:` URI a path, the first other URI the link, else the titled
/// link's URL, and the text as it came.
fn items(uris: &[u8], link: &[u8], text: &[u8]) -> Dropped {
    let mut dropped = Dropped {
        text: String::from_utf8_lossy(text).into_owned(),
        ..Default::default()
    };
    let lines = uris.split(|&b| b == b'\n').map(<[u8]>::trim_ascii);
    for line in lines.filter(|l| !l.is_empty() && !l.starts_with(b"#")) {
        if line.len() >= 5 && line[..5].eq_ignore_ascii_case(b"file:") {
            dropped.paths.extend(file_path(&line[5..]));
        } else if dropped.url.is_empty() {
            if let Some(url) = std::str::from_utf8(line).ok().and_then(web_link) {
                dropped.url = url.to_owned();
            }
        }
    }
    if dropped.url.is_empty() {
        let titled = utf16_or_utf8(link);
        if let Some(url) = titled.lines().next().and_then(web_link) {
            dropped.url = url.to_owned();
        }
    }
    dropped
}

/// `text`, trimmed, as a link that is not a file: a line with a URI scheme
/// (RFC 3986: a letter, then letters, digits, `+`, `-` or `.`) and no
/// whitespace or control character. The app checks the URL before following
/// it (D1), as GTK's URI-list reader and Android's leave its parts.
fn web_link(text: &str) -> Option<&str> {
    let text = text.trim();
    let (scheme, _) = text.split_once(':')?;
    let mut letters = scheme.chars();
    let valid = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && !text.chars().any(|c| c.is_whitespace() || c.is_control());
    (valid && !scheme.eq_ignore_ascii_case("file")).then_some(text)
}

/// Text a browser wrote as UTF-16, as Mozilla's types are: by its byte
/// order mark, else by where an ASCII character's zero byte falls; else
/// UTF-8, without the NUL a C string ends with.
fn utf16_or_utf8(bytes: &[u8]) -> String {
    let (big, units) = match bytes {
        [0xFE, 0xFF, rest @ ..] => (true, rest),
        [0xFF, 0xFE, rest @ ..] => (false, rest),
        [first, 0, ..] if *first != 0 && bytes.len().is_multiple_of(2) => (false, bytes),
        [0, second, ..] if *second != 0 && bytes.len().is_multiple_of(2) => (true, bytes),
        _ => {
            let end = bytes.iter().position(|&b| b == 0).unwrap_or(bytes.len());
            return String::from_utf8_lossy(&bytes[..end]).into_owned();
        }
    };
    let units = units.as_chunks::<2>().0.iter().map(|&pair| {
        if big {
            u16::from_be_bytes(pair)
        } else {
            u16::from_le_bytes(pair)
        }
    });
    char::decode_utf16(units)
        .map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER))
        .take_while(|&c| c != '\0')
        .collect()
}

/// The local path a `file:` URI names (RFC 8089), after its scheme: none
/// for another machine's file, or for a URI with a query, a fragment or a
/// `%` not followed by two hex digits. Its percent-decoded bytes are kept as
/// they are, UTF-8 or not.
fn file_path(rest: &[u8]) -> Option<PathBuf> {
    let path = match rest.strip_prefix(b"//") {
        Some(authority) => {
            let at = authority.iter().position(|&b| b == b'/')?;
            let host = &authority[..at];
            (host.is_empty() || host.eq_ignore_ascii_case(b"localhost"))
                .then_some(&authority[at..])?
        }
        None => rest.starts_with(b"/").then_some(rest)?,
    };
    if path.iter().any(|b| matches!(b, b'?' | b'#')) {
        return None;
    }
    let mut bytes = Vec::with_capacity(path.len());
    let mut at = path.iter();
    while let Some(&b) = at.next() {
        if b == b'%' {
            let hex = [*at.next()?, *at.next()?];
            let digit = |h: u8| (h as char).to_digit(16);
            bytes.push((digit(hex[0])? * 16 + digit(hex[1])?) as u8);
        } else {
            bytes.push(b);
        }
    }
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

impl Dispatch<WlRegistry, GlobalListContents> for Listener {
    fn event(
        state: &mut Self,
        registry: &WlRegistry,
        event: wl_registry::Event,
        _: &GlobalListContents,
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } if interface == "wl_seat" => {
                state.add_seat(registry, name, version, handle);
            }
            wl_registry::Event::GlobalRemove { name } => {
                let Some(at) = state.seats.iter().position(|(n, ..)| *n == name) else {
                    return;
                };
                let (_, seat, device) = state.seats.remove(at);
                if state.drag.as_ref().is_some_and(|d| d.device == device.id()) {
                    state.left();
                }
                if device.version() >= 2 {
                    device.release();
                }
                if seat.version() >= 5 {
                    seat.release();
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlSeat,
        _: <WlSeat as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDeviceManager, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlDataDeviceManager,
        _: <WlDataDeviceManager as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlDataDevice, ()> for Listener {
    fn event(
        state: &mut Self,
        device: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // The window follows one drag at a time, on the device it entered by: another seat's drag meanwhile is
        // refused, and its events are not this drag's
        let ours = state.drag.as_ref().is_some_and(|d| d.device == device.id());
        match event {
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                if ours {
                    state.left();
                }
                let Some(offer) = id else { return };
                let types = offer
                    .data::<Offered>()
                    .and_then(|o| o.types.lock().ok().map(|t| t.clone()))
                    .unwrap_or_default();
                // A decoration's surface takes no drag, and neither does a drag that offers nothing exact2 reads
                let wanted = (state.drag.is_none() && surface.id() == state.surface)
                    .then(|| Wanted::of(&types))
                    .flatten();
                let Some(wanted) = wanted else {
                    offer.accept(serial, None);
                    offer.destroy();
                    return;
                };
                // A copy alone: a move would let the source delete its files
                if offer.version() >= 3 {
                    offer.set_actions(DndAction::Copy, DndAction::Copy);
                }
                state.drags += 1;
                let (number, point) = (state.drags, (x as f32, y as f32));
                state.drag = Some(Drag {
                    number,
                    device: device.id(),
                    offer,
                    serial,
                    wanted,
                    point,
                    accepted: None,
                });
                state.tell(Heard::Over { number, point });
            }
            wl_data_device::Event::Motion { x, y, .. } if ours => {
                if let Some(drag) = state.drag.as_mut() {
                    drag.point = (x as f32, y as f32);
                    let (number, point) = (drag.number, drag.point);
                    state.tell(Heard::Over { number, point });
                }
            }
            wl_data_device::Event::Leave if ours => state.left(),
            wl_data_device::Event::Drop if ours => {
                let Some(drag) = state.drag.take() else {
                    return;
                };
                // Finishing an offer the window last refused, or one the compositor chose no copy for, is a
                // protocol error: such a drop is a cancelled drag
                let copy = drag.offer.version() < 3
                    || drag.offer.data::<Offered>().is_some_and(|o| {
                        o.action.load(Ordering::Relaxed) == DndAction::Copy.bits()
                    });
                if drag.accepted.is_some() && copy {
                    let (number, point) = (drag.number, drag.point);
                    state.released.push(Released {
                        drag,
                        deadline: Instant::now() + DEADLINE,
                        reading: None,
                    });
                    state.tell(Heard::Dropped { number, point });
                } else {
                    drag.offer.destroy();
                    state.tell(Heard::Left);
                }
            }
            wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
            _ => {}
        }
    }

    event_created_child!(Listener, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Offered::default()),
    ]);
}

impl Dispatch<WlDataOffer, Offered> for Listener {
    fn event(
        _: &mut Self,
        _: &WlDataOffer,
        event: wl_data_offer::Event,
        offered: &Offered,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_offer::Event::Offer { mime_type } => {
                if let Ok(mut types) = offered.types.lock() {
                    types.push(mime_type);
                }
            }
            wl_data_offer::Event::Action {
                dnd_action: WEnum::Value(action),
            } => offered.action.store(action.bits(), Ordering::Relaxed),
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "drop_tests.rs"]
mod tests;
