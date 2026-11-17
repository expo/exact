//! A table and a description list: their roles, places and visiting order.
use super::*;

/// LLP 1153 D7.3: a table is ARIA's table of a caption and rows of cells,
/// a `th` its column's or its row's header by its `scope`, and a `dl` a
/// description list of terms and definitions; each holds its words.
#[test]
fn a_table_and_a_description_list_are_their_roles() {
    let source = "component App\n  view\n    column\n      table\n        caption\n          text \"Moves\"\n        thead\n          tr\n            th scope=\"col\"\n              text \"Move\"\n            th\n              text \"Power\"\n        tbody\n          tr\n            th scope=\"row\"\n              text \"Tackle\"\n            td\n              text \"40\"\n      dl\n        dt\n          text \"Type\"\n        dd\n          text \"Electric\"\n";
    let mut p = boot(source, (320., 600.), 0);
    let nodes = read(&mut p);
    let said: Vec<_> = nodes.iter().map(|n| (n.role, n.name.as_str())).collect();
    assert_eq!(
        said,
        [
            (role::TABLE, ""),
            (role::CAPTION, ""),
            (role::TEXT, "Moves"),
            (role::ROW, ""),
            (role::COLUMN_HEADER, ""),
            (role::TEXT, "Move"),
            (role::COLUMN_HEADER, ""),
            (role::TEXT, "Power"),
            (role::ROW, ""),
            (role::ROW_HEADER, ""),
            (role::TEXT, "Tackle"),
            (role::CELL, ""),
            (role::TEXT, "40"),
            (role::DESCRIPTION_LIST, ""),
            (role::TERM, ""),
            (role::TEXT, "Type"),
            (role::DEFINITION, ""),
            (role::TEXT, "Electric"),
        ]
    );
    let parent = |name: &str| nodes.iter().find(|n| n.id == named(&nodes, name).parent);
    assert_eq!(parent("Moves").map(|n| n.role), Some(role::CAPTION));
    assert_eq!(parent("Tackle").map(|n| n.role), Some(role::ROW_HEADER));
    assert_eq!(parent("40").map(|n| n.role), Some(role::CELL));
    assert_eq!(parent("Electric").map(|n| n.role), Some(role::DEFINITION));
}

/// A table says its rows and columns, and each row and cell where the
/// kernel's table placed it, from 1: a caption is no row, and a `colspan`
/// and a `rowspan` move the cells after them. A description list counts its
/// terms; a definition has no place.
#[test]
fn a_tables_cells_stand_where_their_spans_put_them() {
    let source = r#"component App
  view
    column
      table
        caption
          text "Moves"
        thead
          tr
            th colspan=2
              text "Move"
            th
              text "Power"
        tbody
          tr
            th scope="row" rowspan=2
              text "Tackle"
            td
              text "Normal"
            td
              text "40"
          tr
            td
              text "Physical"
            td
              text "35"
      dl
        dt
          text "Type"
        dt
          text "Kind"
        dd
          text "Electric"
        dt
          text "Weight"
        dd
          text "6 kg"
"#;
    let mut p = boot(source, (320., 600.), 0);
    let told = statement(&mut p);
    let nodes = &told.nodes;
    let holder = |name: &str| {
        let parent = named(nodes, name).parent;
        nodes.iter().find(|n| n.id == parent).unwrap()
    };
    let table = nodes.iter().find(|n| n.role == role::TABLE).unwrap();
    assert_eq!(table.grid, [0, 3, 0, 3]);
    let rows: Vec<_> = (nodes.iter())
        .filter(|n| n.role == role::ROW)
        .map(|n| n.grid)
        .collect();
    assert_eq!(rows, [[1, 1, 1, 3], [2, 1, 1, 3], [3, 1, 1, 3]]);
    let cells = ["Move", "Power", "Tackle", "Normal", "40", "Physical", "35"];
    let placed = cells.map(|name| holder(name).grid);
    assert_eq!(
        placed,
        [
            [1, 1, 1, 2],
            [1, 1, 3, 1],
            [2, 2, 1, 1],
            [2, 1, 2, 1],
            [2, 1, 3, 1],
            [3, 1, 2, 1],
            [3, 1, 3, 1],
        ]
    );
    assert_eq!(holder("Moves").grid, [0; 4], "a caption has no place");
    let list = nodes
        .iter()
        .find(|n| n.role == role::DESCRIPTION_LIST)
        .unwrap();
    assert_eq!((list.set, list.grid), ((0, 3), [0; 4]));
    let terms = ["Type", "Kind", "Weight"].map(|name| holder(name).set);
    assert_eq!(terms, [(1, 3), (2, 3), (3, 3)]);
    assert_eq!(holder("Electric").set, (0, 0), "a definition has no place");
}

/// A table's rows are visited in the order it draws and counts them (CSS 2.2
/// §17.2): the first `thead` first and the first `tfoot` last, where an HTML
/// author may write them anywhere; a later `thead` or `tfoot` is a row group
/// where it is written.
#[test]
fn a_tables_rows_are_visited_in_the_order_it_draws_them() {
    let table = |groups: &[(&str, &str)]| {
        let mut source = String::from("component App\n  view\n    column\n      table\n");
        for (tag, words) in groups {
            source += &format!(
                "        {tag}\n          tr\n            td\n              text \"{words}\"\n"
            );
        }
        let mut p = boot(&source, (320., 600.), 0);
        let told = statement(&mut p);
        let row = |name: &str| {
            let cell = named(&told.nodes, name).parent;
            let cell = told.nodes.iter().find(|n| n.id == cell).unwrap();
            cell.grid[0]
        };
        let visited: Vec<(String, i32)> = (told.nodes.iter())
            .filter(|n| n.role == role::TEXT)
            .map(|n| (n.name.clone(), row(&n.name)))
            .collect();
        visited
    };
    let pairs = |said: &[(&str, i32)]| -> Vec<(String, i32)> {
        said.iter().map(|(n, r)| (n.to_string(), *r)).collect()
    };
    assert_eq!(
        table(&[("tfoot", "Foot 1"), ("tbody", "Body"), ("tfoot", "Foot 2")]),
        pairs(&[("Body", 1), ("Foot 2", 2), ("Foot 1", 3)])
    );
    assert_eq!(
        table(&[
            ("tbody", "Body"),
            ("thead", "Head"),
            ("tfoot", "Foot"),
            ("thead", "Head 2")
        ]),
        pairs(&[("Head", 1), ("Body", 2), ("Head 2", 3), ("Foot", 4)])
    );
    assert_eq!(
        table(&[("thead", "Head"), ("tbody", "Body"), ("tfoot", "Foot")]),
        pairs(&[("Head", 1), ("Body", 2), ("Foot", 3)])
    );
}
