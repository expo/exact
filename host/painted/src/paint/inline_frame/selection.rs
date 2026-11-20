//! A selectable paragraph keeps its source map with the accepted picture. The first
//! selection builds its UTF-16 hit geometry from exactly the glyphs that picture drew.
use super::*;
use crate::text::field::Geometry;
use exact_kernel::{NodeKey, UserSelect};
use std::cell::OnceCell;

pub(super) struct Source {
    pub(super) key: NodeKey,
    owners: Vec<ViewId>,
    shaped: OnceCell<Shaped>,
    authored: Option<Rc<Authored>>,
}

pub(crate) struct Shaped {
    pub value: Rc<str>,
    pub runs: Vec<(Range<u32>, ViewId)>,
    pub authored: Option<Rc<Authored>>,
    geom: Geometry,
}

pub(crate) struct Authored {
    pub value: String,
    pub runs: Vec<(Range<u32>, ViewId)>,
    pub map: exact_textflow::Collapsed,
}

impl Authored {
    fn capture(
        kernel: &Kernel,
        owner: &NodeRef<'_>,
        paragraph: &Paragraph,
        palette: &[RunPaint],
    ) -> Option<Self> {
        if owner.props.str(PropId::Markup) == Some("markdown")
            || paragraph.runs().len() != palette.len()
        {
            return None;
        }
        let texts: Vec<_> = palette
            .iter()
            .map(|r| {
                kernel
                    .node(r.source)
                    .and_then(|n| n.props.str(PropId::Text))
                    .unwrap_or("")
            })
            .collect();
        if texts
            .iter()
            .zip(paragraph.runs())
            .all(|(s, r)| *s == r.text)
        {
            return None;
        }
        let white_space = owner.computed_row(StyleId::WhiteSpace, |style| style.white_space);
        let mut map = exact_textflow::collapse(&texts, white_space.model())?;
        if !map
            .runs
            .iter()
            .zip(paragraph.runs())
            .all(|(s, r)| *s == r.text)
        {
            return None;
        }
        map.runs.clear();
        let mut at = 0;
        let runs = texts
            .iter()
            .zip(palette)
            .map(|(text, r)| {
                let end = at + text.encode_utf16().count() as u32;
                let range = at..end;
                at = end;
                (range, r.source)
            })
            .collect();
        Some(Self {
            value: texts.concat(),
            map,
            runs,
        })
    }
}

impl Source {
    pub(super) fn capture(
        kernel: &Kernel,
        owner: &NodeRef<'_>,
        paragraph: &Paragraph,
        palette: &[RunPaint],
        inline: impl Iterator<Item = ViewId>,
    ) -> Option<Self> {
        let admits =
            |n: &NodeRef<'_>| matches!(n.style.user_select, UserSelect::Text | UserSelect::All);
        let above =
            std::iter::successors(Some(*owner), |n| n.parent.and_then(|id| kernel.node(id)));
        if !above
            .chain(inline.filter_map(|id| kernel.node(id)))
            .any(|n| admits(&n))
        {
            return None;
        }
        Some(Self {
            key: owner.key,
            owners: palette.iter().map(|r| r.source).collect(),
            shaped: OnceCell::new(),
            authored: Authored::capture(kernel, owner, paragraph, palette).map(Rc::new),
        })
    }
}

impl PaintedInline {
    pub(crate) fn selectable(&self) -> Option<(NodeKey, &Shaped)> {
        let source = self.captured.selection.as_ref()?;
        let p = &self.captured.paragraph;
        let shaped = source.shaped.get_or_init(|| {
            let mut value = String::new();
            let mut runs = Vec::new();
            let mut start = 0;
            for (i, run) in p.runs().iter().enumerate() {
                if run.inline.is_some() || run.text.is_empty() {
                    continue;
                }
                let end = start + run.text.encode_utf16().count() as u32;
                // Markdown expands the owning text's single authored run.
                if let Some(&id) = source.owners.get(i).or(source.owners.first()) {
                    runs.push((start..end, id));
                }
                value.push_str(&run.text);
                start = end;
            }
            Shaped {
                geom: Geometry::new(p, &value, false, 0., 0., None),
                value: value.into(),
                authored: source.authored.clone(),
                runs,
            }
        });
        Some((source.key, shaped))
    }

    pub(crate) fn selection_hit(&self, x: f32, y: f32, character: bool) -> Option<u32> {
        let (_, shaped) = self.selectable()?;
        let mut point = Point::from_xy(x, y);
        self.transform.invert()?.map_point(&mut point);
        let bottoms = self.captured.paragraph.line_bottoms();
        let mut nearest = None;
        for (lines, (left, top)) in self.placements.iter() {
            let last = lines.end.min(bottoms.len()).checked_sub(1)?;
            let y0 = lines.start.checked_sub(1).map_or(0., |i| bottoms[i]);
            let y1 = bottoms[last];
            let local_y = (point.y - top).clamp(y0, (y1 - 0.001).max(y0));
            let x = point.x - left;
            let cursor = shaped.geom.hit(x, local_y);
            let (cx, _, _, _) = shaped.geom.caret(cursor);
            let distance = (point.y - top - local_y).hypot(cx - x);
            if nearest.is_none_or(|(d, _)| distance < d) {
                let offset = if character {
                    shaped.geom.hit_character(x, local_y)
                } else {
                    cursor.offset
                };
                nearest = Some((distance, offset));
            }
        }
        nearest.map(|(_, offset)| offset)
    }

    fn selection_rect(&self, line: usize, (x, y, w, h): Rect4) -> Option<Rect4> {
        let (_, (left, top)) = self
            .placements
            .iter()
            .find(|(lines, _)| lines.contains(&line))?;
        Some(bbox(self.transform, (x + left, y + top, w, h)))
    }

    pub(crate) fn selection_caret(&self, offset: u32, trailing: bool) -> Option<(Rect4, bool)> {
        let geom = &self.selectable()?.1.geom;
        let (line, rect, rtl) = geom.selection_edge(offset, trailing);
        Some((self.selection_rect(line, rect)?, rtl))
    }

    pub(crate) fn selection_marks(&self, range: Range<u32>, clip: Rect4) -> Vec<Rect4> {
        let Some((_, shaped)) = self.selectable() else {
            return Vec::new();
        };
        let bottoms = self.captured.paragraph.line_bottoms();
        shaped
            .geom
            .highlight(range.start, range.end, None)
            .into_iter()
            .filter_map(|r| {
                let line = bottoms.partition_point(|&bottom| bottom <= r.1);
                let shown = intersect(self.selection_rect(line, r)?, clip);
                (shown.2 > 0. && shown.3 > 0.).then_some(shown)
            })
            .collect()
    }
}
