//! OLE drags from other apps over the window: Explorer's files in one drop
//! (LLP 1069.010), and any app's plain text and link (LLP 1094.001 D3).
#![allow(unsafe_code)]

use super::{App, DataSource};
use crate::accessibility::Event;
use exact_painted::Dropped;
use exact_runner::KeyModifiers;
use std::{
    cell::{Cell, RefCell},
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::PathBuf,
    rc::{Rc, Weak},
};
use windows::{
    core::{implement, Ref, Result},
    Win32::{
        Foundation::{E_POINTER, HWND, POINT, POINTL},
        Graphics::Gdi::ScreenToClient,
        System::{
            Com::{IDataObject, DVASPECT_CONTENT, FORMATETC, STGMEDIUM, TYMED_HGLOBAL},
            DataExchange::RegisterClipboardFormatW,
            Diagnostics::Debug::ReadProcessMemory,
            Memory::{GlobalLock, GlobalSize, GlobalUnlock},
            Ole::{
                IDropTarget, IDropTarget_Impl, OleInitialize, OleUninitialize, RegisterDragDrop,
                ReleaseStgMedium, RevokeDragDrop, CF_HDROP, CF_UNICODETEXT, DROPEFFECT,
                DROPEFFECT_COPY, DROPEFFECT_NONE, MK_ALT,
            },
            SystemServices::{MK_CONTROL, MK_SHIFT, MODIFIERKEYS_FLAGS},
            Threading::GetCurrentProcess,
        },
        UI::{
            Input::KeyboardAndMouse::{GetKeyState, VK_LWIN, VK_RWIN},
            Shell::{DragQueryFileW, HDROP},
        },
    },
};
use winit::{
    application::ApplicationHandler,
    event::{StartCause, WindowEvent},
    event_loop::{ActiveEventLoop, EventLoopProxy},
    raw_window_handle::{HasWindowHandle, RawWindowHandle},
    window::WindowId,
};

enum Drag {
    Over(POINTL, MODIFIERKEYS_FLAGS),
    Leave,
    Drop(POINTL, MODIFIERKEYS_FLAGS, Dropped),
}

#[implement(IDropTarget)]
struct Target {
    /// Whether the drag carries files, text or a link.
    carries: Cell<bool>,
    dispatch: Box<dyn Fn(Drag) -> bool>,
}

/// The shell's URL format (`CFSTR_INETURLW`), which a browser's link drag
/// offers: registered by name, as every app that reads it does.
fn url_format() -> u16 {
    unsafe { RegisterClipboardFormatW(windows::core::w!("UniformResourceLocatorW")) as u16 }
}

fn format(format: u16) -> FORMATETC {
    FORMATETC {
        cfFormat: format,
        dwAspect: DVASPECT_CONTENT.0,
        lindex: -1,
        tymed: TYMED_HGLOBAL.0 as u32,
        ..Default::default()
    }
}

/// IDataObject gives the caller one medium to release, including when its format is unexpected.
struct Medium(STGMEDIUM);
impl Drop for Medium {
    fn drop(&mut self) {
        unsafe { ReleaseStgMedium(&mut self.0) };
    }
}

fn paths(data: &IDataObject) -> Result<Vec<PathBuf>> {
    let medium = Medium(unsafe { data.GetData(&format(CF_HDROP.0))? });
    if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
        return Ok(Vec::new());
    }
    let hdrop = HDROP(unsafe { medium.0.u.hGlobal.0 });
    let count = unsafe { DragQueryFileW(hdrop, u32::MAX, None) };
    let mut paths = Vec::new();
    for i in 0..count {
        let size = unsafe { DragQueryFileW(hdrop, i, None) } as usize;
        let mut wide = vec![0u16; size + 1];
        let copied = unsafe { DragQueryFileW(hdrop, i, Some(&mut wide)) } as usize;
        if copied > 0 && copied == size {
            paths.push(PathBuf::from(OsString::from_wide(&wide[..copied])));
        }
    }
    Ok(paths)
}

/// The text a format's global memory holds as UTF-16, up to the NUL that
/// ends it; none when the drag does not offer the format, the text has no end
/// within the memory, or it cannot be held. The system copies the locked
/// bytes into a buffer of ours (`ReadProcessMemory`), a chunk at a time until
/// the NUL, and only the units it reports transferred are decoded.
fn wide(data: &IDataObject, kind: u16) -> Option<String> {
    let medium = Medium(unsafe { data.GetData(&format(kind)) }.ok()?);
    if medium.0.tymed != TYMED_HGLOBAL.0 as u32 {
        return None;
    }
    let global = unsafe { medium.0.u.hGlobal };
    let units = unsafe { GlobalSize(global) } / 2;
    let start = unsafe { GlobalLock(global) }.cast::<u16>();
    if start.is_null() {
        return None;
    }
    let (mut text, mut chunk, mut done) = (Vec::new(), [0u16; 1024], 0);
    let ended = loop {
        let want = (units - done).min(chunk.len());
        let mut read = 0;
        let copied = want > 0
            && unsafe {
                ReadProcessMemory(
                    GetCurrentProcess(),
                    start.add(done).cast::<std::ffi::c_void>(),
                    chunk.as_mut_ptr().cast::<std::ffi::c_void>(),
                    want * 2,
                    Some(&mut read),
                )
            }
            .is_ok();
        let got = &chunk[..if copied { read / 2 } else { 0 }];
        let end = got.iter().position(|&unit| unit == 0);
        let kept = &got[..end.unwrap_or(got.len())];
        if got.is_empty() || text.try_reserve(kept.len()).is_err() {
            break false;
        }
        text.extend_from_slice(kept);
        if end.is_some() {
            break true;
        }
        done += got.len();
    };
    let _ = unsafe { GlobalUnlock(global) };
    // Decoded into room reserved first, a UTF-16 unit being at most three UTF-8 bytes, so a long text that cannot be
    // held is none rather than an abort
    let mut out = String::new();
    if !ended || out.try_reserve(text.len().checked_mul(3)?).is_err() {
        return None;
    }
    out.extend(char::decode_utf16(text).map(|c| c.unwrap_or(char::REPLACEMENT_CHARACTER)));
    Some(out)
}

/// What the drag drops: Explorer's files, the plain text, and the link
/// unless it names a file (LLP 1094.001 D1).
fn dropped(data: &IDataObject) -> Dropped {
    let url = wide(data, url_format()).filter(|url| {
        !url.get(..5)
            .is_some_and(|scheme| scheme.eq_ignore_ascii_case("file:"))
    });
    Dropped {
        paths: paths(data).unwrap_or_default(),
        text: wide(data, CF_UNICODETEXT.0).unwrap_or_default(),
        url: url.unwrap_or_default(),
    }
}

#[allow(non_snake_case)]
impl IDropTarget_Impl for Target_Impl {
    fn DragEnter(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        self.carries.set(data.as_ref().is_some_and(|data| {
            [CF_HDROP.0, CF_UNICODETEXT.0, url_format()]
                .into_iter()
                .any(|kind| unsafe { data.QueryGetData(&format(kind)).is_ok() })
        }));
        self.DragOver(keys, pt, effect)
    }

    fn DragOver(
        &self,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let effect = unsafe { effect.as_mut() }.ok_or(E_POINTER)?;
        let accepts = self.carries.get() && (self.dispatch)(Drag::Over(*pt, keys));
        *effect = if accepts {
            *effect & DROPEFFECT_COPY
        } else {
            DROPEFFECT_NONE
        };
        Ok(())
    }

    fn DragLeave(&self) -> Result<()> {
        self.carries.set(false);
        (self.dispatch)(Drag::Leave);
        Ok(())
    }

    fn Drop(
        &self,
        data: Ref<IDataObject>,
        keys: MODIFIERKEYS_FLAGS,
        pt: &POINTL,
        effect: *mut DROPEFFECT,
    ) -> Result<()> {
        let effect = unsafe { effect.as_mut() }.ok_or(E_POINTER)?;
        let allowed = *effect & DROPEFFECT_COPY;
        *effect = DROPEFFECT_NONE;
        let offered = self.carries.replace(false);
        if offered && allowed != DROPEFFECT_NONE {
            if let Some(dropped) = data
                .as_ref()
                .map(dropped)
                .filter(|dropped| *dropped != Dropped::default())
            {
                if (self.dispatch)(Drag::Drop(*pt, keys, dropped)) {
                    *effect = allowed;
                    return Ok(());
                }
            }
        }
        (self.dispatch)(Drag::Leave);
        Ok(())
    }
}

struct Registration(HWND);
impl Drop for Registration {
    fn drop(&mut self) {
        unsafe {
            let _ = RevokeDragDrop(self.0);
            OleUninitialize();
        }
    }
}

/// Winit and OLE both call the same STA. During a nested modal callback the window is busy.
pub(super) struct Window<D: DataSource> {
    app: Rc<RefCell<App<D>>>,
    registration: Option<Registration>,
    /// A drag ended while the presenter was busy, so its leave waits for the next turn.
    owed: Rc<Cell<bool>>,
    /// Wakes the event loop for that turn.
    wake: EventLoopProxy<Event>,
}

impl<D: DataSource + Default + 'static> Window<D> {
    pub(super) fn new(app: Rc<RefCell<App<D>>>) -> Self {
        let wake = app.borrow().proxy.clone();
        Self {
            app,
            registration: None,
            owed: Rc::default(),
            wake,
        }
    }

    fn target(&self, hwnd: HWND) -> IDropTarget {
        let (app, owed, wake) = (
            Rc::downgrade(&self.app),
            self.owed.clone(),
            self.wake.clone(),
        );
        Target {
            carries: Cell::new(false),
            dispatch: Box::new(move |drag| dispatch(&app, hwnd, drag, &owed, &wake)),
        }
        .into()
    }
}

impl Registration {
    fn new(hwnd: HWND, target: &IDropTarget) -> Result<Self> {
        unsafe {
            OleInitialize(None)?;
            if let Err(e) = RegisterDragDrop(hwnd, target) {
                OleUninitialize();
                return Err(e);
            }
        }
        Ok(Registration(hwnd))
    }
}

fn dispatch<D: DataSource>(
    app: &Weak<RefCell<App<D>>>,
    hwnd: HWND,
    drag: Drag,
    owed: &Cell<bool>,
    wake: &EventLoopProxy<Event>,
) -> bool {
    let Some(app) = app.upgrade() else {
        return false;
    };
    let Ok(mut app) = app.try_borrow_mut() else {
        // A presenter busy in a nested modal loop takes no event: a drag that ends now owes its leave, paid at the
        // event loop's next turn, which this wakes
        if matches!(drag, Drag::Leave | Drag::Drop(..)) && !owed.replace(true) {
            let _ = wake.send_event(Event::Wake);
        }
        return false;
    };
    let scale = app.scale as f32;
    let Some(p) = app.presenter.as_mut() else {
        return false;
    };
    if owed.replace(false) {
        p.drag_left();
    }
    let result = match drag {
        Drag::Leave => {
            p.drag_left();
            false
        }
        Drag::Over(pt, keys) | Drag::Drop(pt, keys, _) => {
            let mut pt = POINT { x: pt.x, y: pt.y };
            if !unsafe { ScreenToClient(hwnd, &mut pt) }.as_bool() {
                return false;
            }
            let at = (pt.x as f32 / scale, pt.y as f32 / scale);
            let held = KeyModifiers {
                shift: keys.contains(MK_SHIFT),
                ctrl: keys.contains(MK_CONTROL),
                alt: keys.0 & MK_ALT != 0,
                meta: unsafe {
                    GetKeyState(VK_LWIN.0.into()) < 0 || GetKeyState(VK_RWIN.0.into()) < 0
                },
            };
            match drag {
                Drag::Drop(_, _, dropped) => p.dropped(at, held, &dropped).is_some(),
                _ => p.drag_over(at, held).is_some(),
            }
        }
    };
    if p.dirty() {
        if let Some(window) = &app.window {
            window.request_redraw();
        }
    }
    result
}

impl<D: DataSource + Default + 'static> ApplicationHandler<Event> for Window<D> {
    fn new_events(&mut self, events: &ActiveEventLoop, cause: StartCause) {
        let mut app = self.app.borrow_mut();
        app.new_events(events, cause);
        if self.owed.replace(false) {
            if let Some(p) = app.presenter.as_mut() {
                p.drag_left();
                if p.dirty() {
                    if let Some(window) = &app.window {
                        window.request_redraw();
                    }
                }
            }
        }
    }
    fn resumed(&mut self, events: &ActiveEventLoop) {
        self.app.borrow_mut().resumed(events);
        let app = self.app.borrow();
        let Some(window) = &app.window else { return };
        let Some(RawWindowHandle::Win32(raw)) = window.window_handle().ok().map(|h| h.as_raw())
        else {
            return;
        };
        let hwnd = HWND(raw.hwnd.get() as _);
        if self.registration.as_ref().is_some_and(|r| r.0 == hwnd) {
            return;
        }
        self.registration.take();
        match Registration::new(hwnd, &self.target(hwnd)) {
            Ok(r) => self.registration = Some(r),
            Err(e) => eprintln!("exact-windows: file drops unavailable: {e}"),
        }
    }
    fn user_event(&mut self, events: &ActiveEventLoop, event: Event) {
        self.app.borrow_mut().user_event(events, event);
    }
    fn window_event(&mut self, events: &ActiveEventLoop, id: WindowId, event: WindowEvent) {
        self.app.borrow_mut().window_event(events, id, event);
    }
    fn about_to_wait(&mut self, events: &ActiveEventLoop) {
        self.app.borrow_mut().about_to_wait(events);
    }
}

#[cfg(test)]
pub(super) mod tests {
    use super::*;
    use crate::window::tests::windowless_app;
    use windows::{
        core::HSTRING,
        Win32::{
            Graphics::Gdi::ClientToScreen,
            System::Com::CoTaskMemFree,
            UI::Shell::{
                BHID_DataObject, Common::ITEMIDLIST, SHCreateShellItemArrayFromIDLists,
                SHParseDisplayName,
            },
        },
    };
    use winit::{event_loop::EventLoopProxy, platform::windows::WindowAttributesExtWindows};

    struct Item(*mut ITEMIDLIST);
    impl Drop for Item {
        fn drop(&mut self) {
            unsafe { CoTaskMemFree(Some(self.0.cast())) };
        }
    }

    fn shell_selection() -> Result<IDataObject> {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .ancestors()
            .nth(2)
            .unwrap()
            .to_owned();
        let mut items = Vec::new();
        for path in [root.join("README.md"), root.join("AGENTS.md")] {
            let mut pidl = std::ptr::null_mut();
            unsafe {
                SHParseDisplayName(&HSTRING::from(path.as_os_str()), None, &mut pidl, 0, None)?
            };
            items.push(Item(pidl));
        }
        let ids: Vec<_> = items.iter().map(|i| i.0.cast_const()).collect();
        let array = unsafe { SHCreateShellItemArrayFromIDLists(&ids)? };
        unsafe { array.BindToHandler(None, &BHID_DataObject) }
    }

    pub(in crate::window) fn shell_selection_over_an_editor(
        events: &ActiveEventLoop,
        proxy: EventLoopProxy<Event>,
    ) -> Result<()> {
        let plan = contract::compile(
            r#"component App
  state calls = 0
  state names = ""
  action received(e: DragEvent)
    calls = calls + 1
    names = e.files.join("|")
  view
    column drop=received width=140 height=70
      textarea testId="editor" value="Original text" width=100 height=40
"#,
        )
        .unwrap()
        .encode();
        let mut app = windowless_app(proxy, &plan);
        let window = Rc::new(
            events
                .create_window(
                    winit::window::Window::default_attributes()
                        .with_title("Explorer drop regression")
                        .with_visible(false)
                        .with_drag_and_drop(false),
                )
                .unwrap(),
        );
        let RawWindowHandle::Win32(raw) = window.window_handle().unwrap().as_raw() else {
            unreachable!()
        };
        let hwnd = HWND(raw.hwnd.get() as _);
        app.scale = window.scale_factor();
        app.window = Some(window);
        let p = app.presenter.as_mut().unwrap();
        p.frame();
        let kernel = p.host().kernel();
        let editor = kernel
            .node_by_key(kernel.find_by_test_id("editor")[0])
            .unwrap()
            .id;
        let (x, y, w, h) = p.rect_of(editor).unwrap();
        let at = (x + w / 2., y + h / 2.);
        assert_eq!(p.hit(at.0, at.1), Some(editor));
        let mut pt = POINT {
            x: (at.0 * app.scale as f32).round() as i32,
            y: (at.1 * app.scale as f32).round() as i32,
        };
        assert!(unsafe { ClientToScreen(hwnd, &mut pt) }.as_bool());
        let point = POINTL { x: pt.x, y: pt.y };
        let app = Rc::new(RefCell::new(app));
        let carrier = Window::new(app.clone());
        let target = carrier.target(hwnd);
        let _registered = Registration::new(hwnd, &target)?;
        let data = shell_selection()?;
        let mut effect = DROPEFFECT_COPY;
        unsafe {
            target.DragEnter(&data, MK_CONTROL, point, &mut effect)?;
            assert_eq!(effect, DROPEFFECT_COPY);
            target.DragOver(MK_CONTROL, point, &mut effect)?;
            target.Drop(&data, MK_CONTROL, point, &mut effect)?;
        }
        assert_eq!(effect, DROPEFFECT_COPY);
        let mut app = app.borrow_mut();
        let p = app.presenter.as_mut().unwrap();
        let runner = p.host().runner();
        assert_eq!(runner.slot("calls").unwrap().as_number(), Some(1.));
        let docs: Vec<_> = runner
            .slot("names")
            .unwrap()
            .as_str()
            .unwrap()
            .split('|')
            .collect();
        assert_eq!(docs.len(), 2);
        for (doc, name) in docs.iter().zip(["/README.md", "/AGENTS.md"]) {
            assert!(doc.starts_with("doc:/") && doc.ends_with(name), "{doc}");
        }
        p.reader_access(1., true);
        let source = p.access_told().iter().find(|n| n.view == editor).unwrap();
        assert_eq!(p.access_editor(source).unwrap().0, "Original text");
        Ok(())
    }
}
