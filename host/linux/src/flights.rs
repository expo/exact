//! Android shared-element curves. The reader owns the live views and their
//! displayed geometry; this host owns only identity and time (LLP 1013.000 D4).
use super::*;
use exact_kernel::CommitReceipt;

const FLIGHT: u64 = 1 << 63;
const END: f64 = 1000.0;

#[derive(Clone, Copy, Debug)]
pub(crate) struct FlightFrame {
    #[cfg_attr(not(any(target_os = "android", test)), allow(dead_code))]
    pub from: NodeKey,
    pub to: NodeKey,
    pub progress: f32,
}

struct Flight {
    frame: FlightFrame,
    lands_after: Option<f64>,
}

#[derive(Default)]
pub(crate) struct Flights {
    pub enabled: bool,
    running: Vec<Flight>,
    pub reveal: Vec<NodeKey>,
}

impl<D: DataSource> Host<D> {
    /// The live reader starts motion at its first produced frame (LLP 1003.001).
    /// A deterministic driver instead starts pending curves at its takeover time.
    pub(crate) fn reader_frame_clock(&mut self, live: bool) {
        if !self.flights.enabled || self.engine.starts_on_frame() == live {
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
        self.flights.running.retain(|f| {
            let keep =
                !receipt.destroyed.contains(&f.frame.to) && !receipt.renewed.contains(&f.frame.to);
            if !keep {
                engine.remove(motion_node(f.frame.to) | FLIGHT);
            }
            keep
        });
        for h in &receipt.handoffs {
            if self.runner.kernel().node_by_key(h.to).is_none() {
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
            }
            flying
        });
        self.flights.running.len() != before
    }

    pub(crate) fn flight_frames(&self) -> Vec<FlightFrame> {
        self.flights.running.iter().map(|f| f.frame).collect()
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
    fn host() -> Host<()> {
        let (mut host, error) = Host::boot(
            &crate::test_plan(APP),
            (),
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.0,
            700.0,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        host.flights.enabled = true;
        host
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
        p.host_mut().flights.enabled = true;
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
        let mut host = host();
        host.reader_frame_clock(true);
        host.dispatch_at(id(&host, "toggle"), Event::Press, 100.0);
        host.tick(250.0);
        assert_eq!(host.flight_frames()[0].progress, 0.0);
        host.present_reader_frame(250.0);
        assert_eq!(host.flight_frames()[0].progress, 0.0);
        host.tick(266.0);
        host.present_reader_frame(266.0);
        assert!((host.flight_frames()[0].progress - 0.016).abs() < 1e-5);
        host.dispatch_at(id(&host, "toggle"), Event::Press, 300.0);
        host.reader_frame_clock(false);
        host.tick(1301.0);
        assert!(host.flight_frames().is_empty());
    }

    #[test]
    fn flight_clock_reverses_and_retires_with_the_destination() {
        let mut host = host();
        let source = host.kernel().node(id(&host, "thumb")).unwrap().key;
        host.dispatch_at(id(&host, "toggle"), Event::Press, 100.0);
        let first = host.flight_frames();
        assert_eq!(first.len(), 1);
        assert_eq!(first[0].from, source);
        assert_eq!(first[0].progress, 0.0);
        assert!(host.motion());
        host.tick(500.0);
        assert!((host.flight_frames()[0].progress - 0.4).abs() < 1e-5);
        host.dispatch_at(id(&host, "toggle"), Event::Press, 500.0);
        let reversed = host.flight_frames();
        assert_eq!(reversed.len(), 1);
        assert_eq!(reversed[0].from, first[0].to);
        assert_eq!(reversed[0].progress, 0.0);
        assert!(!host
            .engine
            .is_active(motion_node(first[0].to) | FLIGHT, Property::Layout));
        host.dispatch_at(id(&host, "remove"), Event::Press, 600.0);
        assert!(host.flight_frames().is_empty());
        assert!(!host.motion());
    }

    #[test]
    fn flight_suppresses_descendant_hits_and_accessibility_until_landing() {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(APP),
            (),
            (400.0, 700.0),
            1.0,
            std::path::PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none());
        p.host_mut().flights.enabled = true;
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
    fn android_spring_lands_at_the_apple_hosts_threshold() {
        let source = APP.replace("1s linear", "-exact-spring(270, 33, 1)");
        let mut host = Host::boot(
            &crate::test_plan(&source),
            (),
            Box::new(exact_kernel::MonospaceMeasurer::default()),
            400.0,
            700.0,
        )
        .unwrap()
        .0;
        host.flights.enabled = true;
        host.dispatch_at(id(&host, "toggle"), Event::Press, 100.0);
        host.tick(300.0);
        assert_eq!(host.flight_frames().len(), 1);
        host.tick(1100.0);
        assert!(host.flight_frames().is_empty());
        assert!(!host.motion());
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
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.host_mut().flights.enabled = true;
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
