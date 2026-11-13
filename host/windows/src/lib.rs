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
pub mod clipboard;
#[cfg_attr(not(windows), allow(dead_code))]
mod desktop;
#[cfg(windows)]
mod edit_menu;
#[cfg(windows)]
mod icon;
#[cfg(windows)]
mod ime;
#[cfg_attr(not(windows), allow(dead_code))]
mod instance;
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
mod tooltip;
#[cfg(any(windows, test))]
mod touch;
#[cfg(windows)]
mod url;
#[cfg(any(windows, test))]
mod wheel;
#[cfg(windows)]
mod window;

/// The window an app opens in, in logical pixels, and what closing it does:
/// `host.windows.window` in its `app.json`, which the bake writes as
/// `WINDOW`. `EXACT_SIZE` overrides the size.
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
    /// Closing it hides it and the app keeps running (`keepRunning`, LLP
    /// 1133.025 D4); otherwise the app ends with it.
    pub keep_running: bool,
}

impl Default for Window {
    fn default() -> Self {
        Self {
            width: 1280.0,
            height: 720.0,
            min_width: 0.0,
            min_height: 0.0,
            keep_running: false,
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
/// packages' Windows implementations (`exact_painted::package`, LLP 1133.022
/// D10 item 5). The host owns an 8 MiB thread for the window, runner and
/// package calls; the caller waits until it exits.
pub fn run_composed<D: exact_runner::DataSource + Default + 'static>(
    name: &str,
    window: Window,
    plan: &[u8],
    compat: &str,
    composed: exact_painted::Composed,
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
    H: exact_painted::Hatches,
>(
    name: &str,
    window: Window,
    plan: &[u8],
    compat: &str,
    words: &'static [&'static str],
) -> i32 {
    let composed = exact_painted::Composed {
        hatches: Some(exact_painted::hatches::install::<H>(words)),
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
extern crate self as exact_windows;

#[cfg(test)]
mod clipboard_package {
    include!("../../../packages/clipboard/windows/lib.rs");

    mod tests {
        include!("../../../packages/clipboard/test/windows.rs");
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_bare_close_reaches_the_window_but_a_dialog_close_does_not() {
        super::on_desktop_stack(|| {
            use std::{cell::Cell, rc::Rc};
            let plan = contract::compile(
                r#"component App
  action leave
    close()
  action dismiss
    close("dialog")
  view
    column
      button "Close window" press=leave testId="window"
      button "Close dialog" press=dismiss testId="dialog"
"#,
            )
            .unwrap()
            .encode();
            let (mut p, error) = exact_painted::Presenter::boot_with(
                &plan,
                (),
                (400., 300.),
                1.,
                Default::default(),
                exact_painted::presenter::PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            let closed = Rc::new(Cell::new(0));
            let count = closed.clone();
            p.set_window_closer(move || count.set(count.get() + 1));
            for (target, expected) in [("dialog", 0), ("window", 1), ("window", 2)] {
                let key = p.host().kernel().find_by_test_id(target)[0];
                let id = p.host().kernel().node_by_key(key).unwrap().id;
                p.tap(id).unwrap();
                p.run_commands(|| ());
                assert_eq!(closed.get(), expected, "{target}");
            }
        });
    }

    /// A bare `focus()` reaches the window (LLP 1133.025 §8), which shows it
    /// if hidden, and `hideWindow()` too (D4); `focus(id)` stays an
    /// element's. With no window `focus()` is refused in the journal, as on
    /// the headless and DRM hosts.
    #[test]
    fn focus_and_hide_window_reach_the_window_but_an_elements_focus_does_not() {
        super::on_desktop_stack(|| {
            use std::{cell::Cell, rc::Rc};
            let plan = contract::compile(
                r#"component App
  action show
    focus()
  action field
    focus("name")
  action away
    hideWindow()
  view
    column
      button "Show" press=show testId="window"
      button "Field" press=field testId="field"
      button "Hide" press=away testId="hide"
      input id="name"
"#,
            )
            .unwrap()
            .encode();
            let boot = || {
                let (p, error) = exact_painted::Presenter::boot_with(
                    &plan,
                    (),
                    (400., 300.),
                    1.,
                    Default::default(),
                    exact_painted::presenter::PainterChoice::Cpu,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                p
            };
            let tap = |p: &mut exact_painted::Presenter<()>, target: &str| {
                let key = p.host().kernel().find_by_test_id(target)[0];
                let id = p.host().kernel().node_by_key(key).unwrap().id;
                p.tap(id).unwrap();
                p.run_commands(|| ());
            };
            let mut p = boot();
            let shown = Rc::new(Cell::new(0));
            let count = shown.clone();
            p.set_window_focuser(move || count.set(count.get() + 1));
            for (target, expected) in [("field", 0), ("window", 1)] {
                tap(&mut p, target);
                assert_eq!(shown.get(), expected, "{target}");
            }
            // `hideWindow()` reaches the window the same way.
            let hidden = Rc::new(Cell::new(0));
            let count = hidden.clone();
            p.set_window_hider(move || count.set(count.get() + 1));
            tap(&mut p, "hide");
            assert_eq!((shown.get(), hidden.get()), (1, 1));
            let mut p = boot();
            tap(&mut p, "window");
            let said = exact_runner::agent::logs(p.host().runner(), 0);
            assert!(said.contains("focus window: unsupported"), "{said}");
        });
    }

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
