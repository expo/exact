//! SVG `text` and `tspan` resolved into chunks of runs: where each chunk
//! starts, how it anchors and which baseline sits on its `y`, and per run the
//! string (white space collapsed), its font and its paint. The host shapes:
//! only it has the fonts (LLP 1055.000 D11).
//!
//! Stage 5 positions by a chunk's first `x`/`y` and each run's first
//! `dx`/`dy`; per-character lists, `rotate`, `textLength` and `textPath` are
//! later stages.

use super::{cascade, Resolver, ShapePaint};
use crate::generated::{
    DominantBaseline, NodeType, PropId, StrokeLinecap, StrokeLinejoin, StyleProps, TextAnchor,
    Visibility,
};
use crate::id::ViewId;
use crate::kernel::NodeRef;
use crate::svg::length::{Length, Viewport};
use crate::svg::server::{resolve, Resolved};
use crate::svg::Paint;
use crate::text::TextStyle;

/// A `text` element, resolved.
#[derive(Debug, Clone, PartialEq)]
pub struct TextItem {
    /// Its chunks, each starting at an absolute position.
    pub chunks: Vec<TextChunk>,
}

impl TextItem {
    /// A box that holds every glyph, standing in for the object bounding box
    /// when a `filter` or a `mask` sizes its region by it: the box is the
    /// glyph cells' (SVG 2 §8.10), whose advances only the host measures.
    /// Each character is taken as at most an em and a half wide and each
    /// glyph within an em and a half of its baseline, whatever the anchor
    /// and baseline, so the region is larger than the box, never smaller.
    pub(super) fn reach(&self) -> super::Rect4 {
        // Where a chunk can start on each axis, as a range: one that
        // continues from the last can start wherever that one could end.
        let (mut x, mut y) = ((0.0f32, 0.0f32), (0.0f32, 0.0f32));
        let mut out: Option<super::Rect4> = None;
        for chunk in &self.chunks {
            if let Some(v) = chunk.x {
                x = (v, v);
            }
            if let Some(v) = chunk.y {
                y = (v, v);
            }
            let (mut advance, mut shift, mut em, mut stroke) = (0.0f32, 0.0f32, 0.0f32, 0.0f32);
            for run in &chunk.runs {
                let size = run.style.font_size.max(0.0);
                let chars = run.text.chars().count() as f32;
                advance += run.dx.abs() + chars * (1.5 * size + run.style.letter_spacing.abs());
                shift += run.dy.abs();
                em = em.max(size);
                if run.stroke.is_some() {
                    stroke = stroke.max(run.width / 2.0);
                }
            }
            // An anchor moves a chunk back by up to its advance.
            x = (x.0 - advance, x.1 + advance);
            y = (y.0 - shift, y.1 + shift);
            let reach = 1.5 * em + stroke;
            let r = (
                x.0 - stroke,
                y.0 - reach,
                x.1 - x.0 + 2.0 * stroke,
                y.1 - y.0 + 2.0 * reach,
            );
            out = Some(out.map_or(r, |o| super::union(o, r)));
        }
        out.unwrap_or_default()
    }
}

/// Runs laid out one after another from one absolute position.
#[derive(Debug, Clone, PartialEq)]
pub struct TextChunk {
    /// Where the chunk starts; `None` continues from the previous chunk's
    /// end on that axis.
    pub x: Option<f32>,
    /// The baseline's `y`, or `None` to continue.
    pub y: Option<f32>,
    /// `text-anchor`: the chunk's advance is shifted by none, half, or all.
    pub anchor: TextAnchor,
    /// `dominant-baseline`: which of the font's baselines sits on `y`.
    pub baseline: DominantBaseline,
    /// Its runs.
    pub runs: Vec<TextRun>,
}

/// One run: a string in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct TextRun {
    /// The node that carries it (`text` or `tspan`).
    pub id: ViewId,
    /// The string, white space collapsed.
    pub text: String,
    /// Shifts before the run's first glyph, in user units.
    pub dx: f32,
    /// Shifts before the run's first glyph, in user units.
    pub dy: f32,
    /// The font.
    pub style: TextStyle,
    /// Fill, or none.
    pub fill: Option<ShapePaint>,
    /// Stroke, or none.
    pub stroke: Option<ShapePaint>,
    /// `stroke-width`.
    pub width: f32,
    /// `stroke-linecap`.
    pub cap: StrokeLinecap,
    /// `stroke-linejoin`.
    pub join: StrokeLinejoin,
    /// `stroke-miterlimit`.
    pub miter: f32,
}

/// The first number of an SVG length list, against `basis`.
fn first(text: Option<&str>, basis: f32) -> Option<f32> {
    let t = text?;
    let word = t
        .split(|c: char| c == ',' || c.is_ascii_whitespace())
        .find(|w| !w.is_empty())?;
    Length::parse(word).map(|l| l.resolve(basis))
}

/// CSS `white-space: normal` for SVG text: runs of white space become one
/// space; the chunk's edges are trimmed by the caller.
fn collapse(text: &str, trailing_space: &mut bool) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        if c.is_whitespace() {
            if !*trailing_space {
                out.push(' ');
                *trailing_space = true;
            }
        } else {
            out.push(c);
            *trailing_space = false;
        }
    }
    out
}

impl Resolver<'_, '_> {
    /// A `text` element's chunks, or `None` when it shows nothing.
    pub(super) fn text(
        &mut self,
        node: &NodeRef<'_>,
        style: &StyleProps,
        vp: Viewport,
    ) -> Option<TextItem> {
        let mut chunks = vec![TextChunk {
            x: Some(first(node.props.str(PropId::TextX), vp.width).unwrap_or(0.0)),
            y: Some(first(node.props.str(PropId::TextY), vp.height).unwrap_or(0.0)),
            anchor: style.text_anchor,
            baseline: style.dominant_baseline,
            runs: Vec::new(),
        }];
        // A leading space never shows: the chunk starts trimmed.
        let mut space = true;
        self.runs(node, style, vp, &mut chunks, &mut space, true);
        for chunk in &mut chunks {
            if let Some(last) = chunk.runs.iter_mut().rev().find(|r| !r.text.is_empty()) {
                if last.text.ends_with(' ') {
                    last.text.pop();
                }
            }
            chunk
                .runs
                .retain(|r| !r.text.is_empty() || r.dx != 0.0 || r.dy != 0.0);
        }
        chunks.retain(|c| !c.runs.is_empty());
        (!chunks.is_empty()).then_some(TextItem { chunks })
    }

    fn runs(
        &mut self,
        node: &NodeRef<'_>,
        style: &StyleProps,
        vp: Viewport,
        chunks: &mut Vec<TextChunk>,
        space: &mut bool,
        root: bool,
    ) {
        if style.display == crate::generated::Display::None {
            return;
        }
        // A `tspan` with its own `x` or `y` starts a chunk there.
        let (x, y) = (
            first(node.props.str(PropId::TextX), vp.width),
            first(node.props.str(PropId::TextY), vp.height),
        );
        if !root && (x.is_some() || y.is_some()) {
            chunks.push(TextChunk {
                x,
                y,
                anchor: style.text_anchor,
                baseline: style.dominant_baseline,
                runs: Vec::new(),
            });
            *space = true;
        }
        let dx = first(node.props.str(PropId::TextDx), vp.width).unwrap_or(0.0);
        let dy = first(node.props.str(PropId::TextDy), vp.height).unwrap_or(0.0);
        let hidden = style.visibility != Visibility::Visible;
        let paint = |p: &Paint, opacity: f32| -> Option<ShapePaint> {
            if hidden {
                return None;
            }
            let opacity = opacity.clamp(0.0, 1.0);
            let solid = |color, opacity| {
                Some(ShapePaint {
                    color,
                    opacity,
                    server: None,
                    pattern: None,
                })
            };
            match p {
                Paint::None => None,
                Paint::CurrentColor => solid(style.text_color, opacity),
                Paint::Color(c) => solid(*c, opacity),
                // As a shape's paint resolves it, with no box: the host
                // measures the glyphs, so a server in bounding-box units
                // paints nothing here, and a pattern is not painted on text.
                Paint::Url(id, _) => {
                    let target = self.kernel.resolve_id(node.id, id);
                    if target
                        .and_then(|t| self.kernel.node(t))
                        .is_some_and(|t| t.node_type == NodeType::SvgPattern)
                    {
                        return None;
                    }
                    match resolve(self.kernel, node.id, id, None, vp) {
                        Resolved::Server(server) if self.budget.cost(server.stops.len() as u64) => {
                            Some(ShapePaint {
                                color: style.text_color,
                                opacity,
                                server: Some(Box::new(server)),
                                pattern: None,
                            })
                        }
                        Resolved::Server(_) | Resolved::Nothing => None,
                        Resolved::Color(color, stop) => solid(color, opacity * stop),
                        // A missing server paints the fallback, or nothing.
                        Resolved::Fallback => match p.fallback()? {
                            Paint::CurrentColor => solid(style.text_color, opacity),
                            Paint::Color(c) => solid(c, opacity),
                            _ => None,
                        },
                    }
                }
            }
        };
        // Bold Text (LLP 1115.000 D12.3), on the run's own weight and family.
        let mut drawn = TextStyle::from_style(style);
        node.arena.embolden(node.slot, &mut drawn);
        let run = |text: String| TextRun {
            id: node.id,
            text,
            dx,
            dy,
            style: drawn,
            fill: paint(&style.fill, style.fill_opacity),
            stroke: paint(&style.stroke, style.stroke_opacity).filter(|_| style.stroke_width > 0.0),
            width: style.stroke_width.max(0.0),
            cap: style.stroke_linecap,
            join: style.stroke_linejoin,
            miter: style.stroke_miterlimit.max(1.0),
        };
        if let Some(text) = node.props.str(PropId::Text) {
            let text = collapse(text, space);
            chunks.last_mut().expect("a chunk").runs.push(run(text));
            return;
        }
        // A run with no string of its own still carries its shift.
        if dx != 0.0 || dy != 0.0 {
            chunks
                .last_mut()
                .expect("a chunk")
                .runs
                .push(run(String::new()));
        }
        for child in node.children() {
            let Some(c) = self.kernel.node(child) else {
                continue;
            };
            if c.node_type != NodeType::SvgTSpan {
                continue;
            }
            let mut cs = cascade(&c, style);
            self.present(&c, &mut cs);
            self.runs(&c, &cs, vp, chunks, space, false);
        }
    }
}
