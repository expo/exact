//! Drags over the window, from another app or this one (LLP 1094.001 D8).
//! Wayland's own data device, on winit's connection, says where a drag is
//! and what it offers by MIME type; the window takes a drop as a copy, and
//! the drop's items arrive through pipes off the main thread.
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
    Connection, Dispatch, EventQueue, Proxy, QueueHandle,
};
use std::ffi::OsString;
use std::io::{PipeReader, PipeWriter, Read};
use std::os::fd::{AsFd, AsRawFd};
use std::os::unix::ffi::OsStringExt;
use std::path::PathBuf;
use std::sync::Mutex;
use std::thread::JoinHandle;
use winit::event_loop::EventLoopProxy;
use winit::raw_window_handle::{
    HasDisplayHandle, HasWindowHandle, RawDisplayHandle, RawWindowHandle,
};
use winit::window::Window;

/// The URI list a file manager or a browser offers its files and links as.
const URI_LIST: &str = "text/uri-list";
/// The link a browser offers with its title, as Firefox and Chromium do and
/// as GTK and Qt read it: UTF-16, the URL, a line break and the title.
const MOZ_URL: &str = "text/x-moz-url";
/// The plain text types a drag may offer, the first offered taken: UTF-8
/// by name, then X11's name for it, then text in no named charset.
const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "UTF8_STRING", "text/plain"];

/// The window's side of drags from other apps: the drag over the window,
/// and the thread that hears the compositor on the window's connection.
/// Winit sleeps through events that are not its own, so the data devices
/// have a thread of their own, which hands each event to the event loop.
pub(super) struct Drops {
    connection: Connection,
    drag: Option<Drag>,
    stop: Option<PipeWriter>,
    hearing: Option<JoinHandle<()>>,
}

/// What the data devices heard, in order, and a drop whose items arrived.
pub(in crate::window) enum Heard {
    Entered(Drag),
    Moved((f32, f32)),
    Left,
    Dropped,
    Landed(Landed),
}

/// A drop whose items arrived: the presenter's drop, what came, and the
/// offer, which is finished once the presenter has them.
pub(in crate::window) struct Landed {
    id: u32,
    dropped: Dropped,
    offer: WlDataOffer,
}

/// The drag over the window: its offer, the serial its enter came with,
/// the types the window reads from it, where it is, and whether the window
/// last said it takes the drop there.
pub(in crate::window) struct Drag {
    offer: WlDataOffer,
    serial: u32,
    wanted: Wanted,
    point: (f32, f32),
    takes: Option<bool>,
}

/// The data devices, one a seat, on the hearing thread.
struct Devices {
    surface: ObjectId,
    manager: WlDataDeviceManager,
    seats: Vec<(u32, WlSeat, WlDataDevice)>,
    /// Whether a drag the window reads is over its surface.
    over: bool,
    proxy: EventLoopProxy<super::Event>,
}

/// The types the window reads from an offer: its URI list, its titled
/// link and its text.
#[derive(Default, PartialEq, Debug)]
struct Wanted {
    uris: Option<String>,
    link: Option<String>,
    text: Option<String>,
}

/// An offer's MIME types, as the compositor lists them before its enter.
#[derive(Default)]
struct Offered(Mutex<Vec<String>>);

impl Drops {
    /// The data devices of `window`'s seats, on winit's connection; none
    /// when the window is not Wayland's or the compositor offers no drags.
    pub(super) fn new(window: &Window, proxy: EventLoopProxy<super::Event>) -> Option<Self> {
        let RawDisplayHandle::Wayland(display) = window.display_handle().ok()?.as_raw() else {
            return None;
        };
        let RawWindowHandle::Wayland(surface) = window.window_handle().ok()?.as_raw() else {
            return None;
        };
        // The display outlives Drops: the window stops its thread when the event loop exits (`App::exiting`)
        let backend = unsafe { Backend::from_foreign_display(display.display.as_ptr().cast()) };
        let surface =
            unsafe { ObjectId::from_ptr(WlSurface::interface(), surface.surface.as_ptr().cast()) }
                .ok()?;
        let connection = Connection::from_backend(backend);
        let (globals, queue) = registry_queue_init::<Devices>(&connection).ok()?;
        let handle = queue.handle();
        let manager = globals
            .bind::<WlDataDeviceManager, _, _>(&handle, 1..=3, ())
            .ok()?;
        let mut devices = Devices {
            surface,
            manager,
            seats: Vec::new(),
            over: false,
            proxy,
        };
        globals.contents().with_list(|list| {
            for global in list.iter().filter(|g| g.interface == "wl_seat") {
                devices.add_seat(globals.registry(), global.name, global.version, &handle);
            }
        });
        let _ = connection.flush();
        let (stopped, stop) = std::io::pipe().ok()?;
        let hearing = {
            let connection = connection.clone();
            std::thread::Builder::new()
                .name("exact-drags".into())
                .spawn(move || hear(&connection, queue, devices, &stopped))
                .ok()?
        };
        Some(Self {
            connection,
            drag: None,
            stop: Some(stop),
            hearing: Some(hearing),
        })
    }

    /// Tell the presenter what the compositor said, and answer the
    /// compositor: the window takes a drop where a node under the drag does,
    /// and a dropped drag's items are read from its source.
    pub(super) fn heard<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        heard: Heard,
        proxy: &EventLoopProxy<super::Event>,
    ) {
        match heard {
            Heard::Entered(drag) => {
                if let Some(stale) = self.drag.replace(drag) {
                    stale.offer.destroy();
                    p.drag_left();
                }
                self.over(p);
            }
            Heard::Moved(point) => {
                if let Some(drag) = self.drag.as_mut() {
                    drag.point = point;
                    self.over(p);
                }
            }
            Heard::Left => {
                if let Some(drag) = self.drag.take() {
                    drag.offer.destroy();
                    p.drag_left();
                }
            }
            Heard::Dropped => {
                if let Some(drag) = self.drag.take() {
                    match p.drop_released(drag.point, p.modifiers()) {
                        Some(id) => read(id, drag, proxy.clone()),
                        None => drag.offer.destroy(),
                    }
                }
            }
            Heard::Landed(landed) => {
                p.drop_landed(landed.id, &landed.dropped);
                if landed.offer.version() >= 3 {
                    landed.offer.finish();
                }
                landed.offer.destroy();
            }
        }
        let _ = self.connection.flush();
    }

    /// The drag moved: the presenter hears it, and the compositor hears
    /// whether a node under it takes the drop now, when that changed.
    fn over<D: DataSource>(&mut self, p: &mut Presenter<D>) {
        let Some(drag) = self.drag.as_mut() else {
            return;
        };
        let takes = p.drag_over(drag.point, p.modifiers()).is_some();
        if drag.takes != Some(takes) {
            let mime = takes.then(|| drag.wanted.first()).flatten();
            drag.offer.accept(drag.serial, mime.map(str::to_owned));
            drag.takes = Some(takes);
        }
    }
}

impl Drop for Drops {
    fn drop(&mut self) {
        // Closing the pipe wakes the thread, which then leaves the connection alone
        drop(self.stop.take());
        if let Some(hearing) = self.hearing.take() {
            let _ = hearing.join();
        }
        if let Some(drag) = self.drag.take() {
            drag.offer.destroy();
        }
    }
}

/// The hearing thread: dispatch the data devices' events as they come,
/// until `stopped` closes or the connection fails.
fn hear(
    connection: &Connection,
    mut queue: EventQueue<Devices>,
    mut devices: Devices,
    stopped: &PipeReader,
) {
    loop {
        if queue.dispatch_pending(&mut devices).is_err() {
            return;
        }
        // A read is prepared before the poll, as libwayland's threads share one socket
        let Some(guard) = queue.prepare_read() else {
            continue;
        };
        let _ = connection.flush();
        let mut fds =
            [guard.connection_fd().as_raw_fd(), stopped.as_raw_fd()].map(|fd| libc::pollfd {
                fd,
                events: libc::POLLIN,
                revents: 0,
            });
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, -1) };
        if ready < 0 {
            if std::io::Error::last_os_error().kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return;
        }
        if fds[1].revents != 0 {
            return;
        }
        match guard.read() {
            Ok(_) => {}
            Err(WaylandError::Io(e)) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => return,
        }
    }
}

impl Devices {
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
}

impl Wanted {
    /// What an offer of `types` gives the window: its URI list, its titled
    /// link and the first of its text types; none when it offers none.
    fn of(types: &[String]) -> Option<Self> {
        let has = |t: &str| types.iter().any(|offered| offered.eq_ignore_ascii_case(t));
        let wanted = Wanted {
            uris: has(URI_LIST).then(|| URI_LIST.to_owned()),
            link: has(MOZ_URL).then(|| MOZ_URL.to_owned()),
            text: TEXTS.into_iter().find(|t| has(t)).map(str::to_owned),
        };
        (wanted != Wanted::default()).then_some(wanted)
    }

    /// The type the window says it takes while a node under the drag takes
    /// the drop.
    fn first(&self) -> Option<&str> {
        [&self.uris, &self.link, &self.text]
            .into_iter()
            .flatten()
            .next()
            .map(String::as_str)
    }
}

/// Ask the source for drop `id`'s items, and read them off the main thread:
/// the event loop hears them as `Landed`.
fn read(id: u32, drag: Drag, proxy: EventLoopProxy<super::Event>) {
    let Wanted { uris, link, text } = &drag.wanted;
    let mut pipes = [uris, link, text].map(|mime| {
        let (reader, writer) = std::io::pipe().ok()?;
        drag.offer.receive(mime.clone()?, writer.as_fd());
        // The source holds its own copy of the write end, so the read ends when the source closes it
        drop(writer);
        Some(reader)
    });
    let offer = drag.offer;
    std::thread::Builder::new()
        .name("exact-drop".into())
        .spawn(move || {
            let [uris, link, text] = pipes.each_mut().map(|pipe| {
                let mut bytes = Vec::new();
                if let Some(pipe) = pipe {
                    let _ = pipe.read_to_end(&mut bytes);
                }
                bytes
            });
            let dropped = items(&uris, &link, &text);
            let landed = Landed { id, dropped, offer };
            let _ = proxy.send_event(super::Event::Drag(Heard::Landed(landed)));
        })
        .map(drop)
        .unwrap_or_else(|e| eprintln!("exact: drop: no thread to read its items: {e}"));
}

/// The items a drop's URI list (RFC 2483), titled link and text give: each
/// local `file:` URI a path, the first other URI the link, else the titled
/// link's URL, and the text as it came.
fn items(uris: &[u8], link: &[u8], text: &[u8]) -> Dropped {
    let mut dropped = Dropped {
        text: String::from_utf8_lossy(text).into_owned(),
        ..Default::default()
    };
    let list = String::from_utf8_lossy(uris);
    for line in list
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty() && !l.starts_with('#'))
    {
        match line.get(..5).filter(|s| s.eq_ignore_ascii_case("file:")) {
            Some(_) => dropped.paths.extend(file_path(&line[5..])),
            None if dropped.url.is_empty() => dropped.url = line.to_owned(),
            None => {}
        }
    }
    if dropped.url.is_empty() {
        let titled = utf16_or_utf8(link);
        let url = titled.lines().next().unwrap_or_default().trim();
        if !url
            .get(..5)
            .is_some_and(|s| s.eq_ignore_ascii_case("file:"))
        {
            dropped.url = url.to_owned();
        }
    }
    dropped
}

/// Text a browser wrote as UTF-16, with or without a byte order mark, as
/// Mozilla's types are, or as UTF-8 when it holds no NUL byte to be UTF-16.
fn utf16_or_utf8(bytes: &[u8]) -> String {
    let (big, bytes) = match bytes {
        [0xFE, 0xFF, rest @ ..] => (true, rest),
        [0xFF, 0xFE, rest @ ..] => (false, rest),
        _ if bytes.contains(&0) => (false, bytes),
        _ => return String::from_utf8_lossy(bytes).into_owned(),
    };
    let units = bytes.as_chunks::<2>().0.iter().map(|&pair| {
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
/// for another machine's file.
fn file_path(rest: &str) -> Option<PathBuf> {
    let path = match rest.strip_prefix("//") {
        Some(authority) => {
            let at = authority.find('/')?;
            let host = &authority[..at];
            (host.is_empty() || host.eq_ignore_ascii_case("localhost"))
                .then_some(&authority[at..])?
        }
        None => rest.starts_with('/').then_some(rest)?,
    };
    let mut bytes = Vec::with_capacity(path.len());
    let mut at = path.bytes();
    while let Some(b) = at.next() {
        if b == b'%' {
            let hex = [at.next()?, at.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    Some(PathBuf::from(OsString::from_vec(bytes)))
}

impl Dispatch<WlRegistry, GlobalListContents> for Devices {
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
                if let Some(at) = state.seats.iter().position(|(n, ..)| *n == name) {
                    let (_, seat, device) = state.seats.remove(at);
                    if device.version() >= 2 {
                        device.release();
                    }
                    if seat.version() >= 5 {
                        seat.release();
                    }
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<WlSeat, ()> for Devices {
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

impl Dispatch<WlDataDeviceManager, ()> for Devices {
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

impl Dispatch<WlDataDevice, ()> for Devices {
    fn event(
        state: &mut Self,
        _: &WlDataDevice,
        event: wl_data_device::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_device::Event::Enter {
                serial,
                surface,
                x,
                y,
                id,
            } => {
                if std::mem::take(&mut state.over) {
                    state.tell(Heard::Left);
                }
                let Some(offer) = id else { return };
                let types = offer
                    .data::<Offered>()
                    .and_then(|o| o.0.lock().ok().map(|t| t.clone()));
                // A drag over the window's own surface that offers a file, text or a link, taken as a copy: a
                // decoration's surface takes none, and neither does a drag that offers nothing exact2 reads
                let wanted = (surface.id() == state.surface)
                    .then(|| Wanted::of(&types.unwrap_or_default()))
                    .flatten();
                let Some(wanted) = wanted else {
                    offer.accept(serial, None);
                    offer.destroy();
                    return;
                };
                if offer.version() >= 3 {
                    offer.set_actions(DndAction::Copy, DndAction::Copy);
                }
                state.over = true;
                let point = (x as f32, y as f32);
                state.tell(Heard::Entered(Drag {
                    offer,
                    serial,
                    wanted,
                    point,
                    takes: None,
                }));
            }
            wl_data_device::Event::Motion { x, y, .. } if state.over => {
                state.tell(Heard::Moved((x as f32, y as f32)));
            }
            // The drop ends the drag: the leave that follows it finds none
            wl_data_device::Event::Drop if std::mem::take(&mut state.over) => {
                state.tell(Heard::Dropped)
            }
            wl_data_device::Event::Leave if std::mem::take(&mut state.over) => {
                state.tell(Heard::Left)
            }
            wl_data_device::Event::Selection { id: Some(offer) } => offer.destroy(),
            _ => {}
        }
    }

    event_created_child!(Devices, WlDataDevice, [
        wl_data_device::EVT_DATA_OFFER_OPCODE => (WlDataOffer, Offered::default()),
    ]);
}

impl Dispatch<WlDataOffer, Offered> for Devices {
    fn event(
        _: &mut Self,
        _: &WlDataOffer,
        event: wl_data_offer::Event,
        offered: &Offered,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_data_offer::Event::Offer { mime_type } = event {
            if let Ok(mut types) = offered.0.lock() {
                types.push(mime_type);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_uri_list_gives_local_files_and_its_first_link() {
        let list = b"# from a file manager\r\nfile:///home/a/Photo%20one.jpg\r\nfile://localhost/home/a/b.pdf\r\nhttps://example.com/fuji\r\nfile://other.example/x\r\nhttps://example.com/second\r\n";
        let dropped = items(list, b"", b"");
        assert_eq!(
            dropped.paths,
            [
                PathBuf::from("/home/a/Photo one.jpg"),
                PathBuf::from("/home/a/b.pdf")
            ]
        );
        assert_eq!(dropped.url, "https://example.com/fuji");
        assert_eq!(dropped.text, "");
    }

    #[test]
    fn text_arrives_as_it_came_and_a_link_is_not_made_from_it() {
        let dropped = items(b"", b"", "Mount Fuji\nhttps://example.com/fuji".as_bytes());
        assert_eq!(dropped.text, "Mount Fuji\nhttps://example.com/fuji");
        assert_eq!(dropped.url, "");
        assert!(dropped.paths.is_empty());
    }

    #[test]
    fn a_browsers_titled_link_gives_the_link_the_uri_list_lacks() {
        let utf16 = |s: &str| {
            s.encode_utf16()
                .flat_map(u16::to_le_bytes)
                .collect::<Vec<_>>()
        };
        let titled = utf16("https://example.com/fuji\nA link to Fuji");
        let dropped = items(b"", &titled, b"https://example.com/fuji");
        assert_eq!(dropped.url, "https://example.com/fuji");
        assert_eq!(
            dropped.text, "https://example.com/fuji",
            "the text is the source's own"
        );
        let listed = items(b"https://example.com/listed\r\n", &titled, b"");
        assert_eq!(
            listed.url, "https://example.com/listed",
            "a URI list's link comes first"
        );
        let file = items(
            b"file:///tmp/a.jpg\r\n",
            &utf16("file:///tmp/a.jpg\nA photo"),
            b"",
        );
        assert_eq!(
            (file.url.as_str(), file.paths.len()),
            ("", 1),
            "a file: link is a file, not the link"
        );
        let bom: Vec<u8> = [0xFE, 0xFF]
            .into_iter()
            .chain(
                "https://example.com/b\n"
                    .encode_utf16()
                    .flat_map(u16::to_be_bytes),
            )
            .collect();
        assert_eq!(items(b"", &bom, b"").url, "https://example.com/b");
        assert_eq!(
            items(b"", b"https://example.com/u8\ntitle", b"").url,
            "https://example.com/u8"
        );
    }

    #[test]
    fn a_path_keeps_bytes_that_are_not_utf8() {
        let path = file_path("///tmp/caf%E9").unwrap();
        assert_eq!(path.as_os_str().as_encoded_bytes(), b"/tmp/caf\xe9");
        assert_eq!(file_path("//host.example/tmp/a"), None);
        assert_eq!(file_path("relative"), None);
        assert_eq!(file_path("///tmp/%4"), None);
    }

    #[test]
    fn an_offer_is_read_for_its_uri_list_and_its_first_text_type() {
        let types = |list: &[&str]| list.iter().map(|t| t.to_string()).collect::<Vec<_>>();
        let wanted = Wanted::of(&types(&["text/plain", "UTF8_STRING", "text/uri-list"])).unwrap();
        assert_eq!(wanted.uris.as_deref(), Some(URI_LIST));
        assert_eq!(wanted.link, None);
        assert_eq!(wanted.text.as_deref(), Some("UTF8_STRING"));
        assert_eq!(wanted.first(), Some(URI_LIST));
        let text = Wanted::of(&types(&["TEXT/PLAIN;charset=UTF-8"])).unwrap();
        assert_eq!(text.text.as_deref(), Some("text/plain;charset=utf-8"));
        assert_eq!(
            Wanted::of(&types(&["image/png", "application/x-example"])),
            None
        );
        let firefox = Wanted::of(&types(&[
            "text/x-moz-url",
            "_NETSCAPE_URL",
            "text/html",
            "text/plain",
        ]))
        .unwrap();
        assert_eq!(
            (firefox.link.as_deref(), firefox.first()),
            (Some(MOZ_URL), Some(MOZ_URL))
        );
    }
}
