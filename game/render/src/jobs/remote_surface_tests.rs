use super::*;
use crate::jobs::world_tests::{draw, triangle, TestGame};

#[test]
fn worker_clock_handoff_preserves_surface_pictures_saves_and_hashes() {
    std::thread::spawn(check_worker_clock_handoff)
        .join()
        .unwrap();
}

fn check_worker_clock_handoff() {
    use crate::{Hooks, Needs, RenderError, RenderWorld, WorldSurface};
    use exact_gpu::{fixture, wgpu, InputEvent, Surface, Value};

    struct WorkerHooks<const OFFSET: u8 = 0> {
        queue: Option<Jobs<Work>>,
        serial: u64,
        submitted: bool,
        installed: Vec<u8>,
        pipeline: Option<wgpu::RenderPipeline>,
    }

    impl<const OFFSET: u8> Default for WorkerHooks<OFFSET> {
        fn default() -> Self {
            Self {
                queue: Some(Jobs::remote(LIMITS)),
                serial: 0,
                submitted: false,
                installed: Vec::new(),
                pipeline: None,
            }
        }
    }

    impl<const OFFSET: u8> Hooks for WorkerHooks<OFFSET> {
        const ALLOWANCE: u32 = 1;
        const REMOTE: &'static [RemoteJob] = ENTRIES;

        fn prepare(
            &mut self,
            gpu: &HookGpu<'_>,
            view: &FrameView<'_>,
            _: &RenderWorld<'_>,
        ) -> Result<(), RenderError> {
            let mut queue = self.queue.take().unwrap();
            let mut take = queue.take(gpu, view);
            if view.time.allowance > 0 && !self.submitted {
                self.serial = view.time.serial;
                assert_eq!(take.lane(), Lane::Worker);
                assert!(take.next().is_none());
                for value in [2 + OFFSET, 4 + OFFSET, 6 + OFFSET] {
                    take.submit(u64::from(value), 0, 0, Work::new(value))
                        .unwrap();
                }
                self.submitted = true;
            }
            self.installed.extend(take.map(|done| done.result.unwrap()));
            self.queue = Some(queue);
            if !self.installed.is_empty() {
                self.pipeline.get_or_insert_with(|| triangle(gpu, view));
            }
            Ok(())
        }

        fn opaque(
            &mut self,
            pass: &mut wgpu::RenderPass<'_>,
            _: &FrameView<'_>,
        ) -> Result<(), RenderError> {
            if let (Some(value), Some(pipeline)) = (self.installed.last(), &self.pipeline) {
                pass.set_pipeline(pipeline);
                let first = u32::from((value / 2) % 2) * 3;
                pass.draw(first..first + 3, 0..1);
            }
            Ok(())
        }

        fn needs(&self) -> Needs {
            if !self.submitted || self.queue.as_ref().unwrap().pending() > 0 {
                Needs::PENDING | Needs::STARVED
            } else {
                Needs::NONE
            }
        }
    }

    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    eprintln!(
        "Surface jobs test using {:?}",
        gpu.adapter.get_info().backend
    );
    let script = Script::default();
    script.ready(1);
    install_module_transport::<WorkerHooks>(script.clone()).unwrap();
    let drive = |arrivals: &[usize]| {
        let posted = script.state.lock().unwrap().postings.len();
        let mut canvas = WorldSurface::<TestGame, (), false, WorkerHooks>::default();
        canvas
            .bind(
                &[Value::Number(7.), Value::Bool(false), Value::Bool(false)],
                None,
            )
            .unwrap();
        let before = draw(&gpu, &mut canvas, 0., false);
        let (queue, ids) = script.posting(posted);
        for &index in arrivals {
            script.complete(ids[index]);
        }
        canvas.clock(true);
        assert_eq!(draw(&gpu, &mut canvas, 0., true), before);
        assert!(!script.state.lock().unwrap().forgets.contains(&queue));
        assert!(canvas.render_hooks().installed.is_empty());
        canvas.input(&InputEvent::Key {
            code: "KeyH".into(),
            key: "h".into(),
            down: true,
            repeat: false,
            at_ms: 1.,
        });

        let checkpoints: Vec<_> = (1u32..=3)
            .map(|tick| {
                let stamp = f64::from(tick) * 17.;
                let picture = draw(&gpu, &mut canvas, stamp, true);
                assert_ne!(picture, before);
                assert_eq!(draw(&gpu, &mut canvas, stamp, true), picture);
                assert_eq!(canvas.render_hooks().installed.len(), tick as usize);
                let sim = canvas.sim().unwrap();
                assert_eq!(sim.world().tick(), u64::from(tick));
                (picture, sim.save().unwrap(), sim.world().hash())
            })
            .collect();
        assert_eq!(canvas.render_hooks().installed, [2, 4, 6]);
        assert_eq!(script.state.lock().unwrap().forgets.last(), Some(&queue));
        for id in ids {
            script.complete(id);
        }
        checkpoints
    };

    let no_answers = drive(&[]);
    for arrivals in [&[0, 1, 2][..], &[2, 1, 0], &[1]] {
        for ((picture, save, hash), (other_picture, other_save, other_hash)) in
            no_answers.iter().zip(drive(arrivals))
        {
            assert!(*picture == other_picture, "picture differs");
            assert!(*save == other_save, "save differs");
            assert_eq!(*hash, other_hash);
        }
    }

    script.ready(2);
    let posted = script.state.lock().unwrap().postings.len();
    let mut first = WorldSurface::<TestGame, (), false, WorkerHooks<10>>::default();
    let mut second = WorldSurface::<TestGame, (), false, WorkerHooks<20>>::default();
    for canvas in [&mut first as &mut dyn Surface, &mut second] {
        canvas
            .bind(
                &[Value::Number(7.), Value::Bool(false), Value::Bool(false)],
                None,
            )
            .unwrap();
        draw(&gpu, canvas, 0., false);
    }
    assert_eq!(first.render_hooks().serial, second.render_hooks().serial);
    let (first_queue, first_ids) = script.posting(posted);
    let (second_queue, second_ids) = script.posting(posted + 1);
    assert_ne!(first_queue, second_queue);
    for id in second_ids {
        script.complete(id);
    }
    draw(&gpu, &mut first, 17., false);
    assert!(first.render_hooks().installed.is_empty());
    draw(&gpu, &mut second, 17., false);
    assert_eq!(second.render_hooks().installed, [22]);
    for id in first_ids {
        script.complete(id);
    }
    draw(&gpu, &mut first, 34., false);
    assert_eq!(first.render_hooks().installed, [12]);
}
