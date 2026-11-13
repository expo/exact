//! A grouped row's value on iOS (`grouped/value.rs`): beside its title
//! through the largest standard text size, under it at every accessibility
//! size, as UIKit's value cell stacks its two texts there
//! (`prefersSideBySideTextAndSecondaryText` off), however short the pair.
//! Grouped box and value layout share the iOS column in this process.
use exact_kernel::style::metrics::{set_column, Column};
use exact_kernel::{Kernel, Offer};
use exact_runner::{DataError, DataSource, Runner, Value};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const SOURCE: &str = r#"component App
  view
    column testId="root" width=402 height=874
      list appearance="auto" flex=1
        section
          row
            column
              text "Transport" testId="title"
            text "$12.50" testId="value"
"#;

#[test]
fn a_short_value_goes_under_its_title_at_the_accessibility_sizes() {
    set_column(Column::Ios);
    let plan = contract::bake(contract::compile(SOURCE).unwrap(), NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let at = |r: &mut Runner<NoData>, body: f64| {
        r.set_root_font_size(body).unwrap();
        let k = r.kernel_mut();
        let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
        k.compute_layout(root, Offer::definite(402.0, 874.0))
            .unwrap();
        let frame = |t: &str| {
            k.node(k.node_by_key(k.find_by_test_id(t)[0]).unwrap().id)
                .unwrap()
                .frame
        };
        (frame("title"), frame("value"))
    };
    // The largest standard size (xxxLarge, a 23-point body): side by side.
    let (title, value) = at(&mut r, 23.0);
    assert!(
        value.y < title.y + title.height,
        "beside it: {title:?} {value:?}"
    );
    assert!(
        value.x > title.x + title.width,
        "at the far edge: {title:?} {value:?}"
    );
    // Each accessibility size (AX1 to AX5): under it, at its leading edge.
    for body in [28.0, 33.0, 40.0, 47.0, 53.0] {
        let (title, value) = at(&mut r, body);
        assert!(
            value.y >= title.y + title.height,
            "{body}: under it: {title:?} {value:?}"
        );
        assert_eq!(value.x, title.x, "{body}: at its leading edge");
    }
}

fn box_app(row: &str) -> Runner<NoData> {
    set_column(Column::Ios);
    let row = row
        .lines()
        .map(|l| format!("          {l}\n"))
        .collect::<String>();
    let source = format!("style Tile\n  height=300\nstyle Rounded\n  border-top-left-radius=16\nstyle Plain\n  color=\"red\"\ncomponent App\n  state show = false\n  state h = none\n  action flip\n    show = !show\n    h = show ? none : some(300)\n  action go\n  view\n    column testId=\"root\" width=402 height=874\n      button press=flip testId=\"flip\"\n        text \"Flip\"\n      list appearance=\"auto\" testId=\"list\" flex=1\n        section\n{row}");
    let plan = contract::bake(contract::compile(&source).unwrap(), NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let root = node(&r, "root");
    r.kernel_mut()
        .compute_layout(root, Offer::definite(402.0, 874.0))
        .unwrap();
    r
}
fn node(r: &Runner<NoData>, name: &str) -> u32 {
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}
#[test]
fn a_direct_column_keeps_authored_alignment_and_a_plain_minimum_stays_standard() {
    for own in [
        "align-items=\"center\"",
        "justify-content=\"flex-end\"",
        "align-items=\"center\" justify-content=\"flex-end\"",
        "",
    ] {
        let r = box_app(&format!(
            "column min-height=300 {own}\n  text \"Puzzle\"\n  text \"Detail\""
        ));
        let list = r.kernel().grouped_list(node(&r, "list")).unwrap();
        assert_eq!(list.sections[0].rows[0].custom, !own.is_empty(), "{own}");
    }
}
#[test]
fn authored_tiles_have_a_300_point_border_box() {
    for (row, column) in [
        ("", "height=300 align-items=\"center\" justify-content=\"center\" border-radius=16 background-color=\"-exact-fill\""),
        ("", "min-height=300"), ("", "class=Tile"),
        ("height=300", "height=\"100%\""), ("height=300", ""),
        ("class=Tile", "height=\"100%\""),
        ("delete=go", "class=Tile"), ("class=Tile delete=go", "height=\"100%\""),
    ] {
        let r = box_app(&format!("row {row} testId=\"row\"\n  column {column} flex=1 testId=\"tile\"\n    text \"Puzzle\"\n    text \"A 300-point tile\""));
        let k = r.kernel();
        let tile = k.node(node(&r, "tile")).unwrap();
        if !column.is_empty() {
            assert_eq!(tile.frame.height, 300.0, "row {row}, column {column}");
        }
        assert!(k.grouped_list(node(&r, "list")).unwrap().sections[0].rows[0].custom);
        assert_eq!(k.node(node(&r, "row")).unwrap().frame.height, 300.0, "row {row}, column {column}");
        assert_eq!(k.resolved_padding(tile.key).unwrap().1, 0.0, "no subtitle top padding");
        assert_eq!(k.resolved_padding(tile.key).unwrap().3, 0.0, "no subtitle bottom padding");
    }
}

#[test]
fn nullable_boxes_keep_padding_unless_their_presence_is_known() {
    for (own, boxed_padding) in [
        (
            "height=(match [300].at(show ? 0 : 1) { case some(n) => n, case none => none })",
            15.0,
        ),
        (
            "height=(match h { case some(n) => n, case none => none })",
            15.0,
        ),
        (
            "height=(match h { case some(n) => 300, case none => none })",
            15.0,
        ),
        ("class=(show ? Tile : Plain)", 0.0),
        ("class=Tile height=(show ? 300 : none)", 0.0),
        (
            "class=Tile height=(match h { case some(n) => n, case none => none })",
            15.0,
        ),
        ("height=(show ? 300 : none)", 0.0),
        ("background-color=(show ? \"transparent\" : none)", 0.0),
        ("border-radius=(show ? 16 : none)", 0.0),
        ("class=Rounded border-radius=(show ? 16 : none)", 0.0),
        ("min-height=(show ? 300 : none)", 0.0),
        ("align-items=(show ? \"center\" : none)", 0.0),
        ("justify-content=(show ? \"center\" : none)", 0.0),
    ] {
        for on_row in [false, true] {
            if on_row
                && (own.starts_with("min-height")
                    || own.starts_with("align-items")
                    || own.starts_with("justify-content"))
            {
                continue;
            }
            let (row, column) = if on_row { (own, "") } else { ("", own) };
            let mut r = box_app(&format!("row {row}\n  column {column} testId=\"tile\"\n    text \"Title\"\n    text \"Detail\""));
            for custom in [false, true, false] {
                let root = node(&r, "root");
                r.kernel_mut()
                    .compute_layout(root, Offer::definite(402.0, 874.0))
                    .unwrap();
                let k = r.kernel();
                let tile = k.node(node(&r, "tile")).unwrap();
                let padding = k.resolved_padding(tile.key).unwrap();
                assert_eq!(
                    k.grouped_list(node(&r, "list")).unwrap().sections[0].rows[0].custom,
                    custom,
                    "{own}, on_row={on_row}"
                );
                assert_eq!(
                    k.grouped_list(node(&r, "list")).unwrap().sections[0].rows[0].subtitle,
                    !custom
                );
                assert_eq!(
                    (padding.1, padding.3),
                    if custom {
                        (boxed_padding, boxed_padding)
                    } else {
                        (15.0, 15.0)
                    },
                    "{own}, on_row={on_row}, custom={custom}"
                );
                r.dispatch(node(&r, "flip"), exact_runner::Event::Press)
                    .unwrap();
            }
        }
    }
}

#[test]
fn literal_absent_boxes_keep_standard_subtitle_padding() {
    let r = box_app("row height=(true ? none : 300)\n  column height=(true ? none : 300) testId=\"tile\"\n    text \"Title\"\n    text \"Detail\"");
    let k = r.kernel();
    let row = &k.grouped_list(node(&r, "list")).unwrap().sections[0].rows[0];
    assert!(!row.custom);
    assert!(row.subtitle);
    let padding = k
        .resolved_padding(k.node(node(&r, "tile")).unwrap().key)
        .unwrap();
    assert_eq!((padding.1, padding.3), (15.0, 15.0));
}
