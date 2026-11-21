//! One width's lines as the host keeps them (LLP 1085.000 §4, the storage
//! spike's arrangement (b)): extracted from the paragraph's Parley layouts
//! once they are broken at that width, owned by the host and independent of
//! them, so a shaped layout is broken again for the next width while every
//! painted width keeps its own glyphs.
//!
//! Glyphs are walked by run and cluster, never through Parley's glyph-run
//! iterator (6–20× slower per glyph; the spike), and placed as its
//! `positioned_glyphs` places them: each glyph at the pen plus its offset,
//! the pen moved by its advance.
#[path = "field_lines.rs"]
mod field_lines;
pub use field_lines::Glyphs;
pub(super) use field_lines::{Block, FieldLines};

use super::catalog::FaceId;
use parley::FontData;
use std::sync::Arc;

impl super::Paragraph {
    pub(crate) fn baseline_at(&self, origin: (f32, f32)) -> Option<(f32, f32)> {
        let lines = self.lines();
        lines
            .lines
            .iter()
            .zip(self.baselines())
            .find_map(|(line, y)| {
                let glyph = lines.glyphs_of(line).first()?;
                Some((origin.0 + glyph.x, origin.1 + y))
            })
    }

    /// A fragment's ink, retaining original line indices and coordinates.
    /// Lines assigned to other columns have no glyphs or inline boxes.
    pub(crate) fn line_fragment(&self, range: std::ops::Range<usize>) -> Self {
        use std::cell::{OnceCell, RefCell};
        let source = self.lines();
        let glyph_start = source.lines[range.start].glyphs.0;
        let glyph_end = source.lines[range.end - 1].glyphs.1;
        let lines = Lines {
            lines: source
                .lines
                .iter()
                .enumerate()
                .map(|(i, line)| {
                    let mut line = *line;
                    if range.contains(&i) {
                        line.glyphs = (line.glyphs.0 - glyph_start, line.glyphs.1 - glyph_start);
                    } else {
                        line.glyphs = (0, 0);
                        line.w = 0.0;
                    }
                    line
                })
                .collect(),
            glyphs: source.glyphs[glyph_start as usize..glyph_end as usize]
                .to_vec()
                .into(),
            faces: source.faces.clone(),
            field_metrics: Vec::new(),
            boxes: source
                .boxes
                .iter()
                .filter(|b| range.contains(&(b.line as usize)))
                .copied()
                .collect(),
            rooms: source
                .rooms
                .iter()
                .filter(|r| range.contains(&(r.line as usize)))
                .copied()
                .collect(),
        };
        Self {
            source: self.source.clone(),
            record: OnceCell::from(Arc::new(lines)),
            measured: RefCell::new(None),
            remake: None,
            flow: self.flow.clone(),
            #[cfg(test)]
            layout_lifetime: self.layout_lifetime.clone(),
            width: self.width,
            height: self.height,
            first_baseline: self.first_baseline,
            baselines: self.baselines.clone(),
            bottoms: self.bottoms.clone(),
            ink: RefCell::new(Default::default()),
            strokes: RefCell::new(Default::default()),
            ellipsized: RefCell::new(None),
            resident_capacity_bytes: 0,
            private_text_bytes_estimate: 0,
        }
    }
}

/// A laid-out glyph.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutGlyph {
    /// Start of its cluster in [`super::LayoutRun::text`], bytes.
    pub start: u32,
    /// End of its cluster (a ligature's covers every component).
    pub end: u32,
    /// Pen position from the paragraph's left, points, its offset included.
    pub x: f32,
    /// Offset below the baseline, points (y down).
    pub y: f32,
    /// Advance, points.
    pub w: f32,
    /// Points.
    pub font_size: f32,
    /// In its face.
    pub glyph_id: u32,
    /// The canonical run index ([`super::Spec`]'s runs), kept across font
    /// fallback and wrapping.
    pub metadata: u32,
    /// Its face, an index into [`Lines::faces`].
    pub face: u16,
    /// Its bidi embedding level.
    pub level: u8,
}

impl LayoutGlyph {
    /// The canonical run index.
    pub fn run(&self) -> usize {
        self.metadata as usize
    }
    /// The cluster's byte range in [`super::LayoutRun::text`].
    pub fn range(&self) -> std::ops::Range<usize> {
        self.start as usize..self.end as usize
    }
    /// Where the CPU raster places it with the pen at `offset` (device
    /// pixels) at `scale`: whole pixel x, its quarter-pixel phase, and
    /// whole pixel y.
    pub fn pixel(&self, offset: (f32, f32), scale: f32) -> (i32, u8, i32) {
        let (x, bin) = super::catalog::subpixel(self.x.mul_add(scale, offset.0));
        let (y, _) = super::catalog::subpixel(self.y.mul_add(scale, offset.1).trunc());
        (x, bin, y)
    }
}

/// A face as the glyphs of one width name it.
#[derive(Clone, Debug)]
pub struct Face {
    /// The face's data and collection index.
    pub font: FontData,
    /// Normalized variation coordinates (a variable face's axes, as Parley
    /// set them for the run's weight).
    pub coords: Arc<[i16]>,
    /// An oblique Parley synthesized: the face has no italic.
    pub skew: bool,
    /// What fontique set to match the style: its variation settings in the
    /// axes' own units (`wght` 700, `ital` 1, `slnt` 14, `wdth` 75; `coords`
    /// normalized from them) and the skew. A presenter that draws from the
    /// font file sets these on its own font (Android's `Font`).
    pub synthesis: fontique::Synthesis,
    /// The CSS weight shaped with.
    pub weight: u16,
}

impl Face {
    /// The face's identity.
    pub fn id(&self) -> FaceId {
        FaceId::of(&self.font)
    }
}

/// One visual line.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct LayoutLine {
    /// The hard line (text between line feeds) it belongs to.
    pub hard: u32,
    /// Its glyphs, a range of [`Lines::glyphs`], in visual order.
    pub glyphs: (u32, u32),
    /// Its width, points: what its content takes, indent included.
    pub w: f32,
    /// A flowed line's band (LLP 1043.000 §3 D7): ascent, descent and line
    /// height; zero for ordinary lines, whose box is the host's line pass.
    pub max_ascent: f32,
    /// See `max_ascent`.
    pub max_descent: f32,
    /// See `max_ascent`.
    pub line_height: Option<f32>,
}

/// An image's box in a line (LLP 1011.002): where its run's box went.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutBox {
    /// The visual line it is on, an index into [`Lines::lines`].
    pub line: u32,
    /// Its left edge from the paragraph's left, points.
    pub x: f32,
    /// The canonical run index whose box it is.
    pub run: u32,
}

/// One width's lines, glyphs and faces.
#[derive(Clone, Debug, Default)]
pub struct Lines {
    /// Visual lines, top to bottom.
    pub lines: Vec<LayoutLine>,
    /// Every line's glyphs.
    pub glyphs: Glyphs,
    /// The faces the glyphs name.
    pub faces: Vec<Face>,
    /// Every line's inline boxes, in visual order.
    pub boxes: Vec<LayoutBox>,
    /// Every line's inline-box room, by line.
    pub rooms: Vec<LayoutRoom>,
    /// The unrounded ascent, descent and explicit-height flag before paragraph trimming.
    /// Editable lines carry these across revisions even when their vertical origin moves.
    pub(super) field_metrics: Vec<Option<(f32, f32, bool)>>,
}

impl Lines {
    /// A line's glyphs.
    pub fn glyphs_of(&self, line: &LayoutLine) -> &[LayoutGlyph] {
        self.glyphs.line(line)
    }
    /// Exact accessible capacities, bytes.
    pub fn capacity_bytes(&self) -> usize {
        use std::mem::size_of;
        self.lines.capacity() * size_of::<LayoutLine>()
            + self.glyphs.bytes()
            + self.faces.capacity() * size_of::<Face>()
            + self.boxes.capacity() * size_of::<LayoutBox>()
            + self.rooms.capacity() * size_of::<LayoutRoom>()
            + self.field_metrics.capacity() * size_of::<Option<(f32, f32, bool)>>()
            + self
                .faces
                .iter()
                .map(|f| f.coords.len() * size_of::<i16>())
                .sum::<usize>()
    }
    /// The index of `face` among these lines' faces, added when new.
    pub(super) fn face(&mut self, face: Face) -> u16 {
        if let Some(i) = self
            .faces
            .iter()
            .position(|f| f.font == face.font && f.coords == face.coords && f.skew == face.skew)
        {
            return i as u16;
        }
        self.faces.push(face);
        (self.faces.len() - 1) as u16
    }
    /// Release spare capacity once the width is complete. Ordinary
    /// paragraphs keep a little spare; this is not a memory limit.
    pub(super) fn tighten(&mut self) {
        use std::mem::size_of;
        let spare = (self.glyphs.capacity() - self.glyphs.len()) * size_of::<LayoutGlyph>()
            + (self.lines.capacity() - self.lines.len()) * size_of::<LayoutLine>();
        if spare >= 4 * 1024 {
            self.glyphs.shrink_to_fit();
            self.lines.shrink_to_fit();
        }
        self.faces.shrink_to_fit();
    }
}

/// Which of a run's rooms (`Run::edges`) a room is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RoomKind {
    /// Its inline box's start, beside its first character.
    Start,
    /// Its end, beside its last character.
    End,
    /// The whole box of a run with no text, before the character after it.
    Before,
    /// The whole box of a run with no text at the end of its line's text,
    /// after the character before it.
    After,
}

/// One room in a hard line, as it is shaped: on the cluster holding byte
/// `at`, the run's characters there being `span`.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Room {
    pub at: usize,
    pub w: f32,
    pub run: u32,
    pub kind: RoomKind,
    pub span: std::ops::Range<usize>,
}

/// A room as a line draws it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutRoom {
    /// The visual line it is on, an index into [`Lines::lines`].
    pub line: u32,
    /// Its left edge from the paragraph's left, points.
    pub x: f32,
    /// Its width, points.
    pub w: f32,
    /// The canonical run index whose room it is.
    pub run: u32,
    /// Which of the run's rooms it is.
    pub kind: RoomKind,
}

/// A hard line's rooms, its images' start rooms by run, and whether its
/// paragraph, and so each inline box in it, is right to left.
#[derive(Clone, Copy)]
pub(super) struct Rooms<'a> {
    pub rooms: &'a [Room],
    pub boxes: &'a [(u32, f32)],
    pub rtl: bool,
}

impl Rooms<'_> {
    pub const NONE: Rooms<'static> = Rooms {
        rooms: &[],
        boxes: &[],
        rtl: false,
    };
}

/// One cluster of a line in visual order: its first byte, whether it runs
/// right to left, and `None` for an inline box.
type Placed = Option<(usize, bool)>;

/// Where each room on a line is drawn: (visual cluster, right of it, room),
/// in the order each side of a cluster holds them. A start or an end is on
/// its inline box's start or end side of the visually contiguous characters
/// of its run that hold the character it is shaped with: under `rtl` the
/// start on their right and the end on their left (CSS 2 §8.6, §9.10; CSS
/// Writing Modes 3 §2.1), whatever the direction of the text. A run with no
/// text is on the start side of the character after it, or the end side of
/// the character before it, by that character's direction.
fn place(order: &[Placed], ends: &[usize], rooms: &[Room], rtl: bool) -> Vec<(usize, bool, usize)> {
    let mut out = Vec::new();
    for (i, room) in rooms.iter().enumerate() {
        let Some(k) = order
            .iter()
            .zip(ends)
            .position(|(c, end)| c.is_some_and(|(start, _)| start <= room.at && room.at < *end))
        else {
            continue;
        };
        let (_, glyph_rtl) = order[k].expect("a cluster");
        let (k, right) = match room.kind {
            RoomKind::Before => (k, glyph_rtl),
            RoomKind::After => (k, !glyph_rtl),
            kind => {
                let mine = |c: &Placed| c.is_some_and(|(start, _)| room.span.contains(&start));
                let right = (kind == RoomKind::Start) == rtl;
                if right {
                    let run = order[k..].iter().take_while(|c| mine(c)).count();
                    (k + run.max(1) - 1, true)
                } else {
                    let run = order[..k].iter().rev().take_while(|c| mine(c)).count();
                    (k - run, false)
                }
            }
        };
        out.push((k, right, i));
    }
    // Logical order away from the text on a start side, towards it on an end side.
    out.sort_by_key(|&(k, right, i)| {
        let away = match rooms[i].kind {
            RoomKind::Start | RoomKind::Before => right,
            RoomKind::End | RoomKind::After => !right,
        };
        (k, right, if away { usize::MAX - i } else { i })
    });
    out
}

#[cfg(test)]
thread_local! { pub(super) static EXTRACTED: std::cell::Cell<usize> = const { std::cell::Cell::new(0) }; }

/// One Parley line's glyphs, appended in visual order with `shift` added
/// to every x (`styles` is the layout's, whose brushes are run indices),
/// and its inline boxes and rooms as the next line's (a box's id is its run
/// index). A room is in its cluster's advance and not its glyphs': it is
/// drawn where [`place`] puts it, and an image's start before its box.
/// Returns the line's summed cluster advance (justified where it is).
pub(super) fn extract(
    line: &parley::Line<'_, u32>,
    styles: &[parley::Style<u32>],
    shift: f32,
    rooms: Rooms<'_>,
    out: &mut Lines,
) -> f32 {
    let m = line.metrics();
    #[cfg(test)]
    let before = out.glyphs.len();
    let line_index = out.lines.len() as u32;
    // The rooms shaped on this line, and where each is drawn.
    let mut placed = Vec::new();
    let mut here: &[Room] = &[];
    if !rooms.rooms.is_empty() {
        let range = line.text_range();
        let from = rooms.rooms.partition_point(|r| r.at < range.start);
        let to = rooms.rooms.partition_point(|r| r.at < range.end);
        here = &rooms.rooms[from..to];
    }
    if !here.is_empty() {
        let mut order: Vec<Placed> = Vec::new();
        let mut ends = Vec::new();
        for item in line.items_nonpositioned() {
            match item {
                parley::LineItem::Run(run) => {
                    let rtl = run.bidi_level() % 2 == 1;
                    for cluster in run.visual_clusters() {
                        let range = cluster.text_range();
                        order.push(Some((range.start, rtl)));
                        ends.push(range.end.max(range.start + 1));
                    }
                }
                parley::LineItem::InlineBox(_) => {
                    order.push(None);
                    ends.push(0);
                }
            }
        }
        placed = place(&order, &ends, here, rooms.rtl);
    }
    let mut next = 0;
    let mut pen = m.offset + m.inline_min_coord + shift;
    let mut content = 0.0f32;
    let mut k = 0usize;
    // Draw the rooms placed at visual cluster `k` on one side, moving the pen.
    let rooms_at = |k: usize, right: bool, pen: &mut f32, out: &mut Lines, next: &mut usize| {
        while *next < placed.len() && placed[*next].0 == k && placed[*next].1 == right {
            let room = &here[placed[*next].2];
            out.rooms.push(LayoutRoom {
                line: line_index,
                x: *pen,
                w: room.w,
                run: room.run,
                kind: room.kind,
            });
            *pen += room.w;
            *next += 1;
        }
    };
    for item in line.items_nonpositioned() {
        let run = match item {
            parley::LineItem::Run(run) => run,
            parley::LineItem::InlineBox(b) => {
                if b.kind == parley::InlineBoxKind::InFlow {
                    let before = rooms
                        .boxes
                        .iter()
                        .find(|l| l.0 == b.id as u32)
                        .map_or(0.0, |l| l.1);
                    out.boxes.push(LayoutBox {
                        line: line_index,
                        x: pen + before,
                        run: b.id as u32,
                    });
                    pen += b.width;
                    content += b.width;
                }
                k += 1;
                continue;
            }
        };
        let synthesis = run.synthesis();
        let face = out.face(Face {
            font: run.font().clone(),
            coords: Arc::from(run.normalized_coords()),
            skew: synthesis.skew().is_some(),
            synthesis,
            weight: run.font_attrs().weight.value().round().clamp(1.0, 1000.0) as u16,
        });
        let size = run.font_size();
        let level = run.bidi_level();
        for cluster in run.visual_clusters() {
            content += cluster.advance();
            let mut range = cluster.text_range();
            if cluster.is_ligature_start() {
                let mut next = cluster.next_logical();
                while let Some(c) = next.filter(|c| c.is_ligature_continuation()) {
                    range.end = range.end.max(c.text_range().end);
                    next = c.next_logical();
                }
            }
            // The rooms shaped with this cluster, which its advance holds.
            let start = cluster.text_range().start;
            let room: f32 = here.iter().filter(|r| r.at == start).map(|r| r.w).sum();
            rooms_at(k, false, &mut pen, out, &mut next);
            // A single glyph Parley keeps in its cluster takes the cluster's
            // advance, room included: the room is not the glyph's.
            let mut excess = if room == 0.0 {
                0.0
            } else {
                cluster.glyphs().map(|g| g.advance).sum::<f32>() - (cluster.advance() - room)
            };
            for glyph in cluster.glyphs() {
                let style = styles.get(glyph.style_index()).map_or(0, |s| s.brush);
                let w = glyph.advance - std::mem::take(&mut excess);
                out.glyphs.push(LayoutGlyph {
                    start: range.start as u32,
                    end: range.end as u32,
                    x: pen + glyph.x,
                    y: glyph.y,
                    w,
                    font_size: size,
                    glyph_id: glyph.id,
                    metadata: style,
                    face,
                    level,
                });
                pen += w;
            }
            rooms_at(k, true, &mut pen, out, &mut next);
            k += 1;
        }
    }
    #[cfg(test)]
    EXTRACTED.with(|n| n.set(n.get() + out.glyphs.len() - before));
    content
}
