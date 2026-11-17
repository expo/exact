use super::*;
use crate::{DrawReply, DrawRequest, Drawn, Geometry};
use exact_kernel::Kernel;
use std::cell::Cell;
use std::rc::Rc;

fn timer_runner() -> (Runner<()>, Rc<Cell<f64>>) {
    let plan = contract::compile("component App\n  state count = 0\n  state key = 0\n  action tick\n    count = count + 1\n  action rekey\n    key = key + 1\n  task clock when true key=key\n    every(100, tick)\n  view\n    text \"ticks\"\n").unwrap();
    let mut r = Runner::boot(plan, (), Kernel::with_monospace(), Default::default(), "/").unwrap();
    let time = Rc::new(Cell::new(0.));
    let sample = time.clone();
    r.use_native_timers(true);
    r.set_live_clock(Some(Box::new(move || sample.get())));
    (r, time)
}

fn timer(index: u32, generation: u64, cutoff: f64) -> Completion {
    Completion::NativeTimer {
        index,
        generation,
        cutoff,
    }
}

#[test]
fn native_timer_handoff_binds_once_and_rearming_replaces_the_subscription() {
    let (mut r, time) = timer_runner();
    assert!(r.native_timer_completion(0, 1).is_none());
    let row = r.native_timers().remove(0);
    let first = r
        .native_timer_completion(row.index, row.generation)
        .unwrap();
    let binding = first.binding.0.lock().unwrap();
    assert_eq!(
        binding.owner,
        Owner::NativeTimer {
            index: row.index,
            generation: row.generation
        }
    );
    assert_eq!(binding.class, Class::UiOwned);
    assert!(matches!(binding.protocol, Protocol::Subscription));
    assert!(!binding.taken);
    drop(binding);
    r.native_timers();
    let again = r
        .native_timer_completion(row.index, row.generation)
        .unwrap();
    assert!(Arc::ptr_eq(&first.binding.0, &again.binding.0));

    time.set(100.);
    assert!(r
        .deliver(&first, timer(row.index, row.generation, 100.))
        .unwrap()
        .is_some());
    assert_eq!(r.slot("count"), Some(&exact_plan::Value::Number(1.)));
    time.set(200.);
    let integer = r.fire_native_timer(row.index, row.generation, 200.);
    assert_eq!(integer.receipts.len(), 1);
    assert_eq!(integer.receipts[0].at_ms, 200.);
    assert_eq!(integer.now_ms, 200.);
    assert!(integer.error.is_none());
    assert!(!integer.stopped_on_request && !integer.needs_apply);
    assert_eq!(r.slot("count"), Some(&exact_plan::Value::Number(2.)));
    assert!(!first.binding.0.lock().unwrap().taken);

    r.act("rekey", vec![]).unwrap();
    assert!(r
        .native_timer_completion(row.index, row.generation)
        .is_none());
    let replacement = r.native_timers().remove(0);
    let next = r
        .native_timer_completion(replacement.index, replacement.generation)
        .unwrap();
    assert_ne!(replacement.generation, row.generation);
    assert!(!Arc::ptr_eq(&first.binding.0, &next.binding.0));
    assert!(r
        .deliver(&first, timer(row.index, row.generation, 300.))
        .unwrap()
        .is_none());
    time.set(300.);
    assert!(r
        .deliver(
            &next,
            timer(replacement.index, replacement.generation, 300.)
        )
        .unwrap()
        .is_some());
    assert_eq!(r.slot("count"), Some(&exact_plan::Value::Number(3.)));
}

#[test]
fn timer_carry_keeps_early_invalid_and_unissued_integer_delivery() {
    let (mut r, time) = timer_runner();
    let row = r.native_timers().remove(0);
    let binding = r
        .native_timer_completion(row.index, row.generation)
        .unwrap();
    assert!(r
        .deliver(&binding, timer(row.index, row.generation, 99.))
        .unwrap()
        .is_none());
    assert!(matches!(
        r.deliver(&binding, timer(row.index, row.generation, f64::NAN)),
        Err(RunnerError::NonFiniteClock)
    ));
    assert!(matches!(
        r.fire_native_timer(row.index, row.generation, f64::NAN)
            .error,
        Some(RunnerError::NonFiniteClock)
    ));
    assert_eq!(r.now_ms, 0.);
    time.set(100.);
    assert_eq!(
        r.fire_native_timer(row.index, row.generation, 100.)
            .receipts
            .len(),
        1
    );

    let (mut unissued, time) = timer_runner();
    let generation = unissued.timers[0].generation;
    assert!(unissued.timers[0].completion.get().is_none());
    time.set(100.);
    assert_eq!(
        unissued
            .fire_native_timer(0, generation, 100.)
            .receipts
            .len(),
        1
    );
    assert!(unissued.timers[0].completion.get().is_none());
    // Carry does not reject another session's binding. Existing timer guards
    // still decide what the supplied index, generation and cutoff mean.
    time.set(200.);
    assert!(unissued
        .deliver(&binding, timer(0, generation, 200.))
        .unwrap()
        .is_some());
}

#[derive(Default)]
struct CanvasSource {
    draws: Vec<(u64, u32, u64)>,
}

impl DataSource for CanvasSource {
    fn query(
        &mut self,
        _: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, crate::DataError> {
        unreachable!()
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        vec![("spark".into(), 1)]
    }
    fn draw(&mut self, request: &DrawRequest<'_>, _: &exact_canvas::Context2d) -> Drawn {
        self.draws
            .push((request.canvas, request.generation, request.seq));
        Drawn::Later
    }
}

fn canvas_runner() -> Runner<CanvasSource> {
    let plan = contract::compile("component App\n  state n = 1\n  action bump\n    n = n + 1\n  view\n    canvas surface=spark(n) width=20 height=10\n").unwrap();
    Runner::boot(
        plan,
        CanvasSource::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn draw(stamp: (u64, u32, u64), reply: DrawReply) -> Completion {
    Completion::CanvasDraw {
        lifetime: stamp.0,
        generation: stamp.1,
        seq: stamp.2,
        reply,
    }
}

fn geometry(width: f64) -> Geometry {
    Geometry {
        width,
        height: 10.,
        scale: 1.,
        ..Default::default()
    }
}

#[test]
fn canvas_draws_share_the_generation_binding_and_keep_sequence_checks() {
    let mut r = canvas_runner();
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(20.));
    let mut state = String::new();
    r.canvases.as_ref().unwrap().state(&mut state);
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();
    let lifetime = state[0]["lifetime"].as_u64().unwrap();
    let generation = state[0]["generation"].as_u64().unwrap() as u32;
    assert!(r.canvas_draw_completion(lifetime, generation).is_none());
    r.draw_canvases(&|_| true);
    let first = r.data().draws[0];
    let binding = r.canvas_draw_completion(first.0, first.1).unwrap();
    let metadata = binding.binding.0.lock().unwrap();
    assert_eq!(
        metadata.owner,
        Owner::Canvas {
            lifetime: first.0,
            generation: first.1
        }
    );
    assert_eq!(metadata.class, Class::UiOwned);
    assert!(matches!(metadata.protocol, Protocol::Subscription));
    drop(metadata);
    r.take_canvas_lists();
    assert!(r
        .deliver(&binding, draw(first, DrawReply::default()))
        .unwrap()
        .is_none());
    assert_eq!(r.take_canvas_lists()[0].seq, first.2);
    let typed_error = r.deliver(&binding, draw(first, DrawReply::default()));
    let Err(RunnerError::CanvasDraw(typed_error)) = typed_error else {
        panic!("duplicate draw applied")
    };
    assert_eq!(
        r.canvas_reply(first.0, first.1, first.2, DrawReply::default()),
        Err(typed_error)
    );

    r.act("bump", vec![]).unwrap();
    r.draw_canvases(&|_| true);
    let second = r.data().draws[1];
    let again = r.canvas_draw_completion(second.0, second.1).unwrap();
    assert!(Arc::ptr_eq(&binding.binding.0, &again.binding.0));
    assert_eq!(second.2, first.2 + 1);
    r.canvas_reply(second.0, second.1, second.2, DrawReply::default())
        .unwrap();
    assert_eq!(r.take_canvas_lists()[0].seq, second.2);
    assert!(!binding.binding.0.lock().unwrap().taken);

    r.set_canvas_geometry(view, geometry(40.));
    assert!(r.canvas_draw_completion(first.0, first.1).is_none());
    r.draw_canvases(&|_| true);
    let third = r.data().draws[2];
    let next = r.canvas_draw_completion(third.0, third.1).unwrap();
    assert!(!Arc::ptr_eq(&binding.binding.0, &next.binding.0));
    assert!(matches!(
        r.deliver(&binding, draw(second, DrawReply::default())),
        Err(RunnerError::CanvasDraw(_))
    ));
    assert!(r
        .deliver(&next, draw(third, DrawReply::default()))
        .unwrap()
        .is_none());
}

#[test]
fn malformed_canvas_lists_keep_the_existing_error_and_retire_the_binding() {
    let mut r = canvas_runner();
    let view = r.canvas_views()[0];
    r.set_canvas_geometry(view, geometry(20.));
    r.draw_canvases(&|_| true);
    let stamp = r.data().draws[0];
    let binding = r.canvas_draw_completion(stamp.0, stamp.1).unwrap();
    let malformed = DrawReply {
        lists: vec![vec![0xff; 8]],
        ..Default::default()
    };
    assert!(
        matches!(r.deliver(&binding, draw(stamp, malformed)), Err(RunnerError::CanvasDraw(why)) if why == "malformed list")
    );
    assert!(r.canvas_draw_completion(stamp.0, stamp.1).is_none());
    r.draw_canvases(&|_| true);
    let next = r.data().draws[1];
    assert_eq!(next.1, stamp.1 + 1);
    assert!(r.canvas_draw_completion(next.0, next.1).is_some());
    assert!(r
        .canvas_reply(stamp.0, stamp.1, stamp.2, DrawReply::default())
        .is_err());
    r.canvas_reply(next.0, next.1, next.2, DrawReply::default())
        .unwrap();
}
