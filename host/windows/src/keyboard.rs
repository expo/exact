//! Windows key names, what a key types, and held modifiers.
use exact_painted::Presenter;
use exact_runner::{DataSource, KeyModifiers};

#[cfg(windows)]
pub(super) fn key_names(
    physical: winit::keyboard::PhysicalKey,
    logical: &winit::keyboard::Key,
    text: Option<&str>,
) -> (String, String) {
    use winit::keyboard::{Key, PhysicalKey};
    let code = match physical {
        PhysicalKey::Code(code) => format!("{code:?}").replace("Super", "Meta"),
        PhysicalKey::Unidentified(_) => "Unidentified".into(),
    };
    let key = match logical {
        Key::Character(s) => s.to_string(),
        Key::Named(k) => format!("{k:?}"),
        // The DOM's name ([UI Events KeyboardEvent key
        // Values](https://www.w3.org/TR/uievents-key/#keys-composition)).
        Key::Dead(_) => "Dead".into(),
        _ => text.unwrap_or("Unidentified").to_owned(),
    };
    (code, key)
}

/// What a pressed key types when that is not `key`, its logical key's name.
/// After a dead key, Windows translates the next key with it: Space to the
/// accent alone, and a character the accent does not combine with to the
/// accent and then the character, as two `WM_CHAR` messages
/// ([Dead-Character Messages](https://learn.microsoft.com/windows/win32/inputdev/about-keyboard-input#dead-character-messages)).
/// winit gives that text beside the key's own logical key (Space, or the
/// letter alone). A control character (Enter's `"\r"`, Tab's) is no text.
pub(super) fn typed<'a>(key: &str, text: Option<&'a str>) -> Option<&'a str> {
    let text = text?;
    let own = if key == "Space" { " " } else { key };
    (text != own && !text.chars().any(char::is_control)).then_some(text)
}

/// A device key to the presenter. The app's `key` handlers hear `key`; a
/// press's default action types [`typed`]'s text when there is one, as a
/// browser's `keydown` names the key and its `input` carries the text.
pub(super) fn press<D: DataSource>(
    presenter: &mut Presenter<D>,
    (code, key): (&str, &str),
    text: Option<&str>,
    down: bool,
    repeat: bool,
) {
    match typed(key, text).filter(|_| down) {
        Some(text) => presenter.hardware_key_typing(code, key, text, repeat),
        None => presenter.hardware_key(code, key, down, repeat),
    }
}

/// Outside ModifiersChanged, only clear releases consumed by a menu or focus change.
#[cfg(windows)]
#[allow(unsafe_code)]
pub(super) fn sync<D: DataSource>(
    presenter: &mut Presenter<D>,
    modifiers: Option<winit::keyboard::ModifiersState>,
) {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::*;
    // GetKeyState follows this thread's input queue.
    let pressed = [
        [VK_LSHIFT, VK_RSHIFT],
        [VK_LCONTROL, VK_RCONTROL],
        [VK_LMENU, VK_RMENU],
        [VK_LWIN, VK_RWIN],
    ]
    .map(|pair| pair.map(|key| unsafe { GetKeyState(i32::from(key)) } < 0));
    reconcile(
        modifiers.map(|state| KeyModifiers {
            shift: state.shift_key(),
            ctrl: state.control_key(),
            alt: state.alt_key(),
            meta: state.super_key(),
        }),
        pressed,
        |code, down| presenter.hold_modifier(code, down),
    );
}

fn reconcile(
    modifiers: Option<KeyModifiers>,
    pressed: [[bool; 2]; 4],
    mut hold: impl FnMut(&str, bool),
) {
    let active = modifiers.unwrap_or_default();
    for ((codes, active), sides) in [
        (["ShiftLeft", "ShiftRight"], active.shift),
        (["ControlLeft", "ControlRight"], active.ctrl),
        (["AltLeft", "AltRight"], active.alt),
        (["MetaLeft", "MetaRight"], active.meta),
    ]
    .into_iter()
    .zip(pressed)
    {
        for (code, down) in codes.into_iter().zip(sides) {
            // winit filters AltGr's synthetic Control and Alt. Raw key state
            // identifies a side, but cannot introduce a modifier it filtered.
            if modifiers.is_some() {
                hold(code, active && down);
            } else if !down {
                hold(code, false);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_painted::presenter::{clipboard::Clipboard, PainterChoice};
    use exact_painted::Presenter;
    use std::{cell::RefCell, rc::Rc};

    struct Memory(Rc<RefCell<String>>);
    impl Clipboard for Memory {
        fn read_text(&mut self) -> Result<Option<String>, String> {
            Ok(Some(self.0.borrow().clone()))
        }
        fn write_text(&mut self, text: &str) -> Result<(), String> {
            *self.0.borrow_mut() = text.into();
            Ok(())
        }
    }

    #[test]
    fn altgr_types_text_and_leaves_no_control_chord() {
        let plan = contract::compile(
            "component App\n  view\n    input testId=\"field\" value=\"name\" width=200 height=40\n",
        )
        .unwrap()
        .encode();
        let (mut p, warning) = Presenter::boot_with(
            &plan,
            (),
            (240., 80.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(warning.is_none());
        let clipboard = Rc::new(RefCell::new("untouched".to_string()));
        p.set_clipboard(Box::new(Memory(clipboard.clone())));
        let k = p.host().kernel();
        let field = k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id;
        p.tap(field).unwrap();
        p.hardware_key("End", "End", true, false);
        p.hardware_key("End", "End", false, false);

        // AltGr's synthetic Control press can first report Control held.
        reconcile(
            Some(KeyModifiers {
                ctrl: true,
                ..Default::default()
            }),
            [[false; 2], [true, false], [false; 2], [false; 2]],
            |code, down| p.hold_modifier(code, down),
        );
        // winit filters Control and Alt while right Alt is down.
        reconcile(
            Some(KeyModifiers::default()),
            [[false; 2], [true, false], [false, true], [false; 2]],
            |code, down| p.hold_modifier(code, down),
        );
        p.hardware_key("AltRight", "AltGraph", true, false);
        p.hardware_key("KeyQ", "@", true, false);
        p.hardware_key("KeyQ", "@", false, false);
        p.hardware_key("AltRight", "AltGraph", false, false);
        // No ModifiersChanged or Control release follows AltGr's release.
        for ch in "example.com".chars() {
            let code = if ch == '.' {
                "Period".into()
            } else {
                format!("Key{}", ch.to_ascii_uppercase())
            };
            let key = ch.to_string();
            p.hardware_key(&code, &key, true, false);
            p.hardware_key(&code, &key, false, false);
        }
        let value = p
            .access_tree()
            .0
            .into_iter()
            .find(|n| n.view == field)
            .unwrap()
            .value;
        assert_eq!(
            (value.as_str(), clipboard.borrow().as_str()),
            ("name@example.com", "untouched")
        );

        for side in 0..2 {
            let mut pressed = [[false; 2]; 4];
            pressed[1][side] = true;
            reconcile(
                Some(KeyModifiers {
                    ctrl: true,
                    ..Default::default()
                }),
                pressed,
                |code, down| p.hold_modifier(code, down),
            );
            for (code, key) in [("KeyA", "a"), ("KeyC", "c")] {
                p.hardware_key(code, key, true, false);
                p.hardware_key(code, key, false, false);
            }
            assert_eq!(clipboard.borrow().as_str(), "name@example.com");
            *clipboard.borrow_mut() = "untouched".into();
            // A native menu may consume the Control release.
            reconcile(None, [[false; 2]; 4], |code, down| {
                p.hold_modifier(code, down)
            });
            p.hardware_key("End", "End", true, false);
            p.hardware_key("End", "End", false, false);
        }
        p.hardware_key("KeyX", "x", true, false);
        p.hardware_key("KeyX", "x", false, false);
        let value = p
            .access_tree()
            .0
            .into_iter()
            .find(|n| n.view == field)
            .unwrap()
            .value;
        assert_eq!(value, "name@example.comx");
    }

    /// A dead key then Space, a letter it does not combine with, and one it
    /// does, as winit reports them on US-International.
    #[test]
    fn a_dead_key_types_what_windows_translates() {
        let plan = contract::compile(
            "component App\n  state keys = \"\"\n  action heard(value: string)\n    keys = `${keys}[${value}]`\n  view\n    column\n      input testId=\"field\" value=\"h\" key=heard width=200 height=40\n      text keys testId=\"keys\" height=20\n",
        )
        .unwrap()
        .encode();
        let (mut p, warning) = Presenter::boot_with(
            &plan,
            (),
            (240., 80.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(warning.is_none());
        let k = p.host().kernel();
        let field = k.node_by_key(k.find_by_test_id("field")[0]).unwrap().id;
        p.tap(field).unwrap();
        p.hardware_key("End", "End", true, false);
        p.hardware_key("End", "End", false, false);
        let mut typed = Vec::new();
        for (code, key, text) in [
            ("Space", "Space", "'"),
            ("KeyT", "t", "'t"),
            ("KeyE", "é", "é"),
        ] {
            press(&mut p, ("Quote", "Dead"), None, true, false);
            press(&mut p, ("Quote", "Dead"), None, false, false);
            press(&mut p, (code, key), Some(text), true, false);
            press(&mut p, (code, key), None, false, false);
            let value = p.access_tree().0.into_iter().find(|n| n.view == field);
            typed.push(value.unwrap().value);
        }
        assert_eq!(typed, ["h'", "h''t", "h''té"]);
        assert_eq!(
            p.host().runner().slot("keys"),
            Some(&exact_runner::Value::str(
                "[End][Dead][ ][Dead][t][Dead][é]"
            ))
        );
        assert_eq!(super::typed("Enter", Some("\r")), None);
        assert_eq!(super::typed("Space", Some(" ")), None);
    }

    #[test]
    fn modifiers_keep_their_sides_and_recovery_only_releases() {
        let codes = [
            "ShiftLeft",
            "ShiftRight",
            "ControlLeft",
            "ControlRight",
            "AltLeft",
            "AltRight",
            "MetaLeft",
            "MetaRight",
        ];
        for (group, name) in ["Shift", "Control", "Alt", "Meta"].into_iter().enumerate() {
            for sides in [[true, false], [false, true], [true, true], [false, false]] {
                let mut pressed = [[true; 2]; 4];
                pressed[group] = sides;
                let mut held = Vec::new();
                reconcile(KeyModifiers::held(name), pressed, |code, down| {
                    held.push((code.to_owned(), down));
                });
                assert_eq!(held.len(), codes.len());
                for (index, (code, down)) in held.into_iter().enumerate() {
                    assert_eq!(code, codes[index]);
                    assert_eq!(
                        down,
                        index / 2 == group && sides[index % 2],
                        "{name} {code}"
                    );
                }
            }
        }
        let mut released = Vec::new();
        reconcile(
            None,
            [[false; 2], [true, false], [false, true], [false; 2]],
            |code, down| {
                assert!(!down, "raw state must not restore {code}");
                released.push(code.to_owned());
            },
        );
        assert_eq!(
            released,
            [
                "ShiftLeft",
                "ShiftRight",
                "ControlRight",
                "AltLeft",
                "MetaLeft",
                "MetaRight"
            ]
        );
    }
}
