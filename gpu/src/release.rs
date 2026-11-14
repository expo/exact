//! Letting go of a canvas's presentation target, on every path that does:
//! a window going, buffers replacing it, the canvas destroyed, the device
//! lost, the module dropped.
//!
//! A drawable goes before its surface. wgpu discards an unpresented drawable
//! through the surface it came from and panics when that surface is gone
//! (wgpu-core's `surface_texture_discard`: "Cannot get non-existent
//! resource"), and wgpu-hal's Vulkan swapchain refuses to be destroyed while
//! a drawable still holds its semaphores.
//!
//! Releasing a canvas never waits for the compositor. Its acquisition thread may
//! be inside `vkAcquireNextImageKHR` for an image Android's compositor
//! releases only after the window's `surfaceDestroyed` returns, and that
//! callback waits for this ([`Module::detach`]). The thread is hung up on
//! ([`crate::acquire::Acquire::retire`]); it ends when its acquisition does,
//! and the module drops its surface here once it has ([`Module::reap`]).
//! Final module shutdown joins every thread before the host can unload its code.

#[cfg(not(target_arch = "wasm32"))]
pub(crate) use crate::acquire::Retired;
use crate::{Instance, Module};

/// The web acquires on the browser's thread: nothing is ever retired.
#[cfg(target_arch = "wasm32")]
pub(crate) struct Retired;

#[cfg(target_arch = "wasm32")]
impl Retired {
    fn finished(&self) -> bool {
        true
    }
    fn join(self) {}
}

impl Instance {
    /// Drop the canvas's presentation target, drawables first; an
    /// acquisition thread still running is returned for [`Module::reap`].
    /// Its configuration stays, for a device found again (`Module::set_gpu`).
    /// What the compositor was shown goes with the target: `presented` is
    /// false until a frame on the next one is.
    pub(crate) fn release(&mut self) -> Option<Retired> {
        self.held = None;
        self.presented.reset();
        #[cfg(not(target_arch = "wasm32"))]
        let retired = {
            self.scratch = None;
            self.readback = None;
            self.acquire.retire()
        };
        #[cfg(target_arch = "wasm32")]
        let retired = None;
        let presentation = self.presentation.take();
        crate::swapchains(|| drop(presentation));
        #[cfg(target_os = "android")]
        {
            self.ring = None;
        }
        retired
    }
}

impl Module {
    /// Release canvas `id`'s presentation target ([`Instance::release`])
    /// and its configuration.
    pub(crate) fn release(&mut self, id: u32) {
        if let Some(inst) = self.instances.get_mut(&id) {
            self.retired.extend(inst.release());
            inst.config = None;
        }
        self.reap();
    }

    /// Drop the surfaces whose acquisition threads have ended.
    pub(crate) fn reap(&mut self) {
        let mut at = 0;
        while at < self.retired.len() {
            if self.retired[at].finished() {
                self.retired.swap_remove(at).join();
            } else {
                at += 1;
            }
        }
    }

    /// Wait for every retired acquisition thread before unloading its code.
    /// Android also joins before attaching a window again: a retired swapchain
    /// may still own it (`VK_ERROR_NATIVE_WINDOW_IN_USE_KHR`). At attach the
    /// window is not going, so its compositor releases what the thread waits for.
    pub(crate) fn join_retired(&mut self) {
        for retired in self.retired.drain(..) {
            retired.join();
        }
    }

    /// The web's unload: the module dropped as native unload drops it, the
    /// open frame's drawables before their surfaces ([`Drop`]), then its
    /// device destroyed, which frees the browser's GPU memory at once
    /// rather than when the device is collected.
    #[cfg(any(target_arch = "wasm32", test))]
    pub(crate) fn unload(mut self) {
        let gpu = self.gpu.take();
        drop(self);
        if let Some(gpu) = gpu {
            gpu.device.destroy();
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        // The open frame's drawables, then each canvas's own, before their
        // surfaces.
        self.open = None;
        for inst in self.instances.values_mut() {
            self.retired.extend(inst.release());
        }
        self.join_retired();
    }
}

#[cfg(all(test, target_os = "macos"))]
#[path = "release_tests.rs"]
mod tests;
