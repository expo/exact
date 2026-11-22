#![cfg(drm)]

use alloc::{format, string::ToString, sync::Arc};
use core::num::NonZeroU32;
use std::os::fd::{AsFd, AsRawFd, BorrowedFd, OwnedFd};

use ash::{ext, khr, vk};

use drm::{
    self,
    control::{self, connector, Device as _},
};

struct Card(i32);
impl AsFd for Card {
    fn as_fd(&self) -> BorrowedFd<'_> {
        unsafe { BorrowedFd::borrow_raw(self.0) }
    }
}
impl control::Device for Card {}
impl drm::Device for Card {}

// EXACT (EXACT-PATCHES.md, 18). A display can be driven by a different DRM
// device from the one that renders. Vulkan answers for the connector itself.
fn connector_display(
    devices: impl IntoIterator<Item = vk::PhysicalDevice>,
    connector: u32,
    mut lookup: impl FnMut(vk::PhysicalDevice) -> Result<vk::DisplayKHR, vk::Result>,
) -> Result<(vk::PhysicalDevice, vk::DisplayKHR), crate::InstanceError> {
    let mut failure = None;
    for device in devices {
        match lookup(device) {
            Ok(display) if display != vk::DisplayKHR::null() => return Ok((device, display)),
            // VK_EXT_acquire_drm_display specifies NULL for an unknown connector
            // and ERROR_UNKNOWN for another device's fd. The Vulkan loader can
            // also enumerate an ICD without the display extension (e.g. llvmpipe).
            Ok(_) | Err(vk::Result::ERROR_UNKNOWN | vk::Result::ERROR_EXTENSION_NOT_PRESENT) => {}
            Err(error @ vk::Result::ERROR_INITIALIZATION_FAILED) => failure = Some(error),
            Err(error) => {
                return Err(crate::InstanceError::with_source(
                    "Query DRM display".into(),
                    error,
                ))
            }
        }
    }
    let message = format!("No Vulkan display for DRM connector {connector}");
    Err(match failure {
        Some(error) => crate::InstanceError::with_source(message, error),
        None => crate::InstanceError::new(message),
    })
}

pub(super) struct AcquiredDisplay {
    instance: Arc<super::InstanceShared>,
    device: vk::PhysicalDevice,
    display: vk::DisplayKHR,
    // vkAcquireDrmDisplayEXT requires this descriptor to stay open until release.
    _fd: OwnedFd,
}

impl Drop for AcquiredDisplay {
    fn drop(&mut self) {
        let direct =
            ext::direct_mode_display::Instance::new(&self.instance.entry, &self.instance.raw);
        let result = unsafe { (direct.fp().release_display_ext)(self.device, self.display) };
        if result != vk::Result::SUCCESS {
            log::warn!("vkReleaseDisplayEXT: {result}");
        }
    }
}

impl super::Instance {
    /// Creates a new surface from the given drm fd and plane, deriving the connector and mode.
    ///
    /// # Safety
    ///
    /// - All parameters must point to valid DRM values.
    pub fn create_surface_from_drm_plane(
        &self,
        fd: i32,
        plane: u32,
    ) -> Result<super::Surface, crate::InstanceError> {
        let card = Card(fd);
        let plane_info = card
            .get_plane(
                NonZeroU32::new(plane)
                    .ok_or(crate::InstanceError::new("Invalid drm plane".to_string()))?
                    .into(),
            )
            .map_err(|e| crate::InstanceError::with_source("drm plane not found".to_string(), e))?;
        let crtc_handle = plane_info.crtc().ok_or(crate::InstanceError::new(
            "No CRTC for drm plane".to_string(),
        ))?;
        let crtc_info = card
            .get_crtc(crtc_handle)
            .map_err(|e| crate::InstanceError::with_source("drm CRTC not found".to_string(), e))?;
        let mode = crtc_info.mode().ok_or(crate::InstanceError::new(
            "No mode for drm CRTC".to_string(),
        ))?;

        let mut connector_and_mode = None;
        let resources = card.resource_handles().map_err(|e| {
            crate::InstanceError::with_source("No drm resource handles found".to_string(), e)
        })?;
        for connector_handle in resources.connectors() {
            let Ok(connector_info) = card.get_connector(*connector_handle, false) else {
                continue;
            };
            if connector_info.state() != connector::State::Connected {
                continue;
            }

            if let Some(encoder_handle) = connector_info.current_encoder() {
                if let Ok(encoder_info) = card.get_encoder(encoder_handle) {
                    if encoder_info.crtc() == Some(crtc_handle) {
                        connector_and_mode = Some((*connector_handle, mode));
                        break;
                    }
                }
            }
        }

        let (connector_handle, mode) = connector_and_mode.ok_or(crate::InstanceError::new(
            "Failed to derive drm connector and mode for plane".to_string(),
        ))?;
        let (width, height) = mode.size();
        // Rate in millihertz
        let refresh_rate = (((mode.clock() as f64 * 1000.0)
            / (mode.hsync().2 as f64 * mode.vsync().2 as f64))
            * 1000.0)
            .round() as u32;
        unsafe {
            self.create_surface_from_drm(
                fd,
                plane,
                connector_handle.into(),
                width as u32,
                height as u32,
                refresh_rate,
            )
        }
    }

    /// Creates a new surface from the given drm configuration.
    ///
    /// # Safety
    ///
    /// - All parameters must point to valid DRM values.
    pub unsafe fn create_surface_from_drm(
        &self,
        fd: i32,
        plane: u32,
        connector_id: u32,
        width: u32,
        height: u32,
        refresh_rate: u32,
    ) -> Result<super::Surface, crate::InstanceError> {
        if !self
            .shared
            .extensions
            .contains(&ext::acquire_drm_display::NAME)
        {
            return Err(crate::InstanceError::new(
                "Vulkan driver does not support VK_EXT_acquire_drm_display".to_string(),
            ));
        }

        let fd = Card(fd).as_fd().try_clone_to_owned().map_err(|error| {
            crate::InstanceError::with_source("Duplicate DRM display descriptor".into(), error)
        })?;
        let drm = ext::acquire_drm_display::Instance::new(&self.shared.entry, &self.shared.raw);
        let devices = unsafe { self.shared.raw.enumerate_physical_devices() }.map_err(|error| {
            crate::InstanceError::with_source("Enumerate Vulkan physical devices".into(), error)
        })?;
        let (physical_device, display) =
            connector_display(devices, connector_id, |device| unsafe {
                drm.get_drm_display(device, fd.as_raw_fd(), connector_id)
            })?;
        unsafe { drm.acquire_drm_display(physical_device, fd.as_raw_fd(), display) }.map_err(
            |error| crate::InstanceError::with_source("Acquire DRM display".into(), error),
        )?;
        let acquired = AcquiredDisplay {
            instance: Arc::clone(&self.shared),
            device: physical_device,
            display,
            _fd: fd,
        };

        let display_instance = khr::display::Instance::new(&self.shared.entry, &self.shared.raw);

        let modes =
            unsafe { display_instance.get_display_mode_properties(physical_device, display) }
                .map_err(|error| {
                    crate::InstanceError::with_source("Get DRM display modes".into(), error)
                })?;

        let mut mode = None;

        for current_mode in modes {
            log::trace!(
                "Comparing mode {}x{}@{} with {width}x{height}@{refresh_rate}",
                current_mode.parameters.visible_region.width,
                current_mode.parameters.visible_region.height,
                current_mode.parameters.refresh_rate
            );
            if current_mode.parameters.refresh_rate == refresh_rate
                && current_mode.parameters.visible_region.width == width
                && current_mode.parameters.visible_region.height == height
            {
                mode = Some(current_mode)
            }
        }

        let mode = mode.ok_or(crate::InstanceError::new(
            "Failed to find suitable display mode".to_string(),
        ))?;

        let planes = unsafe {
            display_instance.get_physical_device_display_plane_properties(physical_device)
        }
        .map_err(|e| crate::InstanceError::with_source("Get Vulkan display planes".into(), e))?;
        let selected = planes.get(plane as usize).ok_or_else(|| {
            crate::InstanceError::new(format!("Unknown Vulkan display plane {plane}"))
        })?;
        let caps = unsafe {
            display_instance.get_display_plane_capabilities(
                physical_device,
                mode.display_mode,
                plane,
            )
        }
        .map_err(|e| {
            crate::InstanceError::with_source("Get Vulkan display plane capabilities".into(), e)
        })?;
        if !caps
            .supported_alpha
            .contains(vk::DisplayPlaneAlphaFlagsKHR::OPAQUE)
        {
            return Err(crate::InstanceError::new(
                "Display plane lacks opaque scanout".into(),
            ));
        }
        let create_info = vk::DisplaySurfaceCreateInfoKHR::default()
            .display_mode(mode.display_mode)
            .image_extent(mode.parameters.visible_region)
            .transform(vk::SurfaceTransformFlagsKHR::IDENTITY)
            .alpha_mode(vk::DisplayPlaneAlphaFlagsKHR::OPAQUE)
            .plane_index(plane)
            .plane_stack_index(selected.current_stack_index);

        let raw = unsafe { display_instance.create_display_plane_surface(&create_info, None) }
            .map_err(|error| {
                crate::InstanceError::with_source("Create DRM plane surface".into(), error)
            })?;
        let mut surface = self.create_surface_from_vk_surface_khr(raw, None);
        surface.drm_display = Some(acquired);
        Ok(surface)
    }
}

impl super::Surface {
    /// Label the next queue presentation on this swapchain for `vkWaitForPresentKHR`.
    /// Returns false if the swapchain is unconfigured, lacks the extension, or
    /// already has a label waiting for a presentation.
    ///
    /// # Safety
    ///
    /// The device must have the `presentId` feature enabled. Each ID must be
    /// greater than every ID previously presented on this swapchain.
    pub unsafe fn set_next_present_id(&self, id: core::num::NonZeroU64) -> bool {
        self.swapchain.write().as_mut().is_some_and(|chain| {
            chain
                .as_any_mut()
                .downcast_mut::<super::swapchain::NativeSwapchain>()
                .is_some_and(|chain| chain.set_next_present_id(id))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ash::vk::Handle;
    use core::error::Error as _;

    #[test]
    fn query_each_device_for_the_connector() {
        let devices = [1, 2, 3, 4, 5].map(vk::PhysicalDevice::from_raw);
        let display = vk::DisplayKHR::from_raw(9);
        let answer = connector_display(devices, 38, |device| match device.as_raw() {
            1 => Err(vk::Result::ERROR_UNKNOWN),
            2 => Ok(vk::DisplayKHR::null()),
            3 => Err(vk::Result::ERROR_INITIALIZATION_FAILED),
            4 => Err(vk::Result::ERROR_EXTENSION_NOT_PRESENT),
            5 => Ok(display),
            _ => unreachable!(),
        });
        assert_eq!(answer.unwrap(), (devices[4], display));

        let absent = connector_display(devices, 38, |_| Ok(vk::DisplayKHR::null())).unwrap_err();
        assert!(absent.to_string().contains("connector 38"));
        assert!(absent.source().is_none());
        let fatal = connector_display(devices, 38, |device| {
            assert_eq!(device, devices[0]);
            Err(vk::Result::ERROR_OUT_OF_HOST_MEMORY)
        })
        .unwrap_err();
        assert_eq!(
            fatal.source().unwrap().to_string(),
            vk::Result::ERROR_OUT_OF_HOST_MEMORY.to_string()
        );
    }
}
