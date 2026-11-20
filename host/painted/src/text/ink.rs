//! CPU-only conservative ink selection. Full shaping and GPU paint stay intact.
use super::catalog::{subpixel, Catalog, FaceKey, GlyphKey};
use super::lines::{LayoutGlyph, Lines};
use super::{GlyphRun, Paragraph, RunPaint, TextEngine};
use std::mem::size_of;
use std::rc::{Rc, Weak};
use std::sync::Arc;
use tiny_skia::Transform;

pub(super) const MAX_BYTES: usize = 8 * 1024 * 1024;
const ENVELOPES: usize = 256;
pub(super) const GLYPHS_PER_CHUNK: usize = 128;
// Beyond this range float/integer conversion and inversion use the full path.
const COORD_LIMIT: f64 = 16_777_216.0;

#[derive(Clone, Copy, Debug)]
pub(super) struct Bounds {
    x0: f64,
    y0: f64,
    x1: f64,
    y1: f64,
}
impl Bounds {
    const EMPTY: Self = Self {
        x0: f64::INFINITY,
        y0: f64::INFINITY,
        x1: f64::NEG_INFINITY,
        y1: f64::NEG_INFINITY,
    };
    const ALL: Self = Self {
        x0: f64::NEG_INFINITY,
        y0: f64::NEG_INFINITY,
        x1: f64::INFINITY,
        y1: f64::INFINITY,
    };
    fn finite(self) -> bool {
        [self.x0, self.y0, self.x1, self.y1]
            .iter()
            .all(|v| v.is_finite())
    }
    fn empty(self) -> bool {
        self.x0 > self.x1 || self.y0 > self.y1
    }
    fn union(self, b: Self) -> Self {
        Self {
            x0: self.x0.min(b.x0),
            y0: self.y0.min(b.y0),
            x1: self.x1.max(b.x1),
            y1: self.y1.max(b.y1),
        }
    }
    fn translated(self, x: f64, y: f64) -> Self {
        if self.empty() {
            return self;
        }
        if !x.is_finite() || !y.is_finite() {
            return Self::ALL;
        }
        Self {
            x0: self.x0 + x,
            y0: self.y0 + y,
            x1: self.x1 + x,
            y1: self.y1 + y,
        }
    }
    fn expand(self, x: f64, y: f64) -> Self {
        Self {
            x0: self.x0 - x,
            y0: self.y0 - y,
            x1: self.x1 + x,
            y1: self.y1 + y,
        }
    }
    fn overlaps(self, b: Self) -> bool {
        !self.empty()
            && !b.empty()
            && self.x0 <= b.x1
            && self.x1 >= b.x0
            && self.y0 <= b.y1
            && self.y1 >= b.y0
    }
}

// Numeric canonical-key envelopes shared by index builds in this catalog only.
// The weak generation never pins fonts, glyph images, sources, or old indices.
#[derive(Default)]
pub(super) struct Envelopes {
    catalog: Weak<()>,
    pub(super) entries: Vec<(GlyphKey, Bounds)>,
}
impl Envelopes {
    fn prepare(&mut self, catalog: &Rc<()>) -> Option<()> {
        let token = Rc::downgrade(catalog);
        if !self.catalog.ptr_eq(&token) {
            self.entries.clear();
            self.catalog = token;
        }
        if self.entries.capacity() < ENVELOPES {
            self.entries
                .try_reserve_exact(ENVELOPES - self.entries.len())
                .ok()?;
        }
        Some(())
    }
}

#[derive(Default)]
pub(super) struct Cache {
    catalog: Weak<()>,
    scale: u32,
    pub index: Option<IndexOwner>,
    previous: Weak<Paragraph>,
}
impl Cache {
    /// Attach numeric worker output; the UI catalog Weak stays local.
    pub fn from_index(catalog: &Rc<()>, scale: f32, layout: Arc<super::transfer::Layout>) -> Self {
        Self {
            catalog: Rc::downgrade(catalog),
            scale: scale.to_bits(),
            index: Some(IndexOwner::Prepared(layout)),
            previous: Weak::new(),
        }
    }

    pub fn matches(&self, catalog: &Rc<()>, scale: f32) -> bool {
        self.scale == scale.to_bits() && self.catalog.ptr_eq(&Rc::downgrade(catalog))
    }
    pub fn reset(&mut self, catalog: &Rc<()>, scale: f32) {
        self.index = None; // Release this owner before allocation; siblings may pin it.
        self.catalog = Rc::downgrade(catalog);
        self.scale = scale.to_bits();
    }
    pub fn prepare(&mut self, catalog: &mut Catalog, paragraph: &Paragraph, scale: f32) -> bool {
        if self.matches(&catalog.ink_catalog, scale) {
            return false;
        }
        let previous = std::mem::take(&mut self.previous).upgrade();
        self.reset(&catalog.ink_catalog, scale);
        let cache = previous.as_ref().map(|old| old.ink.borrow());
        let prior = previous
            .as_deref()
            .zip(cache.as_deref())
            .and_then(|(old, ink)| {
                ink.index
                    .as_deref()
                    .filter(|_| ink.matches(&catalog.ink_catalog, scale))
                    .map(|index| (old, index))
            });
        self.index = Index::replacing(catalog, paragraph, scale, MAX_BYTES, prior).map(Into::into);
        true
    }
    pub fn bytes(&self) -> usize {
        self.index.as_ref().map_or(0, |index| index.bytes())
    }
}

// The UI cache owns its prepared backing, not merely cloned arrays. This is
// what keeps the source's weak slot usable after CompletedText is consumed.
// Reset affects only this cache; sibling numeric indices remain immutable.
pub(super) enum IndexOwner {
    Local(Index),
    Prepared(Arc<super::transfer::Layout>),
}
impl From<Index> for IndexOwner {
    fn from(index: Index) -> Self {
        Self::Local(index)
    }
}
impl std::ops::Deref for IndexOwner {
    type Target = Index;
    fn deref(&self) -> &Index {
        match self {
            Self::Local(index) => index,
            Self::Prepared(layout) => &layout.index,
        }
    }
}

impl TextEngine {
    pub(crate) fn physical_glyph_runs(
        &mut self,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        origin: (f32, f32),
        scale: f32,
        clip: Option<(Transform, [f32; 4])>,
    ) -> Vec<GlyphRun> {
        let mut cache = paragraph.ink.borrow_mut();
        let visible = clip
            .filter(|_| paragraph.layouts().glyphs.len() > GLYPHS_PER_CHUNK)
            .and_then(|(transform, [x, y, w, h])| {
                cache.prepare(&mut paragraph.source.catalog.borrow_mut(), paragraph, scale);
                let index = cache.index.as_deref()?;
                Some((
                    index,
                    index.viewport(origin, scale, transform, (x, y, w, h))?,
                ))
            });
        Self::glyph_runs_mapped(paragraph, palette, visible, |g, baseline| {
            exact_ink::glyph::position(g.x, g.y, baseline, origin, scale)
        })
    }
}

impl Paragraph {
    /// The previous frame owns the old field while this one paints. A
    /// deferred or GPU paint need not keep its paragraph or build an index.
    pub(crate) fn reuse_field_ink(&self, old: &Rc<Self>) {
        let spec = &self.source.spec;
        if !std::ptr::eq(self, old.as_ref())
            && old.ink.borrow().index.is_some()
            && self.ink.borrow().index.is_none()
            && self.remake.is_some()
            && self.remake == old.remake
            && matches!(spec.runs.as_slice(), [run] if run.edges.is_empty() && run.inline.is_none())
            && super::cache::same_style(spec, &old.source.spec)
        {
            self.ink.borrow_mut().previous = Rc::downgrade(old);
        }
    }

    /// Shaped glyph ink in paragraph points, including raster placement rounding.
    pub(crate) fn paint_bounds(&self, scale: f32) -> Option<(f32, f32, f32, f32)> {
        let mut catalog = self.source.catalog.borrow_mut();
        let mut cache = self.ink.borrow_mut();
        cache.prepare(&mut catalog, self, scale);
        let index = cache.index.as_ref()?;
        let bounds = index.spans[1];
        if bounds.empty() {
            return Some((0., 0., 0., 0.));
        }
        if !bounds.finite() || index.max_input > COORD_LIMIT {
            return None;
        }
        let s = f64::from(scale);
        let b = bounds.expand(1., 1.);
        Some((
            (b.x0 / s) as f32,
            (b.y0 / s) as f32,
            ((b.x1 - b.x0) / s) as f32,
            ((b.y1 - b.y0) / s) as f32,
        ))
    }
}

/// Each face of `lines` as this catalog's raster slot.
pub(super) fn slots(catalog: &mut Catalog, lines: &Lines) -> Vec<u32> {
    lines
        .faces
        .iter()
        .map(|f| {
            catalog.slot(
                &f.font,
                &FaceKey {
                    face: f.id(),
                    coords: f.coords.clone(),
                    skew: f.skew,
                },
            )
        })
        .collect()
}

/// A glyph's raster key and whole-pixel origin at `scale`, the pen at
/// `offset` (device pixels): x in quarter-pixel phases, y truncated to the
/// pixel row, as cosmic-text's `LayoutGlyph::physical` placed them.
pub(super) fn physical(
    g: &LayoutGlyph,
    slot: u32,
    offset: (f32, f32),
    scale: f32,
) -> (GlyphKey, i32, i32) {
    let (x, x_bin) = subpixel(g.x.mul_add(scale, offset.0));
    let (y, y_bin) = subpixel(g.y.mul_add(scale, offset.1).trunc());
    (
        GlyphKey {
            slot,
            glyph: g.glyph_id as u16,
            size_bits: (g.font_size * scale).to_bits(),
            x_bin,
            y_bin,
        },
        x,
        y,
    )
}

pub(super) struct Index {
    #[cfg(test)]
    pub(super) lifetime: std::sync::Arc<()>,
    spans: Vec<Bounds>,
    // Only long lines need a map from leaves to (visual line, first glyph).
    chunks: Vec<(usize, usize)>,
    leaf_count: usize,
    leaves: usize,
    // Bounds on inputs to the CPU's f32 baseline arithmetic, not CSS line boxes.
    max_input: f64,
    // Keep each leaf's contribution so an edit can remove the old maximum.
    leaf_inputs: Vec<f32>,
}

fn storage(count: usize, limit: usize) -> Option<(usize, usize)> {
    let leaves = count.max(1).checked_next_power_of_two()?;
    let nodes = leaves.checked_mul(2)?;
    let bytes = nodes.checked_mul(size_of::<Bounds>())?;
    (bytes <= limit).then_some((leaves, nodes))
}

/// How a build finds a glyph's envelope: the catalog's kept placements
/// first, rendering a missing phase without keeping it.
type EnvelopeOf = fn(&mut Catalog, GlyphKey) -> Bounds;

impl Index {
    pub fn build(engine: &mut Catalog, p: &Paragraph, scale: f32) -> Option<Self> {
        Self::with_limit(engine, p, scale, MAX_BYTES)
    }
    pub(super) fn with_limit(
        engine: &mut Catalog,
        p: &Paragraph,
        scale: f32,
        limit: usize,
    ) -> Option<Self> {
        Self::replacing(engine, p, scale, limit, None)
    }

    fn replacing(
        engine: &mut Catalog,
        p: &Paragraph,
        scale: f32,
        limit: usize,
        previous: Option<(&Paragraph, &Index)>,
    ) -> Option<Self> {
        #[cfg(test)]
        count(|n| n.attempts += 1);
        engine.envelopes.prepare(&engine.ink_catalog)?;
        let mut envelopes = std::mem::take(&mut engine.envelopes.entries);
        let result = Self::build_with(engine, p, scale, limit, &mut envelopes, envelope, previous);
        engine.envelopes.entries = envelopes;
        result
    }

    fn build_with(
        engine: &mut Catalog,
        p: &Paragraph,
        scale: f32,
        limit: usize,
        envelopes: &mut Vec<(GlyphKey, Bounds)>,
        envelope_of: EnvelopeOf,
        previous: Option<(&Paragraph, &Index)>,
    ) -> Option<Self> {
        if !scale.is_finite() || scale <= 0.0 {
            return None;
        }
        let lines = p.layouts();
        if p.baselines.len() != lines.lines.len() {
            return None;
        }
        let chunked = lines
            .lines
            .iter()
            .any(|line| lines.glyphs_of(line).len() > GLYPHS_PER_CHUNK);
        let chunk_count = if chunked {
            lines.lines.iter().try_fold(0usize, |n, line| {
                n.checked_add(lines.glyphs_of(line).len().div_ceil(GLYPHS_PER_CHUNK))
            })?
        } else {
            0
        };
        let leaf_count = if chunked {
            chunk_count
        } else {
            lines.lines.len()
        };
        let metadata = chunk_count
            .checked_mul(size_of::<(usize, usize)>())?
            .checked_add(leaf_count.checked_mul(size_of::<f32>())?)?;
        let (leaves, nodes) = storage(leaf_count, limit.checked_sub(metadata)?)?;
        let mut result = Self {
            #[cfg(test)]
            lifetime: std::sync::Arc::new(()),
            spans: Vec::new(),
            chunks: Vec::new(),
            leaf_count,
            leaves,
            max_input: 0.0,
            leaf_inputs: Vec::new(),
        };
        result.spans.try_reserve_exact(nodes).ok()?;
        result.chunks.try_reserve_exact(chunk_count).ok()?;
        result.leaf_inputs.try_reserve_exact(leaf_count).ok()?;
        if result.bytes() > limit {
            return None;
        }
        result.spans.resize(nodes, Bounds::EMPTY);
        result.leaf_inputs.resize(leaf_count, 0.);
        let slots = slots(engine, lines);
        let mut same_hard = None;
        for (line_no, line) in lines.lines.iter().enumerate() {
            let baseline = p.baselines[line_no];
            let reusable = previous.and_then(|(old, index)| {
                let old_lines = old.layouts();
                let other = old_lines.lines.get(line_no)?;
                if line_no == 0 || lines.lines[line_no - 1].hard != line.hard {
                    // For a single styled run, shaping and wrapping start
                    // afresh at each hard break. Compare its source once
                    // for all its visual lines, regardless of glyph count.
                    same_hard = ((line_no == 0 || old_lines.lines[line_no - 1].hard != other.hard)
                        && (line.hard == 0) == (other.hard == 0)
                        && !(line.hard == 0
                            && p.source.spec.text_wrap_style
                                == exact_kernel::TextWrapStyle::Balance)
                        && p.source.data.lines[line.hard as usize].text
                            == old.source.data.lines[other.hard as usize].text)
                        .then_some(other.hard);
                }
                (same_hard == Some(other.hard)
                    && baseline.to_bits() == old.baselines[line_no].to_bits()
                    && lines.glyphs_of(line).len() == old_lines.glyphs_of(other).len())
                .then_some(index)
            });
            for (chunk, glyphs) in lines.glyphs_of(line).chunks(GLYPHS_PER_CHUNK).enumerate() {
                let first = chunk * GLYPHS_PER_CHUNK;
                let leaf = if chunked {
                    result.chunks.push((line_no, first));
                    result.chunks.len() - 1
                } else {
                    line_no
                };
                if let Some((old, i)) =
                    reusable.and_then(|old| Some((old, old.leaf(line_no, first)?)))
                {
                    result.spans[leaves + leaf] = old.spans[old.leaves + i];
                    result.leaf_inputs[leaf] = old.leaf_inputs[i];
                    continue;
                }
                let mut span = Bounds::EMPTY;
                let mut max_input = 0.0f32;
                for glyph in glyphs {
                    #[cfg(test)]
                    count(|n| n.glyphs += 1);
                    let (x, y) = (glyph.x, glyph.y);
                    if !x.is_finite() || !y.is_finite() || !baseline.is_finite() {
                        span = Bounds::ALL;
                        continue;
                    }
                    max_input = max_input.max(x.abs()).max(y.abs()).max(baseline.abs());
                    let (mut key, _, _) =
                        physical(glyph, slots[glyph.face as usize], (0.0, 0.0), scale);
                    key.x_bin = 0;
                    key.y_bin = 0;
                    let bound = match envelopes.binary_search_by_key(&key, |(k, _)| *k) {
                        Ok(i) => envelopes[i].1,
                        Err(_) => {
                            let bound = envelope_of(engine, key);
                            if envelopes.len() == ENVELOPES {
                                envelopes.clear();
                            }
                            let i = envelopes
                                .binary_search_by_key(&key, |(k, _)| *k)
                                .unwrap_err();
                            envelopes.insert(i, (key, bound));
                            bound
                        }
                    };
                    span = span.union(bound.translated(
                        f64::from(x) * f64::from(scale),
                        (f64::from(baseline) + f64::from(y)) * f64::from(scale),
                    ));
                }
                result.spans[leaves + leaf] = span;
                result.leaf_inputs[leaf] = max_input;
            }
        }
        for i in (1..leaves).rev() {
            result.spans[i] = result.spans[2 * i].union(result.spans[2 * i + 1]);
        }
        result.max_input = f64::from(result.leaf_inputs.iter().copied().fold(0., f32::max));
        Some(result)
    }

    fn leaf(&self, line: usize, first: usize) -> Option<usize> {
        if self.chunks.is_empty() {
            return (first == 0 && line < self.leaf_count).then_some(line);
        }
        self.chunks.binary_search(&(line, first)).ok()
    }

    pub(super) fn bytes(&self) -> usize {
        self.spans.capacity() * size_of::<Bounds>()
            + self.chunks.capacity() * size_of::<(usize, usize)>()
            + self.leaf_inputs.capacity() * size_of::<f32>()
    }

    pub fn glyphs<'a>(&self, p: &'a Paragraph, chunk: usize) -> (&'a [LayoutGlyph], f32) {
        let (line, first) = self.chunks.get(chunk).copied().unwrap_or((chunk, 0));
        let glyphs = p.layouts().glyphs_of(&p.layouts().lines[line]);
        (
            &glyphs[first..(first + GLYPHS_PER_CHUNK).min(glyphs.len())],
            p.baselines[line],
        )
    }

    /// Bounds in the same pre-transform device-pixel coordinates as our spans.
    /// All uncertain arithmetic fails open. The deliberately generous error
    /// envelope includes CPU baseline mul/add rounding, quantization, integer
    /// rectangle conversion, and f32 affine arithmetic in tiny-skia.
    pub fn viewport(
        &self,
        origin: (f32, f32),
        scale: f32,
        ts: Transform,
        clip: (f32, f32, f32, f32),
    ) -> Option<Bounds> {
        let root = self.spans[1];
        if root.empty() {
            return Some(Bounds::EMPTY);
        }
        let [sx, kx, ky, sy, tx, ty] = [ts.sx, ts.kx, ts.ky, ts.sy, ts.tx, ts.ty].map(f64::from);
        let (ox, oy) = (
            f64::from(origin.0) * f64::from(scale),
            f64::from(origin.1) * f64::from(scale),
        );
        let (x, y, w, h) = (
            f64::from(clip.0),
            f64::from(clip.1),
            f64::from(clip.2),
            f64::from(clip.3),
        );
        if !root.finite()
            || ![sx, kx, ky, sy, tx, ty, ox, oy, x, y, w, h]
                .iter()
                .all(|n| n.is_finite())
        {
            return None;
        }
        if w <= 0.0 || h <= 0.0 {
            return Some(Bounds::EMPTY);
        }
        let magnitude = ox.abs()
            + oy.abs()
            + self.max_input * f64::from(scale)
            + root
                .x0
                .abs()
                .max(root.x1.abs())
                .max(root.y0.abs())
                .max(root.y1.abs())
            + 1.0;
        if magnitude > COORD_LIMIT {
            return None;
        }
        let eps = f64::from(f32::EPSILON);
        let pixel_error = 4.0 + 16.0 * eps * magnitude;
        let dx = 4.0 + 16.0 * eps * ((sx.abs() + kx.abs()) * (magnitude + pixel_error) + tx.abs());
        let dy = 4.0 + 16.0 * eps * ((ky.abs() + sy.abs()) * (magnitude + pixel_error) + ty.abs());
        let det = sx * sy - kx * ky;
        let norm = sx.abs().max(kx.abs()).max(ky.abs()).max(sy.abs());
        if !det.is_finite() || det.abs() <= norm * norm * 1e-8 {
            return None;
        }
        let mut result = Bounds::EMPTY;
        for (px, py) in [
            (x - dx, y - dy),
            (x + w + dx, y - dy),
            (x - dx, y + h + dy),
            (x + w + dx, y + h + dy),
        ] {
            let (px, py) = (px - tx, py - ty);
            let (qx, qy) = (
                (sy * px - kx * py) / det - ox,
                (sx * py - ky * px) / det - oy,
            );
            result = result.union(Bounds {
                x0: qx,
                y0: qy,
                x1: qx,
                y1: qy,
            });
        }
        result = result.expand(pixel_error, pixel_error);
        result.finite().then_some(result)
    }

    pub fn visit(&self, query: Bounds, mut draw: impl FnMut(usize)) -> usize {
        fn walk(index: &Index, node: usize, query: Bounds, draw: &mut impl FnMut(usize)) -> usize {
            if !index.spans[node].overlaps(query) {
                return 1;
            }
            if node >= index.leaves {
                let chunk = node - index.leaves;
                if chunk < index.leaf_count {
                    draw(chunk);
                }
                1
            } else {
                1 + walk(index, node * 2, query, draw) + walk(index, node * 2 + 1, query, draw)
            }
        }
        walk(self, 1, query, &mut draw)
    }
}

/// The union of a glyph's ink over the four x phases. The CPU places every
/// finite glyph on a whole-pixel row (y truncated): the y phase is always
/// zero. Tiny-skia applies transforms after this raster choice.
fn envelope(engine: &mut Catalog, mut key: GlyphKey) -> Bounds {
    if !f32::from_bits(key.size_bits).is_finite() {
        return Bounds::ALL;
    }
    let mut result = Bounds::EMPTY;
    for bin in 0..4 {
        key.x_bin = bin;
        #[cfg(test)]
        count(|n| n.raster_phases += 1);
        // Reuse only an existing exact-key placement; do not retain extra
        // phases. A kept None is no ink, distinct from an absent entry.
        let placement = if engine.has_placement(&key) {
            #[cfg(test)]
            count(|n| n.cached_placements += 1);
            engine.placement(key)
        } else {
            #[cfg(test)]
            count(|n| n.uncached_calls += 1);
            engine.placement_uncached(key)
        };
        if let Some(p) = placement {
            if p.width != 0 && p.height != 0 {
                result = result.union(Bounds {
                    x0: f64::from(p.left),
                    y0: -f64::from(p.top),
                    x1: f64::from(p.left) + f64::from(p.width),
                    y1: -f64::from(p.top) + f64::from(p.height),
                });
            }
        }
    }
    result
}

#[cfg(test)]
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct BuildWork {
    pub attempts: usize,
    pub glyphs: usize,
    pub raster_phases: usize,
    pub uncached_calls: usize,
    pub cached_placements: usize,
}
#[cfg(test)]
thread_local! { static BUILD_WORK: std::cell::Cell<BuildWork> = const {
    std::cell::Cell::new(BuildWork { attempts: 0, glyphs: 0, raster_phases: 0, uncached_calls: 0, cached_placements: 0 })
}; }
#[cfg(test)]
pub(super) fn build_work() -> BuildWork {
    BUILD_WORK.with(std::cell::Cell::get)
}
#[cfg(test)]
fn count(f: impl FnOnce(&mut BuildWork)) {
    BUILD_WORK.with(|c| {
        let mut value = c.get();
        f(&mut value);
        c.set(value);
    });
}

// Test-only formula without the catalog's shared envelopes or kept
// placements: every phase rendered fresh, envelopes scratch to this build.
#[cfg(test)]
impl Index {
    pub(super) fn uncached_oracle(
        engine: &mut Catalog,
        p: &Paragraph,
        scale: f32,
        limit: usize,
    ) -> Option<Self> {
        let mut envelopes = Vec::new();
        envelopes.try_reserve_exact(ENVELOPES).ok()?;
        Self::build_with(
            engine,
            p,
            scale,
            limit,
            &mut envelopes,
            uncached_envelope_oracle,
            None,
        )
    }

    pub(super) fn assert_same_numeric(&self, other: &Self) {
        assert_eq!(self.leaves, other.leaves);
        assert_eq!(self.max_input.to_bits(), other.max_input.to_bits());
        assert_eq!(self.bytes(), other.bytes());
        let bits = |b: &Bounds| [b.x0, b.y0, b.x1, b.y1].map(f64::to_bits);
        assert_eq!(
            self.spans.iter().map(bits).collect::<Vec<_>>(),
            other.spans.iter().map(bits).collect::<Vec<_>>()
        );
        assert_eq!(self.chunks, other.chunks);
        assert_eq!(self.leaf_inputs, other.leaf_inputs);
    }
}
#[cfg(test)]
fn uncached_envelope_oracle(engine: &mut Catalog, mut key: GlyphKey) -> Bounds {
    if !f32::from_bits(key.size_bits).is_finite() {
        return Bounds::ALL;
    }
    let mut result = Bounds::EMPTY;
    for bin in 0..4 {
        key.x_bin = bin;
        if let Some(p) = engine.render_placement(key) {
            if p.width != 0 && p.height != 0 {
                result = result.union(Bounds {
                    x0: f64::from(p.left),
                    y0: -f64::from(p.top),
                    x1: f64::from(p.left) + f64::from(p.width),
                    y1: -f64::from(p.top) + f64::from(p.height),
                });
            }
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn array_budget_and_checked_dimensions_refuse_before_allocation() {
        assert!(storage(usize::MAX, MAX_BYTES).is_none());
        assert!(storage(200_000, MAX_BYTES).is_none());
        assert!(storage(1_000, 64).is_none());
        assert!(storage(1_000, MAX_BYTES).is_some());
    }
    #[test]
    fn unknown_span_keeps_original_order_without_hiding_known_neighbors() {
        let index = Index {
            lifetime: std::sync::Arc::new(()),
            spans: vec![Bounds::EMPTY, Bounds::ALL, Bounds::ALL, Bounds::ALL],
            chunks: Vec::new(),
            leaf_count: 2,
            leaves: 2,
            max_input: 0.0,
            leaf_inputs: vec![0.; 2],
        };
        let mut order = Vec::new();
        index.visit(
            Bounds {
                x0: 0.0,
                y0: 0.0,
                x1: 1.0,
                y1: 1.0,
            },
            |line| order.push(line),
        );
        assert_eq!(order, [0, 1]);
    }
}
