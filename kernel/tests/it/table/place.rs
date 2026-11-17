//! Where each row and cell stands in its table, as the hosts' table
//! accessibility reads it (`Kernel::table_place`, `Kernel::table_size`).
use super::*;
use exact_kernel::TablePlace;

/// Where `view` stands: its table, its first row, the rows it covers, its
/// first column, the columns it covers.
fn at(k: &Kernel, view: u32) -> Option<[u32; 5]> {
    let p: TablePlace = k.table_place(view)?;
    Some([p.table, p.row, p.rows, p.column, p.columns])
}

fn th(props: &[(PropId, i64)]) -> N {
    N::Tag("th", Vec::new(), props.to_vec(), vec![N::Box(20.0, 10.0)])
}

/// A caption is no row; a header cell spanning two columns puts the next
/// in the third; a row span reaches the next row and moves its first cell
/// on, but ends with its row group; a row covers every column.
#[test]
fn each_cell_stands_where_its_spans_put_it_and_a_caption_is_no_row() {
    // 2 table, 3 caption, 4 box, 5 thead, 6 tr, 7 th (colspan 2), 8 box,
    // 9 th (rowspan 5), 10 box, 11 tbody, 12 tr, 13 td (rowspan 2), 14 box,
    // 15/17 td, 19 tr, 20/22 td.
    let k = lay_out(tag(
        "table",
        vec![
            tag("caption", vec![N::Box(20.0, 10.0)]),
            tag(
                "thead",
                vec![tag("tr", vec![th(&[(COLSPAN, 2)]), th(&[(ROWSPAN, 5)])])],
            ),
            tag(
                "tbody",
                vec![
                    tag(
                        "tr",
                        vec![
                            cell(&[(ROWSPAN, 2)], 20.0, 10.0),
                            cell(&[], 20.0, 10.0),
                            cell(&[], 20.0, 10.0),
                        ],
                    ),
                    tag("tr", vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]),
                ],
            ),
        ],
    ));
    assert_eq!(k.table_size(2), Some((3, 3)));
    let mut views = k.table_views();
    views.sort_unstable();
    assert_eq!(views, [2, 3, 5, 6, 7, 9, 11, 12, 13, 15, 17, 19, 20, 22]);
    let places = [6, 7, 9, 12, 13, 15, 17, 19, 20, 22].map(|v| at(&k, v));
    let expected = [
        [2, 0, 1, 0, 3],
        [2, 0, 1, 0, 2],
        [2, 0, 1, 2, 1],
        [2, 1, 1, 0, 3],
        [2, 1, 2, 0, 1],
        [2, 1, 1, 1, 1],
        [2, 1, 1, 2, 1],
        [2, 2, 1, 0, 3],
        [2, 2, 1, 1, 1],
        [2, 2, 1, 2, 1],
    ];
    assert_eq!(places, expected.map(Some));
    for not in [2, 3, 5, 11, 4] {
        assert_eq!(at(&k, not), None, "{not} is no row or cell");
    }
    assert_eq!(k.table_size(6), None, "a row is no table");
}

/// A changed span and a new row place the cells again, and the table's
/// size follows.
#[test]
fn a_changed_span_and_a_new_row_move_the_places() {
    // 2 table, 3 tr, 4/6 td, 8 tr, 9/11 td.
    let row = || tag("tr", vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]);
    let mut k = lay_out(tag("table", vec![row(), row()]));
    assert_eq!(k.table_size(2), Some((2, 2)));
    let span = Op::SetProp {
        id: 4,
        prop: COLSPAN,
        value: PropValue::Int(2),
    };
    let (mut ops, mut next) = (vec![span], 13);
    let added = make(&tag("tr", vec![cell(&[], 20.0, 10.0)]), &mut next, &mut ops);
    ops.push(Op::SetChildren {
        id: 2,
        children: vec![3, 8, added],
    });
    k.apply(0, 2, &ops).unwrap();
    k.compute_layout(1, Offer::definite(400.0, 400.0)).unwrap();
    assert_eq!(k.table_size(2), Some((3, 3)));
    assert_eq!(at(&k, 4), Some([2, 0, 1, 0, 2]));
    assert_eq!(at(&k, 6), Some([2, 0, 1, 2, 1]));
    assert_eq!(at(&k, added + 1), Some([2, 2, 1, 0, 1]));
}

/// A table its author made a block holds an anonymous table for each run
/// of its parts (CSS 2.2 §17.2.1); its rows are counted across them, in
/// the order they are written, and its columns are the widest's.
#[test]
fn a_block_tables_rows_count_on_across_its_anonymous_tables() {
    // 2 table, 3 tr, 4/6 td, 8 box, 9 tr, 10 td.
    let mut k = lay_out(tag(
        "table",
        vec![
            tag("tr", vec![cell(&[], 20.0, 10.0), cell(&[], 20.0, 10.0)]),
            N::Box(20.0, 10.0),
            tag("tr", vec![cell(&[], 20.0, 10.0)]),
        ],
    ));
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    assert_eq!(k.table_size(2), Some((2, 2)));
    assert_eq!(at(&k, 6), Some([2, 0, 1, 1, 1]));
    assert_eq!(at(&k, 9), Some([2, 1, 1, 0, 2]));
    assert_eq!(at(&k, 10), Some([2, 1, 1, 0, 1]));
}

/// Rows are counted in the order the table draws them (CSS 2.2 §17.2),
/// which is not HTML's table model where the two differ: of two `tfoot`s,
/// only the first is the footer, drawn last, and the second is a row group
/// where it stands. So `tfoot, tbody, tfoot` is drawn body, second foot,
/// first foot, and its rows count in that order.
#[test]
fn rows_count_in_the_order_they_are_drawn_so_only_the_first_tfoot_is_last() {
    // 2 table; then each group, its row, its cell and its box: 3 tfoot,
    // 7 tbody, 11 tfoot (the rows 4, 8, 12, the cells 5, 9, 13).
    let group = |name: &'static str| tag(name, vec![tag("tr", vec![cell(&[], 20.0, 10.0)])]);
    let k = lay_out(tag("table", ["tfoot", "tbody", "tfoot"].map(group).into()));
    let y = |id| frame(&k, id)[1];
    assert_eq!([8, 12, 4].map(y), [2.0, 16.0, 30.0], "body, foot 2, foot 1");
    let row = |id| at(&k, id).map(|[_, row, ..]| row);
    assert_eq!([8, 12, 4].map(row), [0, 1, 2].map(Some), "rows");
    assert_eq!([9, 13, 5].map(row), [0, 1, 2].map(Some), "cells");
    assert_eq!(k.table_order(2), Some(vec![1, 2, 0]), "listed as drawn");
}

/// A table's children are listed in the order it draws them; a block
/// table orders each anonymous table's parts, and a box between two keeps
/// its place. A table drawn as written has no order of its own.
#[test]
fn a_tables_children_are_listed_as_drawn_and_a_box_keeps_its_place() {
    let group = |name: &'static str| tag(name, vec![tag("tr", vec![cell(&[], 20.0, 10.0)])]);
    let in_order = lay_out(tag("table", ["thead", "tbody", "tfoot"].map(group).into()));
    assert_eq!(in_order.table_order(2), None);
    let mut k = lay_out(tag(
        "table",
        vec![
            group("tfoot"),
            group("tbody"),
            N::Box(20.0, 10.0),
            group("tbody"),
            group("thead"),
        ],
    ));
    assert_eq!(
        k.table_order(2),
        Some(vec![4, 1, 2, 3, 0]),
        "the box is no part"
    );
    restyle(&mut k, 2, 2, &[(StyleId::Display, "block")]);
    assert_eq!(k.table_order(2), Some(vec![1, 0, 2, 4, 3]));
}
