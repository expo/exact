//! The Linux display uses the connected panel at its preferred mode. Ink
//! presents directly when Vulkan supports the connector and presentation
//! feedback. Other painters and failed Vulkan surfaces use KMS dumb buffers.
//! One frame can be outstanding while the host services input and worker wakes.
//! @ref LLP 1015 §6; expo/exact#145.

#![allow(unsafe_code)]

use crate::event_loop::{self, Output as _};
use crate::input::Input;
use crate::vnc::Vnc;
use drm::buffer::{Buffer as _, DrmFourcc};
use drm::control::{
    connector, crtc, dumbbuffer::DumbBuffer, framebuffer, Device as ControlDevice, Mode,
    ModeTypeFlags, PageFlipFlags,
};
use drm::Device;
use event_loop::{Completion, PanelFrame};
use exact_painted::app::Config;
use exact_painted::presenter::{PainterChoice, Presenter};
use exact_runner::DataSource;
use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Instant;
use tiny_skia::Pixmap;

mod clock_set;
mod flip;
mod pointer;
mod scanout;
use flip::FlipState;
#[cfg(test)]
mod tests;

struct Card(File);

impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        self.0.as_fd()
    }
}
impl Device for Card {}
impl ControlDevice for Card {}

/// One DRM connector and the resources its selected output uses.
pub struct Display {
    card: Card,
    crtc: crtc::Handle,
    connector: connector::Handle,
    mode: Mode,
    buffers: Vec<(DumbBuffer, framebuffer::Handle)>,
    flips: FlipState<PanelFrame>,
    direct: Option<scanout::Direct>,
    width: u32,
    height: u32,
    /// The connector's name (`eDP-1`), the one display's `label` (LLP 1178 D6).
    label: String,
    /// Whether the connector is a built-in panel: `isInternal`.
    internal: bool,
    rotation: crate::rotation::Rotation,
}

impl Display {
    /// Open the card and take its first connected connector at its
    /// preferred mode.
    pub fn open(path: &str) -> Result<Display, String> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open(path)
            .map_err(|e| format!("{path}: {e}"))?;
        let card = Card(file);
        // Master follows the VT; when nobody holds it, ask.
        let _ = card.acquire_master_lock();
        let res = card
            .resource_handles()
            .map_err(|e| format!("resources: {e}"))?;
        let con = res
            .connectors()
            .iter()
            .filter_map(|c| card.get_connector(*c, true).ok())
            .find(|c| c.state() == connector::State::Connected && !c.modes().is_empty())
            .ok_or_else(|| "no connected connector with a mode".to_string())?;
        let mode = con
            .modes()
            .iter()
            .find(|m| m.mode_type().contains(ModeTypeFlags::PREFERRED))
            .or_else(|| con.modes().first())
            .copied()
            .ok_or_else(|| "no mode".to_string())?;
        let crtc = con
            .current_encoder()
            .and_then(|e| card.get_encoder(e).ok())
            .and_then(|e| e.crtc())
            .or_else(|| res.crtcs().first().copied())
            .ok_or_else(|| "no crtc".to_string())?;
        let (w, h) = mode.size();
        let (width, height) = (w as u32, h as u32);
        let internal = matches!(
            con.interface(),
            connector::Interface::EmbeddedDisplayPort
                | connector::Interface::LVDS
                | connector::Interface::DSI
        );
        let rotation = crate::rotation::requested()?
            .map_or_else(|| crate::rotation::panel(&card, con.handle()), Ok)?;
        let label = con.to_string();
        Ok(Display {
            card,
            crtc,
            connector: con.handle(),
            mode,
            buffers: Vec::new(),
            direct: None,
            flips: FlipState::default(),
            width,
            height,
            label,
            internal,
            rotation,
        })
    }

    fn open_dumb_buffers(&mut self) -> Result<(), String> {
        if self.buffers.is_empty() {
            // Releasing a Vulkan display can drop master on this open file description.
            let _ = self.card.acquire_master_lock();
        }
        while self.buffers.len() < 2 {
            let db = self
                .card
                .create_dumb_buffer((self.width, self.height), DrmFourcc::Xrgb8888, 32)
                .map_err(|e| format!("dumb buffer: {e}"))?;
            match self.card.add_framebuffer(&db, 24, 32) {
                Ok(fb) => self.buffers.push((db, fb)),
                Err(error) => {
                    let _ = self.card.destroy_dumb_buffer(db);
                    return Err(format!("framebuffer: {error}"));
                }
            }
        }
        Ok(())
    }

    fn finish_direct(&mut self, result: Completion) -> Completion {
        if matches!(result, Completion::Retry(_)) {
            self.direct = None;
        }
        result
    }

    /// The mode's size, pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl event_loop::Output for Display {
    fn direct(&self) -> bool {
        self.direct.is_some()
    }
    /// The mode's refresh rate, Hz.
    fn refresh(&self) -> u32 {
        self.mode.vrefresh()
    }

    /// The mode's frame period, milliseconds: its total pixels a frame over
    /// its pixel clock (kHz), which `vrefresh`'s whole hertz round.
    fn period_ms(&self) -> f64 {
        let (h, v) = (self.mode.hsync().2, self.mode.vsync().2);
        let clock = self.mode.clock();
        if clock == 0 || h == 0 || v == 0 {
            1000.0 / f64::from(self.refresh().max(1))
        } else {
            f64::from(h) * f64::from(v) / f64::from(clock)
        }
    }

    fn pending(&self) -> bool {
        self.direct
            .as_ref()
            .map_or_else(|| self.flips.pending(), |d| d.pending())
    }
    /// The vblank sequence of the last completed flip (LLP 1079 D3).
    fn observed(&self) -> Option<Instant> {
        self.direct.as_ref().and_then(|d| d.observed)
    }
    fn sequence(&self) -> Option<u32> {
        self.flips.sequence
    }
    /// The kernel's timestamp of the last completed flip (CLOCK_MONOTONIC).
    fn last_flip(&self) -> Option<std::time::Duration> {
        self.flips.at
    }
    fn fd(&self) -> i32 {
        self.direct
            .as_ref()
            .map_or_else(|| self.card.0.as_raw_fd(), |d| d.fd())
    }

    // Copy/submit only when neither scanout nor an outstanding flip owns back.
    // The first set_crtc is synchronous; later calls never wait for a vblank.
    fn submit(&mut self, frame: PanelFrame) -> Result<Completion, String> {
        if let Some(direct) = &mut self.direct {
            let result = direct.submit(frame);
            return Ok(self.finish_direct(result));
        }
        self.open_dumb_buffers()?;
        let Self {
            card,
            buffers,
            flips,
            ..
        } = self;
        flips
            .submit(frame, |back, first, frame| {
                let pixels = frame
                    .mirror()
                    .ok_or("KMS dumb buffer requires panel pixels")?;
                let (db, fb) = &mut buffers[back];
                let pitch = db.pitch() as usize;
                {
                    let mut map = card.map_dumb_buffer(db).map_err(|e| format!("map: {e}"))?;
                    copy_xrgb(pixels, map.as_mut(), pitch, self.width, self.height);
                }
                if first {
                    card.set_crtc(
                        self.crtc,
                        Some(*fb),
                        (0, 0),
                        &[self.connector],
                        Some(self.mode),
                    )
                    .map_err(|e| format!("set_crtc: {e}"))
                } else {
                    card.page_flip(self.crtc, *fb, PageFlipFlags::EVENT, None)
                        .map_err(|e| format!("page flip: {e}"))
                }
            })
            .map(|f| f.map_or(Completion::Pending, Completion::Shown))
    }

    fn ready(&mut self) -> Result<Completion, String> {
        if let Some(direct) = &mut self.direct {
            let result = direct.ready()?;
            return Ok(self.finish_direct(result));
        }

        self.flips
            .ready(self.crtc, || {
                self.card.receive_events().map(|events| events.collect())
            })
            .map(|f| f.map_or(Completion::Pending, Completion::Shown))
            .map_err(|e| format!("events: {e}"))
    }
}

impl Drop for Display {
    fn drop(&mut self) {
        for (db, fb) in self.buffers.drain(..) {
            let _ = self.card.destroy_framebuffer(fb);
            let _ = self.card.destroy_dumb_buffer(db);
        }
    }
}

/// Premultiplied RGBA rows into an XRGB8888 (little-endian: B, G, R, X)
/// buffer with its pitch; the page is opaque, so premultiplied is straight.
pub fn copy_xrgb(frame: &Pixmap, dst: &mut [u8], pitch: usize, width: u32, height: u32) {
    let w = frame.width().min(width) as usize;
    let h = frame.height().min(height) as usize;
    let src = frame.data();
    for y in 0..h {
        let row = &src[y * frame.width() as usize * 4..][..w * 4];
        let out = &mut dst[y * pitch..][..w * 4];
        for (s, d) in row
            .as_chunks::<4>()
            .0
            .iter()
            .zip(out.as_chunks_mut::<4>().0)
        {
            d.copy_from_slice(
                &((u32::from_le_bytes(*s).swap_bytes() >> 8) | 0xff00_0000).to_le_bytes(),
            );
        }
    }
}

/// The display loop: paint when something changed, present, wait for input
/// or the next timer or motion frame, repeat. Exit code.
pub fn run<D: DataSource + Default>(config: &mut Config, started: Instant) -> i32 {
    let mut display = match Display::open(&config.card) {
        Ok(d) => d,
        Err(e) => {
            eprintln!(
                "exact: no display: {e}\n  (a VT and the video group, or EXACT_AGENT=1 / EXACT_SHOT=<png> / EXACT_SMOKE=1 to run without one)"
            );
            return 1;
        }
    };
    let (pw, ph) = display.size();
    config.output_rotation = display.rotation;
    config
        .preferences
        .get_or_insert_with(Default::default)
        .orientation = Some(exact_runner::ScreenOrientation::rotated(
        ph >= pw,
        (display.rotation.degrees() / 90) as u8,
    ));
    let [lw, lh] = display.rotation.size([pw, ph]);
    let viewport = (lw as f32 / config.scale, lh as f32 / config.scale);
    exact_painted::desktop::install_displays(std::rc::Rc::new(
        exact_painted::desktop::FullScreen {
            label: display.label.clone(),
            internal: display.internal,
        },
    ));
    let wall = || started.elapsed().as_secs_f64() * 1000.0;
    config.clock_started = Some(started);
    // Startup timestamps for Observe's launch metrics.
    let mut marks = exact_painted::launch_marks::LaunchMarks::new();
    marks.launch_end();
    // The page is presented at its first picture's present, not its first commit (LLP 1069.000 D2).
    exact_runner::page::set_awaits_present(true);
    let mut ink = None;
    if config.painter() == PainterChoice::Ink {
        let refresh = (1_000_000.0 / display.period_ms() / f64::from(display.mode.vscan().max(1)))
            .round() as u32;
        let opened = display
            .card
            .0
            .try_clone()
            .map_err(|e| e.to_string())
            .and_then(|card| {
                // SAFETY: this is the selected connector and mode of our master
                // descriptor. Only Vulkan reads its events until Direct is dropped.
                unsafe {
                    exact_painted::ink::scanout::Scanout::open(
                        card.into(),
                        display.connector.into(),
                        [pw, ph],
                        refresh,
                    )
                }
                .map_err(|e| e.to_string())
            })
            .and_then(|(device, surface)| {
                scanout::Direct::new(surface)
                    .map(|direct| (device, direct))
                    .map_err(|e| e.to_string())
            });
        match opened {
            Ok((device, direct)) => {
                display.direct = Some(direct);
                ink = Some(device);
            }
            Err(e) => {
                eprintln!("exact: direct Vulkan display unavailable: {e}; using KMS dumb buffers")
            }
        }
    }
    let boot = match ink {
        Some(device) => exact_painted::app::boot_scanout_presenter::<D>(config, viewport, device),
        None => exact_painted::app::boot_presenter::<D>(config, viewport),
    };
    let (mut p, error) = match boot {
        Ok(v) => v,
        Err(e) => {
            eprintln!("exact: boot: {e}");
            return 1;
        }
    };
    if let Some(e) = error {
        eprintln!("exact: {e}");
    }
    // The display's clock owns time (the timers RFC, D1), and a commit's
    // motion starts at the flip that shows it (LLP 1003.001 D9).
    p.presents_pictures(true);
    p.set_live_clock(Some(exact_painted::presenter::wall_since(started)));
    let input = Input::open();
    let clock_set = clock_set::ClockSet::open();
    // @ref LLP 1132 D10 — a touch screen alone is `pointer: coarse`, `hover: none`.
    if input.touch_only() {
        let mut preferences = p.host().runner().viewport().preferences;
        preferences.pointer = exact_runner::Pointer::Coarse;
        preferences.hover = exact_runner::Hover::None;
        if let Some(e) = p.set_preferences(preferences) {
            eprintln!("exact: {e}");
        }
    }
    let vnc = match config.vnc.as_deref() {
        Some(addr) => match Vnc::start(addr, pw, ph) {
            Ok(v) => Some(v),
            Err(e) => {
                eprintln!("exact: vnc: {e}");
                None
            }
        },
        None => None,
    };
    let pointer = (viewport.0 / 2.0, viewport.1 / 2.0);
    p.set_pointer(Some(pointer));
    if !exact_runner::delivery::production(p.compat()) {
        trace_on_signal();
    }
    println!(
        "exact: {pw}x{ph} @{}Hz on {} ({}), scale {}, {} input device(s){}, boot {:.1} ms",
        display.refresh(),
        config.card,
        if display.direct() {
            "direct Vulkan"
        } else {
            "KMS dumb buffers"
        },
        config.scale,
        input.len(),
        match (&vnc, config.vnc.as_deref()) {
            (Some(_), Some("1")) => ", vnc on 127.0.0.1:5900".to_string(),
            (Some(_), Some(a)) => format!(", vnc on {a}"),
            _ => String::new(),
        },
        wall()
    );
    let mut events = Events {
        input,
        vnc,
        clock_set,
        pointer: pointer::Pointer::new(pointer),
        viewport,
        scale: config.scale,
        rotation: display.rotation,
    };
    event_loop::run(
        config,
        started,
        p,
        &mut display,
        &mut events,
        marks,
        D::default,
    )
}

static TRACE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_usr1(_: libc::c_int) {
    TRACE.store(true, std::sync::atomic::Ordering::Relaxed);
}

/// `SIGUSR1` asks for a trace (LLP 1079 D5): the loop takes the request
/// after its `poll`, which the signal interrupts (EINTR is a quiet turn).
fn trace_on_signal() {
    // SAFETY: the handler only stores to an atomic, which is async-signal-safe.
    unsafe { libc::signal(libc::SIGUSR1, on_usr1 as *const () as libc::sighandler_t) };
}

struct Events {
    input: Input,
    vnc: Option<Vnc>,
    clock_set: Option<clock_set::ClockSet>,
    pointer: pointer::Pointer,
    viewport: (f32, f32),
    scale: f32,
    rotation: crate::rotation::Rotation,
}

impl<D: DataSource> event_loop::Events<D> for Events {
    fn fds(&self) -> Vec<i32> {
        let mut fds = self.input.fds();
        if let Some(vnc) = &self.vnc {
            fds.push(vnc.fd());
        }
        if let Some(set) = &self.clock_set {
            fds.push(set.fd());
        }
        fds
    }
    fn idle(&self) -> bool {
        self.input.is_empty() && self.vnc.is_none()
    }
    fn mirror(&self) -> bool {
        self.vnc.as_ref().is_some_and(Vnc::has_viewers)
    }
    fn publish(&mut self, pixels: std::sync::Arc<Pixmap>) {
        if let Some(vnc) = self.vnc.as_ref().filter(|vnc| vnc.has_viewers()) {
            vnc.publish(pixels);
        }
    }
    fn turn(&mut self, p: &mut Presenter<D>, wall: impl Fn() -> f64) {
        let mut events = self.input.read(wall());
        if let Some(vnc) = &mut self.vnc {
            events.extend(vnc.take_events());
            if vnc.take_refresh() {
                p.repaint();
            }
        }
        for event in events {
            if let Err(error) = pointer::dispatch(
                p,
                &mut self.pointer,
                self.viewport,
                self.scale,
                crate::rotation::input(self.rotation, event),
                wall(),
            ) {
                eprintln!("exact: {error}");
            }
        }
    }
    fn date_jumped(&self) -> bool {
        self.clock_set.as_ref().is_some_and(|set| set.take())
    }
    fn trace_requested(&self) -> bool {
        TRACE.swap(false, std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
use event_loop::{poll, work_timeout};
