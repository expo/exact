//! Windows clipboard formats for @exact/clipboard (LLP 1133.006).
//! PNG is offered first, then an sRGB bitmap when valid, then optional text.

mod image;
pub use image::Image;

#[cfg(windows)]
mod system;
#[cfg(windows)]
pub(crate) use system::Clipboard;

use std::cell::RefCell;

/// One validated clipboard item. Preparing it does not change the clipboard.
#[derive(Default)]
pub struct Item {
    /// Unicode text, when the item carries text/plain.
    pub text: Option<String>,
    /// Decoded pixels with colour information, without text or EXIF metadata.
    pub image: Option<Image>,
}

impl Item {
    /// The bounded agent disclosure: types and dimensions, never image bytes.
    pub fn description(&self, agent: bool) -> serde_json::Value {
        use serde_json::json;
        let mut types = Vec::new();
        let mut platform_types = Vec::new();
        if self.text.is_some() {
            types.push("text/plain");
        }
        if let Some(image) = &self.image {
            types.push("image/png");
            platform_types.push("PNG");
            if image.dib.is_some() {
                platform_types.push("CF_DIBV5");
            }
        }
        if self.text.is_some() {
            platform_types.push("CF_UNICODETEXT");
        }
        let mut shown = json!({
            "types": types, "platformTypes": platform_types,
            "pasteboard": if agent { "memory" } else { "Windows" },
        });
        if let Some(text) = &self.text {
            shown["text"] = json!(text.chars().take(200).collect::<String>());
        }
        if let Some(image) = &self.image {
            shown["image"] = image.description();
        }
        shown
    }
}

thread_local! {
    // The Windows carrier has one session per window thread. Its agent
    // pasteboard is also used by the presenter's text clipboard adapter.
    pub(crate) static PRIVATE: RefCell<Item> = RefCell::new(Item::default());
}

/// Replace this thread's clipboard with a prepared item. Under the agent
/// keep it in memory; otherwise require the host window on this thread.
/// Nothing is emptied for an empty item.
pub fn write(item: Item, agent: bool) -> Result<(), String> {
    if item.text.is_none() && item.image.is_none() {
        return Ok(());
    }
    if agent {
        PRIVATE.with(|board| *board.borrow_mut() = item);
        return Ok(());
    }
    #[cfg(windows)]
    return system::write(&item);
    #[cfg(not(windows))]
    Err("Windows clipboard is unavailable on this host".into())
}

#[cfg(any(windows, test))]
pub(super) fn text_bytes(text: &str) -> Vec<u8> {
    text.replace("\r\n", "\n")
        .replace('\r', "\n")
        .replace('\n', "\r\n")
        .encode_utf16()
        .chain(Some(0))
        .flat_map(u16::to_le_bytes)
        .collect()
}

#[cfg(any(windows, test))]
pub(crate) fn run_headless<D: exact_runner::DataSource + Default>(
    config: &mut exact_painted::app::Config,
    started: std::time::Instant,
) -> i32 {
    let adapter: Box<dyn exact_painted::presenter::clipboard::Clipboard> = if substituting() {
        Box::new(Memory)
    } else {
        #[cfg(windows)]
        match Clipboard::headless() {
            Ok(adapter) => Box::new(adapter),
            Err(error) => {
                eprintln!("exact-windows: {error}");
                return 1;
            }
        }
        #[cfg(not(windows))]
        return 1;
    };
    exact_painted::app::run_headless::<D>(config, started, Some(adapter))
}

/// Whether the editor's Paste has text to insert: under the agent's memory
/// board, that board's text, which the paste reads (`Memory`, and
/// `Clipboard::read_text` while substituting); otherwise `system`, whether
/// the Windows clipboard has text.
#[cfg(windows)]
pub(crate) fn paste_available(system: impl FnOnce() -> bool) -> bool {
    paste_from(substituting(), system)
}

#[cfg(any(windows, test))]
fn paste_from(substituting: bool, system: impl FnOnce() -> bool) -> bool {
    match substituting {
        true => PRIVATE.with(|board| board.borrow().text.is_some()),
        false => system(),
    }
}

#[cfg(any(windows, test))]
fn substituting() -> bool {
    exact_painted::picker::agent()
        && std::env::var("EXACT_DEVICE").as_deref() != Ok("real")
        && !std::env::var("EXACT_AGENT_BIND")
            .unwrap_or_default()
            .split(',')
            .any(|s| s == "clipboard")
}

#[cfg(any(windows, test))]
struct Memory;
#[cfg(any(windows, test))]
impl exact_painted::presenter::clipboard::Clipboard for Memory {
    fn read_text(&mut self) -> Result<Option<String>, String> {
        Ok(PRIVATE.with(|board| board.borrow().text.clone()))
    }
    fn write_text(&mut self, text: &str) -> Result<(), String> {
        write(
            Item {
                text: Some(text.into()),
                image: None,
            },
            true,
        )
    }
}
#[cfg(any(windows, test))]
impl Drop for Memory {
    fn drop(&mut self) {
        PRIVATE.with(|board| *board.borrow_mut() = Item::default());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(windows)]
    #[test]
    fn mouse_selection_copies_both_drag_directions_into_a_windows_textarea() {
        use exact_painted::{presenter::PainterChoice, Presenter};
        let plan = contract::compile(
            r#"component App
  view
    column width=240
      text "First" testId="first" user-select="text" width=200 height=32
      text "Second" testId="second" user-select="text" width=200 height=32
      textarea testId="editor" width=200 height=64
"#,
        )
        .unwrap()
        .encode();
        let (mut p, warning) = Presenter::boot_with(
            &plan,
            (),
            (240., 180.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(warning.is_none(), "{warning:?}");
        p.set_clipboard(Box::new(Memory));
        let [first, second, editor] = ["first", "second", "editor"].map(|name| {
            let k = p.host().kernel();
            k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
        });
        let point = |p: &mut Presenter<()>, id, start: bool| {
            let (x, y, w, h) = p.boxes().iter().find(|b| b.id == id).unwrap().rect;
            (x + if start { 1. } else { w - 1. }, y + h / 2.)
        };
        let start = point(&mut p, first, true);
        let end = point(&mut p, second, false);
        let chord = |p: &mut Presenter<()>, code, key| {
            for (code, key, down) in [
                ("ControlLeft", "Control", true),
                (code, key, true),
                (code, key, false),
                ("ControlLeft", "Control", false),
            ] {
                crate::keyboard::press(p, (code, key), None, down, false);
            }
        };
        for (from, to, time) in [(start, end, 1.), (end, start, 4.)] {
            write(
                Item {
                    text: Some("unchanged".into()),
                    image: None,
                },
                true,
            )
            .unwrap();
            p.pointer_down_with_clicks(from.0, from.1, time, 1).unwrap();
            p.pointer_move(to.0, to.1, time + 1.).unwrap();
            p.pointer_up(to.0, to.1, time + 2.).unwrap();
            chord(&mut p, "KeyC", "c");
            assert_eq!(
                PRIVATE.with(|v| v.borrow().text.clone()).as_deref(),
                Some("First\nSecond")
            );
            p.tap(editor).unwrap();
            chord(&mut p, "KeyA", "a");
            chord(&mut p, "KeyV", "v");
            let field = p
                .access_tree()
                .0
                .into_iter()
                .find(|v| v.view == editor)
                .unwrap();
            assert_eq!(field.value, "First\nSecond");
        }
    }

    /// The edit menu offers Paste from the board Ctrl+V pastes from: the
    /// memory board under the agent, whatever the Windows clipboard holds.
    #[test]
    fn paste_is_offered_from_the_board_the_editor_pastes_from() {
        PRIVATE.with(|board| *board.borrow_mut() = Item::default());
        assert!(!paste_from(true, || true), "nothing copied in the app");
        let copied = Item {
            text: Some("copied".into()),
            image: None,
        };
        write(copied, true).unwrap();
        assert!(paste_from(true, || false), "an in-app copy");
        assert!(paste_from(false, || true));
        assert!(!paste_from(false, || false));
        PRIVATE.with(|board| *board.borrow_mut() = Item::default());
    }
}
