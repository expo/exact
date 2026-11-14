//! Shared live subtrees: state precedes teardown, drawings remain reader-owned.
use super::*;
use crate::paint::{SharedElement, SharedState};
use exact_kernel::motion::motion_node;

pub(super) const STATE: u32 = 50;
pub(super) const SET: u32 = 51;
pub(super) const DRAW: u32 = 52;

#[derive(Default)]
pub(super) struct Shared {
    open: Vec<Open>,
    pub(super) rows: std::collections::HashMap<u32, Vec<u32>>,
    row: Option<(Vec<u32>, bool)>,
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
    pub(super) fn shared_row_begin(&mut self) {
        self.shared.row = Some((self.layers.shared_ancestors(), false));
    }

    pub(super) fn shared_row_end(&mut self, id: u32) {
        if let Some((layers, true)) = self.shared.row.take() {
            self.shared.rows.insert(id, layers);
        }
    }

    pub(super) fn shared_row_reusable(&self, id: u32) -> bool {
        self.shared
            .rows
            .get(&id)
            .is_none_or(|layers| *layers == self.layers.shared_ancestors())
    }

    pub(super) fn shared_state(&mut self, state: &SharedState) {
        let active = !state.alive.is_empty() || !state.frames.is_empty();
        if !active && !self.shared.active {
            return;
        }
        self.shared.active = active;
        let mut words = vec![
            STATE,
            3 + state.frames.len() as u32 * 6 + state.alive.len() as u32 * 2,
            u32::from(state.reduced),
            state.frames.len() as u32,
        ];
        for f in &state.frames {
            for key in [f.from, f.to] {
                let key = motion_node(key);
                words.extend([key as u32, (key >> 32) as u32]);
            }
            words.extend([f.progress.to_bits(), f.ancestor_opacity.to_bits()]);
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
        if let Some((_, named)) = &mut self.shared.row {
            *named = true;
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
        // Device pixels a point covers here: natural image sizes are in points.
        header.push(scale.to_bits());
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
                    let parents = h + 31 + ops[h + 30] as usize;
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
    fn kept_shared_rows_refresh_when_live_ancestors_change() {
        let mut r = Recorder::new();
        r.begin(400., 800., 1.);
        let e = SharedElement {
            key: 1,
            rect: (0., 0., 80., 60.),
            radius: 0.,
            opacity: 1.,
            transform: Transform::identity(),
            image: None,
            fit: 0,
            content: (0., 0., 80., 60.),
        };
        let layer = |r: &mut Recorder, alpha| {
            assert!(r.layer_begin(
                7,
                Transform::identity(),
                (0., 0.),
                [0., 0., 1., 0., alpha, 0., 0.]
            ));
        };
        r.row_begin(1, (0., 0.), None);
        r.shared_begin(&e);
        r.shared_end();
        assert_eq!(r.row_end(e.rect), 1);
        assert!(r.row_reusable(1, (0., 0.)));
        layer(&mut r, 0.5);
        assert!(
            !r.row_reusable(1, (0., 0.)),
            "a new live ancestor must enter the shared definition"
        );
        r.row_begin(2, (0., 0.), Some(1));
        r.shared_begin(&e);
        r.shared_end();
        assert_eq!(r.row_end(e.rect), 2);
        r.layer_end();
        layer(&mut r, 0.25);
        assert!(
            r.row_reusable(2, (0., 0.)),
            "live alpha changes need no new drawing"
        );
        r.layer_end();
        assert!(
            !r.row_reusable(2, (0., 0.)),
            "a removed live ancestor must leave the shared definition"
        );
        r.row_free(2);
        assert!(!r.shared.rows.contains_key(&2));
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
    fn invisible_shared_images_use_the_live_subtree() {
        for (parent, own, fit) in [
            ("", "", 3),
            ("", "visibility=\"hidden\"", 0),
            ("visibility=\"hidden\"", "", 0),
            ("visibility=\"hidden\"", "visibility=\"visible\"", 3),
        ] {
            let app = format!("component App\n  view\n    column {parent}\n      image \"\" sharedElement=\"photo\" object-fit=\"cover\" width=80 height=60 {own}\n");
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
            assert_eq!(
                defs[0][17], fit,
                "a raster flight draws pixels the page itself skips: {parent} {own}"
            );
        }
    }

    #[test]
    fn a_destination_that_stops_rendering_retires_its_flight() {
        for (ancestor, own) in [
            ("", " display=(hidden ? \"none\" : \"flex\")"),
            (" display=(hidden ? \"none\" : \"flex\")", ""),
        ] {
            let app = format!(
                r#"component App
  state open = false
  state hidden = false
  action toggle
    open = not open
  action hide
    hidden = not hidden
  view
    column
      button "Toggle" press=toggle testId="toggle"
      button "Hide" press=hide testId="hide"
      column{ancestor}
        if open
          column sharedElement="card" width=300 height=200{own} -exact-layout-transition="1s linear"
            button "Inside" press=toggle testId="inside"
        else
          view sharedElement="card" width=80 height=60 -exact-layout-transition="1s linear"
"#
            );
            let (mut p, error) = Presenter::boot_with(
                &crate::test_plan(&app),
                (),
                (400., 700.),
                1.,
                std::path::PathBuf::new(),
                PainterChoice::Canvas,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            p.frame();
            let id = |p: &Presenter<()>, name: &str| {
                let key = p.host().kernel().find_by_test_id(name)[0];
                p.host().kernel().node_by_key(key).unwrap().id
            };
            p.tap(id(&p, "toggle")).unwrap();
            p.frame();
            assert_eq!(p.host().flight_frames().len(), 1);
            take();
            p.tap(id(&p, "hide")).unwrap();
            p.frame();
            let ops = take();
            assert_eq!(ops[0], STATE);
            assert_eq!(
                ops[3..5],
                [0, 0],
                "a destination that is not rendered has no flight and no live definition"
            );
            assert!(
                p.host().flight_frames().is_empty(),
                "the host retires a flight whose destination stopped rendering"
            );
            p.tap(id(&p, "hide")).unwrap();
            p.frame();
            let child = id(&p, "inside");
            assert!(!p.host().flying(child));
            let rect = p.boxes().iter().find(|b| b.id == child).unwrap().rect;
            assert_eq!(
                p.hit(rect.0 + 5., rect.1 + 5.),
                Some(child),
                "shown again, the destination is interactive at once"
            );
        }
    }

    struct Pixels;
    impl crate::presenter::top::SurfacePainter for Pixels {
        fn record(
            &mut self,
            size: (f32, f32),
            scale: f32,
            paint: &mut dyn FnMut(&mut dyn crate::paint::Backend),
        ) -> Vec<u32> {
            let mut raster = crate::raster::Raster::transparent();
            raster.begin(size.0, size.1, scale);
            paint(&mut raster);
            let pixels = raster.finish().unwrap();
            vec![pixels.width(), pixels.height()]
        }
    }

    /// A route's hiding changes with no commit: when a push's crossing ends,
    /// and while a Back preview shows the route beneath.
    #[test]
    fn a_route_hidden_between_commits_ends_flights_and_keeps_names_alive() {
        let app = r##"routes nav
  home "/"
    detail "/detail"
component App
  state open = false
  state count = 0
  action toggle
    open = not open
  action bump
    count = count + 1
  action show(url: string)
    nav = push(nav, url)
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" viewport-fit="cover" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0 background-color="#ffffff"
            button "Toggle" press=toggle testId="toggle" height=40
            button "Detail" press=show("/detail") testId="open-detail" height=40
            if open
              box sharedElement="card" width=300 height=200 background-color="red" -exact-layout-transition="2s linear" testId="card"
            else
              box sharedElement="card" width=80 height=60 background-color="red" -exact-layout-transition="2s linear"
        if e.name == "detail"
          column navigationKey=`${e.id}` position="absolute" inset=0 background-color="#ffffff"
            button "Bump" press=bump testId="bump" height=40
            text `${count}`
"##;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (412., 916.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.serve_windows(Box::new(Pixels));
        assert_eq!(p.serve_bars(Box::new(Pixels)), None);
        let window = crate::presenter::Window {
            size: (412., 916.),
            bars: [52., 0., 24., 0.],
            ..Default::default()
        };
        assert_eq!(p.set_window(window), None);
        // The last lifecycle the reader was sent: its flights and its names.
        let mut state = (0, Vec::new());
        let mut paint = |p: &mut Presenter<()>, ms: f64| {
            let now = p.host().now() + ms;
            p.tick(now);
            p.frame();
            let ops = take();
            if ops[0] == STATE {
                let names = 5 + ops[3] as usize * 6;
                state = (
                    ops[3],
                    ops[names..names + ops[names - 1] as usize * 2]
                        .chunks_exact(2)
                        .map(|k| u64::from(k[0]) | u64::from(k[1]) << 32)
                        .collect(),
                );
            }
            (state.clone(), ops)
        };
        let tap = |p: &mut Presenter<()>, name: &str| {
            let key = p.host().kernel().find_by_test_id(name)[0];
            let id = p.host().kernel().node_by_key(key).unwrap().id;
            p.tap(id).unwrap();
            p.run_commands(|| ());
        };
        paint(&mut p, 0.);
        tap(&mut p, "toggle");
        let card = motion_node(p.host().kernel().find_by_test_id("card")[0]);
        assert_eq!(paint(&mut p, 0.).0, (1, vec![card]));
        tap(&mut p, "open-detail");
        let ((flights, _), _) = paint(&mut p, 0.);
        assert!(p.stage_moving());
        assert_eq!(flights, 1, "the route still shows while it crosses out");
        paint(&mut p, 500.);
        let (lifecycle, _) = paint(&mut p, 16.);
        assert!(!p.stage_moving());
        assert_eq!(
            lifecycle,
            (0, vec![card]),
            "the covered route's flight ends though no commit hid it; its name stays alive"
        );
        assert!(p.host().flight_frames().is_empty());
        // A commit while the route is covered must not retire its names: a
        // Back preview draws the route again with no commit of its own.
        tap(&mut p, "bump");
        paint(&mut p, 0.);
        let now = p.host().now();
        assert!(p.back_progress(0, 0., 0, now));
        assert!(p.back_progress(1, 0.5, 0, now + 50.));
        let ((_, alive), ops) = paint(&mut p, 50.);
        let drawn: Vec<_> = sets(&ops)
            .iter()
            .map(|def| u64::from(def[0]) | u64::from(def[1]) << 32)
            .collect();
        assert_eq!(drawn, [card], "the preview records the route beneath");
        assert!(
            alive.contains(&card),
            "the reader discards a definition whose name it was told is gone"
        );
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
            let parents = 31 + def[30] as usize;
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
                let parents = h + 31 + ops[h + 30] as usize;
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
        assert!(
            sets(&first).iter().all(|def| def[29] == 2f32.to_bits()),
            "a definition says how many device pixels a point covers"
        );
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
        assert_eq!(after[16], 2, "only the arriving names remain alive");
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
