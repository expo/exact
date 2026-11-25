//! Desktop input follows the painted presenter's pointer, keyboard and composition paths.
use super::drop::{Pointed, Press};
use exact_painted::{presenter::ime::Target, presenter::PointerKind, Presenter};
use exact_runner::DataSource;
use std::cell::Cell;
use std::collections::HashMap;
use std::rc::Rc;
use winit::{
    dpi::{LogicalPosition, LogicalSize},
    event::{ElementState, Ime, MouseButton, MouseScrollDelta, WindowEvent},
    keyboard::{Key, ModifiersState, PhysicalKey},
    window::{CursorIcon, Window},
};

pub(super) const SCROLL_LINE: f32 = 48.;
/// The left button's Linux code (`BTN_LEFT`).
const LEFT: u32 = 0x110;

#[derive(Default)]
pub(super) struct Input {
    pointer: Option<(f32, f32)>,
    buttons: u32,
    /// Whether the drag listener hands the window its whole pointer, each
    /// event with its seat and a button with its serial, which winit's lack,
    /// so winit's pointer is not heard (Wayland; LLP 1094.000 D13).
    listened: bool,
    /// The seat whose pointer the window follows: the last heard, and while
    /// it holds a button the window heard pressed, the only one heard.
    seat: Option<u32>,
    /// Each seat's pointer as the drag listener tells it, heard or not.
    seats: HashMap<u32, SeatPointer>,
    /// The left button's press while it is held.
    left: Option<Press>,
    /// The left press while it alone is held, which a lift names.
    held: Rc<Cell<Option<Press>>>,
    /// Whether the pointer is over the window.
    inside: bool,
    /// Whether hover and the cursor still owe the seat the window turned to
    /// a move to its point.
    rehover: bool,
    previous_click: Option<(f64, (f32, f32), u32)>,
    focused: bool,
    occluded: bool,
    allowed: Option<Target>,
    session: Option<Target>,
    preedit: Preedit,
    ime_area: Option<(f32, f32, f32, f32)>,
}

/// A seat's pointer over the window: where it is, the buttons it holds by
/// Linux code, the serial of the press that began its grab, and the
/// buttons whose press the window heard.
#[derive(Default)]
struct SeatPointer {
    point: Option<(f32, f32)>,
    held: Vec<u32>,
    grab: Option<u32>,
    heard: u32,
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

    /// Take the pointer from the drag listener, keeping the left press
    /// while it alone is held in `held`.
    pub(super) fn listen(&mut self, held: Rc<Cell<Option<Press>>>) {
        self.listened = true;
        self.held = held;
    }

    /// The drag listener ended: the window takes winit's pointer, keeping
    /// where it is, and the contact the listener's was holding is lost.
    pub(super) fn unlisten<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        now_ms: f64,
    ) -> Result<(), String> {
        self.listened = false;
        self.seats.clear();
        self.release_all();
        presenter.pointer_lost(now_ms)
    }

    /// What seat `seat`'s pointer did on the window's surface, as the drag
    /// listener heard it, with the modifiers the seat's keyboard held then.
    pub(super) fn pointed<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        seat: u32,
        pointed: Pointed,
        modifiers: Option<u32>,
        now_ms: f64,
    ) -> Result<(), String> {
        let Some(turned) = self.admit(seat, &pointed) else {
            return Ok(());
        };
        let modifiers = pointer_modifiers(modifiers);
        // The window turned to this seat: hover and the cursor go to its point
        // before its next button or wheel, once the presenter can place it
        self.rehover |= turned;
        let result = presenter.with_pointer_modifiers(modifiers, |presenter| {
            let pressed = matches!(pointed, Pointed::Button { .. } | Pointed::Wheel { .. });
            if self.rehover && pressed && pointer_mapped(presenter, window) {
                self.rehover = false;
                if let Some(point) = self.pointer {
                    if let Err(error) = self.moved(presenter, window, point, now_ms) {
                        eprintln!("exact: {error}");
                    }
                }
            }
            self.deliver(presenter, window, pointed, now_ms)
        });
        self.sync_held();
        result
    }

    fn deliver<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        pointed: Pointed,
        now_ms: f64,
    ) -> Result<(), String> {
        match pointed {
            Pointed::Entered(point) | Pointed::Moved(point) => {
                self.rehover &= !pointer_mapped(presenter, window);
                self.moved(presenter, window, point, now_ms)
            }
            Pointed::Left => self.left(presenter, window, now_ms),
            Pointed::Button {
                button, pressed, ..
            } => self.button(presenter, window, bit_of(button), pressed, now_ms),
            Pointed::Wheel { lines, x, y } => {
                let line = if lines { SCROLL_LINE } else { 1. };
                self.wheel(presenter, window, x * line, y * line);
                Ok(())
            }
            Pointed::Lost => presenter.pointer_lost(now_ms),
        }
    }

    /// Keep what seat `seat`'s pointer did, and whether the presenter hears
    /// it (none), and if so whether the window turned to that seat from
    /// another. Each seat's point and held buttons are kept; while a seat
    /// holds a button the window heard pressed, only that seat is heard, and
    /// a release whose press the window did not hear is not. A heard event
    /// is at its own seat's point.
    fn admit(&mut self, seat: u32, pointed: &Pointed) -> Option<bool> {
        let state = self.seats.entry(seat).or_default();
        match pointed {
            Pointed::Entered(point) | Pointed::Moved(point) => state.point = Some(*point),
            Pointed::Left => state.point = None,
            Pointed::Button {
                serial,
                button,
                pressed,
            } => {
                state.held.retain(|b| b != button);
                // The grab is the first press's, as the compositor's
                if *pressed {
                    if state.held.is_empty() {
                        state.grab = Some(*serial);
                    }
                    state.held.push(*button);
                } else if state.held.is_empty() {
                    state.grab = None;
                }
            }
            Pointed::Wheel { .. } | Pointed::Lost => {}
        }
        let point = state.point;
        let followed = self.seat == Some(seat);
        if self.buttons != 0 && !followed {
            if matches!(pointed, Pointed::Lost) {
                self.seats.remove(&seat);
            }
            self.sync_held();
            return None;
        }
        match *pointed {
            Pointed::Entered(_) | Pointed::Moved(_) => self.inside = true,
            Pointed::Left => {
                self.inside &= !followed;
                return followed.then_some(false);
            }
            Pointed::Button {
                serial,
                button,
                pressed,
            } => {
                let bit = bit_of(button);
                let heard = &mut self.seats.entry(seat).or_default().heard;
                if bit == 0 || !pressed && *heard & bit == 0 {
                    self.sync_held();
                    return None;
                }
                if pressed {
                    *heard |= bit;
                } else {
                    *heard &= !bit;
                }
                if bit == 1 {
                    self.left = pressed.then_some(Press { seat, serial });
                }
            }
            Pointed::Wheel { .. } => {}
            Pointed::Lost => {
                self.seats.remove(&seat);
                if !followed {
                    return None;
                }
                self.pointer = None;
                self.inside = false;
                self.previous_click = None;
                self.release_all();
                return Some(false);
            }
        }
        // Another seat's pointer is the window's from here: its place, and a
        // click history of its own
        if !followed {
            self.seat = Some(seat);
            self.pointer = point;
            self.inside = point.is_some();
            self.previous_click = None;
        }
        Some(!followed)
    }

    /// Keep the left press a lift may name: the followed seat's, while the
    /// left button is the one button it holds and its press began the grab.
    fn sync_held(&self) {
        let grab = |press: &Press| {
            self.seats
                .get(&press.seat)
                .is_some_and(|s| s.held == [LEFT] && s.grab == Some(press.serial))
        };
        let held = self
            .left
            .filter(|press| self.seat == Some(press.seat) && grab(press));
        self.held.set(held);
    }

    /// Let go of every button the window heard.
    fn release_all(&mut self) {
        self.buttons = 0;
        self.left = None;
        self.held.set(None);
        for state in self.seats.values_mut() {
            state.heard = 0;
        }
    }

    /// The drag out of the window that `press` lifted ended, and the
    /// compositor kept its release: the window and the presenter let go of
    /// that press, and of the pointer if it left meanwhile. A newer press
    /// stays held.
    pub(super) fn ended<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        press: Press,
        now_ms: f64,
    ) -> Result<(), String> {
        if self.left != Some(press) {
            return Ok(());
        }
        self.left = None;
        self.buttons &= !1;
        if let Some(state) = self.seats.get_mut(&press.seat) {
            state.held.retain(|&b| b != LEFT);
            state.heard &= !1;
            if state.held.is_empty() {
                state.grab = None;
            }
        }
        self.sync_held();
        presenter.pointer_released();
        if !self.inside && self.buttons == 0 {
            self.pointer = None;
            presenter.pointer_lost(now_ms)?;
        }
        Ok(())
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
                    self.release_all();
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
            // The drag listener hands the window its whole pointer, in its seats' order
            WindowEvent::CursorMoved { .. }
            | WindowEvent::CursorEntered { .. }
            | WindowEvent::CursorLeft { .. }
            | WindowEvent::MouseInput { .. }
            | WindowEvent::MouseWheel { .. }
                if self.listened => {}
            WindowEvent::CursorMoved { position, .. } => {
                let position = position.to_logical::<f32>(window.scale_factor());
                self.moved(presenter, window, (position.x, position.y), now_ms)?;
            }
            WindowEvent::CursorEntered { .. } => self.inside = true,
            WindowEvent::CursorLeft { .. } => self.left(presenter, window, now_ms)?,
            WindowEvent::MouseInput { state, button, .. } => {
                let bit = match button {
                    MouseButton::Left => 1,
                    MouseButton::Right => 2,
                    MouseButton::Middle => 4,
                    MouseButton::Back => 8,
                    MouseButton::Forward => 16,
                    _ => return Ok(()),
                };
                self.button(
                    presenter,
                    window,
                    bit,
                    state == ElementState::Pressed,
                    now_ms,
                )?;
            }
            WindowEvent::MouseWheel { delta, .. } => {
                let (dx, dy) = match delta {
                    MouseScrollDelta::LineDelta(dx, dy) => (-dx * SCROLL_LINE, -dy * SCROLL_LINE),
                    MouseScrollDelta::PixelDelta(position) => {
                        let position = position.to_logical::<f32>(window.scale_factor());
                        (-position.x, -position.y)
                    }
                };
                self.wheel(presenter, window, dx, dy);
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

    /// The pointer moved to `point`, in points.
    fn moved<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        point: (f32, f32),
        now_ms: f64,
    ) -> Result<(), String> {
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
        // The cursor follows the point even when a handler of the move fails
        let moved = presenter.pointer_move(point.0, point.1, now_ms);
        let cursor = presenter
            .cursor_at(point.0, point.1)
            .name()
            .parse::<CursorIcon>()
            .unwrap_or(CursorIcon::Default);
        window.set_cursor(cursor);
        moved.map(|_| ())
    }

    /// The pointer left the window; a held button keeps it.
    fn left<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        now_ms: f64,
    ) -> Result<(), String> {
        self.inside = false;
        // The cursor resets even when a handler of the loss fails
        window.set_cursor(CursorIcon::Default);
        if self.buttons == 0 {
            self.pointer = None;
            presenter.pointer_lost(now_ms)?;
        }
        Ok(())
    }

    /// The wheel turned by (`dx`, `dy`) points, down and right positive.
    fn wheel<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        dx: f32,
        dy: f32,
    ) {
        self.previous_click = None;
        if !pointer_mapped(presenter, window) {
            return;
        }
        if let Some((x, y)) = self.pointer {
            presenter.wheel_at(x, y, dx, dy);
        }
    }

    /// Button `bit` went down or up at the pointer.
    fn button<D: DataSource>(
        &mut self,
        presenter: &mut Presenter<D>,
        window: &Window,
        bit: u32,
        down: bool,
        now_ms: f64,
    ) -> Result<(), String> {
        let Some((x, y)) = self.pointer else {
            return Ok(());
        };
        presenter.set_pointer_kind(PointerKind::Mouse);
        if down {
            self.buttons |= bit;
        } else {
            self.buttons &= !bit;
        }
        self.sync_held();
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
        Ok(())
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

/// The presenter's bit for a button by its Linux code, `BTN_LEFT` to
/// `BTN_EXTRA` as winit names them; 0 for another.
fn bit_of(code: u32) -> u32 {
    match code {
        LEFT => 1,
        0x111 => 2,
        0x112 => 4,
        0x113 => 8,
        0x114 => 16,
        _ => 0,
    }
}

/// The modifiers a pointer event is dispatched holding: its seat's, none for
/// a seat without a keyboard.
fn pointer_modifiers(mask: Option<u32>) -> exact_runner::KeyModifiers {
    key_modifiers(mask.unwrap_or(0))
}

/// The modifiers an effective xkb mask holds, by its real modifiers' fixed
/// indices, as winit reads them: Shift 0, Control 2, Mod1 (Alt) 3 and Mod4
/// (Super, the meta key) 6.
fn key_modifiers(mask: u32) -> exact_runner::KeyModifiers {
    exact_runner::KeyModifiers {
        shift: mask & 1 != 0,
        ctrl: mask & 1 << 2 != 0,
        alt: mask & 1 << 3 != 0,
        meta: mask & 1 << 6 != 0,
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

    fn button(serial: u32, button: u32, pressed: bool) -> Pointed {
        Pointed::Button {
            serial,
            button,
            pressed,
        }
    }

    /// Hear `pointed` from `seat` as the window would, its buttons'
    /// delivery standing in for the presenter's: whether it was heard.
    fn hear(input: &mut Input, seat: u32, pointed: Pointed) -> bool {
        let delivered = match pointed {
            Pointed::Button {
                button, pressed, ..
            } => Some((bit_of(button), pressed)),
            _ => None,
        };
        let heard = input.admit(seat, &pointed).is_some();
        if let Some((bit, pressed)) = delivered.filter(|_| heard) {
            if pressed {
                input.buttons |= bit;
            } else {
                input.buttons &= !bit;
            }
        }
        input.sync_held();
        heard
    }

    #[test]
    fn each_seat_presses_at_its_own_point_and_a_seat_holding_a_button_alone_is_heard() {
        let mut input = Input::default();
        input.listen(Rc::default());
        assert!(hear(&mut input, 1, Pointed::Entered((10., 10.))));
        assert!(hear(&mut input, 2, Pointed::Moved((100., 100.))));
        assert!(hear(&mut input, 1, button(5, LEFT, true)));
        assert_eq!(
            input.pointer,
            Some((10., 10.)),
            "the press is at its seat's point"
        );
        assert!(
            !hear(&mut input, 2, Pointed::Moved((200., 200.))),
            "the other seat waits"
        );
        assert!(!hear(&mut input, 2, button(9, LEFT, true)));
        assert!(!hear(
            &mut input,
            2,
            Pointed::Wheel {
                lines: true,
                x: 0.,
                y: 1.
            }
        ));
        assert!(hear(&mut input, 1, button(6, LEFT, false)));
        assert!(
            !hear(&mut input, 2, button(10, LEFT, false)),
            "its press was not heard"
        );
        input.previous_click = Some((0., (200., 200.), 1));
        assert_eq!(
            input.admit(2, &button(11, LEFT, true)),
            Some(true),
            "the window turns to it"
        );
        assert_eq!(
            input.pointer,
            Some((200., 200.)),
            "where that seat moved meanwhile"
        );
        assert!(input.inside, "and over the window, as that seat is");
        assert_eq!(
            input.previous_click, None,
            "with no click of the other seat's to double"
        );
    }

    #[test]
    fn a_seat_turned_to_brings_its_own_place_from_outside_the_window() {
        let mut input = Input::default();
        input.listen(Rc::default());
        hear(&mut input, 2, Pointed::Entered((50., 60.)));
        hear(&mut input, 1, Pointed::Entered((10., 10.)));
        hear(&mut input, 1, Pointed::Left);
        assert!(!input.inside, "the followed seat left");
        assert_eq!(input.admit(2, &button(3, LEFT, true)), Some(true));
        assert_eq!(input.pointer, Some((50., 60.)));
        assert!(input.inside, "the seat turned to is over the window");
    }

    #[test]
    fn a_lift_names_the_first_left_press_while_it_is_the_seats_one_button() {
        let held = Rc::new(Cell::new(None));
        let mut input = Input::default();
        input.listen(held.clone());
        hear(&mut input, 1, Pointed::Entered((10., 10.)));
        hear(&mut input, 1, button(5, LEFT, true));
        let press = Some(Press { seat: 1, serial: 5 });
        assert_eq!(held.get(), press);
        hear(&mut input, 1, button(6, 0x111, true));
        assert_eq!(held.get(), None, "a chord lifts nothing");
        hear(&mut input, 1, button(7, 0x111, false));
        assert_eq!(held.get(), press, "the left press is still the grab");
        assert!(
            !hear(&mut input, 1, button(8, 0x118, true)),
            "a button the presenter has no bit for"
        );
        assert_eq!(held.get(), None, "still counts");
        hear(&mut input, 1, button(9, 0x118, false));
        assert_eq!(held.get(), press);
    }

    #[test]
    fn an_effective_mask_holds_the_modifiers_winit_reads() {
        let held = |shift, ctrl, alt, meta| exact_runner::KeyModifiers {
            shift,
            ctrl,
            alt,
            meta,
        };
        assert_eq!(key_modifiers(0b101), held(true, true, false, false));
        assert_eq!(
            key_modifiers(1 << 3 | 1 << 6),
            held(false, false, true, true)
        );
        assert_eq!(
            key_modifiers(1 << 1 | 1 << 4),
            held(false, false, false, false),
            "Lock and Mod2 are none of them"
        );
    }

    #[test]
    fn a_pointer_event_holds_its_own_modifiers_and_the_keyboard_keeps_its_own() {
        let mut presenter = editor();
        presenter.hold_modifier("ControlRight", true);
        let shift = pointer_modifiers(Some(1));
        let during = presenter.with_pointer_modifiers(shift, |p| p.modifiers());
        assert!(
            during.shift && !during.ctrl,
            "the pointer event's seat held Shift alone"
        );
        let keyless = presenter.with_pointer_modifiers(pointer_modifiers(None), |p| p.modifiers());
        assert_eq!(
            keyless,
            exact_runner::KeyModifiers::default(),
            "a seat with no keyboard holds none"
        );
        let after = presenter.modifiers();
        assert!(
            after.ctrl && !after.shift,
            "the keyboard's right Control is back"
        );
        presenter.hold_modifier("ControlRight", false);
        assert!(!presenter.modifiers().ctrl, "and its release clears it");
    }

    #[test]
    fn a_chord_begun_by_another_button_names_no_left_press() {
        let held = Rc::new(Cell::new(None));
        let mut input = Input::default();
        input.listen(held.clone());
        hear(&mut input, 1, Pointed::Entered((10., 10.)));
        hear(&mut input, 1, button(5, 0x111, true));
        hear(&mut input, 1, button(6, LEFT, true));
        hear(&mut input, 1, button(7, 0x111, false));
        assert_eq!(held.get(), None, "the right press began the grab");
        hear(&mut input, 1, button(8, LEFT, false));
        hear(&mut input, 1, button(9, LEFT, true));
        assert_eq!(held.get(), Some(Press { seat: 1, serial: 9 }));
    }
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
