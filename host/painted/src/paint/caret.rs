//! A field's caret and selection use the same visual lines as its text.
use super::{field::PaintedField, Rect4, Shape};
use exact_runner::FieldSelection;
use std::ops::Range;
use tiny_skia::Transform;

pub(super) const HIGHLIGHT: [[u8; 4]; 2] = [[0xb4, 0xd5, 0xfe, 0xff], [0x3f, 0x63, 0x8b, 0xff]];

impl PaintedField {
    pub(super) fn selection_rects(
        &self,
        selection: FieldSelection,
        clip_y: Range<f32>,
        scale: f32,
    ) -> Vec<Rect4> {
        // The selection and clip can each cover part of the same device pixel.
        let visible = self.transform.invert().map(|inverse| {
            let margin = (inverse.ky.abs() + inverse.sy.abs()) / scale;
            clip_y.start - margin..clip_y.end + margin
        });
        let mut rects = self
            .geometry
            .highlight(selection.start, selection.end, visible);
        for rect in &mut rects {
            *rect = self.place(*rect);
        }
        rects
    }
}

impl super::Painter {
    /// Clip a single text paint to all selected portions of the visual lines.
    pub(super) fn push_selection_clip(&mut self, rects: &[Rect4], ts: Transform) -> usize {
        use exact_kernel::svg::{
            scene::{Clip, ClipShape},
            Path, Seg,
        };
        if let [rect] = rects {
            self.backend.push_clip(&Shape::rect(*rect), ts);
            return 1;
        }
        let mut segments = Vec::with_capacity(5 * rects.len());
        for &(x, y, w, h) in rects {
            segments.extend([
                Seg::Move(x, y),
                Seg::Line(x + w, y),
                Seg::Line(x + w, y + h),
                Seg::Line(x, y + h),
                Seg::Close,
            ]);
        }
        let clip = Clip {
            shapes: vec![ClipShape {
                path: Path(segments),
                even_odd: false,
            }],
            then: None,
        };
        self.backend.push_svg_clip(&clip, ts)
    }

    pub(super) fn field_caret(
        &mut self,
        field: &PaintedField,
        color: [u8; 4],
        selection: FieldSelection,
        upstream: bool,
    ) {
        if selection.start != selection.end || self.caret_opacity == 0 {
            return;
        }
        let rect = field.caret_rect(crate::text::field::Cursor {
            offset: selection.start,
            upstream,
        });
        self.backend
            .fill(&Shape::rect(rect), self.caret_ink(color), field.transform);
    }

    pub(super) fn caret_ink(&self, mut ink: [u8; 4]) -> [u8; 4] {
        ink[3] = ((u16::from(ink[3]) * u16::from(self.caret_opacity) + 127) / 255) as u8;
        ink
    }

    /// A field's painted focus: a two-point ring in the accent
    /// colour over its chrome or bare outer edge (LLP 1104 D6–D7).
    pub(super) fn field_ring(&mut self, outer: &Shape, accent: [u8; 4], ts: Transform) {
        for part in super::border::border_fills(outer, [2.0; 4], [accent; 4]) {
            self.backend.fill_border(&part, ts);
        }
    }
}
