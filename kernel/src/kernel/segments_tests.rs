//! An owner's segments (LLP 1123 D16), read by `--exact-viewport-segment-*`,
//! driven through the kernel's leaf as lowering produces it.
use super::*;
use crate::style::{env::SegmentGrid, Rect};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// Two columns side by side, the second starting at `left`.
fn columns(left: f32) -> SegmentGrid {
    SegmentGrid {
        cols: 2,
        rows: 1,
        rects: vec![
            rect(0.0, 0.0, 190.0, 844.0),
            rect(left, 0.0, 390.0 - left, 844.0),
        ],
    }
}

/// `id` pads its top by `name`, 7 where it is unavailable.
fn pads(id: ViewId, name: &str) -> Op {
    let text = format!("var({name}, 7px)");
    style(id, vec![], &[(StyleId::PaddingTop, &text)])
}

#[test]
fn an_owners_segment_reads_its_report_and_follows_it() {
    // 3, in the owner 2, pads by the second column's left edge: unavailable until the owner
    // reports two segments, then each report's, then unavailable again where it lies on one
    let mut ops = create(3..=3);
    ops.push(pads(3, "--exact-viewport-segment-left-1-0"));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0);
    assert!(k.set_owner_segments(2, Some(columns(210.0))).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 210.0);
    assert!(
        !k.set_owner_segments(2, Some(columns(210.0))).unwrap(),
        "the same report"
    );
    assert!(k.set_owner_segments(2, Some(columns(220.0))).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 220.0);
    assert!(k.set_owner_segments(2, None).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0, "an owner on one segment has none");
}

#[test]
fn a_box_outside_every_owner_reads_the_documents_segments_and_a_capture_its_holders() {
    // 3 is outside every owner and pads by the document's first segment's width; 5, in the
    // owner 2, pads by `--s`, which 4 above it declares from its holder's second column
    let mut ops = create(3..=5);
    ops.extend([
        pads(3, "--exact-viewport-segment-width-0-0"),
        style(
            4,
            vec![],
            &[(
                StyleId::SafeAreaProperties,
                "--s: var(--exact-viewport-segment-left-1-0, 3px)",
            )],
        ),
        pads(5, "--s"),
    ]);
    ops.extend([children(1, &[2, 3]), children(2, &[4]), children(4, &[5])]);
    let mut k = owner(390.0, 844.0, ops);
    let document = vec![rect(0.0, 0.0, 180.0, 844.0), rect(200.0, 0.0, 190.0, 844.0)];
    k.set_segments(2, 1, document).unwrap();
    k.set_owner_segments(2, Some(columns(210.0))).unwrap();
    lay_out(&mut k);
    assert_eq!([top(&k, 3), top(&k, 5)], [180.0, 210.0]);
    let wider = vec![rect(0.0, 0.0, 185.0, 844.0), rect(205.0, 0.0, 185.0, 844.0)];
    assert!(k.set_segments(2, 1, wider).unwrap());
    k.set_owner_segments(2, Some(columns(230.0))).unwrap();
    lay_out(&mut k);
    assert_eq!([top(&k, 3), top(&k, 5)], [185.0, 230.0]);
}

#[test]
fn a_malformed_segment_grid_is_refused() {
    let mut k = owner(390.0, 844.0, create(3..=3));
    let one = SegmentGrid {
        cols: 1,
        rows: 1,
        rects: vec![rect(0.0, 0.0, 390.0, 844.0)],
    };
    let short = SegmentGrid {
        rects: vec![rect(0.0, 0.0, 190.0, 844.0)],
        ..columns(210.0)
    };
    let wide = SegmentGrid {
        cols: 17,
        ..columns(210.0)
    };
    let mut nonfinite = columns(210.0);
    nonfinite.rects[1].width = f32::NAN;
    for grid in [one, short, wide, nonfinite] {
        assert!(
            k.set_owner_segments(2, Some(grid.clone())).is_err(),
            "{grid:?}"
        );
    }
}

#[test]
fn the_wire_and_the_parser_carry_a_segments_leaf() {
    // A segment's leaf reads back as written, its indices from 0 to 15 as written
    let parsed = |text: &str| crate::style::Dimension::parse_calc(text);
    let leaf = parsed("var(--exact-viewport-segment-bottom-15-0, 2px)").expect("a segment");
    let crate::style::Dimension::Compare(c) = leaf else {
        panic!("a tree: {leaf:?}");
    };
    let mut w = crate::wire::codec::Writer::new();
    c.encode(&mut w);
    let bytes = w.into_vec();
    let mut r = crate::wire::codec::Reader::new(&bytes);
    let read = crate::style::calc::Comparison::decode(&mut r, StyleId::PaddingTop);
    assert_eq!(read.ok(), Some(c));
    for name in [
        "bottom-16-0",
        "bottom-01-0",
        "middle-0-0",
        "width-0",
        "width-0-0-0",
    ] {
        let text = format!("var(--exact-viewport-segment-{name}, 2px)");
        assert_eq!(parsed(&text), None, "{name}");
    }
}

#[test]
fn a_box_given_a_block_gives_its_readers_its_segments() {
    // 3, in the owner 2, holds the reader 4; 3's own report applies while 3 holds a block of
    // its own, and 2's when it gives it back
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    ops.push(pads(4, "--exact-viewport-segment-left-1-0"));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_segments(2, Some(columns(210.0))).unwrap();
    k.set_owner_segments(3, Some(columns(250.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 210.0);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 250.0);
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 210.0);
}

/// The document's two columns, the second at 200.
fn document_columns(k: &mut Kernel) {
    let rects = vec![rect(0.0, 0.0, 180.0, 844.0), rect(200.0, 0.0, 190.0, 844.0)];
    k.set_segments(2, 1, rects).unwrap();
}

#[test]
fn a_segment_beside_a_percentage_reads_its_holders() {
    // 3, in the owner 2, pads by 10% of its 390-point containing block and its holder's second
    // column: 249, and 46 once the owner lies on one segment, not the document's 239
    let mut ops = create(3..=3);
    let padding = "calc(10% + var(--exact-viewport-segment-left-1-0, 7px))";
    ops.push(style(3, vec![], &[(StyleId::PaddingTop, padding)]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    document_columns(&mut k);
    k.set_owner_segments(2, Some(columns(210.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 249.0);
    k.set_owner_segments(2, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 46.0);
}

#[test]
fn a_box_that_takes_a_block_gives_its_readers_none_under_the_documents_segments() {
    // 4, below 3 outside every owner, reads the document's second column, 200; when 3 takes a
    // block and so becomes an owner that has reported none, 4 takes its fallback, and the
    // document's again when 3 gives it back
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Height, "100")]));
    ops.push(pads(4, "--exact-viewport-segment-left-1-0"));
    ops.extend([children(1, &[2, 3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    document_columns(&mut k);
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 200.0);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(block)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 7.0);
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 200.0);
}

#[test]
fn a_reader_moved_into_another_owner_reads_its_segments() {
    // 4 moves from the owner 2 (210) into the owner 3 (250)
    let mut ops = create(3..=4);
    ops.push(pads(4, "--exact-viewport-segment-left-1-0"));
    ops.extend([children(1, &[2, 3]), children(2, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    let block = block(BlockAxis::Is(390.0), BlockAxis::Is(844.0));
    k.set_host_block(3, Some(block)).unwrap();
    k.set_owner_segments(2, Some(columns(210.0))).unwrap();
    k.set_owner_segments(3, Some(columns(250.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 210.0);
    k.apply(0, 2, &[children(2, &[]), children(3, &[4])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 250.0);
}

#[test]
fn a_box_its_host_places_in_the_top_layer_gives_its_readers_no_segments() {
    // 3, in the owner 2, reads 210; when 2 joins the top layer keeping its block, its host
    // places it where the kernel is not told, so 3 takes its fallback, not the document's 200
    let mut ops = create(3..=3);
    ops.push(pads(3, "--exact-viewport-segment-left-1-0"));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    document_columns(&mut k);
    k.set_owner_segments(2, Some(columns(210.0))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 210.0);
    k.apply(0, 2, &[popover(2)]).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0);
}

#[test]
fn every_field_of_a_grid_reads_and_one_with_no_fallback_is_invalid() {
    // A 2 × 2 grid: 3 to 6 pad by the second row's fields, and 7's width, which has no
    // fallback, is the segment's, then `auto` once the owner lies on one segment; no read
    // takes a round of the solve
    let mut ops = create(3..=7);
    let fields = [
        (3, "top-1-1"),
        (4, "bottom-0-1"),
        (5, "right-1-1"),
        (6, "height-0-1"),
    ];
    for (id, name) in fields {
        ops.push(pads(id, &format!("--exact-viewport-segment-{name}")));
    }
    let width = "var(--exact-viewport-segment-width-1-1)";
    ops.push(style(
        7,
        vec![],
        &[(StyleId::Width, width), (StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5, 6, 7])]);
    let mut k = owner(390.0, 844.0, ops);
    let grid = SegmentGrid {
        cols: 2,
        rows: 2,
        rects: vec![
            rect(0.0, 0.0, 190.0, 400.0),
            rect(210.0, 0.0, 180.0, 400.0),
            rect(0.0, 420.0, 190.0, 424.0),
            rect(210.0, 420.0, 180.0, 424.0),
        ],
    };
    k.set_owner_segments(2, Some(grid)).unwrap();
    lay_out(&mut k);
    let tops = [3, 4, 5, 6].map(|id| top(&k, id));
    assert_eq!(tops, [420.0, 844.0, 390.0, 424.0]);
    assert_eq!(frame(&k, 7).2, 180.0);
    assert_eq!(k.arena.observations.rounds, 0);
    k.set_owner_segments(2, None).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 7).2, 390.0, "an invalid width is `auto`");
}

#[test]
fn a_host_that_starts_or_stops_placing_a_top_layer_box_moves_its_readers() {
    // 3, outside every owner, is a popover holding 4, which reads the document's second
    // column, 200, under the block the kernel implies; once its host gives it a block and
    // places it, where the kernel is not told, 4 takes its fallback, and the document's
    // again when the host gives the block back
    let mut ops = create(3..=4);
    ops.extend([popover(3), pads(4, "--exact-viewport-segment-left-1-0")]);
    ops.extend([children(1, &[2, 3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    document_columns(&mut k);
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 200.0);
    let given = block(BlockAxis::Is(200.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(given)).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 7.0);
    k.set_host_block(3, None).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 4), 200.0);
}
