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
/// down, a pen's or a finger's, and where it is.
#[derive(Default)]
pub(crate) struct Touches {
    held: Option<(u64, PointerKind, (f32, f32))>,
    pinch: Option<Pinch>,
    /// Every finger and pen down, the contact's or not.
    down: Vec<u64>,
}

/// Two fingers on a zoomable scroller: its pinch (LLP 1057.001.000 §5,
/// step 6), each finger's pointer id and where it is.
struct Pinch {
    container: u32,
    fingers: [(u64, (f32, f32)); 2],
}

impl Pinch {
    /// The fingers' midpoint and how far apart they are.
    fn focal(&self) -> ((f32, f32), f32) {
        let [(_, a), (_, b)] = self.fingers;
        (
            ((a.0 + b.0) / 2., (a.1 + b.1) / 2.),
            (a.0 - b.0).hypot(a.1 - b.1),
        )
    }
}

impl Touches {
    /// Pointer `id`'s `phase` at `(x, y)` in logical pixels. `pen` says
    /// whether a new contact is a pen's; it is asked only of a touch down.
    /// A pen's hover (an update with no contact) does nothing, nor does a
    /// finger after the first, except that its touch down cancels the
    /// first's press, as Android's reader does, and over a zoomable
    /// scroller the two fingers pinch it until either lifts. Answers
    /// whether a contact went down.
    pub(crate) fn contact<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        (id, phase): (u64, Phase),
        at: (f32, f32),
        now: f64,
        pen: impl FnOnce() -> bool,
    ) -> bool {
        match phase {
            Phase::Started => self.down.push(id),
            Phase::Moved => {}
            Phase::Ended | Phase::Cancelled => self.down.retain(|down| *down != id),
        }
        // Said before the routing below, which follows one contact: a
        // replacement waits for every finger and pen (LLP 1029.000.002 D9).
        if phase != Phase::Moved {
            p.set_touching(!self.down.is_empty());
        }
        let went_down = self.route(p, (id, phase), at, now, pen);
        // A cancelled touch has no up, and after a second finger took the
        // contact neither lift is routed: the press the presenter heard
        // ends with the last of them.
        if phase != Phase::Moved && self.down.is_empty() {
            p.pointer_released();
        }
        went_down
    }

    fn route<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        (id, phase): (u64, Phase),
        (x, y): (f32, f32),
        now: f64,
        pen: impl FnOnce() -> bool,
    ) -> bool {
        if self.pinched(p, (id, phase), (x, y)) {
            return false;
        }
        let kind = match (phase, self.held) {
            (Phase::Started, None) => {
                let kind = if pen() {
                    PointerKind::Pen
                } else {
                    PointerKind::Touch
                };
                self.held = Some((id, kind, (x, y)));
                kind
            }
            (Phase::Started, Some((first, kind, at))) => {
                self.held = None;
                p.set_pointer_kind(kind);
                let _ = p.pointer_cancel(now);
                let both = [(first, at), (id, (x, y))];
                let under = |p: &mut Presenter<D>, at: (f32, f32)| p.zoomable_under(at.0, at.1);
                if kind == PointerKind::Touch {
                    if let Some(container) = under(p, at).filter(|c| under(p, (x, y)) == Some(*c)) {
                        p.zoom_pinch_began(container);
                        self.pinch = Some(Pinch {
                            container,
                            fingers: both,
                        });
                    }
                }
                return false;
            }
            (_, Some((held, kind, _))) if held == id => {
                self.held = Some((held, kind, (x, y)));
                kind
            }
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

    /// A finger of the pinch moved or lifted: the zoom follows the two, the
    /// point between them staying under them, and either's lift ends it.
    /// Whether the event was the pinch's.
    fn pinched<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        (id, phase): (u64, Phase),
        at: (f32, f32),
    ) -> bool {
        let Some(pinch) = self.pinch.as_mut() else {
            return false;
        };
        let Some(k) = pinch.fingers.iter().position(|f| f.0 == id) else {
            // A third finger is no part of it.
            return true;
        };
        if phase == Phase::Moved {
            let (from, was) = pinch.focal();
            pinch.fingers[k].1 = at;
            let (to, now) = pinch.focal();
            let factor = if was > 1. { now / was } else { 1. };
            p.zoom_pinched(pinch.container, factor, from, to);
        } else {
            p.zoom_pinch_ended(pinch.container);
            self.pinch = None;
        }
        true
    }

    /// The window lost the focus: the contact ends, its lift perhaps going
    /// to another window. Each finger and pen stays down for a replacement
    /// (LLP 1029.000.002 D9) until its own lift or cancel, or until
    /// [`Touches::prune`] finds it up.
    pub(crate) fn forget<D: DataSource>(&mut self, p: &mut Presenter<D>, now: f64) {
        self.held = None;
        if let Some(pinch) = self.pinch.take() {
            p.zoom_pinch_ended(pinch.container);
        }
        let _ = p.pointer_abandoned(now);
    }

    /// Whether a finger or a pen is down.
    pub(crate) fn any_down(&self) -> bool {
        !self.down.is_empty()
    }

    /// Lets go of each finger or pen that `down` says is no longer in
    /// contact, for one whose lift another window heard.
    pub(crate) fn prune<D: DataSource>(
        &mut self,
        p: &mut Presenter<D>,
        down: impl Fn(u64) -> bool,
    ) {
        if self.down.is_empty() {
            return;
        }
        self.down.retain(|id| down(*id));
        if self.down.is_empty() {
            p.set_touching(false);
            p.pointer_released();
        }
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

/// Whether pointer `id` is still in contact, as Windows has it now
/// ([GetPointerInfo](https://learn.microsoft.com/windows/win32/api/winuser/nf-winuser-getpointerinfo)
/// fails for a pointer no longer active).
#[cfg(windows)]
#[allow(unsafe_code)]
pub(crate) fn in_contact(id: u64) -> bool {
    use windows_sys::Win32::UI::Input::Pointer::{
        GetPointerInfo, POINTER_FLAG_INCONTACT, POINTER_INFO,
    };
    // SAFETY: POINTER_INFO is plain data, all zeros valid; the call writes it.
    let mut info: POINTER_INFO = unsafe { std::mem::zeroed() };
    let active = unsafe { GetPointerInfo(id as u32, &mut info) } != 0;
    active && info.pointerFlags & POINTER_FLAG_INCONTACT != 0
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
        // Both are still down, and hold a replacement until the last lifts.
        let held = |p: &mut Presenter<()>| {
            let result = p.reload(&plan, ()).unwrap();
            result == exact_painted::presenter::ReloadResult::Held("pointer")
        };
        assert!(held(&mut p));
        // A third finger is a contact of its own, and its lift leaves two down.
        send(&mut p, &mut t, 20, Phase::Started, (200., 250.), false);
        send(&mut p, &mut t, 20, Phase::Ended, (200., 250.), false);
        assert!(held(&mut p));
        send(&mut p, &mut t, 11, Phase::Ended, (60., 120.), false);
        assert!(held(&mut p));
        send(&mut p, &mut t, 10, Phase::Ended, (20., 120.), false);
        assert!(!held(&mut p));
        // Nor does a cancelled touch.
        send(&mut p, &mut t, 12, Phase::Started, (20., 120.), false);
        send(&mut p, &mut t, 12, Phase::Cancelled, (20., 120.), false);
        assert!(!held(&mut p));
        // The window's blur ends the contact, so the lift presses nothing, and
        // the finger holds a replacement until it lifts.
        send(&mut p, &mut t, 13, Phase::Started, (20., 120.), false);
        t.forget(&mut p, 1000.);
        assert!(held(&mut p), "the finger is still down");
        assert!(!send(&mut p, &mut t, 13, Phase::Ended, (20., 120.), false));
        assert!(!held(&mut p));
        assert_eq!(seen(&p), Some(Value::str(" touch pen press")));
        // One whose lift another window heard is let go when Windows says it is up.
        send(&mut p, &mut t, 15, Phase::Started, (20., 120.), false);
        t.forget(&mut p, 1001.);
        t.prune(&mut p, |_| true);
        assert!(held(&mut p));
        t.prune(&mut p, |_| false);
        assert!(!held(&mut p));
        assert_eq!(seen(&p), Some(Value::str(" touch pen press")));
        // Then a finger presses again.
        send(&mut p, &mut t, 14, Phase::Started, (20., 120.), false);
        send(&mut p, &mut t, 14, Phase::Ended, (20., 120.), false);
        assert_eq!(seen(&p), Some(Value::str(" touch pen press press")));
    }

    #[test]
    fn two_fingers_on_a_zoomable_scroller_pinch_it_about_their_midpoint() {
        let source = "component App
  state zoom = 1
  action zoomed(x: number, y: number, e: ScrollEvent)
    zoom = e.scrollZoom
  view
    scroll width=300 height=200 -exact-max-zoom=4 scrollZoom=zoom scroll=zoomed testId=\"port\"
      box width=300 height=200
";
        let plan = contract::compile(source).unwrap().encode();
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
        let mut send = |p: &mut Presenter<()>, t: &mut Touches, id, phase, xy| {
            at += 10.;
            t.contact(p, (id, phase), xy, at, || false)
        };
        // Fingers 100 apart about (150, 100), spread to 200 apart.
        send(&mut p, &mut t, 1, Phase::Started, (100., 100.));
        send(&mut p, &mut t, 2, Phase::Started, (200., 100.));
        send(&mut p, &mut t, 1, Phase::Moved, (75., 100.));
        send(&mut p, &mut t, 2, Phase::Moved, (225., 100.));
        send(&mut p, &mut t, 1, Phase::Moved, (50., 100.));
        send(&mut p, &mut t, 2, Phase::Moved, (250., 100.));
        send(&mut p, &mut t, 2, Phase::Ended, (250., 100.));
        send(&mut p, &mut t, 1, Phase::Ended, (50., 100.));
        let port = p.host().kernel();
        let port = port
            .node_by_key(port.find_by_test_id("port")[0])
            .unwrap()
            .id;
        // Twice the zoom, the midpoint kept: the child's point at (150, 100) stays there.
        let offset = p.scroll_of(port);
        assert!(
            (offset.0 - 150.).abs() < 0.01 && (offset.1 - 100.).abs() < 0.01,
            "{offset:?}"
        );
        let zoom = p.host().runner().slot("zoom").cloned();
        assert!(
            matches!(zoom, Some(Value::Number(z)) if (z - 2.).abs() < 1e-4),
            "{zoom:?}"
        );
    }
}
