//! Input: evdev, read directly — pointer motion, buttons, wheels, keys,
//! gamepads — with a US keymap for typing. No libinput (nothing to link), so no
//! pointer acceleration, touchpad gestures, or hotplug: the trades LLP 1015
//! §7 declares. A touchpad's finger moves the pointer by its travel.
//!
//! @ref LLP 1015 §6

#![allow(unsafe_code)]

use evdev::{
    AbsoluteAxisCode, Device, EventSummary, EventType, KeyCode, PropType, RelativeAxisCode,
    SynchronizationCode,
};
use std::os::fd::AsRawFd;

mod pad;
pub use crate::gamepad::Reading as PadReading;
pub use pad::MAX_PADS;

/// One thing the user did.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum InputEvent {
    /// The pointer moved by (dx, dy) device pixels.
    Motion(f32, f32),
    /// The pointer is at a fraction of the screen (an absolute device — a
    /// tablet, a KVM's mouse): `None` on an axis that did not move.
    Absolute(Option<f32>, Option<f32>),
    /// The primary button went down (`true`) or up.
    Button(bool),
    /// The secondary (2), middle (4), back (8) or forward (16) button, by
    /// its `PointerEvent.buttons` bit, went down (`true`) or up. Back and
    /// Forward are the last two's default action on their release (LLP
    /// 1115.000 D5.5).
    Aux(u32, bool),
    /// Carrier/device loss cancels without manufacturing a successful release.
    Cancel,
    /// A wheel: (dx, dy) in points, the web's sign (a positive `dy` scrolls
    /// down).
    Wheel(f32, f32),
    /// A gamepad's whole state in the W3C Gamepad API's standard mapping,
    /// when it changed; `connected: false` when the pad went away.
    Pad {
        /// The pad's index, in the order the pads were opened.
        index: u8,
        /// False once the pad is gone.
        connected: bool,
        /// Its buttons and axes.
        reading: PadReading,
        /// When the device reported it, in the host's clock (milliseconds).
        at: f64,
    },
    /// A keyboard edge. Translate US codes at dispatch, keeping the queue compact.
    Key {
        /// Physical evdev key code, also used by VNC's US mapping.
        code: u16,
        /// Whether the logical key uses the shifted US character.
        shift: bool,
        /// Press (`true`) or release.
        down: bool,
        /// A repeated press while the key is held.
        repeat: bool,
    },
}

/// US keyboard state shared by evdev and VNC. Shift sides are independent.
/// A Ctrl/Meta chord passes: it is a `key` handler's (⌘S, kanban F27), and
/// the presenter keeps it from typing or starting a game action
/// (`hardware_key`).
#[derive(Default)]
pub(crate) struct Keyboard {
    /// The Shift keys held, a bit per side.
    shifts: u8,
}
impl Keyboard {
    pub(crate) fn event(
        &mut self,
        code: u16,
        value: i32,
        shift: Option<bool>,
    ) -> Option<InputEvent> {
        if !(0..=2).contains(&value) {
            return None;
        }
        let down = value != 0;
        let bit = match code {
            42 => 1,
            54 => 2,
            _ => 0,
        };
        if down {
            self.shifts |= bit;
        } else {
            self.shifts &= !bit;
        }
        let shift = shift.unwrap_or(self.shifts != 0);
        key(code, shift)?;
        Some(InputEvent::Key {
            code,
            shift,
            down,
            repeat: value == 2,
        })
    }
}

/// Points per wheel notch — the browser's tick.
pub const LINE: f32 = 40.0;

/// An absolute device's (min, max) per axis, x then y.
type Ranges = [(i32, i32); 2];

/// How an absolute device's ABS_X and ABS_Y move the pointer.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Absolute {
    /// To the same fraction of the screen: a touch screen, a drawing
    /// tablet, a KVM's mouse.
    Screen(Ranges),
    /// By the finger's travel: a touchpad.
    Pad(Glide),
}

/// Device pixels a touchpad finger's millimetre of travel moves the
/// pointer: libinput's flat profile, which counts a touchpad's travel at
/// 1000 dpi and slows it by 0.2968 (`TP_MAGIC_SLOWDOWN`).
const PAD_PIXELS_PER_MM: f32 = 1000.0 / 25.4 * 0.2968;

/// A touchpad read as relative motion. The kernel reports the touch's
/// place on the pad (`ABS_X`, `ABS_Y`) and sends an axis only when it
/// changes, so `at` is always the pad's current place. While a finger
/// touches, each report moves the pointer by its change; in a report where
/// a finger lands or the fingers change, the place moves and the pointer
/// does not, since the kernel's single point then jumps to the new touch.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Glide {
    /// Device pixels per pad unit, x then y.
    scale: [f32; 2],
    at: [i32; 2],
    touching: bool,
    landing: bool,
}

impl Glide {
    /// A pad with these axes (`input_absinfo`): its place, its range, and
    /// its resolution in units per millimetre. A driver that reports no
    /// resolution (0) is taken to have a pad 100 mm wide.
    fn new(axes: [(i32, (i32, i32), i32); 2]) -> Glide {
        let width = (axes[0].1 .1 - axes[0].1 .0).max(1) as f32;
        let per_mm = |(_, _, resolution): (i32, (i32, i32), i32)| {
            if resolution > 0 {
                resolution as f32
            } else {
                width / 100.0
            }
        };
        Glide {
            scale: axes.map(|a| PAD_PIXELS_PER_MM / per_mm(a)),
            at: axes.map(|a| a.0),
            touching: false,
            landing: false,
        }
    }

    /// The pointer's move, in device pixels, for the place `value` on
    /// `axis` (0 for x, 1 for y).
    fn axis(&mut self, axis: usize, value: i32) -> f32 {
        let moved = (value - self.at[axis]) as f32 * self.scale[axis];
        self.at[axis] = value;
        if self.touching && !self.landing {
            moved
        } else {
            0.0
        }
    }

    /// `BTN_TOUCH`: a finger came down (`true`) or the last one lifted.
    fn touch(&mut self, down: bool) {
        self.touching = down;
        self.landing = true;
    }

    /// A `BTN_TOOL_*` finger count changed, or events were dropped: the
    /// next place is a new start.
    fn restart(&mut self) {
        self.landing = true;
    }

    /// `SYN_REPORT`: the report is whole.
    fn report(&mut self) {
        self.landing = false;
    }
}

/// A pad's identity and motors, as the registry takes them.
type Registering = (crate::pads::Identity, Option<Box<dyn crate::pads::Output>>);

/// Every device that points, types or plays.
pub struct Input {
    devices: Vec<(Device, Option<Absolute>)>,
    // A pad, its index, its state, and whether its connection was reported.
    pads: Vec<(Device, crate::gamepad::Opened)>,
    /// Each pad's identity and motors, for the registry (`crate::pads`)
    /// when it is first read, by index; `None` once it is there.
    to_register: Vec<Option<Registering>>,
    keyboard: Keyboard,
    touch_only: bool,
}

impl Input {
    /// Open every evdev device with relative axes or a keyboard's keys.
    /// Devices that cannot be opened (no permission — the `input` group) are
    /// skipped.
    pub fn open() -> Input {
        let (mut devices, mut pads, mut pointing) = (Vec::new(), Vec::new(), Vec::new());
        let mut to_register = Vec::new();
        for (path, d) in evdev::enumerate() {
            // A pad's sticks are ABS_X/ABS_Y too: it is a pad, not a pointer.
            if pad::is_pad(&d) {
                if pads.len() < MAX_PADS {
                    nonblocking(&d);
                    let state = pad::open(&path, &d);
                    to_register.push(Some((pad::identity(&path, &d), pad::motors(&path, &d))));
                    let index = pads.len() as u8;
                    let announced = false;
                    pads.push((
                        d,
                        crate::gamepad::Opened {
                            index,
                            state,
                            announced,
                        },
                    ));
                }
                continue;
            }
            let pointer = d.supported_events().contains(EventType::RELATIVE);
            let keys = d.supported_keys();
            let has = |code| keys.is_some_and(|k| k.contains(code));
            // A touchpad, as udev's `input_id` tells one: fingers and no
            // pen, on a device not mapped to the screen. The kernel's
            // "Device properties" (docs.kernel.org/input/event-codes.html)
            // set INPUT_PROP_DIRECT on a touch screen or drawing tablet and
            // INPUT_PROP_POINTER on a new touchpad; with neither, the type
            // is deduced from the events, as here.
            let touchpad = has(KeyCode::BTN_TOOL_FINGER)
                && !has(KeyCode::BTN_TOOL_PEN)
                && !d.properties().contains(PropType::DIRECT);
            // An absolute pointer: ABS_X/ABS_Y. Multitouch slots are not
            // tracked; ABS_MT_POSITION_* is the same point as ABS_X/Y on
            // the single-contact screens we open, and a touchpad's
            // ABS_X/Y is its first finger.
            let absolute = d
                .supported_absolute_axes()
                .is_some_and(|a| {
                    a.contains(AbsoluteAxisCode::ABS_X) && a.contains(AbsoluteAxisCode::ABS_Y)
                })
                .then(|| d.get_abs_state().ok())
                .flatten()
                .map(|abs| {
                    let [x, y] = [AbsoluteAxisCode::ABS_X, AbsoluteAxisCode::ABS_Y]
                        .map(|axis| abs[axis.0 as usize]);
                    if touchpad {
                        Absolute::Pad(Glide::new(
                            [x, y].map(|a| (a.value, (a.minimum, a.maximum), a.resolution)),
                        ))
                    } else {
                        Absolute::Screen([(x.minimum, x.maximum), (y.minimum, y.maximum)])
                    }
                });
            let keyboard = has(KeyCode::KEY_A) || has(KeyCode::BTN_LEFT);
            if !(pointer || absolute.is_some() || keyboard) {
                continue;
            }
            let pad = matches!(absolute, Some(Absolute::Pad(_)));
            pointing.push(crate::presenter::Pointing {
                relative: pointer || pad,
                absolute: absolute.is_some() && !pad,
                touch: has(KeyCode::BTN_TOUCH),
                left: has(KeyCode::BTN_LEFT),
            });
            nonblocking(&d);
            devices.push((d, absolute));
        }
        Input {
            devices,
            pads,
            to_register,
            keyboard: Keyboard::default(),
            touch_only: crate::presenter::touch_only(&pointing),
        }
    }

    /// Whether a touch screen is the only pointing device open
    /// ([`crate::presenter::touch_only`]).
    pub fn touch_only(&self) -> bool {
        self.touch_only
    }

    /// How many devices are open.
    pub fn len(&self) -> usize {
        self.devices.len() + self.pads.len()
    }

    /// Whether none opened.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// The descriptors to poll.
    pub fn fds(&self) -> Vec<i32> {
        let pads = self.pads.iter().map(|(d, ..)| d);
        let devices = self.devices.iter().map(|(d, _)| d);
        devices.chain(pads).map(|d| d.as_raw_fd()).collect()
    }

    /// Each pad's changes since the last read (`gamepad::read`), at their
    /// reports' times moved into the host's clock by `now_ms`, its time at
    /// this read.
    fn read_pads(&mut self, now_ms: f64, out: &mut Vec<InputEvent>) {
        use crate::gamepad::{change, Fetch};
        let epoch_ms = |t: std::time::SystemTime| {
            t.duration_since(std::time::UNIX_EPOCH)
                .map_or(0., |d| d.as_secs_f64() * 1000.)
        };
        let now = (epoch_ms(std::time::SystemTime::now()), now_ms);
        let readings = crate::gamepad::read(&mut self.pads, now, |d| match d.fetch_events() {
            Ok(events) => Fetch::Changes(
                events
                    .filter_map(|e| {
                        let at = epoch_ms(e.timestamp());
                        Some((change(e.event_type().0, e.code(), e.value())?, at))
                    })
                    .collect(),
            ),
            Err(e)
                if matches!(
                    e.kind(),
                    std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                ) =>
            {
                Fetch::Nothing
            }
            Err(_) => Fetch::Gone,
        });
        for (index, reading, at) in readings {
            // The registry a package reads has the same pads by index.
            match (reading, self.to_register.get_mut(index as usize)) {
                (Some(r), Some(waiting)) => {
                    if let Some((identity, motors)) = waiting.take() {
                        crate::pads::connect(index, identity, motors);
                    }
                    crate::pads::read(index, r);
                }
                (None, _) => crate::pads::disconnect(index),
                _ => {}
            }
            out.push(InputEvent::Pad {
                index,
                connected: reading.is_some(),
                reading: reading.unwrap_or_default(),
                at,
            });
        }
    }

    /// Everything that arrived since the last read; `now_ms` is the host's
    /// clock at this read.
    pub fn read(&mut self, now_ms: f64) -> Vec<InputEvent> {
        let mut out = Vec::new();
        self.read_pads(now_ms, &mut out);
        let mut gone = Vec::new();
        for (index, (d, absolute)) in self.devices.iter_mut().enumerate() {
            let wheels = Wheels::new(d.supported_relative_axes());
            let events = match d.fetch_events() {
                Ok(events) => events,
                Err(e)
                    if matches!(
                        e.kind(),
                        std::io::ErrorKind::WouldBlock | std::io::ErrorKind::Interrupted
                    ) =>
                {
                    continue
                }
                Err(_) => {
                    gone.push(index);
                    out.push(InputEvent::Cancel);
                    continue;
                }
            };
            let mut pointer = PointerReport::default();
            let fraction = |range: (i32, i32), v: i32| {
                let span = (range.1 - range.0).max(1) as f32;
                ((v - range.0) as f32 / span).clamp(0.0, 1.0)
            };
            for e in events {
                let summary = e.destructure();
                if !matches!(
                    summary,
                    EventSummary::RelativeAxis(
                        _,
                        RelativeAxisCode::REL_X | RelativeAxisCode::REL_Y,
                        _
                    ) | EventSummary::AbsoluteAxis(
                        _,
                        AbsoluteAxisCode::ABS_X
                            | AbsoluteAxisCode::ABS_Y
                            | AbsoluteAxisCode::ABS_MT_POSITION_X
                            | AbsoluteAxisCode::ABS_MT_POSITION_Y,
                        _
                    )
                ) {
                    pointer.flush(&mut out);
                }
                let (screen, glide) = match absolute {
                    Some(Absolute::Screen(r)) => (Some(*r), None),
                    Some(Absolute::Pad(glide)) => (None, Some(glide)),
                    None => (None, None),
                };
                match (summary, glide) {
                    (EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_X, v), Some(g)) => {
                        pointer.relative.0 += g.axis(0, v);
                    }
                    (EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_Y, v), Some(g)) => {
                        pointer.relative.1 += g.axis(1, v);
                    }
                    // A touch on the pad is not a press: its click is BTN_LEFT.
                    (EventSummary::Key(_, KeyCode::BTN_TOUCH, value), Some(g)) => {
                        g.touch(value != 0);
                    }
                    (
                        EventSummary::Key(
                            _,
                            KeyCode::BTN_TOOL_FINGER
                            | KeyCode::BTN_TOOL_DOUBLETAP
                            | KeyCode::BTN_TOOL_TRIPLETAP
                            | KeyCode::BTN_TOOL_QUADTAP
                            | KeyCode::BTN_TOOL_QUINTTAP,
                            _,
                        ),
                        Some(g),
                    ) => g.restart(),
                    (
                        EventSummary::Synchronization(_, SynchronizationCode::SYN_REPORT, _),
                        Some(g),
                    ) => {
                        g.report();
                    }
                    (EventSummary::AbsoluteAxis(..), Some(_)) => {}
                    (
                        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_X, v)
                        | EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_POSITION_X, v),
                        _,
                    ) => {
                        if let Some(r) = screen {
                            pointer.absolute.0 = Some(fraction(r[0], v));
                        }
                    }
                    (
                        EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_Y, v)
                        | EventSummary::AbsoluteAxis(_, AbsoluteAxisCode::ABS_MT_POSITION_Y, v),
                        _,
                    ) => {
                        if let Some(r) = screen {
                            pointer.absolute.1 = Some(fraction(r[1], v));
                        }
                    }
                    (EventSummary::RelativeAxis(_, RelativeAxisCode::REL_X, v), _) => {
                        pointer.relative.0 += v as f32;
                    }
                    (EventSummary::RelativeAxis(_, RelativeAxisCode::REL_Y, v), _) => {
                        pointer.relative.1 += v as f32;
                    }
                    (EventSummary::RelativeAxis(_, code, v), _) => {
                        if let Some(event) = wheels.event(code, v) {
                            out.push(event);
                        }
                    }
                    (EventSummary::Key(_, code, value), _) => {
                        let code = code.0;
                        let down = value != 0;
                        match code {
                            // BTN_LEFT (a mouse) and BTN_TOUCH (a
                            // touchscreen). Both are a primary press.
                            0x110 | 0x14a => out.push(InputEvent::Button(down)),
                            // BTN_RIGHT, BTN_MIDDLE.
                            0x111 => out.push(InputEvent::Aux(2, down)),
                            0x112 => out.push(InputEvent::Aux(4, down)),
                            // BTN_SIDE and BTN_EXTRA, a mouse's back and
                            // forward (thumb) buttons, and BTN_BACK and
                            // BTN_FORWARD, as some mice report them.
                            0x113 | 0x116 => out.push(InputEvent::Aux(8, down)),
                            0x114 | 0x115 => out.push(InputEvent::Aux(16, down)),
                            _ => {
                                if let Some(event) = self.keyboard.event(code, value, None) {
                                    out.push(event);
                                }
                            }
                        }
                    }
                    (EventSummary::Synchronization(_, SynchronizationCode::SYN_DROPPED, _), g) => {
                        if let Some(g) = g {
                            g.restart();
                        }
                        out.push(InputEvent::Cancel)
                    }
                    _ => {}
                }
            }
            pointer.flush(&mut out);
        }
        for index in gone.into_iter().rev() {
            self.devices.remove(index);
        }
        out
    }
}

fn nonblocking(d: &Device) {
    // SAFETY: fcntl on a file descriptor this process owns; the flags are
    // read and written whole.
    unsafe {
        let fd = d.as_raw_fd();
        let flags = libc::fcntl(fd, libc::F_GETFL);
        if flags >= 0 {
            libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK);
        }
    }
}

// Both axes of one evdev report reach recognition together. Separate X then Y
// callbacks could claim a horizontal swipe before the dominant vertical axis.
#[derive(Default)]
struct PointerReport {
    relative: (f32, f32),
    absolute: (Option<f32>, Option<f32>),
}
impl PointerReport {
    fn flush(&mut self, out: &mut Vec<InputEvent>) {
        let (dx, dy) = std::mem::take(&mut self.relative);
        if dx != 0. || dy != 0. {
            out.push(InputEvent::Motion(dx, dy));
        }
        let (x, y) = std::mem::take(&mut self.absolute);
        if x.is_some() || y.is_some() {
            out.push(InputEvent::Absolute(x, y));
        }
    }
}

// High-resolution wheels also report coarse ticks. Choose by the device's
// capabilities on each axis: 120 high-resolution units can be a whole detent.
struct Wheels {
    vertical_hi_res: bool,
    horizontal_hi_res: bool,
}
impl Wheels {
    fn new(axes: Option<&evdev::AttributeSetRef<RelativeAxisCode>>) -> Self {
        Self {
            vertical_hi_res: axes.is_some_and(|a| a.contains(RelativeAxisCode::REL_WHEEL_HI_RES)),
            horizontal_hi_res: axes
                .is_some_and(|a| a.contains(RelativeAxisCode::REL_HWHEEL_HI_RES)),
        }
    }

    fn event(&self, code: RelativeAxisCode, value: i32) -> Option<InputEvent> {
        let v = value as f32;
        match code {
            RelativeAxisCode::REL_WHEEL_HI_RES => Some(InputEvent::Wheel(0., -v / 120. * LINE)),
            RelativeAxisCode::REL_HWHEEL_HI_RES => Some(InputEvent::Wheel(v / 120. * LINE, 0.)),
            RelativeAxisCode::REL_WHEEL if !self.vertical_hi_res => {
                Some(InputEvent::Wheel(0., -v * LINE))
            }
            RelativeAxisCode::REL_HWHEEL if !self.horizontal_hi_res => {
                Some(InputEvent::Wheel(v * LINE, 0.))
            }
            _ => None,
        }
    }
}

/// The US evdev map: physical code, unshifted/shifted logical key.
pub(crate) fn key(code: u16, shift: bool) -> Option<(&'static str, &'static str)> {
    let (code, plain, shifted) = match code {
        1 => ("Escape", "Escape", "Escape"),
        2 => ("Digit1", "1", "!"),
        3 => ("Digit2", "2", "@"),
        4 => ("Digit3", "3", "#"),
        5 => ("Digit4", "4", "$"),
        6 => ("Digit5", "5", "%"),
        7 => ("Digit6", "6", "^"),
        8 => ("Digit7", "7", "&"),
        9 => ("Digit8", "8", "*"),
        10 => ("Digit9", "9", "("),
        11 => ("Digit0", "0", ")"),
        12 => ("Minus", "-", "_"),
        13 => ("Equal", "=", "+"),
        14 => ("Backspace", "Backspace", "Backspace"),
        15 => ("Tab", "Tab", "Tab"),
        16 => ("KeyQ", "q", "Q"),
        17 => ("KeyW", "w", "W"),
        18 => ("KeyE", "e", "E"),
        19 => ("KeyR", "r", "R"),
        20 => ("KeyT", "t", "T"),
        21 => ("KeyY", "y", "Y"),
        22 => ("KeyU", "u", "U"),
        23 => ("KeyI", "i", "I"),
        24 => ("KeyO", "o", "O"),
        25 => ("KeyP", "p", "P"),
        26 => ("BracketLeft", "[", "{"),
        27 => ("BracketRight", "]", "}"),
        28 => ("Enter", "Enter", "Enter"),
        29 => ("ControlLeft", "Control", "Control"),
        30 => ("KeyA", "a", "A"),
        31 => ("KeyS", "s", "S"),
        32 => ("KeyD", "d", "D"),
        33 => ("KeyF", "f", "F"),
        34 => ("KeyG", "g", "G"),
        35 => ("KeyH", "h", "H"),
        36 => ("KeyJ", "j", "J"),
        37 => ("KeyK", "k", "K"),
        38 => ("KeyL", "l", "L"),
        39 => ("Semicolon", ";", ":"),
        40 => ("Quote", "'", "\""),
        41 => ("Backquote", "`", "~"),
        42 => ("ShiftLeft", "Shift", "Shift"),
        43 => ("Backslash", "\\", "|"),
        44 => ("KeyZ", "z", "Z"),
        45 => ("KeyX", "x", "X"),
        46 => ("KeyC", "c", "C"),
        47 => ("KeyV", "v", "V"),
        48 => ("KeyB", "b", "B"),
        49 => ("KeyN", "n", "N"),
        50 => ("KeyM", "m", "M"),
        51 => ("Comma", ",", "<"),
        52 => ("Period", ".", ">"),
        53 => ("Slash", "/", "?"),
        54 => ("ShiftRight", "Shift", "Shift"),
        56 => ("AltLeft", "Alt", "Alt"),
        57 => ("Space", " ", " "),
        96 => ("NumpadEnter", "Enter", "Enter"),
        97 => ("ControlRight", "Control", "Control"),
        100 => ("AltRight", "Alt", "Alt"),
        102 => ("Home", "Home", "Home"),
        103 => ("ArrowUp", "ArrowUp", "ArrowUp"),
        104 => ("PageUp", "PageUp", "PageUp"),
        105 => ("ArrowLeft", "ArrowLeft", "ArrowLeft"),
        106 => ("ArrowRight", "ArrowRight", "ArrowRight"),
        107 => ("End", "End", "End"),
        108 => ("ArrowDown", "ArrowDown", "ArrowDown"),
        109 => ("PageDown", "PageDown", "PageDown"),
        110 => ("Insert", "Insert", "Insert"),
        111 => ("Delete", "Delete", "Delete"),
        // KEY_F1 is 59; F11 and F12 are 87 and 88, not contiguous with F10.
        59 => ("F1", "F1", "F1"),
        60 => ("F2", "F2", "F2"),
        61 => ("F3", "F3", "F3"),
        62 => ("F4", "F4", "F4"),
        63 => ("F5", "F5", "F5"),
        64 => ("F6", "F6", "F6"),
        65 => ("F7", "F7", "F7"),
        66 => ("F8", "F8", "F8"),
        67 => ("F9", "F9", "F9"),
        68 => ("F10", "F10", "F10"),
        87 => ("F11", "F11", "F11"),
        88 => ("F12", "F12", "F12"),
        125 => ("MetaLeft", "Meta", "Meta"),
        126 => ("MetaRight", "Meta", "Meta"),
        // KEY_BACK, a keyboard's own Back key, by the web's name for it.
        158 => ("BrowserBack", "BrowserBack", "BrowserBack"),
        159 => ("BrowserForward", "BrowserForward", "BrowserForward"),
        _ => return None,
    };
    Some((code, if shift { shifted } else { plain }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use RelativeAxisCode as Rel;

    fn wheel_events(axes: &[Rel], events: &[(Rel, i32)]) -> Vec<InputEvent> {
        let axes = axes.iter().collect::<evdev::AttributeSet<_>>();
        let wheels = Wheels::new(Some(&axes));
        events
            .iter()
            .filter_map(|&(code, value)| wheels.event(code, value))
            .collect()
    }

    #[test]
    fn wheel_detent_is_delivered_once() {
        for (ticks, units, points) in [(-1, -120, 40.), (1, 120, -40.), (2, 240, -80.)] {
            assert_eq!(
                wheel_events(
                    &[Rel::REL_WHEEL, Rel::REL_WHEEL_HI_RES],
                    &[(Rel::REL_WHEEL, ticks), (Rel::REL_WHEEL_HI_RES, units)],
                ),
                vec![InputEvent::Wheel(0., points)]
            );
            assert_eq!(
                wheel_events(
                    &[Rel::REL_HWHEEL, Rel::REL_HWHEEL_HI_RES],
                    &[(Rel::REL_HWHEEL_HI_RES, units), (Rel::REL_HWHEEL, ticks)],
                ),
                vec![InputEvent::Wheel(-points, 0.)]
            );
        }
    }

    #[test]
    fn wheel_horizontal_fine_delta_replaces_the_coarse_tick() {
        assert_eq!(
            wheel_events(
                &[Rel::REL_HWHEEL, Rel::REL_HWHEEL_HI_RES],
                &[(Rel::REL_HWHEEL, 1), (Rel::REL_HWHEEL_HI_RES, 15)],
            ),
            vec![InputEvent::Wheel(5., 0.)]
        );
    }

    #[test]
    fn wheel_fine_and_whole_deltas_are_kept_across_reads() {
        let axes = [Rel::REL_WHEEL, Rel::REL_WHEEL_HI_RES];
        assert_eq!(
            wheel_events(
                &axes,
                &[
                    (Rel::REL_WHEEL_HI_RES, -15),
                    (Rel::REL_WHEEL, -1),
                    (Rel::REL_WHEEL_HI_RES, -120),
                ],
            ),
            vec![InputEvent::Wheel(0., 5.), InputEvent::Wheel(0., 40.)]
        );
        assert!(wheel_events(&axes, &[(Rel::REL_WHEEL, -1)]).is_empty());
        assert_eq!(
            wheel_events(&axes, &[(Rel::REL_WHEEL_HI_RES, -15); 8]),
            vec![InputEvent::Wheel(0., 5.); 8]
        );
    }

    #[test]
    fn wheel_capabilities_are_independent_per_axis_and_device() {
        let mut events = wheel_events(
            &[Rel::REL_WHEEL, Rel::REL_WHEEL_HI_RES, Rel::REL_HWHEEL],
            &[
                (Rel::REL_WHEEL, -1),
                (Rel::REL_WHEEL_HI_RES, -15),
                (Rel::REL_HWHEEL, 1),
            ],
        );
        events.extend(wheel_events(
            &[Rel::REL_WHEEL, Rel::REL_HWHEEL, Rel::REL_HWHEEL_HI_RES],
            &[
                (Rel::REL_WHEEL, -1),
                (Rel::REL_HWHEEL, 1),
                (Rel::REL_HWHEEL_HI_RES, 15),
            ],
        ));
        assert_eq!(
            events,
            vec![
                InputEvent::Wheel(0., 5.),
                InputEvent::Wheel(40., 0.),
                InputEvent::Wheel(0., 40.),
                InputEvent::Wheel(5., 0.),
            ]
        );
    }

    #[test]
    fn wheel_coarse_only_devices_keep_both_axes() {
        assert_eq!(
            wheel_events(
                &[Rel::REL_WHEEL, Rel::REL_HWHEEL],
                &[
                    (Rel::REL_WHEEL, -1),
                    (Rel::REL_WHEEL, 2),
                    (Rel::REL_HWHEEL, -2),
                    (Rel::REL_HWHEEL, 1),
                ],
            ),
            vec![
                InputEvent::Wheel(0., 40.),
                InputEvent::Wheel(0., -80.),
                InputEvent::Wheel(-80., 0.),
                InputEvent::Wheel(40., 0.),
            ]
        );
        assert_eq!(Wheels::new(None).event(Rel::REL_X, 10), None);
    }

    #[test]
    fn physical_press_repeat_and_release_keep_code_and_logical_key() {
        let mut keyboard = Keyboard::default();
        keyboard.event(42, 1, None);
        for (value, down, repeat) in [(1, true, false), (2, true, true), (0, false, false)] {
            assert_eq!(
                keyboard.event(17, value, None),
                Some(InputEvent::Key {
                    code: 17,
                    shift: true,
                    down,
                    repeat,
                })
            );
        }
        assert_eq!(keyboard.event(17, 3, None), None);
        assert_eq!(keyboard.event(0xffff, 1, None), None);
        assert_eq!(key(59, false), Some(("F1", "F1")));
        assert_eq!(key(87, false), Some(("F11", "F11")));
        assert_eq!(key(88, false), Some(("F12", "F12")));
        // KEY_BACK is a key like any other, named as the web names it.
        assert_eq!(
            keyboard.event(158, 1, None),
            Some(InputEvent::Key {
                code: 158,
                shift: true,
                down: true,
                repeat: false,
            })
        );
        assert_eq!(key(158, false), Some(("BrowserBack", "BrowserBack")));
    }
    #[test]
    fn shift_sides_and_shortcut_chords_pass() {
        let mut keyboard = Keyboard::default();
        keyboard.event(42, 1, None);
        keyboard.event(54, 1, None);
        keyboard.event(42, 0, None);
        assert!(matches!(
            keyboard.event(17, 1, None),
            Some(InputEvent::Key {
                code: 17,
                shift: true,
                ..
            })
        ));
        keyboard.event(54, 0, None);
        for modifier in [29, 97, 125, 126] {
            keyboard.event(modifier, 1, None);
            assert!(matches!(
                keyboard.event(17, 1, None),
                Some(InputEvent::Key {
                    code: 17,
                    down: true,
                    ..
                })
            ));
            assert!(matches!(
                keyboard.event(17, 0, None),
                Some(InputEvent::Key {
                    code: 17,
                    down: false,
                    ..
                })
            ));
            keyboard.event(modifier, 0, None);
        }
        assert!(matches!(
            keyboard.event(17, 1, None),
            Some(InputEvent::Key {
                code: 17,
                shift: false,
                ..
            })
        ));
        assert!(matches!(
            keyboard.event(103, 1, None),
            Some(InputEvent::Key {
                code: 103,
                shift: false,
                ..
            })
        ));
    }
}

#[cfg(test)]
mod pointer_tests {
    use super::*;

    /// A finger on a touchpad moves the pointer by its travel; landing,
    /// another finger joining and lifting move nothing.
    #[test]
    fn a_touchpad_moves_by_the_fingers_travel_and_never_jumps() {
        // 10 units per millimetre, the pad's place at (500, 300).
        let mut g = Glide::new([(500, (0, 1000), 10), (300, (0, 600), 10)]);
        let px = PAD_PIXELS_PER_MM / 10.0;
        assert_eq!(g.axis(0, 520), 0.0, "no finger touches");
        g.report();
        g.touch(true);
        assert_eq!(g.axis(0, 900), 0.0, "a finger lands across the pad");
        assert_eq!(g.axis(1, 100), 0.0);
        g.report();
        assert_eq!(g.axis(0, 920), 20.0 * px);
        assert_eq!(g.axis(1, 90), -10.0 * px);
        g.report();
        g.restart();
        assert_eq!(g.axis(0, 100), 0.0, "a second finger joins");
        g.report();
        assert_eq!(g.axis(0, 110), 10.0 * px);
        g.report();
        g.touch(false);
        assert_eq!(g.axis(0, 500), 0.0, "lifted");
        g.report();
        assert_eq!(g.axis(0, 510), 0.0);
        let unknown = Glide::new([(0, (0, 2000), 0), (0, (0, 1000), 0)]);
        assert_eq!(unknown.scale, [PAD_PIXELS_PER_MM / 20.0; 2], "100 mm wide");
    }
    #[test]
    fn a_report_keeps_diagonal_intent_atomic_and_boundaries_separate() {
        let mut report = PointerReport::default();
        let mut out = vec![InputEvent::Button(true)];
        report.relative.0 += 10.;
        report.relative.1 += 30.;
        report.flush(&mut out);
        report.absolute.0 = Some(0.2);
        report.absolute.1 = Some(0.3);
        report.flush(&mut out);
        out.push(InputEvent::Button(false));
        report.flush(&mut out);
        assert_eq!(
            out,
            vec![
                InputEvent::Button(true),
                InputEvent::Motion(10., 30.),
                InputEvent::Absolute(Some(0.2), Some(0.3)),
                InputEvent::Button(false)
            ]
        );
    }
}
