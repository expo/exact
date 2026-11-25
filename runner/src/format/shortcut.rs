//! A single key cap in the reader's notation (LLP 1021 §5.3).
use crate::page::Keyboard;

pub(super) fn label(chord: &str, keyboard: Keyboard) -> String {
    if keyboard == Keyboard::Unknown
        || chord
            .chars()
            .any(|c| c.is_whitespace() || c.is_control() || c == '\u{feff}')
    {
        return String::new();
    }
    let apple = keyboard == Keyboard::Apple;
    let Some(chord) = exact_plan::shortcut::resolve(chord, if apple { "Meta" } else { "Control" })
    else {
        return String::new();
    };
    let (held, key) = if chord == "+" {
        (None, "Plus")
    } else if let Some(held) = chord.strip_suffix("++") {
        (Some(held), "Plus")
    } else if let Some((held, key)) = chord.rsplit_once('+') {
        (Some(held), key)
    } else {
        (None, chord.as_ref())
    };

    let mut mods = [false; 4];
    for token in held.into_iter().flat_map(|s| s.split('+')) {
        let i = match token {
            "Control" => 0,
            "Alt" => 1,
            "Shift" => 2,
            "Meta" => 3,
            _ => return String::new(),
        };
        mods[i] = true;
    }
    let key_label = match key {
        "ArrowUp" => ("↑", "Up arrow", "Up"),
        "ArrowDown" => ("↓", "Down arrow", "Down"),
        "ArrowLeft" => ("←", "Left arrow", "Left"),
        "ArrowRight" => ("→", "Right arrow", "Right"),
        "Enter" => ("↩", "Enter", "Enter"),
        "Escape" => ("⎋", "Esc", "Esc"),
        "Space" => ("Space", "Spacebar", "Space"),
        "Tab" => ("⇥", "Tab", "Tab"),
        "Backspace" => ("⌫", "Backspace", "Backspace"),
        "Delete" => ("⌦", "Delete", "Delete"),
        "Home" => ("↖", "Home", "Home"),
        "End" => ("↘", "End", "End"),
        "PageUp" => ("⇞", "Page up", "Page Up"),
        "PageDown" => ("⇟", "Page down", "Page Down"),
        "Insert" => ("Insert", "Insert", "Insert"),
        "Plus" => ("+", "Plus sign", "+"),
        "," => (",", "Comma", ","),
        "." => (".", "Period", "."),
        "-" => ("-", "Minus sign", "-"),
        k if k.encode_utf16().count() == 1 => (k, k, k),
        k if k
            .strip_prefix('F')
            .and_then(|n| n.parse::<u8>().ok())
            .is_some_and(|n| (1..=35).contains(&n) && k == format!("F{n}")) =>
        {
            (k, k, k)
        }
        _ => return String::new(),
    };
    let meta = match keyboard {
        Keyboard::Windows => "Windows logo key",
        Keyboard::Linux => "Super",
        _ => "Meta",
    };
    let names = if apple {
        ["⌃", "⌥", "⇧", "⌘"]
    } else {
        ["Ctrl", "Alt", "Shift", meta]
    };
    let order = if keyboard == Keyboard::Windows {
        [3, 0, 1, 2]
    } else {
        [0, 1, 2, 3]
    };
    let mut out = String::new();
    for i in order {
        if mods[i] {
            out.push_str(names[i]);
            if !apple {
                out.push('+');
            }
        }
    }
    let key = if apple {
        key_label.0
    } else if keyboard == Keyboard::Windows {
        key_label.1
    } else {
        key_label.2
    };
    if key.len() == 1 {
        out.push(char::from(key.as_bytes()[0]).to_ascii_uppercase());
    } else {
        out.push_str(key);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn the_readers_keyboard_supplies_the_key_cap() {
        for (chord, apple, windows) in [
            ("Shift+Mod+o", "⇧⌘O", "Ctrl+Shift+O"),
            (
                "Meta+Shift+Alt+Control+ArrowUp",
                "⌃⌥⇧⌘↑",
                "Windows logo key+Ctrl+Alt+Shift+Up arrow",
            ),
            ("Mod+Insert", "⌘Insert", "Ctrl+Insert"),
            ("Mod+ß", "⌘ß", "Ctrl+ß"),
            ("Mod+𐐀", "", ""),
            ("Mod+Enter", "⌘↩", "Ctrl+Enter"),
            ("Escape", "⎋", "Esc"),
            ("Space", "Space", "Spacebar"),
            ("Mod++", "⌘+", "Ctrl+Plus sign"),
            ("F35", "F35", "F35"),
            ("F36", "", ""),
            ("Mod+Meta+O", "", ""),
            ("Mod+Control+O", "", ""),
            ("Hyper+O", "", ""),
            ("Mod+O Mod+P", "", ""),
            ("+O", "", ""),
        ] {
            assert_eq!(label(chord, Keyboard::Apple), apple, "{chord}");
            assert_eq!(label(chord, Keyboard::Windows), windows, "{chord}");
            assert_eq!(label(chord, Keyboard::Unknown), "", "{chord}");
        }
        assert_eq!(label("Meta+O", Keyboard::Linux), "Super+O");
        assert_eq!(label("Meta+O", Keyboard::Android), "Meta+O");
    }
}
