//! The UIA client and its results live on the same worker thread.
#![allow(unsafe_code)]
use super::{frame, owner, provider_role, Monitor};
use exact_painted::package::{json, Json};
use std::{
    ffi::OsString,
    os::windows::ffi::OsStringExt,
    path::{Path, PathBuf},
};
use windows::Win32::{
    Foundation::{E_ACCESSDENIED, POINT},
    System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, CLSCTX_INPROC_SERVER,
        COINIT_MULTITHREADED,
    },
    UI::Accessibility::*,
};

struct Apartment;
impl Drop for Apartment {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}

#[allow(non_upper_case_globals)]
fn role(id: UIA_CONTROLTYPE_ID) -> &'static str {
    match id {
        UIA_ButtonControlTypeId | UIA_SplitButtonControlTypeId => "button",
        UIA_HyperlinkControlTypeId => "link",
        UIA_CheckBoxControlTypeId => "checkbox",
        UIA_RadioButtonControlTypeId => "radio",
        UIA_EditControlTypeId => "textbox",
        UIA_ComboBoxControlTypeId => "combobox",
        UIA_ListControlTypeId => "list",
        UIA_ListItemControlTypeId => "listitem",
        UIA_TreeControlTypeId => "tree",
        UIA_TreeItemControlTypeId => "treeitem",
        UIA_MenuControlTypeId => "menu",
        UIA_MenuBarControlTypeId => "menubar",
        UIA_MenuItemControlTypeId => "menuitem",
        UIA_SliderControlTypeId => "slider",
        UIA_SpinnerControlTypeId => "spinbutton",
        UIA_ScrollBarControlTypeId => "scrollbar",
        UIA_TabControlTypeId => "tablist",
        UIA_TabItemControlTypeId => "tab",
        UIA_TableControlTypeId => "table",
        UIA_DataGridControlTypeId => "grid",
        UIA_ImageControlTypeId => "img",
        UIA_ToolBarControlTypeId => "toolbar",
        UIA_ProgressBarControlTypeId => "progressbar",
        UIA_GroupControlTypeId => "group",
        UIA_SeparatorControlTypeId => "separator",
        UIA_ToolTipControlTypeId => "tooltip",
        UIA_StatusBarControlTypeId => "status",
        UIA_DocumentControlTypeId => "document",
        _ => "generic",
    }
}

fn process_path(pid: u32) -> Option<PathBuf> {
    use windows_sys::Win32::{Foundation::CloseHandle, System::Threading::*};
    let process = unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid) };
    if process.is_null() {
        return None;
    }
    let mut path = vec![0u16; 32768];
    let mut length = path.len() as u32;
    let read = unsafe { QueryFullProcessImageNameW(process, 0, path.as_mut_ptr(), &mut length) };
    unsafe { CloseHandle(process) };
    if read == 0 {
        return None;
    }
    Some(OsString::from_wide(&path[..length as usize]).into())
}

fn description(path: &Path) -> Option<String> {
    use windows::Win32::{
        Storage::EnhancedStorage::PKEY_FileDescription,
        System::Com::CoTaskMemFree,
        UI::Shell::{IShellItem2, SHCreateItemFromParsingName},
    };
    let item: IShellItem2 =
        unsafe { SHCreateItemFromParsingName(&crate::shell_path(path).ok()?, None) }.ok()?;
    let string = unsafe { item.GetString(&PKEY_FileDescription) }.ok()?;
    if string.is_null() {
        return None;
    }
    let description = unsafe { string.to_string() }.ok();
    unsafe { CoTaskMemFree(Some(string.0.cast())) };
    description.filter(|s| !s.trim().is_empty())
}

pub(super) fn hit([x, y]: [i32; 2], monitors: &[Monitor]) -> Result<Json, &'static str> {
    let error = |e: windows::core::Error| match e.code() {
        E_ACCESSDENIED => "NotAllowedError",
        code if code.0 as u32 == UIA_E_ELEMENTNOTAVAILABLE => "NotFoundError",
        _ => "OperationError",
    };
    let _dpi = crate::screen_capture::os::Dpi::new();
    unsafe {
        CoInitializeEx(None, COINIT_MULTITHREADED)
            .ok()
            .map_err(error)?;
        let _apartment = Apartment;
        let automation: IUIAutomation =
            CoCreateInstance(&CUIAutomation8, None, CLSCTX_INPROC_SERVER).map_err(error)?;
        let element = automation.ElementFromPoint(POINT { x, y }).map_err(error)?;
        let protected = element.CurrentIsPassword().map_err(error)?.as_bool();
        let name = if protected {
            String::new()
        } else {
            element.CurrentName().map_err(error)?.to_string()
        };
        let aria = element.CurrentAriaRole().ok().map(|s| s.to_string());
        let role = match aria.as_deref().and_then(provider_role) {
            Some(role) => role,
            None => role(element.CurrentControlType().map_err(error)?),
        };
        let r = element.CurrentBoundingRectangle().map_err(error)?;
        let frame = frame([r.left, r.top, r.right, r.bottom], monitors).ok_or("NotFoundError")?;
        let pid = element.CurrentProcessId().map_err(error)?;
        if pid <= 0 {
            return Err("NotFoundError");
        }
        let path = process_path(pid as u32);
        let app = owner(
            pid,
            path.as_deref()
                .and_then(Path::file_name)
                .and_then(|s| s.to_str()),
            path.as_deref().and_then(description).as_deref(),
        );
        Ok(json!({"role":role,"name":name,"frame":frame,"app":app}))
    }
}
