//! A posted window message shows the dialog after Winit releases its handler.
//! Its modal loop can then dispatch Winit's timer and executor callbacks.
#![allow(unsafe_code)]
use crate::{accessibility::Event, modal::Schedule};
use exact_painted::presenter::file_dialog::{FileDialog, Outcome, StartFolder};
use exact_runner::file_pickers::Kind;
use std::{cell::Cell, path::PathBuf, rc::Rc};
use windows::{
    core::{Interface, HSTRING, PCWSTR, PWSTR},
    Win32::{
        Foundation::{ERROR_CANCELLED, E_INVALIDARG, HWND},
        System::Com::{
            CoCreateInstance, CoInitializeEx, CoTaskMemFree, CoUninitialize, CLSCTX_INPROC_SERVER,
            COINIT_APARTMENTTHREADED,
        },
        UI::Shell::{
            Common::COMDLG_FILTERSPEC, FileOpenDialog, FileSaveDialog, IFileDialog,
            IFileOpenDialog, IShellItem, SHCreateItemFromParsingName, SHGetKnownFolderItem,
            FILEOPENDIALOGOPTIONS, KF_FLAG_DEFAULT, SIGDN_FILESYSPATH,
        },
    },
};
use windows_sys::Win32::{
    Foundation::HWND as RawWindow,
    UI::{
        Shell::{DefSubclassProc, RemoveWindowSubclass, SetWindowSubclass},
        WindowsAndMessaging::{PostMessageW, RegisterWindowMessageW, WM_NCDESTROY},
    },
};
use winit::event_loop::EventLoopProxy;

#[derive(Default)]
pub(crate) struct Dialogs(Option<Rc<Window>>);

struct Window {
    window: Cell<RawWindow>,
    message: u32,
    dialogs: super::State,
    modal: Schedule,
    proxy: EventLoopProxy<Event>,
}

use super::Answer;

impl Dialogs {
    pub fn install(
        &mut self,
        window: isize,
        modal: Schedule,
        proxy: EventLoopProxy<Event>,
    ) -> bool {
        let name: Vec<u16> = "Exact.FileDialog\0".encode_utf16().collect();
        let message = unsafe { RegisterWindowMessageW(name.as_ptr()) };
        if message == 0 {
            return false;
        }
        let state = Rc::new(Window {
            window: Cell::new(window as RawWindow),
            message,
            dialogs: Default::default(),
            modal,
            proxy,
        });
        let data = Rc::into_raw(state.clone());
        if unsafe {
            SetWindowSubclass(
                window as RawWindow,
                Some(observe),
                message as usize,
                data as usize,
            )
        } == 0
        {
            unsafe {
                drop(Rc::from_raw(data));
            }
            return false;
        }
        self.0 = Some(state);
        true
    }

    pub fn available(&self) -> bool {
        self.0.as_ref().is_some_and(|s| !s.window.get().is_null())
    }

    pub fn queue(&self, id: u32, dialog: FileDialog, live: std::sync::Weak<()>) {
        if let Some(state) = &self.0 {
            state
                .dialogs
                .queued
                .borrow_mut()
                .push_back((id, dialog, live));
            state.post();
        }
    }

    pub fn take_answers(&self) -> Vec<Answer> {
        self.0.as_ref().map_or_else(Vec::new, |s| {
            std::mem::take(&mut *s.dialogs.answered.borrow_mut())
        })
    }
}

impl Window {
    fn wake(&self) {
        let _ = self.proxy.send_event(Event::Wake);
    }

    fn post(&self) {
        if self.dialogs.open.get()
            || self.dialogs.posted.get()
            || self.dialogs.queued.borrow().is_empty()
        {
            return;
        }
        if !self.window.get().is_null()
            && unsafe { PostMessageW(self.window.get(), self.message, 0, 0) } != 0
        {
            self.dialogs.posted.set(true);
        } else {
            for (id, _, _) in self.dialogs.queued.borrow_mut().drain(..) {
                self.dialogs.answered.borrow_mut().push(Answer {
                    id,
                    outcome: Outcome::Refused("no window for the file dialog".into()),
                    lines: Vec::new(),
                });
            }
            self.wake();
        }
    }

    fn show(&self) {
        self.dialogs.show(
            |dialog, lines| choose(dialog, self.window.get(), &self.modal, lines),
            || self.wake(),
            || self.post(),
        );
    }
}

unsafe extern "system" fn observe(
    window: RawWindow,
    message: u32,
    wparam: usize,
    lparam: isize,
    subclass: usize,
    data: usize,
) -> isize {
    unsafe {
        // A timer can close the owner during Show. Hold the state through that reentry.
        Rc::increment_strong_count(data as *const Window);
        let state = Rc::from_raw(data as *const Window);
        if message == state.message {
            state.show();
            return 0;
        }
        if message == WM_NCDESTROY {
            state.window.set(std::ptr::null_mut());
            RemoveWindowSubclass(window, Some(observe), subclass);
            drop(Rc::from_raw(data as *const Window));
        }
        DefSubclassProc(window, message, wparam, lparam)
    }
}

fn choose(
    request: &FileDialog,
    owner: RawWindow,
    modal: &Schedule,
    lines: &mut Vec<String>,
) -> Outcome {
    if owner.is_null() {
        return Outcome::Refused("no window for the file dialog".into());
    }
    let mut unmatched = Vec::new();
    let filters = super::filters(&request.types, mime_extension, &mut unmatched);
    if !unmatched.is_empty() {
        // With none of the declared types to offer, the dialog would offer every file.
        if filters.is_empty() {
            return Outcome::Refused(format!(
                "Windows registers no extension for {}, the only types the app declares",
                unmatched.join(", ")
            ));
        }
        lines.push(format!(
            "{}: types: Windows registers no extension for {}, so the dialog cannot offer it",
            request.command,
            unmatched.join(", ")
        ));
    }
    unsafe {
        if let Err(error) = CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok() {
            return Outcome::Refused(format!("COM did not start for the file dialog ({error})"));
        }
        let result = ask(request, &filters, HWND(owner), modal, lines);
        CoUninitialize();
        match result {
            Ok(paths) => Outcome::Chosen(paths),
            Err(error) if error.code() == ERROR_CANCELLED.to_hresult() => Outcome::Cancelled,
            Err(error) => Outcome::Refused(format!("the file dialog failed ({error})")),
        }
    }
}

fn ask(
    request: &FileDialog,
    filters: &[super::Filter],
    owner: HWND,
    modal: &Schedule,
    lines: &mut Vec<String>,
) -> windows::core::Result<Vec<PathBuf>> {
    unsafe {
        let picker = &request.picker;
        let dialog: IFileDialog = match picker.kind {
            Kind::Save => CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER)?,
            _ => CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER)?,
        };
        dialog.SetOptions(FILEOPENDIALOGOPTIONS(super::options(
            picker.kind,
            picker.multiple,
            dialog.GetOptions()?.0,
        )))?;
        set_file_types(&dialog, picker, filters)?;
        if picker.kind == Kind::Save {
            dialog.SetFileName(&HSTRING::from(&picker.suggested_name))?;
        }
        super::start_folder(request, lines, |folder| {
            set_folder(&dialog, folder).map_err(|error| error.to_string())
        });
        {
            let _entered = modal.enter();
            dialog.Show(Some(owner))?;
        }
        if picker.kind == Kind::Save {
            return Ok(vec![path(dialog.GetResult()?)?]);
        }
        let items = dialog.cast::<IFileOpenDialog>()?.GetResults()?;
        paths((0..items.GetCount()?).map(|i| items.GetItemAt(i)))
    }
}

fn set_file_types(
    dialog: &IFileDialog,
    picker: &exact_runner::file_pickers::Picker,
    filters: &[super::Filter],
) -> windows::core::Result<()> {
    if filters.is_empty() {
        return Ok(());
    }
    let text: Vec<(HSTRING, HSTRING)> = filters
        .iter()
        .map(|f| (HSTRING::from(&f.label), HSTRING::from(f.spec())))
        .collect();
    let specs: Vec<COMDLG_FILTERSPEC> = text
        .iter()
        .map(|(label, spec)| COMDLG_FILTERSPEC {
            pszName: PCWSTR(label.as_ptr()),
            pszSpec: PCWSTR(spec.as_ptr()),
        })
        .collect();
    unsafe {
        dialog.SetFileTypes(&specs)?;
        if picker.kind == Kind::Save {
            let (index, extension) = super::save_type(filters, &picker.suggested_name);
            dialog.SetFileTypeIndex(index)?;
            // With a default extension set, Windows adds the chosen type's
            // extension to a name typed without one and follows the type
            // the person picks.
            // https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifiledialog-setdefaultextension
            dialog.SetDefaultExtension(&HSTRING::from(extension))?;
        }
    }
    Ok(())
}

/// The extension Windows registers for a MIME type
/// (`HKEY_CLASSES_ROOT\MIME\Database\Content Type\<type>`, value `Extension`).
fn mime_extension(mime: &str) -> Option<String> {
    if mime.contains('\\') {
        return None;
    }
    use windows_sys::Win32::{
        Foundation::ERROR_SUCCESS,
        System::Registry::{RegGetValueW, HKEY_CLASSES_ROOT, RRF_RT_REG_SZ},
    };
    let wide = |s: &str| s.encode_utf16().chain(Some(0)).collect::<Vec<u16>>();
    let (key, name) = (
        wide(&format!(r"MIME\Database\Content Type\{mime}")),
        wide("Extension"),
    );
    let mut buffer = [0u16; 64];
    let mut size = std::mem::size_of_val(&buffer) as u32;
    let status = unsafe {
        RegGetValueW(
            HKEY_CLASSES_ROOT,
            key.as_ptr(),
            name.as_ptr(),
            RRF_RT_REG_SZ,
            std::ptr::null_mut(),
            buffer.as_mut_ptr().cast(),
            &mut size,
        )
    };
    if status != ERROR_SUCCESS {
        return None;
    }
    let end = buffer.iter().position(|c| *c == 0)?;
    let extension = String::from_utf16(&buffer[..end]).ok()?.to_lowercase();
    (extension.len() > 1 && extension.starts_with('.') && !extension.contains(['*', '?', ';']))
        .then_some(extension)
}

fn set_folder(dialog: &IFileDialog, folder: &StartFolder) -> windows::core::Result<()> {
    unsafe {
        match folder {
            StartFolder::Chosen(folder) => {
                let item: IShellItem =
                    SHCreateItemFromParsingName(&crate::shell_path(folder)?, None)?;
                dialog.SetFolder(&item)
            }
            StartFolder::Known(name) => {
                let id = exact_data::known_folders::windows_id(name)
                    .map(windows::core::GUID::from_u128)
                    .ok_or_else(|| windows::core::Error::from_hresult(E_INVALIDARG))?;
                let item: IShellItem = SHGetKnownFolderItem(&id, KF_FLAG_DEFAULT, None)?;
                if !path(item.clone())?.is_dir() {
                    return Err(windows::core::Error::from_hresult(windows::core::HRESULT(
                        0x80070003u32 as i32,
                    )));
                }
                // Windows remembers the person's last folder ahead of a named default.
                // https://learn.microsoft.com/windows/win32/api/shobjidl_core/nf-shobjidl_core-ifiledialog-setdefaultfolder
                dialog.SetDefaultFolder(&item)
            }
        }
    }
}

fn paths(
    items: impl Iterator<Item = windows::core::Result<IShellItem>>,
) -> windows::core::Result<Vec<PathBuf>> {
    items.map(|item| path(item?)).collect()
}

fn path(item: IShellItem) -> windows::core::Result<PathBuf> {
    let text = unsafe { item.GetDisplayName(SIGDN_FILESYSPATH)? };
    Ok(PathBuf::from(taken(text)))
}

fn taken(text: PWSTR) -> std::ffi::OsString {
    use std::os::windows::ffi::OsStringExt;
    let owned = std::ffi::OsString::from_wide(unsafe { text.as_wide() });
    unsafe { CoTaskMemFree(Some(text.0.cast())) };
    owned
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_chosen_verbatim_folder_overrides_the_previous_folder() {
        let root = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("507-tmp")
            .join(format!("start-folder-{}", std::process::id()));
        let a = root.join("A");
        let b = root.join("B");
        for folder in [&a, &b] {
            std::fs::create_dir_all(folder).unwrap();
        }
        let a = std::fs::canonicalize(a).unwrap();
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            {
                let dialog: IFileDialog =
                    CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).unwrap();
                set_folder(&dialog, &StartFolder::Chosen(b)).unwrap();
                set_folder(&dialog, &StartFolder::Chosen(a.clone())).unwrap();
                assert_eq!(
                    std::fs::canonicalize(path(dialog.GetFolder().unwrap()).unwrap()).unwrap(),
                    a
                );
            }
            CoUninitialize();
        }
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn collecting_shell_paths_preserves_non_unicode_beside_unicode() {
        use std::os::windows::ffi::OsStringExt;
        let dir = std::env::current_dir()
            .unwrap()
            .join("target")
            .join("507-tmp")
            .join(format!("shell-paths-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let chosen = [
            dir.join(std::ffi::OsString::from_wide(&[0xd800, 0x2e, 0x74])),
            dir.join("good.txt"),
        ];
        for path in &chosen {
            std::fs::write(path, b"chosen").unwrap();
        }
        unsafe {
            CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
            let items = chosen.iter().map(|p| {
                SHCreateItemFromParsingName::<_, _, IShellItem>(&crate::shell_path(p)?, None)
            });
            let result = paths(items);
            CoUninitialize();
            assert_eq!(result.unwrap(), chosen);
        }
        std::fs::remove_dir_all(dir).unwrap();
    }

    #[test]
    fn a_save_dialog_takes_the_declared_types_and_starts_on_the_suggested_names() {
        use exact_painted::presenter::file_dialog::FileType;
        use exact_runner::file_pickers::Picker;
        let types = [
            (Some("Documents"), ".pdf"),
            (Some("Notes"), ".md"),
            (None, ".zip"),
        ]
        .map(|(name, suffix)| FileType {
            name: name.map(str::to_owned),
            suffixes: vec![suffix.into()],
            mimes: Vec::new(),
        });
        let filters = super::super::filters(&types, |_| None, &mut Vec::new());
        for (suggested, index) in [("Today.MD", 2), ("Bundle.zip", 3), ("Untitled", 1)] {
            let picker = Picker {
                kind: Kind::Save,
                id: "result".into(),
                multiple: false,
                suggested_name: suggested.into(),
                start_in: None,
            };
            unsafe {
                CoInitializeEx(None, COINIT_APARTMENTTHREADED).ok().unwrap();
                {
                    let dialog: IFileDialog =
                        CoCreateInstance(&FileSaveDialog, None, CLSCTX_INPROC_SERVER).unwrap();
                    set_file_types(&dialog, &picker, &filters).unwrap();
                    assert_eq!(dialog.GetFileTypeIndex().unwrap(), index, "{suggested}");
                }
                CoUninitialize();
            }
        }
    }

    #[test]
    fn a_registered_mime_type_names_its_extension() {
        assert_eq!(mime_extension("image/png").as_deref(), Some(".png"));
        assert_eq!(mime_extension("application/x-exact-unregistered"), None);
    }
}
