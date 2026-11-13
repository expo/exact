//! A finger's or a pen's contact with the screen. winit answers
//! `WM_POINTERDOWN`, `WM_POINTERUPDATE` and `WM_POINTERUP` for every pointer
//! type with `WindowEvent::Touch` and does not call `DefWindowProc`, which is
//! where Windows would make mouse input of them
//! ([WM_POINTERDOWN](https://learn.microsoft.com/windows/win32/inputmsg/wm-pointerdown)):
//! these events are all the app hears of a touch screen or a pen.
use exact_painted::{presenter::PointerKind, Presenter};
use exact_runner::DataSource;

/// What a pointer message says of a contact (winit's `TouchPhase`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Phase {
    Started,
    Moved,
    Ended,
    Cancelled,
}

/// The pointer the presenter's one contact follows: the first to touch
/// down, a pen's or a finger's.
#[derive(Default)]
pub(crate) struct Touches {
    held: Option<(u64, PointerKind)>,
}

impl Touches {
    /// Pointer `id`'s `phase` at `(x, y)` in logical pixels. `pen` says
    /// whether a new contact is a pen's; it is asked only of a touch down.
    /// A pen's hover (an update with no contact) does nothing, nor does a
    /// finger after the first, except that its touch down cancels the
    /// first's press, as Android's reader does. Answers whether a contact
    /// went down.
    pub(crate) fn contact<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        (id, phase): (u64, Phase),
        (x, y): (f32, f32),
        now: f64,
        pen: impl FnOnce() -> bool,
    ) -> bool {
        let kind = match (phase, self.held) {
            (Phase::Started, None) => {
                let kind = if pen() {
                    PointerKind::Pen
                } else {
                    PointerKind::Touch
                };
                self.held = Some((id, kind));
                kind
            }
            (Phase::Started, Some((_, kind))) => {
                self.held = None;
                p.set_pointer_kind(kind);
                let _ = p.pointer_cancel(now);
                return false;
            }
            (_, Some((held, kind))) if held == id => kind,
            _ => return false,
        };
        // The mouse may have moved since: its events say it is a mouse again.
        p.set_pointer_kind(kind);
        let _ = match phase {
            Phase::Started => p.pointer_down(x, y, now).map(drop),
            Phase::Moved => p.pointer_move(x, y, now).map(drop),
            Phase::Ended => p.pointer_up(x, y, now).map(drop),
            Phase::Cancelled => p.pointer_cancel(now),
        };
        if matches!(phase, Phase::Ended | Phase::Cancelled) {
            self.held = None;
        }
        phase == Phase::Started
    }

    /// The window lost the focus, and its contact with it.
    pub(crate) fn forget(&mut self) {
        self.held = None;
    }

    /// winit's touch, at the window's `scale`.
    #[cfg(windows)]
    pub(crate) fn event<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        touch: winit::event::Touch,
        scale: f64,
        now: f64,
    ) -> bool {
        use winit::event::TouchPhase;
        let phase = match touch.phase {
            TouchPhase::Started => Phase::Started,
            TouchPhase::Moved => Phase::Moved,
            TouchPhase::Ended => Phase::Ended,
            TouchPhase::Cancelled => Phase::Cancelled,
        };
        let at = (
            (touch.location.x / scale) as f32,
            (touch.location.y / scale) as f32,
        );
        self.contact(p, (touch.id, phase), at, now, || pen(touch.id))
    }
}

/// Whether pointer `id` is a pen's: winit's touch id is the message's
/// pointer id ([GetPointerType](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getpointertype)).
#[cfg(windows)]
#[allow(unsafe_code)]
fn pen(id: u64) -> bool {
    use windows_sys::Win32::UI::{Input::Pointer::GetPointerType, WindowsAndMessaging::PT_PEN};
    let mut kind = 0;
    let Ok(id) = u32::try_from(id) else {
        return false;
    };
    // SAFETY: `kind` outlives the call, which writes one POINTER_INPUT_TYPE.
    unsafe { GetPointerType(id, &mut kind) != 0 && kind == PT_PEN }
}

/// Whether a mouse message is one Windows made from a touch or a pen: its
/// extra information carries the signature `0xFF515700` in its upper 24 bits
/// ([System Events and Mouse Messages](https://learn.microsoft.com/windows/win32/tablet/system-events-and-mouse-messages)).
/// What a finger or a pen does arrives as its own touch, so such a message
/// stands for no mouse.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn from_touch_or_pen(extra: isize) -> bool {
    extra as usize & 0xFFFF_FF00 == 0xFF51_5700
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_mouse_message_made_from_a_touch_or_a_pen_is_known_by_its_signature() {
        // MI_WP_SIGNATURE, with the pen and touch flags in the low byte.
        for extra in [0xFF51_5700usize, 0xFF51_5780, 0xFF51_5701] {
            assert!(from_touch_or_pen(extra as isize), "{extra:#x}");
        }
        for extra in [0usize, 0xFF51_5600, 0x0051_5700, 0xFF41_5700] {
            assert!(!from_touch_or_pen(extra as isize), "{extra:#x}");
        }
    }
    use exact_painted::presenter::PainterChoice;
    use exact_runner::Value;

    const APP: &str = "component App
  state seen = \"\"
  action down(e: PointerEvent)
    seen = `${seen} ${e.pointerType}`
  action press
    seen = `${seen} press`
  view
    column width=400 height=300 align-items=\"flex-start\"
      box pointerdown=down width=200 height=100
      button \"Go\" press=press width=120 height=40
";

    #[test]
    fn a_finger_or_a_pen_presses_and_a_second_finger_cancels() {
        let plan = contract::compile(APP).unwrap().encode();
        let (mut p, warning) = Presenter::boot_with(
            &plan,
            (),
            (400., 300.),
            1.,
            Default::default(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(warning.is_none());
        let mut t = Touches::default();
        let mut at = 0.;
        let mut send = |p: &mut Presenter<()>, t: &mut Touches, id, phase, xy, pen: bool| {
            at += 10.;
            t.contact(p, (id, phase), xy, at, || pen)
        };
        let seen = |p: &Presenter<()>| p.host().runner().slot("seen").cloned();
        // A finger's tap on the box, then a pen's after its hover.
        assert!(send(&mut p, &mut t, 7, Phase::Started, (20., 20.), false));
        assert!(!send(&mut p, &mut t, 7, Phase::Ended, (20., 20.), false));
        assert!(!send(&mut p, &mut t, 8, Phase::Moved, (30., 30.), true));
        assert!(send(&mut p, &mut t, 8, Phase::Started, (30., 30.), true));
        send(&mut p, &mut t, 8, Phase::Ended, (30., 30.), true);
        assert_eq!(seen(&p), Some(Value::str(" touch pen")));
        // A finger's tap on the button presses it.
        send(&mut p, &mut t, 9, Phase::Started, (20., 120.), false);
        send(&mut p, &mut t, 9, Phase::Moved, (22., 121.), false);
        send(&mut p, &mut t, 9, Phase::Ended, (22., 121.), false);
        assert_eq!(seen(&p), Some(Value::str(" touch pen press")));
        // A second finger cancels the first's press; neither lift presses.
        send(&mut p, &mut t, 10, Phase::Started, (20., 120.), false);
        assert!(!send(
            &mut p,
            &mut t,
            11,
            Phase::Started,
            (60., 120.),
            false
        ));
        send(&mut p, &mut t, 11, Phase::Ended, (60., 120.), false);
        send(&mut p, &mut t, 10, Phase::Ended, (20., 120.), false);
        // Nor does a cancelled touch, or one the window's blur let go.
        send(&mut p, &mut t, 12, Phase::Started, (20., 120.), false);
        send(&mut p, &mut t, 12, Phase::Cancelled, (20., 120.), false);
        send(&mut p, &mut t, 13, Phase::Started, (20., 120.), false);
        t.forget();
        p.pointer_lost(1000.).unwrap();
        assert!(!send(&mut p, &mut t, 13, Phase::Ended, (20., 120.), false));
        assert_eq!(seen(&p), Some(Value::str(" touch pen press")));
        // Then a finger presses again.
        send(&mut p, &mut t, 14, Phase::Started, (20., 120.), false);
        send(&mut p, &mut t, 14, Phase::Ended, (20., 120.), false);
        assert_eq!(seen(&p), Some(Value::str(" touch pen press press")));
    }
}
