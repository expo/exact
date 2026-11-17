//! The display's frame period, which every loaded GPU module is given
//! (`gpu_period`: `Frame::period_ms`, and the slack of a canvas's frame-rate
//! cap; LLP 1009, gpu/README.md "Frame pacing and timing"), and on Android
//! each canvas's cap (`gpu_max_fps`), which the reader asks the platform for.
use super::*;
use std::cell::Cell;

thread_local! {
    /// The period last given, milliseconds; 0 while unknown.
    static PERIOD: Cell<f64> = const { Cell::new(0.0) };
    /// The caps the reader was last told, by canvas view.
    #[cfg(target_os = "android")]
    static CAPS: std::cell::RefCell<BTreeMap<u32, u32>> = const { std::cell::RefCell::new(BTreeMap::new()) };
}

/// Give `abi` the display's period, once it is known. The period is the
/// module's, not a canvas's: one call covers all of its canvases.
pub(super) fn give(abi: &Abi) {
    let ms = PERIOD.get();
    if ms <= 0.0 {
        return;
    }
    // SAFETY: `gpu_period(f64)` as gpu/src/native.rs declares it.
    if let Ok(period) = unsafe { abi.library.get::<unsafe extern "C" fn(f64)>(b"gpu_period") } {
        unsafe { period(ms) };
    }
}

impl Surfaces {
    /// The display's frame period changed to `ms`: every module loaded now
    /// and later is given it. The Linux display and the Android reader know
    /// the period; other hosts give none yet.
    #[cfg_attr(not(any(target_os = "linux", target_os = "android")), allow(dead_code))]
    pub(crate) fn period(&mut self, ms: f64) {
        if !(ms.is_finite() && ms > 0.0) || ms == PERIOD.get() {
            return;
        }
        PERIOD.set(ms);
        for abi in self.abis.values() {
            give(abi);
        }
    }

    /// The frame-rate caps (`Surface::max_fps`, 0 for none) of the canvases
    /// that have a view, as view and rate pairs, when any changed since the
    /// last call; a canvas that went is not told.
    #[cfg(target_os = "android")]
    pub(crate) fn frame_rates(&mut self) -> Option<Vec<i32>> {
        let mut changed = Vec::new();
        let mut caps = BTreeMap::new();
        for (view, c) in &self.canvases {
            let abi = &self.abis[&c.artifact];
            // SAFETY: `gpu_max_fps(u32) -> u32` as gpu/src/native.rs declares it.
            let cap = unsafe { abi.library.get::<Read>(b"gpu_max_fps") }
                .map_or(0, |max_fps| unsafe { max_fps(c.id) });
            let told = CAPS.with_borrow(|t| t.get(view).copied());
            if told.unwrap_or(0) != cap {
                changed.extend([*view as i32, cap as i32]);
            }
            caps.insert(*view, cap);
        }
        CAPS.set(caps);
        (!changed.is_empty()).then_some(changed)
    }
}
