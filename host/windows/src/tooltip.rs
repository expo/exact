//! The global `title` attribute, shown by Windows' tooltip control.
//! Windows owns its delay, placement and automatic dismissal:
//! https://learn.microsoft.com/windows/win32/controls/tooltip-controls
#![allow(unsafe_code)]
use exact_kernel::{NodeKey, PropId};
use exact_painted::Presenter;
use exact_runner::DataSource;

type Tip = (NodeKey, String);

fn title_at<D: DataSource>(p: &mut Presenter<D>, point: (f32, f32)) -> Option<Tip> {
    let dirty = p.dirty();
    let mut at = p.hit(point.0, point.1);
    // A geometry query does not present the CPU window's pending picture.
    if dirty {
        p.repaint();
    }
    while let Some(id) = at {
        let node = p.host().kernel().node(id)?;
        if let Some(title) = node.props.str(PropId::Title) {
            return (!title.is_empty()).then(|| (node.key, title.to_owned()));
        }
        at = node.parent;
    }
    None
}

use std::{mem::size_of, ptr};
use windows_sys::Win32::{
    Foundation::{HWND, POINT},
    Graphics::Gdi::ClientToScreen,
    UI::{Controls::*, WindowsAndMessaging::*},
};
use winit::{
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::Window,
};

#[derive(Default)]
pub(super) struct Tooltip {
    hwnd: HWND,
    owner: HWND,
    current: Option<Tip>,
    text: Vec<u16>,
}

impl Tooltip {
    fn create(&mut self, owner: HWND) -> bool {
        self.owner = owner;
        unsafe {
            InitCommonControlsEx(&INITCOMMONCONTROLSEX {
                dwSize: size_of::<INITCOMMONCONTROLSEX>() as u32,
                dwICC: ICC_BAR_CLASSES,
            });
            self.hwnd = CreateWindowExW(
                WS_EX_TOPMOST,
                TOOLTIPS_CLASSW,
                ptr::null(),
                WS_POPUP | TTS_NOPREFIX,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                CW_USEDEFAULT,
                self.owner,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );
        }
        !self.hwnd.is_null()
    }

    fn tool(&self) -> TTTOOLINFOW {
        let mut tool: TTTOOLINFOW = unsafe { std::mem::zeroed() };
        // TOOLINFO_V2_SIZE: Windows also uses this control without a v6 manifest.
        tool.cbSize = std::mem::offset_of!(TTTOOLINFOW, lpReserved) as u32;
        tool.hwnd = self.owner;
        tool.uId = 1;
        tool.lpszText = self.text.as_ptr().cast_mut();
        unsafe { GetClientRect(self.owner, &mut tool.rect) };
        tool
    }

    pub(super) fn clear(&mut self) {
        if self.current.take().is_some() {
            let tool = self.tool();
            unsafe {
                SendMessageW(self.hwnd, TTM_POP, 0, 0);
                SendMessageW(self.hwnd, TTM_DELTOOLW, 0, &tool as *const _ as isize);
            }
            self.text.clear();
        }
    }

    fn set_tip(&mut self, tip: Option<Tip>, owner: HWND) {
        if self.current != tip {
            if let Some((_, next)) = self
                .current
                .as_ref()
                .zip(tip.as_ref())
                .filter(|(current, next)| current.0 == next.0)
            {
                self.text = next.1.encode_utf16().chain(Some(0)).collect();
                let tool = self.tool();
                unsafe {
                    SendMessageW(self.hwnd, TTM_UPDATETIPTEXTW, 0, &tool as *const _ as isize);
                }
                self.current = tip;
                return;
            }
            self.clear();
            let Some(tip) = tip else { return };
            if self.hwnd.is_null() && !self.create(owner) {
                eprintln!("exact-windows: could not create a tooltip control");
                return;
            }
            self.text = tip.1.encode_utf16().chain(Some(0)).collect();
            let tool = self.tool();
            if unsafe { SendMessageW(self.hwnd, TTM_ADDTOOLW, 0, &tool as *const _ as isize) } == 0
            {
                eprintln!("exact-windows: could not register tooltip text");
                return;
            }
            self.current = Some(tip);
        }
    }

    pub(super) fn update<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        window: &Window,
        point: Option<(f32, f32)>,
        moved: bool,
    ) {
        // A move Windows made from a touch or a pen shows no tip, as touch
        // hosts show none; it reads the message being dispatched.
        let point = point.filter(|_| {
            !(moved && crate::touch::from_touch_or_pen(unsafe { GetMessageExtraInfo() }))
        });
        let tip = point.and_then(|point| title_at(p, point));
        let Ok(handle) = window.window_handle() else {
            return;
        };
        let RawWindowHandle::Win32(handle) = handle.as_raw() else {
            return;
        };
        self.set_tip(tip, handle.hwnd.get() as HWND);
        if self.current.is_none() {
            return;
        }
        let tool = self.tool();
        unsafe { SendMessageW(self.hwnd, TTM_NEWTOOLRECTW, 0, &tool as *const _ as isize) };
        // Only a real move starts Windows' hover timer. A commit may remove or
        // replace the title under a resting pointer, but must not fake input.
        if moved {
            let (x, y) = point.unwrap();
            let scale = window.scale_factor();
            let (x, y) = (
                (x as f64 * scale).round() as i32,
                (y as f64 * scale).round() as i32,
            );
            let mut message: MSG = unsafe { std::mem::zeroed() };
            message.hwnd = self.owner;
            message.message = WM_MOUSEMOVE;
            message.lParam = ((y as u16 as u32) << 16 | x as u16 as u32) as isize;
            message.pt = POINT { x, y };
            unsafe {
                ClientToScreen(self.owner, &mut message.pt);
                message.time = GetMessageTime() as u32;
                SendMessageW(self.hwnd, TTM_RELAYEVENT, 0, &message as *const _ as isize);
            }
        }
    }
}

impl Drop for Tooltip {
    fn drop(&mut self) {
        if !self.hwnd.is_null() {
            unsafe { DestroyWindow(self.hwnd) };
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_system_control_accepts_the_tool_without_an_app_manifest() {
        let owner = unsafe {
            CreateWindowExW(
                0,
                windows_sys::core::w!("STATIC"),
                ptr::null(),
                WS_POPUP,
                0,
                0,
                300,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            )
        };
        assert!(!owner.is_null());
        let mut tip = Tooltip::default();
        assert!(tip.create(owner));
        tip.text = "A title".encode_utf16().chain(Some(0)).collect();
        let tool = tip.tool();
        let added = unsafe { SendMessageW(tip.hwnd, TTM_ADDTOOLW, 0, &tool as *const _ as isize) };
        let count = unsafe { SendMessageW(tip.hwnd, TTM_GETTOOLCOUNT, 0, 0) };
        drop(tip);
        unsafe { DestroyWindow(owner) };
        assert_ne!(added, 0);
        assert_eq!(count, 1);
    }

    #[test]
    fn a_title_query_keeps_a_cpu_timer_update_pending() {
        crate::on_desktop_stack(|| {
            for title in ["", " title=\"Hint\""] {
                let plan = contract::compile(&format!(
                    r#"component App
  state label = "Before"
  action change
    label = "After"
  task change_later mount
    after(100, change)
  view
    text label width=200 height=40{title}
"#
                ))
                .unwrap()
                .encode();
                let (mut p, error) = Presenter::boot_with(
                    &plan,
                    (),
                    (300., 200.),
                    1.,
                    Default::default(),
                    exact_painted::presenter::PainterChoice::Cpu,
                )
                .unwrap();
                assert!(error.is_none(), "{error:?}");
                let before = p.frame();
                assert!(!p.dirty());
                assert!(p.advance(100.).is_none());
                assert!(p.dirty(), "the timer changed displayed text");
                let tip = title_at(&mut p, (10., 10.));
                assert_eq!(tip.is_some(), !title.is_empty());
                assert!(
                    p.dirty(),
                    "tooltip lookup must leave the new picture pending"
                );
                let after = p.frame();
                assert_ne!(before.data(), after.data());
                assert!(!p.dirty());
                title_at(&mut p, (10., 10.));
                assert!(
                    !p.dirty(),
                    "an unchanged tooltip must not request another frame"
                );
            }
        });
    }

    #[test]
    #[ignore = "requires the interactive Windows desktop"]
    fn changing_text_keeps_the_visible_tool_and_the_hover_delay() {
        unsafe {
            let class = windows_sys::core::w!("ExactTooltipTest");
            let definition = WNDCLASSW {
                lpfnWndProc: Some(DefWindowProcW),
                lpszClassName: class,
                ..std::mem::zeroed()
            };
            assert_ne!(RegisterClassW(&definition), 0);
            let owner = CreateWindowExW(
                0,
                class,
                ptr::null(),
                WS_POPUP | WS_VISIBLE,
                100,
                100,
                300,
                200,
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null_mut(),
                ptr::null(),
            );
            assert!(!owner.is_null());
            SetForegroundWindow(owner);
            let mut tip = Tooltip::default();
            let key = NodeKey {
                index: 1,
                generation: 0,
            };
            for during_delay in [false, true] {
                tip.set_tip(Some((key, "Before".into())), owner);
                SendMessageW(tip.hwnd, TTM_SETDELAYTIME, TTDT_INITIAL as usize, 100);
                SetCursorPos(120, 120);
                let mut message: MSG = std::mem::zeroed();
                message.hwnd = owner;
                message.message = WM_MOUSEMOVE;
                message.lParam = (20 << 16) | 20;
                message.pt = POINT { x: 120, y: 120 };
                message.time = windows_sys::Win32::System::SystemInformation::GetTickCount();
                SendMessageW(tip.hwnd, TTM_RELAYEVENT, 0, &message as *const _ as isize);
                let wait_visible = |hwnd| {
                    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(3);
                    while IsWindowVisible(hwnd) == 0 && std::time::Instant::now() < deadline {
                        let mut msg: MSG = std::mem::zeroed();
                        while PeekMessageW(&mut msg, ptr::null_mut(), 0, 0, PM_REMOVE) != 0 {
                            TranslateMessage(&msg);
                            DispatchMessageW(&msg);
                        }
                        std::thread::sleep(std::time::Duration::from_millis(10));
                    }
                    assert_ne!(
                        IsWindowVisible(hwnd),
                        0,
                        "tooltip must appear without another mouse move"
                    );
                };
                if !during_delay {
                    wait_visible(tip.hwnd);
                }
                tip.set_tip(Some((key, "After".into())), owner);
                if !during_delay {
                    assert_ne!(
                        IsWindowVisible(tip.hwnd),
                        0,
                        "updating text must not dismiss the tooltip"
                    );
                }
                wait_visible(tip.hwnd);
                let mut text = [0u16; 128];
                let mut tool = tip.tool();
                tool.lpszText = text.as_mut_ptr();
                SendMessageW(
                    tip.hwnd,
                    TTM_GETTEXTW,
                    text.len(),
                    &mut tool as *mut _ as isize,
                );
                let end = text.iter().position(|&c| c == 0).unwrap();
                assert_eq!(String::from_utf16_lossy(&text[..end]), "After");
                tip.clear();
                SetCursorPos(450, 350);
            }
            drop(tip);
            DestroyWindow(owner);
            UnregisterClassW(class, ptr::null_mut());
        }
    }

    #[test]
    fn the_hovered_title_follows_ancestors_and_an_empty_title_stops_it() {
        crate::on_desktop_stack(|| {
            let plan = contract::compile(
                r#"component App
  state hint = "Own title"
  action rename
    hint = "Changed title"
  view
    column title="Parent title" width=400 height=300
      text "Inherited" testId="inherited" width=200 height=40
      button "Empty" title="" testId="empty" width=200 height=40
      button "Own" title=hint press=rename testId="own" width=200 height=40
"#,
            )
            .unwrap()
            .encode();
            let (mut p, error) = Presenter::boot_with(
                &plan,
                (),
                (400., 300.),
                1.,
                Default::default(),
                exact_painted::presenter::PainterChoice::Cpu,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            for (name, expected) in [
                ("inherited", Some("Parent title")),
                ("empty", None),
                ("own", Some("Own title")),
            ] {
                let key = p.host().kernel().find_by_test_id(name)[0];
                let id = p.host().kernel().node_by_key(key).unwrap().id;
                let (x, y, w, h) = p.rect_of(id).unwrap();
                assert_eq!(
                    title_at(&mut p, (x + w / 2., y + h / 2.)).map(|(_, text)| text),
                    expected.map(str::to_owned),
                    "{name}"
                );
            }
            let key = p.host().kernel().find_by_test_id("own")[0];
            let id = p.host().kernel().node_by_key(key).unwrap().id;
            p.tap(id).unwrap();
            p.frame();
            let (x, y, w, h) = p.rect_of(id).unwrap();
            assert_eq!(
                title_at(&mut p, (x + w / 2., y + h / 2.)).unwrap().1,
                "Changed title"
            );
            assert!(title_at(&mut p, (500., 500.)).is_none());
        });
    }
}
