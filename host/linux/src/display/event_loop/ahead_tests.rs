use super::*;
use exact_kernel::ViewId;
use exact_painted::presenter::PainterChoice;
use std::cell::RefCell;
use std::collections::VecDeque;
use std::rc::Rc;

const APP: &str = r##"component App
  state step = 0
  state chosen = false
  action next
    step = step + 1
  action choose
    chosen = true
  view
    column width=80 height=60
      box testId="target" press=choose margin-left=(step * 20) width=20 height=20 background-color=(step == 0 ? "#ff0000" : step == 1 ? "#00ff00" : "#0000ff")
      box testId="next" press=next width=20 height=20
      box width=20 height=20 background-color=(chosen ? "#ffff00" : "#000000")
"##;

enum Wake {
    Input,
    Shown(usize),
    Lost,
    Stop,
}

#[derive(Default)]
struct Trace {
    submitted: Vec<Arc<Pixmap>>,
    published: Vec<Arc<Pixmap>>,
    completed: Vec<usize>,
    lost: Vec<usize>,
    waits: Vec<(usize, i32)>,
    ready_calls: usize,
}

struct OutputState {
    direct: bool,
    pending: VecDeque<(usize, PanelFrame)>,
    script: VecDeque<Wake>,
    readable: usize,
    failed: bool,
}

struct QueuedOutput {
    state: RefCell<OutputState>,
    trace: Rc<RefCell<Trace>>,
}

impl Output for QueuedOutput {
    fn direct(&self) -> bool {
        self.state.borrow().direct
    }
    fn refresh(&self) -> u32 {
        60
    }
    fn period_ms(&self) -> f64 {
        1000. / 60.
    }
    fn pending(&self) -> bool {
        !self.state.borrow().pending.is_empty()
    }
    fn can_submit(&self) -> bool {
        let state = self.state.borrow();
        state.pending.len() < if state.direct { 2 } else { 1 }
    }
    fn sequence(&self) -> Option<u32> {
        None
    }
    fn last_flip(&self) -> Option<Duration> {
        None
    }
    fn fd(&self) -> i32 {
        -1
    }
    fn submit(&mut self, frame: PanelFrame) -> Result<Completion, String> {
        assert!(
            self.can_submit(),
            "a full output must not be submitted again"
        );
        let mut trace = self.trace.borrow_mut();
        trace
            .submitted
            .push(frame.picture.mirror().unwrap().clone());
        let id = trace.submitted.len();
        let state = self.state.get_mut();
        if state.direct {
            state.pending.push_back((id, frame));
            Ok(Completion::Pending)
        } else {
            assert!(state.pending.is_empty());
            trace.completed.push(id);
            Ok(Completion::Shown(frame))
        }
    }
    fn ready(&mut self) -> Result<Completion, String> {
        let state = self.state.get_mut();
        assert!(
            state.readable > 0,
            "ready must follow a readable notification"
        );
        state.readable -= 1;
        let mut trace = self.trace.borrow_mut();
        trace.ready_calls += 1;
        if state.failed {
            state.direct = false;
            return Ok(Completion::Retry(
                state
                    .pending
                    .drain(..)
                    .map(|(id, frame)| {
                        trace.lost.push(id);
                        frame
                    })
                    .collect(),
            ));
        }
        let (id, frame) = state.pending.pop_front().unwrap();
        trace.completed.push(id);
        Ok(Completion::Shown(frame))
    }
    fn wait(&self, fds: &[i32], timeout: i32) -> Result<bool, String> {
        let mut state = self.state.borrow_mut();
        if fds.len() == 1 {
            assert_eq!(timeout, 0, "draining must not block");
            return Ok(state.readable > 0);
        }
        self.trace
            .borrow_mut()
            .waits
            .push((state.pending.len(), timeout));
        assert_eq!(
            state.readable, 0,
            "all available ACKs precede the next turn"
        );
        match state
            .script
            .pop_front()
            .expect("the loop exceeded its bounded script")
        {
            Wake::Input => Ok(false),
            Wake::Shown(count) => {
                assert!(count <= state.pending.len());
                state.readable = count;
                Ok(true)
            }
            Wake::Lost => {
                assert!(!state.pending.is_empty());
                state.readable = 1;
                state.failed = true;
                Ok(true)
            }
            Wake::Stop => Err("test display closed".into()),
        }
    }
}

struct Observer<F> {
    trace: Rc<RefCell<Trace>>,
    turn: usize,
    input: F,
}

impl<F: FnMut(&mut Presenter<()>, usize, &Trace)> Events<()> for Observer<F> {
    fn fds(&self) -> Vec<i32> {
        Vec::new()
    }
    fn idle(&self) -> bool {
        false
    }
    fn publish(&mut self, pixels: Arc<Pixmap>) {
        self.trace.borrow_mut().published.push(pixels);
    }
    fn turn(&mut self, p: &mut Presenter<()>, _: impl Fn() -> f64) {
        (self.input)(p, self.turn, &self.trace.borrow());
        self.turn += 1;
    }
    fn date_jumped(&self) -> bool {
        false
    }
    fn trace_requested(&self) -> bool {
        false
    }
}

fn boot(source: &str) -> (Presenter<()>, Config) {
    let plan = contract::compile(source).unwrap().encode();
    let previous = exact_runner::page::set_awaits_present(true);
    let (mut p, error) = Presenter::boot_with(
        &plan,
        (),
        (80., 60.),
        1.,
        Path::new("").to_path_buf(),
        PainterChoice::Cpu,
    )
    .unwrap();
    exact_runner::page::set_awaits_present(previous);
    assert!(error.is_none(), "{error:?}");
    p.presents_pictures(true);
    p.set_live_clock(Some(Rc::new(|| 0.)));
    p.set_pointer(None);
    let mut config = Config::from_env(&plan, "{}");
    config.dev_plan = None;
    config.dev_url = None;
    (p, config)
}

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn next(p: &mut Presenter<()>) {
    let next = named(p, "next");
    assert!(p
        .host_mut()
        .dispatch_at(next, exact_runner::Event::Press, 0.)
        .is_none());
    p.repaint();
}

fn run_script(script: Vec<Wake>, input: impl FnMut(&mut Presenter<()>, usize, &Trace)) -> Trace {
    let (p, config) = boot(APP);
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut output = QueuedOutput {
        state: RefCell::new(OutputState {
            direct: true,
            pending: VecDeque::new(),
            script: script.into(),
            readable: 0,
            failed: false,
        }),
        trace: trace.clone(),
    };
    let mut events = Observer {
        trace: trace.clone(),
        turn: 0,
        input,
    };
    assert_eq!(
        run(
            &config,
            Instant::now(),
            p,
            &mut output,
            &mut events,
            LaunchMarks::default(),
            || ()
        ),
        1
    );
    assert!(output.state.borrow().script.is_empty());
    assert!(
        !output.pending(),
        "the final frame was actually acknowledged"
    );
    drop(events);
    drop(output);
    Rc::try_unwrap(trace).ok().unwrap().into_inner()
}

#[test]
fn ahead_completions_publish_both_hit_maps_before_input_and_keep_the_final_frame() {
    let trace = run_script(
        vec![
            Wake::Input,
            Wake::Shown(1),
            Wake::Input,
            Wake::Input,
            Wake::Shown(2),
            Wake::Shown(1),
            Wake::Stop,
        ],
        |p, turn, trace| match turn {
            0 => {
                assert!(trace.published.is_empty(), "submit is not first pixel");
                assert!(!p.host().runner().presented().seen);
                assert!(p.hit(5., 5.).is_none());
                p.repaint();
            }
            1 => {
                assert_eq!(trace.completed, [1]);
                assert!(p.host().runner().presented().seen);
                assert_eq!(p.hit(5., 5.), Some(named(p, "target")));
                next(p);
            }
            2 => {
                assert_eq!(trace.submitted.len(), 2);
                assert_eq!(trace.completed, [1]);
                next(p);
            }
            3 => {
                assert_eq!(trace.submitted.len(), 3, "one frame got ahead of B");
                assert_eq!(trace.completed, [1]);
                p.repaint();
            }
            4 => {
                assert_eq!(
                    trace.submitted.len(),
                    3,
                    "two pending frames bound preparation"
                );
                assert_eq!(trace.completed, [1, 2, 3]);
                assert_eq!(trace.published.len(), 3, "both ACKs precede input");
                assert_eq!(p.hit(45., 5.), Some(named(p, "target")));
                assert!(p.pointer_down(45., 5., 0.).unwrap());
                p.pointer_up(45., 5., 0.).unwrap();
            }
            5 => {
                assert_eq!(trace.completed, [1, 2, 3, 4]);
                assert!(!p.dirty());
                assert!(!p.needs_animation_frame());
            }
            _ => panic!("unexpected turn {turn}"),
        },
    );
    assert_ne!(
        trace.waits[1].1, 0,
        "waiting for the first pixel cannot spin"
    );
    assert_ne!(
        trace.waits[4].1, 0,
        "a full output cannot spin on dirty work"
    );
    assert_ne!(trace.waits[6].1, 0, "a final frame needs no recurring wake");
    assert_eq!(trace.ready_calls, 4);
    let final_pixel = trace.published.last().unwrap().pixel(5, 45).unwrap();
    assert_eq!(
        (final_pixel.red(), final_pixel.green(), final_pixel.blue()),
        (255, 255, 0)
    );
}

#[test]
fn lost_ahead_frames_are_repainted_on_the_single_pending_fallback() {
    let trace = run_script(
        vec![
            Wake::Shown(1),
            Wake::Input,
            Wake::Lost,
            Wake::Input,
            Wake::Stop,
        ],
        |p, turn, trace| match turn {
            0 | 1 => next(p),
            2 => {
                assert_eq!(trace.lost, [2, 3]);
                assert_eq!(trace.completed, [1]);
                assert_eq!(trace.published.len(), 1, "lost frames are never published");
                assert_eq!(p.hit(5., 5.), Some(named(p, "target")));
                assert!(p.dirty(), "the newest state must be repainted");
            }
            3 => {
                assert_eq!(trace.completed, [1, 4]);
                assert_eq!(trace.published.len(), 2);
                assert_eq!(p.hit(45., 5.), Some(named(p, "target")));
                assert!(!p.dirty());
            }
            _ => panic!("unexpected turn {turn}"),
        },
    );
    assert_eq!(trace.submitted.len(), 4);
    assert_eq!(trace.submitted[2].data(), trace.submitted[3].data());
    assert_eq!(trace.ready_calls, 2);
}

#[test]
fn an_animation_starts_at_its_real_present_and_its_last_ahead_picture_is_published() {
    let (mut p, _) = boot(
        r##"component App
  state faded = false
  action fade
    faded = true
  view
    column width=80 height=60
      box testId="next" press=fade width=20 height=20
      box testId="fader" opacity=(faded ? 0 : 1) transition="opacity 200ms linear" width=20 height=20 background-color="#ff0000"
"##,
    );
    let trace = Rc::new(RefCell::new(Trace::default()));
    let mut events = Observer {
        trace: trace.clone(),
        turn: 0,
        input: |_: &mut Presenter<()>, _: usize, _: &Trace| {},
    };
    let mut first = None;
    let a = p.panel_frame_ahead(true).unwrap();
    assert!(first.is_none());
    presented(&mut p, a, 0., &mut events, &mut first);
    assert!(first.is_some());

    let fader = named(&p, "fader");
    let fade = named(&p, "next");
    assert!(p
        .host_mut()
        .dispatch_at(fade, exact_runner::Event::Press, 100.)
        .is_none());
    p.repaint();
    p.tick(150.);
    let b = p.panel_frame_ahead(true).unwrap();
    p.tick(170.);
    assert_eq!(p.host().presented(fader).opacity, 1.);
    assert!(
        !p.panel_can_prepare_ahead(),
        "the held start waits for B's actual present"
    );
    assert_eq!(trace.borrow().published.len(), 1);
    presented(&mut p, b, 166., &mut events, &mut first);

    p.tick(266.);
    assert!((p.host().presented(fader).opacity - 0.5).abs() < 1e-3);
    let c = p.panel_frame_ahead(true).unwrap();
    p.tick(400.);
    assert_eq!(p.host().presented(fader).opacity, 0.);
    assert!(p.dirty(), "the settling sample still needs a picture");
    let d = p.panel_frame_ahead(true).unwrap();
    let last_pixels = d.mirror().unwrap().clone();
    assert_eq!(trace.borrow().published.len(), 2);
    presented(&mut p, c, 283., &mut events, &mut first);
    presented(&mut p, d, 416., &mut events, &mut first);
    assert_eq!(
        trace.borrow().published.last().unwrap().data(),
        last_pixels.data()
    );
    assert!(!p.dirty());
    assert!(!p.needs_animation_frame());
}
