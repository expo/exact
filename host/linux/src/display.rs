//! The display: DRM/KMS with two dumb buffers and a page flip per frame,
//! and the frame loop that owns the process when there is a screen.
//!
//! @ref LLP 1015 §6; LLP 1009 D5 as LLP 1015 §7 amends it (the GPU painter
//! compiles its shaders on the first launch on a machine and reads them
//! from the cache after; the display itself is dumb buffers and a readback
//! until the KMS surface lands)
//!
//! The card's first connected connector at its preferred mode; two
//! XRGB8888 dumb buffers; the first frame by `set_crtc`, every later one by
//! one pending page flip whose FD is polled alongside input and worker wakes.
//! Its completion releases the back buffer, pacing paint to the display refresh. Frames are painted only when something changed:
//! input, a timer, a motion frame, an image, a reload. Needs DRM master —
//! a VT, or a card nobody else holds — and the `video` group.

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
use exact_painted::app::Config;
use exact_painted::presenter::{Presenter, SubmittedFrame};
use exact_runner::DataSource;
use std::fs::{File, OpenOptions};
use std::os::fd::{AsFd, AsRawFd, BorrowedFd};
use std::os::unix::fs::OpenOptionsExt;
use std::time::Instant;
use tiny_skia::Pixmap;

mod clock_set;
mod flip;
mod pointer;
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

/// A KMS output with two dumb buffers.
pub struct Display {
    card: Card,
    crtc: crtc::Handle,
    connector: connector::Handle,
    mode: Mode,
    buffers: Vec<(DumbBuffer, framebuffer::Handle)>,
    flips: FlipState<SubmittedFrame>,
    width: u32,
    height: u32,
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
        let mut buffers = Vec::new();
        for _ in 0..2 {
            let db = card
                .create_dumb_buffer((width, height), DrmFourcc::Xrgb8888, 32)
                .map_err(|e| format!("dumb buffer: {e}"))?;
            let fb = card
                .add_framebuffer(&db, 24, 32)
                .map_err(|e| format!("framebuffer: {e}"))?;
            buffers.push((db, fb));
        }
        Ok(Display {
            card,
            crtc,
            connector: con.handle(),
            mode,
            buffers,
            flips: FlipState::default(),
            width,
            height,
        })
    }

    /// The mode's size, pixels.
    pub fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }
}

impl event_loop::Output for Display {
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
        self.flips.pending()
    }
    /// The vblank sequence of the last completed flip (LLP 1079 D3).
    fn sequence(&self) -> Option<u32> {
        self.flips.sequence
    }
    /// The kernel's timestamp of the last completed flip (CLOCK_MONOTONIC).
    fn last_flip(&self) -> Option<std::time::Duration> {
        self.flips.at
    }
    fn fd(&self) -> i32 {
        self.card.0.as_raw_fd()
    }

    // Copy/submit only when neither scanout nor an outstanding flip owns back.
    // The first set_crtc is synchronous; later calls never wait for a vblank.
    fn submit(&mut self, frame: SubmittedFrame) -> Result<Option<SubmittedFrame>, String> {
        let Self {
            card,
            buffers,
            flips,
            ..
        } = self;
        flips.submit(frame, |back, first, pixels| {
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
    }

    fn ready(&mut self) -> Result<Option<SubmittedFrame>, String> {
        self.flips
            .ready(self.crtc, || {
                self.card.receive_events().map(|events| events.collect())
            })
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
        for (s, d) in row.chunks_exact(4).zip(out.chunks_exact_mut(4)) {
            d.copy_from_slice(
                &((u32::from_le_bytes(s.try_into().unwrap()).swap_bytes() >> 8) | 0xff00_0000)
                    .to_le_bytes(),
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
    let viewport = (pw as f32 / config.scale, ph as f32 / config.scale);
    let wall = || started.elapsed().as_secs_f64() * 1000.0;
    config.clock_started = Some(started);
    // Startup timestamps for Observe's launch metrics.
    let mut marks = exact_painted::launch_marks::LaunchMarks::new();
    marks.launch_end();
    let (mut p, error) = match exact_painted::app::boot_presenter::<D>(config, viewport) {
        Ok(v) => v,
        Err(e) => {
            eprintln!("exact: boot: {e}");
            return 1;
        }
    };
    if let Some(e) = error {
        eprintln!("exact: {e}");
    }
    // The display's clock owns time (the timers RFC, D1)
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
        "exact: {pw}x{ph} @{}Hz on {}, scale {}, {} input device(s){}, boot {:.1} ms",
        display.refresh(),
        config.card,
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
        pointer,
        viewport,
        scale: config.scale,
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
    pointer: (f32, f32),
    viewport: (f32, f32),
    scale: f32,
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
    fn publish(&mut self, pixels: std::sync::Arc<Pixmap>) {
        if let Some(vnc) = &self.vnc {
            vnc.publish(pixels);
        }
    }
    fn turn(&mut self, p: &mut Presenter<D>, wall: impl Fn() -> f64) {
        let mut events = self.input.read(wall());
        if let Some(vnc) = &mut self.vnc {
            events.extend(vnc.take_events());
        }
        for event in events {
            if let Err(error) = pointer::dispatch(
                p,
                &mut self.pointer,
                self.viewport,
                self.scale,
                event,
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
