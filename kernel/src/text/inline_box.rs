//! An image in a paragraph's text (LLP 1011.002): an inline replaced box,
//! as `<img>` in a `<p>` is (CSS 2 §10.3.2, §10.6.2, §10.8). Its run's text is
//! U+FFFC, where the measurer reserves the box; the kernel places the image
//! node where the measurer says the box went.

use crate::arena::NodeArena;
use crate::generated::VerticalAlign;

/// The text an inline box's run carries: OBJECT REPLACEMENT CHARACTER, as
/// an `NSTextAttachment` stands in an attributed string.
pub const INLINE_BOX_TEXT: &str = "\u{fffc}";

/// What a measurer reserves for an image in its paragraph: the image's
/// margin box, and where it sits against the line's baseline.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineBox {
    /// The margin box's width.
    pub width: f32,
    /// The margin box's height.
    pub height: f32,
    /// `vertical-align`'s keyword.
    pub align: VerticalAlign,
    /// `vertical-align`'s length: how far the box is raised from where
    /// `align` puts it.
    pub shift: f32,
}

/// The parent font's metrics an alignment reads (CSS Inline 3 §4.2), in
/// points: its ascent and descent, both positive, and its x-height.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct FontEdges {
    /// From the baseline up to the text-over edge.
    pub ascent: f32,
    /// From the baseline down to the text-under edge.
    pub descent: f32,
    /// The x-height above the baseline.
    pub x_height: f32,
}

impl InlineBox {
    /// How far the box's top stands above the baseline, among text in
    /// `font`. Its descent below the baseline is `height` less this, and
    /// negative when the box is raised clear of it.
    pub fn ascent(&self, font: FontEdges) -> f32 {
        let top = match self.align {
            VerticalAlign::Baseline => self.height,
            VerticalAlign::Middle => (self.height + font.x_height) / 2.0,
            VerticalAlign::TextTop => font.ascent,
            VerticalAlign::TextBottom => self.height - font.descent,
        };
        top + self.shift
    }
}

/// An inline image's geometry: the box its paragraph reserves, and where
/// the image's own border box stands in it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InlineImage {
    /// The reserved margin box.
    pub reserved: InlineBox,
    /// The border box's offset from the margin box's top left.
    pub offset: (f32, f32),
    /// The border box's size.
    pub size: (f32, f32),
    /// Its padding and border widths, each left, top, right, bottom.
    pub(crate) inset: ([f32; 4], [f32; 4]),
}

/// The box an image inside a paragraph takes, from its rows and its natural
/// size: a length `width` or `height` is the content box's (or the border
/// box's, under `box-sizing: border-box`); an `auto` one follows the other by
/// the ratio (`aspect-ratio`, else the natural one), else is the natural
/// size, else zero, as an `<img>` is before it loads. Percentages and the
/// min and max sizes are not read here (LLP 1011.002 §4).
pub(crate) fn inline_image(arena: &NodeArena, slot: u32) -> InlineImage {
    use taffy::style::CompactLength;
    let style = crate::style::taffy_style(arena, slot);
    let length = |raw: CompactLength| {
        (raw.tag() == CompactLength::LENGTH_TAG)
            .then(|| raw.value())
            .filter(|v| v.is_finite())
    };
    let edge = |raw: CompactLength| length(raw).unwrap_or(0.0).max(0.0);
    let pad = &style.padding;
    let border = &style.border;
    let margin = &style.margin;
    let inset_w = edge(pad.left.into_raw())
        + edge(pad.right.into_raw())
        + edge(border.left.into_raw())
        + edge(border.right.into_raw());
    let inset_h = edge(pad.top.into_raw())
        + edge(pad.bottom.into_raw())
        + edge(border.top.into_raw())
        + edge(border.bottom.into_raw());
    let content = |given: Option<f32>, inset: f32| {
        given.map(|v| match style.box_sizing {
            taffy::style::BoxSizing::BorderBox => (v - inset).max(0.0),
            taffy::style::BoxSizing::ContentBox => v.max(0.0),
        })
    };
    let width = content(length(style.size.width.into_raw()), inset_w);
    let height = content(length(style.size.height.into_raw()), inset_h);
    let natural = arena.intrinsic(slot);
    let valid = |r: &f32| r.is_finite() && *r > 0.0;
    // An authored ratio relates the border box under `box-sizing:
    // border-box` (unless `auto <ratio>`), the natural one the content box.
    let authored = style.aspect_ratio.filter(valid);
    let (pw, ph) = match style.box_sizing {
        taffy::style::BoxSizing::BorderBox
            if authored.is_some() && !style.aspect_ratio_content_box =>
        {
            (inset_w, inset_h)
        }
        _ => (0.0, 0.0),
    };
    let ratio = authored.or_else(|| {
        natural
            .filter(|n| n.1 > 0.0)
            .map(|(w, h)| w / h)
            .filter(valid)
    });
    let across = |h: f32, r: f32| ((h + ph) * r - pw).max(0.0);
    let down = |w: f32, r: f32| ((w + pw) / r - ph).max(0.0);
    let (w, h) = match (width, height) {
        (Some(w), Some(h)) => (w, h),
        (Some(w), None) => (
            w,
            ratio.map_or(natural.map_or(0.0, |n| n.1), |r| down(w, r)),
        ),
        (None, Some(h)) => (
            ratio.map_or(natural.map_or(0.0, |n| n.0), |r| across(h, r)),
            h,
        ),
        (None, None) => match natural {
            Some((nw, _)) => (nw, ratio.map_or(0.0, |r| down(nw, r))),
            None => (0.0, 0.0),
        },
    };
    let (ml, mr, mt, mb) = (
        length(margin.left.into_raw()).unwrap_or(0.0),
        length(margin.right.into_raw()).unwrap_or(0.0),
        length(margin.top.into_raw()).unwrap_or(0.0),
        length(margin.bottom.into_raw()).unwrap_or(0.0),
    );
    let size = (w + inset_w, h + inset_h);
    let s = arena.style(slot);
    let shift = match s.rare.baseline_shift.resolve(&arena.env_of(slot)) {
        crate::Dimension::Points(v) if v.is_finite() => v,
        _ => 0.0,
    };
    InlineImage {
        reserved: InlineBox {
            width: (ml + size.0 + mr).max(0.0),
            height: (mt + size.1 + mb).max(0.0),
            align: s.rare.vertical_align,
            shift,
        },
        offset: (ml, mt),
        size,
        inset: (
            [pad.left, pad.top, pad.right, pad.bottom].map(|d| edge(d.into_raw())),
            [border.left, border.top, border.right, border.bottom].map(|d| edge(d.into_raw())),
        ),
    }
}

/// The rows an inline run's edges are made of (`inline_edges`).
pub(crate) const EDGE_ROWS: [crate::generated::StyleId; 8] = {
    use crate::generated::StyleId;
    [
        StyleId::PaddingLeft,
        StyleId::PaddingRight,
        StyleId::BorderWidthLeft,
        StyleId::BorderWidthRight,
        StyleId::BorderStyleLeft,
        StyleId::BorderStyleRight,
        StyleId::MarginLeft,
        StyleId::MarginRight,
    ]
};

/// What the inline box a `text` inside a paragraph is takes along the line
/// beside its text (`TextRun::edges`): its padding, border and margin on
/// each inline side. A percentage is not read, as for an image (LLP 1011.002
/// §4).
pub(crate) fn inline_edges(arena: &NodeArena, slot: u32) -> crate::text::InlineEdges {
    let (inside, margin) = box_sides(arena, slot);
    crate::text::InlineEdges {
        start: inside.start + margin.start,
        end: inside.end + margin.end,
    }
}

/// How far the padding box of the inline box `slot` is stands out from its
/// text on each side: its own padding and border, and the room of the boxes
/// inside it that start with its first run or end with its last. Its
/// background covers it (CSS 2 §14.2, `background-clip: border-box`).
pub(crate) fn background_reach(arena: &NodeArena, slot: u32) -> crate::text::InlineEdges {
    let (inside, _) = box_sides(arena, slot);
    crate::text::InlineEdges {
        start: inside.start + inner(arena, slot, true),
        end: inside.end + inner(arena, slot, false),
    }
}

/// The room the boxes inside `slot` take at its first run (`first`) or its
/// last: down the chain of its first (last) child that makes a run.
fn inner(arena: &NodeArena, slot: u32, first: bool) -> f32 {
    if arena.node_type(slot) != crate::NodeType::Text
        || arena.props(slot).str(crate::PropId::Text).is_some()
    {
        return 0.0;
    }
    let children = arena.children(slot);
    let mut order: Box<dyn Iterator<Item = &u32>> = if first {
        Box::new(children.iter())
    } else {
        Box::new(children.iter().rev())
    };
    let Some(&child) = order.find(|c| makes_run(arena, **c)) else {
        return 0.0;
    };
    if arena.node_type(child) != crate::NodeType::Text {
        return 0.0;
    }
    let edges = inline_edges(arena, child);
    (if first { edges.start } else { edges.end }) + inner(arena, child, first)
}

/// Whether `slot` inside a paragraph makes at least one of its runs.
fn makes_run(arena: &NodeArena, slot: u32) -> bool {
    match arena.node_type(slot) {
        crate::NodeType::Text => {
            arena.props(slot).str(crate::PropId::Text).is_some()
                || arena.children(slot).iter().any(|c| makes_run(arena, *c))
        }
        crate::NodeType::Image => arena.is_run_image(slot),
        _ => false,
    }
}

/// An inline box's padding and border, and its margin, on each inline side,
/// as its taffy style resolves them, start and end by its paragraph's
/// direction, in which hosts lay its runs out: a box's own `direction`
/// reorders nothing without `unicode-bidi`, so its `padding-right` is on
/// its right in a left-to-right paragraph.
fn box_sides(arena: &NodeArena, slot: u32) -> (crate::text::InlineEdges, crate::text::InlineEdges) {
    use crate::generated::StyleId;
    use crate::text::InlineEdges;
    use taffy::style::CompactLength;
    // Most runs say none of these rows: no taffy style is made for them.
    let own = arena.style(slot);
    if !EDGE_ROWS.iter().any(|row| own.mask.has(*row)) {
        return (InlineEdges::default(), InlineEdges::default());
    }
    let style = crate::style::taffy_style(arena, slot);
    let length = |raw: CompactLength| {
        (raw.tag() == CompactLength::LENGTH_TAG)
            .then(|| raw.value())
            .filter(|v| v.is_finite())
            .unwrap_or(0.0)
    };
    let inside =
        |pad: CompactLength, border: CompactLength| length(pad).max(0.0) + length(border).max(0.0);
    let (left, right) = (
        inside(style.padding.left.into_raw(), style.border.left.into_raw()),
        inside(
            style.padding.right.into_raw(),
            style.border.right.into_raw(),
        ),
    );
    let (margin_left, margin_right) = (
        length(style.margin.left.into_raw()),
        length(style.margin.right.into_raw()),
    );
    let paragraph = arena.measure_owner(slot);
    let sides = |a: f32, b: f32| match arena
        .computed_source(paragraph, StyleId::Direction)
        .direction
    {
        crate::Direction::Rtl => InlineEdges { start: b, end: a },
        _ => InlineEdges { start: a, end: b },
    };
    (sides(left, right), sides(margin_left, margin_right))
}

impl crate::NodeRef<'_> {
    /// For a `text` inside a paragraph, how far its padding box stands out
    /// from its text at its first run and its last: what its background
    /// covers beside its glyphs (`TextRun::edges` is the room its line gives
    /// it, margins included). Zero for any other node.
    /// At its paragraph's fit scale, as its runs are (LLP 1116.001 D1.4).
    pub fn inline_background_reach(&self) -> crate::InlineEdges {
        if !self.arena.is_inline_run(self.slot) {
            return crate::InlineEdges::default();
        }
        background_reach(self.arena, self.slot).scaled(self.arena.fit_scale(self.slot))
    }
}
