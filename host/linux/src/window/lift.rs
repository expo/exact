//! Drags out of the window (LLP 1094.000 D13). The listening thread holds
//! its own pointer on each seat and hands the window each button with its
//! seat and serial, which winit's events lack and Wayland's `start_drag`
//! needs; a lift names its press by them, and the listener starts the drag
//! only while that press is still the seat's grab. The drag is a copy-only
//! data source whose types are ready at the lift, sent without blocking in
//! the listener's loop, with the node's picture as its icon.

use super::{nonblocking, Command, Heard, Listener};
use exact_painted::presenter::drag_out::{Lift, LiftPicture, Lifter, Offers};
use smithay_client_toolkit::reexports::client::{
    protocol::{
        wl_buffer::WlBuffer,
        wl_compositor::WlCompositor,
        wl_data_device_manager::DndAction,
        wl_data_source::{self, WlDataSource},
        wl_keyboard::{self, WlKeyboard},
        wl_pointer::{self, WlPointer},
        wl_seat::{self, WlSeat},
        wl_shm::{self, WlShm},
        wl_shm_pool::WlShmPool,
        wl_surface::WlSurface,
    },
    Connection, Dispatch, Proxy, QueueHandle, WEnum,
};
use smithay_client_toolkit::reexports::protocols::wp::viewporter::client::{
    wp_viewport::WpViewport, wp_viewporter::WpViewporter,
};
use std::cell::{Cell, RefCell};
use std::io::{ErrorKind, Write};
use std::os::fd::{AsFd, AsRawFd, OwnedFd};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// The types a lift offers its text as: UTF-8 by name, then X11's name for
/// it. Bare `text/plain` names no charset, and GTK 3 reads it as Latin-1.
pub(super) const TEXTS: [&str; 2] = ["text/plain;charset=utf-8", "UTF8_STRING"];
/// The URI list a lift offers its files, then its link, as (RFC 2483).
pub(super) const URI_LIST: &str = "text/uri-list";
/// Mozilla's titled link, the form Firefox and Chromium read a link in.
pub(super) const MOZ_URL: &str = "text/x-moz-url";
/// What the window's drags offer of D1's values: files are stage 7's.
pub(super) const OFFERS: Offers = Offers {
    text: true,
    url: true,
    files: false,
};
/// How long after a target asks for a type its bytes may take to go: a
/// target that stops reading then gets no more.
const DEADLINE: Duration = Duration::from_secs(10);
/// The left button's code (Linux's `BTN_LEFT`).
const LEFT: u32 = 0x110;

/// A press the listener heard: its seat, by the seat's global name, and
/// its serial.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(in crate::window) struct Press {
    pub(in crate::window) seat: u32,
    pub(in crate::window) serial: u32,
}

/// The event loop's side of lifts: the window's left press while it alone
/// is held, which the window keeps, and a lift the presenter made, waiting
/// to go to the listener.
#[derive(Default)]
pub(super) struct Lifts {
    held: Rc<Cell<Option<Press>>>,
    lifted: Rc<RefCell<Option<(Press, Lift)>>>,
}

impl Lifts {
    /// The cell the window keeps its held left press in.
    pub(super) fn held(&self) -> Rc<Cell<Option<Press>>> {
        self.held.clone()
    }

    /// The presenter's lifter: it takes a lift while the window holds the
    /// left button alone, and keeps it for `take`.
    pub(super) fn lifter(&self) -> Lifter {
        let (held, lifted) = (self.held.clone(), self.lifted.clone());
        Lifter {
            offers: OFFERS,
            lift: Box::new(move |lift| {
                let Some(press) = held.get() else {
                    return false;
                };
                *lifted.borrow_mut() = Some((press, lift));
                true
            }),
        }
    }

    /// The lift waiting to go, as the listener's command.
    pub(super) fn take(&self) -> Option<Command> {
        let (press, lift) = self.lifted.borrow_mut().take()?;
        let mut offers: Vec<(&'static str, Arc<[u8]>)> = Vec::new();
        if !lift.url.is_empty() {
            offers.push((
                URI_LIST,
                uri_list(&[] as &[&Path], &lift.url).into_bytes().into(),
            ));
            offers.push((MOZ_URL, moz_url(&lift.url).into()));
        }
        if !lift.text.is_empty() {
            let text: Arc<[u8]> = lift.text.as_bytes().into();
            offers.extend(TEXTS.map(|mime| (mime, text.clone())));
        }
        let icon = lift
            .picture
            .as_ref()
            .map(|picture| Icon::of(picture, lift.hotspot));
        Some(Command::Lift {
            press,
            offers,
            icon,
        })
    }
}

/// `url` as `text/x-moz-url`: UTF-16 in the host's byte order with no byte
/// order mark, as Firefox writes it, the URL then a line break then a title.
/// The title is the URL, since some Chromium builds read a link only from
/// two lines that both hold something.
pub(super) fn moz_url(url: &str) -> Vec<u8> {
    format!("{url}\n{url}")
        .encode_utf16()
        .flat_map(u16::to_ne_bytes)
        .collect()
}

/// The URI list of `files`, then `url`: each file a `file:` URI with no
/// host (RFC 8089), its path's bytes percent-encoded but for RFC 3986's
/// unreserved characters and `/`, each line ended by CRLF.
pub(super) fn uri_list(files: &[impl AsRef<Path>], url: &str) -> String {
    let mut list = String::new();
    for file in files {
        list.push_str("file://");
        for &b in file.as_ref().as_os_str().as_bytes() {
            if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                list.push(b as char);
            } else {
                list.push_str(&format!("%{b:02X}"));
            }
        }
        list.push_str("\r\n");
    }
    if !url.is_empty() {
        list.push_str(url);
        list.push_str("\r\n");
    }
    list
}

/// A drag's icon as Wayland places it, by whole points from the pointer:
/// the picture grown outward to whole points, the pixels it gains clear, as
/// premultiplied ARGB in the host's byte order (`wl_shm`'s ARGB8888).
#[derive(Debug, PartialEq)]
pub(super) struct Icon {
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) argb: Vec<u8>,
    /// The buffer's scale at an integer scale; none at a fractional one,
    /// which a viewport shows at `size`.
    pub(super) scale: Option<i32>,
    /// Its size in points.
    pub(super) size: (i32, i32),
    /// Where its corner goes from the pointer, in points.
    pub(super) offset: (i32, i32),
}

impl Icon {
    /// `picture`, held at `hotspot` points from its corner.
    pub(super) fn of(picture: &LiftPicture, hotspot: (f32, f32)) -> Self {
        let s = picture.scale.max(1. / 64.);
        // Each axis grows on the near side until the press is whole points in, then on the far side to whole points
        let axis = |hot: f32, pixels: u32| {
            let press = (hot * s).round();
            let points = (press / s).ceil();
            let before = (points * s - press).round().max(0.) as u32;
            let size = ((before + pixels) as f32 / s).ceil() as i32;
            let total = ((size as f32 * s).round() as u32).max(before + pixels);
            (before, total, size, points as i32)
        };
        let (left, width, points_wide, x) = axis(hotspot.0, picture.width);
        let (top, height, points_high, y) = axis(hotspot.1, picture.height);
        let mut argb = vec![0; width as usize * height as usize * 4];
        for row in 0..picture.height as usize {
            let from =
                &picture.rgba[row * picture.width as usize * 4..][..picture.width as usize * 4];
            let to = &mut argb[((top as usize + row) * width as usize + left as usize) * 4..]
                [..picture.width as usize * 4];
            for (to, from) in to.chunks_exact_mut(4).zip(from.chunks_exact(4)) {
                let pixel = u32::from_be_bytes([from[3], from[0], from[1], from[2]]);
                to.copy_from_slice(&pixel.to_ne_bytes());
            }
        }
        Icon {
            width: width as i32,
            height: height as i32,
            argb,
            scale: (s.fract() == 0.).then_some(s as i32),
            size: (points_wide, points_high),
            offset: (-x, -y),
        }
    }
}

/// The listener's side of lifts: the globals an icon needs, the window's
/// surface a drag starts from, the sources not yet retired and the types
/// being sent.
pub(super) struct Out {
    compositor: Option<WlCompositor>,
    shm: Option<WlShm>,
    viewporter: Option<WpViewporter>,
    origin: Option<WlSurface>,
    sources: Vec<Source>,
    sends: Vec<Sending>,
}

/// A drag the window started, or asked for: its press, its source, its
/// types and their bytes, its icon, and whether the press's gesture ended.
struct Source {
    press: Press,
    source: WlDataSource,
    offers: Vec<(&'static str, Arc<[u8]>)>,
    icon: Option<(WlSurface, WlBuffer, Option<WpViewport>)>,
    ended: bool,
}

/// A type going to a target through its pipe, due by `deadline`.
struct Sending {
    pipe: std::fs::File,
    bytes: Arc<[u8]>,
    at: usize,
    deadline: Instant,
}

/// The pointer a seat holds for the listener: whether it is over the
/// window's surface, the buttons held, the last press's serial, the buttons
/// whose press the window heard, and the wheel's frame so far.
pub(super) struct Pointer {
    pointer: WlPointer,
    on: bool,
    held: Vec<u32>,
    press: Option<u32>,
    told: Vec<u32>,
    wheel: Wheel,
}

/// A frame's wheel: its distance in points and its discrete steps, on each
/// axis, by Wayland's sign (down and right are positive).
#[derive(Default)]
struct Wheel {
    distance: (f64, f64),
    steps: (i32, i32),
    turned: bool,
}

/// What a seat's pointer did on the window's surface, in the seat's order:
/// it entered at a point, left, moved to a point, a button by its Linux
/// code went down or up with its serial, the wheel turned (by lines when
/// the frame had discrete steps, else by points, by Wayland's sign), or the
/// seat's pointer went away.
pub(in crate::window) enum Pointed {
    Entered((f32, f32)),
    Left,
    Moved((f32, f32)),
    Button {
        serial: u32,
        button: u32,
        pressed: bool,
    },
    Wheel {
        lines: bool,
        x: f32,
        y: f32,
    },
    Lost,
}

impl Out {
    pub(super) fn new(
        globals: &smithay_client_toolkit::reexports::client::globals::GlobalList,
        handle: &QueueHandle<Listener>,
        connection: &Connection,
        surface: &smithay_client_toolkit::reexports::client::backend::ObjectId,
    ) -> Self {
        Out {
            compositor: globals.bind(handle, 1..=6, ()).ok(),
            shm: globals.bind(handle, 1..=1, ()).ok(),
            viewporter: globals.bind(handle, 1..=1, ()).ok(),
            origin: WlSurface::from_id(connection, surface.clone()).ok(),
            sources: Vec::new(),
            sends: Vec::new(),
        }
    }

    /// Each pipe being sent to, by its place, with its descriptor.
    pub(super) fn pipes(&self) -> impl Iterator<Item = (usize, i32)> + '_ {
        self.sends
            .iter()
            .enumerate()
            .map(|(k, s)| (k, s.pipe.as_raw_fd()))
    }

    /// How long until the next send is due, in milliseconds.
    pub(super) fn due(&self, now: Instant) -> Option<i32> {
        self.sends
            .iter()
            .map(|s| {
                s.deadline
                    .saturating_duration_since(now)
                    .as_millis()
                    .min(i32::MAX as u128) as i32
            })
            .min()
    }

    /// Send some of each ready pipe's bytes, a bounded batch so the display
    /// and other pipes are heard between, and close each pipe that is done,
    /// failed or past its deadline.
    pub(super) fn send(&mut self, ready: &[usize]) {
        let now = Instant::now();
        for &k in ready {
            let sending = &mut self.sends[k];
            for _ in 0..16 {
                if sending.at == sending.bytes.len() {
                    break;
                }
                let end = (sending.at + (1 << 16)).min(sending.bytes.len());
                match sending.pipe.write(&sending.bytes[sending.at..end]) {
                    Ok(n) => sending.at += n,
                    Err(e) if e.kind() == ErrorKind::Interrupted => {}
                    Err(e) if e.kind() == ErrorKind::WouldBlock => break,
                    Err(_) => {
                        sending.at = sending.bytes.len();
                        break;
                    }
                }
            }
        }
        self.sends
            .retain(|s| s.at < s.bytes.len() && now < s.deadline);
    }

    /// Retire every source, as the window closes.
    pub(super) fn close(&mut self) {
        for source in self.sources.drain(..) {
            source.retire();
        }
    }
}

impl Pointer {
    /// Let go of the pointer, its seat or its capability gone.
    pub(super) fn release(self) {
        if self.pointer.version() >= 3 {
            self.pointer.release();
        }
    }
}

impl Source {
    fn retire(self) {
        self.source.destroy();
        if let Some((surface, buffer, viewport)) = self.icon {
            if let Some(viewport) = viewport {
                viewport.destroy();
            }
            surface.destroy();
            buffer.destroy();
        }
    }
}

impl Listener {
    /// Start the drag `press` lifted, if that press is still its seat's grab
    /// with the left button alone held on the window's surface; else the
    /// lift is stale, and the release or press that made it so reaches the
    /// window as usual.
    pub(super) fn lift(
        &mut self,
        press: Press,
        offers: Vec<(&'static str, Arc<[u8]>)>,
        icon: Option<Icon>,
    ) {
        let Some(seat) = self.seats.iter().find(|s| s.name == press.seat) else {
            return;
        };
        let grabbed = seat
            .pointer
            .as_ref()
            .is_some_and(|p| p.on && p.held == [LEFT] && p.press == Some(press.serial));
        let Some(origin) = self
            .out
            .origin
            .as_ref()
            .filter(|_| grabbed && self.manager.version() >= 3)
        else {
            return;
        };
        let handle = &self.handle;
        let source = self.manager.create_data_source(handle, ());
        for (mime, _) in &offers {
            source.offer((*mime).to_owned());
        }
        // A copy alone, so a drop never moves anything of the app's
        source.set_actions(DndAction::Copy);
        let surface = icon
            .as_ref()
            .and_then(|_| self.out.compositor.as_ref().zip(self.out.shm.as_ref()))
            .map(|(compositor, _)| compositor.create_surface(handle, ()));
        seat.device
            .start_drag(Some(&source), origin, surface.as_ref(), press.serial);
        let icon = surface
            .zip(icon)
            .and_then(|(surface, icon)| self.show(surface, &icon));
        self.out.sources.push(Source {
            press,
            source,
            offers,
            icon,
            ended: false,
        });
    }

    /// Show `icon` on `surface`, which `start_drag` made the drag's icon.
    fn show(
        &self,
        surface: WlSurface,
        icon: &Icon,
    ) -> Option<(WlSurface, WlBuffer, Option<WpViewport>)> {
        let shm = self.out.shm.as_ref()?;
        // A buffer with no pixels is a protocol error: such an icon is left out
        let length = icon
            .width
            .checked_mul(4)
            .and_then(|stride| stride.checked_mul(icon.height))
            .filter(|&length| {
                icon.width > 0 && icon.height > 0 && length as usize == icon.argb.len()
            });
        let Some(length) = length else {
            surface.destroy();
            return None;
        };
        let stride = icon.width * 4;
        let memory = unsafe { libc::memfd_create(c"exact-drag-icon".as_ptr(), libc::MFD_CLOEXEC) };
        if memory < 0 {
            surface.destroy();
            return None;
        }
        let mut memory = std::fs::File::from(unsafe {
            <OwnedFd as std::os::fd::FromRawFd>::from_raw_fd(memory)
        });
        if memory.write_all(&icon.argb).is_err() {
            surface.destroy();
            return None;
        }
        let pool = shm.create_pool(memory.as_fd(), length, &self.handle, ());
        let buffer = pool.create_buffer(
            0,
            icon.width,
            icon.height,
            stride,
            wl_shm::Format::Argb8888,
            &self.handle,
            (),
        );
        pool.destroy();
        let viewport = match icon.scale {
            Some(scale) => {
                if surface.version() >= 3 {
                    surface.set_buffer_scale(scale);
                }
                None
            }
            None => self.out.viewporter.as_ref().map(|viewporter| {
                let viewport = viewporter.get_viewport(&surface, &self.handle, ());
                viewport.set_destination(icon.size.0, icon.size.1);
                viewport
            }),
        };
        let (x, y) = icon.offset;
        if surface.version() >= 5 {
            surface.attach(Some(&buffer), 0, 0);
            surface.offset(x, y);
        } else {
            surface.attach(Some(&buffer), x, y);
        }
        if surface.version() >= 4 {
            surface.damage_buffer(0, 0, icon.width, icon.height);
        } else {
            surface.damage(0, 0, icon.size.0, icon.size.1);
        }
        surface.commit();
        Some((surface, buffer, viewport))
    }

    /// The gesture of `source`'s press ended: the window lets go of the
    /// press its release never reached, and so does the seat's pointer.
    fn ended(&mut self, at: usize) {
        let source = &mut self.out.sources[at];
        if std::mem::replace(&mut source.ended, true) {
            return;
        }
        let press = source.press;
        if let Some(pointer) = self
            .seats
            .iter_mut()
            .find(|s| s.name == press.seat)
            .and_then(|s| s.pointer.as_mut())
            .filter(|p| p.press == Some(press.serial))
        {
            pointer.held.retain(|&b| b != LEFT);
            pointer.told.retain(|&b| b != LEFT);
        }
        self.tell(Heard::Ended(press));
    }

    /// Retire seat `seat`'s sources whose gesture had not ended, its seat or
    /// its pointer gone: the window lets go of their presses. A source past
    /// its drop still serves its types until the compositor retires it.
    pub(super) fn seat_gone(&mut self, seat: u32) {
        while let Some(at) = self
            .out
            .sources
            .iter()
            .position(|s| s.press.seat == seat && !s.ended)
        {
            self.ended(at);
            self.out.sources.remove(at).retire();
        }
    }

    fn retire(&mut self, source: &WlDataSource) {
        if let Some(at) = self.out.sources.iter().position(|s| s.source == *source) {
            self.ended(at);
            self.out.sources.remove(at).retire();
        }
    }
}

impl Dispatch<WlSeat, ()> for Listener {
    fn event(
        state: &mut Self,
        seat: &WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        handle: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };
        let Some(entry) = state.seats.iter_mut().find(|s| s.seat == *seat) else {
            return;
        };
        let name = entry.name;
        let keyboard = capabilities.contains(wl_seat::Capability::Keyboard);
        match (keyboard, entry.keyboard.is_some()) {
            (true, false) => entry.keyboard = Some(Keyboard::new(seat.get_keyboard(handle, name))),
            (false, true) => {
                if let Some(keyboard) = entry.keyboard.take() {
                    keyboard.release();
                }
            }
            _ => {}
        }
        let pointer = capabilities.contains(wl_seat::Capability::Pointer);
        match (pointer, entry.pointer.is_some()) {
            (true, false) => {
                entry.pointer = Some(Pointer {
                    pointer: seat.get_pointer(handle, name),
                    on: false,
                    held: Vec::new(),
                    press: None,
                    told: Vec::new(),
                    wheel: Wheel::default(),
                });
            }
            (false, true) => {
                if let Some(pointer) = entry.pointer.take() {
                    pointer.release();
                }
                state.seat_gone(name);
                state.tell_pointed(name, Pointed::Lost);
            }
            _ => {}
        }
    }
}

/// A seat's keyboard as the listener holds it, for the modifiers in force
/// at each pointer event: the effective mask of xkb's modifier indices,
/// whose real modifiers (Shift 0, Control 2, Mod1 3, Mod4 6) winit names
/// Shift, Control, Alt and Super.
pub(super) struct Keyboard {
    keyboard: WlKeyboard,
    modifiers: u32,
}

impl Keyboard {
    fn new(keyboard: WlKeyboard) -> Self {
        Keyboard {
            keyboard,
            modifiers: 0,
        }
    }

    /// Let go of the keyboard, its seat or its capability gone.
    pub(super) fn release(self) {
        if self.keyboard.version() >= 3 {
            self.keyboard.release();
        }
    }
}

impl Listener {
    /// Tell the window what seat `seat`'s pointer did, with the modifiers the
    /// seat's keyboard holds then, when it has one.
    pub(super) fn tell_pointed(&self, seat: u32, pointed: Pointed) {
        let modifiers = self
            .seats
            .iter()
            .find(|s| s.name == seat)
            .and_then(|s| s.keyboard.as_ref())
            .map(|k| k.modifiers);
        self.tell(Heard::Pointer {
            seat,
            pointed,
            modifiers,
        });
    }
}

impl Dispatch<WlKeyboard, u32> for Listener {
    fn event(
        state: &mut Self,
        _: &WlKeyboard,
        event: wl_keyboard::Event,
        &seat: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(keyboard) = state
            .seats
            .iter_mut()
            .find(|s| s.name == seat)
            .and_then(|s| s.keyboard.as_mut())
        else {
            return;
        };
        // The keymap's descriptor closes as the event drops; the real modifiers' indices need no keymap
        match event {
            wl_keyboard::Event::Modifiers {
                mods_depressed,
                mods_latched,
                mods_locked,
                ..
            } => keyboard.modifiers = mods_depressed | mods_latched | mods_locked,
            wl_keyboard::Event::Leave { .. } => keyboard.modifiers = 0,
            _ => {}
        }
    }
}

impl Dispatch<WlPointer, u32> for Listener {
    fn event(
        state: &mut Self,
        wl: &WlPointer,
        event: wl_pointer::Event,
        &seat: &u32,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        let Some(pointer) = state
            .seats
            .iter_mut()
            .find(|s| s.name == seat)
            .and_then(|s| s.pointer.as_mut())
        else {
            return;
        };
        let point = |x: f64, y: f64| (x as f32, y as f32);
        let pointed = match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                pointer.on = surface.id() == state.surface;
                pointer
                    .on
                    .then(|| Pointed::Entered(point(surface_x, surface_y)))
            }
            wl_pointer::Event::Leave { .. } => {
                std::mem::take(&mut pointer.on).then_some(Pointed::Left)
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => pointer
                .on
                .then(|| Pointed::Moved(point(surface_x, surface_y))),
            wl_pointer::Event::Button {
                serial,
                button,
                state: WEnum::Value(pressed),
                ..
            } => {
                let pressed = pressed == wl_pointer::ButtonState::Pressed;
                pointer.held.retain(|&b| b != button);
                // The implicit grab is the first press's: a press while another
                // button is held does not begin one
                if pressed {
                    if pointer.held.is_empty() {
                        pointer.press = Some(serial);
                    }
                    pointer.held.push(button);
                }
                // The window hears a press on its surface, and the release of each press it heard
                let told = if pressed {
                    pointer.on && {
                        pointer.told.push(button);
                        true
                    }
                } else {
                    let told = pointer.told.contains(&button);
                    pointer.told.retain(|&b| b != button);
                    told
                };
                // A button the seat sent while a drag was asked for and not confirmed means no drag began
                state.refused(seat);
                told.then_some(Pointed::Button {
                    serial,
                    button,
                    pressed,
                })
            }
            wl_pointer::Event::Axis { axis, value, .. } if pointer.on => {
                match axis {
                    WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                        pointer.wheel.distance.0 += value
                    }
                    WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                        pointer.wheel.distance.1 += value
                    }
                    _ => return,
                }
                pointer.wheel.turned = true;
                // A pointer older than frames ends each axis event's frame itself
                (wl.version() < 5).then(|| pointer.wheel.take()).flatten()
            }
            wl_pointer::Event::AxisDiscrete { axis, discrete } if pointer.on => {
                match axis {
                    WEnum::Value(wl_pointer::Axis::HorizontalScroll) => {
                        pointer.wheel.steps.0 += discrete
                    }
                    WEnum::Value(wl_pointer::Axis::VerticalScroll) => {
                        pointer.wheel.steps.1 += discrete
                    }
                    _ => {}
                }
                None
            }
            wl_pointer::Event::Frame => pointer.wheel.take(),
            _ => None,
        };
        if let Some(pointed) = pointed {
            state.tell_pointed(seat, pointed);
        }
    }
}

impl Wheel {
    /// The frame's wheel, by lines when it had discrete steps, as winit reads
    /// it; none when the wheel did not turn.
    fn take(&mut self) -> Option<Pointed> {
        let Wheel {
            distance,
            steps,
            turned,
        } = std::mem::take(self);
        turned.then(|| match steps {
            (0, 0) => Pointed::Wheel {
                lines: false,
                x: distance.0 as f32,
                y: distance.1 as f32,
            },
            (x, y) => Pointed::Wheel {
                lines: true,
                x: x as f32,
                y: y as f32,
            },
        })
    }
}

impl Listener {
    /// Retire seat `seat`'s sources that a button showed no drag began for.
    fn refused(&mut self, seat: u32) {
        let mut at = 0;
        while at < self.out.sources.len() {
            let source = &self.out.sources[at];
            if source.press.seat == seat && !source.ended {
                self.out.sources.remove(at).retire();
            } else {
                at += 1;
            }
        }
    }
}

impl Dispatch<WlDataSource, ()> for Listener {
    fn event(
        state: &mut Self,
        source: &WlDataSource,
        event: wl_data_source::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_data_source::Event::Send { mime_type, fd } => {
                let bytes = state
                    .out
                    .sources
                    .iter()
                    .find(|s| s.source == *source)
                    .and_then(|s| s.offers.iter().find(|(mime, _)| *mime == mime_type))
                    .map(|(_, bytes)| bytes.clone());
                let pipe = std::fs::File::from(fd);
                if let Some(bytes) = bytes.filter(|_| nonblocking(&pipe)) {
                    state.out.sends.push(Sending {
                        pipe,
                        bytes,
                        at: 0,
                        deadline: Instant::now() + DEADLINE,
                    });
                }
            }
            wl_data_source::Event::DndDropPerformed => {
                if let Some(at) = state.out.sources.iter().position(|s| s.source == *source) {
                    state.ended(at);
                }
            }
            wl_data_source::Event::DndFinished | wl_data_source::Event::Cancelled => {
                state.retire(source)
            }
            _ => {}
        }
    }
}

impl Dispatch<WlCompositor, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlCompositor,
        _: <WlCompositor as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlSurface, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlSurface,
        _: <WlSurface as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShm, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlShm,
        _: wl_shm::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlShmPool, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlShmPool,
        _: <WlShmPool as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WlBuffer, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WlBuffer,
        _: <WlBuffer as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewporter, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WpViewporter,
        _: <WpViewporter as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<WpViewport, ()> for Listener {
    fn event(
        _: &mut Self,
        _: &WpViewport,
        _: <WpViewport as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

#[cfg(test)]
#[path = "lift_tests.rs"]
mod tests;
