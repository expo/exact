//! Desktop input follows the painted presenter's pointer, keyboard and composition paths.
use exact_painted::{presenter::ime::Target, presenter::PointerKind, Presenter};
use exact_runner::DataSource;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, PhysicalKey},
    window::{CursorIcon, Window},
};

pub(super) const SCROLL_LINE: f32 = 48.;

#[derive(Default)]
pub(super) struct Input {
    pointer: Option<(f32, f32)>,
    buttons: u32,
    previous_click: Option<(f64, (f32, f32), u32)>,
    focused: bool,
    occluded: bool,
    allowed: Option<Target>,
    session: Option<Target>,
    preedit: Preedit,
    ime_area: Option<(f32, f32, f32, f32)>,
}

#[derive(Default, PartialEq, Eq)]
enum Preedit {
    #[default]
    Idle,
    Active,
    Clearing,
}

impl Input {
    pub(super) fn active(&self) -> bool {
        self.focused && !self.occluded
    }

    pub(super) fn handle<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        event: WindowEvent,
        now_ms: f64,
    ) -> Result<(), String> {
        match event {
            WindowEvent::Focused(focused) => {
                self.focused = focused;
                self.previous_click = None;
                if !focused {
                    presenter.ime_end();
                    presenter.pointer_lost(now_ms)?;
                    self.buttons = 0;
                    self.session = None;
                    self.preedit = Preedit::Idle;
                    presenter.forget_pads();
                }
                presenter.set_window_active(focused);
                presenter.surface_lifecycle(self.occluded, !focused);
            }
            WindowEvent::Occluded(occluded) => {
                self.occluded = occluded;
                presenter.surface_lifecycle(occluded, !self.focused);
            }
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical::<f32>(window.scale_factor());
                let point = (position.x, position.y);
                if self.previous_click.is_some_and(|(_, origin, _)| {
                    (point.0 - origin.0).abs() > 5. || (point.1 - origin.1).abs() > 5.
                }) {
                    self.previous_click = None;
                }
                self.pointer = Some(point);
                if !pointer_mapped(presenter, window) {
                    return Ok(());
                }
                presenter.set_pointer_kind(PointerKind::Mouse);
                presenter.pointer_move(point.0, point.1, now_ms)?;
                let cursor = presenter
                    .cursor_at(point.0, point.1)
                    .name()
                    .parse::<CursorIcon>()
                    .unwrap_or(CursorIcon::Default);
                window.set_cursor(cursor);
            }
            WindowEvent::CursorLeft { .. } => {
                if self.buttons == 0 {
                    self.pointer = None;
                    presenter.pointer_lost(now_ms)?;
                }
                window.set_cursor(CursorIcon::Default);
            }
            WindowEvent::MouseInput { state, button, .. } => {
                let Some((x, y)) = self.pointer else {
                    return Ok(());
                };
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    MouseButton::Back => 8,
                    MouseButton::Forward => 16,
                    _ => return Ok(()),
                };
                let down = state == ElementState::Pressed;
                presenter.set_pointer_kind(PointerKind::Mouse);
                if down {
                    self.buttons |= bit;
                } else {
                    self.buttons &= !bit;
                }
                if !pointer_mapped(presenter, window) {
                    self.previous_click = None;
                    presenter.pointer_lost(now_ms)?;
                    return Ok(());
                }
                if bit == 1 {
                    if down {
                        let clicks = self.clicks(now_ms, (x, y));
                        presenter.pointer_down_with_clicks(x, y, now_ms, clicks)?;
                    } else {
                        presenter.pointer_up(x, y, now_ms)?;
                    }
                } else {
                    self.previous_click = None;
                    presenter.pointer_aux(bit, down, x, y, now_ms);
                }
            }
            WindowEvent::MouseWheel { delta, .. } => {
                self.previous_click = None;
                if !pointer_mapped(presenter, window) {
                    return Ok(());
                }
                if let Some((x, y)) = self.pointer {
                    let (dx, dy) = match delta {
                        MouseScrollDelta::LineDelta(dx, dy) => {
                            (-dx * SCROLL_LINE, -dy * SCROLL_LINE)
                        }
                        MouseScrollDelta::PixelDelta(position) => {
                            let position = position.to_logical::<f32>(window.scale_factor());
                            (-position.x, -position.y)
                        }
                    };
                    presenter.wheel_at(x, y, dx, dy);
                }
            }
            WindowEvent::ModifiersChanged(modifiers) => {
                sync_modifiers(presenter, modifiers.state())
            }
            WindowEvent::KeyboardInput {
                event,
                is_synthetic,
                ..
            } => {
                self.previous_click = None;
                let (code, key) = key_names(event.physical_key, &event.logical_key);
                let down = event.state == ElementState::Pressed;
                if is_synthetic && down || self.preedit == Preedit::Active {
                    presenter.hold_modifier(&code, down);
                } else {
                    presenter.ime_end();
                    self.preedit = Preedit::Idle;
                    if down && code == "Escape" {
                        presenter.pointer_abandoned(now_ms)?;
                    }
                    press(
                        presenter,
                        &code,
                        &key,
                        event.text.as_deref(),
                        down,
                        event.repeat,
                    );
                }
            }
            WindowEvent::Ime(event) => self.ime(presenter, event),
            _ => {}
        }
        self.update_ime(presenter, window);
        Ok(())
    }

    pub(super) fn update_ime<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
    ) {
        let target = presenter.ime_target().filter(|_| self.focused);
        if target != self.allowed {
            presenter.ime_end();
            self.session = None;
            self.preedit = Preedit::Idle;
            self.ime_area = None;
            if self.allowed.is_some() {
                window.set_ime_allowed(false);
            }
            self.allowed = target;
            if target.is_some() {
                window.set_ime_allowed(true);
            }
        }
        let area = presenter.ime_cursor_area();
        if area != self.ime_area {
            self.ime_area = area;
            if let Some((x, y, width, height)) = area {
                window.set_ime_cursor_area(
                    LogicalPosition::new(x, y),
                    LogicalSize::new(width, height),
                );
            }
        }
    }

    fn ime<D: DataSource>(&mut self, presenter: &mut Presenter<D>, event: Ime) {
        match event {
            // Wayland enables text input for a focused field before composition starts.
            Ime::Enabled => self.session = self.allowed,
            Ime::Preedit(text, _) if text.is_empty() => {
                // Winit also clears preedit immediately before a Commit in the
                // same batch. Keep the original selection until that result arrives.
                if self.preedit == Preedit::Active {
                    self.preedit = Preedit::Clearing;
                }
            }
            Ime::Preedit(text, cursor) => {
                if self
                    .session
                    .is_some_and(|target| presenter.ime_begin(target))
                {
                    self.preedit = Preedit::Active;
                    presenter.ime_preedit(&text, cursor);
                }
            }
            Ime::Commit(text) => {
                if self
                    .session
                    .is_some_and(|target| presenter.ime_begin(target))
                {
                    presenter.ime_commit(&text);
                }
                self.preedit = Preedit::Idle;
            }
            Ime::Disabled => {
                presenter.ime_end();
                self.session = None;
                self.preedit = Preedit::Idle;
            }
        }
    }

    pub(super) fn finish_ime_events<D: DataSource>(&mut self, presenter: &mut Presenter<D>) {
        if self.preedit == Preedit::Clearing {
            presenter.ime_end();
            self.preedit = Preedit::Idle;
        }
    }

    fn clicks(&mut self, now_ms: f64, point: (f32, f32)) -> u32 {
        // Match the standalone Linux input's GTK default interval and distance.
        let clicks = self
            .previous_click
            .filter(|(then, origin, _)| {
                now_ms - then <= 400.
                    && (point.0 - origin.0).abs() <= 5.
                    && (point.1 - origin.1).abs() <= 5.
            })
            .map_or(1, |(_, _, clicks)| clicks % 3 + 1);
        self.previous_click = Some((now_ms, point, clicks));
        clicks
    }
}

fn pointer_mapped<D: DataSource>(presenter: &Presenter<D>, window: &Window) -> bool {
    let scale = window.scale_factor() as f32;
    let size = window.inner_size();
    presenter.display_input_mapping(
        (size.width as f32 / scale, size.height as f32 / scale),
        scale,
    )
}

fn key_names(physical: PhysicalKey, logical: &Key) -> (String, String) {
    let code = match physical {
        PhysicalKey::Code(code) => format!("{code:?}").replace("Super", "Meta"),
        PhysicalKey::Unidentified(_) => "Unidentified".into(),
    };
    let key = match logical {
        Key::Character(text) => text.to_string(),
        Key::Named(key) => format!("{key:?}"),
        Key::Dead(_) => "Dead".into(),
        Key::Unidentified(_) => "Unidentified".into(),
    };
    (code, key)
}

fn press<D: DataSource>(
    presenter: &mut Presenter<D>,
    code: &str,
    key: &str,
    text: Option<&str>,
    down: bool,
    repeat: bool,
) {
    let typed = text.filter(|text| down && !text.chars().any(char::is_control));
    match typed.filter(|text| *text != key) {
        Some(text) => presenter.hardware_key_typing(code, key, text, repeat),
        None => presenter.hardware_key(code, key, down, repeat),
    }
}

fn sync_modifiers<D: DataSource>(presenter: &mut Presenter<D>, modifiers: ModifiersState) {
    for (left, right, held) in [
        ("ShiftLeft", "ShiftRight", modifiers.shift_key()),
        ("ControlLeft", "ControlRight", modifiers.control_key()),
        ("AltLeft", "AltRight", modifiers.alt_key()),
        ("MetaLeft", "MetaRight", modifiers.super_key()),
    ] {
        presenter.hold_modifier(left, held);
        presenter.hold_modifier(right, false);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exact_painted::presenter::PainterChoice;
    use winit::keyboard::{KeyCode, NamedKey};

    fn editor() -> Presenter<()> {
        let plan = contract::compile(
            r#"component App
  state draft = ""
  state submitted = ""
  action edit(value: string)
    draft = value
  action submit
    submitted = draft
  view
    column
      input value=draft input=edit submit=submit testId="field" width=240 height=40
      text submitted testId="submitted"
"#,
        )
        .unwrap();
        let (mut presenter, warning) = Presenter::boot_with(
            &plan.encode(),
            (),
            (320., 160.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(warning.is_none(), "{warning:?}");
        presenter.frame();
        let kernel = presenter.host().kernel();
        let field = kernel
            .node_by_key(kernel.find_by_test_id("field")[0])
            .unwrap()
            .id;
        presenter.tap(field).unwrap();
        presenter
    }

    fn view(presenter: &Presenter<()>, name: &str) -> u32 {
        let kernel = presenter.host().kernel();
        kernel
            .node_by_key(kernel.find_by_test_id(name)[0])
            .unwrap()
            .id
    }

    fn value(presenter: &mut Presenter<()>) -> String {
        let field = view(presenter, "field");
        presenter
            .access_tree()
            .0
            .into_iter()
            .find(|node| node.view == field)
            .unwrap()
            .value
    }

    #[test]
    fn device_keys_keep_the_logical_character_and_translated_text() {
        let (code, key) = key_names(
            PhysicalKey::Code(KeyCode::KeyQ),
            &Key::Character("a".into()),
        );
        assert_eq!((code.as_str(), key.as_str()), ("KeyQ", "a"));
        let mut presenter = editor();
        press(&mut presenter, &code, &key, Some("a"), true, false);
        press(&mut presenter, &code, &key, None, false, false);
        press(&mut presenter, "Quote", "'", Some("é"), true, false);
        press(&mut presenter, "Quote", "'", None, false, false);
        assert_eq!(value(&mut presenter), "aé");
        assert_eq!(
            key_names(
                PhysicalKey::Code(KeyCode::Enter),
                &Key::Named(NamedKey::Enter)
            ),
            ("Enter".into(), "Enter".into())
        );
    }

    #[test]
    fn empty_wayland_preedit_leaves_the_insertion_caret_visible() {
        let mut presenter = editor();
        let mut input = Input {
            allowed: presenter.ime_target(),
            ..Default::default()
        };
        input.ime(&mut presenter, Ime::Enabled);
        let focused = presenter.frame();
        assert!(presenter.caret_frame().is_some());

        input.ime(&mut presenter, Ime::Preedit(String::new(), None));
        input.finish_ime_events(&mut presenter);
        let cleared = presenter.frame();
        assert!(
            focused.data() == cleared.data(),
            "an empty Wayland update must preserve the focused caret pixels"
        );
        assert!(presenter.caret_frame().is_some());

        input.ime(&mut presenter, Ime::Preedit("ni".into(), None));
        presenter.frame();
        assert!(presenter.caret_frame().is_none());
        input.ime(&mut presenter, Ime::Preedit(String::new(), None));
        input.finish_ime_events(&mut presenter);
        assert_eq!(value(&mut presenter), "");
        let cancelled = presenter.frame();
        assert!(
            focused.data() == cancelled.data(),
            "clearing an unfinished composition must restore the focused caret"
        );
    }

    #[test]
    fn enabling_wayland_ime_keeps_enter_available_and_composition_commits_once() {
        let mut presenter = editor();
        press(&mut presenter, "KeyX", "x", Some("x"), true, false);
        press(&mut presenter, "KeyX", "x", None, false, false);
        presenter.frame();
        presenter.hold_modifier("ShiftLeft", true);
        press(&mut presenter, "Home", "Home", None, true, false);
        press(&mut presenter, "Home", "Home", None, false, false);
        presenter.hold_modifier("ShiftLeft", false);
        let mut input = Input {
            allowed: presenter.ime_target(),
            ..Default::default()
        };
        input.ime(&mut presenter, Ime::Enabled);
        assert!(input.preedit == Preedit::Idle);
        input.ime(&mut presenter, Ime::Preedit("ni".into(), Some((2, 2))));
        assert!(input.preedit == Preedit::Active);
        assert_eq!(value(&mut presenter), "ni");
        input.ime(&mut presenter, Ime::Preedit(String::new(), None));
        input.ime(&mut presenter, Ime::Commit("你好".into()));
        input.ime(&mut presenter, Ime::Preedit(String::new(), None));
        input.finish_ime_events(&mut presenter);
        assert!(input.preedit == Preedit::Idle);
        assert_eq!(value(&mut presenter), "你好");
        presenter.frame();
        assert!(presenter.caret_frame().is_some());
        press(&mut presenter, "Enter", "Enter", Some("\r"), true, false);
        press(&mut presenter, "Enter", "Enter", None, false, false);
        let submitted = view(&presenter, "submitted");
        assert!(presenter
            .access_tree()
            .0
            .iter()
            .any(|node| node.view == submitted && node.name == "你好"));
    }
}
