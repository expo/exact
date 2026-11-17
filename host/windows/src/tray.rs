//! The notification area (LLP 1133.025 D4): each `-exact-menu="status"`
//! button is an icon there, by `Shell_NotifyIconW` under
//! `NOTIFYICON_VERSION_4`, and its `popovertarget` menu is the shared
//! presenter's MenuFlyout in a popup window of its own beside the icon
//! (`tray/flyout.rs`, LLP 1115.004 D10.1), so it opens with the app's window
//! hidden.
//!
//! - **The icon** is the button's image (`Presenter::status_icon`), drawn at
//!   the system's small-icon size for its taskbar monitor's DPI. A button
//!   with no image, or one still loading, shows the app's own icon (D7). A `disabled`
//!   button's icon is dimmed to WinUI's disabled text opacity. The tooltip
//!   is its label (`szTip`).
//! - **The clicks**: the shell sends `NIN_SELECT` for a click and
//!   `NIN_KEYSELECT` for a keyboard selection, which run the button's
//!   `press`, or open its menu when it has none; `WM_CONTEXTMENU` (the right
//!   button, Shift+F10, the Menu key) opens the menu and never presses
//!   ([Shell_NotifyIconW](https://learn.microsoft.com/windows/win32/api/shellapi/nf-shellapi-shell_notifyiconw)).
//! - **Explorer restarting** broadcasts `TaskbarCreated` to every top-level
//!   window, and each icon is added again
//!   ([Taskbar Creation Notification](https://learn.microsoft.com/windows/win32/shell/taskbar#taskbar-creation-notification)).
//!   The icons' window is therefore a hidden top-level window, not a
//!   message-only one, which broadcasts do not reach.
//! - **An icon's identity** is its window and an id hashed from the
//!   button's `id`, else its `testId`, so that the person's choice to show
//!   or hide it in the taskbar follows it across launches, as the Mac's
//!   `autosaveName` keeps a status item's place. A production build also
//!   gives it a GUID derived from the app's `id` and that name, which the
//!   shell prefers, and which no update changes. The shell binds a GUID to
//!   the executable's path, so a development build, whose path moves, gives
//!   none, and an icon whose GUID the shell refuses is added by its id
//!   alone ([NOTIFYICONDATAW](https://learn.microsoft.com/windows/win32/api/shellapi/ns-shellapi-notifyicondataw#troubleshooting)).
//!   Each icon's callback is its own message, `CALLBACK` plus its id: "if
//!   guidItem is specified, uID is ignored", so `lParam`'s high word is not
//!   relied on to say which icon was clicked.

/// `WM_APP`: icon `id`'s callback message is `CALLBACK + id`, so ids run
/// from 1 to [`ID_MAX`], which keeps every message below `0xC000`, where
/// registered messages start.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) const CALLBACK: u32 = 0x8000;
const ID_MAX: u16 = 0x3FFF;
/// `NIN_SELECT` (`WM_USER`), and `NIN_KEYSELECT` (`NIN_SELECT | NINF_KEY`).
const NIN_SELECT: u16 = 0x400;
const NIN_KEYSELECT: u16 = 0x401;
const WM_CONTEXTMENU: u16 = 0x7B;

/// What a notification icon's callback says.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Event {
    /// A click (`keyboard` false) or a keyboard selection.
    Select { keyboard: bool },
    /// The icon's context menu was asked for.
    Menu,
    /// Anything else (a hover, a double click, a balloon).
    Other,
}

/// A callback, decoded: `message` names the icon (`CALLBACK + id`); under
/// version 4 `lParam`'s low word is the event, and `wParam` holds the
/// anchor's screen coordinates, as `GET_X_LPARAM` and `GET_Y_LPARAM` read
/// them.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Callback {
    pub event: Event,
    pub icon: u16,
    pub at: (i32, i32),
}

/// `message` decoded, or `None` when it is no icon's callback.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn callback(message: u32, wparam: usize, lparam: isize) -> Option<Callback> {
    let icon = u16::try_from(message.checked_sub(CALLBACK)?)
        .ok()
        .filter(|id| (1..=ID_MAX).contains(id))?;
    let event = match lparam as usize as u16 {
        NIN_SELECT => Event::Select { keyboard: false },
        NIN_KEYSELECT => Event::Select { keyboard: true },
        WM_CONTEXTMENU => Event::Menu,
        _ => Event::Other,
    };
    Some(Callback {
        event,
        icon,
        at: (
            wparam as u16 as i16 as i32,
            (wparam >> 16) as u16 as i16 as i32,
        ),
    })
}

/// What a click does to an item (D4's Windows clicks).
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Does {
    Press,
    Menu { keyboard: bool },
    Nothing,
}

/// `event` on an item with or without a `press` and a menu: a selection
/// presses, or opens the menu when there is no `press`; the context menu
/// opens the menu and never presses; a disabled item does neither.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn does(event: Event, press: bool, menu: bool, disabled: bool) -> Does {
    match event {
        _ if disabled => Does::Nothing,
        Event::Select { .. } if press => Does::Press,
        Event::Select { keyboard } if menu => Does::Menu { keyboard },
        Event::Menu if menu => Does::Menu { keyboard: false },
        _ => Does::Nothing,
    }
}

/// The icon's id for an item named `name`, distinct from the ids `taken`:
/// FNV-1a folded to 1 to [`ID_MAX`], else the next free one. Of two names
/// that fold alike, the first listed keeps the id.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn id(name: &str, taken: &[u16]) -> u16 {
    let hash = name.bytes().fold(0x811c_9dc5_u32, |h, b| {
        (h ^ u32::from(b)).wrapping_mul(0x0100_0193)
    });
    let next = |id: u16| id % ID_MAX + 1;
    let mut id = next(((hash >> 16) ^ hash) as u16);
    for _ in 0..ID_MAX {
        if !taken.contains(&id) {
            break;
        }
        id = next(id);
    }
    id
}

/// The icon's GUID in a production build: a name-based UUID (version 8,
/// from SHA-256, as RFC 9562 §6.5 builds one) of the app's `id` and the
/// item's `name`, so the same app and button give the same GUID in every
/// build and update.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn guid(app: &str, name: &str) -> u128 {
    use sha2::{Digest, Sha256};
    let digest = Sha256::new()
        .chain_update("exact notification icon\0")
        .chain_update(app)
        .chain_update("\0")
        .chain_update(name)
        .finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0F) | 0x80;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;
    u128::from_be_bytes(bytes)
}

/// Whose GUIDs a binary's icons take: its app's `id` in a production
/// build (`compat`'s embedded trust), else none.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn guid_owner(compat: &str) -> Option<String> {
    if !exact_runner::delivery::production(compat) {
        return None;
    }
    serde_json::from_str::<serde_json::Value>(compat)
        .ok()?
        .pointer("/inputs/app")?
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_owned)
}

/// What the shell knows an icon by: its id, and its GUID if it has one.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) struct Icon {
    pub id: u16,
    pub guid: Option<u128>,
}

/// `szTip`: the label in at most 127 UTF-16 units and a NUL, never ending
/// on half a surrogate pair.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn tip(label: &str) -> [u16; 128] {
    let mut out = [0u16; 128];
    let mut at = 0;
    for c in label.chars() {
        let mut units = [0u16; 2];
        let units = c.encode_utf16(&mut units);
        if at + units.len() > 127 {
            break;
        }
        out[at..at + units.len()].copy_from_slice(units);
        at += units.len();
    }
    out
}

/// WinUI's disabled text opacity (`TextFillColorDisabled`, `#5C` of 255),
/// which a disabled item's icon takes.
const DIMMED: u32 = 0x5C;

/// An icon's colour bitmap from premultiplied RGBA pixels: BGRA with
/// straight alpha, as a 32-bit icon image holds it, dimmed for a disabled
/// item.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn icon_bgra(rgba: &[u8], dimmed: bool) -> Vec<u8> {
    let mut out = Vec::with_capacity(rgba.len());
    for px in rgba.as_chunks::<4>().0 {
        let a = u32::from(px[3]);
        let straight = |c: u8| match a {
            0 => 0,
            _ => ((u32::from(c) * 255 + a / 2) / a).min(255) as u8,
        };
        let alpha = if dimmed { (a * DIMMED + 127) / 255 } else { a };
        out.extend([
            straight(px[2]),
            straight(px[1]),
            straight(px[0]),
            alpha as u8,
        ]);
    }
    out
}

/// The system's small-icon metric and the DPI it was measured for.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn icon_metrics(side: u32, dpi: u32) -> (u32, f32) {
    (side.max(16), dpi.max(96) as f32 / 96.)
}

/// Whether the taskbar is dark, from `SystemUsesLightTheme` under
/// `HKCU\…\Themes\Personalize` (the system mode the taskbar follows, apart
/// from the apps' mode): 1 is light; 0, or a Windows that keeps no value
/// (before version 1903, whose taskbar was always dark), is dark.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn taskbar_dark(system_uses_light: Option<u32>) -> bool {
    system_uses_light != Some(1)
}

/// What the shell must be told to show `wanted` where it shows `placed`:
/// each `(key, picture)`, in order, where a key is what the shell knows an
/// icon by and a picture is whatever tells two icons apart (the tooltip and
/// the pixels). A key that changes is a removal and an addition, and every
/// removal comes first.
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) enum Op<K> {
    Add(K),
    Modify(K),
    Delete(K),
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn ops<K: Copy + PartialEq, T: PartialEq>(
    placed: &[(K, T)],
    wanted: &[(K, T)],
) -> Vec<Op<K>> {
    let gone = placed
        .iter()
        .filter(|(key, _)| !wanted.iter().any(|(w, _)| w == key))
        .map(|(key, _)| Op::Delete(*key));
    let changed =
        wanted.iter().filter_map(
            |(key, picture)| match placed.iter().find(|(p, _)| p == key) {
                None => Some(Op::Add(*key)),
                Some((_, was)) if was != picture => Some(Op::Modify(*key)),
                Some(_) => None,
            },
        );
    gone.chain(changed).collect()
}

/// A key in the menu's window, by `KeyboardEvent.key`'s name: the keys a
/// menu takes, and a letter or digit for its type-ahead.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn key(vk: u32) -> Option<String> {
    Some(
        match vk {
            0x0D => "Enter",
            0x1B => "Escape",
            0x20 => " ",
            0x09 => "Tab",
            0x21 => "PageUp",
            0x22 => "PageDown",
            0x23 => "End",
            0x24 => "Home",
            0x25 => "ArrowLeft",
            0x26 => "ArrowUp",
            0x27 => "ArrowRight",
            0x28 => "ArrowDown",
            0x30..=0x39 | 0x41..=0x5A => {
                return char::from_u32(vk).map(|c| c.to_ascii_lowercase().to_string())
            }
            _ => return None,
        }
        .to_owned(),
    )
}

/// A wheel notch's `delta` (`WHEEL_DELTA` is 120) in points down the menu,
/// at the person's lines per notch and WinUI's 16-point line.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn wheel(delta: i16, lines: u32) -> f32 {
    -f32::from(delta) / 120. * lines as f32 * 16.
}

#[cfg(windows)]
mod flyout;
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) mod placement;
#[cfg(windows)]
pub(crate) mod shell;
#[cfg(windows)]
pub(crate) use flyout::{Flyout, Input};
#[cfg(windows)]
pub(crate) use shell::{Heard, Shell, Shown};

#[cfg(test)]
mod tests;
