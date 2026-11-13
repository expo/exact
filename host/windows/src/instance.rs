//! One instance per app (LLP 1133.025 D6): a later launch hands its command
//! line to the running one and ends, as a Mac app is one process. Starting
//! the app from Start, its taskbar button, a shortcut or a jump-list task
//! then shows the window it already has, hidden or not, and a location on
//! that command line opens there.
//!
//! The running instance holds a mutex named for the app in the session's
//! namespace (`Local\`), and a message-only window of the same name. A later
//! launch that finds the mutex taken finds the window, lets its process take
//! the foreground ([AllowSetForegroundWindow](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-allowsetforegroundwindow):
//! the launch the person just made has the right to give), sends its
//! arguments by `WM_COPYDATA`, and ends. While the mutex is held, no launch
//! runs a second session on the app's storage: one the running app does not
//! answer in time ends with a message, and one that finds the mutex gone
//! (the first has just ended) becomes the instance.
//! ([Using Mutex Objects](https://learn.microsoft.com/windows/win32/sync/using-mutex-objects),
//! [WM_COPYDATA](https://learn.microsoft.com/windows/win32/dataxchg/wm-copydata)).

/// The name both objects take for the app: its id (`app.json`'s `id`, which
/// the build records in `compat.json`), else its display name. A backslash
/// is the namespace's separator, so none is kept.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn key(compat: &str, name: &str) -> String {
    let id = serde_json::from_str::<serde_json::Value>(compat)
        .ok()
        .and_then(|c| c["inputs"]["app"].as_str().map(str::to_owned))
        .filter(|id| !id.is_empty())
        .unwrap_or_else(|| name.to_owned());
    format!("Exact.{}", id.replace('\\', "_"))
}

/// A launch's arguments as one message: UTF-16, each ended by a NUL.
#[cfg_attr(not(windows), allow(dead_code))]
fn encode(args: &[String]) -> Vec<u16> {
    args.iter()
        .flat_map(|a| a.encode_utf16().chain(Some(0)))
        .collect()
}

/// The inverse of [`encode`].
#[cfg_attr(not(windows), allow(dead_code))]
fn decode(data: &[u16]) -> Vec<String> {
    let data = data.strip_suffix(&[0]).unwrap_or(data);
    if data.is_empty() {
        return Vec::new();
    }
    data.split(|c| *c == 0)
        .map(String::from_utf16_lossy)
        .collect()
}

/// Where a launch stands, tried once.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Attempt {
    /// No instance holds the mutex: this launch took it.
    Free,
    /// The running instance took this launch's arguments.
    Answered,
    /// An instance holds the mutex and has not answered.
    Taken,
}

/// What a launch is, once settled.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Settled {
    /// The app's one instance.
    First,
    /// The running instance has this launch's arguments.
    Handed,
    /// An instance holds the mutex and did not answer by the deadline: this
    /// launch ends rather than run a second session beside it.
    Busy,
}

/// Try until the launch is the instance, has been handed over, or `patience`
/// has passed with the mutex still held; `pause` waits between tries.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn settle(
    mut attempt: impl FnMut() -> Attempt,
    patience: std::time::Duration,
    mut pause: impl FnMut() -> std::time::Duration,
) -> Settled {
    let mut waited = std::time::Duration::ZERO;
    loop {
        match attempt() {
            Attempt::Free => return Settled::First,
            Attempt::Answered => return Settled::Handed,
            Attempt::Taken if waited >= patience => return Settled::Busy,
            Attempt::Taken => waited += pause(),
        }
    }
}

/// What the running app does with a later launch.
#[derive(Debug, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Handed {
    /// The login item's launch, while the app already runs: nothing new.
    Nothing,
    /// A plain launch: the window shown and brought forward, as the Dock's
    /// reopen does on the Mac.
    Show,
    /// A launch with a location (a jump-list task, a link): the window
    /// shown, and the location opened in it.
    Open(String),
}

/// What a later launch with `args` (its own path first) asks of the app.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn handed(args: &[String]) -> Handed {
    let args = args.get(1..).unwrap_or_default();
    if crate::desktop::login_launch(args).is_some() {
        return Handed::Nothing;
    }
    match exact_painted::app::launch_argument(args.iter().cloned()) {
        Some(href) => Handed::Open(href),
        None => Handed::Show,
    }
}

#[cfg(windows)]
pub(crate) use imp::{claim, Claim, Instance};

#[cfg(windows)]
#[allow(unsafe_code)]
mod imp {
    use std::{cell::RefCell, rc::Rc, time::Duration};
    use windows_sys::Win32::{
        Foundation::{CloseHandle, GetLastError, ERROR_ALREADY_EXISTS, HANDLE, HWND},
        System::{DataExchange::COPYDATASTRUCT, Threading::CreateMutexW},
        UI::{
            Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
            WindowsAndMessaging::{
                AllowSetForegroundWindow, CreateWindowExW, DestroyWindow, FindWindowExW,
                GetWindowThreadProcessId, SendMessageTimeoutW, HWND_MESSAGE, SMTO_ABORTIFHUNG,
                WM_COPYDATA, WM_NCDESTROY,
            },
        },
    };

    /// Marks a launch's arguments among `WM_COPYDATA`s: "EXAR".
    const ARGUMENTS: usize = 0x4558_4152;
    /// How long a later launch waits for the first to answer: while the
    /// first is still starting, its window may not exist yet.
    const PATIENCE: Duration = Duration::from_secs(5);

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(Some(0)).collect()
    }

    /// What this launch is.
    pub(crate) enum Claim {
        /// The app's one instance.
        First(Instance),
        /// The running instance has this launch's arguments: end.
        Handed,
        /// The running instance did not answer: end, saying so.
        Busy,
        /// The objects could not be made, so no instance can hold them:
        /// this launch runs on its own.
        Alone,
    }

    #[derive(Default)]
    struct State {
        handed: RefCell<Vec<Vec<String>>>,
        wake: RefCell<Option<Box<dyn Fn()>>>,
    }

    /// The running instance's mutex and window, held for the process's life.
    pub(crate) struct Instance {
        mutex: HANDLE,
        window: HWND,
        state: Rc<State>,
    }

    impl Instance {
        /// What wakes the event loop once a later launch is heard.
        pub(crate) fn set_wake(&self, wake: impl Fn() + 'static) {
            *self.state.wake.borrow_mut() = Some(Box::new(wake));
        }

        /// The later launches' arguments heard since the last call.
        pub(crate) fn take(&self) -> Vec<Vec<String>> {
            std::mem::take(&mut *self.state.handed.borrow_mut())
        }
    }

    impl Drop for Instance {
        fn drop(&mut self) {
            unsafe {
                DestroyWindow(self.window);
                CloseHandle(self.mutex);
            }
        }
    }

    /// Settle whether this launch, with `args`, is the app's instance under `key`.
    pub(crate) fn claim(key: &str, args: &[String]) -> Claim {
        let name = wide(key);
        let mutex_name = wide(&format!("Local\\{key}"));
        let message = super::encode(args);
        let mut mutex = std::ptr::null_mut();
        let mut failed = false;
        let settled = super::settle(
            || {
                let held = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
                if held.is_null() {
                    failed = true;
                    return super::Attempt::Free;
                }
                if unsafe { GetLastError() } != ERROR_ALREADY_EXISTS {
                    mutex = held;
                    return super::Attempt::Free;
                }
                unsafe { CloseHandle(held) };
                match hand_over(&name, &message) {
                    true => super::Attempt::Answered,
                    false => super::Attempt::Taken,
                }
            },
            PATIENCE,
            || {
                let pause = Duration::from_millis(50);
                std::thread::sleep(pause);
                pause
            },
        );
        match settled {
            _ if failed => {
                let error = std::io::Error::last_os_error();
                eprintln!("exact-windows: one instance: {error}");
                Claim::Alone
            }
            super::Settled::Handed => Claim::Handed,
            super::Settled::Busy => Claim::Busy,
            super::Settled::First => listen(mutex, &name),
        }
    }

    /// The first instance's message-only window, which later launches find.
    fn listen(mutex: HANDLE, name: &[u16]) -> Claim {
        let class = wide("Static");
        let window = unsafe {
            CreateWindowExW(
                0,
                class.as_ptr(),
                name.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
                std::ptr::null(),
            )
        };
        let state = Rc::new(State::default());
        let data = Rc::into_raw(state.clone());
        // The window holds this reference until WM_NCDESTROY; it is used only
        // on this thread, whose event loop dispatches the window's messages.
        if window.is_null()
            || unsafe { SetWindowSubclass(window, Some(receive), 0, data as usize) } == 0
        {
            let error = std::io::Error::last_os_error();
            unsafe {
                drop(Rc::from_raw(data));
                if !window.is_null() {
                    DestroyWindow(window);
                }
            }
            // The mutex stays held, never closed: a later launch waits for
            // this one, which runs without hearing it, and ends rather than
            // start a second session.
            let _ = mutex;
            eprintln!("exact-windows: one instance: {error}");
            return Claim::Alone;
        }
        Claim::First(Instance {
            mutex,
            window,
            state,
        })
    }

    /// Hand `message` to the running instance's window, if it is there and answers.
    fn hand_over(name: &[u16], message: &[u16]) -> bool {
        let class = wide("Static");
        let data = COPYDATASTRUCT {
            dwData: ARGUMENTS,
            cbData: (message.len() * 2) as u32,
            lpData: message.as_ptr() as *mut _,
        };
        let window = unsafe {
            FindWindowExW(
                HWND_MESSAGE,
                std::ptr::null_mut(),
                class.as_ptr(),
                name.as_ptr(),
            )
        };
        if window.is_null() {
            return false;
        }
        let mut process = 0;
        unsafe {
            GetWindowThreadProcessId(window, &mut process);
            AllowSetForegroundWindow(process);
        }
        let mut answer = 0;
        let sent = unsafe {
            SendMessageTimeoutW(
                window,
                WM_COPYDATA,
                0,
                &data as *const COPYDATASTRUCT as isize,
                SMTO_ABORTIFHUNG,
                PATIENCE.as_millis() as u32,
                &mut answer,
            )
        };
        sent != 0 && answer == 1
    }

    unsafe extern "system" fn receive(
        window: HWND,
        message: u32,
        wparam: usize,
        lparam: isize,
        subclass: usize,
        data: usize,
    ) -> isize {
        unsafe {
            let state = &*(data as *const State);
            match message {
                WM_COPYDATA => {
                    let copied = &*(lparam as *const COPYDATASTRUCT);
                    if copied.dwData != ARGUMENTS || copied.lpData.is_null() {
                        return 0;
                    }
                    let words = std::slice::from_raw_parts(
                        copied.lpData as *const u16,
                        copied.cbData as usize / 2,
                    );
                    state.handed.borrow_mut().push(super::decode(words));
                    if let Some(wake) = &*state.wake.borrow() {
                        wake();
                    }
                    return 1;
                }
                WM_NCDESTROY => {
                    RemoveWindowSubclass(window, Some(receive), subclass);
                    drop(Rc::from_raw(data as *const State));
                }
                _ => {}
            }
            DefSubclassProc(window, message, wparam, lparam)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A launch that finds the mutex held never becomes a second session:
    /// it is handed over, or ends once its patience runs out; one that finds
    /// it gone (the first has just ended) becomes the instance.
    #[test]
    fn a_launch_that_finds_the_app_running_never_runs_beside_it() {
        use std::time::Duration;
        let pause = || Duration::from_millis(50);
        let patience = Duration::from_secs(5);
        let mut tries = 0;
        let mut never = || {
            tries += 1;
            Attempt::Taken
        };
        assert_eq!(settle(&mut never, patience, pause), Settled::Busy);
        assert_eq!(tries, 101, "it tried for its whole patience");
        let mut answers = [Attempt::Taken, Attempt::Taken, Attempt::Answered].into_iter();
        assert_eq!(
            settle(|| answers.next().unwrap(), patience, pause),
            Settled::Handed
        );
        let mut ends = [Attempt::Taken, Attempt::Free].into_iter();
        assert_eq!(
            settle(|| ends.next().unwrap(), patience, pause),
            Settled::First
        );
        assert_eq!(settle(|| Attempt::Free, patience, pause), Settled::First);
    }

    #[test]
    fn the_key_is_the_apps_id_else_its_name() {
        let compat = r#"{"id":"0f","inputs":{"app":"com.exact.shelf"}}"#;
        assert_eq!(key(compat, "Shelf"), "Exact.com.exact.shelf");
        assert_eq!(key(r#"{"inputs":{"app":""}}"#, "Shelf"), "Exact.Shelf");
        assert_eq!(key("not json", r"A\B"), "Exact.A_B");
    }

    #[test]
    fn a_launch_round_trips_through_its_message() {
        for args in [
            vec![],
            vec![r"C:\Apps\Shelf.exe".to_owned()],
            vec![
                r"C:\Apps\Shelf.exe".to_owned(),
                "/add".into(),
                "café 🪟".into(),
            ],
            vec!["x".to_owned(), String::new(), "y".into()],
        ] {
            assert_eq!(decode(&encode(&args)), args);
        }
    }

    #[test]
    fn a_later_launch_shows_the_window_opens_its_location_or_does_nothing_at_login() {
        let args = |rest: &[&str]| {
            std::iter::once(r"C:\Apps\Shelf.exe")
                .chain(rest.iter().copied())
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };
        assert_eq!(handed(&args(&[])), Handed::Show);
        assert_eq!(handed(&args(&["/add"])), Handed::Open("/add".into()));
        assert_eq!(
            handed(&args(&["shelf://book/7"])),
            Handed::Open("shelf://book/7".into())
        );
        assert_eq!(handed(&args(&["--login"])), Handed::Nothing);
        assert_eq!(handed(&args(&["--login", "hidden"])), Handed::Nothing);
        assert_eq!(handed(&[]), Handed::Show);
    }
}
