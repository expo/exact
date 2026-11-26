//! Drags from other apps over the window (LLP 1094.001 D8), and out of it
//! (`lift.rs`, LLP 1094.000 D13). Wayland's own data device, on winit's
//! connection, says where a drag is and what it offers by MIME type; the
//! window takes a drop as a copy, and reads the drop's items from the pipes
//! the source writes them to.
//!
//! A listening thread owns the data devices, their offers and sources and
//! every request to the compositor about them, and reads and writes each
//! drag's pipes in the same loop, as a toolkit's main loop would. It waits on the connection itself,
//! because winit's loop goes back to sleep when none of the events it read
//! were its own queue's. The event loop tells the presenter what the
//! listener heard and sends back the presenter's answers.
#![allow(unsafe_code)]

use exact_painted::presenter::Presenter;
use exact_painted::{Checked, Dropped};
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
use std::sync::Arc;

#[path = "lift.rs"]
mod lift;
#[path = "sandbox.rs"]
mod sandbox;
#[path = "staging.rs"]
mod staging;
pub(in crate::window) use lift::{Pointed, Press};
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
/// How long after the drop its text and titled link may take to arrive:
/// what a source has not finished sending of them by then is not
/// delivered. The URI list and the portal's key are waited for until their
/// source ends them, as the Mac waits for a promised file, since a source may
/// first copy its files (LLP 1094.000 D13); another drop ends that wait
/// (D2 holds a newer drag's enters meanwhile).
const DEADLINE: Duration = Duration::from_secs(10);
/// The most a drop's type may send; a type that sends more is not
/// delivered.
const LARGEST: usize = 64 << 20;

/// The event loop's side of drags: the drag over the window as the
/// presenter knows it, the lifts out of it, whether it can lift, and the
/// way to the listener.
pub(super) struct Drops {
    hover: Option<Hover>,
    lifts: lift::Lifts,
    lifting: bool,
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
/// drop, and later the drop's items, each by the drag's number; what a
/// seat's pointer did on the window's surface; the end of a drag out of the
/// window, whose press's release the window never hears; and the listener's
/// own end, after which the window takes winit's pointer.
pub(in crate::window) enum Heard {
    Pointer {
        seat: u32,
        pointed: Pointed,
        modifiers: Option<u32>,
    },
    Ended(Press),
    Gone,
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
        landing: Landing,
    },
    Journal(String),
}

/// The presenter's answers: whether a node under drag `number` takes the
/// drop, the drop the presenter made of it (none: nothing took it), and
/// whether the drop was delivered once its items arrived; and a lift of
/// `press`, with its types' bytes and its icon.
enum Command {
    Takes {
        number: u64,
        takes: bool,
    },
    Released {
        number: u64,
        id: Option<u32>,
    },
    Landed {
        number: u64,
        delivered: bool,
    },
    Lift {
        press: Press,
        offers: Vec<(&'static str, Arc<[u8]>)>,
        files: Option<lift::Files>,
        icon: Option<lift::Icon>,
    },
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
        let lifting = manager.version() >= 3;
        if lifting {
            // The portal is probed as the listener starts, and a sandbox's old drag copies aged
            exact_painted::file_transfer::Portal::session();
            staging::sweep_at_start();
        }
        let out = lift::Out::new(&globals, &handle, &connection, &surface)?;
        let (answers, answered) = std::io::pipe().ok()?;
        (nonblocking(&answers) && nonblocking(&answered)).then_some(())?;
        let mut listener = Listener {
            surface,
            manager,
            seats: Vec::new(),
            drag: None,
            released: Vec::new(),
            drags: 0,
            proxy,
            commands: heard,
            handle: handle.clone(),
            out,
            answered: (answers, Arc::new(answered)),
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
            lifts: lift::Lifts::default(),
            lifting,
            commands,
            wake: Some(wake),
            listener: Some(listener),
        })
    }

    /// The cell the window keeps its held left press in, which decides
    /// whether a lift is taken.
    pub(super) fn held(&self) -> std::rc::Rc<std::cell::Cell<Option<Press>>> {
        self.lifts.held()
    }

    /// The presenter's lifter, when the compositor's data sources can offer
    /// a copy alone (version 3).
    pub(super) fn lifter(&self) -> Option<exact_painted::presenter::drag_out::Lifter> {
        self.lifting.then(|| self.lifts.lifter())
    }

    /// Send the listener the lift the presenter made, if any.
    pub(super) fn flush(&mut self) {
        if let Some(command) = self.lifts.take() {
            self.send(command);
        }
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
                // Drops still waiting are about to land, as this one ended their wait
                let id = p.drop_released_behind(point, p.modifiers());
                self.send(Command::Released { number, id });
            }
            Heard::Landed { number, landing } => {
                let Landing {
                    id,
                    dropped,
                    checked,
                } = landing;
                let delivered = match checked {
                    Some(checked) => p.drop_landed_checked(id, &dropped, checked),
                    None => p.drop_landed(id, &dropped),
                };
                self.send(Command::Landed {
                    number,
                    delivered: delivered.is_some(),
                });
            }
            Heard::Journal(line) => p.host_mut().log(line),
            // The window's input hears these
            Heard::Pointer { .. } | Heard::Ended(_) | Heard::Gone => {}
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

/// The listening thread's state: the seats, the drag over the window, the
/// drops released to the presenter, and the drags out of it.
struct Listener {
    surface: ObjectId,
    manager: WlDataDeviceManager,
    seats: Vec<Seat>,
    drag: Option<Drag>,
    released: Vec<Released>,
    drags: u64,
    proxy: EventLoopProxy<super::Event>,
    commands: Receiver<Command>,
    handle: QueueHandle<Self>,
    out: lift::Out,
    /// The pipe that says a worker answered for a drop: the portal for its
    /// key, or the sandbox's check of its files. The workers share the
    /// writer, so none needs a descriptor of its own.
    answered: (PipeReader, Arc<PipeWriter>),
}

/// A seat, by its global's name: its data device, and its pointer while it
/// has one.
struct Seat {
    name: u32,
    seat: WlSeat,
    device: WlDataDevice,
    pointer: Option<lift::Pointer>,
    keyboard: Option<lift::Keyboard>,
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
/// refused: when its items are due, counted from Wayland's drop, and where
/// it is.
struct Released {
    drag: Drag,
    deadline: Instant,
    progress: Progress,
}

/// Where a released drop is, and whether another drop ended its wait.
struct Progress {
    stage: Stage,
    cut: bool,
}

/// Where a released drop is: waiting for the presenter to make a drop of
/// it, its items arriving, its files checked against what the sandbox
/// shares, or handed to the event loop.
enum Stage {
    Waiting,
    Reading(Reading),
    Checking(Checking),
    Landed,
}

/// Drop `id`'s items, all come, while a worker checks which of the URI
/// list's `files` the sandbox shares with the host (LLP 1094.001 D8); the
/// paths are the worker's.
struct Checking {
    id: u32,
    dropped: Dropped,
    files: usize,
    kept: Answer<Checked>,
}

/// A worker's answer, once it gives it.
type Answer<T> = Arc<Mutex<Option<Vec<T>>>>;

/// A drop handed to the event loop: its number among the drops, its items,
/// and, for a sandbox's URI list, the files its check kept, each with its
/// folder held, in place of the items' paths.
pub(in crate::window) struct Landing {
    id: u32,
    dropped: Dropped,
    checked: Option<Vec<Checked>>,
}

/// Drop `id`'s items arriving through a pipe a type, in [`Wanted`]'s order,
/// each type at most `largest` bytes, and the portal's answer for its key
/// once it is asked: its paths, when it gave some.
struct Reading {
    id: u32,
    pipes: [Option<PipeReader>; 4],
    bytes: [Vec<u8>; 4],
    largest: usize,
    retrieved: Option<Answer<PathBuf>>,
}

/// Where each type is in a [`Reading`]: its URI list, its titled link, its
/// text and the portal's key.
const URIS: usize = 0;
const LINK: usize = 1;
const TEXT: usize = 2;
const KEY: usize = 3;

/// The types the window reads from an offer, each spelled as offered: its
/// URI list, its titled link, its text and, in a Flatpak sandbox, the
/// portal's file transfer key.
#[derive(Default, PartialEq, Debug)]
struct Wanted {
    uris: Option<String>,
    link: Option<String>,
    text: Option<String>,
    key: Option<String>,
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
        self.tell(Heard::Gone);
        self.out.close();
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
            // A drop the presenter just made may have nothing to wait for, as when none of its pipes opened
            self.land();
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
            let sends: Vec<(usize, i32)> = self.out.pipes().collect();
            fds.extend(sends.iter().map(|&(_, fd)| poll_for(fd, libc::POLLOUT)));
            let made = self.out.made();
            fds.extend(made.map(|fd| poll_for(fd, libc::POLLIN)));
            // Always heard, so a worker that answers between the landing above and this poll wakes it
            fds.push(poll_for(self.answered.0.as_raw_fd(), libc::POLLIN));
            let now = Instant::now();
            let wait = self
                .released
                .iter()
                .filter(|r| r.progress.reading().is_some_and(Reading::timed))
                .map(|r| {
                    r.deadline
                        .saturating_duration_since(now)
                        .as_millis()
                        .min(i32::MAX as u128) as i32
                })
                .chain(self.out.due(now))
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
            let extra = &fds[2 + pipes.len() + sends.len()..];
            // Each pass, so a key request whose time ended closes though nothing woke the loop for it
            if made.is_some() {
                self.out.staged();
            }
            if extra.last().is_some_and(|fd| fd.revents != 0) {
                let mut bytes = [0; 64];
                while matches!((&self.answered.0).read(&mut bytes), Ok(n) if n > 0) {}
            }
            let ready: Vec<usize> = fds[2 + pipes.len()..]
                .iter()
                .zip(&sends)
                .filter(|(fd, _)| fd.revents != 0)
                .map(|(_, &(k, _))| k)
                .collect();
            self.out.send(&ready);
            for (fd, &(r, k, _)) in fds[2..2 + pipes.len()].iter().zip(&pipes) {
                if fd.revents == 0 {
                    continue;
                }
                let released = &mut self.released[r];
                let deadline = released.deadline;
                if let Some(reading) = released.progress.reading_mut() {
                    reading.read(k, deadline);
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
        self.seats.push(Seat {
            name,
            seat,
            device,
            pointer: None,
            keyboard: None,
        });
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
                .progress
                .reading()
                .into_iter()
                .flat_map(|reading| reading.pipes.iter().enumerate());
            open.filter_map(move |(k, pipe)| Some((r, k, pipe.as_ref()?.as_raw_fd())))
        })
    }

    /// Hand the event loop each drop whose items have all arrived, in the
    /// order they were released ([`settle`]). Past its deadline a drop waits
    /// no more for its text and titled link, and one whose key has come asks
    /// the portal for its files.
    fn land(&mut self) {
        let now = Instant::now();
        let Listener {
            released,
            proxy,
            answered,
            ..
        } = self;
        for released in released.iter_mut() {
            let deadline = released.deadline;
            let Some(reading) = released.progress.reading_mut() else {
                continue;
            };
            if now >= deadline {
                reading.time_out();
            }
            if reading.retrieved.is_none() {
                if let Some(key) = reading.key() {
                    let journal = lift::journal_to(proxy.clone());
                    reading.retrieved = Some(retrieve(key.to_owned(), answered.1.clone(), journal));
                }
            }
        }
        let journal = lift::journal_to(proxy.clone());
        let landings = settle(
            released.iter_mut().map(|r| &mut r.progress),
            sandbox::sandboxed(),
            &mut |paths| check(paths, answered.1.clone(), lift::journal_to(proxy.clone())),
            &journal,
        );
        for (at, landing) in landings {
            let number = released[at].drag.number;
            let _ = proxy.send_event(super::Event::Drag(Heard::Landed { number, landing }));
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
                            let mut reading = Reading::start(id, &released.drag);
                            if released.progress.cut {
                                reading.cut();
                            }
                            released.progress.stage = Stage::Reading(reading);
                        }
                        None => self.released.remove(at).drag.offer.destroy(),
                    }
                }
                Command::Lift {
                    press,
                    offers,
                    files,
                    icon,
                } => self.lift(press, offers, files, icon),
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

impl Progress {
    fn reading(&self) -> Option<&Reading> {
        match &self.stage {
            Stage::Reading(reading) => Some(reading),
            _ => None,
        }
    }

    fn reading_mut(&mut self) -> Option<&mut Reading> {
        match &mut self.stage {
            Stage::Reading(reading) => Some(reading),
            _ => None,
        }
    }

    /// Another drop ended this one's wait (LLP 1094.001 D8): it lands with
    /// what came, which keeps a portal's answer or a check's that came, and
    /// files whose check had not ended are not delivered.
    fn cut(&mut self, journal: &dyn Fn(String)) {
        self.cut = true;
        match &mut self.stage {
            Stage::Reading(reading) => reading.cut(),
            Stage::Checking(checking) => {
                let ended = checking
                    .kept
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .is_some();
                if !ended {
                    journal(format!(
                        "drop: {} files were left out: another drop came before the sandbox's check of them ended",
                        checking.files
                    ));
                    checking.kept = Arc::new(Mutex::new(Some(Vec::new())));
                }
            }
            Stage::Waiting | Stage::Landed => {}
        }
    }
}

/// Move each released drop on, in the order they were released, and hand
/// over each whose items have all arrived (LLP 1094.001 D8): the landings,
/// by their place among `drops`. A drop that still waits holds those after
/// it, so one released behind another never lands first. In a sandbox the
/// URI list's files go to a worker first (`check`), which keeps those the
/// sandbox shares with the host, unless another drop ended this one's wait:
/// then they are left out, so a stalled check holds no later drop.
fn settle<'a>(
    drops: impl IntoIterator<Item = &'a mut Progress>,
    sandboxed: bool,
    check: &mut dyn FnMut(Vec<PathBuf>) -> Answer<Checked>,
    journal: &dyn Fn(String),
) -> Vec<(usize, Landing)> {
    let mut landings = Vec::new();
    for (at, progress) in drops.into_iter().enumerate() {
        let stage = std::mem::replace(&mut progress.stage, Stage::Landed);
        progress.stage = match stage {
            Stage::Reading(reading) if reading.complete() => {
                let (id, mut dropped, given) = reading.items();
                let checks = sandboxed && !given && !dropped.paths.is_empty();
                if checks && progress.cut {
                    journal(format!(
                        "drop: {} files were left out: another drop came before the sandbox's check of them began",
                        dropped.paths.len()
                    ));
                    dropped.paths.clear();
                }
                if checks && !progress.cut {
                    let paths = std::mem::take(&mut dropped.paths);
                    let files = paths.len();
                    let kept = check(paths);
                    Stage::Checking(Checking {
                        id,
                        dropped,
                        files,
                        kept,
                    })
                } else {
                    let checked = None;
                    landings.push((
                        at,
                        Landing {
                            id,
                            dropped,
                            checked,
                        },
                    ));
                    Stage::Landed
                }
            }
            stage => stage,
        };
        // A check that has ended lands its drop with the files it kept
        let ended = match &progress.stage {
            Stage::Checking(checking) => checking
                .kept
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .take(),
            _ => None,
        };
        if let Some(kept) = ended {
            if let Stage::Checking(Checking { id, dropped, .. }) =
                std::mem::replace(&mut progress.stage, Stage::Landed)
            {
                let checked = Some(kept);
                landings.push((
                    at,
                    Landing {
                        id,
                        dropped,
                        checked,
                    },
                ));
            }
        }
        if !matches!(progress.stage, Stage::Landed) {
            break;
        }
    }
    landings
}

impl Reading {
    /// Ask the source for `drag`'s types for drop `id`. A type whose pipe
    /// cannot be made arrives empty.
    fn start(id: u32, drag: &Drag) -> Self {
        let Wanted {
            uris,
            link,
            text,
            key,
        } = &drag.wanted;
        let pipes = [uris, link, text, key].map(|mime| {
            let mime = mime.clone()?;
            let (reader, writer) = std::io::pipe().ok()?;
            nonblocking(&reader).then_some(())?;
            drag.offer.receive(mime, writer.as_fd());
            // The source holds its own copy of the write end, so the read ends when the source closes it
            drop(writer);
            Some(reader)
        });
        Reading {
            id,
            pipes,
            bytes: Default::default(),
            largest: LARGEST,
            retrieved: None,
        }
    }

    /// Whether every type has arrived or given up, and the portal answered
    /// for the key if it was asked.
    fn complete(&self) -> bool {
        self.pipes.iter().all(Option::is_none)
            && self
                .retrieved
                .as_ref()
                .is_none_or(|r| r.lock().unwrap_or_else(|e| e.into_inner()).is_some())
    }

    /// Whether the text or the titled link is still arriving, which a
    /// deadline bounds.
    fn timed(&self) -> bool {
        self.pipes[LINK].is_some() || self.pipes[TEXT].is_some()
    }

    /// Stop waiting for the text and the titled link, past their deadline:
    /// what they had not finished sending is not delivered.
    fn time_out(&mut self) {
        for k in [LINK, TEXT] {
            if self.pipes[k].take().is_some() {
                self.bytes[k].clear();
            }
        }
    }

    /// Stop waiting for every type and for the portal, another drop having
    /// come: the drop lands with what came, which keeps an answer the portal
    /// already gave, and a key not yet redeemed is not redeemed.
    fn cut(&mut self) {
        for k in [URIS, LINK, TEXT, KEY] {
            if self.pipes[k].take().is_some() {
                self.bytes[k].clear();
            }
        }
        self.bytes[KEY].clear();
        self.retrieved = self
            .retrieved
            .take()
            .filter(|r| r.lock().unwrap_or_else(|e| e.into_inner()).is_some());
    }

    /// The key the portal's type sent once it has all come: as GTK writes
    /// it, with a terminal NUL, or as KDE does, without; none when it holds
    /// a NUL inside or is not UTF-8.
    fn key(&self) -> Option<&str> {
        if self.pipes[KEY].is_some() {
            return None;
        }
        let bytes = &self.bytes[KEY];
        let bytes = bytes.strip_suffix(b"\0").unwrap_or(bytes);
        std::str::from_utf8(bytes)
            .ok()
            .filter(|key| !key.is_empty() && !key.contains('\0'))
    }

    /// The drop's number and items, and whether its files are the portal's:
    /// a type still arriving gives nothing, and the portal's paths for the
    /// key, when it gave some, are the files.
    fn items(self) -> (u32, Dropped, bool) {
        let Reading {
            id,
            pipes,
            mut bytes,
            retrieved,
            ..
        } = self;
        for (pipe, bytes) in pipes.iter().zip(&mut bytes) {
            if pipe.is_some() {
                bytes.clear();
            }
        }
        let [uris, link, text, _] = bytes;
        let mut dropped = items(&uris, &link, &text);
        let given = retrieved.and_then(|r| r.lock().unwrap_or_else(|e| e.into_inner()).take());
        let given = given.filter(|paths| !paths.is_empty());
        let portal = given.is_some();
        if let Some(paths) = given {
            dropped.paths = paths;
        }
        (id, dropped, portal)
    }

    /// Read some of what type `k`'s pipe holds, a bounded batch so other
    /// pipes and the display are heard between: its end closes the pipe, and
    /// an error or more than `largest` bytes closes it with nothing, as does
    /// an end of the text or the titled link after `deadline`.
    fn read(&mut self, k: usize, deadline: Instant) {
        let Some(pipe) = self.pipes[k].as_mut() else {
            return;
        };
        let mut chunk = [0; 1 << 16];
        for _ in 0..16 {
            match pipe.read(&mut chunk) {
                Ok(0) => {
                    self.pipes[k] = None;
                    if [LINK, TEXT].contains(&k) && Instant::now() >= deadline {
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

/// Ask the document portal for the files of drop key `key`, on a thread of
/// its own: the answer, empty when it gave none.
fn retrieve(
    key: String,
    answered: Arc<PipeWriter>,
    journal: impl Fn(String) + Send + 'static,
) -> Answer<PathBuf> {
    on_worker("exact-drop-key", answered, move || {
        exact_painted::file_transfer::Portal::session()
            .retrieve(&key)
            .unwrap_or_else(|e| {
                journal(format!(
                    "drop: the document portal gave no files for its key: {e}"
                ));
                Vec::new()
            })
    })
}

/// Keep the files of `paths` that the sandbox shares with the host, each
/// resolved and checked with its folder held, on a thread of its own, since
/// a path on a slow mount can take long; the journal hears of each left out.
fn check(
    paths: Vec<PathBuf>,
    answered: Arc<PipeWriter>,
    journal: impl Fn(String) + Send + 'static,
) -> Answer<Checked> {
    on_worker("exact-drop-check", answered, move || {
        let shares = sandbox::Shares::here();
        paths
            .into_iter()
            .filter_map(|path| {
                let checked = shares.resolve(&path).and_then(|real| Checked::of(&real));
                if checked.is_none() {
                    journal(format!(
                        "drop: {} was left out: the sandbox does not share it with the host",
                        path.display()
                    ));
                }
                checked
            })
            .collect()
    })
}

/// `work` on a thread of its own: its answer once it comes, then a byte to
/// `answered`; an empty answer when no thread can start.
fn on_worker<T: Send + 'static>(
    name: &str,
    answered: Arc<PipeWriter>,
    work: impl FnOnce() -> Vec<T> + Send + 'static,
) -> Answer<T> {
    let answer = Arc::new(Mutex::new(None));
    let to = answer.clone();
    let run = move || {
        let found = work();
        *to.lock().unwrap_or_else(|e| e.into_inner()) = Some(found);
        let _ = (&*answered).write(&[1]);
    };
    if std::thread::Builder::new()
        .name(name.into())
        .spawn(run)
        .is_err()
    {
        *answer.lock().unwrap_or_else(|e| e.into_inner()) = Some(Vec::new());
    }
    answer
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
    /// its URI list, its titled link, the first of its text types and, in a
    /// sandbox, the first of the portal's key types; none when it offers none
    /// of them.
    fn of(types: &[String], sandboxed: bool) -> Option<Self> {
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
            key: lift::KEYS.into_iter().find_map(find).filter(|_| sandboxed),
        };
        (wanted != Wanted::default()).then_some(wanted)
    }

    /// The type the window accepts while a node under the drag takes the
    /// drop.
    fn first(&self) -> Option<&str> {
        [&self.uris, &self.link, &self.text, &self.key]
            .into_iter()
            .flatten()
            .next()
            .map(String::as_str)
    }
}

/// The items a drop's URI list (RFC 2483), titled link and text give: each
/// local `file:` URI a path, the first other URI the link, else the titled
/// link's URL, and the text as it came.
pub(super) fn items(uris: &[u8], link: &[u8], text: &[u8]) -> Dropped {
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

/// `text`, its ASCII spaces and controls trimmed, as a link that is not a
/// file: a line with a URI scheme (RFC 3986: a letter, then letters, digits,
/// `+`, `-` or `.`) and no ASCII space or control inside, the bytes D1's URL
/// test refuses, so a URL D1 admits arrives whole. The app checks the URL
/// before following it (D1), as GTK's URI-list reader and Android's leave
/// its parts.
fn web_link(text: &str) -> Option<&str> {
    let blank = |c: char| c <= ' ' || c == '\u{7f}';
    let text = text.trim_matches(blank);
    let (scheme, _) = text.split_once(':')?;
    let mut letters = scheme.chars();
    let valid = letters.next().is_some_and(|c| c.is_ascii_alphabetic())
        && letters.all(|c| c.is_ascii_alphanumeric() || "+-.".contains(c))
        && !text.chars().any(blank);
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
                let Some(at) = state.seats.iter().position(|s| s.name == name) else {
                    return;
                };
                let Seat {
                    seat,
                    device,
                    pointer,
                    keyboard,
                    ..
                } = state.seats.remove(at);
                if let Some(keyboard) = keyboard {
                    keyboard.release();
                }
                if state.drag.as_ref().is_some_and(|d| d.device == device.id()) {
                    state.left();
                }
                state.seat_gone(name);
                if let Some(pointer) = pointer {
                    pointer.release();
                    state.tell(Heard::Pointer {
                        seat: name,
                        pointed: Pointed::Lost,
                        modifiers: None,
                    });
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
                    .then(|| Wanted::of(&types, sandbox::sandboxed()))
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
                    // The event loop takes this drop's target first; a drop still waiting then lands with what
                    // came, and this drop's enters wait for it there
                    let (number, point) = (drag.number, drag.point);
                    state.tell(Heard::Dropped { number, point });
                    let journal = state.journal();
                    for released in &mut state.released {
                        released.progress.cut(&journal);
                    }
                    state.released.push(Released {
                        drag,
                        deadline: Instant::now() + DEADLINE,
                        progress: Progress {
                            stage: Stage::Waiting,
                            cut: false,
                        },
                    });
                    state.land();
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
