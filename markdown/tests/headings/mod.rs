//! ATX headings (CommonMark 0.31.2 §4.2).

use super::*;

#[test]
fn a_heading_takes_up_to_three_spaces_of_indent() {
    // CommonMark 0.31.2 example 68; four spaces (example 69) would be indented code,
    // which this dialect reads as a paragraph.
    let source = " ### foo\n\n  ## foo\n\n   # foo\n\n    # foo";
    let styled = style(source, None);
    let kinds: Vec<_> = styled.paragraphs.iter().map(|p| p.kind.clone()).collect();
    assert_eq!(
        kinds,
        [
            ParagraphKind::Heading(3),
            ParagraphKind::Heading(2),
            ParagraphKind::Heading(1),
            ParagraphKind::Body,
        ]
    );
    assert_eq!(hidden(source, None), [" ### ", "  ## ", "   # "]);
    assert_eq!(plain(source), "foo\n\nfoo\n\nfoo\n\n    # foo");
}

#[test]
fn a_tab_after_a_headings_hashes_opens_it() {
    // CommonMark 0.31.2 §4.2: the hashes are followed by spaces or tabs.
    let source = "#\tfoo\n\n## \tbar";
    let styled = style(source, None);
    let kinds: Vec<_> = styled.paragraphs.iter().map(|p| p.kind.clone()).collect();
    assert_eq!(
        kinds,
        [ParagraphKind::Heading(1), ParagraphKind::Heading(2)]
    );
    assert_eq!(hidden(source, None), ["#\t", "## \t"]);
    assert_eq!(plain(source), "foo\n\nbar");
    assert_eq!(run("#\tfo|o", Command::Heading(1)), "fo|o");
}
