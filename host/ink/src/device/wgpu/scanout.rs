//! Ink frames on a Vulkan display plane. @ref LLP 1015 §6.
use super::panel_blit::Blit;
use super::{block_on, checked, failure, Error, Instant, Result, TextureId, Wgpu};
use crate::Rotation;
use ::wgpu::hal::api::Vulkan;
use ash::{ext, khr, vk};
use std::num::NonZeroU64;
use std::os::fd::{AsRawFd, OwnedFd};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// A configured FIFO swapchain for one connector and mode.
pub struct Scanout {
    output: Arc<Output>,
    blitter: Blit,
    next_id: u64,
    upload: Option<::wgpu::Texture>,
    size: [u32; 2],
    queue: ::wgpu::Queue,
    errors: Arc<Mutex<Option<Error>>>,
}

struct Output {
    // A receipt holds this configuration until its wait finishes. Replacing the
    // output creates a new surface, so a waiter never uses a retired swapchain.
    surface: ::wgpu::Surface<'static>,
    device: ::wgpu::Device,
    wait: khr::present_wait::Device,
    chain: vk::SwapchainKHR,
    _card: OwnedFd,
}

/// A frame accepted by the presentation queue.
pub struct QueuedFrame {
    output: Arc<Output>,
    id: NonZeroU64,
}

impl QueuedFrame {
    /// Whether Vulkan has completed this presentation within the given time.
    pub fn wait(&self, timeout: Duration) -> Result<bool> {
        let ns = u64::try_from(timeout.as_nanos()).unwrap_or(u64::MAX);
        // SAFETY: the immutable output owns the swapchain and its device.
        // This ID was submitted to this FIFO swapchain with presentWait enabled.
        match unsafe {
            self.output
                .wait
                .wait_for_present(self.output.chain, self.id.get(), ns)
        } {
            Ok(()) => Ok(true),
            Err(vk::Result::TIMEOUT) => Ok(false),
            Err(e) => Err(vk_error(e)),
        }
    }
}

fn vk_error(e: vk::Result) -> Error {
    match e {
        vk::Result::ERROR_DEVICE_LOST => Error::DeviceLost(e.to_string()),
        _ => Error::Device(e.to_string()),
    }
}

impl Scanout {
    /// Open Ink on the adapter that can present to the connector.
    ///
    /// # Safety
    ///
    /// `card` must be the DRM master for this connected display and the given
    /// mode. Its owner must leave the connector and its DRM events to Vulkan
    /// until the scanout and its queued frames have been dropped.
    pub unsafe fn open(
        card: OwnedFd,
        connector: u32,
        size: [u32; 2],
        refresh_millihz: u32,
    ) -> Result<(Wgpu, Self)> {
        if std::env::var("WGPU_BACKEND").is_ok_and(|s| {
            !::wgpu::Backends::from_comma_list(&s).contains(::wgpu::Backends::VULKAN)
        }) || std::env::var("EXACT_INK_DEVICE").is_ok_and(|s| s != "wgpu")
        {
            return Err(Error::Unsupported(
                "Vulkan scanout with the selected Ink backend",
            ));
        }
        let started = Instant::now();
        let instance = ::wgpu::Instance::new(::wgpu::InstanceDescriptor {
            backends: ::wgpu::Backends::VULKAN,
            ..::wgpu::InstanceDescriptor::new_without_display_handle()
        });
        for adapter in block_on(instance.enumerate_adapters(::wgpu::Backends::VULKAN)) {
            let Some(plane) = display_plane(&adapter, &card, connector)? else {
                continue;
            };
            // SAFETY: the caller supplies the master and mode. Vulkan reported
            // this plane for the connector. Output keeps the descriptor alive.
            let surface = unsafe {
                instance.create_surface_unsafe(::wgpu::SurfaceTargetUnsafe::Drm {
                    fd: card.as_raw_fd(),
                    plane,
                    connector_id: connector,
                    width: size[0],
                    height: size[1],
                    refresh_rate: refresh_millihz,
                })
            }
            .map_err(|e| Error::Device(e.to_string()))?;
            return Self::configure(instance, adapter, surface, size, card, started);
        }
        Err(Error::Unsupported(
            "no Vulkan plane supports the DRM connector",
        ))
    }

    fn configure(
        instance: ::wgpu::Instance,
        adapter: ::wgpu::Adapter,
        surface: ::wgpu::Surface<'static>,
        size: [u32; 2],
        card: OwnedFd,
        started: Instant,
    ) -> Result<(Wgpu, Self)> {
        let caps = surface.get_capabilities(&adapter);
        // Ink's target holds the same RGBA bytes as a screenshot. An sRGB view
        // would encode those bytes a second time before scanout.
        let format = caps
            .formats
            .into_iter()
            .find(|f| {
                matches!(
                    f,
                    ::wgpu::TextureFormat::Bgra8Unorm | ::wgpu::TextureFormat::Rgba8Unorm
                )
            })
            .ok_or(Error::Unsupported("an 8-bit UNORM scanout format"))?;
        let alpha_mode = [
            ::wgpu::CompositeAlphaMode::Opaque,
            ::wgpu::CompositeAlphaMode::Inherit,
        ]
        .into_iter()
        .find(|v| caps.alpha_modes.contains(v))
        .ok_or(Error::Unsupported("opaque scanout"))?;
        let (device, queue, wait) = open_device(&adapter)?;
        let ink = block_on(Wgpu::from_device(
            instance,
            adapter,
            device.clone(),
            queue.clone(),
            started,
        ))?;
        let config = ::wgpu::SurfaceConfiguration {
            usage: ::wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size[0],
            height: size[1],
            color_space: ::wgpu::SurfaceColorSpace::Srgb,
            present_mode: ::wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 1,
            alpha_mode,
            view_formats: Vec::new(),
        };
        let blitter = checked(&device, &ink.errors, || {
            surface.configure(&device, &config);
            Blit::new(&device, format)
        })?;
        // SAFETY: this borrows the configured native surface and reads its handle.
        let chain = unsafe { surface.as_hal::<Vulkan>() }
            .and_then(|s| s.raw_native_swapchain())
            .ok_or(Error::Unsupported("a native Vulkan swapchain"))?;
        let errors = ink.errors.clone();
        Ok((
            ink,
            Self {
                output: Arc::new(Output {
                    surface,
                    device,
                    wait,
                    chain,
                    _card: card,
                }),
                blitter,
                next_id: 1,
                upload: None,
                size,
                queue,
                errors,
            },
        ))
    }

    /// A CPU-composited content region or fallback painter uses the same display.
    pub fn present_pixels(&mut self, pixels: &tiny_skia::Pixmap) -> Result<QueuedFrame> {
        if self.size != [pixels.width(), pixels.height()] {
            return Err(Error::Invalid(
                "panel pixels have a different extent from the display".into(),
            ));
        }
        let extent = ::wgpu::Extent3d {
            width: pixels.width(),
            height: pixels.height(),
            depth_or_array_layers: 1,
        };
        let texture = self.upload.get_or_insert_with(|| {
            self.output
                .device
                .create_texture(&::wgpu::TextureDescriptor {
                    label: Some("Linux panel pixels"),
                    size: extent,
                    mip_level_count: 1,
                    sample_count: 1,
                    dimension: ::wgpu::TextureDimension::D2,
                    format: ::wgpu::TextureFormat::Rgba8Unorm,
                    usage: ::wgpu::TextureUsages::COPY_DST | ::wgpu::TextureUsages::TEXTURE_BINDING,
                    view_formats: &[],
                })
        });
        self.queue.write_texture(
            texture.as_image_copy(),
            pixels.data(),
            ::wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(pixels.width() * 4),
                rows_per_image: None,
            },
            extent,
        );
        let view = texture.create_view(&Default::default());
        self.present(&view)
    }

    /// Copy the completed Ink image to the swapchain and label its presentation.
    pub fn present(&mut self, source: &::wgpu::TextureView) -> Result<QueuedFrame> {
        self.present_rotated(source, Rotation::R0)
    }

    /// Rotate a logical Ink image as it is copied to the physical panel.
    pub fn present_rotated(
        &mut self,
        source: &::wgpu::TextureView,
        rotation: Rotation,
    ) -> Result<QueuedFrame> {
        failure(&self.errors)?;
        let size = source.texture().size();
        if rotation.size([size.width, size.height]) != self.size {
            return Err(Error::Invalid(
                "rotated Ink image has a different extent from the display".into(),
            ));
        }
        let output = &self.output;
        let image = match output.surface.get_current_texture() {
            ::wgpu::CurrentSurfaceTexture::Success(t)
            | ::wgpu::CurrentSurfaceTexture::Suboptimal(t) => t,
            other => return Err(Error::Device(format!("acquire scanout: {other:?}"))),
        };
        let id = NonZeroU64::new(self.next_id)
            .ok_or(Error::Invalid("Vulkan presentation IDs exhausted".into()))?;
        self.next_id = self.next_id.wrapping_add(1);
        // SAFETY: open_device enabled presentId. IDs increase on this
        // swapchain and each acquired image goes to the queue exactly once.
        let tagged = unsafe {
            output
                .surface
                .as_hal::<Vulkan>()
                .is_some_and(|s| s.set_next_present_id(id))
        };
        if !tagged {
            return Err(Error::Device(
                "could not label the Vulkan presentation".into(),
            ));
        }
        let view = image.texture.create_view(&Default::default());
        let mut encoder = output.device.create_command_encoder(&Default::default());
        self.blitter
            .copy(&output.device, &mut encoder, source, &view, rotation);
        self.queue.submit([encoder.finish()]);
        self.queue.present(image);
        failure(&self.errors)?;
        Ok(QueuedFrame {
            output: self.output.clone(),
            id,
        })
    }
}

impl Wgpu {
    /// The finished, optionally rotated Ink target for a scanout on this device.
    pub fn scanout_view(&self, id: TextureId) -> Result<::wgpu::TextureView> {
        Ok(self.texture(id)?.view.clone())
    }
}

fn display_plane(adapter: &::wgpu::Adapter, card: &OwnedFd, connector: u32) -> Result<Option<u32>> {
    // SAFETY: this guard only borrows the instance and physical device.
    let hal =
        unsafe { adapter.as_hal::<Vulkan>() }.ok_or(Error::Unsupported("a Vulkan adapter"))?;
    let instance = hal.shared_instance();
    if !instance
        .extensions()
        .contains(&ext::acquire_drm_display::NAME)
    {
        return Ok(None);
    }
    let drm = ext::acquire_drm_display::Instance::new(instance.entry(), instance.raw_instance());
    let physical = hal.raw_physical_device();
    // SAFETY: the live DRM card and its connector come from the carrier.
    let display = match unsafe { drm.get_drm_display(physical, card.as_raw_fd(), connector) } {
        Ok(display) if display != vk::DisplayKHR::null() => display,
        Ok(_)
        | Err(
            vk::Result::ERROR_UNKNOWN
            | vk::Result::ERROR_EXTENSION_NOT_PRESENT
            | vk::Result::ERROR_INITIALIZATION_FAILED,
        ) => return Ok(None),
        Err(e) => return Err(vk_error(e)),
    };
    let api = khr::display::Instance::new(instance.entry(), instance.raw_instance());
    // SAFETY: enumerate the planes and supported displays of the same physical device.
    let planes =
        unsafe { api.get_physical_device_display_plane_properties(physical) }.map_err(vk_error)?;
    for (index, plane) in planes.iter().enumerate() {
        if plane.current_display != vk::DisplayKHR::null() && plane.current_display != display {
            continue;
        }
        let index = index as u32;
        if unsafe { api.get_display_plane_supported_displays(physical, index) }
            .map_err(vk_error)?
            .contains(&display)
        {
            return Ok(Some(index));
        }
    }
    Ok(None)
}

fn open_device(
    adapter: &::wgpu::Adapter,
) -> Result<(::wgpu::Device, ::wgpu::Queue, khr::present_wait::Device)> {
    let desc = Wgpu::device_descriptor(adapter)?;
    // SAFETY: the adapter guards its raw handles. The callback adds only two
    // features that this physical device reports. Both feature structs live
    // through vkCreateDevice and all of wgpu's requested features remain enabled.
    let (open, wait) = unsafe {
        let hal = adapter
            .as_hal::<Vulkan>()
            .ok_or(Error::Unsupported("a Vulkan adapter"))?;
        let supported = hal.physical_device_capabilities();
        let shared = hal.shared_instance();
        if shared.instance_api_version() < vk::API_VERSION_1_1
            || ![khr::present_id::NAME, khr::present_wait::NAME]
                .iter()
                .all(|ext| supported.supports_extension(ext))
        {
            return Err(Error::Unsupported("Vulkan presentation feedback"));
        }
        let instance = shared.raw_instance();
        let mut id = vk::PhysicalDevicePresentIdFeaturesKHR::default();
        let mut wait = vk::PhysicalDevicePresentWaitFeaturesKHR::default();
        let mut query = vk::PhysicalDeviceFeatures2::default()
            .push_next(&mut id)
            .push_next(&mut wait);
        instance.get_physical_device_features2(hal.raw_physical_device(), &mut query);
        if id.present_id != vk::TRUE || wait.present_wait != vk::TRUE {
            return Err(Error::Unsupported("Vulkan presentation feedback"));
        }
        let mut id = vk::PhysicalDevicePresentIdFeaturesKHR::default().present_id(true);
        let mut wait = vk::PhysicalDevicePresentWaitFeaturesKHR::default().present_wait(true);
        let open = hal
            .open_with_callback(
                desc.required_features,
                &desc.required_limits,
                &desc.memory_hints,
                Some(Box::new(|args| {
                    args.extensions
                        .extend([khr::present_id::NAME, khr::present_wait::NAME]);
                    *args.create_info = args.create_info.push_next(&mut id).push_next(&mut wait);
                })),
            )
            .map_err(|e| Error::Device(e.to_string()))?;
        let wait = khr::present_wait::Device::new(instance, open.device.raw_device());
        (open, wait)
    };
    // SAFETY: open belongs to this adapter and was created with this descriptor.
    let (device, queue) = unsafe { adapter.create_device_from_hal::<Vulkan>(open, &desc) }
        .map_err(|e| Error::Device(e.to_string()))?;
    Ok((device, queue, wait))
}
