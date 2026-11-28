//! The instance's own bookkeeping: its offscreen format, its ring, and what a render left to drain.

use super::*;

impl Instance {
    pub(crate) fn offscreen_format(&self) -> wgpu::TextureFormat {
        if self.surface.srgb_target() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        }
    }

    pub(crate) fn has_ring(&self) -> bool {
        #[cfg(target_os = "android")]
        return self.ring.is_some();
        #[cfg(not(target_os = "android"))]
        false
    }

    pub(crate) fn drain(&mut self) {
        self.messages.extend(self.surface.messages());
        if let Some(record) = self.surface.published() {
            self.published = Some(record);
        }
        self.presented.generation(self.surface.generation());
        if self.surface.is_empty() {
            #[cfg(not(target_arch = "wasm32"))]
            {
                self.readback = None;
                self.sampled = None;
            }
            self.seen.store(false, Ordering::Release);
            #[cfg(target_os = "android")]
            if let Some(ring) = self.ring.as_mut() {
                ring.clear();
            }
        }
    }

    /// Drop an ordinary surface that did not opt into in-place device recovery,
    /// replace it from its registered factory, and replay retained host state.
    pub(crate) fn reconstruct(&mut self, seekable: bool, reduced_motion: bool) {
        self.wake.kill();
        let (wake, waker) = wake::WakeState::new();
        self.wake = wake;
        self.surface = (self.factory)();
        self.messages.clear();
        self.published = None;
        self.presented.forget_record();
        self.presented.reset();
        self.input_dirty = false;
        self.surface.set_waker(waker);
        #[cfg(not(target_arch = "wasm32"))]
        if let Some([data, cache, temporary]) = &self.storage_dirs {
            self.surface.storage(data, cache, temporary);
        }
        self.surface.clock(seekable);
        self.surface.reduced_motion(reduced_motion);
        #[cfg(not(target_arch = "wasm32"))]
        if self.poll_input {
            self.surface.poll_input_before_draw();
        }
        if let Some(visibility) = self.visibility {
            self.surface.lifecycle(visibility);
        }
        if let Some(interrupted) = self.interrupted {
            self.surface.lifecycle(interrupted);
        }
        if let Some((width, height)) = self.last_size {
            let _ = self.surface.size(width, height);
        }
        self.surface.color_target(self.destination);
        if self.performance.enabled() {
            self.surface.set_performance_reporting(true);
        }
        // Defer replaying the last accepted bind until `finish_attach` announces
        // `device_ready` and `color_space_granted` on the replacement device.
        self.needs_rebind |= self.bound;
        self.bound = false;
        self.drain();
    }
}
