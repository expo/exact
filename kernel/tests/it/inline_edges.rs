//! A `text` inside a paragraph is an inline box: its inline-axis padding,
//! border and margin take room in the line beside its text, before its first
//! run and after its last (CSS 2 §8.3, §10.3.1), as a `<span>`'s do in
//! Chrome (#389: a verse number's `padding-right` touched the next word).

use exact_kernel::{
    InlineEdges, Kernel, NodeType, Offer, Op, PropId, StyleId, StyleProps, StyleValue,
};

// The reference measurer at 16 px advances 9.6 px a glyph.
const ADVANCE: f32 = 9.6;

/// Rows as their author wrote them (`written`), which a right-to-left box
/// keeps on the side they name.
fn rows(rows: &[(StyleId, &str)]) -> Box<StyleProps> {
    let mut patch = StyleProps::default();
    for (row, value) in rows {
        patch
            .set_dynamic(*row, &StyleValue::Text((*value).into()))
            .unwrap();
        patch.written.set(*row);
    }
    Box::new(patch)
}

fn create(id: u32, node_type: NodeType) -> Op {
    Op::CreateView { id, node_type }
}

fn text(id: u32, text: &str) -> Op {
    Op::SetProp {
        id,
        prop: PropId::Text,
        value: text.into(),
    }
}

fn style(id: u32, style: &[(StyleId, &str)]) -> Op {
    Op::SetStyle {
        id,
        patch: rows(style),
    }
}

/// A paragraph (2) at its max-content width in a column: a box (3) holding
/// "1" (4) and "2" (5), then "The" (6). `outer` styles the box, `inner` the
/// "1" in it.
fn verse(outer: &[(StyleId, &str)], inner: &[(StyleId, &str)]) -> Kernel {
    verse_in(&[], "400px", outer, inner)
}

/// `verse` with `paragraph` styling the paragraph, in a column `width` wide.
fn verse_in(
    paragraph: &[(StyleId, &str)],
    width: &str,
    outer: &[(StyleId, &str)],
    inner: &[(StyleId, &str)],
) -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut own = vec![(StyleId::FontSize, "16px")];
    own.extend_from_slice(paragraph);
    let ops = vec![
        create(1, NodeType::View),
        create(2, NodeType::Text),
        create(3, NodeType::Text),
        create(4, NodeType::Text),
        create(5, NodeType::Text),
        create(6, NodeType::Text),
        text(4, "1"),
        text(5, "2"),
        text(6, "The"),
        style(
            1,
            &[
                (StyleId::Width, width),
                (StyleId::Display, "flex"),
                (StyleId::FlexDirection, "column"),
                (StyleId::AlignItems, "flex-start"),
            ],
        ),
        style(2, &own),
        style(3, outer),
        style(4, inner),
        Op::SetChildren {
            id: 3,
            children: vec![4, 5],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 6],
        },
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::AttachRoot { id: 1 },
    ];
    k.apply(0, 1, &ops).unwrap();
    k.compute_layout(1, Offer::definite(400.0, 600.0)).unwrap();
    k
}

fn edges(k: &Kernel) -> Vec<InlineEdges> {
    k.node(2)
        .unwrap()
        .text_runs()
        .iter()
        .map(|r| r.edges)
        .collect()
}

fn width(k: &Kernel) -> f32 {
    k.node(2).unwrap().frame.width
}

const NONE: InlineEdges = InlineEdges {
    start: 0.0,
    end: 0.0,
};

#[test]
fn a_runs_padding_takes_room_beside_its_text() {
    let plain = verse(&[], &[]);
    assert_eq!(edges(&plain), [NONE; 3]);
    assert!((width(&plain) - 5.0 * ADVANCE).abs() < 1e-3);
    // The box's `padding-right` in its font's ems, after its last run.
    let k = verse(&[(StyleId::PaddingRight, "0.3em")], &[]);
    assert_eq!(
        edges(&k),
        [
            NONE,
            InlineEdges {
                start: 0.0,
                end: 4.8
            },
            NONE
        ]
    );
    assert!(
        (width(&k) - (5.0 * ADVANCE + 4.8)).abs() < 1e-3,
        "{}",
        width(&k)
    );
}

#[test]
fn a_boxs_start_goes_before_its_first_run_and_nested_boxes_add() {
    // The box's start before "1", its end after "2"; the "1"'s own box adds
    // its margin and border on both of its sides.
    let k = verse(
        &[(StyleId::PaddingLeft, "3px"), (StyleId::MarginRight, "2px")],
        &[
            (StyleId::MarginLeft, "1px"),
            (StyleId::BorderStyleRight, "solid"),
            (StyleId::BorderWidthRight, "2px"),
        ],
    );
    assert_eq!(
        edges(&k),
        [
            InlineEdges {
                start: 4.0,
                end: 2.0
            },
            InlineEdges {
                start: 0.0,
                end: 2.0
            },
            NONE
        ]
    );
    assert!(
        (width(&k) - (5.0 * ADVANCE + 8.0)).abs() < 1e-3,
        "{}",
        width(&k)
    );
    // A border with no style takes no room, as CSS's `none` has no width.
    let unstyled = verse(&[], &[(StyleId::BorderWidthRight, "2px")]);
    assert_eq!(edges(&unstyled), [NONE; 3]);
}

/// What a box's background covers beside its text: its padding and border,
/// and the room of the boxes inside it at its ends, never its own margin.
#[test]
fn a_boxs_background_reaches_its_padding_and_the_boxes_inside_it() {
    let k = verse(
        &[(StyleId::PaddingLeft, "3px"), (StyleId::MarginRight, "2px")],
        &[
            (StyleId::MarginLeft, "1px"),
            (StyleId::BorderStyleRight, "solid"),
            (StyleId::BorderWidthRight, "2px"),
        ],
    );
    let reach = |id| k.node(id).unwrap().inline_background_reach();
    // The box: its padding, and the "1"'s margin inside it at its start.
    assert_eq!(
        reach(3),
        InlineEdges {
            start: 4.0,
            end: 0.0
        }
    );
    // The "1": its border, not its margin.
    assert_eq!(
        reach(4),
        InlineEdges {
            start: 0.0,
            end: 2.0
        }
    );
    // No paragraph or run without a box has one.
    assert_eq!(reach(2), NONE);
    assert_eq!(reach(6), NONE);
}

/// Start and end are the paragraph's, in which hosts lay its runs out: in
/// a right-to-left paragraph a box's right side is its start, before its
/// first run; a box's own `direction` reorders nothing without
/// `unicode-bidi`, so in a left-to-right paragraph its right side is still
/// after its last run, as Chrome puts `<span dir=rtl style="padding-right:
/// 5px">12</span>The`'s gap between "2" and "The".
#[test]
fn its_sides_are_the_paragraphs_start_and_end() {
    let k = verse_in(
        &[(StyleId::Direction, "rtl")],
        "400px",
        &[(StyleId::PaddingRight, "5px")],
        &[],
    );
    assert_eq!(
        edges(&k)[0],
        InlineEdges {
            start: 5.0,
            end: 0.0
        }
    );
    let k = verse(
        &[(StyleId::Direction, "rtl"), (StyleId::PaddingRight, "5px")],
        &[],
    );
    assert_eq!(
        edges(&k),
        [
            NONE,
            InlineEdges {
                start: 0.0,
                end: 5.0
            },
            NONE
        ]
    );
}

/// A fitted paragraph's runs are at its scale (LLP 1116.001 D1.4), and the
/// room beside them with them: what a host measures is what it paints.
#[test]
fn a_fitted_paragraphs_room_is_at_its_scale() {
    let k = verse_in(
        &[(StyleId::Fit, "shrink"), (StyleId::WhiteSpace, "nowrap")],
        "10px",
        &[(StyleId::PaddingRight, "10px")],
        &[],
    );
    let node = k.node(2).unwrap();
    assert_eq!(node.fit_scale(), 0.5);
    assert_eq!(
        edges(&k)[1],
        InlineEdges {
            start: 0.0,
            end: 5.0
        }
    );
    assert_eq!(
        k.node(3).unwrap().inline_background_reach(),
        InlineEdges {
            start: 0.0,
            end: 5.0
        }
    );
}

#[test]
fn a_runs_padding_written_later_measures_its_paragraph_again() {
    let mut k = verse(&[], &[]);
    k.apply(1, 2, &[style(3, &[(StyleId::PaddingRight, "10px")])])
        .unwrap();
    k.compute_layout(1, Offer::definite(400.0, 600.0)).unwrap();
    assert!(
        (width(&k) - (5.0 * ADVANCE + 10.0)).abs() < 1e-3,
        "{}",
        width(&k)
    );
}
