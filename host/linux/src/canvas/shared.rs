//! Shared live subtrees: state precedes teardown, drawings remain reader-owned.
use super::*;
use crate::paint::{SharedElement, SharedState};
use exact_kernel::motion::motion_node;

pub(super) const STATE: u32 = 49;
pub(super) const SET: u32 = 50;
pub(super) const DRAW: u32 = 51;

#[derive(Default)]
pub(super) struct Shared {
    open: Vec<Open>,
    order: std::collections::HashMap<u64, u32>,
    sent: Vec<u32>,
    active: bool,
}
impl Shared {
    pub(super) fn opened(&self) -> bool {
        !self.open.is_empty()
    }
}
struct Open {
    header: Vec<u32>,
    ops: Vec<u32>,
    clips: Vec<clip::Clip>,
    matrix: Option<[f32; 6]>,
}

impl Recorder {
    pub(super) fn shared_state(&mut self, state: &SharedState) {
        let active = !state.alive.is_empty() || !state.frames.is_empty();
        if !active && !self.shared.active {
            return;
        }
        self.shared.active = active;
        let mut words = vec![
            STATE,
            3 + state.frames.len() as u32 * 5 + state.alive.len() as u32 * 2,
            u32::from(state.reduced),
            state.frames.len() as u32,
        ];
        for f in &state.frames {
            for key in [f.from, f.to] {
                let key = motion_node(key);
                words.extend([key as u32, (key >> 32) as u32]);
            }
            words.push(f.progress.to_bits());
        }
        words.push(state.alive.len() as u32);
        for key in &state.alive {
            words.extend([*key as u32, (key >> 32) as u32]);
        }
        if words == self.shared.sent {
            return;
        }
        self.shared.sent.clone_from(&words);
        self.shared.order = state
            .alive
            .iter()
            .enumerate()
            .map(|(i, key)| (*key, i as u32))
            .collect();
        // begin() may already have announced freed images. Capture first.
        words.append(&mut self.ops);
        self.ops = words;
    }

    pub(super) fn shared_open(&mut self, e: &SharedElement<'_>) -> bool {
        if !self.planes.is_empty() {
            return false;
        }
        self.need(None);
        let rect = |r: Rect4| {
            let r = crate::paint::bbox(e.transform, r);
            [
                (r.0 - self.origin.0) * self.scale,
                (r.1 - self.origin.1) * self.scale,
                r.2 * self.scale,
                r.3 * self.scale,
            ]
        };
        let bounds = rect(e.rect);
        let crop = e.image.map(|(_, r)| rect(r)).unwrap_or([0.0; 4]);
        let content = rect(e.content);
        let natural = e
            .image
            .map(|(image, _)| image.natural())
            .unwrap_or_default();
        let unit = e
            .image
            .map(|(image, _)| image.placed((0.0, 0.0, 1.0, 1.0)))
            .unwrap_or((0.0, 0.0, 1.0, 1.0));
        let image = e.image.map_or(0, |(image, _)| {
            self.image_id(&picture::Picture::Bitmap(image.clone()))
        });
        let scale = e.transform.sx.hypot(e.transform.ky) * self.scale;
        let mut header = vec![
            e.key as u32,
            (e.key >> 32) as u32,
            self.shared.order.get(&e.key).copied().unwrap_or(0),
            self.group.unwrap_or(0),
            self.row.as_ref().map_or(0, |r| r.id),
        ];
        header.extend([self.origin.0 * self.scale, self.origin.1 * self.scale].map(f32::to_bits));
        header.extend(bounds.map(f32::to_bits));
        header.push((e.radius * scale).to_bits());
        header.push(image);
        header.extend(crop.map(f32::to_bits));
        header.extend([e.fit, natural.0, natural.1]);
        header.extend([unit.0, unit.1, unit.2, unit.3].map(f32::to_bits));
        header.extend(content.map(f32::to_bits));
        header.push(e.opacity.to_bits());
        let layers = self.layers.shared_ancestors();
        header.push(layers.len() as u32);
        header.extend(layers);
        header.push(self.shared.open.len() as u32);
        for parent in &self.shared.open {
            header.extend(&parent.header[..2]);
        }
        self.ops.extend([DRAW, e.key as u32, (e.key >> 32) as u32]);
        self.shared.open.push(Open {
            header,
            ops: std::mem::take(&mut self.ops),
            clips: std::mem::take(&mut self.clips),
            matrix: self.matrix.take(),
        });
        self.direct.isolation += 1;
        true
    }

    pub(super) fn shared_close(&mut self) {
        let Some(Open {
            header,
            ops,
            clips,
            matrix,
        }) = self.shared.open.pop()
        else {
            return;
        };
        let body = std::mem::replace(&mut self.ops, ops);
        self.ops.extend([SET, (header.len() + body.len()) as u32]);
        self.ops.extend(header);
        self.ops.extend(body);
        self.clips = clips;
        self.matrix = matrix;
        self.direct.isolation -= 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    fn take() -> Vec<u32> {
        FINISHED
            .with(|f| f.borrow_mut().take())
            .expect("painted stream")
    }

    fn sets(ops: &[u32]) -> Vec<&[u32]> {
        let mut out = Vec::new();
        let mut at = 0;
        while at < ops.len() {
            let end = stream::end(ops, at).expect("valid op");
            match ops[at] {
                SET => {
                    let h = at + 2;
                    out.push(&ops[h..end]);
                    let parents = h + 30 + ops[h + 29] as usize;
                    out.extend(sets(&ops[parents + 1 + ops[parents] as usize * 2..end]));
                }
                ROW_BEGIN => out.extend(sets(&ops[at + 6..end - 1])),
                _ => (),
            }
            at = end;
        }
        out
    }

    #[test]
    fn shared_full_window_surface_cannot_mark_an_inner_index_as_direct() {
        let mut r = Recorder::new();
        r.begin(400., 800., 1.);
        let e = SharedElement {
            key: 1,
            rect: (0., 0., 400., 800.),
            radius: 0.,
            opacity: 1.,
            transform: Transform::identity(),
            image: None,
            fit: 0,
            content: (0., 0., 400., 800.),
        };
        assert!(r.shared_begin(&e));
        assert!(r.shared_begin(&SharedElement { key: 2, ..e }));
        r.native(
            91,
            crate::paint::NativeKind::Surface,
            &Shape::rect(e.rect),
            e.transform,
            "{}",
            true,
        );
        assert_eq!(r.direct.at, None);
        r.shared_end();
        r.shared_end();
        assert_eq!(r.direct.isolation, 0);
        let expected = r.ops.clone();
        r.finish().unwrap();
        assert_eq!(
            take(),
            expected,
            "finish must not corrupt a shared header or drawing"
        );
    }

    #[test]
    fn decorated_shared_images_use_the_live_subtree() {
        for (style, fit) in [
            ("", 3),
            ("filter=\"blur(2px)\"", 0),
            ("mask-image=\"linear-gradient(black, transparent)\"", 0),
            ("clip-path=\"path('M0 0 L80 0 L40 60 Z')\"", 0),
        ] {
            let app = format!("component App\n  view\n    image \"\" sharedElement=\"photo\" object-fit=\"cover\" width=80 height=60 {style}\n");
            let (mut p, error) = Presenter::boot_with(
                &crate::test_plan(&app),
                (),
                (400., 800.),
                1.,
                std::path::PathBuf::new(),
                PainterChoice::Canvas,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            let ops = take();
            let defs = sets(&ops);
            assert_eq!(defs.len(), 1);
            assert_eq!(defs[0][17], fit, "{style}");
        }
    }

    #[test]
    fn idle_named_rows_are_retained_and_emit_no_repeated_lifecycle() {
        let app = "component App\n  view\n    scroll width=300 height=300\n      column\n        box sharedElement=\"a\" width=100 height=80 background-color=\"red\"\n        box sharedElement=\"b\" width=100 height=80 background-color=\"blue\"\n";
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 800.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        let first = take();
        assert_eq!(sets(&first).len(), 2);
        let expected = drawing(&first).unwrap().0;
        for _ in 0..3 {
            p.repaint();
            p.frame();
            let ops = take();
            assert!(
                sets(&ops).is_empty(),
                "named rows must not re-record at rest"
            );
            assert_eq!(
                drawing(&ops),
                Some((expected.clone(), false)),
                "the reader can skip unchanged frames"
            );
        }
    }

    #[test]
    fn shared_lifecycle_orders_retained_names_by_css_stacking() {
        let app = r#"component App
  state reversed = false
  action reverse
    reversed = not reversed
  view
    column
      button "Reverse" press=reverse testId="reverse"
      scroll width=300 height=300
        column
          box sharedElement="front" testId="front" z-index=(reversed ? 0 : 2) width=100 height=80 background-color="red"
          box sharedElement="back" testId="back" z-index=1 width=100 height=80 background-color="blue"
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 800.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let key = |name| motion_node(p.host().kernel().find_by_test_id(name)[0]);
        let (front, back) = (key("front"), key("back"));
        for expected in [[back, front], [front, back]] {
            p.frame();
            let ops = take();
            assert_eq!(ops[0], STATE);
            let names: Vec<_> = ops[5..9]
                .chunks_exact(2)
                .map(|k| u64::from(k[0]) | u64::from(k[1]) << 32)
                .collect();
            assert_eq!(names, expected);
            for def in sets(&ops) {
                let key = u64::from(def[0]) | u64::from(def[1]) << 32;
                assert_eq!(
                    def[2] as usize,
                    expected.iter().position(|k| *k == key).unwrap()
                );
            }
            let toggle = p.host().kernel().find_by_test_id("reverse")[0];
            p.tap(p.host().kernel().node_by_key(toggle).unwrap().id)
                .unwrap();
        }
    }

    #[test]
    fn retained_flight_corners_refresh_when_reduced_motion_changes_or_the_flight_lands() {
        let app = r#"component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      scroll width=300 height=300
        column
          if open
            box sharedElement="card" width=100 height=80 border-radius=20 background-color="red" -exact-layout-transition="10s linear"
          else
            box sharedElement="card" width=100 height=80 border-radius=10 background-color="red" -exact-layout-transition="10s linear"
          box height=300
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 800.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host_mut().flights.enabled = true;
        p.frame();
        take();
        let toggle = p.host().kernel().find_by_test_id("toggle")[0];
        p.tap(p.host().kernel().node_by_key(toggle).unwrap().id)
            .unwrap();
        let assert_corner = |ops: &[u32], radius: f32| {
            let defs = sets(ops);
            assert_eq!(defs.len(), 1, "flight-dependent drawing must be refreshed");
            let def = defs[0];
            let parents = 30 + def[29] as usize;
            let mut at = parents + 1 + def[parents] as usize * 2;
            while at < def.len() {
                if def[at] == RRECT {
                    assert_eq!(def[at + 6..at + 14], [radius.to_bits(); 8]);
                    return;
                }
                at = stream::end(def, at).unwrap();
            }
            panic!("the rounded destination must paint its red surface");
        };
        p.frame();
        assert_corner(&take(), 0.);
        for reduced in [true, false] {
            let mut preferences = p.host().runner().viewport().preferences;
            preferences.reduced_motion = reduced;
            assert!(p.set_preferences(preferences).is_none());
            p.frame();
            assert_corner(&take(), if reduced { 20. } else { 0. });
            p.repaint();
            p.frame();
            assert!(
                sets(&take()).is_empty(),
                "unchanged flight rows remain reusable"
            );
        }
        p.clock(11000.);
        p.frame();
        assert_corner(&take(), 20.);
    }

    #[test]
    fn a_shared_image_reloads_its_header_with_its_row() {
        let mut r = Recorder::new();
        r.begin(400., 800., 1.);
        r.row_begin(1, (0., 0.), None);
        let e = SharedElement {
            key: 1,
            rect: (0., 0., 100., 100.),
            radius: 0.,
            opacity: 1.,
            transform: Transform::identity(),
            image: None,
            fit: 3,
            content: (0., 0., 100., 100.),
        };
        r.shared_begin(&e);
        r.slot_begin(42);
        r.slot_end();
        r.shared_end();
        let row = r.row_end(e.rect);
        assert!(
            !r.slot_again(row, 42),
            "a new decode must refresh the shared image metadata, not just its pixels"
        );
    }

    fn definitions(ops: &[u32], out: &mut Vec<(u64, u32, Vec<u64>)>) {
        let mut at = 0;
        while at < ops.len() {
            let end = stream::end(ops, at).expect("valid op");
            assert!(end <= ops.len(), "op {} runs past its body", ops[at]);
            if ops[at] == SET {
                let key = |i| u64::from(ops[i]) | u64::from(ops[i + 1]) << 32;
                let h = at + 2;
                let parents = h + 30 + ops[h + 29] as usize;
                let names = (0..ops[parents] as usize)
                    .map(|i| key(parents + 1 + i * 2))
                    .collect();
                out.push((key(h), ops[h + 4], names));
                definitions(&ops[parents + 1 + ops[parents] as usize * 2..end], out);
            } else if ops[at] == ROW_BEGIN {
                definitions(&ops[at + 6..end - 1], out);
            }
            at = end;
        }
    }

    #[test]
    fn shared_flights_record_lifecycle_before_nested_live_subtrees() {
        let app = r##"component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      box height=100
      scroll width=350 height=300
        column
          box height=80
          if open
            column sharedElement="body" width=300 height=200 -exact-layout-transition="1s linear"
              text "Detail" sharedElement="title" -exact-layout-transition="1s linear"
          else
            column sharedElement="body" width=120 height=90 -exact-layout-transition="1s linear"
              text "Card" sharedElement="title" -exact-layout-transition="1s linear"
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 800.),
            2.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host_mut().flights.enabled = true;
        p.frame();
        let first = take();
        assert_eq!(first[0], STATE);
        assert_eq!((first[3], first[4]), (0, 2));
        let mut before = Vec::new();
        definitions(&first, &mut before);
        assert_eq!(before.len(), 2);
        assert_ne!(
            before[0].1, 0,
            "shared subtree is recorded inside a scroll row"
        );
        assert_eq!(
            before[1].2,
            vec![before[0].0],
            "child retains its shared parent"
        );
        let toggle = p.host().kernel().find_by_test_id("toggle")[0];
        let toggle = p.host().kernel().node_by_key(toggle).unwrap().id;
        p.host_mut()
            .dispatch_at(toggle, exact_runner::Event::Press, 100.);
        p.after_commit();
        p.frame();
        let after = take();
        assert_eq!(
            after[0], STATE,
            "capture source before any frees or node mutations"
        );
        assert_eq!(after[3], 2, "parent and child have their own flights");
        assert_eq!(after[14], 2, "only the arriving names remain alive");
        let mut arrived = Vec::new();
        definitions(&after, &mut arrived);
        assert_eq!(arrived.len(), 2);
        assert!(arrived.iter().all(|a| before.iter().all(|b| a.0 != b.0)));
        assert!(
            drawing(&after).unwrap().1,
            "lifecycle streams cannot be skipped as unchanged drawing"
        );
    }
}
