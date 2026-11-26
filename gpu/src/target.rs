#[cfg(target_os = "android")]
use crate::{buffers, Instance};
use crate::{shaders, swapchains, wgpu, Module, Surface, SurfaceRef, FRAME_LATENCY};

/// RGB primaries for a GPU surface's output. Range and transfer encoding
/// are selected separately by HDR and the render target's texture format.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum ColorSpace {
    /// sRGB primaries (also used by an extended-sRGB HDR target).
    #[default]
    Srgb,
    /// Display P3 primaries with the sRGB transfer function, in SDR.
    DisplayP3,
}

pub(crate) fn granted(config: Option<&wgpu::SurfaceConfiguration>) -> ColorSpace {
    match config.map(|c| c.color_space) {
        Some(wgpu::SurfaceColorSpace::DisplayP3) => ColorSpace::DisplayP3,
        _ => ColorSpace::Srgb,
    }
}

/// The compositor convention for the current render destination, including native readback.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ColorTarget {
    /// The granted RGB primaries.
    pub space: ColorSpace,
    /// The attachment view the module renders into.
    pub format: wgpu::TextureFormat,
    pub(crate) alpha: wgpu::CompositeAlphaMode,
}

impl Default for ColorTarget {
    fn default() -> Self {
        Self {
            space: ColorSpace::Srgb,
            format: wgpu::TextureFormat::Rgba8Unorm,
            alpha: wgpu::CompositeAlphaMode::PreMultiplied,
        }
    }
}

impl ColorTarget {
    /// Fragment output for a replacement draw into the presentation attachment.
    pub fn fragment(self, rgba: [f32; 4]) -> [f32; 4] {
        let [r, g, b, a] = rgba.map(f64::from);
        let space = match self.space {
            ColorSpace::Srgb => exact_color::MixSpace::Srgb,
            ColorSpace::DisplayP3 => exact_color::MixSpace::DisplayP3,
        };
        use wgpu::CompositeAlphaMode::*;
        let associated = match self.alpha {
            Opaque => false,
            // Metal's PostMultiplied selects a nonopaque layer. Core Animation
            // composites its associated pixels, like other layer contents.
            PostMultiplied => cfg!(target_vendor = "apple"),
            _ => true,
        };
        let [r, g, b] = space.from_linear_srgb([r, g, b]).map(|mut c| {
            if self.format != wgpu::TextureFormat::Rgba16Float {
                c = c.clamp(0.0, 1.0);
            }
            if associated {
                c *= a;
            }
            if self.format.is_srgb() {
                c = exact_color::srgb_to_linear(c);
            }
            c as f32
        });
        [r, g, b, if self.alpha == Opaque { 1.0 } else { a as f32 }]
    }

    /// The same attachment value for a render-pass clear.
    pub fn clear(self, rgba: [f32; 4]) -> wgpu::Color {
        let [r, g, b, a] = self.fragment(rgba).map(f64::from);
        wgpu::Color { r, g, b, a }
    }

    pub(crate) fn of(config: &wgpu::SurfaceConfiguration) -> Self {
        Self {
            space: granted(Some(config)),
            format: crate::view_format(config),
            alpha: config.alpha_mode,
        }
    }

    pub(crate) fn readback(format: wgpu::TextureFormat) -> Self {
        Self {
            format,
            ..Self::default()
        }
    }
}

impl crate::Instance {
    pub(crate) fn report_color_space(&mut self) -> Result<(), crate::SurfaceError> {
        let destination = self.config.as_ref().map_or_else(
            || ColorTarget::readback(self.offscreen_format()),
            ColorTarget::of,
        );
        self.set_color_target(destination)
    }

    pub(crate) fn set_color_target(
        &mut self,
        target: ColorTarget,
    ) -> Result<(), crate::SurfaceError> {
        self.surface.color_target(target);
        if self.destination != target {
            self.destination = target;
            if self.has_color_inputs {
                let result = self.surface.bind(&self.inputs, None);
                self.bound = result.is_ok();
                result?;
            }
        }
        Ok(())
    }
}

impl Module {
    /// Create a canvas's surface by name on a platform target; `None` (and
    /// [`Module::take_error`]) when the name is unknown or the target refused.
    pub fn create(
        &mut self,
        name: &str,
        target: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> Option<u32> {
        self.check_device();
        if let Some(error) = self.refused.get(name).cloned() {
            return self.fail(error);
        }
        let Some(gpu) = self.gpu.as_ref() else {
            return self.fail("no device");
        };
        let Some((_, _, factory)) = self.registry.surfaces.iter().find(|(n, _, _)| *n == name)
        else {
            return self.fail(format!("no surface named `{name}` in this module"));
        };
        // Every shader this module's surfaces bind against has its text, at
        // the interface the binary was built for (LLP 1030 D8): a pipeline
        // built over nothing would be wgpu's error, not a refusal by name.
        if let Some(why) = shaders::missing(self.registry.shaders) {
            return self.fail(why);
        }
        let Some(mut config) = target.get_default_config(&gpu.adapter, width.max(1), height.max(1))
        else {
            return self.fail("the adapter cannot present to this target");
        };
        let caps = target.get_capabilities(&gpu.adapter);
        if self.frame_samples && caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        let surface = factory();
        let hdr = configure(&*surface, &gpu.adapter, &caps, &mut config);
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = FRAME_LATENCY;
        swapchains(|| target.configure(&gpu.device, &config));
        let id = self.insert(name, surface, Some((target, config)))?;
        if let Some(inst) = self.instances.get_mut(&id) {
            inst.hdr = hdr;
            inst.alpha_modes = caps.alpha_modes;
        }
        Some(id)
    }

    /// Whether canvas `id` draws above SDR white: it asked, and its target
    /// is extended sRGB (LLP 1100 D12b).
    pub fn high_dynamic_range(&self, id: u32) -> bool {
        self.instances.get(&id).is_some_and(|i| i.hdr)
    }

    /// The headroom an HDR canvas draws its next frames to (LLP 1100 D12b);
    /// at least 1.
    pub fn set_headroom(&mut self, id: u32, headroom: f32) {
        if let Some(inst) = self.instances.get_mut(&id) {
            let h = if headroom.is_finite() {
                headroom.max(1.0)
            } else {
                1.0
            };
            if inst.headroom != h {
                inst.headroom = h;
                inst.invalidate();
            }
        }
    }

    /// Give a canvas made without a target ([`Module::create_headless`]) a
    /// platform target to present to, configured as [`Module::create`] does
    /// (a host whose windows come after the surface, as Android's do).
    pub fn attach(
        &mut self,
        id: u32,
        target: wgpu::Surface<'static>,
        width: u32,
        height: u32,
    ) -> bool {
        if let Some(error) = self.refused_instance(id) {
            self.fail::<()>(error);
            return false;
        }
        self.settle(id);
        self.release(id);
        self.check_device();
        let Some(gpu) = self.gpu.as_ref() else {
            return self.fail::<()>("no device").is_some();
        };
        if let Some(why) = shaders::missing(self.registry.shaders) {
            return self.fail::<()>(why).is_some();
        }
        let Some(mut config) = target.get_default_config(&gpu.adapter, width.max(1), height.max(1))
        else {
            return self
                .fail::<()>("the adapter cannot present to this target")
                .is_some();
        };
        let caps = target.get_capabilities(&gpu.adapter);
        if self.frame_samples && caps.usages.contains(wgpu::TextureUsages::COPY_SRC) {
            config.usage |= wgpu::TextureUsages::COPY_SRC;
        }
        config.present_mode = wgpu::PresentMode::AutoVsync;
        config.desired_maximum_frame_latency = FRAME_LATENCY;
        let features = gpu.device.features();
        let limits = gpu.device.limits();
        let Some(inst) = self.instances.get_mut(&id) else {
            return self.fail::<()>(format!("no canvas {id}")).is_some();
        };
        inst.alpha_modes = caps.alpha_modes.clone();
        inst.hdr = configure(&*inst.surface, &gpu.adapter, &caps, &mut config);
        swapchains(|| target.configure(&gpu.device, &config));
        inst.seen = Default::default();
        inst.presentation = Some(SurfaceRef::new(target));
        inst.config = Some(config);
        inst.surface.device_ready(features, limits);
        if let Err(e) = inst.report_color_space() {
            self.error = e.0;
            return false;
        }
        inst.invalidate();
        true
    }

    /// Drop a canvas's target (its window is going); its state stays. This
    /// never waits for the compositor ([`release`]).
    pub fn detach(&mut self, id: u32) {
        self.settle(id);
        self.release(id);
        if let Some(inst) = self.instances.get_mut(&id).filter(|i| i.has_color_inputs) {
            if let Err(e) = inst.report_color_space() {
                self.error = e.0;
            }
        }
    }

    /// Give a canvas made without a target buffers to render into, which a
    /// reader draws itself ([`buffers`]); `false` when the device cannot
    /// (the canvas then needs a window).
    #[cfg(target_os = "android")]
    pub fn attach_buffers(&mut self, id: u32, width: u32, height: u32) -> bool {
        // A frame rendered into the target these replace is submitted into
        // it, and presented, first (`detach`).
        self.settle(id);
        self.check_device();
        let Some(gpu) = self.gpu.as_ref() else {
            return self.fail::<()>("no device").is_some();
        };
        if let Some(why) = shaders::missing(self.registry.shaders) {
            return self.fail::<()>(why).is_some();
        }
        let format = self
            .instances
            .get(&id)
            .map_or(buffers::FORMAT, Instance::offscreen_format);
        let Some(ring) = buffers::Ring::new(gpu, width, height, format) else {
            return self
                .fail::<()>("the device cannot render into hardware buffers")
                .is_some();
        };
        let features = gpu.device.features();
        let Some(inst) = self.buffers_ready(id, features) else {
            return false;
        };
        inst.ring = Some(ring);
        true
    }

    /// Replace a presentation target with the hardware buffers' SDR policy.
    #[cfg(any(target_os = "android", all(test, target_os = "macos")))]
    pub(crate) fn buffers_ready(
        &mut self,
        id: u32,
        features: wgpu::Features,
    ) -> Option<&mut crate::Instance> {
        if let Some(error) = self.refused_instance(id) {
            return self.fail(error);
        }
        self.release(id);
        if !self.instances.contains_key(&id) {
            return self.fail(format!("no canvas {id}"));
        }
        let limits = self.gpu.as_ref()?.device.limits();
        let inst = self.instances.get_mut(&id).unwrap();
        inst.hdr = false;
        inst.surface.device_ready(features, limits);
        if let Err(e) = inst.set_color_target(ColorTarget::readback(inst.offscreen_format())) {
            self.error = e.0;
            return None;
        }
        inst.invalidate();
        Some(inst)
    }
}

pub(crate) fn configure(
    surface: &dyn Surface,
    adapter: &wgpu::Adapter,
    caps: &wgpu::SurfaceCapabilities,
    config: &mut wgpu::SurfaceConfiguration,
) -> bool {
    let views = adapter
        .get_downlevel_capabilities()
        .flags
        .contains(wgpu::DownlevelFlags::SURFACE_VIEW_FORMATS);
    let hdr = surface.high_dynamic_range()
        && caps
            .color_spaces(wgpu::TextureFormat::Rgba16Float)
            .contains(wgpu::SurfaceColorSpaces::EXTENDED_SRGB);
    if hdr {
        config.format = wgpu::TextureFormat::Rgba16Float;
        config.color_space = wgpu::SurfaceColorSpace::ExtendedSrgb;
        config.view_formats.clear();
    } else {
        let native_srgb = adapter.get_info().backend == wgpu::Backend::Vulkan;
        let (format, view) = sdr_format(
            surface.srgb_target(),
            views,
            native_srgb,
            &caps.formats,
            config.format,
        );
        config.format = format;
        config.view_formats = view.into_iter().collect();
        config.color_space = sdr_color_space(surface.color_space(), caps.color_spaces(format));
    }
    config.alpha_mode = alpha_mode(surface.opaque(), &caps.alpha_modes);
    hdr
}

fn sdr_color_space(
    wanted: ColorSpace,
    offered: wgpu::SurfaceColorSpaces,
) -> wgpu::SurfaceColorSpace {
    if wanted == ColorSpace::DisplayP3 && offered.contains(wgpu::SurfaceColorSpaces::DISPLAY_P3) {
        wgpu::SurfaceColorSpace::DisplayP3
    } else {
        wgpu::SurfaceColorSpace::Auto
    }
}

/// How the compositor takes a canvas's alpha: ignored when the surface
/// promises every pixel opaque, else blended where the target can, else
/// the window's own policy (`Inherit`: Android's `Opaque`/`Inherit` pair),
/// else whatever the target offers. Never `Auto`: wgpu-core resolves it to
/// `Opaque` before `Inherit`, which would ignore a translucent canvas's
/// alpha, and wgpu-hal's Vulkan backend cannot map it at all.
pub(crate) fn alpha_mode(
    opaque: bool,
    modes: &[wgpu::CompositeAlphaMode],
) -> wgpu::CompositeAlphaMode {
    use wgpu::CompositeAlphaMode::{Inherit, Opaque, PostMultiplied, PreMultiplied};
    let blended = [PreMultiplied, PostMultiplied, Inherit];
    [Opaque]
        .iter()
        .filter(|_| opaque)
        .chain(&blended)
        .chain(modes.first())
        .copied()
        .find(|m| modes.contains(m))
        .unwrap_or(Inherit)
}

fn sdr_format(
    srgb: bool,
    views: bool,
    native_srgb: bool,
    formats: &[wgpu::TextureFormat],
    fallback: wgpu::TextureFormat,
) -> (wgpu::TextureFormat, Option<wgpu::TextureFormat>) {
    // Ordinary canvases encode their own output, as on the web. A surface
    // requesting linear-light rendering can instead use an sRGB target.
    let format = formats
        .iter()
        .copied()
        .find(|f| !f.is_srgb())
        .or_else(|| formats.first().copied())
        .unwrap_or(fallback);
    let encoded = format.add_srgb_suffix();
    if srgb && encoded != format {
        if views {
            return (format, Some(encoded));
        }
        // Vulkan presents encoded bytes. GLES/WebGL encodes at present,
        // so a native sRGB attachment there would encode twice.
        if native_srgb && formats.contains(&encoded) {
            return (encoded, None);
        }
    }
    (format, None)
}

impl crate::Module {
    /// Instance policy: bit 0 promises alpha one; bit 1 requests SDR;
    /// bit 2 discards any retained picture in favour of the canvas's fallback.
    pub fn presentation_flags(&self, id: u32) -> u32 {
        self.instances.get(&id).map_or(0, |i| {
            u32::from(i.surface.opaque())
                | (u32::from(!i.surface.high_dynamic_range()) << 1)
                | (u32::from(i.surface.is_empty()) << 2)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opaque_output_keeps_straight_rgb_and_writes_full_alpha() {
        let target = ColorTarget {
            alpha: wgpu::CompositeAlphaMode::Opaque,
            ..Default::default()
        };
        assert_eq!(target.fragment([1.0, 0.0, 0.0, 0.5]), [1.0, 0.0, 0.0, 1.0]);
    }

    #[test]
    fn sdr_color_space_requires_both_the_request_and_the_targets_capability() {
        use wgpu::SurfaceColorSpace::{Auto, DisplayP3};
        use wgpu::SurfaceColorSpaces as Offered;
        let both = Offered::SRGB | Offered::DISPLAY_P3;
        assert_eq!(sdr_color_space(ColorSpace::DisplayP3, both), DisplayP3);
        assert_eq!(sdr_color_space(ColorSpace::Srgb, both), Auto);
        assert_eq!(sdr_color_space(ColorSpace::DisplayP3, Offered::SRGB), Auto);
        assert_eq!(
            sdr_color_space(ColorSpace::DisplayP3, Offered::empty()),
            Auto
        );
    }

    #[test]
    fn alpha_modes_are_ones_the_target_lists() {
        use wgpu::CompositeAlphaMode::{Inherit, Opaque, PostMultiplied, PreMultiplied};
        // Android's windows; Metal's layers; a Vulkan display with all.
        let android = [Opaque, Inherit];
        let metal = [Opaque, PostMultiplied];
        let all = [Opaque, PreMultiplied, PostMultiplied, Inherit];
        assert_eq!(alpha_mode(true, &android), Opaque);
        assert_eq!(alpha_mode(false, &android), Inherit);
        assert_eq!(alpha_mode(true, &metal), Opaque);
        assert_eq!(alpha_mode(false, &metal), PostMultiplied);
        assert_eq!(alpha_mode(false, &all), PreMultiplied);
        assert_eq!(alpha_mode(true, &[Inherit, PreMultiplied]), PreMultiplied);
        assert_eq!(alpha_mode(false, &[Opaque]), Opaque);
    }

    #[test]
    fn srgb_targets_respect_swapchain_format_support() {
        use wgpu::TextureFormat::{Bgra8Unorm as Linear, Bgra8UnormSrgb as Srgb};
        let both = [Linear, Srgb];
        assert_eq!(
            sdr_format(true, false, false, &both, Linear),
            (Linear, None)
        );
        assert_eq!(
            sdr_format(true, true, false, &both, Linear),
            (Linear, Some(Srgb))
        );
        assert_eq!(sdr_format(true, false, true, &both, Linear), (Srgb, None));
        assert_eq!(
            sdr_format(true, false, true, &[Linear], Linear),
            (Linear, None)
        );
        assert_eq!(sdr_format(false, true, false, &both, Srgb), (Linear, None));
        assert_eq!(sdr_format(true, false, true, &[Srgb], Srgb), (Srgb, None));
        assert_eq!(
            sdr_format(
                false,
                false,
                false,
                &[Srgb],
                wgpu::TextureFormat::Rgba16Float
            ),
            (Srgb, None),
            "a restored target cannot keep an unsupported old format"
        );
    }
}
