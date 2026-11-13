//! A grouped list's rows as the kernel hands them to UIKit's list.

use crate::host::{view, NoData};
use exact_apple::Host;
use exact_kernel::{GroupedCells, MonospaceMeasurer};

#[test]
fn a_rows_width_is_its_cells_as_told() {
    for direction in ["ltr", "rtl"] {
        for style in ["plain", "inset-grouped"] {
            let src = format!(
                r#"component Form
  view
    column width=402 height=600 direction="{direction}"
      list testId="list" appearance="auto" listStyle="{style}" flex=1 padding-left="4.5rem"
        section
          row testId="row"
            view width=24 height=24
            text "Custom" flex=1
"#
            );
            let plan = contract::compile(&src).unwrap();
            let (mut host, _) = Host::boot(
                &plan.encode(),
                NoData,
                Box::new(MonospaceMeasurer::default()),
                402.0,
                600.0,
            )
            .unwrap();
            let list = view(&host, "list");
            let row = view(&host, "row");
            for root_font in [230.0 / 13.5, 20.0] {
                host.set_root_font_size(root_font);
                let padding = (root_font * 4.5) as f32;
                if style == "plain" {
                    let frame = host.runner().kernel().node(row).unwrap().frame;
                    assert!(
                        (frame.width - (402.0 - padding)).abs() < 0.01,
                        "{style}, {direction}, padding {padding}: {frame:?}"
                    );
                }
                // The row is as wide as UIKit's cell, which UIKit has already
                // narrowed by the list's padding and the card's insets: the
                // kernel takes the width as told and narrows it by neither.
                for width in [402.0 - padding, 402.0 - padding - 36.0, 402.0 - padding] {
                    let batch = host.set_claims(&exact_apple::Claims {
                        cells: &[(
                            list,
                            Some(GroupedCells {
                                width,
                                cell: [16.0, 16.0],
                            }),
                        )],
                        ..Default::default()
                    });
                    let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
                    assert!(batch["error"].is_null(), "{batch}");
                    let frame = host.runner().kernel().node(row).unwrap().frame;
                    let note = format!(
                        "{style}, {direction}, padding {padding}, width {width}: {frame:?}"
                    );
                    assert!((frame.width - width).abs() < 0.01, "{note}");
                }
                // Cleared, so the next root font starts from the sheet's
                let batch = host.set_claims(&exact_apple::Claims {
                    cells: &[(list, None)],
                    ..Default::default()
                });
                let batch: serde_json::Value = serde_json::from_str(&batch).unwrap();
                assert!(batch["error"].is_null(), "{batch}");
            }
        }
    }
}

/// UIKit's content configuration draws none of a row's box, so a row of a
/// standard cell's parts whose author wrote an edge of it, or its
/// background image, is carried as its own views, as every other host
/// draws it (LLP 1084 D7); a row with none is the standard cell.
#[test]
fn a_row_with_an_authored_edge_is_carried() {
    let src = r##"component Form
  view
    list testId="list" appearance="auto" listStyle="inset-grouped" width="100%" height="100%"
      section
        row testId="plain"
          text "Plain"
        row testId="ruled" border-bottom="1px solid -exact-separator"
          text "Ruled"
        row testId="unruled" border-bottom="none"
          text "Unruled"
        row testId="framed" border="1px solid -exact-separator" border-bottom="none"
          text "Framed"
        row testId="pictured" background-image="linear-gradient(red, blue)"
          text "Pictured"
"##;
    let plan = contract::compile(src).unwrap();
    let (host, _) = Host::boot(
        &plan.encode(),
        NoData,
        Box::new(MonospaceMeasurer::default()),
        402.0,
        874.0,
    )
    .unwrap();
    let list = host
        .runner()
        .kernel()
        .grouped_list(view(&host, "list"))
        .expect("a grouped list");
    let custom = |id: &str| {
        let id = view(&host, id);
        list.sections
            .iter()
            .flat_map(|s| s.rows.iter())
            .find(|r| r.view == id)
            .unwrap_or_else(|| panic!("no row {id}"))
            .custom
    };
    assert!(!custom("plain"), "a title alone is the standard cell");
    for id in ["ruled", "unruled", "framed", "pictured"] {
        assert!(custom(id), "{id}: carried, its box drawn as written");
    }
}
