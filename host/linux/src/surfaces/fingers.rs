//! The fingers of a touch past its first, on Android (`Native.finger`). The
//! first finger is the presenter's contact: a press, a scroll, or pointer 1
//! of the canvas it landed on (`contact.rs`). When a canvas holds the touch,
//! each other finger that lands on that canvas is a pointer of its own
//! there, numbered from 2 and never reused, as iOS numbers a canvas's
//! touches (`CanvasInputIOS.swift`); it stays the canvas's after the first
//! finger lifts. A finger landing anywhere else is nobody's. Where no canvas
//! holds the touch, a second finger ends it (`CanvasHost::finger`).
//!
//! The canvas hears no move or up for a finger it did not hear land, and
//! every finger it heard land ends with an up or a cancel: a cancel when the
//! touch is cancelled (a scroll took it, the system cancelled it), when a
//! commit leaves the node it landed on no longer the canvas's input (as the
//! contact's own, `Presenter::canvas_input_live`), or when a new touch
//! begins with one never lifted. When the canvas goes, or another surface
//! takes its view, its fingers end unheard: the new surface never saw them
//! land.
use super::*;
use exact_kernel::NodeKey;

/// A finger the canvas took.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Finger {
    /// Its pointer id on the canvas.
    pointer: u32,
    /// Its last viewport point.
    at: (f32, f32),
    /// The node it landed on.
    hit: NodeKey,
}

/// The fingers on the canvas holding them, by Android's pointer id.
#[derive(Default)]
pub(crate) struct Fingers {
    /// The canvas they are on: its view, its instance and its surface name.
    on: Option<(u32, u32, String)>,
    /// By Android's pointer id.
    down: BTreeMap<i32, Finger>,
    /// The last pointer id given.
    last: u32,
}

impl Fingers {
    /// Finger `id` landed at `at` on node `hit` of canvas `on`, the one the
    /// others are on if any: its pointer id there, and a finger recorded
    /// under the same id whose up never came, to be cancelled.
    fn land(
        &mut self,
        id: i32,
        on: (u32, u32, String),
        at: (f32, f32),
        hit: NodeKey,
    ) -> (u32, Option<Finger>) {
        self.on = Some(on);
        self.last = self.last.max(1) + 1;
        let pointer = self.last;
        (pointer, self.down.insert(id, Finger { pointer, at, hit }))
    }

    /// Finger `id` moved to `at`: its pointer and motion, when it is the
    /// canvas's and it moved.
    fn moved(&mut self, id: i32, at: (f32, f32)) -> Option<(u32, (f32, f32))> {
        let finger = self.down.get_mut(&id)?;
        let motion = (at.0 - finger.at.0, at.1 - finger.at.1);
        (motion != (0., 0.)).then(|| {
            finger.at = at;
            (finger.pointer, motion)
        })
    }

    /// Finger `id` lifted or was cancelled, when it was the canvas's.
    fn lifted(&mut self, id: i32) -> Option<Finger> {
        let finger = self.down.remove(&id);
        if self.down.is_empty() {
            self.on = None;
        }
        finger
    }

    /// Every finger, and the canvas view they are on.
    fn take(&mut self) -> Option<(u32, Vec<Finger>)> {
        let (view, _, _) = self.on.take()?;
        Some((view, std::mem::take(&mut self.down).into_values().collect()))
    }
}

impl<D: DataSource> Presenter<D> {
    /// A finger past the touch's first (0 down, 1 up, 2 move, 3 cancel) at
    /// a viewport point, by Android's pointer `id`. Whether a canvas holds
    /// the touch: where none does, the caller ends it.
    pub(crate) fn canvas_finger(&mut self, id: i32, action: i32, x: f32, y: f32, at: f64) -> bool {
        self.forget_gone_fingers();
        let on = match self.surfaces.fingers.on.clone() {
            Some(on) => on,
            None => {
                let Some(view) = self.contact_canvas() else {
                    return false;
                };
                let Some(c) = self.surfaces.canvases.get(&view) else {
                    return false;
                };
                (view, c.id, c.name.clone())
            }
        };
        let view = on.0;
        match action {
            0 => {
                // A button over the canvas, a menu, another canvas: not this
                // canvas's, and no end of its touch.
                let hit = self.hit(x, y);
                let key = hit
                    .and_then(|id| self.host.kernel().node(id))
                    .map(|n| n.key);
                let Some(key) = key.filter(|key| {
                    hit.and_then(|id| self.canvas_contact_target(id)) == Some(view)
                        && self.canvas_input_live(*key, view)
                }) else {
                    return true;
                };
                let (pointer, stale) = self.surfaces.fingers.land(id, on, (x, y), key);
                if let Some(stale) = stale {
                    self.finger_event(view, stale.pointer, "cancel", stale.at, (0., 0.), at);
                }
                self.finger_event(view, pointer, "down", (x, y), (0., 0.), at);
            }
            2 => {
                if let Some((pointer, motion)) = self.surfaces.fingers.moved(id, (x, y)) {
                    self.finger_event(view, pointer, "move", (x, y), motion, at);
                }
            }
            _ => {
                if let Some(finger) = self.surfaces.fingers.lifted(id) {
                    let (phase, point) = match action {
                        1 => ("up", (x, y)),
                        _ => ("cancel", finger.at),
                    };
                    self.finger_event(view, finger.pointer, phase, point, (0., 0.), at);
                }
            }
        }
        true
    }

    /// Every finger past the touch's first ends with a cancel.
    pub(crate) fn cancel_fingers(&mut self, at: f64) {
        self.forget_gone_fingers();
        if let Some((view, fingers)) = self.surfaces.fingers.take() {
            for finger in fingers {
                self.finger_event(view, finger.pointer, "cancel", finger.at, (0., 0.), at);
            }
        }
    }

    /// After a commit, as the contact is: a finger whose node is no longer
    /// its canvas's input (disabled, inert, hidden, removed) is cancelled.
    pub(crate) fn retire_fingers(&mut self, at: f64) {
        self.forget_gone_fingers();
        let Some((view, _, _)) = self.surfaces.fingers.on.clone() else {
            return;
        };
        let dead: Vec<i32> = (self.surfaces.fingers.down.iter())
            .filter(|(_, f)| !self.canvas_input_live(f.hit, view))
            .map(|(id, _)| *id)
            .collect();
        for id in dead {
            if let Some(finger) = self.surfaces.fingers.lifted(id) {
                self.finger_event(view, finger.pointer, "cancel", finger.at, (0., 0.), at);
            }
        }
    }

    /// The canvas the fingers are on went, or another surface took its view.
    fn forget_gone_fingers(&mut self) {
        let fingers = &mut self.surfaces.fingers;
        let live = fingers.on.as_ref().is_none_or(|(view, id, name)| {
            (self.surfaces.canvases.get(view)).is_some_and(|c| c.id == *id && c.name == *name)
        });
        if !live {
            fingers.on = None;
            fingers.down.clear();
        }
    }

    fn finger_event(
        &mut self,
        view: u32,
        pointer: u32,
        phase: &str,
        (x, y): (f32, f32),
        (dx, dy): (f32, f32),
        at: f64,
    ) {
        let (ox, oy) = self.rect_of(view).map_or((0., 0.), |r| (r.0, r.1));
        let buttons = u32::from(matches!(phase, "down" | "move"));
        self.surfaces.input(view, json!({"t":"pointer","id":pointer,"phase":phase,"kind":"touch","buttons":buttons,"x":x-ox,"y":y-oy,"dx":dx,"dy":dy,"at":at}));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_finger_is_a_pointer_of_its_own_never_reused() {
        let canvas = || (5, 1, "touches".to_string());
        let hit = NodeKey {
            index: 3,
            generation: 1,
        };
        let finger = |pointer, at| Finger { pointer, at, hit };
        let mut f = Fingers::default();
        assert_eq!(f.land(1, canvas(), (10., 10.), hit), (2, None));
        assert_eq!(f.land(2, canvas(), (20., 20.), hit), (3, None));
        assert_eq!(f.moved(1, (10., 10.)), None, "a move that moved nothing");
        assert_eq!(f.moved(1, (12., 9.)), Some((2, (2., -1.))));
        assert_eq!(
            f.moved(7, (1., 1.)),
            None,
            "a finger that is not the canvas's"
        );
        assert_eq!(f.lifted(7), None);
        assert_eq!(f.lifted(1), Some(finger(2, (12., 9.))));
        assert_eq!(f.moved(1, (13., 9.)), None, "no move after its up");
        assert!(f.on.is_some(), "the canvas keeps the finger still down");
        // Android gives a new finger the id the lifted one had: a new pointer.
        assert_eq!(f.land(1, canvas(), (30., 30.), hit), (4, None));
        // An up that never came: the finger is cancelled as the new one lands.
        assert_eq!(
            f.land(2, canvas(), (40., 40.), hit),
            (5, Some(finger(3, (20., 20.))))
        );
        let (view, mut all) = f.take().unwrap();
        all.sort_by_key(|f| f.pointer);
        assert_eq!(
            (view, all),
            (5, vec![finger(4, (30., 30.)), finger(5, (40., 40.))])
        );
        assert!(f.on.is_none() && f.take().is_none());
        f.land(3, canvas(), (0., 0.), hit);
        f.lifted(3);
        assert!(f.on.is_none(), "the last finger's up frees the canvas");
    }

    #[test]
    fn the_canvas_holding_the_touch_hears_each_finger_from_its_down_to_its_end() {
        let (path, compat) = super::super::tests::fixture();
        let source = r#"component App
  state off = false
  action disable
    off = true
  view
    column
      column disabled=off
        canvas testId="world" width=100 height=100
      button testId="other" width=100 height=40 press=disable
"#;
        let plan = contract::compile(source).unwrap();
        let (mut p, _) = Presenter::boot(
            &plan.encode(),
            (),
            (100., 200.),
            1.,
            path.parent().unwrap().into(),
        )
        .unwrap();
        let find = |p: &Presenter<()>, kind: exact_kernel::NodeType| {
            (p.host.preorder().into_iter())
                .find(|id| p.host.kernel().node(*id).unwrap().node_type == kind)
                .unwrap()
        };
        let canvas = find(&p, exact_kernel::NodeType::Canvas);
        let button = (p.host.preorder().into_iter())
            .find(|id| {
                let node = p.host.kernel().node(*id).unwrap();
                node.props.str(exact_kernel::PropId::TestId) == Some("other")
            })
            .unwrap();
        p.surfaces
            .abis
            .insert(String::new(), Abi::open_path(&path, &compat, "").unwrap());
        let surface = |id: u32, name: &str| Canvas {
            performance: false,
            id,
            name: name.into(),
            artifact: String::new(),
            owner: true,
            since: 0,
            held: BTreeSet::new(),
            restored_controls: None,
            restore_error: None,
            restore_input: false,
            restore_bytes: None,
            restore_logged: false,
            uploaded: None,
        };
        p.surfaces.canvases.insert(canvas, surface(1, "world"));
        p.boxes();
        // The probe's count of events and the last two, as (pointer, phase).
        let heard = |p: &Presenter<()>| {
            let abi = &p.surfaces.abis[""];
            let read = |name: &'static [u8]| unsafe {
                let text = abi.symbol::<unsafe extern "C" fn() -> *const std::ffi::c_char>(name)();
                let event: Value =
                    serde_json::from_slice(std::ffi::CStr::from_ptr(text).to_bytes())
                        .unwrap_or(Value::Null);
                (
                    event["id"].as_u64().unwrap_or(0) as u32,
                    event["phase"].as_str().unwrap_or("").to_string(),
                )
            };
            let count =
                unsafe { abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_input_count")() };
            (count, read(b"test_previous_input"), read(b"test_input"))
        };
        let phase = |s: &str| s.to_string();

        assert!(
            !p.canvas_finger(0, 0, 50., 50., 0.),
            "no canvas holds a touch that is not down"
        );
        p.pointer_down(50., 50., 1.).unwrap();
        assert_eq!(p.contact_canvas(), Some(canvas));
        let (before, _, last) = heard(&p);
        assert_eq!(last, (1, phase("down")));
        assert!(p.canvas_finger(1, 0, 60., 60., 2.));
        assert_eq!(
            heard(&p),
            (before + 1, (1, phase("down")), (2, phase("down")))
        );
        assert!(
            p.canvas_finger(1, 2, 60., 60., 3.),
            "a move that moved nothing"
        );
        assert!(
            p.canvas_finger(2, 0, 50., 120., 3.),
            "a finger on the button is nobody's"
        );
        assert!(p.canvas_finger(2, 2, 50., 130., 3.));
        assert!(p.canvas_finger(2, 1, 50., 130., 3.));
        assert_eq!(heard(&p).0, before + 1);
        assert!(p.canvas_finger(1, 2, 61., 62., 4.));
        assert_eq!(heard(&p).2, (2, phase("move")));
        // The first finger lifts; the canvas keeps the other, and takes a new one.
        p.pointer_up(50., 50., 5.).unwrap();
        assert!(p.canvas_finger(3, 0, 20., 20., 6.));
        assert_eq!(heard(&p).2, (3, phase("down")));
        // A cancelled touch cancels every finger.
        let (before, _, _) = heard(&p);
        p.pointer_cancel(7.).unwrap();
        assert_eq!(
            heard(&p),
            (before + 2, (2, phase("cancel")), (3, phase("cancel")))
        );
        assert!(
            !p.canvas_finger(1, 2, 70., 70., 8.),
            "nothing is left of them"
        );
        assert_eq!(heard(&p).0, before + 2);

        // A new touch while a finger's up never came: it is cancelled first.
        p.pointer_down(50., 50., 9.).unwrap();
        assert!(p.canvas_finger(4, 0, 30., 30., 10.));
        p.pointer_up(50., 50., 10.).unwrap();
        p.pointer_down(50., 50., 11.).unwrap();
        assert_eq!(heard(&p).1, (4, phase("cancel")));
        // Another surface takes the view: the old one's finger ends unheard.
        assert!(p.canvas_finger(5, 0, 30., 30., 12.));
        p.surfaces
            .canvases
            .insert(canvas, surface(2, "replacement"));
        let (before, _, _) = heard(&p);
        p.canvas_finger(5, 2, 40., 40., 13.);
        p.canvas_finger(5, 1, 40., 40., 13.);
        assert_eq!(heard(&p).0, before);

        // The first finger lifts, then a commit disables the canvas's parent:
        // the other finger is cancelled, and nothing more of it is heard.
        p.pointer_down(50., 50., 14.).unwrap();
        assert!(p.canvas_finger(6, 0, 30., 30., 15.));
        let (_, _, (pointer, _)) = heard(&p);
        p.pointer_up(50., 50., 16.).unwrap();
        p.host.dispatch_at(button, Event::Press, 17.);
        p.after_commit();
        let (before, _, last) = heard(&p);
        assert_eq!(last, (pointer, phase("cancel")));
        p.canvas_finger(6, 2, 35., 35., 18.);
        p.canvas_finger(6, 1, 35., 35., 18.);
        assert_eq!(heard(&p).0, before);
        drop(p);
        std::fs::remove_dir_all(path.parent().unwrap()).unwrap();
    }
}
