use exact_game_render::HookGpu;
use exact_gpu::wgpu::{self, util::DrawIndexedIndirectArgs};
use std::ops::Range;

// Batches hundreds of packed sections while limiting the cost of an empty arena.
pub(super) const ARENA_BYTES: u64 = 4 * 1024 * 1024;

struct FreeRanges {
    ranges: Vec<Range<u64>>,
}

impl FreeRanges {
    fn new(size: u64) -> Self {
        Self {
            ranges: std::iter::once(0..size).collect(),
        }
    }

    fn allocate(&mut self, size: u64, alignment: u64) -> Option<Range<u64>> {
        for (index, free) in self.ranges.iter().enumerate() {
            let start = free.start.div_ceil(alignment) * alignment;
            let end = start + size;
            if end > free.end {
                continue;
            }
            let free = free.clone();
            match (free.start < start, end < free.end) {
                (true, true) => {
                    self.ranges[index].end = start;
                    self.ranges.insert(index + 1, end..free.end);
                }
                (true, false) => self.ranges[index].end = start,
                (false, true) => self.ranges[index].start = end,
                (false, false) => {
                    self.ranges.remove(index);
                }
            }
            return Some(start..end);
        }
        None
    }

    fn free(&mut self, mut range: Range<u64>) {
        let mut index = self.ranges.partition_point(|free| free.start < range.start);
        if index > 0 && self.ranges[index - 1].end == range.start {
            index -= 1;
            range.start = self.ranges.remove(index).start;
        }
        if index < self.ranges.len() && self.ranges[index].start == range.end {
            range.end = self.ranges.remove(index).end;
        }
        self.ranges.insert(index, range);
    }

    fn bytes(&self) -> u64 {
        self.ranges
            .iter()
            .map(|range| range.end - range.start)
            .sum()
    }
}

pub(super) struct Allocation {
    pub(super) arena: usize,
    range: Range<u64>,
    pub(super) draw: Indexed,
}

#[derive(Clone, Copy)]
pub(super) struct Indexed {
    pub(super) count: u32,
    pub(super) first: u32,
    pub(super) base: i32,
}

impl Indexed {
    pub(super) fn issue(self, pass: &mut wgpu::RenderPass<'_>) {
        pass.draw_indexed(self.first..self.first + self.count, self.base, 0..1);
    }
}

pub(super) struct Arena<B> {
    pub(super) buffer: B,
    free: FreeRanges,
    pub(super) visible: Vec<(Indexed, Option<u32>)>,
}

pub(super) struct Arenas<B = wgpu::Buffer>(pub(super) Vec<Arena<B>>);

impl<B> Default for Arenas<B> {
    fn default() -> Self {
        Self(Vec::new())
    }
}

impl<B> Arenas<B> {
    fn allocate(
        &mut self,
        size: u64,
        stride: u64,
        index_offset: u64,
        count: u32,
        create: impl FnOnce() -> B,
    ) -> Option<(Allocation, u64)> {
        if size > ARENA_BYTES {
            return None;
        }
        let mut added = 0;
        let (arena, range) = self
            .0
            .iter_mut()
            .enumerate()
            .find_map(|(index, arena)| {
                arena
                    .free
                    .allocate(size, stride)
                    .map(|range| (index, range))
            })
            .unwrap_or_else(|| {
                let index = self.0.len();
                let mut free = FreeRanges::new(ARENA_BYTES);
                let range = free.allocate(size, stride).unwrap();
                self.0.push(Arena {
                    buffer: create(),
                    free,
                    visible: Vec::new(),
                });
                added = ARENA_BYTES;
                (index, range)
            });
        let draw = Indexed {
            first: ((range.start + index_offset) / 4) as u32,
            base: (range.start / stride) as i32,
            count,
        };
        Some((Allocation { arena, range, draw }, added))
    }

    pub(super) fn free(&mut self, allocation: Allocation) {
        self.0[allocation.arena].free.free(allocation.range);
    }

    pub(super) fn free_bytes(&self) -> u64 {
        self.0.iter().map(|arena| arena.free.bytes()).sum()
    }
}

impl Arenas {
    pub(super) fn upload(
        &mut self,
        gpu: &HookGpu<'_>,
        bytes: &[u8],
        stride: u64,
        index_offset: u64,
        count: u32,
    ) -> Option<(Allocation, u64)> {
        let (allocation, added) =
            self.allocate(bytes.len() as u64, stride, index_offset, count, || {
                gpu.device.create_buffer(&wgpu::BufferDescriptor {
                    label: Some("terrain arena"),
                    size: ARENA_BYTES,
                    usage: wgpu::BufferUsages::VERTEX
                        | wgpu::BufferUsages::INDEX
                        | wgpu::BufferUsages::COPY_DST,
                    mapped_at_creation: false,
                })
            })?;
        gpu.queue.write_buffer(
            &self.0[allocation.arena].buffer,
            allocation.range.start,
            bytes,
        );
        Some((allocation, added))
    }
}

const SLOTS_PER_PAGE: u32 = 1024;
pub(super) const INDIRECT_BYTES: u64 = size_of::<DrawIndexedIndirectArgs>() as u64;

pub(super) struct Indirect {
    pub(super) layout: wgpu::BindGroupLayout,
    stride: u32,
    pages: Vec<IndirectPage>,
    slots: IndirectSlots,
}

#[derive(Default)]
struct IndirectSlots {
    next_slot: u32,
    free: Vec<u32>,
}

impl IndirectSlots {
    fn install(&mut self, draw: Indexed, previous: Option<u32>) -> (u32, DrawIndexedIndirectArgs) {
        let slot = previous.or_else(|| self.free.pop()).unwrap_or_else(|| {
            let slot = self.next_slot;
            self.next_slot += 1;
            slot
        });
        let args = DrawIndexedIndirectArgs {
            index_count: draw.count,
            instance_count: 1,
            first_index: draw.first,
            base_vertex: draw.base,
            first_instance: 0,
        };
        (slot, args)
    }

    fn free(&mut self, slot: u32) {
        self.free.push(slot);
    }
}

struct IndirectPage {
    args: wgpu::Buffer,
    group: wgpu::BindGroup,
}

impl Indirect {
    pub(super) fn new(device: &wgpu::Device) -> Self {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("terrain draw slot"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: true,
                    min_binding_size: wgpu::BufferSize::new(4),
                },
                count: None,
            }],
        });
        Self {
            layout,
            stride: device.limits().min_uniform_buffer_offset_alignment,
            pages: Vec::new(),
            slots: IndirectSlots::default(),
        }
    }

    fn add_page(&mut self, device: &wgpu::Device) -> u64 {
        let slots = u64::from(SLOTS_PER_PAGE);
        let uniform_bytes = slots * u64::from(self.stride);
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain draw slots"),
            size: uniform_bytes,
            usage: wgpu::BufferUsages::UNIFORM,
            mapped_at_creation: false,
        });
        let group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("terrain draw slot"),
            layout: &self.layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                    buffer: &buffer,
                    offset: 0,
                    size: wgpu::BufferSize::new(4),
                }),
            }],
        });
        let args_bytes = slots * INDIRECT_BYTES;
        let args = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("terrain indirect arguments"),
            size: args_bytes,
            usage: wgpu::BufferUsages::INDIRECT | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        self.pages.push(IndirectPage { args, group });
        uniform_bytes + args_bytes
    }

    pub(super) fn upload(
        &mut self,
        gpu: &HookGpu<'_>,
        draw: Indexed,
        previous: Option<u32>,
    ) -> (u32, u64) {
        let (slot, args) = self.slots.install(draw, previous);
        let (page, entry) = Self::address(slot);
        let mut added = 0;
        if page == self.pages.len() {
            added = self.add_page(gpu.device);
        }
        gpu.queue.write_buffer(
            &self.pages[page].args,
            u64::from(entry) * INDIRECT_BYTES,
            args.as_bytes(),
        );
        (slot, added)
    }

    fn address(slot: u32) -> (usize, u32) {
        ((slot / SLOTS_PER_PAGE) as usize, slot % SLOTS_PER_PAGE)
    }

    pub(super) fn free(&mut self, slot: u32) {
        self.slots.free(slot);
    }

    pub(super) fn issue(&self, pass: &mut wgpu::RenderPass<'_>, slot: u32) {
        let (page, entry) = Self::address(slot);
        pass.set_bind_group(1, &self.pages[page].group, &[entry * self.stride]);
        pass.draw_indexed_indirect(&self.pages[page].args, u64::from(entry) * INDIRECT_BYTES);
    }
}

#[cfg(test)]
#[test]
fn reuse_a_freed_middle_range() {
    let mut arena = FreeRanges::new(128);
    assert_eq!(arena.allocate(24, 12), Some(0..24));
    let middle = arena.allocate(24, 12).unwrap();
    assert_eq!(arena.allocate(24, 12), Some(48..72));
    arena.free(middle.clone());
    assert_eq!(arena.allocate(24, 12), Some(middle));
}

#[cfg(test)]
#[test]
fn aligns_ranges_and_coalesces_all_free_bytes() {
    for stride in [12, 64] {
        let mut arena = FreeRanges::new(256);
        let first = arena.allocate(stride + 4, stride).unwrap();
        let second = arena.allocate(stride, stride).unwrap();
        assert_eq!(second.start, 2 * stride);
        assert_eq!(arena.bytes(), 256 - 2 * stride - 4);
        arena.free(second);
        arena.free(first);
        assert_eq!(arena.bytes(), 256);
        assert_eq!(arena.allocate(256, stride), Some(0..256));
        assert_eq!(arena.allocate(4, stride), None);
    }
    assert_eq!(FreeRanges::new(256).allocate(260, 12), None);
}

#[cfg(test)]
#[test]
fn replacement_and_retirement_keep_arena_capacity_and_return_free_bytes() {
    let mut arenas = Arenas::default();
    let (first, mut resident) = arenas.allocate(48, 12, 36, 3, || ()).unwrap();
    let (second, added) = arenas.allocate(48, 12, 36, 3, || ()).unwrap();
    resident += added;
    assert_eq!(resident, ARENA_BYTES);
    assert_eq!(arenas.free_bytes(), resident - 96);
    assert_eq!((second.draw.first, second.draw.base), (21, 4));

    let (replacement, added) = arenas.allocate(76, 12, 60, 4, || ()).unwrap();
    resident += added;
    assert_ne!(first.range, replacement.range);
    arenas.free(first);
    assert_eq!(arenas.free_bytes(), resident - 48 - 76);
    arenas.free(second);
    arenas.free(replacement);
    assert_eq!(arenas.free_bytes(), resident);
    assert_eq!(resident, ARENA_BYTES);

    assert!(arenas.allocate(ARENA_BYTES + 4, 12, 12, 3, || ()).is_none());
    let (full, added) = arenas.allocate(ARENA_BYTES, 12, 36, 3, || ()).unwrap();
    assert_eq!(added, 0);
    let (next, added) = arenas.allocate(48, 12, 36, 3, || ()).unwrap();
    assert_ne!(full.arena, next.arena);
    assert_eq!(added, ARENA_BYTES);
}

#[cfg(test)]
#[test]
fn indirect_slots_rewrite_arguments_on_replacement_and_reuse() {
    use std::collections::BTreeMap;

    let mut arenas = Arenas::default();
    let (original, _) = arenas.allocate(48, 12, 36, 3, || ()).unwrap();
    let (replacement, _) = arenas.allocate(72, 12, 48, 6, || ()).unwrap();
    let mut slots = IndirectSlots::default();
    let mut page = BTreeMap::new();
    let draw = |page: &BTreeMap<_, DrawIndexedIndirectArgs>, slot| {
        let args = &page[&Indirect::address(slot)];
        (
            args.index_count,
            args.instance_count,
            args.first_index,
            args.base_vertex,
            args.first_instance,
        )
    };

    let (slot, args) = slots.install(original.draw, None);
    page.insert(Indirect::address(slot), args);
    assert_eq!(draw(&page, slot), (3, 1, 9, 0, 0));

    let (replaced, args) = slots.install(replacement.draw, Some(slot));
    assert_eq!(replaced, slot);
    page.insert(Indirect::address(replaced), args);
    arenas.free(original);
    assert_eq!(draw(&page, slot), (6, 1, 24, 4, 0));

    arenas.free(replacement);
    slots.free(replaced);

    let (neighbour, _) = arenas.allocate(96, 12, 60, 9, || ()).unwrap();
    let (reused, args) = slots.install(neighbour.draw, None);
    assert_eq!(reused, slot);
    page.insert(Indirect::address(reused), args);
    assert_eq!(draw(&page, reused), (9, 1, 15, 0, 0));
}
