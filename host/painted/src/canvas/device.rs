//! The device, from the reader (LLP 1115.001 D3): its settings in one call,
//! its window and the system's raw insets in another, each before the host
//! boots and again, whole, on every change. What the reader said last is
//! kept for the process, so a host that boots after the calls is laid out
//! in that window with those settings before its first frame is recorded,
//! and nothing is ever painted without them.
//!
//! The reader pads nothing itself (D3.3). The presenter decides where the
//! page is in the window ([`crate::presenter::Presenter::fit_window`]), and
//! the reader asks for that, with the colour behind the page and the
//! scheme its bar icons follow ([`CanvasHost::chrome`]).

#[cfg(target_os = "android")]
use super::CanvasHost;
use crate::presenter::Chrome;
#[cfg(target_os = "android")]
use crate::presenter::{Device, Window};
#[cfg(target_os = "android")]
use exact_runner::DataSource;

/// A window as the reader tells it: its size in pixels, the pixels a point,
/// and four insets in pixels (top, right, bottom, left) each for the system
/// bars, the display cutout, the software keyboard and the system's gestures.
#[cfg(target_os = "android")]
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Told {
    /// Width and height, pixels.
    pub size: (u32, u32),
    /// Pixels a point: the display's density.
    pub scale: f32,
    /// Bars, cutout, keyboard, gestures; each top, right, bottom, left.
    pub insets: [i32; 16],
}

#[cfg(target_os = "android")]
impl Told {
    /// The window in points.
    fn window(&self) -> Window {
        let side = |at: usize| -> [f32; 4] {
            std::array::from_fn(|i| self.insets[at * 4 + i].max(0) as f32 / self.scale)
        };
        Window {
            size: (
                self.size.0 as f32 / self.scale,
                self.size.1 as f32 / self.scale,
            ),
            bars: side(0),
            cutout: side(1),
            keyboard: side(2),
            gestures: side(3),
        }
    }

    /// The viewport a page that does not ask for the window's edges is laid
    /// out in, points: what a host boots at, since most pages do not ask
    /// (one that does is laid out again once, when its root is known).
    pub(super) fn inside(&self) -> (f32, f32) {
        let w = self.window();
        let safe: [f32; 4] = std::array::from_fn(|i| w.bars[i].max(w.cutout[i]));
        (
            (w.size.0 - safe[1] - safe[3]).max(1.0),
            (w.size.1 - safe[0] - safe[2]).max(1.0),
        )
    }
}

/// What the reader last said, for a host that boots after it said so.
#[cfg(target_os = "android")]
static DEVICE: std::sync::Mutex<Option<Device>> = std::sync::Mutex::new(None);
#[cfg(target_os = "android")]
static WINDOW: std::sync::Mutex<Option<Told>> = std::sync::Mutex::new(None);

/// Keep `device` for the host that boots next (the reader's `configure`,
/// from any thread; a running host is told on its own, by
/// [`CanvasHost::configure`]).
#[cfg(target_os = "android")]
pub fn configure(device: &Device) {
    *DEVICE.lock().unwrap_or_else(|e| e.into_inner()) = Some(device.clone());
}

/// What the reader last said of the device, if it has said anything.
#[cfg(target_os = "android")]
pub(super) fn configured() -> Option<Device> {
    DEVICE.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// Keep the window for the host that boots next (the reader's `window`,
/// from any thread; a running host is told on its own, by
/// [`CanvasHost::set_window`]).
#[cfg(target_os = "android")]
pub fn window(told: Told) {
    *WINDOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(told);
}

/// The window the reader last told, when it is `size` pixels at `scale`: a
/// reader that boots or adopts a host names the window it then has.
#[cfg(target_os = "android")]
pub(super) fn windowed(size: (u32, u32), scale: f32) -> Option<Told> {
    (*WINDOW.lock().unwrap_or_else(|e| e.into_inner()))
        .filter(|told| told.size == size && told.scale == scale)
}

/// What a host keeps of its reader's window.
#[cfg(target_os = "android")]
#[derive(Default)]
pub(super) struct Shell {
    /// Whether the reader tells its window: the presenter then places the
    /// page in it, and the view's own size is never told.
    told: bool,
    /// The chrome the reader was last given.
    chrome: Option<[i32; 6]>,
}

#[cfg(target_os = "android")]
impl<D: DataSource + Default> CanvasHost<D> {
    /// The device's settings changed, or are told for the first time: the
    /// presenter takes them through its doors
    /// ([`crate::presenter::Presenter::set_device`]) and the next frame is
    /// a paint.
    #[cfg(target_os = "android")]
    pub fn configure(&mut self, device: &Device) {
        if let Some(e) = self.p.set_device(device) {
            eprintln!("exact: device: {e}");
        }
        self.force = true;
    }

    /// The window is `told`: its size, the display's density or an inset
    /// changed (a rotation, a resize, the keyboard, a bar shown or hidden).
    /// A new density is a new number of points for the same pixels, and
    /// every kept row is recorded again at it. The presenter lays the page
    /// out where the insets and its root put it.
    pub fn set_window(&mut self, told: Told) {
        if !(told.scale.is_finite() && told.scale > 0.0) || told.size.0 == 0 || told.size.1 == 0 {
            return;
        }
        self.shell.told = true;
        if told.scale != self.scale {
            self.scale = told.scale;
            crate::fields::rescale(told.scale);
            let (w, h) = self.viewport;
            if let Some(e) = self.p.resize_scaled(w, h, told.scale) {
                eprintln!("exact: scale: {e}");
            }
        }
        if let Some(e) = self.p.set_window(told.window()) {
            eprintln!("exact: window: {e}");
        }
        self.fitted();
    }

    /// Before a paint: a commit may have changed what the root says of the
    /// window (`viewport-fit`, `interactive-widget`).
    pub(super) fn fit(&mut self) {
        if !self.shell.told {
            return;
        }
        if let Some(e) = self.p.fit_window() {
            eprintln!("exact: window: {e}");
        }
        self.fitted();
    }

    /// The presenter placed the page: this host's viewport is its.
    fn fitted(&mut self) {
        let [_, _, w, h] = self.p.chrome().frame;
        if (w, h) != self.viewport {
            self.viewport = (w, h);
            self.force = true;
        }
    }

    /// The reader was made again: it has been given no chrome.
    pub(super) fn forget_chrome(&mut self) {
        self.shell.chrome = None;
    }

    /// What the reader draws around the page, when that is not what it was
    /// last given: where the page is in the window (x, y, width, height, in
    /// pixels), the colour behind it and in the edges it keeps out of
    /// (ARGB), and three bits: bit 0 is the dark scheme (light navigation
    /// icons); bit 1 requests light status bar icons; bit 2 hides the
    /// system bars (LLP 1105 D7). `None` while nothing changed.
    pub fn chrome(&mut self) -> Option<[i32; 6]> {
        let now = pack_chrome(self.p.chrome(), self.scale);
        if self.shell.chrome == Some(now) {
            return None;
        }
        self.shell.chrome = Some(now);
        Some(now)
    }
}

/// The JNI array keeps six words; the bars have independent appearance bits
/// and a bit of their own for hiding.
fn pack_chrome(c: Chrome, scale: f32) -> [i32; 6] {
    let px = |v: f32| (v * scale).round() as i32;
    let [r, g, b, a] = c.background;
    [
        px(c.frame[0]),
        px(c.frame[1]),
        px(c.frame[2]),
        px(c.frame[3]),
        i32::from_be_bytes([a, r, g, b]),
        i32::from(c.dark)
            | (i32::from(c.status_light_icons) << 1)
            | (i32::from(c.bars_hidden) << 2),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chrome_keeps_geometry_and_argb_and_packs_independent_bar_bits() {
        for (dark, status_light_icons, bars_hidden, flags) in [
            (false, false, false, 0),
            (true, false, false, 1),
            (false, true, false, 2),
            (true, true, false, 3),
            (false, false, true, 4),
            (true, true, true, 7),
        ] {
            let chrome = Chrome {
                frame: [1.25, 2.5, 100., 200.],
                background: [0x12, 0x34, 0x56, 0xff],
                dark,
                status_light_icons,
                bars_hidden,
            };
            assert_eq!(
                pack_chrome(chrome, 2.),
                [3, 5, 200, 400, 0xff123456u32 as i32, flags]
            );
        }
    }
}
