use crate::*;
struct Probe;
impl Surface for Probe {
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
        panic!("lost device rendered")
    }
    fn agent(&mut self, _: &str) -> Option<String> {
        Some("alive".into())
    }
}
static REGISTRY: Registry = Registry {
    surfaces: &[("probe", 0, || Box::new(Probe))],
    shaders: &[],
};
#[test]
fn device_loss_guards_each_gpu_entry_before_any_other_call() {
    for operation in 0..7 {
        let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
            return;
        };
        let mut m = Module::new(&REGISTRY);
        m.set_gpu(gpu);
        let id = m.create_headless("probe").unwrap();
        m.bind(id, &[], None);
        m.set_seekable(false);
        m.instances.get_mut(&id).unwrap().dirty = false;
        assert!(!m.frame_dirty(id, 300.));
        // Model the asynchronous callback, without calling lose_device first.
        m.device_lost.store(true, Ordering::Release);
        assert!(m.frame_pending(id));
        assert!(m.frame_dirty(id, 300.));
        match operation {
            0 => assert!(!m.child(id, 0, "", [0., 0., 1., 1.], [1, 1], &[0; 4])),
            1 => assert!(!m.texture(id, 1, 1, &[0; 4])),
            2 => assert!(m
                .readback(
                    id,
                    &Frame {
                        width: 1.,
                        height: 1.,
                        scale: 1.,
                        now_ms: 0.,
                        seekable: true,
                        period_ms: 0.,
                        children_generation: 0,
                        shader_generation: 0,
                        headroom: 1.0,
                    }
                )
                .is_none()),
            3 => assert!(!m.dirty(id)),
            4 => assert!(!m.sync()),
            5 => assert!(m.gpu().is_none()),
            _ => {
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
                // A sentinel must never be retained/imported after loss.
                assert!(!unsafe { m.texture_from_metal(id, 1, 1, std::ptr::dangling_mut()) });
            }
        }
        assert_eq!(m.agent(id, "state").as_deref(), Some("alive"));
        assert!(!m.dirty(id));
    }
}

#[test]
fn default_surface_reconstructs_on_device_loss_with_replayed_state_and_fresh_gpu_resources() {
    use std::path::Path;
    use std::sync::{Arc, Mutex};

    const WGSL: &str = r#"
        struct U { color: vec4<f32> }
        @group(0) @binding(0) var<uniform> u: U;
        @vertex fn vs(@builtin(vertex_index) i: u32) -> @builtin(position) vec4<f32> {
            let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
            return vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
        }
        @fragment fn fs() -> @location(0) vec4<f32> { return u.color; }
    "#;

    static LOG: Mutex<Vec<String>> = Mutex::new(Vec::new());
    static NEXT_GEN: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
    static WAKERS: Mutex<Vec<SurfaceWaker>> = Mutex::new(Vec::new());

    struct GpuCache {
        format: wgpu::TextureFormat,
        pipeline: wgpu::RenderPipeline,
        uniform: FrameUniform,
        groups: Vec<wgpu::BindGroup>,
    }

    // Format-only cache with no `device_lost` override, matching `Field` and app shaders.
    struct FormatOnly {
        instance: usize,
        color: [f32; 4],
        device_ready: bool,
        color_space_granted: bool,
        gpu: Option<GpuCache>,
    }

    impl FormatOnly {
        fn new() -> Box<dyn Surface> {
            let instance = NEXT_GEN.fetch_add(1, Ordering::Relaxed);
            LOG.lock().unwrap().push(format!("new:{instance}"));
            Box::new(Self {
                instance,
                color: [0.0; 4],
                device_ready: false,
                color_space_granted: false,
                gpu: None,
            })
        }
    }

    impl Surface for FormatOnly {
        fn device_ready(&mut self, _: wgpu::Features, _: wgpu::Limits) {
            self.device_ready = true;
        }
        fn color_space_granted(&mut self, _: ColorSpace) {
            self.color_space_granted = true;
        }
        fn set_waker(&mut self, waker: SurfaceWaker) {
            LOG.lock().unwrap().push(format!("waker:{}", self.instance));
            WAKERS.lock().unwrap().push(waker);
        }
        fn storage(&mut self, data: &Path, cache: &Path, tmp: &Path) {
            LOG.lock().unwrap().push(format!(
                "storage:{}:{}|{}|{}",
                self.instance,
                data.display(),
                cache.display(),
                tmp.display()
            ));
        }
        fn clock(&mut self, seekable: bool) {
            LOG.lock()
                .unwrap()
                .push(format!("clock:{}:{seekable}", self.instance));
        }
        fn reduced_motion(&mut self, reduce: bool) {
            LOG.lock()
                .unwrap()
                .push(format!("motion:{}:{reduce}", self.instance));
        }
        fn lifecycle(&mut self, event: Lifecycle) {
            LOG.lock()
                .unwrap()
                .push(format!("lifecycle:{}:{event:?}", self.instance));
        }
        fn size(&mut self, w: f32, h: f32) -> bool {
            LOG.lock()
                .unwrap()
                .push(format!("size:{}:{w}x{h}", self.instance));
            false
        }
        fn set_performance_reporting(&mut self, enabled: bool) {
            LOG.lock()
                .unwrap()
                .push(format!("perf:{}:{enabled}", self.instance));
        }
        fn bind(&mut self, inputs: &[Value], at_ms: Option<f64>) -> Result<(), SurfaceError> {
            if !self.device_ready || !self.color_space_granted {
                return Err(SurfaceError(
                    "bind called before device_ready and color_space_granted".into(),
                ));
            }
            let Value::Number(r) = inputs
                .first()
                .ok_or_else(|| SurfaceError("missing".into()))?
            else {
                return Err(SurfaceError("expected number".into()));
            };
            if *r < 0.0 {
                return Err(SurfaceError("negative red".into()));
            }
            self.color = [*r as f32, 0.25, 0.5, 1.0];
            LOG.lock()
                .unwrap()
                .push(format!("bind:{}:{r}@{at_ms:?}", self.instance));
            Ok(())
        }
        fn render(
            &mut self,
            _: &Frame,
            device: &wgpu::Device,
            queue: &wgpu::Queue,
            encoder: &mut wgpu::CommandEncoder,
            target: &wgpu::TextureView,
            format: wgpu::TextureFormat,
        ) -> bool {
            if self.gpu.as_ref().map(|g| g.format) != Some(format) {
                let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
                    label: Some("format-only"),
                    source: wgpu::ShaderSource::Wgsl(WGSL.into()),
                });
                let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("format-only"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::FRAGMENT,
                        count: None,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                    }],
                });
                let pipeline_layout =
                    device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                        label: Some("format-only"),
                        bind_group_layouts: &[Some(&layout)],
                        immediate_size: 0,
                    });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("format-only"),
                    layout: Some(&pipeline_layout),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs"),
                        targets: &[Some(format.into())],
                        compilation_options: Default::default(),
                    }),
                    primitive: Default::default(),
                    depth_stencil: None,
                    multisample: Default::default(),
                    multiview_mask: None,
                    cache: None,
                });
                let uniform = FrameUniform::new(device, 16, "glass uniforms");
                let groups = (0..uniform.slots())
                    .map(|slot| {
                        device.create_bind_group(&wgpu::BindGroupDescriptor {
                            label: Some("format-only"),
                            layout: &layout,
                            entries: &[wgpu::BindGroupEntry {
                                binding: 0,
                                resource: uniform.buffer(slot).as_entire_binding(),
                            }],
                        })
                    })
                    .collect();
                self.gpu = Some(GpuCache {
                    format,
                    pipeline,
                    uniform,
                    groups,
                });
            }
            let gpu = self.gpu.as_mut().unwrap();
            let bytes: Vec<u8> = self.color.iter().flat_map(|f| f.to_le_bytes()).collect();
            let slot = gpu.uniform.write(queue, &bytes);
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("format-only"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: target,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..Default::default()
            });
            pass.set_pipeline(&gpu.pipeline);
            pass.set_bind_group(0, &gpu.groups[slot], &[]);
            pass.draw(0..3, 0..1);
            false
        }
        fn agent(&mut self, _: &str) -> Option<String> {
            Some(self.instance.to_string())
        }
    }

    struct Retained {
        ticks: u32,
        children_cleared: bool,
        each_cleared: bool,
        losses: u32,
    }
    impl Surface for Retained {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            self.ticks += 1;
            Ok(())
        }
        fn children(
            &mut self,
            current: Option<&wgpu::TextureView>,
            previous: Option<&wgpu::TextureView>,
        ) {
            if current.is_none() && previous.is_none() {
                self.children_cleared = true;
            }
        }
        fn child(
            &mut self,
            _: usize,
            _: &str,
            texture: Option<&wgpu::TextureView>,
            frame: [f32; 4],
        ) {
            if texture.is_none() && frame == [0.; 4] {
                self.each_cleared = true;
            }
        }
        fn device_lost(&mut self) -> bool {
            self.losses += 1;
            true
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
        fn agent(&mut self, _: &str) -> Option<String> {
            Some(format!(
                "ticks={},children={},each={},losses={}",
                self.ticks, self.children_cleared, self.each_cleared, self.losses
            ))
        }
    }

    static TEST_REG: Registry = Registry {
        surfaces: &[
            ("formatOnly", 1, FormatOnly::new),
            ("retained", 0, || {
                Box::new(Retained {
                    ticks: 0,
                    children_cleared: false,
                    each_cleared: false,
                    losses: 0,
                })
            }),
        ],
        shaders: &[],
    };

    let Some(gpu1) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let Some(gpu2) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    LOG.lock().unwrap().clear();
    WAKERS.lock().unwrap().clear();
    NEXT_GEN.store(0, Ordering::Relaxed);

    let mut m = Module::new(&TEST_REG);
    m.set_seekable(true);
    m.set_reduced_motion(true);
    m.set_gpu(gpu1);
    let id = m.create_headless("formatOnly").unwrap();
    let kept = m.create_headless("retained").unwrap();

    let root = std::env::current_dir().unwrap();
    assert!(m.storage(id, &root.join("d"), &root.join("c"), &root.join("t")));
    assert!(m.size(id, 320.0, 240.0));
    m.lifecycle(id, 1); // Visible
    m.lifecycle(id, 2); // Interrupted (coexists with Visible)
    m.lifecycle(id, 4); // Activated (must NOT replay)
    assert!(m.set_performance_reporting(id, true));
    assert!(m.bind(id, &[Value::Number(1.0)], Some(123.0)));
    // Rejected bind must leave the accepted snapshot (1.0 @ 123.0) intact.
    assert!(!m.bind(id, &[Value::Number(-1.0)], Some(999.0)));
    assert_eq!(m.take_error(), "negative red");
    assert!(m.bind(kept, &[], None));
    assert!(m.texture(kept, 1, 1, &[255; 4]));
    assert!(m.child(kept, 0, "card", [0., 0., 10., 10.], [1, 1], &[255; 4]));

    let frame = Frame {
        width: 4.,
        height: 4.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.0,
    };
    let (px1, _) = m.readback(id, &frame).expect("device 1 render");
    assert_eq!(&px1.data[..4], &[255, 64, 128, 255]);
    assert_eq!(m.agent(id, "").as_deref(), Some("0"));
    assert_eq!(WAKERS.lock().unwrap().len(), 1);
    let old_waker = WAKERS.lock().unwrap()[0].clone();

    LOG.lock().unwrap().clear();
    // Directly replacing an active device through `set_gpu` must also lose/reconstruct.
    m.gpu().unwrap().device.destroy();
    let _ = m.gpu.as_ref().unwrap().device.poll(wgpu::PollType::Poll);
    m.set_gpu(gpu2);

    // Old waker was killed; new waker wakes the replacement.
    m.instances.get_mut(&id).unwrap().dirty = false;
    old_waker.wake();
    assert!(!m.instances[&id].wake.is_woken(), "old waker must be dead");
    let new_waker = WAKERS.lock().unwrap()[1].clone();
    new_waker.wake();
    assert!(m.instances[&id].wake.is_woken(), "new waker must be live");

    // Replacement object has new identity and same canvas ID; replayed size before bind,
    // storage, visibility+interrupted (not Activated), perf, and accepted stamped bind.
    assert_eq!(m.agent(id, "").as_deref(), Some("1"));
    let log = LOG.lock().unwrap().clone();
    assert!(log.iter().any(|s| s.starts_with("storage:1:")), "{log:?}");
    assert!(log.contains(&"clock:1:true".to_string()), "{log:?}");
    assert!(log.contains(&"motion:1:true".to_string()), "{log:?}");
    assert!(log.contains(&"lifecycle:1:Visible".to_string()), "{log:?}");
    assert!(
        log.contains(&"lifecycle:1:Interrupted".to_string()),
        "{log:?}"
    );
    assert!(
        !log.iter().any(|s| s.contains("Activated")),
        "Activated gesture must not replay: {log:?}"
    );
    let size_pos = log.iter().position(|s| s == "size:1:320x240").unwrap();
    let bind_pos = log
        .iter()
        .position(|s| s == "bind:1:1@Some(123.0)")
        .unwrap();
    assert!(size_pos < bind_pos, "size must precede bind: {log:?}");
    assert!(log.contains(&"perf:1:true".to_string()), "{log:?}");

    // Render on device #2 with the exact same format — zero cross-device errors, identical pixels!
    let (px2, _) = m
        .readback(id, &frame)
        .expect("device 2 render must succeed without device mismatch");
    assert_eq!(&px2.data[..4], &[255, 64, 128, 255]);
    assert!(m.take_error().is_empty());

    // Retained surface kept its CPU counter and received child texture teardown.
    assert_eq!(
        m.agent(kept, "").as_deref(),
        Some("ticks=1,children=true,each=true,losses=1")
    );
    let _ = Arc::new(());
}

#[test]
fn failed_post_recovery_bind_preserves_needs_rebind_for_the_next_device() {
    static FAIL_NEXT_BIND: AtomicBool = AtomicBool::new(false);
    static BIND_CALLS: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

    struct FlakyBind;
    impl Surface for FlakyBind {
        fn bind(&mut self, _: &[Value], _: Option<f64>) -> Result<(), SurfaceError> {
            BIND_CALLS.fetch_add(1, Ordering::Relaxed);
            if FAIL_NEXT_BIND.swap(false, Ordering::Relaxed) {
                return Err(SurfaceError("transient bind failure".into()));
            }
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

    static REG: Registry = Registry {
        surfaces: &[("flaky", 1, || Box::new(FlakyBind))],
        shaders: &[],
    };

    let Some(gpu1) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let Some(gpu2) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let Some(gpu3) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    BIND_CALLS.store(0, Ordering::Relaxed);
    FAIL_NEXT_BIND.store(false, Ordering::Relaxed);

    let mut m = Module::new(&REG);
    m.set_gpu(gpu1);
    let id = m.create_headless("flaky").unwrap();
    assert!(m.bind(id, &[Value::Number(1.)], Some(10.0)));
    assert_eq!(BIND_CALLS.load(Ordering::Relaxed), 1);

    // Lose device -> reconstruction marks needs_rebind = true.
    m.lose_device();
    assert!(m.instances[&id].needs_rebind);
    // Installing gpu2 attempts bind(), which fails; gpu2 is still installed, and needs_rebind must stay true.
    FAIL_NEXT_BIND.store(true, Ordering::Relaxed);
    m.set_gpu(gpu2);
    assert!(m.gpu.is_some());
    assert!(m.instances[&id].needs_rebind);
    assert!(!m.instances[&id].bound);

    // A second device loss (which now genuinely reconstructs because gpu2 is installed) + reattach replays bind cleanly.
    m.lose_device();
    assert!(m.instances[&id].needs_rebind);
    m.set_gpu(gpu3);
    assert!(!m.instances[&id].needs_rebind);
    assert!(m.instances[&id].bound);
    assert_eq!(BIND_CALLS.load(Ordering::Relaxed), 3);
}
