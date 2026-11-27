//! Windows ending the session: a sign-out, a restart or a shutdown, or
//! Restart Manager closing the app for an installer (`ENDSESSION_CLOSEAPP`).
//! Windows asks each top-level window with `WM_QUERYENDSESSION`, then tells
//! it with `WM_ENDSESSION`, and may end the process once that returns
//! ([WM_ENDSESSION](https://learn.microsoft.com/windows/win32/shutdown/wm-endsession)).
//! Winit answers neither. The query asks `beforeunload` as the close button
//! does: a handler that keeps the window queues a focus turn and keeps the
//! session, and Windows names the app as one preventing the shutdown, as it
//! names a native app asking to save its changes
//! ([Application Shutdown Changes](https://learn.microsoft.com/windows/win32/shutdown/application-shutdown-changes-in-windows-vista)).
//! A critical end is not asked.
//!
//! The end keeps the window: Windows ends an app with no visible window that
//! takes longer than five seconds
//! ([Shutdown Changes for Windows Vista](https://learn.microsoft.com/windows/win32/shutdown/shutdown-changes-for-windows-vista)),
//! and the end can wait up to the exit bound. A block reason names what the
//! app waits for in the list Windows shows meanwhile
//! ([ShutdownBlockReasonCreate](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-shutdownblockreasoncreate)).
//! Then the loop ends after every end, Restart Manager's or the session's:
//! Restart Manager ends no process, and Windows may not have ended this one
//! yet.
//!
//! The subclass runs from winit's message pump outside its handler, where
//! winit hands the App at once an event the window's own messages raise. So
//! no borrow of the App is held across anything that shows, hides or focuses
//! the window: a kept window comes forward through the loop (`Event::Focus`).
#![allow(unsafe_code)]

use super::{App, DataSource};
use crate::accessibility::Event;
use std::{cell::RefCell, rc::Weak, time::Instant};
use windows_sys::Win32::{
    Foundation::HWND,
    System::Shutdown::{ShutdownBlockReasonCreate, ShutdownBlockReasonDestroy},
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            EndMenu, GetPropW, RemovePropW, SetPropW, ENDSESSION_CRITICAL, WM_ENDSESSION,
            WM_NCDESTROY, WM_QUERYENDSESSION,
        },
    },
};
use winit::event_loop::EventLoopProxy;

/// What the list of apps preventing the end says under the app's title.
const REASON: &str = "Saving its work";
/// The window property that marks the subclass installed. `GetWindowSubclass`
/// would tell, but the system's `comctl32.dll` 5.82, which a binary without
/// the common-controls 6 manifest loads (a test binary), exports it by
/// ordinal only, so such a binary fails to load.
const INSTALLED: windows_sys::core::PCWSTR = windows_sys::w!("exact.session_end");

struct Heard<D: DataSource> {
    app: Weak<RefCell<App<D>>>,
    /// The loop's, which a busy App does not lend.
    proxy: EventLoopProxy<Event>,
}

/// Hears the session's end on the app's window, once per HWND.
pub(super) fn install<D: DataSource + Default + 'static>(
    window: HWND,
    app: Weak<RefCell<App<D>>>,
    proxy: EventLoopProxy<Event>,
) {
    // Skip re-installation on the same HWND so the prior Box is not leaked.
    if !unsafe { GetPropW(window, INSTALLED) }.is_null() {
        return;
    }
    let data = Box::into_raw(Box::new(Heard { app, proxy }));
    // The HWND owns the box until WM_NCDESTROY; all access is on its thread.
    if unsafe { SetWindowSubclass(window, Some(heard::<D>), 0, data as usize) } == 0 {
        unsafe { drop(Box::from_raw(data)) };
        eprintln!("exact-windows: could not hear the session end");
        return;
    }
    unsafe { SetPropW(window, INSTALLED, 1 as _) };
}

unsafe extern "system" fn heard<D: DataSource + Default + 'static>(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass: usize,
    data: usize,
) -> isize {
    let state = data as *mut Heard<D>;
    match message {
        WM_QUERYENDSESSION => return isize::from(!keeps(unsafe { &*state }, lparam)),
        WM_ENDSESSION if wparam != 0 => {
            end(unsafe { &*state }, window);
            return 0;
        }
        WM_NCDESTROY => unsafe {
            RemoveWindowSubclass(window, Some(heard::<D>), subclass);
            RemovePropW(window, INSTALLED);
            drop(Box::from_raw(state));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}

/// Whether `beforeunload` keeps the session (`App::keeps_session`).
fn keeps<D: DataSource + Default + 'static>(heard: &Heard<D>, lparam: isize) -> bool {
    // A critical end cannot wait.
    if lparam as u32 & ENDSESSION_CRITICAL != 0 {
        return false;
    }
    let Some(app) = heard.app.upgrade() else {
        return false;
    };
    let Ok(mut app) = app.try_borrow_mut() else {
        // Busy in a nested loop (the editor's context menu holds the App
        // through TrackPopupMenuEx): not asked. The menu closes, so the end
        // that follows finds the App free.
        unsafe { EndMenu() };
        return false;
    };
    app.keeps_session()
}

/// The session ends: the app ends in order with its window kept, then the
/// loop ends.
fn end<D: DataSource + Default + 'static>(heard: &Heard<D>, window: HWND) {
    let reason: Vec<u16> = REASON.encode_utf16().chain([0]).collect();
    let blocked = unsafe { ShutdownBlockReasonCreate(window, reason.as_ptr()) } != 0;
    if let Some(app) = heard.app.upgrade() {
        // An App busy in a nested loop is not ended here: the loop's end still
        // drains its store writes and exports, if Windows has not ended the
        // process first.
        if let Ok(mut app) = app.try_borrow_mut() {
            app.end_session();
        }
    }
    if blocked {
        unsafe { ShutdownBlockReasonDestroy(window) };
    }
    let _ = heard.proxy.send_event(Event::Exit);
}

impl<D: DataSource + Default + 'static> App<D> {
    /// [`App::end`] without its hide, nothing here touching the window, and
    /// an accepted `saveFile` export, whose copy `must_finish` does not count,
    /// joined if it finishes within the same bound: its destination was
    /// emptied before the copy began.
    fn end_session(&mut self) {
        let deadline = Instant::now() + self.exit_bound();
        if self.begin_end().is_none() {
            return;
        }
        self.finish_end();
        let Some(p) = &mut self.presenter else {
            return;
        };
        if p.file_exports_done_by(deadline) {
            for failure in p.drain_file_exports() {
                eprintln!("exact: {failure}");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use exact_runner::{
        Answer, BackgroundState, DataError, DataSource, Dispatch, Outcome, Request, Store, Value,
        Work,
    };
    use std::process::{Child, Command, Stdio};
    use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
    use std::time::{Duration, Instant};
    use windows_sys::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        Graphics::Dwm::{DwmGetWindowAttribute, DWMWA_CLOAKED},
        System::Threading::{AttachThreadInput, GetCurrentThreadId},
        UI::WindowsAndMessaging::{
            EnumWindows, GetClassNameW, GetForegroundWindow, GetWindowThreadProcessId,
            IsWindowVisible, SendMessageTimeoutW, SetForegroundWindow, SwitchToThisWindow,
            ENDSESSION_CLOSEAPP, ENDSESSION_CRITICAL, SMTO_ABORTIFHUNG, WM_ENDSESSION,
            WM_QUERYENDSESSION,
        },
    };

    /// The child's app: `saving` or `keeping`.
    const CHILD: &str = "EXACT_SESSION_END_CHILD";
    /// A file the parent makes as the session ends, which the storage waits for.
    const GO: &str = "EXACT_SESSION_END_GO";
    static LEFT: AtomicU64 = AtomicU64::new(0);
    static ANSWERED: AtomicBool = AtomicBool::new(false);

    struct Background;
    impl exact_runner::HostWork for Background {}

    /// Answers `save()` with 1 and leaves two storage operations to the
    /// background, once however often it is asked. Each waits on the I/O
    /// worker until the session ends (`GO`), then takes half a second, so
    /// both are still waiting when it ends.
    #[derive(Default)]
    struct Slow {
        out: bool,
    }
    impl DataSource for Slow {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            if !ANSWERED.swap(true, Ordering::SeqCst) {
                LEFT.fetch_add(2, Ordering::SeqCst);
            }
            Ok(Answer::Now(Value::Number(1.0)))
        }
        fn dispatch(&mut self, _: u64, _: &Store) -> Dispatch {
            Dispatch::Run(Work::Now(Box::new(|| {
                let go = std::env::var_os(GO).map(std::path::PathBuf::from);
                let deadline = Instant::now() + Duration::from_secs(60);
                while go.as_ref().is_some_and(|go| !go.exists()) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(20));
                }
                std::thread::sleep(Duration::from_millis(500));
                Outcome::Storage(Vec::new())
            })))
        }
        fn background(&mut self, _: &Store) -> Option<Request> {
            if self.out || LEFT.load(Ordering::SeqCst) == 0 {
                return None;
            }
            self.out = true;
            Some(Request::continuation::<Background>(
                exact_runner::BACKGROUND,
            ))
        }
        fn background_landed(
            &mut self,
            _: &Store,
            _: Outcome,
        ) -> Result<Option<Request>, DataError> {
            self.out = false;
            LEFT.fetch_sub(1, Ordering::SeqCst);
            Ok(None)
        }
        fn background_state(&self) -> Option<BackgroundState> {
            let left = LEFT.load(Ordering::SeqCst);
            Some(BackgroundState {
                queued: left.saturating_sub(1),
                in_flight: left.min(1),
                ..Default::default()
            })
        }
    }

    /// The app the parent ends, through the shell's own entry
    /// (`run_baked`), so its window and loop are a shipped app's.
    #[test]
    #[ignore = "the child process of the_session_end_reaches_the_apps_own_focused_window"]
    fn child() {
        let Ok(mode) = std::env::var(CHILD) else {
            return;
        };
        let source = match mode.as_str() {
            "saving" => "component App\n  resource item = save() as shape number else save()\n  view\n    text item.toString() height=20\n",
            _ => "component App\n  action ask\n    preventDefault()\n  view\n    main beforeunload=ask\n      text \"Unsaved\" height=20\n",
        };
        let plan = contract::compile(source).unwrap().encode();
        let window = crate::Window {
            width: 320.,
            height: 200.,
            ..Default::default()
        };
        let compat = r#"{"inputs":{"store":{"L":"0"}},"embedded":{"trust":"development"}}"#;
        let code = crate::run_baked::<Slow, false>(
            "Session end check",
            window,
            &plan,
            compat,
            Default::default(),
            Default::default(),
        );
        assert_eq!(code, 0);
        let left = LEFT.load(Ordering::SeqCst);
        assert_eq!(left, 0, "the session's end dropped the storage");
        println!("session end child: landed");
    }

    /// Kills the child if the check fails first.
    struct Running(Child);
    impl Drop for Running {
        fn drop(&mut self) {
            let _ = self.0.kill();
        }
    }

    fn spawn(mode: &str, go: &std::path::Path) -> Running {
        let test = "window::session_end::tests::child";
        Running(
            Command::new(std::env::current_exe().unwrap())
                .args(["--exact", test, "--ignored", "--nocapture"])
                .env(CHILD, mode)
                .env(GO, go)
                .stdout(Stdio::piped())
                .spawn()
                .unwrap(),
        )
    }

    struct Search {
        pid: u32,
        found: HWND,
    }

    unsafe extern "system" fn each(window: HWND, search: LPARAM) -> i32 {
        let search = unsafe { &mut *(search as *mut Search) };
        let mut pid = 0;
        unsafe { GetWindowThreadProcessId(window, &mut pid) };
        let mut class = [0u16; 32];
        let n = unsafe { GetClassNameW(window, class.as_mut_ptr(), 32) } as usize;
        let mut cloaked = 0u32;
        let attribute = DWMWA_CLOAKED as u32;
        unsafe { DwmGetWindowAttribute(window, attribute, (&mut cloaked as *mut u32).cast(), 4) };
        if pid == search.pid
            && String::from_utf16_lossy(&class[..n]) == "Window Class"
            && unsafe { IsWindowVisible(window) } != 0
            && cloaked == 0
        {
            search.found = window;
            return 0;
        }
        1
    }

    /// The child's top-level window once it is shown and uncloaked.
    fn shown(child: &Running) -> HWND {
        let deadline = Instant::now() + Duration::from_secs(60);
        loop {
            let mut search = Search {
                pid: child.0.id(),
                found: std::ptr::null_mut(),
            };
            unsafe { EnumWindows(Some(each), &mut search as *mut Search as LPARAM) };
            if !search.found.is_null() {
                return search.found;
            }
            assert!(Instant::now() < deadline, "the child showed no window");
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// The window in front, with the keyboard focus, as a person leaves the
    /// app they work in when they sign out, where Windows lets this process
    /// bring it: its foreground lock refuses a process that has had no input.
    fn focus(window: HWND) -> bool {
        let deadline = Instant::now() + Duration::from_secs(3);
        while unsafe { GetForegroundWindow() } != window {
            if Instant::now() >= deadline {
                return false;
            }
            unsafe {
                let front = GetWindowThreadProcessId(GetForegroundWindow(), std::ptr::null_mut());
                let ours = GetCurrentThreadId();
                AttachThreadInput(ours, front, 1);
                SetForegroundWindow(window);
                AttachThreadInput(ours, front, 0);
                // As Alt+Tab switches.
                if GetForegroundWindow() != window {
                    SwitchToThisWindow(window, 1);
                }
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        std::thread::sleep(Duration::from_millis(300));
        true
    }

    /// What the window answers, through its thread's own message pump.
    fn send(window: HWND, message: u32, wparam: WPARAM, lparam: u32) -> Option<isize> {
        let mut answer = 0usize;
        let sent = unsafe {
            SendMessageTimeoutW(
                window,
                message,
                wparam,
                lparam as LPARAM,
                SMTO_ABORTIFHUNG,
                20_000,
                &mut answer,
            )
        };
        (sent != 0).then_some(answer as isize)
    }

    /// The child's exit within `bound`, and what it printed.
    fn ended(mut child: Running, bound: Duration) -> (std::process::ExitStatus, String) {
        let deadline = Instant::now() + bound;
        loop {
            if let Some(status) = child.0.try_wait().unwrap() {
                let mut out = String::new();
                if let Some(mut stdout) = child.0.stdout.take() {
                    std::io::Read::read_to_string(&mut stdout, &mut out).unwrap();
                }
                return (status, out);
            }
            assert!(
                Instant::now() < deadline,
                "the session's end left the child running"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
    }

    /// The messages reach the app's own window, focused, from winit's pump:
    /// outside its handler, where anything the window's messages raise reaches
    /// the App at once.
    #[test]
    #[ignore = "requires the interactive Windows desktop"]
    fn the_session_end_reaches_the_apps_own_focused_window() {
        // Restart Manager closing it for an installer: asked, then ended in
        // order with its storage landed, and the loop ends.
        let go = std::env::temp_dir().join(format!("exact-session-end-{}", std::process::id()));
        let _ = std::fs::remove_file(&go);
        let child = spawn("saving", &go);
        let window = shown(&child);
        println!("in front: {}", focus(window));
        let close = ENDSESSION_CLOSEAPP;
        assert_eq!(send(window, WM_QUERYENDSESSION, 0, close), Some(1));
        std::fs::write(&go, b"").unwrap();
        assert_eq!(send(window, WM_ENDSESSION, 1, close), Some(0));
        let _ = std::fs::remove_file(&go);
        let (status, out) = ended(child, Duration::from_secs(10));
        assert!(status.success(), "{status}\n{out}");
        assert!(out.contains("session end child: landed"), "{out}");
        // A handler that keeps the window keeps a sign-out, not a critical end.
        let child = spawn("keeping", &go);
        let window = shown(&child);
        println!("in front: {}", focus(window));
        assert_eq!(send(window, WM_QUERYENDSESSION, 0, 0), Some(0));
        let critical = ENDSESSION_CRITICAL;
        assert_eq!(send(window, WM_QUERYENDSESSION, 0, critical), Some(1));
        std::thread::sleep(Duration::from_millis(500));
        assert!(
            unsafe { IsWindowVisible(window) } != 0,
            "the kept window went"
        );
        // A sign-out's end, not Restart Manager's: the loop ends too.
        assert_eq!(send(window, WM_ENDSESSION, 1, 0), Some(0));
        let (status, out) = ended(child, Duration::from_secs(10));
        assert!(status.success(), "{status}\n{out}");
    }
}
