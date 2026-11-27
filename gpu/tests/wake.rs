#![cfg(not(target_arch = "wasm32"))]

use exact_gpu::{
    fixture, native, Frame, InputEvent, Module, Registry, Surface, SurfaceError, SurfaceWaker,
    Value,
};
use std::sync::{
    atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering},
    Arc, Mutex,
};

struct AsyncSurface {
    waker: Option<SurfaceWaker>,
    events: Arc<Mutex<Vec<&'static str>>>,
    color: Arc<AtomicU32>,
    empty: Arc<AtomicBool>,
    rearm_in_render: Arc<AtomicBool>,
    renders: Arc<AtomicUsize>,
    poll_input: bool,
}

impl Surface for AsyncSurface {
    fn set_waker(&mut self, waker: SurfaceWaker) {
        self.events.lock().unwrap().push("set_waker");
        self.waker = Some(waker);
    }
    fn clock(&mut self, _seekable: bool) {
        self.events.lock().unwrap().push("clock");
    }
    fn reduced_motion(&mut self, _reduce: bool) {
        self.events.lock().unwrap().push("reduced_motion");
    }
    fn device_ready(&mut self, _features: wgpu::Features, _limits: wgpu::Limits) {
        self.events.lock().unwrap().push("device_ready");
    }
    fn color_target(&mut self, _destination: exact_gpu::ColorTarget) {
        self.events.lock().unwrap().push("color_target");
    }
    fn is_empty(&self) -> bool {
        self.empty.load(Ordering::Acquire)
    }
    fn poll_input_before_draw(&mut self) {
        self.poll_input = true;
    }
    fn redraw_for_input(&mut self, _now_ms: f64, _period_ms: f64) -> std::task::Poll<bool> {
        std::task::Poll::Pending
    }
    fn input(&mut self, _event: &InputEvent) {}
    fn bind(&mut self, _inputs: &[Value], _at_ms: Option<f64>) -> Result<(), SurfaceError> {
        self.events.lock().unwrap().push("bind");
        Ok(())
    }
    fn render(
        &mut self,
        _frame: &Frame,
        _device: &wgpu::Device,
        _queue: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        view: &wgpu::TextureView,
        _format: wgpu::TextureFormat,
    ) -> bool {
        self.renders.fetch_add(1, Ordering::SeqCst);
        let c = self.color.load(Ordering::Acquire);
        let r = f64::from((c >> 16) & 0xff) / 255.0;
        let g = f64::from((c >> 8) & 0xff) / 255.0;
        let b = f64::from(c & 0xff) / 255.0;
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color { r, g, b, a: 1.0 }),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        if self.rearm_in_render.swap(false, Ordering::AcqRel) {
            if let Some(w) = &self.waker {
                w.wake();
            }
        }
        false
    }
}

struct SharedState {
    waker: Mutex<Option<SurfaceWaker>>,
    events: Arc<Mutex<Vec<&'static str>>>,
    color: Arc<AtomicU32>,
    empty: Arc<AtomicBool>,
    rearm_in_render: Arc<AtomicBool>,
    renders: Arc<AtomicUsize>,
}

static SHARED: std::sync::OnceLock<SharedState> = std::sync::OnceLock::new();
static WAKE_HITS: AtomicUsize = AtomicUsize::new(0);
static SUITE_LOCK: Mutex<()> = Mutex::new(());

fn shared() -> &'static SharedState {
    SHARED.get_or_init(|| SharedState {
        waker: Mutex::new(None),
        events: Arc::new(Mutex::new(Vec::new())),
        color: Arc::new(AtomicU32::new(0x00_00_ff)),
        empty: Arc::new(AtomicBool::new(false)),
        rearm_in_render: Arc::new(AtomicBool::new(false)),
        renders: Arc::new(AtomicUsize::new(0)),
    })
}

fn make_surface() -> Box<dyn Surface> {
    struct Wrapper(AsyncSurface);
    impl Surface for Wrapper {
        fn set_waker(&mut self, waker: SurfaceWaker) {
            *shared().waker.lock().unwrap() = Some(waker.clone());
            self.0.set_waker(waker);
        }
        fn clock(&mut self, s: bool) {
            self.0.clock(s);
        }
        fn reduced_motion(&mut self, r: bool) {
            self.0.reduced_motion(r);
        }
        fn device_ready(&mut self, f: wgpu::Features, l: wgpu::Limits) {
            self.0.device_ready(f, l);
        }
        fn color_target(&mut self, d: exact_gpu::ColorTarget) {
            self.0.color_target(d);
        }
        fn is_empty(&self) -> bool {
            self.0.is_empty()
        }
        fn poll_input_before_draw(&mut self) {
            self.0.poll_input_before_draw();
        }
        fn redraw_for_input(&mut self, n: f64, p: f64) -> std::task::Poll<bool> {
            self.0.redraw_for_input(n, p)
        }
        fn input(&mut self, e: &InputEvent) {
            self.0.input(e);
        }
        fn bind(&mut self, i: &[Value], a: Option<f64>) -> Result<(), SurfaceError> {
            self.0.bind(i, a)
        }
        fn render(
            &mut self,
            f: &Frame,
            d: &wgpu::Device,
            q: &wgpu::Queue,
            e: &mut wgpu::CommandEncoder,
            v: &wgpu::TextureView,
            fmt: wgpu::TextureFormat,
        ) -> bool {
            self.0.render(f, d, q, e, v, fmt)
        }
    }
    let s = shared();
    Box::new(Wrapper(AsyncSurface {
        waker: None,
        events: Arc::clone(&s.events),
        color: Arc::clone(&s.color),
        empty: Arc::clone(&s.empty),
        rearm_in_render: Arc::clone(&s.rearm_in_render),
        renders: Arc::clone(&s.renders),
        poll_input: false,
    }))
}

static REGISTRY: Registry = Registry {
    surfaces: &[("async", 0, make_surface)],
    shaders: &[],
};

extern "C" fn count_wake() {
    WAKE_HITS.fetch_add(1, Ordering::SeqCst);
}

fn frame(now_ms: f64) -> Frame {
    Frame {
        width: 2.0,
        height: 2.0,
        scale: 1.0,
        now_ms,
        seekable: false,
        period_ms: 16.0,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    }
}

struct WakeCallbackGuard;
impl Drop for WakeCallbackGuard {
    fn drop(&mut self) {
        native::on_wake(None);
    }
}

#[test]
fn waker_lifecycle_off_thread_wake_rearm_and_destroy() {
    let _guard = SUITE_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let _cb = WakeCallbackGuard;
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };

    let s = shared();
    s.events.lock().unwrap().clear();
    s.color.store(0x00_00_ff, Ordering::Release);
    s.empty.store(false, Ordering::Release);
    s.rearm_in_render.store(false, Ordering::Release);
    s.renders.store(0, Ordering::SeqCst);
    *s.waker.lock().unwrap() = None;
    WAKE_HITS.store(0, Ordering::SeqCst);
    native::on_wake(Some(count_wake));

    let mut module = Module::new(&REGISTRY);
    module.set_gpu(gpu);
    let id = module.create_headless("async").unwrap();

    // `set_waker` must run first, before `clock`, `reduced_motion`, `device_ready`, `color_target`.
    let events = s.events.lock().unwrap().clone();
    assert_eq!(events.first().copied(), Some("set_waker"), "{events:?}");
    let waker = s.waker.lock().unwrap().clone().expect("waker given");

    // Waking before first `bind` must not report `frame_pending` or `frame_dirty` as ready to draw
    // via wake until bound.
    waker.wake();
    assert_eq!(WAKE_HITS.load(Ordering::SeqCst), 1);
    assert!(!module.frame_pending(id));

    // Bind and render first frame; it rests (`wants == false`) and consumes the pre-bind wake.
    assert!(module.bind(id, &[], None));
    assert!(module.frame_pending(id));
    let (pixels, wants) = module.readback(id, &frame(0.0)).unwrap();
    assert!(!wants);
    assert_eq!(&pixels.data[..4], &[0, 0, 255, 255]);
    assert!(!module.frame_pending(id));
    assert!(!module.frame_dirty(id, 16.0));

    // Off-thread wake changes rendered pixels, coalesces multiple wakes, and overrides
    // `redraw_for_input() == Pending` so the resting canvas redraws.
    let worker_waker = waker.clone();
    let worker_color = Arc::clone(&s.color);
    std::thread::spawn(move || {
        worker_color.store(0xff_00_00, Ordering::Release);
        worker_waker.wake();
        worker_waker.wake();
        worker_waker.wake();
    })
    .join()
    .unwrap();
    assert_eq!(WAKE_HITS.load(Ordering::SeqCst), 2);
    assert!(module.frame_pending(id));
    assert!(module.frame_dirty(id, 32.0));

    // Even if an input event arrives while woken, `frame_dirty` stays true (not masked by Pending).
    module.input(id, &InputEvent::Blur { at_ms: 32.0 });
    assert!(module.frame_dirty(id, 32.0));

    // A wake raised *during* `render()` must survive and cause `readback` to return `wants = true`.
    s.rearm_in_render.store(true, Ordering::Release);
    let (pixels, wants) = module.readback(id, &frame(32.0)).unwrap();
    assert!(
        wants,
        "wake raised during render must request another frame"
    );
    assert_eq!(&pixels.data[..4], &[255, 0, 0, 255]);
    assert!(module.frame_pending(id));
    assert_eq!(WAKE_HITS.load(Ordering::SeqCst), 3);

    // Next render consumes that wake and returns to rest.
    let (_, wants) = module.readback(id, &frame(48.0)).unwrap();
    assert!(!wants);
    assert!(!module.frame_pending(id));

    // Empty-to-non-empty wake transition works and clears properly.
    s.empty.store(true, Ordering::Release);
    waker.wake();
    let (pixels, wants) = module.readback(id, &frame(64.0)).unwrap();
    assert!(!wants);
    assert_eq!(&pixels.data[..4], &[0, 0, 0, 0]);
    assert!(!module.frame_pending(id));

    s.empty.store(false, Ordering::Release);
    s.color.store(0x00_ff_00, Ordering::Release);
    waker.wake();
    assert!(module.frame_pending(id));
    let (pixels, wants) = module.readback(id, &frame(80.0)).unwrap();
    assert!(!wants);
    assert_eq!(&pixels.data[..4], &[0, 255, 0, 255]);

    // Seekable mode (agent clock) must not make an idle surface report `frame_pending`.
    module.set_seekable(true);
    assert!(!module.frame_pending(id));
    assert!(module.frame_dirty(id, 96.0));
    module.set_seekable(false);

    // Destroy silences the waker permanently.
    let hits_before_destroy = WAKE_HITS.load(Ordering::SeqCst);
    module.destroy(id);
    waker.wake();
    assert_eq!(WAKE_HITS.load(Ordering::SeqCst), hits_before_destroy);
}
