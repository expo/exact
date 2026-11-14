//! @ref LLP 1043.000 §3 D5–D7 — retained shaping, logical ranges, visual glyphs.
//! Only the accepted painter owns the current flowed layout per leaf. Flow layouts
//! never enter the width cache; replacing that owner releases the previous frame.
//! Preparation is O(G log G); a frame adds O(G + F log G + sum(g_f log g_f)) to
//! textflow's documented band bound, with O(G + F + S) storage, never frame history.
use super::lines::{Face, LayoutLine, Lines};
use super::*;
use exact_textflow::{FlowOptions, FlowShape, Fragment, Options, Prepared};
use std::ops::Range;

pub(super) struct FlowLayout {
    pub fragments: Vec<Fragment>,
    pub text: Arc<str>,
    /// Consumed source ends, excluding characters removed by line-clamp.
    pub paint_ends: Vec<usize>,
    pub line_height: f32,
    width: f32,
    pub(super) shapes: Vec<FlowShape>,
    pub(super) incomplete: bool,
}
impl FlowLayout {
    pub fn capacity_bytes(&self) -> usize {
        self.fragments.capacity() * std::mem::size_of::<Fragment>()
            + self.paint_ends.capacity() * std::mem::size_of::<usize>()
            + self.shapes.capacity() * std::mem::size_of::<FlowShape>()
    }
}
struct Cluster {
    range: Range<usize>,
    line: usize,
    glyphs: Vec<LayoutGlyph>,
    width: f32,
}
pub(super) struct FlowSource {
    text: Arc<str>,
    run_ends: Vec<usize>,
    clusters: Vec<Cluster>,
    prepared: Prepared,
    baseline: f32,
    height: f32,
    ellipses: Vec<Vec<LayoutGlyph>>,
    hyphen: Vec<LayoutGlyph>,
    /// The faces every glyph above names.
    faces: Vec<Face>,
}
impl FlowSource {
    pub(super) fn capacity_bytes(&self) -> usize {
        self.faces.capacity() * std::mem::size_of::<Face>()
            + self.hyphen.capacity() * std::mem::size_of::<LayoutGlyph>()
            + self
                .ellipses
                .iter()
                .map(|g| g.capacity() * std::mem::size_of::<LayoutGlyph>())
                .sum::<usize>()
            + self.ellipses.capacity() * std::mem::size_of::<Vec<LayoutGlyph>>()
            + self.text.len()
            + self.run_ends.capacity() * std::mem::size_of::<usize>()
            + self.prepared.capacity_bytes()
            + self.clusters.capacity() * std::mem::size_of::<Cluster>()
            + self
                .clusters
                .iter()
                .map(|c| c.glyphs.capacity() * std::mem::size_of::<LayoutGlyph>())
                .sum::<usize>()
    }

    // Synthetic ink uses the same run font and shaper as ordinary text.
    fn symbol(
        source: &Rc<ShapedSource>,
        faces: &mut Lines,
        text: &str,
        run: usize,
    ) -> Vec<LayoutGlyph> {
        let mut catalog = source.catalog.borrow_mut();
        let spec_run = &source.spec.runs[run];
        let family = catalog.choice(spec_run.family);
        let layout = catalog.shape_one(spec_run, text, &family);
        drop(catalog);
        let mut ink = Lines::default();
        for line in layout.lines() {
            super::lines::extract(&line, layout.styles(), 0.0, (&[], &[]), &mut ink);
        }
        // Source-backed symbols receive their ranges when placed in the flow.
        // Truncation tokens keep an empty range.
        ink.glyphs
            .iter()
            .map(|g| LayoutGlyph {
                start: 0,
                end: 0,
                metadata: run as u32,
                face: faces.face(ink.faces[g.face as usize].clone()),
                ..*g
            })
            .collect()
    }

    fn new(source: &Rc<ShapedSource>) -> Self {
        // This is layout of the retained shape, NOT another shaping call.
        // It also supplies exactly the ordinary CSS strut/fallback-font baseline.
        let plain = source.layout(None);
        let mut faces = Lines {
            faces: plain.layouts().faces.clone(),
            ..Lines::default()
        };
        let text: Arc<str> = source
            .spec
            .runs
            .iter()
            .map(|r| r.text.as_str())
            .collect::<String>()
            .into();
        let run_ends: Vec<_> = source
            .spec
            .runs
            .iter()
            .scan(0, |end, r| {
                *end += r.text.len();
                Some(*end)
            })
            .collect();
        let mut clusters: Vec<Cluster> = Vec::new();
        let (baseline, height) = source.flow_box(
            plain.layout_runs().flat_map(|r| r.glyphs),
            &plain.layouts().faces,
        );
        for run in plain.layout_runs() {
            let start = source.data.lines[run.line_i].start;
            let mut glyphs = run.glyphs.to_vec();
            // Logical clusters: a stable sort keeps the shaper's order inside
            // multi-glyph clusters.
            glyphs.sort_by_key(|g| g.start);
            for mut glyph in glyphs {
                let range = start + glyph.start as usize..start + glyph.end as usize;
                glyph.start = range.start as u32;
                glyph.end = range.end as u32;
                if let Some(last) = clusters.last_mut().filter(|c| c.range == range) {
                    last.width += glyph.w;
                    last.glyphs.push(glyph);
                } else {
                    clusters.push(Cluster {
                        range,
                        line: run.line_i,
                        width: glyph.w,
                        glyphs: vec![glyph],
                    });
                }
            }
        }
        if !source.spec.white_space.model().preserves()
            && text.chars().any(|ch| {
                matches!(
                    ch,
                    '\t' | '\n' | '\r' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
                )
            })
        {
            let template = Self::symbol(source, &mut faces, " ", 0);
            let width = template.iter().map(|g| g.w).sum();
            let whitespace = |ch| {
                matches!(
                    ch,
                    ' ' | '\t' | '\n' | '\r' | '\u{c}' | '\u{85}' | '\u{2028}' | '\u{2029}'
                )
            };
            let spaces: Vec<_> = text
                .char_indices()
                .filter(|(_, ch)| whitespace(*ch))
                .map(|(at, ch)| {
                    // Only CSS-collapsible characters receive a space advance.
                    // Other break controls have no ink in the flowed painter.
                    let collapsible = matches!(ch, ' ' | '\t' | '\n' | '\r');
                    let pos = clusters.partition_point(|c| c.range.start < at);
                    let line = clusters
                        .get(pos)
                        .or_else(|| clusters.last())
                        .map_or(0, |c| c.line);
                    Cluster {
                        range: at..at + ch.len_utf8(),
                        line,
                        glyphs: template
                            .iter()
                            .map(|g| LayoutGlyph {
                                start: at as u32,
                                end: (at + ch.len_utf8()) as u32,
                                metadata: run_ends.partition_point(|end| *end <= at) as u32,
                                w: if collapsible { g.w } else { 0.0 },
                                ..*g
                            })
                            .collect(),
                        width: if collapsible { width } else { 0.0 },
                    }
                })
                .collect();
            clusters.retain(|c| !text[c.range.clone()].chars().all(whitespace));
            clusters.extend(spaces);
            clusters.sort_by_key(|c| c.range.start);
        }

        // The room before a run's first cluster (`Run::edges`): its glyphs
        // are drawn past it, and it is the cluster's width.
        let leads: Vec<(usize, f32)> = source
            .spec
            .runs
            .iter()
            .zip(std::iter::once(0).chain(run_ends.iter().copied()))
            .filter(|(r, _)| r.edges.start != 0.0 && r.inline.is_none())
            .map(|(r, at)| (at, r.edges.start))
            .collect();
        let mut prefix = Vec::with_capacity(clusters.len() + 1);
        prefix.push(0.0_f64);
        for c in &mut clusters {
            let left = c.glyphs.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let lead = leads
                .iter()
                .find(|l| l.0 == c.range.start)
                .map_or(0.0, |l| l.1);
            c.width += lead;
            for g in &mut c.glyphs {
                g.x -= left - lead;
            }
            prefix.push(prefix.last().unwrap() + c.width as f64);
        }
        let mut measure = |range: Range<usize>| {
            let a = clusters.partition_point(|c| c.range.start < range.start);
            let b = clusters.partition_point(|c| c.range.start < range.end);
            (prefix[b] - prefix[a]) as f32
        };
        let overflow_wrap = match source.spec.overflow_wrap {
            exact_kernel::OverflowWrap::Normal => exact_textflow::OverflowWrap::Normal,
            exact_kernel::OverflowWrap::BreakWord => exact_textflow::OverflowWrap::BreakWord,
            exact_kernel::OverflowWrap::Anywhere => exact_textflow::OverflowWrap::Anywhere,
        };
        let hyphen = if text.contains('\u{ad}') {
            Self::symbol(source, &mut faces, "-", 0)
        } else {
            Vec::new()
        };
        let prepared = Prepared::new(
            &text,
            Options {
                white_space: source.spec.white_space.model(),
                overflow_wrap,
                hyphen_advance: hyphen.iter().map(|g| g.w).sum(),
            },
            &mut measure,
        );
        let ellipses = if source.spec.line_clamp > 0 {
            (0..source.spec.runs.len())
                .map(|i| Self::symbol(source, &mut faces, "…", i))
                .collect()
        } else {
            Vec::new()
        };
        Self {
            faces: faces.faces,
            hyphen,
            ellipses,
            text,
            run_ends,
            clusters,
            prepared,
            baseline,
            height,
        }
    }
}

impl Paragraph {
    /// An incomplete flow was replaced by ordinary engine layout.
    pub fn flow_incomplete(&self) -> bool {
        self.flow.as_ref().is_some_and(|f| f.incomplete)
    }
    /// Flowed source ranges in logical order, including consumed whitespace.
    /// Empty for ordinary engine-wrapped paragraphs.
    pub fn fragments(&self) -> &[Fragment] {
        self.flow.as_ref().map_or(&[], |f| &f.fragments)
    }
    /// Fixed CSS line band for a flowed paragraph (zero for ordinary paragraphs).
    pub fn flow_line_height(&self) -> f32 {
        self.flow.as_ref().map_or(0.0, |f| f.line_height)
    }
}
impl TextEngine {
    /// Layout from the same full shape as ordinary text. The caller may hand back
    /// its current snapshot; no shape/frame history is retained by this method.
    /// Clamp counts bands, including obstructed ones.
    pub fn paragraph_flow(
        &mut self,
        spec: &Spec,
        width: f32,
        shapes: &[FlowShape],
        previous: Option<&Rc<Paragraph>>,
    ) -> Rc<Paragraph> {
        let key = self.paragraphs.identity(spec);
        self.flow_for(key, width, shapes, previous)
    }
    pub(crate) fn flow_identified(
        &mut self,
        stamp: &ParagraphStamp,
        width: f32,
        shapes: &[FlowShape],
        previous: Option<&Rc<Paragraph>>,
        build: impl FnOnce() -> Spec,
    ) -> Option<Rc<Paragraph>> {
        if shapes.is_empty() {
            return self.paragraph_identified(stamp, Some(width), build);
        }
        let (key, spec) = self.identified_spec(stamp, build);
        (!spec.is_empty()).then(|| self.flow_for(key, width, shapes, previous))
    }
    pub(super) fn flow_for(
        &mut self,
        key: (u64, u64),
        width: f32,
        shapes: &[FlowShape],
        previous: Option<&Rc<Paragraph>>,
    ) -> Rc<Paragraph> {
        if shapes.is_empty() {
            let spec = self.paragraphs.spec(key).unwrap();
            return self.paragraph_for(&spec, Some(width), key);
        }
        // Retain the source even if the cold index evicted its identity meanwhile.
        let source = if let Some(p) = previous.filter(|p| {
            Rc::ptr_eq(&self.catalog, &p.source.catalog)
                && self.paragraphs.spec(key).as_deref() == Some(&*p.source.spec)
        }) {
            p.source.clone()
        } else {
            self.source(key)
        };
        if let Some(p) = previous.filter(|p| {
            Rc::ptr_eq(&p.source, &source)
                && p.flow
                    .as_ref()
                    .is_some_and(|f| f.width.to_bits() == width.to_bits() && f.shapes == shapes)
        }) {
            return p.clone();
        }
        let started = Instant::now();
        self.layout_calls += 1;
        let (p, walk) = source.layout_flow(width, shapes);
        self.flow_walk += walk;
        self.flowing += started.elapsed();
        Rc::new(p)
    }
}

impl ShapedSource {
    pub(super) fn layout_flow(
        self: &Rc<Self>,
        width: f32,
        shapes: &[FlowShape],
    ) -> (Paragraph, Duration) {
        let source = self.clone();
        if source.flow.borrow().is_none() {
            let prepared = Rc::new(FlowSource::new(&source));
            *source.flow.borrow_mut() = Some(prepared);
        }
        let data = source.flow.borrow().as_ref().unwrap().clone();
        let options = FlowOptions {
            direction: if source.spec.direction == exact_kernel::Direction::Rtl {
                exact_textflow::Direction::Rtl
            } else {
                exact_textflow::Direction::Ltr
            },
            width,
            line_height: data.height,
            min_fragment: source.spec.strut.size * exact_textflow::MIN_FRAGMENT_EM,
            max_lines: source.spec.line_clamp,
        };
        let mut fragments = Vec::new();
        let walk_started = Instant::now();
        let result = exact_textflow::flow(&data.prepared, shapes, &options, &mut fragments);
        let walk = walk_started.elapsed();
        if !result.complete && !result.clamped {
            let mut ordinary = source.layout(Some(width));
            ordinary.flow = Some(Arc::new(FlowLayout {
                fragments: Vec::new(),
                text: data.text.clone(),
                paint_ends: Vec::new(),
                line_height: data.height,
                width,
                shapes: shapes.to_vec(),
                incomplete: true,
            }));
            ordinary.resident_capacity_bytes = cache::capacities(&ordinary);
            return (ordinary, walk);
        }
        let mut height = result.height;
        let mut layouts = Lines {
            faces: data.faces.clone(),
            ..Lines::default()
        };
        let mut baselines = Vec::new();
        let mut bottoms = Vec::new();
        let mut slots = Vec::new();
        let mut band = u32::MAX;
        let mut ending_band = false;
        let fragment_count = fragments.len();
        let mut paint_ends = Vec::with_capacity(fragment_count);
        for fragment_index in 0..fragment_count {
            let f = &fragments[fragment_index];
            if band != f.line {
                exact_textflow::intervals(
                    shapes,
                    f.y,
                    f.y + data.height,
                    width,
                    options.min_fragment,
                    &mut slots,
                );
                band = f.line;
                // Every interval on an ending line keeps start alignment.
                let last = fragments[fragment_index..]
                    .iter()
                    .take_while(|f| f.line == band)
                    .last()
                    .unwrap();
                ending_band = last.end == data.text.len()
                    || (result.clamped && last.line == fragments[fragment_count - 1].line)
                    || data.text[..last.end]
                        .chars()
                        .next_back()
                        .is_some_and(|ch| source.spec.white_space.model().forces_break(ch));
            }
            let f = &mut fragments[fragment_index];
            let available = slots
                .iter()
                .find(|s| s.0 == f.x)
                .map_or(width, |s| s.1 - s.0);
            let a = data.clusters.partition_point(|c| c.range.start < f.start);
            let b = data.clusters.partition_point(|c| c.range.start < f.end);
            let mut visible = Vec::new();
            let mut pending_space = None;
            let preserves = source.spec.white_space.model().preserves();
            for c in &data.clusters[a..b] {
                let value = &data.text[c.range.clone()];
                if preserves
                    && value.chars().all(|ch| {
                        matches!(
                            ch,
                            ' ' | '\t' | '\r' | '\n' | '\u{85}' | '\u{c}' | '\u{2028}' | '\u{2029}'
                        )
                    })
                {
                    if value.chars().all(|ch| matches!(ch, ' ' | '\t')) {
                        visible.push(c);
                    }
                } else if value
                    .chars()
                    .all(|ch| matches!(ch, ' ' | '\t' | '\r' | '\n'))
                {
                    if !visible.is_empty() && pending_space.is_none() {
                        pending_space = Some(c);
                    }
                } else if value
                    .chars()
                    .all(|ch| matches!(ch, '\u{85}' | '\u{c}' | '\u{2028}' | '\u{2029}'))
                {
                    // Under collapse these are zero-width text, as the walker
                    // measures them: the spaces on either side are kept.
                    if let Some(space) = pending_space.take() {
                        visible.push(space);
                    }
                    visible.push(c);
                } else if !value.chars().all(|ch| {
                    matches!(
                        ch,
                        '\r' | '\n'
                            | '\u{85}'
                            | '\u{c}'
                            | '\u{2028}'
                            | '\u{2029}'
                            | '\u{200b}'
                            | '\u{ad}'
                    )
                }) {
                    if let Some(space) = pending_space.take() {
                        visible.push(space);
                    }
                    visible.push(c);
                }
            }
            // @ref LLP 1043.000 §3 D7 — reserve the run's real ellipsis ink.
            let mut suffix: Option<&[LayoutGlyph]> = f.hyphenated.then_some(&data.hyphen);
            let mut paint_end = f.end;
            if result.clamped && fragment_index + 1 == fragment_count {
                f.hyphenated = false;
                while visible
                    .last()
                    .is_some_and(|c| data.text[c.range.clone()].chars().all(char::is_whitespace))
                {
                    paint_end = paint_end.min(visible.pop().unwrap().range.start);
                }
                let mut total: f32 = visible.iter().map(|c| c.width).sum();
                loop {
                    let run = visible
                        .last()
                        .and_then(|c| c.glyphs.first())
                        .map_or(0, |g| g.run());
                    let ink = &data.ellipses[run];
                    let advance: f32 = ink.iter().map(|g| g.w).sum();
                    if total + advance <= available || visible.is_empty() {
                        suffix = (advance <= available).then_some(ink);
                        f.width = total + if suffix.is_some() { advance } else { 0.0 };
                        break;
                    }
                    let removed = visible.pop().unwrap();
                    total -= removed.width;
                    paint_end = paint_end.min(removed.range.start);
                }
            }
            paint_ends.push(paint_end);
            // UAX #9 L2 on whole clusters, preserving glyph order inside a cluster.
            let max = visible.iter().map(|c| c.glyphs[0].level).max().unwrap_or(0);
            let min_odd = visible
                .iter()
                .map(|c| c.glyphs[0].level)
                .filter(|l| l % 2 == 1)
                .min()
                .unwrap_or(max + 1);
            for level in (min_odd..=max).rev() {
                let mut i = 0;
                while i < visible.len() {
                    if visible[i].glyphs[0].level < level {
                        i += 1;
                        continue;
                    }
                    let start = i;
                    while i < visible.len() && visible[i].glyphs[0].level >= level {
                        i += 1;
                    }
                    visible[start..i].reverse();
                }
            }
            // Match ordinary Parley justification: widen spaces and NBSPs,
            // leaving paragraph ends, forced breaks and clamp endings at start.
            let space = |c: &Cluster| matches!(&data.text[c.range.clone()], " " | "\u{a0}");
            let last_ink = visible
                .iter()
                .filter(|c| !space(c))
                .map(|c| c.range.start)
                .max();
            let expands = |c: &Cluster| space(c) && last_ink.is_some_and(|end| c.range.start < end);
            let spaces = visible.iter().filter(|c| expands(c)).count();
            let extra = if source.spec.align == TextAlign::Justify && !ending_band && spaces > 0 {
                (available - f.width).max(0.0) / spaces as f32
            } else {
                0.0
            };
            f.width += extra * spaces as f32;
            let shift = match source.spec.align.physical(source.spec.direction) {
                TextAlign::Center => (available - f.width) / 2.0,
                TextAlign::Right => available - f.width,
                TextAlign::Justify if source.spec.direction == exact_kernel::Direction::Rtl => {
                    available - f.width
                }
                _ => 0.0,
            }
            .max(0.0);
            f.x += shift;
            let mut x = f.x;
            let first = layouts.glyphs.len() as u32;
            let glyphs = &mut layouts.glyphs;
            let append_suffix = |glyphs: &mut Vec<LayoutGlyph>, x: &mut f32| {
                if let Some(ink) = suffix {
                    for g in ink {
                        let mut g = *g;
                        if f.hyphenated {
                            g.start = (f.end - '\u{ad}'.len_utf8()) as u32;
                            g.end = f.end as u32;
                            g.metadata = data
                                .run_ends
                                .partition_point(|end| *end <= g.start as usize)
                                as u32;
                        }
                        g.x += *x;
                        *x += g.w;
                        glyphs.push(g);
                    }
                }
            };
            if source.spec.direction == exact_kernel::Direction::Rtl {
                append_suffix(glyphs, &mut x);
            }
            for c in visible {
                let adjustment = if expands(c) { extra } else { 0.0 };
                let last = c.glyphs.len().saturating_sub(1);
                glyphs.extend(c.glyphs.iter().copied().enumerate().map(|(i, mut g)| {
                    g.x += x;
                    if i == last {
                        g.w += adjustment;
                    }
                    g
                }));
                x += c.width + adjustment;
            }
            if source.spec.direction != exact_kernel::Direction::Rtl {
                append_suffix(glyphs, &mut x);
            }
            // Trailing hard breaks have no next cluster. Keep their empty
            // layouts after the final source line, never back in line zero.
            let line = data
                .clusters
                .get(a)
                .or_else(|| data.clusters.last())
                .map_or(0, |c| c.line);
            let end = layouts.glyphs.len() as u32;
            layouts.lines.push(LayoutLine {
                hard: line as u32,
                glyphs: (first, end),
                w: f.width,
                max_ascent: data.baseline,
                max_descent: data.height - data.baseline,
                line_height: Some(data.height),
            });
            baselines.push(f.y + data.baseline);
            bottoms.push(f.y + data.height);
        }
        let measured_width = fragments
            .iter()
            .map(|f| f.x + f.width)
            .fold(0.0_f32, f32::max)
            .ceil();
        // CSS `text-box-trim`, as an unflowed paragraph has it.
        let cut = super::shaping::text_box_trim(
            &source.spec,
            &layouts,
            &mut self.catalog.borrow_mut(),
            &baselines,
            height,
            // A line flow pushed below an obstacle loses only its own space above the edge
            fragments.first().map_or(0.0, |f| f.y),
        );
        super::shaping::trim_lines(cut, &mut height, &mut baselines, &mut bottoms);
        // The fragments move with the lines they hold
        for f in &mut fragments {
            f.y -= cut.0;
        }
        let mut p = Paragraph {
            source,
            record: std::cell::OnceCell::from(Arc::new(layouts)),
            remake: None,
            measured: RefCell::new(None),
            flow: Some(Arc::new(FlowLayout {
                fragments,
                text: data.text.clone(),
                paint_ends,
                line_height: data.height,
                width,
                shapes: shapes.to_vec(),
                incomplete: false,
            })),
            #[cfg(test)]
            layout_lifetime: Arc::new(()),
            width: measured_width,
            height,
            first_baseline: baselines.first().copied().unwrap_or(0.0),
            baselines: Arc::new(baselines),
            bottoms: Arc::new(bottoms),
            ellipsized: RefCell::new(None),
            strokes: RefCell::new(super::stroke::Cache::default()),
            ink: RefCell::new(ink::Cache::default()),
            resident_capacity_bytes: 0,
            private_text_bytes_estimate: 0,
        };
        p.resident_capacity_bytes = cache::capacities(&p);
        (p, walk)
    }
}
