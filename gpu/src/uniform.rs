//! A small uniform a surface writes every frame, without a staging copy.
//!
//! `queue.write_buffer` stages each write in a buffer of its own (on Metal a
//! new `MTLBuffer` per call) and makes the tick commit wgpu's pending-writes
//! command buffer: about 57 µs of the iPhone's main thread per frame for an
//! animated shader row's 80 bytes (LLP 1009 §3). On Apple, where the GPU
//! reads the CPU's memory, the uniform is instead a ring of shared-storage
//! buffers written in place: the frame writes the slot after the last one,
//! and binds that slot's bind group. On Vulkan the ring is host-visible,
//! coherent memory, mapped once: there `write_buffer` is a staging copy the
//! GPU's transfer engine runs as a job of its own each frame (PowerVR's TDM,
//! with its fences), about a tenth of the thread that flushes an animated
//! canvas.
//!
//! The ring has `FRAME_LATENCY + 2` slots (four on Apple and desktop, six on
//! Android, exceeding its five-image swapchain), and tracks GPU completion per
//! slot with `on_submitted_work_done`. Steady-state frames write in place; if
//! a slot is still in flight on the GPU (extra swapchain images, `Scratch`
//! retries, or back-to-back submits), the write falls back to
//! `queue.write_buffer` without blocking. Elsewhere — the browser — it is one
//! buffer written through the queue.

#[cfg(not(target_arch = "wasm32"))]
use std::sync::{
    atomic::{AtomicU64, Ordering},
    Arc,
};

/// A per-frame uniform: `slots()` buffers, one bound each frame.
pub struct FrameUniform {
    #[cfg(not(target_arch = "wasm32"))]
    device: Option<wgpu::Device>,
    slots: Vec<Slot>,
    next: usize,
    /// Highest submission serial whose GPU work has completed.
    #[cfg(not(target_arch = "wasm32"))]
    completed: Arc<AtomicU64>,
    /// Next submission serial assigned when fencing the prior frame's slot.
    #[cfg(not(target_arch = "wasm32"))]
    serial: u64,
    /// The slot written by the previous `write`, fenced on the next call once
    /// its frame's commands have been submitted.
    #[cfg(not(target_arch = "wasm32"))]
    last: Option<usize>,
}

struct Slot {
    buffer: wgpu::Buffer,
    /// The slot's memory, written in place (Apple's shared storage or Vulkan's
    /// mapped coherent memory); `None` writes through the queue.
    contents: Option<std::ptr::NonNull<u8>>,
    size: usize,
    /// Submission serial that must complete before writing this slot in place.
    #[cfg(not(target_arch = "wasm32"))]
    busy_until: u64,
    /// Keeps the Metal buffer, and so `contents`, alive with the slot.
    #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
    _raw: Option<objc2::rc::Retained<objc2::runtime::ProtocolObject<dyn objc2_metal::MTLBuffer>>>,
}

impl FrameUniform {
    /// Slots written in place (Apple's shared storage, Vulkan's mapped memory):
    /// strictly above the swapchain's `FRAME_LATENCY + 1` images.
    #[cfg_attr(target_arch = "wasm32", allow(dead_code))]
    pub(crate) const RING: usize = crate::FRAME_LATENCY as usize + 2;

    /// A uniform of `size` bytes, labelled `label`.
    pub fn new(device: &wgpu::Device, size: usize, label: &str) -> FrameUniform {
        #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
        if let Some(slots) = shared(device, size, label) {
            return FrameUniform {
                device: Some(device.clone()),
                slots,
                next: 0,
                completed: Arc::new(AtomicU64::new(0)),
                serial: 0,
                last: None,
            };
        }
        #[cfg(not(any(
            target_os = "macos",
            target_os = "ios",
            target_os = "tvos",
            target_arch = "wasm32"
        )))]
        if let Some(slots) = mapped(device, size, label) {
            return FrameUniform {
                device: Some(device.clone()),
                slots,
                next: 0,
                completed: Arc::new(AtomicU64::new(0)),
                serial: 0,
                last: None,
            };
        }
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: size as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        FrameUniform {
            #[cfg(not(target_arch = "wasm32"))]
            device: None,
            slots: vec![Slot {
                buffer,
                contents: None,
                size,
                #[cfg(not(target_arch = "wasm32"))]
                busy_until: 0,
                #[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
                _raw: None,
            }],
            next: 0,
            #[cfg(not(target_arch = "wasm32"))]
            completed: Arc::new(AtomicU64::new(0)),
            #[cfg(not(target_arch = "wasm32"))]
            serial: 0,
            #[cfg(not(target_arch = "wasm32"))]
            last: None,
        }
    }

    /// How many buffers there are: a surface makes a bind group for each.
    pub fn slots(&self) -> usize {
        self.slots.len()
    }

    /// The `slot`th buffer.
    pub fn buffer(&self, slot: usize) -> &wgpu::Buffer {
        &self.slots[slot].buffer
    }

    /// Write this frame's bytes; the slot to bind for it.
    pub fn write(&mut self, queue: &wgpu::Queue, bytes: &[u8]) -> usize {
        #[cfg(not(target_arch = "wasm32"))]
        if let Some(prev) = self.last.take() {
            self.serial += 1;
            let target = self.serial;
            self.slots[prev].busy_until = target;
            let completed = Arc::clone(&self.completed);
            queue.on_submitted_work_done(move || {
                completed.fetch_max(target, Ordering::Release);
            });
        }
        let slot = self.next;
        self.next = (self.next + 1) % self.slots.len();
        let s = &self.slots[slot];
        assert!(
            bytes.len() <= s.size,
            "{} bytes into a {}-byte uniform",
            bytes.len(),
            s.size
        );
        #[cfg(not(target_arch = "wasm32"))]
        let ready = if s.contents.is_some() {
            let mut done = s.busy_until <= self.completed.load(Ordering::Acquire);
            if !done {
                if let Some(device) = &self.device {
                    let _ = device.poll(wgpu::PollType::Poll);
                }
                done = s.busy_until <= self.completed.load(Ordering::Acquire);
            }
            done
        } else {
            false
        };
        #[cfg(target_arch = "wasm32")]
        let ready = false;
        match (s.contents, ready) {
            // SAFETY: `contents` is the slot's own shared (Metal) or mapped
            // coherent (Vulkan) allocation of `size` bytes, alive as long as
            // the slot; `ready` confirms the GPU submission that last read
            // this slot has completed.
            (Some(p), true) => unsafe {
                std::ptr::copy_nonoverlapping(bytes.as_ptr(), p.as_ptr(), bytes.len())
            },
            _ => queue.write_buffer(&s.buffer, 0, bytes),
        }
        #[cfg(not(target_arch = "wasm32"))]
        if s.contents.is_some() {
            self.last = Some(slot);
        }
        slot
    }
}

/// The ring on Metal: shared-storage buffers wgpu binds as uniforms.
#[cfg(any(target_os = "macos", target_os = "ios", target_os = "tvos"))]
fn shared(device: &wgpu::Device, size: usize, label: &str) -> Option<Vec<Slot>> {
    use objc2_metal::{MTLBuffer, MTLDevice, MTLResourceOptions};
    // SAFETY: only reads the raw device to allocate from it.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Metal>() }?;
    let mut slots = Vec::with_capacity(FrameUniform::RING);
    for _ in 0..FrameUniform::RING {
        let raw = hal.raw_device().newBufferWithLength_options(
            size.max(16),
            MTLResourceOptions::StorageModeShared | MTLResourceOptions::CPUCacheModeWriteCombined,
        )?;
        let contents = std::ptr::NonNull::new(raw.contents().as_ptr() as *mut u8)?;
        // SAFETY: a buffer of this device, of this size, handed to wgpu as
        // a uniform it never maps; wgpu treats it as initialized.
        let buffer = unsafe {
            let hal_buffer = wgpu::hal::metal::Device::buffer_from_raw(raw.clone(), size as u64);
            device.create_buffer_from_hal::<wgpu::hal::api::Metal>(
                hal_buffer,
                &wgpu::BufferDescriptor {
                    label: Some(label),
                    size: size as u64,
                    usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                },
            )
        };
        slots.push(Slot {
            buffer,
            contents: Some(contents),
            size,
            busy_until: 0,
            _raw: Some(raw),
        });
    }
    Some(slots)
}

/// The ring on Vulkan: host-visible, coherent buffers, mapped once, that
/// wgpu binds as uniforms. `None` (the queue's writes) on another backend or
/// where the memory would need flushing.
#[cfg(not(any(
    target_os = "macos",
    target_os = "ios",
    target_os = "tvos",
    target_arch = "wasm32"
)))]
fn mapped(device: &wgpu::Device, size: usize, label: &str) -> Option<Vec<Slot>> {
    use wgpu::hal::Device as _;
    // SAFETY: only allocates from the raw device and maps what it allocated.
    let hal = unsafe { device.as_hal::<wgpu::hal::api::Vulkan>() }?;
    let desc = wgpu::hal::BufferDescriptor {
        label: Some(label),
        size: size.max(16) as u64,
        usage: wgpu::BufferUses::MAP_WRITE | wgpu::BufferUses::UNIFORM | wgpu::BufferUses::COPY_DST,
        memory_flags: wgpu::hal::MemoryFlags::PREFER_COHERENT,
    };
    let mut raws = Vec::with_capacity(FrameUniform::RING);
    for _ in 0..FrameUniform::RING {
        // SAFETY: a valid descriptor on this device.
        let Ok(raw) = (unsafe { hal.create_buffer(&desc) }) else {
            break;
        };
        // SAFETY: the range is the buffer's own; it stays mapped for its life.
        match unsafe { hal.map_buffer(&raw, 0..desc.size) } {
            Ok(m) if m.is_coherent => raws.push((raw, m.ptr)),
            _ => {
                // SAFETY: made just above and never handed to wgpu.
                unsafe { hal.destroy_buffer(raw) };
                break;
            }
        }
    }
    if raws.len() < FrameUniform::RING {
        for (raw, _) in raws {
            // SAFETY: as above.
            unsafe { hal.destroy_buffer(raw) };
        }
        return None;
    }
    drop(hal);
    Some(
        raws.into_iter()
            .map(|(raw, contents)| {
                // SAFETY: a buffer of this device, of this size, handed to
                // wgpu as a uniform it never maps; wgpu treats it as
                // initialized.
                let buffer = unsafe {
                    device.create_buffer_from_hal::<wgpu::hal::api::Vulkan>(
                        raw,
                        &wgpu::BufferDescriptor {
                            label: Some(label),
                            size: size as u64,
                            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                            mapped_at_creation: false,
                        },
                    )
                };
                Slot {
                    buffer,
                    contents: Some(contents),
                    size,
                    busy_until: 0,
                }
            })
            .collect(),
    )
}

#[cfg(test)]
mod tests {
    use super::FrameUniform;

    #[test]
    fn ring_exceeds_swapchain_image_count() {
        assert!(FrameUniform::RING > crate::FRAME_LATENCY as usize + 1);
    }
}
