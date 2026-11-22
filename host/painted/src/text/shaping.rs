//! One shape per paragraph; each width breaks it and keeps its own lines.
//!
//! A paragraph is shaped once by Parley, one layout per hard line (text
//! between line feeds: a Markdown list item's lines take that line's own
//! inset, as before). Each width breaks the same layouts again, in place —
//! Parley's re-breaking is cheap and a layout's breaking state is its own —
//! and copies out what painting needs ([`super::lines`]). The line box is
//! the host's: the strut plus each face's ascent, descent and gap.
use super::lines::{self, LayoutGlyph, LayoutLine, Lines, Room, RoomKind};
use super::*;
use parley::{
    Alignment, AlignmentOptions, BaseDirection, IndentOptions, LineBreakContext, RangedBuilder,
    StyleProperty,
};
use std::ops::Range;
use std::sync::Mutex;

#[cfg(test)]
thread_local! { static SHAPE_LINES: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Hard lines shaped on this thread (test probe).
#[cfg(test)]
pub(super) fn shape_line_calls() -> usize {
    SHAPE_LINES.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! { static BREAKS: std::cell::Cell<(usize, usize)> = const { std::cell::Cell::new((0, 0)) }; }
/// Widths whose lines were copied out on this thread, and hard lines
/// Parley broke for them (test probe).
#[cfg(test)]
pub(super) fn break_calls() -> (usize, usize) {
    BREAKS.with(std::cell::Cell::get)
}

#[cfg(test)]
thread_local! { static WIDTH_FONT_LOOKUPS: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }
/// Faces whose metrics were read from font data on this thread (test probe).
#[cfg(test)]
pub(super) fn width_font_lookups() -> usize {
    WIDTH_FONT_LOOKUPS.with(std::cell::Cell::get)
}
#[cfg(test)]
pub(super) fn count_font_lookup() {
    WIDTH_FONT_LOOKUPS.with(|n| n.set(n.get() + 1));
}

/// A face's vertical metrics in font units.
#[derive(Clone, Copy, Debug)]
pub(super) struct RawFontMetrics {
    pub(super) units_per_em: u16,
    pub(super) ascent: f32,
    pub(super) descent: f32,
    pub(super) leading: f32,
    /// The cap height and x-height, for `text-box-edge` (font units).
    pub(super) cap_height: f32,
    pub(super) x_height: f32,
}

/// Chrome's line-break choices for ordinary paragraphs (LLP 1085.000 G6):
/// the table and rules the text-flow walker uses, so the two paths break
/// alike (`exact_textflow::chrome_break`).
fn chrome(cx: LineBreakContext) -> Option<bool> {
    exact_textflow::chrome_break(cx.before_before, cx.before, cx.after)
}
static CHROME: &parley::LineBreakOverrideFn = &(chrome as fn(LineBreakContext) -> Option<bool>);

/// One run's style over `range` of the text being built.
pub(super) fn push_style(
    builder: &mut RangedBuilder<'_, u32>,
    run: &Run,
    range: Range<usize>,
    index: u32,
    family: &[FamilyChoice],
) {
    builder.push(StyleProperty::Brush(index), range.clone());
    builder.push(
        StyleProperty::FontFamily(FamilyChoice::stack(family)),
        range.clone(),
    );
    builder.push(StyleProperty::FontSize(run.size.max(0.5)), range.clone());
    builder.push(
        StyleProperty::FontWeight(parley::FontWeight::new(f32::from(run.weight))),
        range.clone(),
    );
    if run.italic {
        builder.push(
            StyleProperty::FontStyle(parley::FontStyle::Italic),
            range.clone(),
        );
    }
    // CSS `font-optical-sizing: auto` (CSS Fonts 4 §2.7): a face's `opsz` is
    // the size, as the browser and CoreText set it; one without the axis
    // ignores it, and `none` leaves the face's own. Parley applies it after
    // fontique's synthesis, and `font-variation-settings` after it, each axis
    // winning over the weight's, the width's and the size's (CSS Fonts 4
    // §7.2, LLP 1019 D9).
    let mut axes = Vec::new();
    if run.optical_sizing {
        axes.push(parley::FontVariation::new(
            parley::setting::Tag::new(b"opsz"),
            run.size,
        ));
    }
    axes.extend(run.variations.axes().iter().map(|(tag, value)| {
        parley::FontVariation::new(parley::setting::Tag::new(&tag.to_be_bytes()), *value)
    }));
    builder.push(
        StyleProperty::FontVariations(parley::FontVariations::List(axes.into())),
        range.clone(),
    );
    if run.stretch != 100.0 {
        builder.push(
            StyleProperty::FontWidth(parley::FontWidth::from_percentage(run.stretch)),
            range.clone(),
        );
    }
    if run.letter_spacing != 0.0 && run.size > 0.0 {
        builder.push(
            StyleProperty::LetterSpacing(run.letter_spacing),
            range.clone(),
        );
    }
    let features = run.features();
    if !features.is_empty() {
        builder.push(
            StyleProperty::FontFeatures(parley::FontFeatures::List(features.into())),
            range,
        );
    }
}

impl Run {
    /// OpenType features for painted text and hosted fields. Variants set
    /// the chosen face's features (LLP 1053 G4); explicit settings win for
    /// the same tag (CSS Fonts 4 §7.2, LLP 1019 D13).
    pub(crate) fn features(&self) -> Vec<parley::FontFeature> {
        let mut features = Vec::new();
        if self.font_variant_numeric & 1 != 0 {
            features.push(parley::FontFeature::new(
                parley::setting::Tag::new(b"tnum"),
                1,
            ));
        }
        for (tag, value) in ligature_features(self.font_variant_ligatures) {
            features.push(parley::FontFeature::new(
                parley::setting::Tag::new(tag),
                value,
            ));
        }
        for (tag, value) in self.font_features.features() {
            let tag = parley::setting::Tag::new(&tag.to_be_bytes());
            // The kernel keeps no value past 16 bits (`FontFeatures::MAX_VALUE`).
            let value = u16::try_from(*value).unwrap_or(u16::MAX);
            features.retain(|f: &parley::FontFeature| f.tag != tag);
            features.push(parley::FontFeature::new(tag, value));
        }
        features
    }
}

/// The OpenType features `font-variant-ligatures`' bits set, 1 on and 0
/// off: common ligatures are `liga` and `clig`, discretionary `dlig`,
/// historical `hlig`, contextual alternates `calt` (CSS Fonts 4).
fn ligature_features(bits: u8) -> Vec<(&'static [u8; 4], u16)> {
    use exact_kernel::FontVariantLigatures as L;
    let kinds: [(L, L, &[&'static [u8; 4]]); 4] = [
        (
            L::CommonLigatures,
            L::NoCommonLigatures,
            &[b"liga", b"clig"],
        ),
        (
            L::DiscretionaryLigatures,
            L::NoDiscretionaryLigatures,
            &[b"dlig"],
        ),
        (L::HistoricalLigatures, L::NoHistoricalLigatures, &[b"hlig"]),
        (L::Contextual, L::NoContextual, &[b"calt"]),
    ];
    let mut out = Vec::new();
    for (on, off, tags) in kinds {
        let value = if bits & on.keyword_bits() != 0 {
            1
        } else if bits & off.keyword_bits() != 0 {
            0
        } else {
            continue;
        };
        out.extend(tags.iter().map(|tag| (*tag, value)));
    }
    out
}

/// A hard line: its text and its one shaped layout.
#[derive(Clone)]
pub(super) struct HardLine {
    /// Byte offset in the canonical runs, after the preceding separator.
    pub(super) start: usize,
    /// The canonical run corresponding to brush zero in this line.
    pub(super) run_base: u32,
    pub(super) shape: Arc<LineShape>,
}

impl std::ops::Deref for HardLine {
    type Target = LineShape;
    fn deref(&self) -> &LineShape {
        &self.shape
    }
}

pub(super) struct LineShape {
    pub(super) text: String,
    pub(super) bytes: usize,
    layout: Mutex<Broken>,
    /// The resolved base direction (CSS `direction`, or the first strong
    /// character under `ltr`).
    pub(super) rtl: bool,
    /// `(first, rest)` inset, points: a Markdown list item's head indent,
    /// its first line's less the marker hung before it (LLP 1045 D4).
    inset: (f32, f32),
    /// The first visual line's indent as Parley applies it.
    indent: f32,
    /// The inline boxes' room beside its runs (`run_edges`), in logical
    /// order, and before its images' boxes.
    rooms: Vec<Room>,
    box_leads: Vec<(u32, f32)>,
}

/// A hard line's layout and the width, alignment and balancing it was last
/// broken and aligned at (`None` until a width breaks it): breaking it
/// again at the same ones leaves the same lines, so a width painted after it
/// was measured is not broken twice. `balance` is the last width's
/// [`ShapedSource::balanced`].
struct Broken {
    layout: parley::Layout<u32>,
    at: Option<(Option<u32>, bool, Alignment)>,
    balance: Option<(u32, Option<(Balanced, f32)>)>,
}

/// Where a balanced paragraph's first hard line breaks
/// ([`ShapedSource::balanced`]).
#[derive(Clone)]
enum Balanced {
    /// Its scorer's breaks: where each line before the first forced break
    /// ends, the last at that break or the text's end.
    Breaks(Vec<parley::BreakOpportunity>),
    /// Its bisection's width for the lines, the inset apart.
    Width(f32),
}

pub(super) struct ShapedSource {
    pub(super) spec: Arc<Spec>,
    pub(super) catalog: catalog::Lease,
    pub(super) data: Arc<ShapeData>,
    pub(super) accessible_capacity_bytes: usize,
    pub(super) flow: RefCell<Option<Rc<super::flow::FlowSource>>>,
}

/// Only shaped data and scalars cross the thread boundary, never the UI
/// lease. A width locks each layout while it breaks it.
pub(super) struct ShapeData {
    pub(super) lines: Vec<HardLine>,
    pub(super) strut: (f32, f32),
    /// The strut's font's ascent and descent: an empty line's text box.
    pub(super) strut_font: (f32, f32),
    pub(super) run_metrics: Vec<FontMetrics>,
}

mod hard_lines;
#[cfg(test)]
pub(super) use hard_lines::SPLIT_BYTES;
use hard_lines::{hard_lines, LineEdit, SplitLine};

/// A hard line's inline-box room (`Run::edges`), each room shaped as advance
/// on one cluster, which line breaking keeps it with: a run's start on the
/// cluster holding its first character, so that it goes to the next line
/// with its text wherever the line breaks before it, as Chrome keeps a
/// `<span>`'s padding with its text, and its end on the cluster holding its
/// last; and for an image, its start before its box, by run. Where the room
/// is drawn is the line's to decide once it is broken ([`lines::extract`]).
#[derive(Default)]
struct Edges {
    rooms: Vec<Room>,
    box_leads: Vec<(u32, f32)>,
}

fn run_edges(spec: &Spec, pieces: &[SplitLine<'_>]) -> Vec<Edges> {
    let mut out: Vec<Edges> = (0..pieces.len()).map(|_| Edges::default()).collect();
    if spec.runs.iter().all(|r| r.edges.is_empty()) {
        return out;
    }
    // Each run's first and last place: its hard line and its byte range there.
    let mut first: Vec<Option<(usize, usize)>> = vec![None; spec.runs.len()];
    let mut last: Vec<Option<(usize, usize)>> = vec![None; spec.runs.len()];
    for (h, (_, _, runs, _, empties)) in pieces.iter().enumerate() {
        for (range, index) in runs {
            first[*index].get_or_insert((h, range.start));
            last[*index] = Some((h, range.end));
        }
        for &(at, index) in empties {
            first[index] = Some((h, at));
        }
    }
    // A run's bytes in hard line `h`: the characters its room is drawn beside.
    let span = |h: usize, index: usize| {
        let mut owned = pieces[h].2.iter().filter(|(_, i)| *i == index);
        let lo = owned.next().map(|(r, _)| r.clone());
        lo.map(|lo| {
            let hi = owned.last().map_or(lo.end, |(r, _)| r.end);
            lo.start..hi
        })
    };
    for (index, run) in spec.runs.iter().enumerate() {
        if run.inline.is_some() {
            if run.edges.start != 0.0 {
                if let Some(h) = pieces.iter().position(|p| p.3.iter().any(|b| b.1 == index)) {
                    out[h].box_leads.push((index as u32, run.edges.start));
                }
            }
            continue;
        }
        if run.text.is_empty() {
            let room = run.edges.start + run.edges.end;
            if let Some((h, at)) = first[index].filter(|_| room != 0.0) {
                let text = &pieces[h].1;
                let (at, kind) = if at < text.len() {
                    (at, RoomKind::Before)
                } else if let Some((before, _)) = text[..at].char_indices().last() {
                    (before, RoomKind::After)
                } else {
                    continue;
                };
                out[h].rooms.push(Room {
                    at,
                    w: room,
                    run: index as u32,
                    kind,
                    span: at..at,
                });
            }
            continue;
        }
        if run.edges.start != 0.0 {
            if let Some((h, start)) = first[index] {
                if let Some(span) = span(h, index) {
                    out[h].rooms.push(Room {
                        at: start,
                        w: run.edges.start,
                        run: index as u32,
                        kind: RoomKind::Start,
                        span,
                    });
                }
            }
        }
        if run.edges.end != 0.0 {
            if let Some((h, end)) = last[index] {
                let at = pieces[h].1[..end].char_indices().last().map(|(at, _)| at);
                if let (Some(at), Some(span)) = (at, span(h, index)) {
                    out[h].rooms.push(Room {
                        at,
                        w: run.edges.end,
                        run: index as u32,
                        kind: RoomKind::End,
                        span,
                    });
                }
            }
        }
    }
    for edges in &mut out {
        // In logical order: by place, then by run (a stable sort).
        edges.rooms.sort_by_key(|r| r.at);
    }
    out
}

impl ShapedSource {
    pub(super) fn flow_box<'a>(
        &self,
        glyphs: impl Iterator<Item = (&'a LayoutGlyph, usize)>,
        faces: &[lines::Face],
    ) -> (f32, f32) {
        let (above, below, _) = line_box(
            glyphs,
            faces,
            &self.spec,
            &mut self.catalog.borrow_mut(),
            self.data.strut,
            &self.data.run_metrics,
        );
        (above, above + below)
    }
    pub(super) fn flow_capacity_bytes(&self) -> usize {
        self.flow
            .borrow()
            .as_ref()
            .map_or(0, |f| f.capacity_bytes())
    }

    pub(super) fn new(lease: catalog::Lease, spec: Arc<Spec>) -> Self {
        Self::replacing(lease, spec, None)
    }

    pub(super) fn replacing(
        lease: catalog::Lease,
        spec: Arc<Spec>,
        old: Option<&ShapedSource>,
    ) -> Self {
        let mut catalog = lease.borrow_mut();
        let minimum = catalog.line_height(&spec.strut);
        let (ascent, descent, leading) = catalog.font_metrics(&spec.strut);
        let half = (minimum - ascent - descent - leading) / 2.0;
        let strut = (ascent + half, descent + leading + half);
        let run_metrics: Vec<_> = spec.runs.iter().map(|r| catalog.font_metrics(r)).collect();
        let families: Vec<Vec<FamilyChoice>> =
            spec.runs.iter().map(|r| catalog.choice(r.family)).collect();
        let wrap = if spec.white_space.model().wraps() {
            parley::TextWrapMode::Wrap
        } else {
            parley::TextWrapMode::NoWrap
        };
        let overflow_wrap = match spec.overflow_wrap {
            exact_kernel::OverflowWrap::Normal => parley::OverflowWrap::Normal,
            exact_kernel::OverflowWrap::BreakWord => parley::OverflowWrap::BreakWord,
            exact_kernel::OverflowWrap::Anywhere => parley::OverflowWrap::Anywhere,
        };
        // `direction: rtl` makes the base direction right-to-left, as CSS
        // does (LLP 1053; LLP 1085.000 G1). Under `ltr` the first strong
        // character still decides (declared in LLP 1001 §1): the text-flow
        // walker cannot yet break an RTL run inside an LTR paragraph.
        let base = if spec.direction == exact_kernel::Direction::Rtl {
            BaseDirection::Rtl
        } else {
            BaseDirection::Auto
        };
        let mut text_bytes = 0;
        let mut end = 0;
        let prior_runs: Vec<_> = old
            .into_iter()
            .flat_map(|s| &s.spec.runs)
            .map(|run| {
                let start = end;
                end += run.text.len();
                (start..end, run)
            })
            .collect();
        let edit = LineEdit::between(&spec, old);
        let mut pieces = hard_lines(&spec, edit.bytes.clone());
        if !edit.suffix.is_empty() && pieces.last().is_some_and(|line| line.0 == edit.bytes.end) {
            pieces.pop();
        }
        let count = edit.prefix.len() + pieces.len() + edit.suffix.len();
        let mut lines = Vec::with_capacity(count);
        lines.extend_from_slice(edit.prefix);
        let mut repeated = HashMap::new();
        let edges = run_edges(&spec, &pieces);
        for ((start, text, runs, boxes, _), edges) in pieces.into_iter().zip(edges) {
            let h = lines.len();
            let run_base = runs.first().map_or(0, |(_, index)| *index as u32);
            // A line within one run borrows its bytes from the source. Its run selects
            // the font and shaping style. A box, edge or first-line treatment is local.
            let first_only = h == 0
                && (spec.text_indent != 0.
                    || spec.text_wrap_style == exact_kernel::TextWrapStyle::Balance);
            if count > 1
                && !first_only
                && boxes.is_empty()
                && edges.rooms.is_empty()
                && edges.box_leads.is_empty()
            {
                if let std::borrow::Cow::Borrowed(bytes) = text {
                    let prior = *repeated.entry((run_base, bytes)).or_insert(h);
                    if prior != h {
                        lines.push(HardLine {
                            start,
                            run_base,
                            shape: lines[prior].shape.clone(),
                        });
                        continue;
                    }
                }
            }
            if let Some(line) = old.and_then(|s| {
                s.matching_line(h, count, |i, line| {
                    text == line.text
                        && runs.iter().all(|(range, index)| {
                            let prior = line.run_base as usize + (*index - run_base as usize);
                            prior_runs.get(prior).is_some_and(|(bounds, before)| {
                                bounds.start <= line.start + range.start
                                    && line.start + range.end <= bounds.end
                                    && cache::same_run_style(before, &spec.runs[*index])
                            })
                        })
                        && (h == 0) == (i == 0)
                        && !(h == 0 && spec.text_wrap_style == exact_kernel::TextWrapStyle::Balance)
                })
            }) {
                lines.push(HardLine {
                    start,
                    run_base,
                    shape: line.1.shape.clone(),
                });
                continue;
            }
            #[cfg(test)]
            SHAPE_LINES.with(|n| n.set(n.get() + 1));
            text_bytes += text.len();
            let catalog = &mut *catalog;
            let mut builder = catalog
                .layout
                .ranged_builder(&mut catalog.fonts, &text, 1.0, false);
            builder.push_default(StyleProperty::TextWrapMode(wrap));
            builder.push_default(StyleProperty::OverflowWrap(overflow_wrap));
            if let Some((_, first)) = runs.first() {
                // An empty hard line still has its run's face for metrics.
                builder.push_default(StyleProperty::FontSize(spec.runs[*first].size.max(0.5)));
            }
            for (range, index) in &runs {
                push_style(
                    &mut builder,
                    &spec.runs[*index],
                    range.clone(),
                    *index as u32 - run_base,
                    &families[*index],
                );
            }
            // An inline box's room separates the characters either side of
            // it: no ligature or kerning joins them across it, as in Chrome.
            for (range, index) in &runs {
                if !spec.runs[*index].edges.is_empty() {
                    builder.push_shaping_break(range.start);
                    builder.push_shaping_break(range.end);
                }
            }
            for &(index, run) in &boxes {
                let b = spec.runs[run].inline.expect("a box's run");
                builder.push_inline_box(parley::InlineBox {
                    id: run as u64,
                    kind: parley::InlineBoxKind::InFlow,
                    index,
                    // The room its boxes take beside it: its width reaches
                    // past the image, which is drawn past the start's.
                    width: b.width.max(0.0) + spec.runs[run].edges.start + spec.runs[run].edges.end,
                    height: b.height.max(0.0),
                });
            }
            builder.set_base_direction(base);
            // The line box keeps the CSS direction even where the first
            // strong character set the bidi base: trailing whitespace hangs
            // at its end edge, and an overflowing line starts at its start.
            builder.set_line_direction(if base == BaseDirection::Rtl {
                BaseDirection::Rtl
            } else {
                BaseDirection::Ltr
            });
            builder.set_line_break_override(Some(CHROME));
            let mut layout = builder.build(&text);
            let Edges { rooms, box_leads } = edges;
            for room in &rooms {
                layout.add_lead(room.at, room.w);
            }
            let inset = line_inset(&spec, &runs, &mut layout);
            // CSS `text-indent`: the paragraph's first line only.
            let indent = if h == 0 { spec.text_indent } else { 0.0 } + inset.0 - inset.1;
            if indent != 0.0 {
                layout.set_text_indent(
                    indent,
                    IndentOptions {
                        each_line: false,
                        hanging: false,
                    },
                );
            }
            let text = text.into_owned();
            lines.push(HardLine {
                start,
                run_base,
                shape: Arc::new(LineShape {
                    bytes: text.capacity() + layout.capacity_bytes(),
                    text,
                    rtl: layout.is_rtl(),
                    layout: Mutex::new(Broken {
                        layout,
                        at: None,
                        balance: None,
                    }),
                    inset,
                    indent,
                    rooms,
                    box_leads,
                }),
            });
        }
        if let Some(first) = edit.suffix.first() {
            lines.extend(edit.suffix.iter().map(|line| HardLine {
                start: edit.bytes.end + (line.start - first.start),
                run_base: line.run_base,
                shape: line.shape.clone(),
            }));
        }
        catalog.release_scratch(text_bytes);
        drop(catalog);
        let mut source = Self {
            spec,
            catalog: lease,
            data: Arc::new(ShapeData {
                lines,
                strut,
                strut_font: (ascent, descent),
                run_metrics,
            }),
            accessible_capacity_bytes: 0,
            flow: RefCell::new(None),
        };
        source.accessible_capacity_bytes = source.capacities();
        source
    }
    pub(super) fn attach(
        catalog: catalog::Lease,
        spec: Arc<Spec>,
        data: Arc<ShapeData>,
        bytes: usize,
    ) -> Self {
        Self {
            spec,
            catalog,
            data,
            accessible_capacity_bytes: bytes,
            flow: RefCell::new(None),
        }
    }
    fn matching_line(
        &self,
        h: usize,
        count: usize,
        matches: impl Fn(usize, &HardLine) -> bool,
    ) -> Option<(usize, &HardLine)> {
        let lines = &self.data.lines;
        [Some(h), lines.len().checked_sub(count - h)]
            .into_iter()
            .flatten()
            .find_map(|i| {
                lines
                    .get(i)
                    .filter(|line| matches(i, line))
                    .map(|line| (i, line))
            })
    }

    /// The shape's own storage: the hard lines' text and Parley's arrays.
    fn capacities(&self) -> usize {
        fn vec<T>(v: &Vec<T>) -> usize {
            v.capacity() * std::mem::size_of::<T>()
        }
        let mut bytes = vec(&self.data.lines) + vec(&self.data.run_metrics);
        let mut shared = std::collections::HashSet::new();
        for line in &self.data.lines {
            if Arc::strong_count(&line.shape) == 1 || shared.insert(Arc::as_ptr(&line.shape)) {
                bytes += line.bytes + std::mem::size_of::<LineShape>();
            }
        }
        bytes
    }
    pub(super) fn layout(self: &Rc<Self>, width: Option<f32>) -> Paragraph {
        self.layout_as(width, false, WidthUse::Measure)
    }
    /// CSS `text-overflow: ellipsis`: each over-wide line ends in "…" (paint).
    pub(super) fn layout_ellipsized(self: &Rc<Self>, width: f32) -> Paragraph {
        self.layout_as(Some(width), true, WidthUse::Measure)
    }
    /// The paragraph's widest content, min- or max-content (LLP 1085.000
    /// §4, "Content widths"): min-content is Parley's patched content
    /// widths (soft hyphens, text-indent and RTL hanging space included),
    /// which agree with Chrome where the width-zero layout did not (a URL's
    /// `/`); max-content stays the unbroken layout's widest line, which
    /// keeps a preserved trailing space as Chrome's `pre-wrap` does.
    pub(super) fn min_content(&self) -> f32 {
        if !self.spec.white_space.model().wraps() {
            return self.layout_widths(None);
        }
        self.data
            .lines
            .iter()
            .map(|line| {
                let broken = line.layout.lock().expect("layout lock");
                broken.layout.calculate_content_widths().min + line.inset.1
            })
            .fold(0.0, f32::max)
    }
    fn layout_widths(&self, width: Option<f32>) -> f32 {
        let (lines, _) = self.break_lines(width, false, None, None, false);
        lines.lines.iter().map(|l| l.w).fold(0.0, f32::max)
    }
    /// A measured width's lines, made again for its first paint when the
    /// measure's were not held: the same breaking as when it was measured
    /// (an unclamped paragraph's lines need no catalog), so what was
    /// measured is what is painted. A layout still broken at this width is
    /// not broken again.
    pub(super) fn remake(&self, width: Option<f32>) -> Lines {
        self.break_lines(width, false, None, None, false).0
    }
    /// Break every hard line at `width` and copy its lines out, clamped and
    /// ellipsized as the paragraph asks. Returns the lines and whether a
    /// clamp cut text.
    fn break_lines(
        &self,
        width: Option<f32>,
        ellipsis: bool,
        mut catalog: Option<&mut catalog::Catalog>,
        previous: Option<&Paragraph>,
        field: bool,
    ) -> (Lines, bool) {
        let spec = &self.spec;
        let preserves = spec.white_space.model().preserves();
        let rtl = spec.direction == exact_kernel::Direction::Rtl;
        let align = self.alignment();
        let mut out = Lines::default();
        let mut left = (spec.line_clamp > 0).then_some(spec.line_clamp as usize);
        let mut cut = false;
        let count = self.data.lines.len();
        let balanced = self.balanced(width).map(|(balanced, _)| balanced);
        #[cfg(test)]
        BREAKS.with(|n| n.set((n.get().0 + 1, n.get().1)));
        let mut field = field.then(lines::FieldLines::default);
        let mut prior_visual = 0;
        let mut faces = Vec::new();
        'hard: for (h, hard) in self.data.lines.iter().enumerate() {
            if let Some((field, (old, i))) = field.as_mut().zip(previous.and_then(|p| {
                let (i, _) = p
                    .source
                    .matching_line(h, count, |_, line| Arc::ptr_eq(&line.shape, &hard.shape))?;
                Some((p.lines(), i))
            })) {
                field.reuse(&mut out, old, i, (h, hard), &mut prior_visual, &mut faces);
                continue;
            }
            let first_visual = out.lines.len();
            let (_, rest) = hard.inset;
            let mut broken = hard.layout.lock().expect("layout lock");
            let balanced = balanced.as_ref().filter(|_| h == 0);
            let at = Some((width.map(f32::to_bits), balanced.is_some(), align));
            if broken.at != at {
                #[cfg(test)]
                BREAKS.with(|n| n.set((n.get().0, n.get().1 + 1)));
                broken.at = None;
                if let (Some(balanced), Some(width)) = (balanced, width) {
                    // Each line is aligned (and justified) in the box's
                    // width wherever it breaks, as Chrome's are.
                    let room = (width - rest).max(0.0);
                    let mut breaker = broken.layout.break_lines();
                    match balanced {
                        Balanced::Breaks(ends) => {
                            let state = breaker.state_mut();
                            state.set_layout_max_advance(room);
                            state.set_line_max_advance(room);
                            for end in ends {
                                breaker.break_next_at(end);
                            }
                            // Past a forced break the lines are greedy.
                            while breaker.break_next().is_some() {}
                        }
                        Balanced::Width(narrow) => {
                            let state = breaker.state_mut();
                            state.set_layout_max_advance(*narrow);
                            state.set_line_max_advance(*narrow);
                            while breaker.break_next().is_some() {
                                breaker.set_prior_line_width(room);
                            }
                        }
                    }
                    breaker.finish();
                } else {
                    broken
                        .layout
                        .break_all_lines(width.map(|w| (w - rest).max(0.0)));
                }
                broken.layout.align(align, AlignmentOptions::default());
                broken.at = at;
            }
            let layout = &broken.layout;
            // From the start edge: `rtl` lays out from the right.
            let shift = if rtl { 0.0 } else { rest };
            let n = layout.len();
            let styles = layout.styles();
            for (i, line) in layout.lines().enumerate() {
                let start = out.glyphs.len() as u32;
                let content = lines::extract(
                    &line,
                    styles,
                    shift,
                    lines::Rooms {
                        rooms: &hard.rooms,
                        boxes: &hard.box_leads,
                        rtl,
                    },
                    &mut out,
                );
                let m = line.metrics();
                // A collapsible space that ends a line hangs; a preserved
                // one counts where the hard line ends (`pre`, `pre-wrap`).
                let ends_hard = i + 1 == n;
                let trailing = if preserves && ends_hard {
                    0.0
                } else {
                    m.trailing_whitespace
                };
                let indent = if i == 0 { hard.indent } else { 0.0 };
                out.lines.push(LayoutLine {
                    hard: h as u32,
                    run_base: hard.run_base,
                    glyphs: (start, out.glyphs.len() as u32),
                    w: (content - trailing + indent).max(0.0) + rest,
                    ..LayoutLine::default()
                });
                if ellipsis {
                    if let (Some(w), Some(catalog)) = (
                        // Overflow as Blink decides fit: past one 1/64 px unit.
                        width.filter(|w| out.lines.last().is_some_and(|l| l.w > w + 1.0 / 64.0)),
                        catalog.as_deref_mut(),
                    ) {
                        self.ellipsize(&mut out, w, catalog);
                    }
                }
                if let Some(k) = left.as_mut() {
                    *k -= 1;
                    if *k == 0 {
                        cut = i + 1 < n || h + 1 < count;
                        if let Some(catalog) = catalog.as_deref_mut().filter(|_| cut) {
                            self.ellipsize(&mut out, width.unwrap_or(f32::INFINITY), catalog);
                        }
                        break 'hard;
                    }
                }
            }
            if let Some(field) = &mut field {
                field.extracted(&mut out, first_visual);
            }
        }
        out.tighten();
        if let Some(field) = field {
            out.glyphs = field.finish();
        }
        (out, cut)
    }

    /// CSS `text-wrap-style: balance` (LLP 1001 §6) as Blink does it: where
    /// the lines before the first forced break end (Chrome balances only
    /// those), and those lines' widest broken greedily, which the paragraph
    /// still measures: balancing moves breaks, never the box. A forced break
    /// is a separator `hard_lines` splits at or a newline Parley breaks at
    /// inside the first hard line (U+2028). `None` keeps the greedy lines: a
    /// clamp, one line or more than six, a line that overflows the box, or
    /// neither Blink's scorer nor its bisection moving a break. A tab in
    /// those lines keeps the scorer out, and a forced break the bisection,
    /// as in Blink.
    fn balanced(&self, width: Option<f32>) -> Option<(Balanced, f32)> {
        let spec = &self.spec;
        if spec.text_wrap_style != exact_kernel::TextWrapStyle::Balance
            || spec.line_clamp > 0
            || !spec.white_space.model().wraps()
        {
            return None;
        }
        let width = width?;
        let hard = self.data.lines.first()?;
        let (_, rest) = hard.inset;
        let mut broken = hard.layout.lock().expect("layout lock");
        if let Some((bits, balanced)) = &broken.balance {
            if *bits == width.to_bits() {
                return balanced.clone();
            }
        }
        broken.at = None;
        let room = (width - rest).max(0.0);
        // The greedy lines in `room` before the first forced break: their
        // count, their widths' sum and widest, whether one overflows (the
        // breaker fits a line under 1/32 px over its room, `fit_limit`) or
        // breaks inside a word (`overflow-wrap`), and whether that break
        // ends them.
        let greedy = |layout: &mut parley::Layout<u32>, room: f32| {
            layout.break_all_lines(Some(room));
            let (mut count, mut total, mut widest) = (0, 0.0, 0.0f32);
            let (mut overflows, mut forced) = (false, false);
            for (i, line) in layout.lines().enumerate() {
                let m = line.metrics();
                let indent = if i == 0 { hard.indent } else { 0.0 };
                let w = m.advance - m.trailing_whitespace + indent;
                count += 1;
                total += w;
                widest = widest.max(w);
                let reason = line.break_reason();
                overflows |= w > room + 1.0 / 32.0 || reason == parley::BreakReason::Emergency;
                if reason == parley::BreakReason::Explicit {
                    forced = true;
                    break;
                }
            }
            (count, total, widest, overflows, forced)
        };
        let (count, total, widest, overflows, forced) = greedy(&mut broken.layout, room);
        // `hard_lines` keeps no separator, so with one, a trailing one
        // too, the first hard line's text is shorter than the runs'.
        let source = spec.runs.iter().filter(|r| r.inline.is_none());
        let forced = forced || source.map(|r| r.text.len()).sum::<usize>() != hard.text.len();
        let scored = || {
            let opportunities = broken.layout.break_lines().opportunities();
            let places: Vec<_> = opportunities
                .iter()
                .map(|o| (o.end, o.next, o.hyphen))
                .collect();
            let breaks = exact_textflow::balanced_breaks(
                room,
                hard.indent,
                spec.strut.size,
                self.alignment() == Alignment::Justify,
                &places,
                count,
            )?;
            let ends = breaks.iter().map(|&at| opportunities[at]);
            Some(ends.chain(opportunities.last().copied()).collect())
        };
        // The lines to balance end at the hard line's first U+2028.
        let before = hard.text.split('\u{2028}').next().unwrap_or("");
        let tab = before.contains('\t');
        let balanced = if overflows || !(2..=exact_textflow::BALANCE_MAX_LINES).contains(&count) {
            None
        } else if let Some(ends) = (!tab).then(scored).flatten() {
            Some(Balanced::Breaks(ends))
        } else if !forced {
            exact_textflow::balanced_width(room, count, total, |w| {
                let (lines, .., overflows, _) = greedy(&mut broken.layout, w);
                lines <= count && !overflows
            })
            .map(Balanced::Width)
        } else {
            None
        };
        let balanced = balanced.map(|balanced| (balanced, widest + rest));
        broken.balance = Some((width.to_bits(), balanced.clone()));
        balanced
    }

    /// The paragraph's alignment as Parley takes it.
    fn alignment(&self) -> Alignment {
        match self.spec.align.physical(self.spec.direction) {
            // Physical: Parley's `Start` would align by each line's own bidi
            // direction, not the paragraph's CSS `direction` (LLP 1053).
            TextAlign::Left | TextAlign::Start => Alignment::Left,
            TextAlign::Center => Alignment::Center,
            TextAlign::Right | TextAlign::End => Alignment::Right,
            TextAlign::Justify => Alignment::Justify,
        }
    }

    /// End the last line in "…" in the run's own face, as Blink's line
    /// truncator does: the line keeps its alignment, whole visual clusters
    /// stay from its start edge (the CSS direction's) while they and the
    /// ellipsis fit `width` measured from where the aligned line starts,
    /// spaces included, and the ellipsis follows the last one kept. A
    /// centred or end-aligned line's ellipsis can so pass the box's end
    /// edge, where Chrome clips it too.
    fn ellipsize(&self, out: &mut Lines, width: f32, catalog: &mut catalog::Catalog) {
        let Some(line) = out.lines.last().copied() else {
            return;
        };
        let rtl = self.spec.direction == exact_kernel::Direction::Rtl;
        // How far alignment moved the line from its start edge: Parley
        // aligns a line that fits by its free space.
        let free = if width.is_finite() {
            (width - line.w).max(0.0)
        } else {
            0.0
        };
        let shift = match (self.alignment(), rtl) {
            (Alignment::Center, _) => free / 2.0,
            (Alignment::Right, false) | (Alignment::Left, true) => free,
            _ => 0.0,
        };
        let (a, b) = (line.glyphs.0 as usize, line.glyphs.1 as usize);
        let glyphs: Vec<LayoutGlyph> = out.glyphs[a..b].to_vec();
        let end = if rtl { glyphs.first() } else { glyphs.last() };
        let brush = end.map_or(0, |g| g.metadata);
        let run = (line.run_base + brush) as usize;
        let Some(spec_run) = self.spec.runs.get(run) else {
            return;
        };
        let family = catalog.choice(spec_run.family);
        let symbol = catalog.shape_one(spec_run, "\u{2026}", &family);
        let mut ink = Lines::default();
        let mut advance = 0.0;
        if let Some(l) = symbol.lines().next() {
            advance = lines::extract(&l, symbol.styles(), 0.0, lines::Rooms::NONE, &mut ink);
        }
        // Visual clusters, kept from the line's start edge.
        let extent = |g: &LayoutGlyph| (g.x, g.x + g.w);
        let order: Vec<usize> = if rtl {
            (0..glyphs.len()).rev().collect()
        } else {
            (0..glyphs.len()).collect()
        };
        let mut kept: Vec<usize> = Vec::new();
        let mut i = 0;
        while i < order.len() {
            let range = glyphs[order[i]].range();
            let mut j = i;
            while j < order.len() && glyphs[order[j]].range() == range {
                j += 1;
            }
            let cluster = &order[i..j];
            let fits = cluster.iter().all(|&k| {
                let (lo, hi) = extent(&glyphs[k]);
                if rtl {
                    lo + shift >= advance - 0.01
                } else {
                    hi - shift <= width - advance + 0.01
                }
            });
            if !fits {
                break;
            }
            kept.extend_from_slice(cluster);
            i = j;
        }
        let original = glyphs
            .iter()
            .map(extent)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |a, e| {
                (a.0.min(e.0), a.1.max(e.1))
            });
        let edge = if rtl {
            kept.iter()
                .map(|&k| glyphs[k].x)
                .reduce(f32::min)
                .unwrap_or(if original.1.is_finite() {
                    original.1
                } else {
                    0.0
                })
        } else {
            kept.iter()
                .map(|&k| glyphs[k].x + glyphs[k].w)
                .reduce(f32::max)
                .unwrap_or(if original.0.is_finite() {
                    original.0
                } else {
                    0.0
                })
        };
        let pen = if rtl { edge - advance } else { edge };
        let boundary = end.map_or(0, |g| if rtl { g.start } else { g.end });
        let mut next: Vec<LayoutGlyph> = Vec::with_capacity(kept.len() + ink.glyphs.len());
        let mut kept_sorted = kept.clone();
        kept_sorted.sort_unstable();
        let mut symbol_glyphs: Vec<LayoutGlyph> = ink
            .glyphs
            .iter()
            .map(|g| {
                let face = ink.faces[g.face as usize].clone();
                LayoutGlyph {
                    x: g.x + pen,
                    start: boundary,
                    end: boundary,
                    metadata: brush,
                    face: out.face(face),
                    ..*g
                }
            })
            .collect();
        if rtl {
            next.append(&mut symbol_glyphs);
            next.extend(kept_sorted.iter().map(|&k| glyphs[k]));
        } else {
            next.extend(kept_sorted.iter().map(|&k| glyphs[k]));
            next.append(&mut symbol_glyphs);
        }
        let new_extent = next
            .iter()
            .map(extent)
            .fold((f32::INFINITY, f32::NEG_INFINITY), |a, e| {
                (a.0.min(e.0), a.1.max(e.1))
            });
        let w = if next.is_empty() || !original.0.is_finite() {
            advance
        } else {
            (line.w - (original.1 - original.0) + (new_extent.1 - new_extent.0)).max(0.0)
        };
        // An image past the ellipsis's edge goes with the glyphs it cut: its
        // box answers no place (LLP 1011.002).
        let line_index = out.lines.len() as u32 - 1;
        let spec = &self.spec;
        out.boxes.retain(|b| {
            let width = spec
                .runs
                .get(b.run as usize)
                .and_then(|r| r.inline)
                .map_or(0.0, |i| i.width);
            b.line != line_index
                || if rtl {
                    b.x >= pen + advance
                } else {
                    b.x + width <= pen
                }
        });
        out.glyphs.truncate(a);
        out.glyphs.extend(next);
        let last = out.lines.last_mut().expect("a line");
        last.glyphs.1 = out.glyphs.len() as u32;
        last.w = w;
    }

    pub(super) fn layout_as(
        self: &Rc<Self>,
        width: Option<f32>,
        ellipsis: bool,
        usage: WidthUse,
    ) -> Paragraph {
        let spec = &self.spec;
        let mut catalog = self.catalog.borrow_mut();
        let hold = !matches!(usage, WidthUse::Measure);
        let previous = usage.previous().filter(|old| {
            old.remake == Some(width)
                && old.flow.is_none()
                && (old.record.get().is_some() || old.measured.borrow().is_some())
        });
        let keep_field_metrics = matches!(usage, WidthUse::Field(_))
            && self.data.lines.len() > 1
            && width.is_some()
            && spec.line_clamp == 0
            && !ellipsis
            && super::field::text_runs(spec);
        let (mut lines, _) = self.break_lines(
            width,
            ellipsis,
            Some(&mut catalog),
            previous,
            keep_field_metrics,
        );
        let strut = self.data.strut;
        let run_metrics = &self.data.run_metrics;
        let mut w = 0.0f32;
        let mut h = 0.0f32;
        let mut baselines = Vec::with_capacity(lines.lines.len());
        let mut bottoms = Vec::with_capacity(lines.lines.len());
        let mut explicit = false;
        if let Some((_, widest)) = self.balanced(width) {
            w = w.max(widest);
        }
        if keep_field_metrics {
            lines.field_metrics.resize(lines.lines.len(), None);
        }
        for (i, line) in lines.lines.iter().enumerate() {
            w = w.max(line.w);
            let (mut above, mut below, is_explicit) = lines
                .field_metrics
                .get(i)
                .copied()
                .flatten()
                .unwrap_or_else(|| {
                    line_box(
                        lines
                            .glyphs_of(line)
                            .iter()
                            .map(|g| (g, g.run(line.run_base))),
                        &lines.faces,
                        spec,
                        &mut catalog,
                        strut,
                        run_metrics,
                    )
                });
            if keep_field_metrics {
                lines.field_metrics[i] = Some((above, below, is_explicit));
            }
            // CSS 2 §10.8: an image's margin box grows the line to hold it.
            for b in lines.boxes.iter().filter(|b| b.line as usize == i) {
                let (a, d) = box_extent(spec, &mut catalog, b.run as usize);
                above = above.max(a);
                below = below.max(d);
            }
            explicit |= is_explicit;
            baselines.push(h + above);
            h += above + below;
            bottoms.push(h);
        }
        let mut height = if explicit { h } else { h.ceil() };
        let cut = text_box_trim(spec, &lines, &mut catalog, &baselines, height, 0.0);
        trim_lines(cut, &mut height, &mut baselines, &mut bottoms);
        drop(catalog);
        // A clamped or ellipsized width keeps its lines (its ellipsis was
        // shaped for it); any other keeps only its scalars until painted,
        // and its lines only until this frame's paint when held.
        let keep = ellipsis || spec.line_clamp > 0;
        let (record, measured) = match (keep, hold) {
            (true, _) => (std::cell::OnceCell::from(Arc::new(lines)), None),
            (false, true) => (std::cell::OnceCell::new(), Some(Arc::new(lines))),
            (false, false) => (std::cell::OnceCell::new(), None),
        };
        let mut paragraph = Paragraph {
            source: self.clone(),
            record,
            measured: RefCell::new(measured),
            remake: (!keep).then_some(width),
            flow: None,
            #[cfg(test)]
            layout_lifetime: Arc::new(()),
            width: w.ceil(),
            height,
            first_baseline: baselines.first().copied().unwrap_or(0.),
            baselines: Arc::new(baselines),
            bottoms: Arc::new(bottoms),
            ink: RefCell::new(ink::Cache::default()),
            ellipsized: RefCell::new(None),
            strokes: RefCell::new(super::stroke::Cache::default()),
            resident_capacity_bytes: 0,
            private_text_bytes_estimate: 0,
        };
        paragraph.resident_capacity_bytes = cache::capacities(&paragraph);
        paragraph
    }
}

/// Each hard line's inset: a line takes the indent of the run its first
/// character is in, and when that run is a hung marker, the marker's
/// shaped advance comes off the first line's.
fn line_inset(
    spec: &Spec,
    runs: &[(Range<usize>, usize)],
    layout: &mut parley::Layout<u32>,
) -> (f32, f32) {
    let Some(&(_, index)) = runs.first() else {
        return (0.0, 0.0);
    };
    let run = &spec.runs[index];
    if run.indent == 0.0 {
        return (0.0, 0.0);
    }
    let marker: f32 = if run.hang {
        layout.break_all_lines(None);
        let mut sum = 0.0;
        for line in layout.lines() {
            for r in line.runs() {
                for c in r.visual_clusters() {
                    if c.first_style().brush == 0 {
                        sum += c.advance();
                    }
                }
            }
        }
        sum
    } else {
        0.0
    };
    (run.indent - marker, run.indent)
}

/// How far an inline box's run reaches above and below the baseline, by
/// its `vertical-align` against the run's own font (LLP 1011.002 D3).
pub(super) fn box_extent(spec: &Spec, catalog: &mut catalog::Catalog, run: usize) -> (f32, f32) {
    let Some(b) = spec.runs.get(run).and_then(|r| r.inline) else {
        return (0.0, 0.0);
    };
    let [ascent, _, x_height, descent] = catalog.edge_metrics(&spec.runs[run]);
    let above = b.ascent(exact_kernel::FontEdges {
        ascent,
        descent,
        x_height,
    });
    (above, b.height - above)
}

/// A line's box (CSS 2 §10.8): the strut, grown by each glyph's own face
/// where its run's `line-height` is normal, or by its run's authored box.
/// Returns the space above and below the baseline, and whether an explicit
/// length decided either.
pub(super) fn line_box<'a>(
    glyphs: impl Iterator<Item = (&'a LayoutGlyph, usize)>,
    faces: &[lines::Face],
    spec: &Spec,
    catalog: &mut catalog::Catalog,
    strut: (f32, f32),
    run_metrics: &[FontMetrics],
) -> (f32, f32, bool) {
    let (mut above, mut below) = strut;
    let mut above_explicit = spec.strut.line_height.is_some();
    let mut below_explicit = above_explicit;
    let mut last = None;
    for (glyph, run) in glyphs {
        let key = (glyph.face, glyph.font_size.to_bits(), run);
        if last == Some(key) {
            continue;
        }
        last = Some(key);
        let Some(face) = faces.get(glyph.face as usize) else {
            continue;
        };
        let Some(m) = catalog.raw_metrics(&face.font) else {
            continue;
        };
        if m.units_per_em == 0 {
            continue;
        }
        let scale = glyph.font_size / m.units_per_em as f32;
        // Explicit lengths size the authored inline box; only normal
        // expands to the actual fallback glyph font.
        let (ascent, descent, leading) = if spec.runs[run].line_height.is_some() {
            run_metrics[run]
        } else {
            (m.ascent * scale, m.descent.abs() * scale, m.leading * scale)
        };
        let height = spec.runs[run]
            .line_height
            .unwrap_or(ascent + descent + leading);
        let half = (height - ascent - descent - leading) / 2.0;
        let run_explicit = spec.runs[run].line_height.is_some();
        let (a, b) = (ascent + half, descent + leading + half);
        if a > above {
            above = a;
            above_explicit = run_explicit;
        } else if a == above {
            above_explicit &= run_explicit;
        }
        if b > below {
            below = b;
            below_explicit = run_explicit;
        } else if b == below {
            below_explicit &= run_explicit;
        }
    }
    (above, below, above_explicit || below_explicit)
}

/// One visual line as painters and tests read it.
#[derive(Clone, Copy, Debug)]
pub struct LayoutRun<'a> {
    /// The first canonical glyph run of this hard line.
    pub run_base: u32,
    /// Its hard line's index.
    pub line_i: usize,
    /// The source text indexed by its glyph ranges: a hard line for ordinary
    /// layout, the whole paragraph for flow across collapsed hard breaks.
    pub text: &'a str,
    /// The hard line's base direction.
    pub rtl: bool,
    /// Its glyphs, in visual order.
    pub glyphs: &'a [LayoutGlyph],
    /// Its baseline, points from the paragraph's top.
    pub line_y: f32,
    /// Its box's top.
    pub line_top: f32,
    /// Its box's height.
    pub line_height: f32,
    /// Its width.
    pub line_w: f32,
    /// Its inline boxes' room beside their text, as it draws them.
    pub rooms: &'a [lines::LayoutRoom],
}

pub(super) struct Runs<'a> {
    paragraph: &'a Paragraph,
    index: usize,
}
impl<'a> Runs<'a> {
    pub(super) fn new(paragraph: &'a Paragraph) -> Self {
        Self {
            paragraph,
            index: 0,
        }
    }
}
impl<'a> Iterator for Runs<'a> {
    type Item = LayoutRun<'a>;
    fn next(&mut self) -> Option<Self::Item> {
        let p = self.paragraph;
        let record = p.layouts();
        let line = record.lines.get(self.index)?;
        let i = self.index;
        self.index += 1;
        let hard = &p.source.data.lines[line.hard as usize];
        let (line_top, line_y, line_height) =
            if let Some(flow) = p.flow.as_ref().filter(|f| !f.incomplete) {
                (flow.fragments[i].y, p.baselines[i], flow.line_height)
            } else {
                let top = if i == 0 { 0.0 } else { p.bottoms[i - 1] };
                (top, p.baselines[i], p.bottoms[i] - top)
            };
        Some(LayoutRun {
            run_base: line.run_base,
            line_i: line.hard as usize,
            text: p
                .flow
                .as_ref()
                .filter(|f| !f.incomplete)
                .map_or(&hard.text, |f| &f.text),
            rtl: hard.rtl,
            glyphs: record.glyphs_of(line),
            line_y,
            line_top,
            line_height,
            line_w: line.w,
            rooms: {
                let from = record.rooms.partition_point(|r| (r.line as usize) < i);
                let to = record.rooms.partition_point(|r| (r.line as usize) <= i);
                &record.rooms[from..to]
            },
        })
    }
}

/// CSS `text-box-trim` (Inline Layout 3 §4.2): how much comes off a laid-out
/// paragraph's top and bottom, from the first line's baseline up to the
/// root inline box's ascent, cap height or x-height, and from the last
/// line's baseline down to its descent or nothing (the paragraph's own font,
/// as on Apple and in CSS). `first_top` is where the first line's box starts:
/// a flowed line pushed below an obstacle keeps its clearance.
pub(super) fn text_box_trim(
    spec: &Spec,
    lines: &Lines,
    catalog: &mut catalog::Catalog,
    baselines: &[f32],
    height: f32,
    first_top: f32,
) -> (f32, f32) {
    let trim = spec.text_box_trim;
    if trim == exact_kernel::TextBoxTrim::None || baselines.is_empty() || lines.lines.is_empty() {
        return (0.0, 0.0);
    }
    // The root inline box's (ascent, cap height, x-height, descent): the
    // paragraph's own font, as CSS trims to it, never an inline run's.
    let edges = catalog.edge_metrics(&spec.strut);
    let top = if trim.start() {
        let [ascent, cap, x, _] = edges;
        baselines[0]
            - first_top
            - match spec.text_box_edge.over() {
                exact_kernel::OverEdge::Cap => cap,
                exact_kernel::OverEdge::Ex => x,
                exact_kernel::OverEdge::Text => ascent,
            }
    } else {
        0.0
    };
    let bottom = if trim.end() {
        let under = match spec.text_box_edge.under() {
            exact_kernel::UnderEdge::Alphabetic => 0.0,
            exact_kernel::UnderEdge::Text => edges[3],
        };
        height - (baselines[baselines.len() - 1] + under)
    } else {
        0.0
    };
    (
        if top.is_finite() { top } else { 0.0 },
        if bottom.is_finite() { bottom } else { 0.0 },
    )
}

/// A paragraph trimmed in place: its baselines and line bottoms move up by
/// the top's cut, the last bottom ends at the trimmed height (a column of it
/// is that tall), and the height loses both cuts.
pub(super) fn trim_lines(
    (top, bottom): (f32, f32),
    height: &mut f32,
    baselines: &mut [f32],
    bottoms: &mut [f32],
) {
    if top == 0.0 && bottom == 0.0 {
        return;
    }
    *height = (*height - top - bottom).max(0.0);
    for b in baselines.iter_mut() {
        *b -= top;
    }
    for b in bottoms.iter_mut() {
        *b = (*b - top).min(*height);
    }
    if let Some(last) = bottoms.last_mut() {
        *last = *height;
    }
}
