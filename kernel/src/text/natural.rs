//! Natural text (LLP 1060.000 D3): where a paragraph's direction is unsaid
//! on a host whose platform lays out in its own direction (the Mac), each
//! paragraph takes its base direction from its first strong character, as
//! AppKit's `NSWritingDirection.natural` does, and the layout direction
//! where it has none. Alignment stays on the layout direction.
use crate::arena::NodeArena;
use crate::{Direction, NodeType, StyleId};
use icu_properties::{props::BidiClass, CodePointMapData};

/// Whether the leaf at `slot` is natural: the host said its text is, and no
/// node from it up to the root declares `direction`.
pub(crate) fn is_natural(arena: &NodeArena, slot: u32) -> bool {
    arena.natural_text
        && matches!(arena.node_type(slot), NodeType::Text | NodeType::TextInput)
        && arena.inherited_source(slot, StyleId::Direction).is_none()
}

/// Each paragraph's base direction, in order, appended to `out`. `text` is
/// the paragraph's text after `white-space` processing; it splits at UAX #9
/// paragraph separators (class B), a carriage return and line feed counting
/// as one, so text ending in a separator ends with an empty paragraph. A
/// paragraph's base is UAX #9 P2 and P3: its first character of class L, R
/// or AL outside an isolate; `fallback` (the layout direction) where it has
/// none, the contextual default AppKit takes (HL1).
pub fn paragraph_bases(text: &str, fallback: Direction, out: &mut Vec<Direction>) {
    let classes = CodePointMapData::<BidiClass>::new();
    let (mut base, mut isolates, mut after_cr) = (None, 0u32, false);
    for c in text.chars() {
        let class = classes.get(c);
        if class == BidiClass::ParagraphSeparator {
            if !(after_cr && c == '\n') {
                out.push(base.take().unwrap_or(fallback));
                isolates = 0;
            }
            after_cr = c == '\r';
            continue;
        }
        after_cr = false;
        if base.is_some() {
            continue;
        }
        match class {
            BidiClass::FirstStrongIsolate
            | BidiClass::LeftToRightIsolate
            | BidiClass::RightToLeftIsolate => isolates += 1,
            BidiClass::PopDirectionalIsolate => isolates = isolates.saturating_sub(1),
            BidiClass::LeftToRight if isolates == 0 => base = Some(Direction::Ltr),
            BidiClass::RightToLeft | BidiClass::ArabicLetter if isolates == 0 => {
                base = Some(Direction::Rtl)
            }
            _ => {}
        }
    }
    out.push(base.unwrap_or(fallback));
}

#[cfg(test)]
mod tests {
    use super::*;
    use Direction::{Ltr, Rtl};

    fn bases(text: &str, fallback: Direction) -> Vec<Direction> {
        let mut out = Vec::new();
        paragraph_bases(text, fallback, &mut out);
        out
    }

    #[test]
    fn a_paragraph_takes_its_first_strong_character() {
        assert_eq!(bases("مرحبا بالعالم.", Ltr), [Rtl]);
        assert_eq!(bases("Hello, مرحبا.", Rtl), [Ltr]);
        assert_eq!(bases("“שלום” said Dan", Ltr), [Rtl]);
    }

    #[test]
    fn each_paragraph_is_resolved_on_its_own() {
        assert_eq!(bases("مرحبا.\nHello.", Ltr), [Rtl, Ltr]);
        assert_eq!(bases("Hello.\u{2029}مرحبا.", Ltr), [Ltr, Rtl]);
        // A carriage return and line feed are one separator.
        assert_eq!(bases("مرحبا.\r\nHello.", Ltr), [Rtl, Ltr]);
        assert_eq!(bases("مرحبا.\r\rHello.", Ltr), [Rtl, Ltr, Ltr]);
        assert_eq!(bases("مرحبا.\n", Ltr), [Rtl, Ltr]);
    }

    #[test]
    fn neutral_text_takes_the_layout_direction() {
        for fallback in [Ltr, Rtl] {
            assert_eq!(bases("(123) 4.", fallback), [fallback]);
            assert_eq!(bases("", fallback), [fallback]);
            assert_eq!(bases("- 12%\nabc", fallback), [fallback, Ltr]);
        }
    }

    #[test]
    fn an_isolate_is_skipped_to_its_matching_pop() {
        // U+2067 RLI … U+2069 PDI, then Latin.
        assert_eq!(bases("\u{2067}مرحبا\u{2069} hello", Rtl), [Ltr]);
        // An isolate left open runs to the paragraph's end.
        assert_eq!(bases("\u{2068}abc\nمرحبا", Rtl), [Rtl, Rtl]);
        assert_eq!(bases("\u{2068}abc\nxyz", Rtl), [Rtl, Ltr]);
        // Nested isolates close by depth; a stray PDI closes nothing.
        assert_eq!(bases("\u{2066}a\u{2067}ب\u{2069}c\u{2069}ب", Ltr), [Rtl]);
        assert_eq!(bases("\u{2069}ب", Ltr), [Rtl]);
        // An isolate holding the only strong character leaves the fallback.
        assert_eq!(bases("(\u{2068}abc\u{2069})", Rtl), [Rtl]);
    }

    #[test]
    fn marks_count_as_strong_and_a_line_separator_splits_nothing() {
        assert_eq!(bases("\u{200F}123 abc", Ltr), [Rtl]); // RLM
        assert_eq!(bases("\u{200E}123 مرحبا", Rtl), [Ltr]); // LRM
        assert_eq!(bases("مرحبا\u{2028}abc", Ltr), [Rtl]);
        // Empty paragraphs keep their place in the list.
        assert_eq!(bases("abc\n\nمرحبا", Rtl), [Ltr, Rtl, Rtl]);
    }
}
