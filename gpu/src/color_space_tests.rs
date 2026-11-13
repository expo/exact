use crate::{fixture, wgpu, ColorSpace, Frame, Module, Registry, Surface, SurfaceError, Value};
use std::ffi::c_void;

#[link(name = "QuartzCore", kind = "framework")]
extern "C" {}
#[link(name = "CoreGraphics", kind = "framework")]
extern "C" {
    static kCGColorSpaceDisplayP3: *const c_void;
    static kCGColorSpaceSRGB: *const c_void;
    static kCGColorSpaceExtendedSRGB: *const c_void;
    fn CGColorSpaceCopyName(space: *const c_void) -> *const c_void;
}
#[link(name = "CoreFoundation", kind = "framework")]
extern "C" {
    fn CFEqual(a: *const c_void, b: *const c_void) -> u8;
    fn CFRelease(value: *const c_void);
}
#[repr(C)]
struct CGColorSpace;
// SAFETY: the Objective-C encoding of CoreGraphics' opaque CGColorSpaceRef.
unsafe impl objc2::encode::RefEncode for CGColorSpace {
    const ENCODING_REF: objc2::encode::Encoding =
        objc2::encode::Encoding::Pointer(&objc2::encode::Encoding::Struct("CGColorSpace", &[]));
}

type Layer = objc2::rc::Retained<objc2::runtime::AnyObject>;
fn layer() -> Layer {
    // SAFETY: CAMetalLayer is linked above; new returns an owned layer.
    unsafe { objc2::msg_send![objc2::class!(CAMetalLayer), new] }
}
fn target(m: &Module, layer: &Layer) -> wgpu::Surface<'static> {
    // SAFETY: callers keep the layer alive until after the module drops its target.
    unsafe {
        m.gpu().unwrap().instance.create_surface_unsafe(
            wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(
                objc2::rc::Retained::as_ptr(layer) as *mut c_void
            ),
        )
    }
    .unwrap()
}
fn assert_space(layer: &Layer, name: *const c_void, hdr: bool) {
    // SAFETY: the layer owns the space; CopyName returns an owned CFString.
    unsafe {
        let space: *const CGColorSpace = objc2::msg_send![&**layer, colorspace];
        assert!(!space.is_null());
        let actual = CGColorSpaceCopyName(space.cast());
        assert!(!actual.is_null());
        let equal = CFEqual(actual, name) != 0;
        CFRelease(actual);
        assert!(equal, "the layer's actual color space");
        let edr: bool = objc2::msg_send![&**layer, wantsExtendedDynamicRangeContent];
        assert_eq!(edr, hdr);
    }
}

#[derive(Default)]
struct Probe {
    wanted: ColorSpace,
    granted: ColorSpace,
    prepared: ColorSpace,
    hdr: bool,
    events: String,
}
impl Surface for Probe {
    fn color_space(&self) -> ColorSpace {
        self.wanted
    }
    fn high_dynamic_range(&self) -> bool {
        self.hdr
    }
    fn device_ready(&mut self, _: wgpu::Features) {
        self.granted = ColorSpace::Srgb;
        self.events.push_str("ready;");
    }
    fn retier(&mut self) -> bool {
        true
    }
    fn color_space_granted(&mut self, space: ColorSpace) {
        self.granted = space;
        self.events.push_str(&format!("{space:?};"));
    }
    fn prepare_assets(&mut self, _: &wgpu::Device, _: &wgpu::Queue, _: wgpu::TextureFormat) {
        self.prepared = self.granted;
        self.events.push_str("prepare;");
    }
    fn agent(&mut self, command: &str) -> Option<String> {
        if command == "prepared" {
            return Some(format!("{:?}", self.prepared));
        }
        Some(std::mem::take(&mut self.events))
    }
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
        let color = if self.granted == ColorSpace::DisplayP3 {
            wgpu::Color::RED
        } else {
            wgpu::Color::GREEN
        };
        encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(color),
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        false
    }
}
static REGISTRY: Registry = Registry {
    surfaces: &[
        ("p3", 0, || {
            Box::new(Probe {
                wanted: ColorSpace::DisplayP3,
                ..Default::default()
            })
        }),
        ("srgb", 0, || Box::<Probe>::default()),
        ("both", 0, || {
            Box::new(Probe {
                wanted: ColorSpace::DisplayP3,
                hdr: true,
                ..Default::default()
            })
        }),
    ],
    shaders: &[],
};

#[test]
fn metal_grants_sdr_p3_and_reports_it_after_attachment_and_restoration() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    // Layers outlive the module, including when an assertion unwinds.
    let layers = [layer(), layer(), layer()];
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let ids: Vec<_> = ["p3", "srgb", "both"]
        .iter()
        .zip(&layers)
        .map(|(name, layer)| {
            let t = target(&m, layer);
            let id = m.create(name, t, 8, 8).unwrap();
            m.instances.get_mut(&id).unwrap().layer =
                Some(objc2::rc::Retained::as_ptr(layer) as usize);
            id
        })
        .collect();
    // SAFETY: process-lifetime CoreGraphics constants.
    let names = unsafe {
        [
            kCGColorSpaceDisplayP3,
            kCGColorSpaceSRGB,
            kCGColorSpaceExtendedSRGB,
        ]
    };
    for (index, expected) in ["DisplayP3", "Srgb", "Srgb"].iter().enumerate() {
        assert_space(&layers[index], names[index], index == 2);
        assert_eq!(
            m.agent(ids[index], "state").unwrap(),
            format!("ready;{expected};")
        );
    }
    assert_eq!(
        m.instances[&ids[0]].config.as_ref().unwrap().format,
        wgpu::TextureFormat::Bgra8Unorm
    );
    assert_eq!(
        m.instances[&ids[2]].config.as_ref().unwrap().format,
        wgpu::TextureFormat::Rgba16Float
    );
    assert!(m.high_dynamic_range(ids[2]));

    let gpu = m.gpu().unwrap();
    let replacement = crate::Gpu {
        instance: gpu.instance.clone(),
        adapter: gpu.adapter.clone(),
        device: gpu.device.clone(),
        queue: gpu.queue.clone(),
    };
    m.set_gpu(replacement);
    assert_eq!(
        m.agent(ids[0], "state").as_deref(),
        Some("ready;DisplayP3;prepare;")
    );
    m.lose_device();
    let report = crate::block_on(m.recover()).unwrap_or_else(|e| panic!("{}", e.message()));
    assert!(report.contains("ready;DisplayP3;prepare;"), "{report}");
    for (index, layer) in layers.iter().enumerate() {
        assert_space(layer, names[index], index == 2);
    }

    // Reuse one layer across P3, ordinary SDR, and P3 again.
    m.destroy(ids[0]);
    let srgb = m.create_headless("srgb").unwrap();
    let t = target(&m, &layers[0]);
    assert!(m.attach(srgb, t, 8, 8));
    assert_space(&layers[0], names[1], false);
    m.destroy(srgb);
    let p3 = m.create_headless("p3").unwrap();
    m.agent(p3, "state");
    let t = target(&m, &layers[0]);
    assert!(m.attach(p3, t, 8, 8));
    assert_eq!(m.agent(p3, "state").as_deref(), Some("ready;DisplayP3;"));
    assert_space(&layers[0], names[0], false);
}

#[test]
fn target_capabilities_gate_p3_and_hdr_independently() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let layer = layer();
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let t = target(&m, &layer);
    let gpu = m.gpu().unwrap();
    let mut caps = t.get_capabilities(&gpu.adapter);
    let mut config = t.get_default_config(&gpu.adapter, 8, 8).unwrap();
    let probe = Probe {
        wanted: ColorSpace::DisplayP3,
        hdr: true,
        ..Default::default()
    };
    for c in &mut caps.format_capabilities {
        c.color_spaces
            .remove(wgpu::SurfaceColorSpaces::EXTENDED_SRGB);
    }
    assert!(!crate::target::configure(
        &probe,
        &gpu.adapter,
        &caps,
        &mut config
    ));
    assert_eq!(
        crate::target::granted(Some(&config)),
        ColorSpace::DisplayP3,
        "HDR refused, P3 granted"
    );
    for c in &mut caps.format_capabilities {
        c.color_spaces = wgpu::SurfaceColorSpaces::SRGB;
    }
    assert!(!crate::target::configure(
        &probe,
        &gpu.adapter,
        &caps,
        &mut config
    ));
    assert_eq!(
        crate::target::granted(Some(&config)),
        ColorSpace::Srgb,
        "neither offered"
    );
    t.configure(&gpu.device, &config);
    assert_space(&layer, unsafe { kCGColorSpaceSRGB }, false);
}

#[test]
fn android_shaped_hardware_buffer_attachment_never_grants_p3() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let layers = [layer(), layer()];
    let features = gpu.device.features();
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    for (name, layer) in ["p3", "both"].iter().zip(&layers) {
        let t = target(&m, layer);
        let id = m.create(name, t, 8, 8).unwrap();
        assert_eq!(m.high_dynamic_range(id), *name == "both");
        let expected = if *name == "p3" { "DisplayP3" } else { "Srgb" };
        assert_eq!(m.agent(id, "state").unwrap(), format!("ready;{expected};"));
        // Run the replacement that attach_buffers uses after allocation.
        // Only allocation/import of AHardwareBuffers needs Android.
        let inst = m.buffers_ready(id, features).unwrap();
        assert!(inst.config.is_none());
        assert!(inst.presentation.is_none());
        assert!(!m.high_dynamic_range(id));
        assert_eq!(m.agent(id, "state").as_deref(), Some("ready;Srgb;"));
    }
}

#[test]
fn p3_presentation_uses_srgb_for_native_capture_then_restores_its_grant() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let layer = layer();
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let t = target(&m, &layer);
    let id = m.create("p3", t, 8, 8).unwrap();
    assert!(m.bind(id, &[], None));
    m.agent(id, "state");
    let frame = Frame {
        width: 8.,
        height: 8.,
        scale: 1.,
        now_ms: 0.,
        seekable: true,
        period_ms: 0.,
        children_generation: 0,
        shader_generation: 0,
        headroom: 1.,
    };
    let (pixels, _) = m.readback(id, &frame).unwrap();
    assert_eq!(
        pixels.at(4, 4),
        [0, 255, 0, 255],
        "the shader received sRGB for capture"
    );
    assert_eq!(
        m.agent(id, "state").as_deref(),
        Some("Srgb;prepare;DisplayP3;")
    );
    assert_space(&layer, unsafe { kCGColorSpaceDisplayP3 }, false);
    m.detach(id);
    let (pixels, _) = m.readback(id, &frame).unwrap();
    assert_eq!(
        pixels.at(4, 4),
        [0, 255, 0, 255],
        "a detached P3 surface receives sRGB for offscreen capture"
    );
    assert_eq!(m.agent(id, "state").as_deref(), Some("Srgb;prepare;"));
}

#[test]
fn retier_restores_p3_before_preparing_the_replacement_renderer() {
    let Some(gpu) = fixture::device_or_skip(fixture::device()) else {
        return;
    };
    let layer = layer();
    let mut m = Module::new(&REGISTRY);
    m.set_gpu(gpu);
    let t = target(&m, &layer);
    let id = m.create("p3", t, 8, 8).unwrap();
    assert_eq!(m.agent(id, "state").as_deref(), Some("ready;DisplayP3;"));
    m.retier();
    assert_eq!(m.agent(id, "prepared").as_deref(), Some("DisplayP3"));
    assert_eq!(
        m.agent(id, "state").as_deref(),
        Some("ready;DisplayP3;prepare;")
    );
}
