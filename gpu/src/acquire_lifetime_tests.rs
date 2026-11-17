use super::*;
use crate::{Frame, Module, Registry, Surface, SurfaceError, Value};
use std::{cell::RefCell, collections::HashMap, thread::ThreadId, time::Duration};

enum Event {
    Joining(ThreadId),
    Unloaded(usize),
}

thread_local! {
    static JOINS: RefCell<Option<mpsc::Sender<Event>>> = const { RefCell::new(None) };
}

pub(super) fn joining(id: ThreadId) {
    JOINS.with_borrow(|events| {
        if let Some(events) = events {
            let _ = events.send(Event::Joining(id));
        }
    });
}

struct Empty;

impl Surface for Empty {
    fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }

    fn render(
        &mut self,
        _: &Frame,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        _: &mut wgpu::CommandEncoder,
        _: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        false
    }
}

static REGISTRY: Registry = Registry {
    surfaces: &[("empty", 0, || Box::new(Empty))],
    shaders: &[],
};

/// No device is needed: the worker waits in place of a compositor acquisition.
fn blocked(ended: Arc<AtomicUsize>) -> (Acquire, mpsc::Sender<()>, mpsc::Receiver<()>) {
    let (ask, asked) = mpsc::channel();
    let (_, answers) = mpsc::channel();
    let (release, released) = mpsc::channel();
    let (retiring, retired) = mpsc::channel();
    let thread = std::thread::spawn(move || {
        let _ = asked.recv();
        let _ = retiring.send(());
        let _ = released.recv();
        ended.fetch_add(1, Ordering::SeqCst);
    });
    (
        Acquire {
            worker: Some((ask, answers, thread)),
            in_flight: true,
            starved: Default::default(),
            sent: Default::default(),
            taken: 0,
            surface: None,
        },
        release,
        retired,
    )
}

#[test]
fn detach_returns_while_final_shutdown_joins_active_and_retired_workers() {
    let ended = Arc::new(AtomicUsize::new(0));
    let observed = ended.clone();
    let (ready, setup) = mpsc::channel();
    let (shutdown, shutting_down) = mpsc::channel();
    let (events, event) = mpsc::channel();
    let owner = std::thread::spawn(move || {
        JOINS.set(Some(events.clone()));
        let mut module = Module::new(&REGISTRY);
        let mut releases = Vec::new();
        let mut retirements = Vec::new();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let id = module.create_headless("empty").unwrap();
            let (acquire, release, retired) = blocked(observed.clone());
            let worker = acquire.worker.as_ref().unwrap().2.thread().id();
            module.instances.get_mut(&id).unwrap().acquire = acquire;
            ids.push(id);
            releases.push((worker, release));
            retirements.push(retired);
        }
        // The controller owns the permits before detach, so a blocking detach
        // fails its bounded wait without leaving a worker stranded.
        ready.send((releases, retirements)).unwrap();
        module.detach(ids[0]);
        module.destroy(ids[1]);
        module.reap();
        let _ = shutting_down.recv();
        drop(module);
        // This is where gpu_unload returns and the host can dlclose its artifact.
        let _ = events.send(Event::Unloaded(observed.load(Ordering::SeqCst)));
    });
    let bound = Duration::from_secs(5);
    let (releases, retirements) = setup.recv_timeout(bound).unwrap();
    for retired in &retirements[..2] {
        retired.recv_timeout(bound).expect("detach must not wait");
    }
    shutdown.send(()).unwrap();
    retirements[2]
        .recv_timeout(bound)
        .expect("final shutdown retires the attached canvas too");
    let mut releases: HashMap<_, _> = releases.into_iter().collect();
    let mut joins = 0;
    let at_unload = loop {
        match event.recv_timeout(bound).expect("shutdown must progress") {
            Event::Joining(id) => {
                // Each worker stays blocked until shutdown reaches its join.
                drop(releases.remove(&id).expect("each worker is joined once"));
                joins += 1;
            }
            Event::Unloaded(ended) => break ended,
        }
    };
    // Release any worker shutdown missed before asserting the failure.
    drop(releases);
    owner.join().unwrap();
    assert_eq!(joins, 3, "unloaded before joining every acquisition worker");
    assert_eq!(
        at_unload, 3,
        "every worker exited before executable code can go"
    );
    assert_eq!(ended.load(Ordering::SeqCst), 3);
}
