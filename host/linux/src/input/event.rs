//! Events shared by physical input, VNC and the output transform.
use exact_painted::gamepad::Reading as PadReading;

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
