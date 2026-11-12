//! Native Windows carrier over the shared native Contract/game presenter.
//! A native executable and lazily loaded GPU DLL; no browser or dev server.
#![deny(unsafe_code)]
#![deny(missing_docs)]

#[cfg(windows)]
mod accessibility;
#[cfg(windows)]
mod appearance;
#[cfg(windows)]
mod caret;
#[cfg(windows)]
mod clicks;
#[cfg(windows)]
mod clipboard;
#[cfg_attr(not(windows), allow(dead_code))]
mod desktop;
#[cfg(windows)]
mod edit_menu;
#[cfg(windows)]
mod icon;
#[cfg(windows)]
mod ime;
#[cfg_attr(not(windows), allow(dead_code))]
mod jump_list;
#[cfg(any(windows, test))]
mod keyboard;
#[cfg(windows)]
mod modal;
#[cfg(windows)]
mod orientation;
#[cfg_attr(not(windows), allow(dead_code))]
mod pad_poll;
#[cfg(windows)]
mod pads;
#[cfg(windows)]
mod placement;
#[cfg(windows)]
mod probe;
#[cfg(windows)]
mod timers;
#[cfg(windows)]
mod url;
#[cfg(any(windows, test))]
mod wheel;
#[cfg(windows)]
mod window;

/// The window an app opens in, in logical pixels: `host.windows.window` in
/// its `app.json`, which the bake writes as `WINDOW`. `EXACT_SIZE` overrides
/// the size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Window {
    /// The width it opens at.
    pub width: f32,
    /// The height it opens at.
    pub height: f32,
    /// The narrowest a person can make it; 0 sets no minimum.
    pub min_width: f32,
    /// The shortest a person can make it; 0 sets no minimum.
    pub min_height: f32,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            min_width: 0.0,
            min_height: 0.0,
        }
    }
}

/// The `windows` crate, for a package's Rust: the desktop builder includes
/// that source in the app's crate and writes no manifest of its own
/// (LLP 1133.022 D10 item 5), so the package names it through this host.
#[cfg(windows)]
pub use windows;

/// Run a Windows app with its display name, window, baked plan and product identity.
pub fn run<D: exact_runner::DataSource + Default + 'static>(
    name: &str,
    window: Window,
    plan: &[u8],
    compat: &str,
) -> i32 {
    run_composed::<D>(name, window, plan, compat, Default::default())
}

/// [`run`] with what the app's build composed: its hatches and its
/// packages' Windows implementations (`exact_linux::package`, LLP 1133.022
/// D10 item 5). The host owns an 8 MiB thread for the window, runner and
/// package calls; the caller waits until it exits.
pub fn run_composed<D: exact_runner::DataSource + Default + 'static>(
    name: &str,
    window: Window,
    plan: &[u8],
    compat: &str,
    composed: exact_linux::Composed,
) -> i32 {
    #[cfg(windows)]
    {
        on_desktop_stack(|| window::run::<D>(name, window, plan, compat, composed))
    }
    #[cfg(not(windows))]
    {
        let _ = (name, window, plan, compat, composed);
        refuse()
    }
}

/// Match the Apple host's runner thread (`ExactCore/Owner.swift`).
const DESKTOP_STACK: usize = 8 << 20;

/// Runs `body` on a thread with [`DESKTOP_STACK`] and answers what it
/// returns, resuming its panic. A Windows main thread defaults to a 1 MiB
/// reservation ([Thread stack size](https://learn.microsoft.com/windows/win32/procthread/thread-stack-size)),
/// which the app's linker controls. Keep the generic entry and the window
/// loop on this stack even when the caller compiles them without optimization.
#[cfg_attr(not(windows), allow(dead_code))]
fn on_desktop_stack<T: Send>(body: impl FnOnce() -> T + Send) -> T {
    std::thread::scope(|scope| {
        std::thread::Builder::new()
            .name("exact-windows".into())
            .stack_size(DESKTOP_STACK)
            .spawn_scoped(scope, body)
            .expect("cannot start the host's thread")
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    })
}

/// [`run`] with the app's hatches (LLP 1075.003.000.001 §5): `H` is the one
/// type the app's `modules/linux` names (the trait is the shared
/// presenter's), `words` the hatch words `app.json` gives this platform.
/// The window's loop tells the window moments and its frame loop the ticks;
/// overlays and observed input go through the presenter. Nothing of `H` is
/// made before the first frame is presented.
pub fn run_with_hatches<
    D: exact_runner::DataSource + Default + 'static,
    H: exact_linux::Hatches,
>(
    name: &str,
    window: Window,
    plan: &[u8],
    compat: &str,
    words: &'static [&'static str],
) -> i32 {
    let composed = exact_linux::Composed {
        hatches: Some(exact_linux::hatches::install::<H>(words)),
        ..Default::default()
    };
    run_composed::<D>(name, window, plan, compat, composed)
}

#[cfg(not(windows))]
fn refuse() -> i32 {
    eprintln!("exact-windows requires a Windows target");
    1
}

#[cfg(test)]
mod tests {
    /// Keep 6 MiB of frames live: beyond the default Windows main stack
    /// and Rust test stack. Reading each frame after recursion prevents
    /// an optimized test build from turning the recursion into a loop.
    #[test]
    fn the_host_stack_holds_unoptimized_startup_frames() {
        #[inline(never)]
        fn deep(n: u32) -> u32 {
            let mut frame = [0u8; 64 << 10];
            std::hint::black_box(&mut frame);
            let result = match n {
                0 => 0,
                _ => 1 + deep(n - 1),
            };
            std::hint::black_box(&frame);
            result
        }
        assert_eq!(super::on_desktop_stack(|| deep(96)), 96);
    }
}
