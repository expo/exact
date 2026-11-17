//! A canvas's presentation let go of, on a real `CAMetalLayer`.
use crate::{fixture, wgpu, Frame, Module, Registry, Surface, SurfaceError, Value};
use objc2::rc::Retained;
use objc2::runtime::AnyObject;
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}

/// Renders this many incomplete frames, then complete ones; each render's
/// target is recorded in [`TARGETS`].
const LATE: u32 = 3;
static TARGETS: Mutex<Vec<wgpu::Texture>> = Mutex::new(Vec::new());

/// Clears its target, complete from its `ready`th render on (never at
/// `u32::MAX`).
struct Clearing {
    ready: u32,
    renders: u32,
    record: bool,
}

impl Surface for Clearing {
    fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn render(
        &mut self,
        _: &Frame,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        self.renders += 1;
        if self.record {
            TARGETS.lock().unwrap().push(target.texture().clone());
        }
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::GREEN),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        true
    }
    fn frame_ready(&self) -> bool {
        self.renders > self.ready
    }
}

/// Each render's number (from 1) and target, for [`Alternating`].
static ALTERNATING: Mutex<Vec<(u32, wgpu::Texture)>> = Mutex::new(Vec::new());

/// Holds its drawable for one render in every two: render 1 clears it green
/// and is incomplete; 2, a retry, draws red and is incomplete too; 3 only
/// loads its target and is complete. 4 is green and incomplete, and 5, the
/// first retry of a new sequence, only loads and is complete. From 6 on, an
/// even render is green and incomplete, an odd one a red, complete retry.
struct Alternating(u32);

impl Alternating {
    fn ready(n: u32) -> bool {
        n == 3 || n == 5 || (n >= 6 && n % 2 == 1)
    }
}

impl Surface for Alternating {
    fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
        Ok(())
    }
    fn render(
        &mut self,
        _: &Frame,
        _: &wgpu::Device,
        _: &wgpu::Queue,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        _: wgpu::TextureFormat,
    ) -> bool {
        self.0 += 1;
        let n = self.0;
        ALTERNATING
            .lock()
            .unwrap()
            .push((n, target.texture().clone()));
        let load = match n {
            3 | 5 => wgpu::LoadOp::Load,
            _ if Self::ready(n) || n == 2 => wgpu::LoadOp::Clear(wgpu::Color::RED),
            _ => wgpu::LoadOp::Clear(wgpu::Color::GREEN),
        };
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: None,
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        true
    }
    fn frame_ready(&self) -> bool {
        Self::ready(self.0)
    }
}

static REGISTRY: Registry = Registry {
    surfaces: &[
        ("clear", 0, || {
            Box::new(Clearing {
                ready: 0,
                renders: 0,
                record: false,
            })
        }),
        ("pending", 0, || {
            Box::new(Clearing {
                ready: u32::MAX,
                renders: 0,
                record: false,
            })
        }),
        ("late", 0, || {
            Box::new(Clearing {
                ready: LATE,
                renders: 0,
                record: true,
            })
        }),
        ("alternating", 0, || Box::new(Alternating(0))),
        ("presenting", 0, || {
            Box::new(Clearing {
                ready: LATE,
                renders: 0,
                record: false,
            })
        }),
        ("once", 0, || {
            Box::new(Clearing {
                ready: 1,
                renders: 0,
                record: false,
            })
        }),
    ],
    shaders: &[],
};

fn frame() -> Frame {
    Frame {
        width: 8.,
        height: 8.,
        scale: 1.,
        now_ms: 0.,
        seekable: false,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    }
}

fn module() -> Option<Module> {
    let gpu = fixture::device_or_skip(fixture::device())?;
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    Some(m)
}

/// A bound canvas of surface `name` on a new `CAMetalLayer`, which the
/// caller keeps past the module.
fn canvas(m: &mut Module, name: &str) -> (Retained<AnyObject>, u32) {
    // SAFETY: a new layer, returned to the caller to outlive the canvas.
    let layer: Retained<AnyObject> = unsafe { objc2::msg_send![objc2::class!(CAMetalLayer), new] };
    let ptr = Retained::as_ptr(&layer) as *mut std::ffi::c_void;
    // SAFETY: as above.
    let target = unsafe {
        m.gpu()
            .unwrap()
            .instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(ptr))
    }
    .unwrap();
    let id = m.create(name, target, 8, 8).unwrap();
    assert!(m.bind(id, &[], None));
    (layer, id)
}

fn within_two_seconds(what: &str, mut done: impl FnMut() -> bool) {
    for _ in 0..400 {
        if done() {
            return;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    panic!("{what}: not in two seconds")
}

/// A drawable held for an incomplete frame goes before its surface when
/// the canvas is destroyed, and when the module is dropped. Under the
/// agent's clock no acquisition thread holds the surface as well.
#[test]
fn a_held_drawable_goes_before_its_surface() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (a_layer, a) = canvas(&mut m, "pending");
    let (b_layer, b) = canvas(&mut m, "pending");
    for id in [a, b] {
        assert_eq!(m.render(id, &frame()), Some(true));
    }
    assert!(m.flush());
    assert!(m.instances[&a].held.is_some() && m.instances[&b].held.is_some());
    m.destroy(a);
    // `b` is rendered into the open frame, its drawable held, when the
    // module goes.
    assert_eq!(m.render(b, &frame()), Some(true));
    drop(m);
    drop((a_layer, b_layer));
}

/// The web's unload, which destroys the device as well, lets the open
/// frame's drawable go before its surface.
#[test]
fn unload_lets_an_open_frames_drawable_go_before_its_surface() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (layer, id) = canvas(&mut m, "clear");
    assert_eq!(m.render(id, &frame()), Some(true));
    assert!(m.open.is_some());
    m.unload();
    drop(layer);
}

/// Where an acquisition thread waits while the test holds it: 1 entered,
/// 2 let go, 3 gave up after ten seconds (the test was waiting on it).
static GATE: (Mutex<u8>, Condvar) = (Mutex::new(0), Condvar::new());

/// The presenter's callback for a landed drawable, run on the acquisition
/// thread: it stays there until the test lets it go.
extern "C" fn hold() {
    let (state, changed) = &GATE;
    let mut s = state.lock().unwrap();
    *s = 1;
    changed.notify_all();
    let (mut s, waited) = changed
        .wait_timeout_while(s, Duration::from_secs(10), |s| *s != 2)
        .unwrap();
    if waited.timed_out() {
        *s = 3;
    }
}

/// A canvas's window going does not wait for its acquisition thread, which
/// may be waiting for an image: on Android the compositor releases it only
/// after the window's `surfaceDestroyed`, which waits for the detach,
/// returns. The surface is dropped once the thread has ended. (An offscreen
/// layer's `nextDrawable` does not wait for a drawable to be released, so
/// the thread is held in the presenter's callback instead.)
#[test]
fn detach_leaves_the_acquisition_thread_to_end_on_its_own() {
    let _callback = crate::acquire::tests::CALLBACK.lock();
    let Some(mut m) = module() else {
        return;
    };
    let (layer, id) = canvas(&mut m, "clear");
    let (state, changed) = &GATE;
    *state.lock().unwrap() = 0;
    crate::acquire::on_acquire(Some(hold));
    // Starved, or presented and waiting for the next: either way the
    // thread says when a drawable lands.
    assert_eq!(m.render(id, &frame()), Some(true));
    assert!(m.flush());
    let (s, waited) = changed
        .wait_timeout_while(state.lock().unwrap(), Duration::from_secs(2), |s| *s != 1)
        .unwrap();
    assert!(!waited.timed_out(), "a drawable landed");
    drop(s);
    let target = m.instances[&id].presentation.clone().unwrap();
    m.detach(id);
    assert_eq!(*state.lock().unwrap(), 1, "detach waited for the thread");
    m.reap();
    assert!(m.retired.len() == 1 && !m.retired[0].finished());
    assert!(
        Arc::strong_count(&target) > 1,
        "the thread holds the surface"
    );
    *state.lock().unwrap() = 2;
    changed.notify_all();
    within_two_seconds("the acquisition thread ends", || {
        m.reap();
        m.retired.is_empty()
    });
    crate::acquire::on_acquire(None);
    assert_eq!(Arc::strong_count(&target), 1, "the module released it");
    drop((target, m, layer));
}

/// Renders after an incomplete frame go offscreen, and the complete one is
/// copied into the drawable the first was written into: a drawable takes
/// two submissions at most, however long its surface is pending.
#[test]
fn retries_of_an_incomplete_frame_leave_its_drawable_alone() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (layer, id) = canvas(&mut m, "late");
    for _ in 0..=LATE {
        assert!(m.render(id, &frame()).is_some());
        assert!(m.flush());
    }
    let targets = std::mem::take(&mut *TARGETS.lock().unwrap());
    assert_eq!(targets.len(), LATE as usize + 1);
    assert!(
        targets[1..].iter().all(|t| *t != targets[0]),
        "a retry wrote the held drawable"
    );
    let inst = &m.instances[&id];
    assert!(inst.held.is_none(), "presented");
    within_two_seconds("the complete frame shown", || m.seen(id));
    drop((targets, m, layer));
}

/// A canvas's record says `presented` once a complete frame is with the
/// compositor, and not for the incomplete ones before it; a lost device
/// starts the picture over (LLP 1009 D9).
#[test]
fn the_record_says_presented_once_a_complete_frame_is_shown() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (layer, id) = canvas(&mut m, "presenting");
    for _ in 0..LATE {
        assert!(m.render(id, &frame()).is_some());
        assert!(m.flush());
    }
    assert_eq!(m.take_published(id), None, "nothing drawn, nothing to say");
    assert!(m.render(id, &frame()).is_some());
    assert!(m.flush());
    let mut record = None;
    within_two_seconds("the record says presented", || {
        record = m.take_published(id);
        record.is_some()
    });
    assert_eq!(record.as_deref(), Some(r#"{"presented":true}"#));
    assert!(m.render(id, &frame()).is_some());
    assert!(m.flush());
    assert_eq!(m.take_published(id), None, "a later frame changes nothing");
    m.lose_device();
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"presented":false}"#)
    );
    drop((m, layer));
}

/// A target let go of takes what it showed with it: the canvas is not
/// presented again until a frame on the target that replaces it is. Every
/// replacement goes through `Module::release`, Android's buffers
/// (`attach_buffers`) as this layer.
#[test]
fn a_replaced_target_is_not_presented_until_it_shows_a_frame() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (first, id) = canvas(&mut m, "clear");
    let presented = |m: &mut Module| {
        assert!(m.render(id, &frame()).is_some());
        assert!(m.flush());
        let mut record = None;
        within_two_seconds("the record says presented", || {
            record = m.take_published(id);
            record.is_some()
        });
        assert_eq!(record.as_deref(), Some(r#"{"presented":true}"#));
    };
    presented(&mut m);
    // SAFETY: a new layer, kept past the module.
    let second: Retained<AnyObject> = unsafe { objc2::msg_send![objc2::class!(CAMetalLayer), new] };
    let ptr = Retained::as_ptr(&second) as *mut std::ffi::c_void;
    // SAFETY: as above.
    let target = unsafe {
        m.gpu()
            .unwrap()
            .instance
            .create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(ptr))
    }
    .unwrap();
    m.detach(id);
    assert_eq!(
        m.take_published(id).as_deref(),
        Some(r#"{"presented":false}"#),
        "the detached target took its picture"
    );
    assert!(m.attach(id, target, 8, 8));
    assert_eq!(m.take_published(id), None, "still not presented");
    presented(&mut m);
    drop((m, first, second));
}

/// A surface that holds every other drawable renders its retries into one
/// texture, made once, however many sequences complete; each sequence's
/// first retry starts on zeros, and a later retry of the sequence sees what
/// the one before it drew.
#[test]
fn alternating_frames_reuse_one_scratch_and_each_sequence_starts_cleared() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let (layer, id) = canvas(&mut m, "alternating");
    let mut seen = Vec::new();
    for n in 1..=45 {
        assert_eq!(m.render(id, &frame()), Some(true));
        assert!(m.flush());
        if n == 3 || n == 5 {
            seen.push(scratch_texel(&m, id));
        }
    }
    assert!(seen[0] != [0; 4], "within a sequence, the last retry's red");
    assert_eq!(seen[1], [0; 4], "a new sequence starts on zeros");
    let recorded = std::mem::take(&mut *ALTERNATING.lock().unwrap());
    let retry = |n: u32| [2, 3, 5].contains(&n) || (n >= 7 && n % 2 == 1);
    let drawables: Vec<_> = recorded.iter().filter(|(n, _)| !retry(*n)).collect();
    let mut scratches: Vec<&wgpu::Texture> = Vec::new();
    for (_, t) in recorded.iter().filter(|(n, _)| retry(*n)) {
        assert!(
            drawables.iter().all(|(_, d)| d != t),
            "a retry drew into a drawable"
        );
        if !scratches.contains(&t) {
            scratches.push(t);
        }
    }
    assert_eq!(
        scratches.len(),
        1,
        "22 retry sequences, one scratch texture"
    );
    drop((recorded, m, layer));
}

/// The first texel of canvas `id`'s scratch, as RGBA, copied out through
/// the scratch's own copy.
fn scratch_texel(m: &Module, id: u32) -> [u8; 4] {
    let gpu = m.gpu().unwrap();
    let s = m.instances[&id].scratch.as_ref().expect("a scratch");
    let out = gpu.device.create_texture(&wgpu::TextureDescriptor {
        label: None,
        size: s.texture.size(),
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: s.texture.format(),
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    });
    let mut encoder = gpu.device.create_command_encoder(&Default::default());
    s.copy(&mut encoder, &out.create_view(&Default::default()));
    gpu.queue.submit([encoder.finish()]);
    fixture::read(gpu, &out).unwrap().at(0, 0)
}

/// A canvas keeps its scratch past a complete retry and gives it back when
/// it is resized, when it has not retried for `SCRATCH_IDLE` renders, when
/// its window goes, and when the module takes another device.
#[test]
fn a_scratch_goes_on_resize_idleness_release_and_a_new_device() {
    let Some(mut m) = module() else {
        return;
    };
    m.set_seekable(true);
    let scratch = |m: &Module, id| m.instances[&id].scratch.is_some();
    // "once": its first render is incomplete, every later one complete.
    let canvases: Vec<_> = (0..4).map(|_| canvas(&mut m, "once")).collect();
    for (_, id) in &canvases {
        for _ in 0..2 {
            assert!(m.render(*id, &frame()).is_some());
            assert!(m.flush());
        }
        assert!(scratch(&m, *id), "kept after the complete retry");
    }
    let [(_, resized), (_, idle), (_, detached), (_, kept)] = canvases[..] else {
        unreachable!()
    };
    let wider = Frame {
        width: 16.,
        ..frame()
    };
    assert!(m.render(resized, &wider).is_some());
    assert!(!scratch(&m, resized), "not kept at the old size");
    for n in 1..=crate::frame::SCRATCH_IDLE {
        assert!(m.render(idle, &frame()).is_some());
        assert!(m.flush());
        assert_eq!(scratch(&m, idle), n < crate::frame::SCRATCH_IDLE);
    }
    m.detach(detached);
    assert!(!scratch(&m, detached), "released with the window");
    assert!(scratch(&m, kept));
    m.set_gpu(fixture::device().unwrap());
    assert!(
        !scratch(&m, kept),
        "made on the device the module let go of"
    );
    drop((m, canvases));
}
