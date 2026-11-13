//! A cache for storing the results of layout computation

#![allow(clippy::unusual_byte_groupings)]

use crate::geometry::{Line, Size};
use crate::style::AvailableSpace;
use crate::tree::{CollapsibleMarginSet, LayoutInput, LayoutOutput, RunMode, SizingMode};
use crate::RequestedAxis;

/// Size cache policy, selectable for comparisons (LLP 1165 D4).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SizeCachePolicy {
    /// Keep every answer stored or used in the current pass.
    #[default]
    All,
    /// Nine entries, replaced by the clock.
    Nine,
    /// Compute every size; the final-layout cache is unchanged.
    Off,
}

/// The number of cache entries under the old policy
const CACHE_SIZE: usize = 9;

// Manually written-out results of float to u32 bit casts because
// `f32::to_bits` is not yet const at our MSRV.

/// `f32::INFINITY` as a u32
const INFINITY_BITS: u32 = 0b_0_11111111_00000000000000000000000_u32;
/// `f32::NEG_INFINITY` as a u32
const NEG_INFINITY_BITS: u32 = 0b_1_11111111_00000000000000000000000_u32;

// The `CacheKey` encodes two f32s as a u64. We know that the f32s will always be
// non-negative, so we pack two extra bits encoding the `RequestedAxis` into the
// sign bits of the f32s. These constants help to encode and decode those bits.

/// The sign bit of the first f32
const SIGN_BIT_1: u64 = 1u64 << 63;
/// The sign bit of the second f32
const SIGN_BIT_2: u64 = 1u64 << 31;
/// Mask of both sign bits (used to compute NON_SIGN_BITS_MASK)
const BOTH_SIGN_BITS_MASK: u64 = SIGN_BIT_1 | SIGN_BIT_2;
/// Mask of excluding the sign bits (used when setting/getting the size excluding the packed bits)
const NON_SIGN_BITS_MASK: u64 = !BOTH_SIGN_BITS_MASK;

/// Pack `Option<f32>` into `u32`
#[inline(always)]
fn option_cache_key(input: Option<f32>) -> u32 {
    match input {
        Some(value) => value.to_bits(),
        None => INFINITY_BITS,
    }
}

/// Pack `Size<Option<f32>>` into `u64`
#[inline(always)]
fn size_option_cache_key(input: Size<Option<f32>>) -> u64 {
    (option_cache_key(input.width) as u64) << 32 | option_cache_key(input.height) as u64
}

/// Pack `AvailableSpace` into `u32`
#[inline(always)]
fn available_space_cache_key(input: AvailableSpace) -> u32 {
    match input {
        AvailableSpace::Definite(value) => (-value).to_bits(),
        AvailableSpace::MinContent => NEG_INFINITY_BITS,
        AvailableSpace::MaxContent => INFINITY_BITS,
    }
}

/// Pack `Size<AvailableSpace>` into `u64`
#[inline(always)]
#[allow(dead_code)]
fn size_available_space_cache_key(input: Size<AvailableSpace>) -> u64 {
    (available_space_cache_key(input.width) as u64) << 32 | available_space_cache_key(input.height) as u64
}

/// Encodes combination of a `known_dimension` (Option<f32>) and `AvailableSpace` in
/// a single dimension into a cache key in a single dimension.
#[inline(always)]
fn mixed_cache_key(kd: Option<f32>, avs: AvailableSpace) -> u32 {
    kd.map(|kd| kd.to_bits()).unwrap_or_else(|| available_space_cache_key(avs))
}

/// Encodes combination of a `known_dimension` (Option<f32>) and `AvailableSpace` in
/// two dimensions into a cache key in a single dimension.
#[inline(always)]
fn size_mixed_cache_key(kd: Size<Option<f32>>, avs: Size<AvailableSpace>) -> u64 {
    (mixed_cache_key(kd.width, avs.width) as u64) << 32 | mixed_cache_key(kd.height, avs.height) as u64
}

/// Space-optimised cache key that packs bits into as small a size as possible
#[derive(Debug, Clone, Copy)]
#[cfg_attr(feature = "serde", derive(Serialize))]
struct CacheKey {
    /// The initial cached size of the node itself
    kd_available_space: u64,
    /// The initial cached size of the parent's node
    parent_size: u64,
    /// Whether each known dimension is definite. Normalized such that an axis
    /// without a known dimension is always `true`.
    known_dimensions_are_definite: Size<bool>,
    /// EXACT PATCH 11: whether the node's vertical margins may collapse with its parent's
    /// (whether it is laid out inside its parent's block formatting context). It decides
    /// whether a first/last child's margin collapses through the node, and so the node's size.
    vertical_margins_are_collapsible: Line<bool>,
    /// EXACT PATCH 11: whether the node's own size styles apply (`InherentSize`) or are ignored
    /// (`ContentSize`). The same known dimensions give different sizes in the two modes.
    sizing_mode: SizingMode,
    /// EXACT PATCH 72: fits in the key's padding. The tree resets stamps before reusing a pass.
    last_used_pass: u16,
}

impl PartialEq for CacheKey {
    fn eq(&self, other: &Self) -> bool {
        self.kd_available_space == other.kd_available_space
            && self.parent_size == other.parent_size
            && self.known_dimensions_are_definite == other.known_dimensions_are_definite
            && self.vertical_margins_are_collapsible == other.vertical_margins_are_collapsible
            && self.sizing_mode == other.sizing_mode
    }
}

impl Eq for CacheKey {}

impl CacheKey {
    fn has_indefinite_size(&self) -> bool {
        !self.known_dimensions_are_definite.width || !self.known_dimensions_are_definite.height
    }

    #[inline(always)]
    /// Return the parent size with the extra bits that encode the requested axis masked out
    fn parent_size(&self) -> u64 {
        self.parent_size & NON_SIGN_BITS_MASK
    }

    /// Return the bits that encode the requested axis
    fn requested_axis_bits(&self) -> u64 {
        self.parent_size & BOTH_SIGN_BITS_MASK
    }

    /// Whether a cached entry with this key contains a valid size for the axis requested by `other`.
    /// Sizes computed for a single axis may contain garbage values in the other axis, so an entry
    /// is only usable if it was computed for the same axis (or for both axes).
    fn size_is_valid_for(&self, other: &CacheKey) -> bool {
        let entry_axis = self.requested_axis_bits();
        entry_axis == BOTH_SIGN_BITS_MASK || entry_axis == other.requested_axis_bits()
    }
}

impl From<&LayoutInput> for CacheKey {
    fn from(input: &LayoutInput) -> Self {
        // Pack axis enum into spare bits in the known_dimensions and available_space values
        let extra_bits = match input.axis {
            RequestedAxis::Horizontal => SIGN_BIT_1,
            RequestedAxis::Vertical => SIGN_BIT_2,
            RequestedAxis::Both => SIGN_BIT_1 | SIGN_BIT_2,
        };

        Self {
            kd_available_space: size_mixed_cache_key(input.known_dimensions, input.available_space),
            parent_size: (size_option_cache_key(input.parent_size) & NON_SIGN_BITS_MASK) | extra_bits,
            known_dimensions_are_definite: input
                .known_dimensions_are_definite
                .zip_map(input.known_dimensions, |is_definite, kd| is_definite || kd.is_none()),
            vertical_margins_are_collapsible: input.vertical_margins_are_collapsible,
            sizing_mode: input.sizing_mode,
            last_used_pass: 0,
        }
    }
}

// EXACT PATCH 74: keep the common key small. A known but indefinite size
// needs both its size and its offer; only those entries use the side storage.
const NO_OFFER: u64 = (INFINITY_BITS as u64) << 32 | INFINITY_BITS as u64;

fn indefinite_offer(input: &LayoutInput) -> u64 {
    size_available_space_cache_key(Size {
        width: if input.known_dimensions.width.is_some() && !input.known_dimensions_are_definite.width {
            input.available_space.width
        } else { AvailableSpace::MaxContent },
        height: if input.known_dimensions.height.is_some() && !input.known_dimensions_are_definite.height {
            input.available_space.height
        } else { AvailableSpace::MaxContent },
    })
}

fn offer_at(offers: Option<&Vec<u64>>, slot: usize) -> u64 {
    offers.and_then(|offers| offers.get(slot)).copied().unwrap_or(NO_OFFER)
}

/// Cached intermediate layout results
#[derive(Debug, Clone, Copy, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub(crate) struct CacheEntry<T> {
    /// The key for the cache entry
    key: CacheKey,
    /// The cached size and baselines of the item
    content: T,
}

/// A cache for caching the results of a sizing a Grid Item or Flexbox Item
#[derive(Debug, Clone, PartialEq)]
#[cfg_attr(feature = "serde", derive(Serialize))]
pub struct Cache {
    /// The cache entry for the node's final layout
    final_layout_entry: Option<CacheEntry<LayoutOutput>>,
    /// EXACT PATCH 72: slots grow when every held answer belongs to this pass.
    /// Patch 60's lazy allocation and retained capacity survive a clear.
    measure_entries: Vec<CacheEntry<Size<f32>>>,
    /// EXACT PATCH 74: offers for indefinite known sizes, allocated only when needed.
    /// Slot zero is the final layout; size slots start at one.
    indefinite_offers: Option<Box<Vec<u64>>>,
    /// Tracks which measure entries have been used since the eviction cursor last passed them
    recently_used_entries: u16,
    /// The next measure entry to consider replacing
    next_measure_entry: usize,
    /// Tracks if all cache entries are empty
    is_empty: bool,
}

impl Default for Cache {
    fn default() -> Self {
        Self::new()
    }
}

impl Cache {
    /// Create a new empty cache
    pub const fn new() -> Self {
        Self {
            final_layout_entry: None,
            measure_entries: Vec::new(),
            indefinite_offers: None,
            recently_used_entries: 0,
            next_measure_entry: 0,
            is_empty: true,
        }
    }

    /// Try to retrieve a cached result from the cache
    #[inline]
    pub fn get(&mut self, input: &LayoutInput, pass: u16, exact_axis: bool) -> Option<LayoutOutput> {
        let key = CacheKey::from(input);
        let offer = indefinite_offer(input);
        match input.run_mode {
            RunMode::PerformLayout => self.final_layout_entry.filter(|entry| entry.key == key && (!key.has_indefinite_size() || self.offer(0) == offer)).map(|e| e.content),
            RunMode::ComputeSize => {
                let offers = self.indefinite_offers.as_deref();
                for (index, entry) in self.measure_entries.iter_mut().enumerate() {
                    // EXACT PATCH 11: every input matches except the requested axis. Upstream
                    // compared only the parent's width, but a node's own percentage height
                    // resolves against the parent's height (CSS 2.1 §10.5).
                    if entry.key.kd_available_space == key.kd_available_space
                        && (!key.has_indefinite_size() || offer_at(offers, index + 1) == offer)
                        // EXACT PATCH 73: ratio width probes have their own sizing rules.
                        && (!exact_axis || entry.key.requested_axis_bits() == key.requested_axis_bits())
                        && entry.key.known_dimensions_are_definite == key.known_dimensions_are_definite
                        && entry.key.vertical_margins_are_collapsible == key.vertical_margins_are_collapsible
                        && entry.key.sizing_mode == key.sizing_mode
                        && entry.key.parent_size() == key.parent_size()
                        && entry.key.size_is_valid_for(&key)
                    {
                        entry.key.last_used_pass = pass;
                        if index < CACHE_SIZE { self.recently_used_entries |= 1 << index; }
                        return Some(LayoutOutput::from_outer_size(entry.content));
                    }
                }

                None
            }
            RunMode::PerformHiddenLayout => None,
        }
    }

    /// Store a computed size in the cache
    pub fn store(&mut self, input: &LayoutInput, layout_output: LayoutOutput, pass: u16, policy: SizeCachePolicy) {
        let key = CacheKey::from(input);
        match input.run_mode {
            RunMode::PerformLayout => {
                self.is_empty = false;
                self.set_offer(0, indefinite_offer(input));
                self.final_layout_entry = Some(CacheEntry { key, content: layout_output })
            }
            RunMode::ComputeSize => {
                if policy == SizeCachePolicy::Off { return; }
                // Measure entries only store the size, and cache hits are reconstructed with
                // `LayoutOutput::from_outer_size`, which resets the margin-collapse metadata
                // (`top_margin`, `bottom_margin`, `margins_can_collapse_through`). Results that
                // carry such metadata cannot be reconstructed from their size, so don't cache them.
                if layout_output.margins_can_collapse_through
                    || layout_output.top_margin != CollapsibleMarginSet::ZERO
                    || layout_output.bottom_margin != CollapsibleMarginSet::ZERO
                {
                    return;
                }
                self.is_empty = false;
                let offer = indefinite_offer(input);
                if let Some(index) = self.measure_entries.iter().enumerate().position(|(index, entry)| entry.key == key && (!key.has_indefinite_size() || self.offer(index + 1) == offer)) {
                    self.measure_entries[index].content = layout_output.size;
                    self.measure_entries[index].key.last_used_pass = pass;
                    if index < CACHE_SIZE { self.recently_used_entries |= 1 << index; }
                    return;
                }
                let entry_index = match policy {
                    SizeCachePolicy::Nine => {
                        self.next_measure_entry %= CACHE_SIZE;
                        while self.recently_used_entries & (1 << self.next_measure_entry) != 0 {
                            self.recently_used_entries &= !(1 << self.next_measure_entry);
                            self.next_measure_entry = (self.next_measure_entry + 1) % CACHE_SIZE;
                        }
                        self.next_measure_entry
                    }
                    _ => {
                        let len = self.measure_entries.len();
                        (0..if len < CACHE_SIZE { 0 } else { len }).map(|offset| (self.next_measure_entry + offset) % len)
                            .find(|&index| self.measure_entries[index].key.last_used_pass != pass)
                            .unwrap_or(len)
                    }
                };
                let entry = CacheEntry { key: CacheKey { last_used_pass: pass, ..key }, content: layout_output.size };
                if entry_index < self.measure_entries.len() {
                    self.measure_entries[entry_index] = entry;
                } else {
                    // Three slots at a time, as patch 60 allocated the first nine.
                    if self.measure_entries.len() == self.measure_entries.capacity() {
                        self.measure_entries.reserve_exact(3);
                    }
                    self.measure_entries.push(entry);
                }
                self.set_offer(entry_index + 1, offer);
                if entry_index < CACHE_SIZE { self.recently_used_entries |= 1 << entry_index; }
                self.next_measure_entry = if entry_index + 1 == (if policy == SizeCachePolicy::Nine { CACHE_SIZE } else { self.measure_entries.len() }) { 0 } else { entry_index + 1 };
            }
            RunMode::PerformHiddenLayout => {}
        }
    }

    fn offer(&self, slot: usize) -> u64 {
        offer_at(self.indefinite_offers.as_deref(), slot)
    }

    fn set_offer(&mut self, slot: usize, offer: u64) {
        if offer == NO_OFFER && self.indefinite_offers.is_none() { return; }
        let offers = self.indefinite_offers.get_or_insert_with(|| Box::new(Vec::new()));
        if slot >= offers.len() {
            if offer == NO_OFFER { return; }
            offers.resize(slot + 1, NO_OFFER);
        }
        offers[slot] = offer;
    }

    /// Allocated bytes, excluding the allocator's own metadata.
    pub fn allocated_bytes(&self) -> usize {
        core::mem::size_of::<Self>()
            + self.measure_entries.capacity() * core::mem::size_of::<CacheEntry<Size<f32>>>()
            + self.indefinite_offers.as_ref().map_or(0, |offers| core::mem::size_of::<Vec<u64>>() + offers.capacity() * core::mem::size_of::<u64>())
    }

    /// EXACT PATCH 72: zero is reserved for answers from before a stamp wrap.
    fn reset_pass_stamps(&mut self) {
        for entry in &mut self.measure_entries { entry.key.last_used_pass = 0; }
    }

    /// EXACT PATCH 72: a size probe laid children out behind the final entry.
    pub fn invalidate_final_layout(&mut self) {
        self.final_layout_entry = None;
        // Keep the dirty-propagation mark: losing the last answer is not a
        // full clear, and a later edit must still invalidate ancestors.
    }

    /// Clear all cache entries and reports clear operation outcome ([`ClearState`])
    pub fn clear(&mut self) -> ClearState {
        if self.is_empty {
            return ClearState::AlreadyEmpty;
        }
        self.is_empty = true;
        self.final_layout_entry = None;
        self.measure_entries.clear();
        if let Some(offers) = &mut self.indefinite_offers { offers.clear(); }
        self.recently_used_entries = 0;
        self.next_measure_entry = 0;
        ClearState::Cleared
    }

    /// Returns true if all cache entries are None, else false
    pub fn is_empty(&self) -> bool {
        self.final_layout_entry.is_none() && self.measure_entries.is_empty()
    }
}

/// EXACT PATCH 31: tree-owned cache storage, allocated after a result is cached. Inline
/// text runs have engine nodes but their paragraph is the measured box, so
/// they never need the nine-entry layout cache. Keep an allocated cache when
/// cleared: active boxes reuse it across edits without another allocation.
#[cfg(feature = "taffy_tree")]
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct LazyCache(Option<std::boxed::Box<Cache>>);

#[cfg(feature = "taffy_tree")]
impl LazyCache {
    pub const fn new() -> Self {
        Self(None)
    }

    #[inline]
    pub fn get(&mut self, input: &LayoutInput, pass: u16, exact_axis: bool) -> Option<LayoutOutput> {
        self.0.as_deref_mut().and_then(|cache| cache.get(input, pass, exact_axis))
    }

    pub fn store(&mut self, input: &LayoutInput, output: LayoutOutput, pass: u16, policy: SizeCachePolicy) {
        if let Some(cache) = self.0.as_deref_mut() {
            cache.store(input, output, pass, policy);
        } else {
            let mut cache = Cache::new();
            cache.store(input, output, pass, policy);
            if !cache.is_empty {
                self.0 = Some(std::boxed::Box::new(cache));
            }
        }
    }

    #[inline]
    pub fn clear(&mut self) -> ClearState {
        self.0.as_deref_mut().map_or(ClearState::AlreadyEmpty, Cache::clear)
    }

    pub fn invalidate_final_layout(&mut self) {
        if let Some(cache) = self.0.as_deref_mut() { cache.invalidate_final_layout(); }
    }

    pub fn allocated_bytes(&self) -> usize {
        self.0.as_deref().map_or(0, Cache::allocated_bytes)
    }

    pub fn reset_pass_stamps(&mut self) {
        if let Some(cache) = self.0.as_deref_mut() { cache.reset_pass_stamps(); }
    }

    pub fn size_answers(&self) -> usize {
        self.0.as_deref().map_or(0, |cache| cache.measure_entries.len())
    }

    pub fn is_empty(&self) -> bool {
        self.0.as_deref().map_or(true, Cache::is_empty)
    }
}

/// Clear operation outcome. See [`Cache::clear`]
pub enum ClearState {
    /// Cleared some values
    Cleared,
    /// Everything was already cleared
    AlreadyEmpty,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::geometry::Line;
    use crate::style::Style;

    fn input(width: f32) -> LayoutInput {
        LayoutInput {
            run_mode: RunMode::ComputeSize,
            sizing_mode: SizingMode::InherentSize,
            axis: RequestedAxis::Both,
            known_dimensions: Size { width: Some(width), height: None },
            known_dimensions_are_definite: Size { width: true, height: true },
            parent_size: Size::NONE,
            available_space: Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent },
            vertical_margins_are_collapsible: Line::FALSE,
        }
    }

    fn output(width: f32) -> LayoutOutput {
        LayoutOutput::from_outer_size(Size { width, height: width })
    }

    #[cfg(feature = "taffy_tree")]
    #[test]
    fn lazy_storage_preserves_cache_answers_and_clear_state() {
        let mut lazy = LazyCache::new();
        let mut eager = Cache::new();
        let mut seen = Vec::new();
        for i in 0..240 {
            let mut request = input((i % 17) as f32);
            request.run_mode = [RunMode::PerformHiddenLayout, RunMode::ComputeSize, RunMode::PerformLayout][i % 3];
            request.axis = [RequestedAxis::Both, RequestedAxis::Vertical, RequestedAxis::Horizontal][(i / 3) % 3];
            request.sizing_mode = if i % 2 == 0 { SizingMode::InherentSize } else { SizingMode::ContentSize };
            request.parent_size.height = (i % 5 != 0).then_some((i % 7) as f32);
            request.known_dimensions_are_definite.width = i % 4 == 0;
            request.vertical_margins_are_collapsible.start = i % 2 == 0;
            let mut result = output(i as f32);
            if i % 7 == 0 { result.top_margin = CollapsibleMarginSet::from_margin(10.0); }
            if i % 11 == 0 { result.margins_can_collapse_through = true; }
            assert_eq!(lazy.get(&request, 0, false), eager.get(&request, 0, false));
            lazy.store(&request, result, 0, SizeCachePolicy::Nine);
            eager.store(&request, result, 0, SizeCachePolicy::Nine);
            seen.push(request);
            for request in seen.iter().rev().take(19) { assert_eq!(lazy.get(request, 0, false), eager.get(request, 0, false)); }
            assert_eq!(lazy.is_empty(), eager.is_empty());
            if i % 31 == 0 {
                assert_eq!(matches!(lazy.clear(), ClearState::Cleared), matches!(eager.clear(), ClearState::Cleared));
            }
        }
    }

    #[cfg(feature = "taffy_tree")]
    #[test]
    fn lazy_storage_allocates_only_for_a_cached_result_and_reuses_it() {
        let mut cache = LazyCache::new();
        assert!(matches!(cache.clear(), ClearState::AlreadyEmpty));
        assert!(cache.get(&input(10.0), 0, false).is_none());
        cache.store(&LayoutInput { run_mode: RunMode::PerformHiddenLayout, ..input(10.0) }, output(10.0), 0, SizeCachePolicy::Nine);
        let mut carried = output(10.0);
        carried.top_margin = CollapsibleMarginSet::from_margin(3.0);
        cache.store(&input(10.0), carried, 0, SizeCachePolicy::Nine);
        assert!(cache.0.is_none());
        cache.store(&input(10.0), output(10.0), 0, SizeCachePolicy::Nine);
        let allocation = cache.0.as_deref().unwrap() as *const Cache;
        let mut cloned = cache.clone();
        assert!(matches!(cache.clear(), ClearState::Cleared));
        assert_eq!(cloned.get(&input(10.0), 0, false), Some(output(10.0)));
        assert!(cache.get(&input(10.0), 0, false).is_none());
        cache.store(&input(11.0), output(11.0), 0, SizeCachePolicy::Nine);
        assert_eq!(allocation, cache.0.as_deref().unwrap() as *const Cache);
    }

    #[test]
    fn recently_used_measure_entries_get_a_second_chance() {
        let mut cache = Cache::new();
        for width in 0..CACHE_SIZE {
            cache.store(&input(width as f32), output(width as f32), 0, SizeCachePolicy::Nine);
        }
        cache.store(&input(CACHE_SIZE as f32), output(CACHE_SIZE as f32), 0, SizeCachePolicy::Nine);

        assert_eq!(cache.get(&input(1.0), 0, false), Some(output(1.0)));
        cache.store(&input((CACHE_SIZE + 1) as f32), output((CACHE_SIZE + 1) as f32), 0, SizeCachePolicy::Nine);

        assert_eq!(cache.get(&input(1.0), 0, false), Some(output(1.0)));
        assert_eq!(cache.get(&input(2.0), 0, false), None);
    }

    #[test]
    fn storing_an_existing_measurement_updates_it_in_place() {
        let mut cache = Cache::new();
        cache.store(&input(1.0), output(1.0), 0, SizeCachePolicy::Nine);
        cache.store(&input(2.0), output(2.0), 0, SizeCachePolicy::Nine);
        cache.store(&input(1.0), output(3.0), 0, SizeCachePolicy::Nine);

        assert_eq!(cache.measure_entries.len(), 2);
        assert_eq!(cache.get(&input(1.0), 0, false), Some(output(3.0)));
    }

    #[test]
    fn measurements_with_margin_collapse_metadata_are_not_cached() {
        let mut cache = Cache::new();

        let mut collapse_through = output(1.0);
        collapse_through.margins_can_collapse_through = true;
        cache.store(&input(1.0), collapse_through, 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&input(1.0), 0, false), None);

        let mut carried_margin = output(2.0);
        carried_margin.top_margin = CollapsibleMarginSet::from_margin(10.0);
        cache.store(&input(2.0), carried_margin, 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&input(2.0), 0, false), None);
    }

    #[test]
    fn retrieving_a_measurement_only_marks_its_slot_as_used() {
        let mut cache = Cache::new();
        cache.store(&input(1.0), output(1.0), 0, SizeCachePolicy::Nine);
        cache.store(&input(2.0), output(2.0), 0, SizeCachePolicy::Nine);
        cache.recently_used_entries = 0;
        let entries = cache.measure_entries.clone();

        assert_eq!(cache.get(&input(1.0), 0, false), Some(output(1.0)));
        assert_eq!(cache.measure_entries, entries);
        assert_ne!(cache.recently_used_entries, 0);
    }

    // EXACT PATCH 11 — the upstream fixture: an entry answers only the inputs it was computed
    // under. Each key component, alone, separates two entries.

    fn perform(width: f32, height: f32) -> LayoutInput {
        LayoutInput {
            run_mode: RunMode::PerformLayout,
            known_dimensions: Size { width: Some(width), height: Some(height) },
            parent_size: Size { width: Some(width), height: Some(height) },
            ..input(width)
        }
    }

    #[test]
    fn entries_are_keyed_on_margin_collapsibility() {
        let collapsible = LayoutInput { vertical_margins_are_collapsible: Line::TRUE, ..perform(100.0, 50.0) };
        let mut cache = Cache::new();
        cache.store(&collapsible, output(1.0), 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&perform(100.0, 50.0), 0, false), None);
        assert_eq!(cache.get(&collapsible, 0, false), Some(output(1.0)));

        let measured = LayoutInput { vertical_margins_are_collapsible: Line::TRUE, ..input(1.0) };
        cache.store(&measured, output(2.0), 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&input(1.0), 0, false), None);
    }

    #[test]
    fn entries_are_keyed_on_sizing_mode() {
        let content = LayoutInput { sizing_mode: SizingMode::ContentSize, ..perform(100.0, 50.0) };
        let mut cache = Cache::new();
        cache.store(&perform(100.0, 50.0), output(1.0), 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&content, 0, false), None);

        cache.store(&input(1.0), output(2.0), 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&LayoutInput { sizing_mode: SizingMode::ContentSize, ..input(1.0) }, 0, false), None);
    }

    #[test]
    fn measurements_are_keyed_on_the_parent_height() {
        let tall = LayoutInput { parent_size: Size { width: Some(78.0), height: Some(62.0) }, ..input(31.0) };
        let auto = LayoutInput { parent_size: Size { width: Some(78.0), height: None }, ..input(31.0) };
        let mut cache = Cache::new();
        cache.store(&tall, output(53.0), 0, SizeCachePolicy::Nine);
        assert_eq!(cache.get(&auto, 0, false), None);
        assert_eq!(cache.get(&tall, 0, false), Some(output(53.0)));
    }

    const MAX_CONTENT: Size<AvailableSpace> =
        Size { width: AvailableSpace::MaxContent, height: AvailableSpace::MaxContent };

    /// Lays a tree out, restyles one node and lays it out again; returns every node's
    /// unrounded layout, and a fresh tree's built directly with the second style.
    /// `build` returns the restyled node first and the root last.
    fn relayout_and_fresh(
        before: Style,
        after: Style,
        available: Size<AvailableSpace>,
        build: fn(&mut crate::TaffyTree<()>, Style) -> Vec<crate::NodeId>,
    ) -> (Vec<crate::Layout>, Vec<crate::Layout>) {
        let layouts = |tree: &crate::TaffyTree<()>, ids: &[crate::NodeId]| {
            ids.iter().map(|&id| *tree.unrounded_layout(id)).collect::<Vec<_>>()
        };
        let mut tree = crate::TaffyTree::new();
        let ids = build(&mut tree, before);
        let root = *ids.last().unwrap();
        tree.compute_layout(root, available).unwrap();
        tree.set_style(ids[0], after.clone()).unwrap();
        tree.compute_layout(root, available).unwrap();
        let mut fresh = crate::TaffyTree::new();
        let fresh_ids = build(&mut fresh, after);
        fresh.compute_layout(*fresh_ids.last().unwrap(), available).unwrap();
        (layouts(&tree, &ids), layouts(&fresh, &fresh_ids))
    }

    /// <div style="width:100px;height:200px"> <div style="height:50px">
    ///   <div style="height:10px;margin-top:20px"></div> </div> </div>, then the outer box
    /// becomes `container`. The inner margin collapses through the middle box (CSS 2.1 §8.3.1)
    /// only while its parent is a block; as a flex or grid item the middle box establishes an
    /// independent formatting context and contains it.
    fn relayout_block_parent_as(container: Style) {
        use crate::style::{Dimension, Display, LengthPercentageAuto};
        let block = Style {
            display: Display::Block,
            size: Size { width: Dimension::length(100.0), height: Dimension::length(200.0) },
            ..Style::default()
        };
        let container = Style { size: block.size, ..container };
        let (incremental, fresh) = relayout_and_fresh(block, container, MAX_CONTENT, |tree, root| {
            let inner = tree
                .new_leaf(Style {
                    display: Display::Block,
                    size: Size { width: Dimension::auto(), height: Dimension::length(10.0) },
                    margin: crate::Rect { top: LengthPercentageAuto::length(20.0), ..crate::Rect::zero() },
                    ..Style::default()
                })
                .unwrap();
            let middle = tree
                .new_with_children(
                    Style {
                        display: Display::Block,
                        size: Size { width: Dimension::auto(), height: Dimension::length(50.0) },
                        ..Style::default()
                    },
                    &[inner],
                )
                .unwrap();
            let root = tree.new_with_children(root, &[middle]).unwrap();
            vec![root, middle, inner, root]
        });
        assert_eq!(fresh[2].location.y, 20.0);
        assert_eq!(fresh[1].location.y, 0.0);
        assert_eq!(incremental, fresh);
    }

    #[test]
    fn a_block_child_does_not_keep_a_collapsed_margin_as_a_flex_item() {
        // Flex lays its item out with `ContentSize` and no collapsible margins; the block
        // parent used `InherentSize` and collapsible margins.
        use crate::style::{Display, FlexDirection};
        relayout_block_parent_as(Style {
            display: Display::Flex,
            flex_direction: FlexDirection::Column,
            ..Style::default()
        });
    }

    #[test]
    fn a_block_child_does_not_keep_a_collapsed_margin_as_a_grid_item() {
        // Grid lays its item out with `InherentSize`, as the block parent did: only
        // `vertical_margins_are_collapsible` tells the two inputs apart.
        relayout_block_parent_as(Style { display: crate::style::Display::Grid, ..Style::default() });
    }

    #[test]
    fn a_percentage_height_is_not_reused_after_its_containing_block_loses_its_height() {
        use crate::style::{Dimension, Display, FlexDirection};
        // <div style="display:flex;flex-direction:column;width:78px">
        //   <div style="width:78px;height:62px">
        //     <div style="display:flex;width:31px;height:86%"> <div style="height:19px"></div>
        // </div></div></div>, then the middle block's height becomes auto: the item's
        // percentage no longer resolves (CSS 2.1 §10.5) and it is 19px tall, not 53.32px.
        let block = |height| Style {
            display: Display::Block,
            size: Size { width: Dimension::length(78.0), height },
            ..Style::default()
        };
        let (incremental, fresh) = relayout_and_fresh(
            block(Dimension::length(62.0)),
            block(Dimension::auto()),
            MAX_CONTENT,
            |tree, middle_style| {
                let leaf = tree
                    .new_leaf(Style {
                        display: Display::Block,
                        size: Size { width: Dimension::auto(), height: Dimension::length(19.0) },
                        ..Style::default()
                    })
                    .unwrap();
                let item = tree
                    .new_with_children(
                        Style {
                            display: Display::Flex,
                            size: Size { width: Dimension::length(31.0), height: Dimension::percent(0.86) },
                            ..Style::default()
                        },
                        &[leaf],
                    )
                    .unwrap();
                let middle = tree.new_with_children(middle_style, &[item]).unwrap();
                let root = tree
                    .new_with_children(
                        Style {
                            display: Display::Flex,
                            flex_direction: FlexDirection::Column,
                            size: Size { width: Dimension::length(78.0), height: Dimension::auto() },
                            ..Style::default()
                        },
                        &[middle],
                    )
                    .unwrap();
                vec![middle, item, leaf, root]
            },
        );
        assert_eq!(fresh[1].size.height, 19.0);
        assert_eq!(incremental, fresh);
    }
}
