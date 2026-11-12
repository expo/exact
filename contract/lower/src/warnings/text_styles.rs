use contract_syntax::{Attr, Span};

/// Effective text sizing and native control boundaries from lowering. A reused
/// source site whose own style differs between instances stays unknown. The tag
/// distinguishes a button from the title synthesized at the same source span.
pub(crate) type TextStyles = std::collections::BTreeMap<(Span, u32, String), Option<TextStyle>>;

#[derive(Clone, PartialEq)]
pub(crate) struct TextStyle {
    pub(crate) native: Option<bool>,
    pub(crate) font: Option<Attr>,
    pub(crate) layout: Vec<Attr>,
    /// The lowered parent includes list cards and swipe scrollers absent from the source.
    pub(crate) parent_horizontal_flex: Option<bool>,
}
