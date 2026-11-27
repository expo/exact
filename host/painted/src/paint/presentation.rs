//! Paint metadata held until the display acknowledges a picture.
use super::*;

// Snapshots share paragraph and command data. Each picture keeps its owners
// until the display acknowledges its replacement.
pub(crate) struct Presentation {
    text: Rc<BTreeMap<exact_kernel::NodeKey, Rc<Paragraph>>>,
    fields: HashMap<ViewId, Rc<field::PaintedField>>,
    inline: HashMap<ViewId, Rc<inline_frame::PaintedInline>>,
    bars: Vec<scrollbar::Bar>,
    picture: Option<Rc<region::Picture>>,
    frame: Option<region::Published>,
}

impl Painter {
    #[cfg(target_os = "linux")]
    pub(crate) fn use_text_predecessor(&mut self, state: &Presentation) {
        self.accepted_text = state.text.clone();
    }

    #[cfg(any(target_os = "linux", target_os = "macos"))]
    pub(crate) fn discard_text(&mut self, state: &Presentation, previous: Option<&Presentation>) {
        self.numerals.discard(
            previous.map_or(&self.accepted_text, |p| &p.text),
            &state.text,
        );
    }

    pub(crate) fn presentation(&self) -> Presentation {
        Presentation {
            text: self.accepted_text.clone(),
            fields: self.field_text.clone(),
            inline: self.inline_text.clone(),
            bars: self.scrollbars.bars.clone(),
            picture: self.region_picture.clone(),
            frame: self.region_frame.as_ref().map(|f| region::Published {
                incarnation: f.incarnation.clone(),
                selection: f.selection.clone(),
            }),
        }
    }
    pub(crate) fn replace_presentation(&mut self, mut state: Presentation) -> Presentation {
        std::mem::swap(&mut self.accepted_text, &mut state.text);
        std::mem::swap(&mut self.field_text, &mut state.fields);
        std::mem::swap(&mut self.inline_text, &mut state.inline);
        std::mem::swap(&mut self.scrollbars.bars, &mut state.bars);
        std::mem::swap(&mut self.region_picture, &mut state.picture);
        std::mem::swap(&mut self.region_frame, &mut state.frame);
        state
    }
}
