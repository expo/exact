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
}
