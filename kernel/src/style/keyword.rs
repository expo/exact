//! The size keywords of CSS Box Sizing 3 §3.2 and CSS Flexbox 1 §7.2.3.

/// A size keyword. In the inline axis each is a size measured from the box's
/// content; in the block axis each is the content's height at the box's
/// width, as `auto` is, but it stops `stretch`. Each gives the border box,
/// whatever `box-sizing` says.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SizeKeyword {
    /// `min-content`: the narrowest the content can be without overflowing
    /// where a line could have broken.
    MinContent,
    /// `max-content`: the content's width with no line broken except where
    /// it must be.
    MaxContent,
    /// `fit-content`: the space available, but no narrower than
    /// `min-content` and no wider than `max-content`.
    FitContent,
    /// `content`, on `flex-basis` only: the size the item's aspect ratio
    /// transfers from a definite cross size, else its `max-content` size.
    Content,
}

impl SizeKeyword {
    /// Every keyword, in wire order (kinds 26 to 29).
    pub const ALL: [SizeKeyword; 4] = [
        SizeKeyword::MinContent,
        SizeKeyword::MaxContent,
        SizeKeyword::FitContent,
        SizeKeyword::Content,
    ];

    /// The keyword as CSS writes it.
    pub fn css(self) -> &'static str {
        match self {
            SizeKeyword::MinContent => "min-content",
            SizeKeyword::MaxContent => "max-content",
            SizeKeyword::FitContent => "fit-content",
            SizeKeyword::Content => "content",
        }
    }

    /// The keyword `text` names, ASCII case-insensitively, after CSS white
    /// space.
    pub fn parse(text: &str) -> Option<SizeKeyword> {
        let text = text.trim_matches(['\t', '\n', '\u{c}', '\r', ' ']);
        Self::ALL
            .into_iter()
            .find(|k| text.eq_ignore_ascii_case(k.css()))
    }

    /// The engine's keyword.
    pub(crate) fn taffy(self) -> taffy::style::Dimension {
        match self {
            SizeKeyword::MinContent => taffy::style::Dimension::min_content(),
            SizeKeyword::MaxContent => taffy::style::Dimension::max_content(),
            SizeKeyword::FitContent => taffy::style::Dimension::fit_content(),
            SizeKeyword::Content => taffy::style::Dimension::content(),
        }
    }
}

impl super::Dimension {
    /// The value, or `auto` where it is a keyword: what a keyword means
    /// where the kernel's own layout takes no content size (a table cell,
    /// SVG geometry).
    pub(crate) fn unless_keyword(self) -> super::Dimension {
        match self {
            super::Dimension::Keyword(_) => super::Dimension::Auto,
            d => d,
        }
    }

    /// Whether the size comes from the box's content: `auto` or a keyword
    /// (LLP 1001.001 D5). A keyword is not a definite length.
    pub fn is_content_sized(&self) -> bool {
        matches!(self, super::Dimension::Auto | super::Dimension::Keyword(_))
    }
}
