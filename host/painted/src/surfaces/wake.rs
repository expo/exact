//! Asynchronous redraw wakes from resting GPU surfaces (`gpu_on_wake`, LLP 1009 D12).
use super::*;
use std::{
    io::{Read as _, Write as _},
    sync::{
        atomic::{AtomicBool, Ordering},
        OnceLock,
    },
};

static SURFACE_WAKE: OnceLock<Option<(crate::wake::Stream, crate::wake::Stream)>> = OnceLock::new();
static SURFACE_WOKEN: AtomicBool = AtomicBool::new(false);

fn channel() -> Option<&'static (crate::wake::Stream, crate::wake::Stream)> {
    SURFACE_WAKE
        .get_or_init(|| {
            let pair = crate::wake::Stream::pair().ok()?;
            pair.0.set_nonblocking(true).ok()?;
            pair.1.set_nonblocking(true).ok()?;
            Some(pair)
        })
        .as_ref()
}

/// Callback registered via `gpu_on_wake`: signals the host loop at most once
/// until [`drain_surface_wake`] runs.
pub(crate) extern "C" fn surface_woken() {
    if !SURFACE_WOKEN.swap(true, Ordering::AcqRel) {
        let Some(pair) = channel() else {
            SURFACE_WOKEN.store(false, Ordering::Release);
            return;
        };
        let mut writer = &pair.1;
        loop {
            match writer.write(&[1]) {
                Ok(_) => break,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => break,
                Err(_) => {
                    SURFACE_WOKEN.store(false, Ordering::Release);
                    break;
                }
            }
        }
    }
}

/// Unix descriptor that becomes readable when a resting GPU surface wakes, or `-1` if unavailable.
#[cfg(unix)]
pub(crate) fn surface_wake_fd() -> std::os::fd::RawFd {
    use std::os::fd::AsRawFd;
    channel().map_or(-1, |pair| pair.0.as_raw_fd())
}

/// Drain pending bytes from the surface wake channel and re-arm signaling.
/// Drains before clearing `SURFACE_WOKEN` so a concurrent wake can never leave
/// the flag stuck true on an empty descriptor.
pub(crate) fn drain_surface_wake() -> bool {
    let mut drained = false;
    if let Some(Some(pair)) = SURFACE_WAKE.get() {
        let mut reader = &pair.0;
        let mut buf = [0u8; 64];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(_) => drained = true,
                Err(e) if e.kind() == std::io::ErrorKind::Interrupted => continue,
                Err(_) => break,
            }
        }
    }
    SURFACE_WOKEN.swap(false, Ordering::SeqCst) || drained
}

impl<D: DataSource> Presenter<D> {
    /// Drain asynchronous GPU surface redraw wakes on the presenter thread.
    pub(crate) fn poll_surfaces(&mut self) -> bool {
        drain_surface_wake()
    }

    /// Descriptor that becomes readable when a resting GPU surface calls `SurfaceWaker::wake`.
    #[cfg(unix)]
    pub fn surface_wake_fd(&self) -> std::os::fd::RawFd {
        surface_wake_fd()
    }

    /// Whether any attached Android window canvas has pending work (including a surface wake).
    #[cfg(target_os = "android")]
    pub(crate) fn has_pending_window_canvas(&self) -> bool {
        self.surfaces.windows.iter().any(|(view, window)| {
            window.attached
                && self.surfaces.canvases.get(view).is_some_and(|c| {
                    self.surfaces
                        .abis
                        .get(&c.artifact)
                        .is_some_and(|abi| abi.frame_pending(c.id))
                })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn surface_wake_coalesces_and_rearms_after_drain() {
        let _ = drain_surface_wake();
        for _ in 0..1000 {
            surface_woken();
        }
        assert!(drain_surface_wake());
        assert!(!drain_surface_wake());
        surface_woken();
        assert!(drain_surface_wake());
    }
}
