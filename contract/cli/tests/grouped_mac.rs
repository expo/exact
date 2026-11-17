//! LLP 1084 D7 on the Mac's grouped form (LLP 1115.002 D6): a row keeps
//! its 36 points under `border-box`, rows stand a point apart, and the
//! separator is drawn in that gap, 10 points in from both ends of the card.
//! Its own process: every test uses the Mac metric column.
use exact_kernel::style::metrics::{set_column, Column};
use exact_kernel::{Color, ColorValue, Kernel, Offer};
use exact_runner::{DataError, DataSource, Runner, Value};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

const SOURCE: &str = r#"style Native
  appearance="auto"
component App
  action go
  view
    column testId="root" width=402 height=874
      list appearance="auto" testId="list" flex=1
        section
          row testId="first"
            view width=4 height=4
          row width="100%" testId="full"
            image "symbol:sf/star"
            text "Full width" flex=1
            text "42" testId="value"
          button press=go testId="last"
            text "About"
          button press=go destructive=true testId="delete"
            text "Delete"
          row delete=go testId="swiped"
            text "Swiped"
          button appearance="auto" press=go width="100%" testId="native-full"
            text "Wide"
          button class=Native press=go destructive=true width="100%" testId="native-pushed"
            text "Erase"
          scroll swipeContent="written-row" swipeTrailing="written-act" testId="written" width="100%" overflow-x="scroll" overflow-y="hidden" display="flex"
            row id="written-row" width="100%" flex-shrink=0 testId="written-row"
              text "Written"
            button id="written-act" press=go flex-shrink=0
              text "Flag"
          row
            column background-color="transparent" testId="ground"
              text "Title" testId="ground-title"
              text "Detail" testId="ground-detail"
          row testId="end"
            text "End"
"#;

#[test]
fn the_macs_rows_keep_their_height_and_their_separator_stands_in_the_gap() {
    set_column(Column::Mac);
    let plan = contract::bake(contract::compile(SOURCE).unwrap(), NoData).unwrap();
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel_mut();
    let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
    k.compute_layout(root, Offer::definite(402.0, 874.0))
        .unwrap();
    let k = r.kernel();
    let id = |t: &str| k.node_by_key(k.find_by_test_id(t)[0]).unwrap().id;
    let frame = |t: &str| k.node(id(t)).unwrap().frame;
    let (first, full) = (frame("first"), frame("full"));
    assert_eq!(
        first.height, 36.0,
        "a row is 36 points at least, its padding in it"
    );
    assert_eq!(full.y - first.y, 37.0, "rows a point apart");
    assert_eq!(
        (full.x, full.width),
        (20.0, 362.0),
        "a full-width row is its card's width, 20 in"
    );
    let value = frame("value");
    assert!(
        value.x + value.width <= 382.0 - 10.0 + 0.01,
        "its content ends 10 short of the card's edge: {value:?}"
    );
    let line = k.grouped_separator(id("first")).unwrap();
    assert_eq!(
        (line.rect.x, line.rect.y, line.rect.width, line.rect.height),
        (10.0, 36.0, 342.0, 1.0),
        "in the gap under the row, 10 in from both ends"
    );
    assert_eq!(
        line.color,
        ColorValue::LightDark(
            Color::parse_hex("#efefef").unwrap(),
            Color::parse_hex("#333333").unwrap()
        ),
        "the measured form's separator"
    );
    // A destructive row is the Mac's push button: its box at the row's
    // inset, the button's padding inside it, its height the button's.
    let delete = k.node(id("delete")).unwrap();
    assert_eq!(delete.frame.x, 30.0, "10 into the card");
    assert_eq!(delete.frame.width, 342.0);
    assert_eq!(k.resolved_padding(delete.key).unwrap().0, 12.0);
    let line = k.grouped_separator(id("delete")).unwrap().rect;
    assert_eq!(
        (line.x, line.y, line.width),
        (0.0, delete.frame.height, 342.0),
        "its line spans the button's box, 10 in from both ends, in the gap"
    );
    // A `delete=` row's swipe clips what it holds: its line is its own foot.
    let swipe = k
        .node(k.node(id("swiped")).unwrap().parent.unwrap())
        .unwrap();
    let line = k.grouped_separator(swipe.id).unwrap().rect;
    assert_eq!(
        line.y,
        swipe.frame.height - 1.0,
        "inside the swipe that clips"
    );
    // A native button as wide as its row ends with the card, as an unsized
    // one at its own size may: no wider than the card less its inset.
    let wide = frame("native-full");
    assert_eq!((wide.x, wide.width), (30.0, 352.0));
    // A destructive one a class made native is inset by its push button's
    // padding, and ends with the card too.
    let pushed = frame("native-pushed");
    assert_eq!(pushed.x + pushed.width, 382.0, "{pushed:?}");
    // A swipe written by hand, as wide as its card: 10 in, as the
    // expansion's, and ending with the card.
    let written = frame("written");
    assert_eq!(
        (written.x, written.x + written.width),
        (30.0, 382.0),
        "{written:?}"
    );
    // The card, the rows' box, has its corners on SwiftUI's continuous
    // curve, not a circle's.
    let card = k
        .node(k.node(id("first")).unwrap().parent.unwrap())
        .unwrap();
    assert_eq!(
        card.style.border_radius_top_left,
        exact_kernel::Dimension::Points(10.0)
    );
    assert!(
        card.style.rare.corner_shape.is_apple_continuous(),
        "{:?}",
        card.style.rare.corner_shape
    );
    let ground = k.node(id("ground")).unwrap();
    let row = k.grouped_list(id("list")).unwrap().sections[0]
        .rows
        .iter()
        .find(|r| r.view == ground.parent.unwrap())
        .unwrap()
        .clone();
    assert!(row.custom);
    assert!(!row.subtitle);
    let padding = k.resolved_padding(ground.key).unwrap();
    assert_eq!((padding.1, padding.3), (0.0, 0.0));
    assert!(!ground.style.mask.has(exact_kernel::StyleId::PaddingTop));
    let title = frame("ground-title");
    let detail = frame("ground-detail");
    assert!(
        (detail.y - title.y - title.height - 3.0).abs() < 0.001,
        "a background keeps the platform interline gap"
    );
}

#[test]
fn boxes_keep_the_macs_subtitle_gap_and_padding_for_each_arm() {
    set_column(Column::Mac);
    for (own, states, opaque) in [
        ("class=(show ? Tile : Plain)", [false, true, false], false),
        (
            "class=Tile height=(show ? 300 : none)",
            [false, true, false],
            false,
        ),
        (
            "class=Tile height=(match h { case some(n) => n, case none => none })",
            [false, true, false],
            true,
        ),
        ("height=(show ? 300 : none)", [false, true, false], false),
        (
            "class=Rounded border-radius=(show ? 16 : none)",
            [false, true, false],
            false,
        ),
        (
            "height=(match h { case some(n) => n, case none => none })",
            [false, true, false],
            true,
        ),
        (
            "height=(match [300].at(show ? 0 : 1) { case some(n) => n, case none => none })",
            [false, true, false],
            true,
        ),
        ("height=(true ? none : 300)", [false; 3], false),
        ("height=300", [true; 3], false),
    ] {
        for on_row in [false, true] {
            let (row, column) = if on_row { (own, "") } else { ("", own) };
            let source = format!(
                r#"style Tile
  height=300
style Rounded
  border-top-left-radius=16
style Plain
  color="red"
component App
  state show = false
  state h = none
  action flip
    show = !show
    h = show ? none : some(300)
  view
    column testId="root" width=402 height=874
      button press=flip testId="flip"
        text "Flip"
      list appearance="auto" testId="list" flex=1
        section
          row {row}
            column {column} testId="tile"
              text "Title" testId="title"
              text "Detail" testId="detail"
"#
            );
            let plan = contract::bake(contract::compile(&source).unwrap(), NoData).unwrap();
            let mut r = Runner::boot(
                plan,
                NoData,
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap();
            for custom in states {
                let k = r.kernel_mut();
                let id =
                    |k: &Kernel, name: &str| k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id;
                k.compute_layout(id(k, "root"), Offer::definite(402.0, 874.0))
                    .unwrap();
                let row = &k.grouped_list(id(k, "list")).unwrap().sections[0].rows[0];
                assert_eq!(row.custom, custom, "{own}");
                assert_eq!(row.subtitle, !custom, "{own}");
                let tile = k.node(id(k, "tile")).unwrap();
                let padding = k.resolved_padding(tile.key).unwrap();
                assert_eq!(
                    (padding.1, padding.3),
                    (0.0, 0.0),
                    "the Mac's block metric is zero: {own}"
                );
                for side in [
                    exact_kernel::StyleId::PaddingTop,
                    exact_kernel::StyleId::PaddingBottom,
                ] {
                    assert_eq!(
                        tile.style.mask.has(side),
                        !custom || opaque,
                        "{own}, on_row={on_row}, custom={custom}: {side:?}"
                    );
                }
                let title = k.node(id(k, "title")).unwrap().frame;
                let detail = k.node(id(k, "detail")).unwrap().frame;
                assert!(
                    (detail.y - title.y - title.height - 3.0).abs() < 0.001,
                    "{own}, on_row={on_row}"
                );
                let flip = id(k, "flip");
                r.dispatch(flip, exact_runner::Event::Press).unwrap();
            }
        }
    }
}
