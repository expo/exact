//! A top-down DIB per monitor; every GDI object stays on its capture thread.
#![allow(unsafe_code)]

use exact_painted::screen_capture::Failure;
use windows_sys::Win32::{
    Graphics::Gdi::*,
    UI::HiDpi::{
        SetThreadDpiAwarenessContext, DPI_AWARENESS_CONTEXT,
        DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2,
    },
};

/// Enumeration and BitBlt both use physical pixels, including without HWND.
pub(crate) struct Dpi(DPI_AWARENESS_CONTEXT);
impl Dpi {
    pub(crate) fn new() -> Self {
        Self(unsafe { SetThreadDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) })
    }
}
impl Drop for Dpi {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe { SetThreadDpiAwarenessContext(self.0) };
        }
    }
}

struct Bitmap {
    screen: HDC,
    memory: HDC,
    bitmap: HBITMAP,
    previous: HGDIOBJ,
}
impl Drop for Bitmap {
    fn drop(&mut self) {
        unsafe {
            if !self.previous.is_null() {
                SelectObject(self.memory, self.previous);
            }
            if !self.bitmap.is_null() {
                DeleteObject(self.bitmap);
            }
            if !self.memory.is_null() {
                DeleteDC(self.memory);
            }
            if !self.screen.is_null() {
                ReleaseDC(std::ptr::null_mut(), self.screen);
            }
        }
    }
}

pub(super) fn capture(rect: [i32; 4], width: u32, height: u32) -> Result<Vec<u8>, Failure> {
    let _dpi = Dpi::new();
    let failure = |operation| {
        (
            "EncodingError",
            format!("{operation}: {}", std::io::Error::last_os_error()),
        )
    };
    let width = i32::try_from(width).map_err(|_| failure("display width"))?;
    let height = i32::try_from(height).map_err(|_| failure("display height"))?;
    unsafe {
        let mut image = Bitmap {
            screen: GetDC(std::ptr::null_mut()),
            memory: std::ptr::null_mut(),
            bitmap: std::ptr::null_mut(),
            previous: std::ptr::null_mut(),
        };
        if image.screen.is_null() {
            return Err(failure("GetDC"));
        }
        image.memory = CreateCompatibleDC(image.screen);
        if image.memory.is_null() {
            return Err(failure("CreateCompatibleDC"));
        }
        let mut info = BITMAPINFO::default();
        info.bmiHeader = BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: width,
            biHeight: -height,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB,
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        image.bitmap = CreateDIBSection(
            image.screen,
            &info,
            DIB_RGB_COLORS,
            &mut bits,
            std::ptr::null_mut(),
            0,
        );
        if image.bitmap.is_null() || bits.is_null() {
            return Err(failure("CreateDIBSection"));
        }
        image.previous = SelectObject(image.memory, image.bitmap);
        if image.previous.is_null() || image.previous as isize == -1 {
            image.previous = std::ptr::null_mut();
            return Err(failure("SelectObject"));
        }
        if BitBlt(
            image.memory,
            0,
            0,
            width,
            height,
            image.screen,
            rect[0],
            rect[1],
            SRCCOPY | CAPTUREBLT,
        ) == 0
        {
            return Err(failure("BitBlt"));
        }
        // CreateDIBSection requires a GDI flush before reading its memory:
        // https://learn.microsoft.com/windows/win32/api/wingdi/nf-wingdi-createdibsection
        if GdiFlush() == 0 {
            return Err(failure("GdiFlush"));
        }
        let count = width as usize * height as usize * 4;
        let mut pixels = std::slice::from_raw_parts(bits.cast::<u8>(), count).to_vec();
        super::rgba(&mut pixels);
        Ok(pixels)
    }
}
