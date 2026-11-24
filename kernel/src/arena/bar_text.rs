//! A readout a navigation bar hosts takes the bar's type for what it leaves
//! unsaid (LLP 1141.004 D3): its text's face, size, weight and style are a
//! plain bar item's title's, which its host gives (`Env::bar_item_font`), its
//! line height the face's own, its colour the label's, with no spacing or
//! case added. A row written on the readout or inside it wins; one it
//! would only inherit from the header or the page does not reach it, as a
//! native control's does not (LLP 1115 D4).
use super::*;

/// A hosted readout's start, from `Env::bar_item_font`, and the readouts
/// whose bar type changed since the host last asked
/// (`Kernel::take_bar_text_flips`).
#[derive(Clone, Debug, Default)]
pub(crate) struct BarText {
    pub(crate) start: Option<Box<StyleProps>>,
    pub(crate) flips: Vec<ViewId>,
}

/// A readout's bar type given, changed or taken back (LLP 1141.004 D3): its
/// text measured again, a symbol it is too, and its host told to send it
/// again (`Kernel::take_bar_text_flips`).
pub(crate) fn bar_type_changed(
    arena: &mut NodeArena,
    layout: &mut dyn crate::layout::LayoutMirror,
    slot: u32,
) {
    crate::txn::control_env_changed(arena, layout, slot);
    let mut receipt = crate::CommitReceipt::default();
    crate::txn::symbol_font_changed(arena, slot, StyleMask::TEXT, &mut receipt);
    arena.bar_text_flipped(slot);
}

/// The rows a hosted readout's start says.
pub(crate) fn rows() -> StyleMask {
    let mut rows = StyleMask::EMPTY;
    for id in [
        StyleId::FontFamily,
        StyleId::FontSize,
        StyleId::FontWeight,
        StyleId::FontStyle,
        StyleId::LineHeight,
        StyleId::LetterSpacing,
        StyleId::TextTransform,
        StyleId::TextColor,
    ] {
        rows.set(id);
    }
    rows
}

impl NodeArena {
    pub(crate) fn refresh_bar_start(&mut self) {
        self.bar_text.start = self.env.bar_item_font.as_ref().map(|font| {
            let mut start = StyleProps::default();
            start.font_family = font.family_id;
            start.font_size = font.size;
            start.font_weight = font.weight;
            start.font_style = font.style;
            start.line_height = crate::LineHeight::Normal;
            start.text_color = crate::ColorValue::Role(
                crate::style::roles::role("-exact-label").expect("schema role"),
            );
            start.mask = rows();
            Box::new(start)
        });
    }

    /// Whether `slot` is a readout a navigation bar hosts: its host gave it a
    /// block and shows it as the bar's item (`Kernel::set_bar_item`).
    pub(crate) fn hosted_readout(&self, slot: u32) -> bool {
        self.blocks.held(slot).is_some_and(|b| b.bar_item)
    }

    /// Tell the host to send `slot`'s subtree again (`Kernel::take_bar_text_flips`).
    pub(crate) fn bar_text_flipped(&mut self, slot: u32) {
        let view = self.local_id(slot);
        if !self.bar_text.flips.contains(&view) {
            self.bar_text.flips.push(view);
        }
    }

    /// The bar's rows for a hosted readout, on a host that gives them.
    pub(crate) fn bar_text_start(&self, slot: u32) -> Option<&StyleProps> {
        let start = self.bar_text.start.as_deref()?;
        self.hosted_readout(slot).then_some(start)
    }
}
