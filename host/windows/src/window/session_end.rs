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
//! The end hides the window with no borrow held — `ShowWindow` re-enters
//! `WndProc` synchronously — then finishes teardown (`App::finish_end`) and,
//! for Restart Manager, asks the loop to exit.
#![allow(unsafe_code)]

use super::{App, DataSource};
use std::{cell::RefCell, rc::Weak};
use windows_sys::Win32::{
    Foundation::HWND,
    UI::{
        Shell::{DefSubclassProc, GetWindowSubclass, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{
            ENDSESSION_CLOSEAPP, ENDSESSION_CRITICAL, WM_ENDSESSION, WM_NCDESTROY,
            WM_QUERYENDSESSION,
        },
    },
};

/// Hears the session's end on the app's window, once per HWND.
pub(super) fn install<D: DataSource + Default + 'static>(window: HWND, app: Weak<RefCell<App<D>>>) {
    let mut existing = 0usize;
    // Skip re-installation on the same HWND so the prior Box is not leaked.
    if unsafe { GetWindowSubclass(window, Some(heard::<D>), 0, &mut existing) } != 0 {
        return;
    }
    let data = Box::into_raw(Box::new(app));
    // The HWND owns the box until WM_NCDESTROY; all access is on its thread.
    if unsafe { SetWindowSubclass(window, Some(heard::<D>), 0, data as usize) } == 0 {
        unsafe { drop(Box::from_raw(data)) };
        eprintln!("exact-windows: could not hear the session end");
    }
}

unsafe extern "system" fn heard<D: DataSource + Default + 'static>(
    window: HWND,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass: usize,
    data: usize,
) -> isize {
    let app = data as *mut Weak<RefCell<App<D>>>;
    match message {
        WM_QUERYENDSESSION => {
            // A critical end cannot wait, and an app busy in a nested loop is not asked.
            let keeps = lparam as u32 & ENDSESSION_CRITICAL == 0
                && unsafe { &*app }.upgrade().is_some_and(|app| {
                    app.try_borrow_mut()
                        .is_ok_and(|mut app| app.keeps_session())
                });
            return isize::from(!keeps);
        }
        WM_ENDSESSION if wparam != 0 => {
            if let Some(app) = unsafe { &*app }.upgrade() {
                let window = match app.try_borrow_mut() {
                    Ok(mut guard) => match guard.begin_end() {
                        Some(window) => window,
                        None => return 0,
                    },
                    Err(_) => return 0,
                };
                // Drop the borrow before hiding: ShowWindow dispatches WM_SHOWWINDOW /
                // WM_ACTIVATE synchronously into winit's WndProc.
                if let Some(window) = window {
                    window.set_visible(false);
                }
                if let Ok(mut guard) = app.try_borrow_mut() {
                    guard.finish_end();
                    if lparam as u32 & ENDSESSION_CLOSEAPP != 0 {
                        let _ = guard.proxy.send_event(crate::accessibility::Event::Close);
                    }
                }
            }
            return 0;
        }
        WM_NCDESTROY => unsafe {
            RemoveWindowSubclass(window, Some(heard::<D>), subclass);
            drop(Box::from_raw(app));
        },
        _ => {}
    }
    unsafe { DefSubclassProc(window, message, wparam, lparam) }
}
