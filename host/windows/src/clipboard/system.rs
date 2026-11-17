//! Win32 ownership and memory transfer (LLP 1133.006 D5).
//! https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-setclipboarddata
#![allow(unsafe_code)]
use windows_sys::Win32::{
    Foundation::{GlobalFree, HGLOBAL, HWND},
    System::{DataExchange::*, Memory::*},
    UI::WindowsAndMessaging::{
        CreateWindowExW, DestroyWindow, DispatchMessageW, GetMessageW, GetWindowThreadProcessId,
        PostThreadMessageW, TranslateMessage, HWND_MESSAGE, MSG, WM_QUIT,
    },
};

const UNICODE_TEXT: u32 = 13;
pub(crate) struct Clipboard {
    hwnd: isize,
    owner: Option<std::thread::JoinHandle<()>>,
}
use super::substituting;
thread_local! {
    static WINDOW: std::cell::Cell<isize> = const { std::cell::Cell::new(0) };
}

impl Drop for Clipboard {
    fn drop(&mut self) {
        WINDOW.with(|w| {
            if w.get() == self.hwnd {
                w.set(0);
            }
        });
        super::PRIVATE.with(|b| *b.borrow_mut() = super::Item::default());
        if let Some(owner) = self.owner.take() {
            // SAFETY: the owner created its message queue before publishing hwnd.
            // It destroys the window itself after the loop receives WM_QUIT.
            unsafe {
                PostThreadMessageW(
                    GetWindowThreadProcessId(self.hwnd as HWND, std::ptr::null_mut()),
                    WM_QUIT,
                    0,
                    0,
                );
            }
            let _ = owner.join();
        }
    }
}
struct Open;
impl Drop for Open {
    fn drop(&mut self) {
        // SAFETY: constructed only after this thread opened the clipboard.
        unsafe {
            CloseClipboard();
        }
    }
}
fn error() -> String {
    format!("Windows clipboard: {}", std::io::Error::last_os_error())
}
impl Clipboard {
    pub(crate) fn new(hwnd: isize) -> Self {
        WINDOW.with(|w| w.set(hwnd));
        Self { hwnd, owner: None }
    }

    pub(crate) fn headless() -> Result<Self, String> {
        let (send, receive) = std::sync::mpsc::sync_channel(1);
        let owner = std::thread::Builder::new()
            .name("exact-clipboard".into())
            .spawn(move || {
                // An idle agent blocks on stdin. Its clipboard owner must still
                // answer WM_DESTROYCLIPBOARD when another application copies.
                // SAFETY: STATIC is a system class; this thread owns the
                // message-only window, pumps its messages and destroys it.
                unsafe {
                    let hwnd = CreateWindowExW(
                        0,
                        windows_sys::w!("STATIC"),
                        windows_sys::w!("Exact clipboard"),
                        0,
                        0,
                        0,
                        0,
                        0,
                        HWND_MESSAGE,
                        std::ptr::null_mut(),
                        std::ptr::null_mut(),
                        std::ptr::null(),
                    );
                    if hwnd.is_null() {
                        let _ = send.send(Err(error()));
                        return;
                    }
                    if send.send(Ok(hwnd as isize)).is_ok() {
                        let mut message = MSG::default();
                        while GetMessageW(&mut message, std::ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    DestroyWindow(hwnd);
                }
            })
            .map_err(|e| format!("Windows clipboard: {e}"))?;
        match receive.recv().map_err(|e| e.to_string())? {
            Ok(hwnd) => {
                WINDOW.with(|w| w.set(hwnd));
                Ok(Self {
                    hwnd,
                    owner: Some(owner),
                })
            }
            Err(error) => {
                let _ = owner.join();
                Err(error)
            }
        }
    }

    fn open(&self) -> Result<Open, String> {
        // SAFETY: the carrier keeps this window alive longer than its presenter.
        if unsafe { OpenClipboard(self.hwnd as HWND) } == 0 {
            return Err(error());
        }
        Ok(Open)
    }
}
impl exact_painted::presenter::clipboard::Clipboard for Clipboard {
    fn read_text(&mut self) -> Result<Option<String>, String> {
        if substituting() {
            return Ok(super::PRIVATE.with(|b| b.borrow().text.clone()));
        }
        let _open = self.open()?;
        // SAFETY: the clipboard remains open; GlobalSize bounds the UTF-16
        // slice. Unlock before closing, and never free the clipboard's memory.
        unsafe {
            if IsClipboardFormatAvailable(UNICODE_TEXT) == 0 {
                return Ok(None);
            }
            let memory = GetClipboardData(UNICODE_TEXT);
            if memory.is_null() {
                return Err(error());
            }
            let bytes = GlobalSize(memory);
            if bytes < 2 {
                return Ok(Some(String::new()));
            }
            let pointer = GlobalLock(memory).cast::<u16>();
            if pointer.is_null() {
                return Err(error());
            }
            let units = std::slice::from_raw_parts(pointer, bytes / 2);
            let end = units.iter().position(|u| *u == 0).unwrap_or(units.len());
            let text = String::from_utf16_lossy(&units[..end]);
            GlobalUnlock(memory);
            Ok(Some(text))
        }
    }
    fn write_text(&mut self, text: &str) -> Result<(), String> {
        super::write(
            super::Item {
                text: Some(text.into()),
                image: None,
            },
            substituting(),
        )
    }
}

struct Memory(HGLOBAL);
impl Memory {
    fn copy(bytes: &[u8]) -> Result<Self, String> {
        // SAFETY: copy within the movable allocation while it is locked.
        unsafe {
            let memory = Self(GlobalAlloc(GMEM_MOVEABLE, bytes.len()));
            if memory.0.is_null() {
                return Err(error());
            }
            let pointer = GlobalLock(memory.0).cast::<u8>();
            if pointer.is_null() {
                return Err(error());
            }
            std::ptr::copy_nonoverlapping(bytes.as_ptr(), pointer, bytes.len());
            GlobalUnlock(memory.0);
            Ok(memory)
        }
    }

    fn give(mut self, format: u32) -> Result<(), String> {
        // SAFETY: the clipboard is open and owned by our window. A successful
        // set transfers ownership; Drop frees only blocks Windows did not take.
        if unsafe { SetClipboardData(format, self.0) }.is_null() {
            return Err(error());
        }
        self.0 = std::ptr::null_mut();
        Ok(())
    }
}
impl Drop for Memory {
    fn drop(&mut self) {
        if !self.0.is_null() {
            // SAFETY: still ours, unlocked, never successfully handed over.
            unsafe {
                GlobalFree(self.0);
            }
        }
    }
}

pub(super) fn write(item: &super::Item) -> Result<(), String> {
    let hwnd = WINDOW.with(|w| w.get()) as HWND;
    // Only the thread holding the adapter has this handle. A headless
    // adapter keeps its owner window alive on a separate message thread.
    if hwnd.is_null() {
        return Err("clipboard writes require the host's clipboard adapter".into());
    }
    // Register and allocate everything before EmptyClipboard: a preparation
    // failure must leave the old clipboard alone. PNG precedes DIBV5, as in
    // Chromium, so consumers that choose the first recognized type keep alpha.
    let mut formats = Vec::new();
    if let Some(image) = &item.image {
        let name: Vec<u16> = "PNG".encode_utf16().chain(Some(0)).collect();
        // SAFETY: NUL-terminated format name, copied by Windows.
        let png = unsafe { RegisterClipboardFormatW(name.as_ptr()) };
        if png == 0 {
            return Err(error());
        }
        formats.push((png, Memory::copy(&image.png)?));
        if let Some(dib) = &image.dib {
            formats.push((17, Memory::copy(dib)?)); // CF_DIBV5
        }
    }
    if let Some(text) = &item.text {
        formats.push((UNICODE_TEXT, Memory::copy(&super::text_bytes(text))?));
    }
    // Do not construct an owning Clipboard adapter here: its Drop uninstalls it.
    // SAFETY: non-null HWND kept alive by this thread's clipboard adapter.
    if unsafe { OpenClipboard(hwnd) } == 0 {
        return Err(error());
    }
    let _open = Open;
    // SAFETY: this thread holds the clipboard open. Emptying makes hwnd its owner.
    if unsafe { EmptyClipboard() } == 0 {
        return Err(error());
    }
    for (format, memory) in formats {
        if let Err(e) = memory.give(format) {
            // No partial item is reported as success. Windows frees previously
            // accepted formats; the iterator drops blocks not yet handed over.
            // SAFETY: the clipboard remains open on this thread.
            unsafe {
                EmptyClipboard();
            }
            return Err(e);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        IsWindow, SendMessageTimeoutW, SMTO_ABORTIFHUNG, WM_NULL,
    };

    #[test]
    fn headless_owner_answers_messages_while_the_agent_thread_waits() {
        let clipboard = Clipboard::headless().unwrap();
        let hwnd = clipboard.hwnd;
        let answered = std::thread::spawn(move || {
            // SAFETY: the adapter keeps this window alive until the thread joins.
            unsafe {
                SendMessageTimeoutW(
                    hwnd as HWND,
                    WM_NULL,
                    0,
                    0,
                    SMTO_ABORTIFHUNG,
                    1000,
                    std::ptr::null_mut(),
                ) != 0
            }
        })
        .join()
        .unwrap();
        drop(clipboard);
        assert!(answered, "the idle agent's clipboard owner did not answer");
        // SAFETY: IsWindow accepts an expired handle; the owner has joined.
        assert_eq!(unsafe { IsWindow(hwnd as HWND) }, 0);
    }
}
