//! Inline `frame` and textarea `rangeFrame` from the acknowledged picture.
//! Scroll extents use the current layout so a resize action sees its new dimensions.
use super::*;
use crate::paint::{field::PaintedField, inline_frame::PaintedInline};
use exact_runner::geometry::{GeometryAnswer, InlineGeometry};
use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::rc::Rc;

/// The current runner reads only its acknowledged picture. `session` counts
/// runners, which can reuse view ids; `shown` identifies the picture's runner.
/// Perspective-projected fields are retained but never read.
#[derive(Debug, Default)]
pub(crate) struct Painted {
    text: RefCell<std::rc::Weak<RefCell<TextEngine>>>,
    fields: RefCell<HashMap<ViewId, (Rc<PaintedField>, bool)>>,
    inline: RefCell<HashMap<ViewId, Rc<PaintedInline>>>,
    session: Cell<u64>,
    pub(crate) shown: Cell<u64>,
}

impl Painted {
    /// The runner painting now.
    pub(crate) fn session(&self) -> u64 {
        self.session.get()
    }
}

impl std::fmt::Debug for PaintedField {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PaintedField")
    }
}

impl InlineGeometry for Painted {
    fn scroll_metrics(
        &self,
        kernel: &exact_kernel::Kernel,
        view: ViewId,
        (x, y): (f64, f64),
    ) -> Option<[f64; 6]> {
        let node = kernel.node(view)?;
        if !crate::paint::paints(kernel, view, None) {
            return None;
        }
        let text = self.text.borrow().upgrade()?;
        let fields = self.fields.borrow();
        let (mx, my) = if crate::fields::hosts(&node) {
            let shell = crate::fields::hosting()?;
            let (width, height) = PaintedField::editor_size(kernel, &node);
            let style = node.text_style();
            let face = crate::fields::face(&mut text.borrow_mut(), &style, true, shell.scale);
            let value = node.props.str(exact_kernel::PropId::Value).unwrap_or("");
            let shown = if value.is_empty() {
                node.props
                    .str(exact_kernel::PropId::Placeholder)
                    .unwrap_or("")
            } else {
                value
            };
            let [w, h, _] = shell
                .metrics
                .measure(&face, shown, Some(width * shell.scale));
            (
                (w / shell.scale - width).max(0.),
                (h / shell.scale - height).max(0.),
            )
        } else {
            PaintedField::textarea_scroll_size(
                fields.get(&view).map(|(f, _)| f.as_ref()),
                &mut text.borrow_mut(),
                kernel,
                &node,
            )
        };
        let (w, h) = (f64::from(node.frame.width), f64::from(node.frame.height));
        let (mx, my) = (f64::from(mx), f64::from(my));
        Some([x.clamp(0., mx), y.clamp(0., my), w + mx, h + my, w, h])
    }

    fn frame(&self, view: ViewId) -> GeometryAnswer {
        if cfg!(target_os = "android") {
            return GeometryAnswer::UNAVAILABLE;
        }
        let inline = self.inline.borrow();
        let Some((x, y, width, height)) = inline.get(&view).and_then(|p| p.frame(view)) else {
            return GeometryAnswer::UNAVAILABLE;
        };
        GeometryAnswer {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
            provisional: false,
            unavailable: false,
        }
    }

    fn range_frame(&self, view: ViewId, start: u32, end: u32) -> GeometryAnswer {
        if cfg!(target_os = "android") {
            return GeometryAnswer::UNAVAILABLE;
        }
        let fields = self.fields.borrow();
        let Some((x, y, width, height)) = fields
            .get(&view)
            .filter(|(_, projected)| !projected)
            .and_then(|(field, _)| field.range_frame(start, end))
        else {
            return GeometryAnswer::UNAVAILABLE;
        };
        GeometryAnswer {
            x: f64::from(x),
            y: f64::from(y),
            width: f64::from(width),
            height: f64::from(height),
            provisional: false,
            unavailable: false,
        }
    }
}

#[cfg(test)]
thread_local! { static PUBLICATION_BOX_VISITS: Cell<usize> = const { Cell::new(0) }; }

impl<D: DataSource> Presenter<D> {
    /// A new runner starts without the previous session’s published frames.
    pub(crate) fn read_ranges(&mut self) {
        let ranges = &self.ranges;
        ranges.session.set(ranges.session.get() + 1);
        ranges.fields.borrow_mut().clear();
        ranges.inline.borrow_mut().clear();
        *ranges.text.borrow_mut() = Rc::downgrade(&self.text);
        self.host
            .runner_mut()
            .set_inline_geometry(Some(ranges.clone()));
    }

    /// The text beside the boxes now shown, when they are this
    /// runner's: a hosted field (the shell's own) is never painted here.
    pub(crate) fn publish_ranges(&self) {
        let mut fields = self.ranges.fields.borrow_mut();
        fields.clear();
        let mut inline = self.ranges.inline.borrow_mut();
        inline.clear();
        if self.ranges.shown.get() != self.ranges.session.get() {
            return;
        }
        // Text with no box beside it is not read: a failed paint can drop
        // the boxes, leaving its placement unknown.
        if self.brush.inline_text.is_empty() && self.brush.field_text.is_empty() {
            return;
        }
        for b in &self.boxes {
            #[cfg(test)]
            PUBLICATION_BOX_VISITS.set(PUBLICATION_BOX_VISITS.get() + 1);
            if let Some(paragraph) = self.brush.inline_text.get(&b.id).filter(|_| !b.projected()) {
                inline.extend(paragraph.views().map(|view| (view, paragraph.clone())));
            }
            if let Some(field) = self.brush.field_text.get(&b.id) {
                fields
                    .entry(b.id)
                    .or_insert_with(|| (field.clone(), b.projected()));
            }
        }
    }
}

#[cfg(test)]
#[path = "range_frame_tests.rs"]
mod range_frame_tests;

#[cfg(test)]
#[path = "inline_frame_tests.rs"]
mod inline_frame_tests;
