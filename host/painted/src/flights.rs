//! Android shared-element curves. The reader owns the live views and their
//! displayed geometry; this host owns only identity and time (LLP 1013.000 D4).
use super::*;
use exact_kernel::CommitReceipt;

const FLIGHT: u64 = 1 << 63;
/// Why a flight's content takes no focus until it lands.
pub(crate) const IN_FLIGHT: &str = "in a shared-element flight";
const END: f64 = 1000.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FlightFrame {
    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    pub from: NodeKey,
    pub to: NodeKey,
    pub progress: f32,
    pub ancestor_opacity: f32,
}

struct Flight {
    frame: FlightFrame,
    lands_after: Option<f64>,
}

#[derive(Default)]
pub(crate) struct Flights {
    pub enabled: bool,
    /// Definitions recorded for the reader, including retained rows.
    pub recorded: std::collections::HashSet<u64>,
    running: Vec<Flight>,
    pub reveal: Vec<NodeKey>,
    /// A flight ended since the presenter last asked: its content may take
    /// the focus it was refused in the air.
    pub landed: bool,
    /// The node a `focus()` named while it flew, focused when it lands.
    pub focus: Option<NodeKey>,
}

impl<D: DataSource> Host<D> {
    /// The live reader starts motion at its first produced frame (LLP 1003.001).
    /// A deterministic driver instead starts pending curves at its takeover time.
    pub(crate) fn reader_frame_clock(&mut self, live: bool) {
        let live = live && self.flights.enabled;
        if self.engine.starts_on_frame() == live {
            return;
        }
        let at = (self.now_ms / 1000.0).max(self.engine.now());
        let started = self
            .engine
            .set_start_on_frame(live, at)
            .expect("finite host clock");
        self.reader_started(&started);
        self.present();
    }

    #[cfg(any(target_os = "android", test))]
    pub(crate) fn present_reader_frame(&mut self, now: f64) -> bool {
        if !self.engine.starts_on_frame() {
            return false;
        }
        let started = self
            .engine
            .present_frame(now / 1000.0)
            .expect("finite frame clock");
        self.reader_started(&started);
        self.present()
    }

    fn reader_started(&mut self, nodes: &[u64]) {
        if !nodes.is_empty() {
            self.lowered_epoch += 1;
            for node in nodes {
                self.lowered_changed.insert(*node, self.lowered_epoch);
            }
        }
    }

    pub(crate) fn start_flights(&mut self, receipt: &CommitReceipt) {
        if !self.flights.enabled {
            return;
        }
        let engine = &mut self.engine;
        let before = self.flights.running.len();
        self.flights.running.retain(|f| {
            let keep =
                !receipt.destroyed.contains(&f.frame.to) && !receipt.renewed.contains(&f.frame.to);
            if !keep {
                engine.remove(motion_node(f.frame.to) | FLIGHT);
            }
            keep
        });
        self.flights.landed |= self.flights.running.len() != before;
        for h in &receipt.handoffs {
            if !self.flights.recorded.contains(&motion_node(h.from))
                || self.runner.kernel().node_by_key(h.to).is_none()
            {
                continue;
            }
            let node = motion_node(h.to) | FLIGHT;
            let declared = engine.set_layout_transition(
                node,
                &exact_motion::Transitions(vec![h.transition.clone()]),
            );
            debug_assert!(declared.is_ok());
            for x in [0.0, END] {
                let observed = engine.observe(exact_motion::Change {
                    node,
                    property: Property::Layout,
                    value: exact_motion::Value::four(x, 0.0, 0.0, 0.0),
                    velocity: None,
                });
                debug_assert!(observed.is_ok());
            }
            let lands_after = match &h.transition.timing {
                exact_motion::TimingFunction::Spring(config) => {
                    land_after(config).map(|t| t - h.transition.delay.min(0.0))
                }
                _ => None,
            };
            self.flights.running.push(Flight {
                frame: FlightFrame {
                    from: h.from,
                    to: h.to,
                    progress: 0.0,
                    ancestor_opacity: 1.0,
                },
                lands_after,
            });
            self.flights.reveal.push(h.to);
        }
    }

    pub(super) fn present_flight(&mut self, p: &exact_motion::Presentation) -> bool {
        if p.node & FLIGHT == 0 {
            return false;
        }
        if let Some(f) = self
            .flights
            .running
            .iter_mut()
            .find(|f| motion_node(f.frame.to) | FLIGHT == p.node)
        {
            f.frame.progress = (p.value.x / END) as f32;
        }
        true
    }

    pub(super) fn land_flights(&mut self) -> bool {
        let before = self.flights.running.len();
        let engine = &mut self.engine;
        let now = engine.sample_time();
        self.flights.running.retain(|f| {
            let node = motion_node(f.frame.to) | FLIGHT;
            let landed = f
                .lands_after
                .zip(engine.curve_start(node, Property::Layout))
                .is_some_and(|(t, start)| now >= start + t);
            let flying = engine.is_active(node, Property::Layout) && !landed;
            if !flying {
                engine.remove(node);
                self.row_dirty.node(f.frame.to);
            }
            flying
        });
        let landed = self.flights.running.len() != before;
        self.flights.landed |= landed;
        landed
    }

    /// A new reader holds no source and no definition of the last one's, so
    /// it can draw no flight: each ends, and the next handoff waits for a
    /// source recorded for this reader.
    pub(crate) fn reader_lost_flights(&mut self) {
        for f in self.flights.running.drain(..) {
            self.engine.remove(motion_node(f.frame.to) | FLIGHT);
            self.row_dirty.node(f.frame.to);
            self.flights.landed = true;
        }
        self.flights.recorded.clear();
        // Nor does it reveal an arrival that no longer flies.
        self.flights.reveal.clear();
    }

    /// Reduced motion changed: a flight's destination draws its own corners
    /// under it, so each is recorded again.
    pub(crate) fn flights_restyled(&mut self, changed: bool) {
        for f in self.flights.running.iter().filter(|_| changed) {
            self.row_dirty.node(f.frame.to);
        }
    }

    /// A destination the last paint found not rendering (`display: none`, a
    /// hidden route) flies no longer, so its flight ends with the reader's.
    pub(crate) fn retire_unrendered_flights(&mut self, unrendered: &[NodeKey]) {
        let engine = &mut self.engine;
        let row_dirty = &mut self.row_dirty;
        let before = self.flights.running.len();
        self.flights.running.retain(|f| {
            let rendered = !unrendered.contains(&f.frame.to);
            if !rendered {
                engine.remove(motion_node(f.frame.to) | FLIGHT);
                row_dirty.node(f.frame.to);
            }
            rendered
        });
        self.flights.landed |= self.flights.running.len() != before;
    }

    pub(crate) fn flight_frames(&self) -> Vec<FlightFrame> {
        self.flights
            .running
            .iter()
            .map(|f| {
                let mut frame = f.frame;
                let mut next = self.kernel().node_by_key(frame.to).and_then(|n| n.parent);
                while let Some(node) = next.and_then(|id| self.kernel().node(id)) {
                    let shown = self.presented(node.id);
                    // The reader samples lowered ancestor layers independently.
                    if shown.lowered & lower::LOWER_OPACITY == 0 {
                        frame.ancestor_opacity *= shown.opacity.clamp(0., 1.);
                    }
                    next = node.parent;
                }
                frame
            })
            .collect()
    }

    pub(crate) fn flying(&self, id: ViewId) -> bool {
        if self.flights.running.is_empty() || self.runner.viewport().preferences.reduced_motion {
            return false;
        }
        let mut next = Some(id);
        while let Some(node) = next.and_then(|id| self.kernel().node(id)) {
            if self.flights.running.iter().any(|f| f.frame.to == node.key) {
                return true;
            }
            next = node.parent;
        }
        false
    }
}

// Match the Apple host's landing threshold, including underdamped springs.
fn land_after(config: &exact_motion::SpringConfig) -> Option<f64> {
    let within = |n: usize| {
        let s = config.sample(1.0, 0.0, n as f64 / 240.0);
        s.displacement.abs() < 1e-3 && s.velocity.abs() < 0.05
    };
    within(2400).then(|| {
        (0..2400)
            .rev()
            .find(|&n| !within(n))
            .map_or(0.0, |n| (n + 1) as f64 / 240.0)
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::presenter::{PainterChoice, Presenter};

    const APP: &str = r#"component App
  state open = false
  state gone = false
  action toggle
    open = not open
  action remove
    gone = true
  view
    column
      button "Toggle" press=toggle testId="toggle"
      button "Remove" press=remove testId="remove"
      if not gone
        if open
          column sharedElement="card" width=300 height=200 testId="detail" -exact-layout-transition="1s linear"
            button "Inside" press=toggle testId="inside"
        else
          view sharedElement="card" width=80 height=60 testId="thumb" -exact-layout-transition="1s linear"
"#;

    fn id(host: &Host<()>, name: &str) -> ViewId {
        host.kernel()
            .node_by_key(host.kernel().find_by_test_id(name)[0])
            .unwrap()
            .id
    }
    fn presenter(app: &str) -> Presenter<()> {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (400., 700.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.frame();
        p
    }

    #[test]
    fn an_unrecorded_source_does_not_disable_its_arrival() {
        for (style, paint_source) in [("display=\"none\"", true), ("", false)] {
            let app = APP.replace(
                "view sharedElement=\"card\" width=80",
                &format!("view {style} sharedElement=\"card\" width=80"),
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
            if paint_source {
                p.frame();
            }
            let toggle = id(p.host(), "toggle");
            p.host_mut().dispatch_at(toggle, Event::Press, 100.);
            p.after_commit();
            p.frame();
            let child = id(p.host(), "inside");
            let rect = p.boxes().iter().find(|b| b.id == child).unwrap().rect;
            assert_eq!(
                p.hit(rect.0 + 5., rect.1 + 5.),
                Some(child),
                "an arrival with no captured source is immediately interactive"
            );
            assert!(p.access_tree().0.iter().any(|n| n.view == child));
            assert!(p.host().flight_frames().is_empty());
        }
    }

    #[test]
    fn retained_sources_fly_but_reload_does_not_reuse_old_definitions() {
        let app = APP.replace("    column\n", "    scroll width=400 height=700\n");
        let mut p = presenter(&app);
        p.repaint();
        p.frame();
        let toggle = id(p.host(), "toggle");
        p.tap(toggle).unwrap();
        assert_eq!(p.host().flight_frames().len(), 1);
        p.frame();
        p.tap(toggle).unwrap();
        p.frame();
        p.reload(
            &crate::test_plan(&APP.replace("width=80", "display=\"none\" width=80")),
            (),
        )
        .unwrap();
        p.frame();
        let toggle = id(p.host(), "toggle");
        p.tap(toggle).unwrap();
        p.frame();
        assert!(p.host().flight_frames().is_empty());
        let child = id(p.host(), "inside");
        assert!(p.access_tree().0.iter().any(|n| n.view == child));
    }

    #[test]
    fn the_surface_loop_advances_without_canvas_reader_frames() {
        let app = r#"component App
  state faded = false
  action fade
    faded = true
  view
    column
      button "Fade" press=fade testId="fade"
      box width=60 height=60 opacity=(faded ? 0 : 1) transition="opacity 1s linear" testId="box"
"#;
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(app),
            (),
            (200., 200.),
            1.,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        // Reproduce the former Android Host boot flag on a headless surface painter.
        p.host_mut().flights.enabled = true;
        p.set_live_clock(Some(std::rc::Rc::new(|| 0.)));
        p.frame();
        let button = id(p.host(), "fade");
        p.host_mut().dispatch_at(button, Event::Press, 100.);
        p.after_commit();
        let mut last_tick = 0.;
        p.android_clock(600., true, &mut last_tick, 16.);
        p.tick(600.);
        let box_id = id(p.host(), "box");
        assert!((p.host().presented(box_id).opacity - 0.5).abs() < 1e-5);
        p.android_clock(1300., true, &mut last_tick, 16.);
        p.tick(1300.);
        assert_eq!(p.host().presented(box_id).opacity, 0.);
        assert!(!p.host().engine().starts_on_frame());
        assert!(!p.host().flights.enabled);
    }

    #[test]
    fn flights_sample_static_ancestor_opacity_even_when_no_row_records() {
        for scroll in [false, true] {
            let app = format!(
                r#"keyframes fade
  from opacity=0.5
  to opacity=0.25
component App
  state open = false
  state alpha = 0.5
  action toggle
    open = not open
  action fade
    alpha = alpha - 0.25
  view
    column
      button "Toggle" press=toggle testId="toggle"
      button "Fade" press=fade testId="fade"
      column opacity=alpha
        {} width=400 height=500
          column
            if open
              column animation="fade 1s linear infinite"
                box sharedElement="card" width=200 height=150 background-color="red" -exact-layout-transition="1s linear"
            else
              box sharedElement="card" width=80 height=60 background-color="red" -exact-layout-transition="1s linear"
"#,
                if scroll { "scroll" } else { "column" }
            );
            let mut p = presenter(&app);
            p.host_mut().lowering = true;
            p.host_mut()
                .engine
                .set_lowered_properties(&[Property::Opacity]);
            let toggle = id(p.host(), "toggle");
            let fade = id(p.host(), "fade");
            p.tap(toggle).unwrap();
            p.frame();
            for expected in [0.5, 0.25, 0.] {
                let flights = p.host().flight_frames();
                assert_eq!(flights.len(), 1);
                assert_eq!(
                    flights[0].ancestor_opacity, expected,
                    "only static ancestry enters the frame; live alpha is sampled by the reader"
                );
                p.tap(fade).unwrap();
                p.frame();
            }
        }
    }

    #[test]
    fn shared_content_keeps_descendant_opacity_layers_alive() {
        let app = r#"keyframes fade
  from opacity=1
  to opacity=0
component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      scroll width=400 height=600
        column
          box height=370
          if open
            column sharedElement="body" width=300 -exact-layout-transition="1s linear"
              text "Title" sharedElement="title"
              box position="relative" height=104
                text "•••" position="absolute" left=0 right=0 top=24 animation="fade 650ms linear both" testId="dots"
                column
                  text "Loaded"
          else
            box sharedElement="body" width=100 height=1 -exact-layout-transition="1s linear"
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
        p.host_mut().lowering = true;
        p.host_mut()
            .engine
            .set_lowered_properties(&[Property::Opacity]);
        p.frame();
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.after_commit();
        p.frame();
        let dots = id(p.host(), "dots");
        let key = p.host().kernel().node(dots).unwrap().key;
        assert_ne!(p.host().lowered_mask(key), 0);
        assert!(
            crate::canvas::LIVE_LAYERS
                .with(|a| a.borrow().iter().any(|(_, k, _)| *k == motion_node(key))),
            "the reader needs the child's layer to animate live shared content"
        );
        p.host_mut().dispatch_at(toggle, Event::Press, 1000.0);
        p.after_commit();
        p.frame();
        assert!(
            crate::canvas::LIVE_LAYERS
                .with(|a| a.borrow().iter().all(|(_, k, _)| *k != motion_node(key))),
            "a removed child's layer is freed after its last drawing"
        );
    }

    #[test]
    fn a_delayed_first_reader_frame_starts_at_the_source_and_takeover_settles() {
        let mut p = presenter(APP);
        p.set_live_clock(Some(std::rc::Rc::new(|| 0.)));
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.tick(250.0);
        assert_eq!(p.host().flight_frames()[0].progress, 0.0);
        p.reader_clock(250.0, true);
        p.frame();
        assert_eq!(p.host().flight_frames()[0].progress, 0.0);
        p.reader_clock(266.0, true);
        assert!((p.host().flight_frames()[0].progress - 0.016).abs() < 1e-5);
        p.host_mut().dispatch_at(toggle, Event::Press, 300.0);
        assert_eq!(p.host().flight_frames().len(), 1);
        p.set_live_clock(None);
        p.tick(1301.0);
        assert!(p.host().flight_frames().is_empty());
    }

    #[test]
    fn flight_clock_reverses_and_retires_with_the_destination() {
        let mut p = presenter(APP);
        let source = p.host().kernel().node(id(p.host(), "thumb")).unwrap().key;
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.after_commit();
        p.frame();
        let first = p.host().flight_frames();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].from, source);
        assert_eq!(first[0].progress, 0.0);
        assert!(p.host().motion());
        p.tick(500.0);
        assert!((p.host().flight_frames()[0].progress - 0.4).abs() < 1e-5);
        p.host_mut().dispatch_at(toggle, Event::Press, 500.0);
        let reversed = p.host().flight_frames();
        assert_eq!(reversed.len(), 1);
        assert_eq!(reversed[0].from, first[0].to);
        assert_eq!(reversed[0].progress, 0.0);
        assert!(!p
            .host()
            .engine()
            .is_active(motion_node(first[0].to) | FLIGHT, Property::Layout));
        let remove = id(p.host(), "remove");
        p.host_mut().dispatch_at(remove, Event::Press, 600.0);
        assert!(p.host().flight_frames().is_empty());
        assert!(!p.host().motion());
    }

    #[test]
    fn flight_suppresses_descendant_hits_and_accessibility_until_landing() {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(APP),
            (),
            (400.0, 700.0),
            1.0,
            std::path::PathBuf::new(),
            PainterChoice::Canvas,
        )
        .unwrap();
        assert!(error.is_none());
        p.frame();
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.after_commit();
        p.frame();
        let child = id(p.host(), "inside");
        assert!(p.host().flying(child));
        let rect = p.boxes().iter().find(|b| b.id == child).unwrap().rect;
        assert_ne!(p.hit(rect.0 + 5.0, rect.1 + 5.0), Some(child));
        let (ax, _, _) = p.access_tree();
        assert!(!ax.iter().any(|n| n.view == child));
        p.tick(1101.0);
        p.frame();
        assert!(!p.host().flying(child));
        assert!(p.host().flight_frames().is_empty());
        assert_eq!(p.hit(rect.0 + 5.0, rect.1 + 5.0), Some(child));
        let (ax, _, _) = p.access_tree();
        assert!(ax.iter().any(|n| n.view == child));
    }

    #[test]
    fn flying_controls_take_no_keyboard_focus_or_activation() {
        let app = APP.replace(
            "press=toggle testId=\"inside\"",
            "press=toggle aria-keyshortcuts=\"Alt+K\" testId=\"inside\"",
        );
        let mut p = presenter(&app);
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.after_commit();
        p.frame();
        let child = id(p.host(), "inside");
        assert!(p.host().flying(child));
        let pressed = |p: &Presenter<()>| p.host().kernel().find_by_test_id("inside").is_empty();
        // Tab and a remote's arrows pass over the flying button.
        for _ in 0..4 {
            p.key_down("Tab", 200.0);
            assert_ne!(p.focus(), Some(child), "Tab stops on a flying control");
        }
        assert_eq!(p.focus_refusal(child), Some(IN_FLIGHT));
        // Nor does the focus reach it any other way: a `focus()`, an addressed key.
        assert!(p.set_focus(Some(child), 300.0).is_some());
        assert!(p.type_key(child, "Enter", "Enter", true, false).is_err());
        assert_ne!(p.focus(), Some(child));
        // Its shortcut presses nothing while it flies.
        p.hardware_key("AltLeft", "Alt", true, false);
        p.hardware_key("KeyK", "k", true, false);
        p.hardware_key("KeyK", "k", false, false);
        p.hardware_key("AltLeft", "Alt", false, false);
        assert!(!pressed(&p), "a shortcut pressed a flying button");
        // A focus left on it (a restored one) activates nothing until it lands.
        p.focus = Some(child);
        for (key, code) in [("Enter", "Enter"), (" ", "Space"), ("Enter", "Select")] {
            p.key_down_with(key, code, false, 400.0);
            p.key_up(key, code, 410.0);
            assert!(!pressed(&p), "{code} pressed a flying button");
        }
        p.tick(1101.0);
        p.frame();
        assert!(!p.host().flying(child));
        assert_eq!(p.focus_refusal(child), None);
        p.key_down("Enter", 1200.0);
        p.key_up("Enter", "Enter", 1210.0);
        assert!(pressed(&p), "a landed button is pressed by Enter");
    }

    #[test]
    fn a_focus_refused_in_the_air_is_taken_at_landing() {
        let field = "            input autofocus testId=\"inside\"\n";
        let autofocus = APP.replace(
            "            button \"Inside\" press=toggle testId=\"inside\"\n",
            field,
        );
        // A `focus()` in the action that opens the destination, with no `autofocus`.
        let asked = autofocus
            .replace("input autofocus testId", "input id=\"field\" testId")
            .replace(
                "    open = not open\n",
                "    open = not open\n    focus(\"field\")\n",
            );
        for (app, how) in [(&autofocus, "autofocus"), (&asked, "focus()")] {
            // Landed under a driven clock, a live reader's, and by a reader made again.
            for end in ["driven", "live", "replaced"] {
                let mut p = presenter(app);
                if end != "driven" {
                    p.set_live_clock(Some(std::rc::Rc::new(|| 0.)));
                }
                let toggle = id(p.host(), "toggle");
                p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
                p.after_commit();
                p.run_commands(|| ());
                p.reader_clock(116.0, end != "driven");
                p.frame();
                let field = id(p.host(), "inside");
                assert!(p.host().flying(field), "{how} {end}");
                assert_ne!(p.focus(), Some(field), "a flying field took the focus");
                match end {
                    "driven" => {
                        p.tick(1301.0);
                        p.reader_clock(1301.0, false);
                    }
                    "live" => {
                        p.reader_clock(700.0, true);
                        assert!(p.host().flying(field));
                        assert_ne!(p.focus(), Some(field));
                        p.reader_clock(1300.0, true);
                    }
                    _ => {
                        p.reader_replaced(Box::new(crate::canvas::Recorder::new()));
                        p.reader_clock(132.0, true);
                    }
                }
                assert!(!p.host().flying(field), "{how} {end}");
                assert_eq!(p.focus(), Some(field), "{how} {end}");
            }
        }
        // A handoff with no duration has no flight to wait for.
        let mut p = presenter(&autofocus.replace("1s linear", "0s linear"));
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.after_commit();
        let field = id(p.host(), "inside");
        assert!(!p.host().flying(field));
        assert_eq!(p.focus(), Some(field));
    }

    #[test]
    fn android_spring_lands_at_the_apple_hosts_threshold() {
        let source = APP.replace("1s linear", "-exact-spring(270, 33, 1)");
        let mut p = presenter(&source);
        let toggle = id(p.host(), "toggle");
        p.host_mut().dispatch_at(toggle, Event::Press, 100.0);
        p.tick(300.0);
        assert_eq!(p.host().flight_frames().len(), 1);
        p.tick(1100.0);
        assert!(p.host().flight_frames().is_empty());
        assert!(!p.host().motion());
    }

    #[test]
    fn display_submission_reveals_a_new_arrival_before_the_first_flight_stream() {
        for (gap, expected) in [(60, 0.), (260, 150.)] {
            let app = format!(
                r#"component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      if open
        scroll width=300 height=200 testId="port"
          column
            box height={gap}
            view sharedElement="card" testId="arrival" width=120 height=90 -exact-layout-transition="1s linear"
            box height=600
      else
        view sharedElement="card" width=120 height=90 -exact-layout-transition="1s linear"
"#
            );
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
            let first = p.display_frame().unwrap();
            assert!(p.display_complete(&first));
            p.tap(id(p.host(), "toggle")).unwrap();
            let port = id(p.host(), "port");
            let arrival = id(p.host(), "arrival");
            assert_eq!(p.host().flights.reveal.len(), 1);
            let next = p.display_frame().unwrap();
            assert_eq!(
                p.scroll_of(port).1,
                expected,
                "reveal uses the new scroller before its stream is submitted"
            );
            assert!(p.host().flights.reveal.is_empty());
            assert!(
                !p.boxes().iter().any(|b| b.id == arrival),
                "input still sees the acknowledged picture"
            );
            assert!(p.display_complete(&next));
            let boxes = p.boxes();
            let target = boxes.iter().find(|b| b.id == arrival).unwrap().rect;
            let port = boxes.iter().find(|b| b.id == port).unwrap().rect;
            assert!(target.1 >= port.1 && target.1 + target.3 <= port.1 + port.3);
        }
    }

    #[test]
    fn flights_reveal_nearest_without_moving_an_already_visible_target() {
        let app = r#"component App
  state open = false
  action toggle
    open = not open
  view
    column
      button "Toggle" press=toggle testId="toggle"
      box height=100
      scroll width=300 height=200 testId="port"
        column
          box height=60
          if open
            view sharedElement="card" width=120 height=90 -exact-layout-transition="1s linear"
          else
            view sharedElement="card" width=120 height=90 -exact-layout-transition="1s linear"
          box height=600
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
        p.frame();
        let port = id(p.host(), "port");
        assert_eq!(p.scroll_of(port), (0., 0.));
        p.tap(id(p.host(), "toggle")).unwrap();
        p.frame();
        assert_eq!(p.host().flight_frames().len(), 1);
        assert_eq!(
            p.scroll_of(port),
            (0., 0.),
            "a port below the header must not scroll a visible arrival"
        );
    }
}
