//! When a picture reaches the screen (LLP 1003.001 D9): what it carried
//! starts there. A directly presenting swap chain says so in its frame
//! statistics once their `PresentCount` reaches the picture's
//! (`IDXGISwapChainMedia::GetFrameStatisticsMedia`, timed by `SyncQPCTime`).
//! Where it has none, and for the CPU window's GDI blit, the present is the
//! first vblank after the `Present` returned, on DWM's composition grid
//! (`DwmGetCompositionTimingInfo`). A refused `Present`
//! presented nothing, and its picture reports nothing; the deadline then
//! releases what it carried.
use super::*;
use exact_painted::presenter::Carried;

/// The picture presented and not yet heard on screen.
#[derive(Default)]
pub(super) struct Presents {
    waiting: Option<(Carried, u32)>,
}

impl Presents {
    /// A picture carrying `carried` was just handed to the display, `now_ms`
    /// being the host's clock. `count` is its `PresentCount`, where the swap
    /// chain keeps statistics; otherwise it shows at the next vblank.
    fn presented<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        carried: Carried,
        count: Option<u32>,
        now_ms: f64,
    ) {
        match count {
            Some(count) => self.waiting = Some((carried, count)),
            None => {
                self.waiting = None;
                report(p, carried, next_vblank_ms(now_ms));
            }
        }
    }

    /// The picture just painted was handed to the display: directly
    /// (`direct`, through the swap chain) or as the CPU window's blit.
    /// `started` is the host clock's origin.
    pub(super) fn drew<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        direct: bool,
        started: Instant,
    ) {
        let carried = p.picture_carried();
        let count = direct.then(|| p.present_statistics()).flatten();
        self.presented(p, carried, count.map(|s| s.last), ms(started));
    }

    /// The waiting picture's present, if the swap chain has shown it;
    /// statistics that have gone away fall back to the next vblank. Returns
    /// when the loop is to wake to poll again while it waits: about a
    /// quarter of a 60 Hz period from now.
    pub(super) fn poll<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        started: Instant,
    ) -> Option<Instant> {
        let (carried, count) = self.waiting?;
        let now_ms = ms(started);
        match p.present_statistics() {
            Some(stats) if !shown(stats.displayed, count) => {
                return Some(Instant::now() + Duration::from_millis(4));
            }
            Some(stats) => report(p, carried, now_ms - stats.ago_ms.max(0.)),
            None => report(p, carried, next_vblank_ms(now_ms)),
        }
        self.waiting = None;
        None
    }
}

/// The host's clock now, in ms since `started`.
fn ms(started: Instant) -> f64 {
    started.elapsed().as_secs_f64() * 1000.
}

/// The sooner of two wakes.
pub(super) fn sooner(a: Option<Instant>, b: Option<Instant>) -> Option<Instant> {
    a.into_iter().chain(b).min()
}

/// Whether the swap chain's displayed count has reached `count`, across the
/// counter's wrap.
fn shown(displayed: u32, count: u32) -> bool {
    displayed.wrapping_sub(count) < u32::MAX / 2
}

fn report<D: DataSource>(p: &mut Presenter<D>, carried: Carried, at_ms: f64) {
    if let Some(e) = p.picture_presented(carried, at_ms) {
        eprintln!("exact-windows: {e}");
    }
}

/// The first vblank from now on DWM's composition grid, on the host's clock
/// (`now_ms` now; `Instant` reads `QueryPerformanceCounter`, as DWM's times
/// are); `now_ms` itself where DWM gives no timing.
#[allow(unsafe_code)]
fn next_vblank_ms(now_ms: f64) -> f64 {
    use windows_sys::Win32::Graphics::Dwm::{DwmGetCompositionTimingInfo, DWM_TIMING_INFO};
    use windows_sys::Win32::System::Performance::{
        QueryPerformanceCounter, QueryPerformanceFrequency,
    };
    let mut info = DWM_TIMING_INFO {
        cbSize: std::mem::size_of::<DWM_TIMING_INFO>() as u32,
        ..unsafe { std::mem::zeroed() }
    };
    let (mut now, mut frequency) = (0i64, 0i64);
    // SAFETY: out pointers valid for each call; a null HWND asks for the
    // desktop's composition timing.
    let ok = unsafe {
        DwmGetCompositionTimingInfo(std::ptr::null_mut(), &mut info) >= 0
            && QueryPerformanceCounter(&mut now) != 0
            && QueryPerformanceFrequency(&mut frequency) != 0
    };
    if !ok || frequency <= 0 || info.qpcRefreshPeriod == 0 {
        return now_ms;
    }
    let qpc_ms = 1000. / frequency as f64;
    let vblank = now_ms - (now as f64 - info.qpcVBlank as f64) * qpc_ms;
    let period = info.qpcRefreshPeriod as f64 * qpc_ms;
    vblank + ((now_ms - vblank) / period).ceil().max(0.) * period
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_displayed_count_reaches_a_present_across_the_wrap() {
        assert!(super::shown(5, 5));
        assert!(super::shown(6, 5));
        assert!(!super::shown(4, 5));
        assert!(super::shown(1, u32::MAX));
    }
}
