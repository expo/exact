use super::*;

#[test]
fn a_block_tables_caption_follows_spacing_changes() {
    let mut k = lay_out(tag(
        "table",
        vec![
            tag("caption", vec![N::Box(10.0, 10.0)]),
            tag("tr", vec![cell(&[], 60.0, 10.0)]),
        ],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    restyle(&mut k, 3, 3, &[(StyleId::MarginBottom, "10%")]);
    for (epoch, spacing, expected) in [(4, "10", 28.2), (5, "2", 18.6)] {
        restyle(&mut k, epoch, 2, &[(StyleId::BorderSpacing, spacing)]);
        assert!(
            (frame(&k, 6)[1] - expected).abs() < 1e-3,
            "{:?}",
            frame(&k, 6)
        );
    }
}

#[test]
fn an_authored_row_group_display_keeps_its_rows_as_items() {
    for display in ["flex", "grid"] {
        // 2 table, 3 tbody, 4/7 rows, 5/8 cells.
        let mut k = lay_out(tag(
            "table",
            vec![tag(
                "tbody",
                vec![
                    tag("tr", vec![cell(&[], 20.0, 10.0)]),
                    tag("tr", vec![cell(&[], 30.0, 10.0)]),
                ],
            )],
        ));
        restyle(&mut k, 2, 2, &[(StyleId::Display, "flex")]);
        restyle(&mut k, 3, 3, &[(StyleId::Display, display)]);
        let (second_x, second_y) = if display == "flex" {
            (28.0, 2.0)
        } else {
            (2.0, 18.0)
        };
        assert_eq!(frame(&k, 5), [2.0, 2.0, 22.0, 12.0], "{display}");
        assert_eq!(frame(&k, 8), [second_x, second_y, 32.0, 12.0], "{display}");
        restyle(&mut k, 4, 2, &[(StyleId::BorderSpacing, "10")]);
        assert_eq!(frame(&k, 5)[0..2], [10.0, 10.0], "{display}");
        restyle(&mut k, 5, 3, &[(StyleId::Display, "block")]);
        assert_eq!(frame(&k, 5), [10.0, 10.0, 32.0, 12.0]);
        assert_eq!(frame(&k, 8), [10.0, 32.0, 32.0, 12.0]);
    }
}

#[test]
fn a_block_table_keeps_normal_children_between_anonymous_tables() {
    // 2 table, 3 box, 4 row, 5 cell, 7 box, 8 row, 9 cell, 11 box.
    let mut k = lay_out(tag(
        "table",
        vec![
            N::Box(10.0, 5.0),
            tag("tr", vec![cell(&[], 20.0, 10.0)]),
            N::Box(10.0, 7.0),
            tag("tr", vec![cell(&[], 30.0, 10.0)]),
            N::Box(10.0, 9.0),
        ],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    for (id, expected) in [
        (2, [0.0, 0.0, 400.0, 53.0]),
        (3, [0.0, 0.0, 10.0, 5.0]),
        (5, [2.0, 7.0, 22.0, 12.0]),
        (7, [0.0, 21.0, 10.0, 7.0]),
        (9, [2.0, 30.0, 32.0, 12.0]),
        (11, [0.0, 44.0, 10.0, 9.0]),
    ] {
        assert_eq!(frame(&k, id), expected, "node {id}");
    }
    k.apply(
        0,
        3,
        &[Op::SetChildren {
            id: 2,
            children: vec![3, 4, 8, 11],
        }],
    )
    .unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    assert_eq!(frame(&k, 5), [2.0, 7.0, 32.0, 12.0]);
    assert_eq!(frame(&k, 9), [2.0, 21.0, 32.0, 12.0]);
    assert_eq!(frame(&k, 11), [0.0, 35.0, 10.0, 9.0]);
}

#[test]
fn every_caption_margin_keeps_the_terminals_space_rule() {
    let mut k = lay_out_in(
        tag(
            "table",
            vec![
                tag("caption", vec![N::Box(8.0, 16.0)]),
                tag("tr", vec![cell(&[], 80.0, 16.0)]),
            ],
        ),
        true,
    );
    restyle(
        &mut k,
        2,
        3,
        &[
            (StyleId::MarginLeft, "14px"),
            (StyleId::MarginRight, "14px"),
            (StyleId::MarginTop, "14px"),
            (StyleId::MarginBottom, "14px"),
        ],
    );
    assert_eq!(frame(&k, 3), [16.0, 16.0, 48.0, 16.0]);
    assert_eq!(frame(&k, 6), [0.0, 48.0, 80.0, 16.0]);
    assert_eq!(frame(&k, 2)[3], 64.0);
}

#[test]
fn a_flex_or_grid_tables_groups_are_separate_items() {
    let two = || tag("tr", vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]);
    let mut k = lay_out(tag(
        "table",
        vec![tag("thead", vec![two()]), tag("tbody", vec![two()])],
    ));
    let ordinary: Vec<_> = (1..=14).map(|id| frame(&k, id)).collect();
    restyle(&mut k, 2, 2, &[(StyleId::Display, "flex")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 3), [0.0, 0.0, 50.0, 16.0]);
    assert_eq!(frame(&k, 9), [50.0, 0.0, 50.0, 16.0]);
    assert_eq!(frame(&k, 11), [52.0, 2.0, 22.0, 12.0]);
    restyle(&mut k, 3, 2, &[(StyleId::Display, "grid")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 32.0]);
    assert_eq!(frame(&k, 9), [0.0, 16.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 11), [2.0, 18.0, 22.0, 12.0]);
    restyle(&mut k, 4, 2, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 30.0]);
    assert_eq!(frame(&k, 9), [2.0, 16.0, 46.0, 12.0]);
    assert_eq!(frame(&k, 11), [2.0, 16.0, 22.0, 12.0]);
    k.apply(
        0,
        5,
        &[Op::ClearStyle {
            id: 2,
            mask: exact_kernel::StyleMask::of(StyleId::Display),
            written: true,
        }],
    )
    .unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    assert_eq!(
        (1..=14).map(|id| frame(&k, id)).collect::<Vec<_>>(),
        ordinary
    );
}

#[test]
fn an_anonymous_table_follows_direction_only_changes() {
    let mut k = lay_out(tag(
        "table",
        vec![tag(
            "tr",
            vec![cell(&[], 20.0, 10.0), cell(&[], 30.0, 10.0)],
        )],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 4)[0], 2.0);
    restyle(&mut k, 3, 2, &[(StyleId::Direction, "rtl")]);
    assert_eq!(frame(&k, 4)[0], 376.0);
    assert_eq!(frame(&k, 6)[0], 342.0);
    restyle(&mut k, 4, 2, &[(StyleId::Direction, "ltr")]);
    assert_eq!(frame(&k, 4)[0], 2.0);
    assert_eq!(frame(&k, 6)[0], 26.0);
}

#[test]
fn a_captions_bottom_percentage_margin_uses_the_table_width() {
    let mut k = lay_out(tag(
        "table",
        vec![
            tag("caption", vec![N::Box(10.0, 10.0)]),
            tag("tr", vec![cell(&[], 60.0, 10.0)]),
        ],
    ));
    restyle(&mut k, 2, 3, &[(StyleId::MarginBottom, "10%")]);
    assert!((frame(&k, 6)[1] - 18.6).abs() < 1e-3, "{:?}", frame(&k, 6));
    restyle(
        &mut k,
        3,
        2,
        &[(StyleId::PaddingLeft, "20"), (StyleId::PaddingRight, "20")],
    );
    assert!((frame(&k, 6)[1] - 22.6).abs() < 1e-3, "{:?}", frame(&k, 6));
}

#[test]
fn an_empty_block_table_has_no_anonymous_spacing() {
    let mut k = lay_out(tag("table", vec![]));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 0.0]);
    let mut ops = Vec::new();
    let child = make(&tag("tr", vec![cell(&[], 20.0, 10.0)]), &mut 3, &mut ops);
    ops.push(Op::SetChildren {
        id: 2,
        children: vec![child],
    });
    k.apply(0, 3, &ops).unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 4), [2.0, 2.0, 22.0, 12.0]);
    k.apply(
        0,
        4,
        &[Op::SetChildren {
            id: 2,
            children: vec![],
        }],
    )
    .unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 0.0]);
}

#[test]
fn a_flex_or_grid_table_forms_new_groups_and_rows() {
    for display in ["flex", "grid"] {
        for group in [true, false] {
            let mut k = lay_out(tag("table", vec![]));
            restyle(&mut k, 2, 2, &[(StyleId::Display, display)]);
            let row = tag("tr", vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]);
            let part = if group { tag("thead", vec![row]) } else { row };
            let mut ops = Vec::new();
            let child = make(&part, &mut 3, &mut ops);
            ops.push(Op::SetChildren {
                id: 2,
                children: vec![child],
            });
            k.apply(0, 3, &ops).unwrap();
            k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
            let first = if group { 5 } else { 4 };
            assert_eq!(
                frame(&k, first),
                [2.0, 2.0, 22.0, 12.0],
                "{display}, group {group}"
            );
            assert_eq!(frame(&k, first + 2), [26.0, 2.0, 22.0, 12.0]);
            assert_eq!(frame(&k, child)[3], 16.0);
        }
    }
}

#[test]
fn a_moved_group_takes_its_new_tables_spacing() {
    for display in ["flex", "grid"] {
        let group = || {
            tag(
                "tbody",
                vec![tag(
                    "tr",
                    vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)],
                )],
            )
        };
        // 3 table, 4 group, 6/8 cells; 10 table, 11 group.
        let mut k = lay_out(tag(
            "div",
            vec![tag("table", vec![group()]), tag("table", vec![group()])],
        ));
        restyle(
            &mut k,
            2,
            3,
            &[(StyleId::Display, display), (StyleId::BorderSpacing, "2")],
        );
        restyle(
            &mut k,
            3,
            10,
            &[(StyleId::Display, display), (StyleId::BorderSpacing, "10")],
        );
        assert_eq!(frame(&k, 6)[0] - frame(&k, 4)[0], 2.0);
        k.apply(
            0,
            4,
            &[
                Op::SetChildren {
                    id: 3,
                    children: vec![],
                },
                Op::SetChildren {
                    id: 10,
                    children: vec![11, 4],
                },
            ],
        )
        .unwrap();
        k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
        let owner = frame(&k, 4);
        assert_eq!(
            frame(&k, 6),
            [owner[0] + 10.0, owner[1] + 10.0, 22.0, 12.0],
            "{display}"
        );
        assert_eq!(frame(&k, 8)[0] - frame(&k, 6)[0], 32.0);
        assert_eq!(owner[3], 32.0);
    }
}

#[test]
fn a_flex_or_grid_tables_spacing_change_reaches_its_grids() {
    for display in ["flex", "grid"] {
        let mut k = lay_out(tag(
            "table",
            vec![tag(
                "tbody",
                vec![tag(
                    "tr",
                    vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)],
                )],
            )],
        ));
        restyle(&mut k, 2, 2, &[(StyleId::Display, display)]);
        for (epoch, spacing) in [(3, "10"), (4, "2")] {
            restyle(&mut k, epoch, 2, &[(StyleId::BorderSpacing, spacing)]);
            let px = spacing.parse::<f32>().unwrap();
            assert_eq!(frame(&k, 5), [px, px, 22.0, 12.0], "{display}, {spacing}px");
            assert_eq!(frame(&k, 7)[0], 22.0 + 2.0 * px);
            assert_eq!(frame(&k, 3)[3], 12.0 + 2.0 * px);
        }
    }
}

#[test]
fn a_captions_bottom_margin_keeps_the_terminals_space_rule() {
    let mut k = lay_out_in(
        tag(
            "table",
            vec![
                tag("caption", vec![N::Box(8.0, 16.0)]),
                tag("tr", vec![cell(&[], 80.0, 16.0)]),
            ],
        ),
        true,
    );
    for (i, (margin, gap)) in [
        ("14px", 16.0),
        ("calc(0% + 14px)", 16.0),
        ("10%", 8.0),
        ("calc(10% + 8px)", 16.0),
    ]
    .into_iter()
    .enumerate()
    {
        restyle(&mut k, i as u64 + 2, 3, &[(StyleId::MarginBottom, margin)]);
        assert_eq!(frame(&k, 6)[1], 16.0 + gap, "{margin}");
        assert_eq!(frame(&k, 2)[3], 32.0 + gap, "{margin}");
    }
}

#[test]
fn hiding_a_box_between_runs_joins_their_anonymous_table() {
    // 2 table, 3 box, 4 row, 5 cell, 7 box, 8 row, 9 cell, 11 box.
    let mut k = lay_out(tag(
        "table",
        vec![
            N::Box(10.0, 5.0),
            tag("tr", vec![cell(&[], 20.0, 10.0)]),
            N::Box(10.0, 7.0),
            tag("tr", vec![cell(&[], 30.0, 10.0)]),
            N::Box(10.0, 9.0),
        ],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    // Chrome: one anonymous table once the box between the rows is hidden.
    restyle(&mut k, 3, 7, &[(StyleId::Display, "none")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 44.0]);
    assert_eq!(frame(&k, 5), [2.0, 7.0, 32.0, 12.0]);
    assert_eq!(frame(&k, 9), [2.0, 21.0, 32.0, 12.0]);
    assert_eq!(frame(&k, 11), [0.0, 35.0, 10.0, 9.0]);
    restyle(&mut k, 4, 7, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 53.0]);
    assert_eq!(frame(&k, 5), [2.0, 7.0, 22.0, 12.0]);
    assert_eq!(frame(&k, 7), [0.0, 21.0, 10.0, 7.0]);
    assert_eq!(frame(&k, 9), [2.0, 30.0, 32.0, 12.0]);
}

#[test]
fn a_cell_that_is_an_item_keeps_its_ua_padding() {
    // 2 table, 3 row, 4 cell. Chrome: the cell is 22 by 12 in a flex row
    // and as wide as the grid row in a grid one.
    for (display, width) in [("flex", 22.0), ("grid", 400.0)] {
        let mut k = lay_out(tag("table", vec![tag("tr", vec![cell(&[], 20.0, 10.0)])]));
        restyle(&mut k, 2, 2, &[(StyleId::Display, display)]);
        restyle(&mut k, 3, 3, &[(StyleId::Display, display)]);
        assert_eq!(frame(&k, 4), [0.0, 0.0, width, 12.0], "{display}");
    }
    // Outside any table too, as the UA sheet's `td` rule has no context.
    let k = lay_out(tag("div", vec![cell(&[], 20.0, 10.0)]));
    assert_eq!(frame(&k, 3), [0.0, 0.0, 400.0, 12.0]);
}

#[test]
fn a_block_tables_part_with_its_own_display_is_that_box() {
    // 2 table, 3 tbody, 4/7 rows, 5/8 cells. Chrome: a flex row group's
    // rows are items side by side, each its own anonymous table.
    let mut k = lay_out(tag(
        "table",
        vec![tag(
            "tbody",
            vec![
                tag("tr", vec![cell(&[], 20.0, 10.0)]),
                tag("tr", vec![cell(&[], 30.0, 10.0)]),
            ],
        )],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    let parts = |k: &Kernel| (2..=9).map(|id| frame(k, id)).collect::<Vec<_>>();
    let grouped = parts(&k);
    restyle(&mut k, 3, 3, &[(StyleId::Display, "flex")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 3), [0.0, 0.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 4)[0..2], [0.0, 0.0]);
    assert_eq!(frame(&k, 5), [2.0, 2.0, 22.0, 12.0]);
    assert_eq!(frame(&k, 7)[0..2], [26.0, 0.0]);
    assert_eq!(frame(&k, 8), [28.0, 2.0, 32.0, 12.0]);
    clear_display(&mut k, 4, 3);
    assert_eq!(parts(&k), grouped);
    // 2 table, 3/6 rows, 4/7 cells. Chrome: a block row holds its cells'
    // anonymous table; the next row is another.
    let mut k = lay_out(tag(
        "table",
        vec![
            tag("tr", vec![cell(&[], 20.0, 10.0)]),
            tag("tr", vec![cell(&[], 30.0, 10.0)]),
        ],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    let parts = |k: &Kernel| (2..=8).map(|id| frame(k, id)).collect::<Vec<_>>();
    let rows = parts(&k);
    restyle(&mut k, 3, 3, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 2), [0.0, 0.0, 400.0, 32.0]);
    assert_eq!(frame(&k, 3), [0.0, 0.0, 400.0, 16.0]);
    assert_eq!(frame(&k, 4), [2.0, 2.0, 22.0, 12.0]);
    assert_eq!(frame(&k, 7), [2.0, 18.0, 32.0, 12.0]);
    clear_display(&mut k, 4, 3);
    assert_eq!(parts(&k), rows);
}

#[test]
fn a_block_rows_box_ends_its_cells_anonymous_table() {
    // 2 table, 3 row, 4 cell, 6 box, 7 cell. Chrome: a table for each run.
    let mut k = lay_out(tag(
        "table",
        vec![tag(
            "tr",
            vec![
                cell(&[], 20.0, 10.0),
                N::Box(10.0, 7.0),
                cell(&[], 30.0, 10.0),
            ],
        )],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    restyle(&mut k, 3, 3, &[(StyleId::Display, "block")]);
    assert_eq!(frame(&k, 3), [0.0, 0.0, 400.0, 39.0]);
    assert_eq!(frame(&k, 4), [2.0, 2.0, 22.0, 12.0]);
    assert_eq!(frame(&k, 6), [0.0, 16.0, 10.0, 7.0]);
    assert_eq!(frame(&k, 7), [2.0, 25.0, 32.0, 12.0]);
}

fn clear_display(k: &mut Kernel, epoch: u64, id: u32) {
    let mask = exact_kernel::StyleMask::of(StyleId::Display);
    let op = Op::ClearStyle {
        id,
        mask,
        written: true,
    };
    k.apply(0, epoch, &[op]).unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
}

#[test]
fn a_row_retagged_as_a_div_lays_out_as_a_fresh_div() {
    // 2 table, 3 row, 4/6 cells.
    let two = |name| tag(name, vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]);
    let mut k = lay_out(tag("table", vec![two("tr")]));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "flex")]);
    assert_eq!(frame(&k, 6), [26.0, 2.0, 22.0, 12.0]);
    k.apply(
        0,
        3,
        &[Op::SetProp {
            id: 3,
            prop: PropId::SemanticTag,
            value: "div".into(),
        }],
    )
    .unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    let mut fresh = lay_out(tag("table", vec![two("div")]));
    restyle(&mut fresh, 2, 2, &[(StyleId::Display, "flex")]);
    for id in 2..=7 {
        assert_eq!(frame(&k, id), frame(&fresh, id), "node {id}");
    }
}
