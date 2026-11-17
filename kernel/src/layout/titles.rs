//! The neutral route title's `1lh` is measured by the host, including an
//! empty title and `line-height: normal` (host/web/neutral.css).
use super::*;

impl LayoutTree {
    pub(super) fn prepare_titles(
        &mut self,
        root: NodeId,
        arena: &NodeArena,
        measurer: &mut dyn TextMeasurer,
    ) -> Result<(), LayoutError> {
        if self.neutral_titles.is_empty()
            && (crate::style::metrics::column() != crate::style::metrics::Column::Neutral
                || !arena.has_prop(crate::PropId::AccessibilityHeadingLevel))
        {
            return Ok(());
        }
        let mut styles = Vec::new();
        for (&node, &slot) in &self.slots {
            let title = arena.neutral_header_title(slot);
            if !title && !self.neutral_titles.contains(&node) {
                continue;
            }
            let mut at = Some(node);
            let mut visible = true;
            while let Some(n) = at {
                visible &= self
                    .taffy
                    .style(n)
                    .is_ok_and(|s| s.display != taffy::Display::None);
                if n == root {
                    break;
                }
                at = self.taffy.parent(n);
            }
            if !visible || at.is_none() {
                continue;
            }
            // Preserve temporary sizes used by auto-height measurement.
            // Only the line floor and block margins belong to this default.
            let authored = taffy_style(arena, slot);
            let mut style = self.taffy.style(node).expect("title node").clone();
            style.min_size.height = authored.min_size.height;
            style.margin.top = authored.margin.top;
            style.margin.bottom = authored.margin.bottom;
            if title {
                let text = arena.text_style(slot);
                let line = match text.line_height {
                    Some(line) => line,
                    None => {
                        let run = TextRun {
                            text: "M".into(),
                            style: text,
                            edges: Default::default(),
                            inline: None,
                        };
                        let mut paragraph =
                            crate::text::Paragraph::from_style(&crate::StyleProps::default());
                        paragraph.strut = text;
                        let request = TextMeasureRequest {
                            runs: std::slice::from_ref(&run),
                            paragraph,
                            atoms: Default::default(),
                            width: AxisOffer::MaxContent,
                            height: AxisOffer::MaxContent,
                            exclusions: &[],
                        };
                        let measured = measurer.measure(&request);
                        if !measured.is_valid() {
                            return Err(LayoutError::InvalidTextMetrics(arena.local_id(slot)));
                        }
                        measured.height
                    }
                };
                arena.neutral_title_layout(slot, line, &mut style);
            }
            styles.push((node, title, style));
        }
        for (node, title, style) in styles {
            if title {
                self.neutral_titles.insert(node);
            } else {
                self.neutral_titles.remove(&node);
            }
            self.set_style(node, style);
        }
        Ok(())
    }
}
