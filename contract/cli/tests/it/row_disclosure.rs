//! LLP 1084.000 D1, D5: a row's disclosure is the literal forward chevron a
//! list's row owns, marked once by Contract (`rowDisclosure`) with its own mode
//! and drawn as the portable role.

use crate::source::{boxed, NoData, Source};
use exact_kernel::{Kernel, Offer, PropId};
use exact_runner::Runner;

fn boot(source: &str) -> Runner<Source> {
    let plan = contract::bake(
        contract::compile(source).unwrap_or_else(|e| panic!("{e}")),
        boxed(NoData),
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        boxed(NoData),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let k = r.kernel_mut();
    let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
    k.compute_layout(root, Offer::definite(402.0, 874.0))
        .unwrap();
    r
}

fn id(r: &Runner<Source>, test_id: &str) -> u32 {
    let k = r.kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// The mark on the image `test_id` names, and its source.
fn mark(r: &Runner<Source>, test_id: &str) -> (Option<String>, Option<String>) {
    let node = r.kernel().node(id(r, test_id)).unwrap();
    (
        node.props.str(PropId::RowDisclosure).map(str::to_owned),
        node.props.str(PropId::ImageSource).map(str::to_owned),
    )
}

fn semantic(r: &Runner<Source>, test_id: &str) {
    assert_eq!(
        mark(r, test_id),
        (
            Some("semantic".into()),
            Some("symbol:forward-chevron".into())
        ),
        "{test_id}"
    );
}

fn artwork(r: &Runner<Source>, test_id: &str) {
    assert_eq!(mark(r, test_id).0, None, "{test_id} is artwork");
}

/// Each refusal compiling `source` gives: its id, line, first column and the column past it, in source order.
fn refusals(source: &str) -> Vec<(String, (u32, u32, u32))> {
    let mut all: Vec<_> =
        contract::compile_path_source_all(std::path::Path::new("app.contract"), source, false)
            .err()
            .expect("refused")
            .into_iter()
            .map(|e| (e.id, (e.span.line, e.span.col, e.span.end_col)))
            .collect();
    all.sort_by_key(|(_, at)| *at);
    all
}

/// Where `text` first stands on line `line` of `source`, as a span reports it.
fn span_of(source: &str, line: u32, text: &str) -> (u32, u32, u32) {
    let col = source
        .lines()
        .nth(line as usize - 1)
        .unwrap()
        .find(text)
        .unwrap() as u32
        + 1;
    (line, col, col + text.len() as u32)
}

/// Each row's chevron is marked, whatever the row's root, through boxes and
/// every kind of branch, in whichever arm shows and in each repeated
/// instance; a `sf/` spelling is drawn as the portable role.
#[test]
fn every_chevron_a_row_owns_is_marked_through_its_boxes_and_branches() {
    for on in [true, false] {
        let source = format!(
            r#"component App
  state on = {on}
  state picked = {picked}
  action pick
    picked = some("x")
  view
    column testId="root"
      list
        link href="/a"
          text "A"
          image "symbol:forward-chevron" testId="link-row"
        row
          text "Wrapped"
          column
            image "symbol:sf/chevron.right" testId="wrapped"
        row
          text "Branch"
          if on
            image "symbol:forward-chevron" testId="then"
          else
            image "symbol:forward-chevron" disclosure="always" testId="else"
        row
          text "Match"
          match picked
            case some(p)
              image "symbol:sf/chevron.forward" testId="some"
            case none
              image "symbol:forward-chevron" disclosure="always" testId="none"
        row
          text "Each"
          for x in ["one", "two"] key=x
            image "symbol:forward-chevron" testId=`each-${{x}}`
        image "symbol:forward-chevron" testId="bare-row"
"#,
            picked = if on { r#"some("b")"# } else { "none" }
        );
        let r = boot(&source);
        for site in ["link-row", "wrapped", "each-one", "each-two", "bare-row"] {
            semantic(&r, site);
        }
        if on {
            semantic(&r, "then");
            semantic(&r, "some");
        } else {
            for site in ["else", "none"] {
                assert_eq!(
                    mark(&r, site),
                    (Some("always".into()), Some("symbol:forward-chevron".into())),
                    "{site}"
                );
            }
        }
    }
}

/// A component's row is a row: `ListRow`'s `button`, its chevron in an `if`.
#[test]
fn a_components_row_marks_its_conditional_chevron() {
    let r = boot(
        r#"component App
  view
    column testId="root"
      list
        Row(title="One", accessory="chevron")
        Row(title="Two", accessory="none")

component Row
  props
    title: string
    accessory: string
  view
    button testId=`row-${title}`
      text title
      if accessory == "chevron"
        image "symbol:forward-chevron" testId=`chevron-${title}`
"#,
    );
    semantic(&r, "chevron-One");
}

/// A section's body rows are rows however the section is drawn; its header and
/// footer are not.
#[test]
fn a_sections_rows_hold_marks_and_its_header_and_footer_do_not() {
    let r = boot(
        r#"component App
  view
    column testId="root"
      list appearance="none"
        section appearance="none"
          header
            row
              text "Header"
              image "symbol:forward-chevron" testId="header"
          link href="/b"
            text "B"
            image "symbol:forward-chevron" testId="body"
          footer
            row
              text "Footer"
              image "symbol:forward-chevron" testId="footer"
"#,
    );
    semantic(&r, "body");
    artwork(&r, "header");
    artwork(&r, "footer");
}

/// A swipe row's disclosure is in its content: a `delete=` row's, and the
/// content a hand-written swipe row names, not its action side.
#[test]
fn a_swipe_rows_content_holds_its_mark_and_its_actions_do_not() {
    let r = boot(
        r#"component App
  state n = 0
  action remove
    n = n + 1
  view
    column testId="root"
      list
        link href="/b" delete=remove
          text "B"
          image "symbol:forward-chevron" testId="deleted"
        scroll swipeContent="content-c" height=44 testId="swiped"
          link href="/c" id="content-c"
            text "C"
            image "symbol:forward-chevron" testId="content"
          button press=remove
            text "More"
            image "symbol:forward-chevron" testId="action"
"#,
    );
    semantic(&r, "deleted");
    semantic(&r, "content");
    artwork(&r, "action");
}

/// The walk goes through a control that carries the row's texts and stops at
/// one that does not (a labelled More beside the row's title, an expander whose
/// face is a branch's chevron), at an image with a press of its own and at a
/// popover; a nested list's rows, and a list that is itself a row, are their
/// own; a chevron spelled by a template is artwork.
#[test]
fn only_a_control_carrying_the_rows_texts_owns_its_chevron() {
    let r = boot(
        r#"component App
  state name = "forward-chevron"
  state open = true
  state n = 0
  action next
    n = n + 1
  view
    column testId="root"
      list
        row
          link href="/c" flex=1
            text "Carried"
            image "symbol:forward-chevron" testId="carried"
          button press=next aria-label="Star"
            image "symbol:star"
        row
          text "Title"
          button press=next
            text "More"
            image "symbol:forward-chevron" testId="more"
        row
          text "Expander"
          button press=next aria-label="Expand"
            if open
              image "symbol:forward-chevron" testId="expander"
        row
          text "Pressed"
          image "symbol:forward-chevron" press=next testId="pressed"
        row
          text "Popover"
          column popover="auto" id="pop"
            row
              text "In the popover"
              image "symbol:forward-chevron" testId="in-popover"
        row
          text "Computed"
          image `symbol:${name}` testId="computed"
        list
          row
            text "Inner"
            image "symbol:forward-chevron" testId="inner"
        row
          link href="/p" flex=1
            p "Carried as a paragraph"
            image "symbol:forward-chevron" testId="carried-p"
          button press=next aria-label="Star"
            image "symbol:star"
        row
          p "Paragraph title"
          button press=next
            text "More"
            image "symbol:forward-chevron" testId="more-beside-p"
        row
          time datetime=0 format="relative" testId="when"
          button press=next
            strong "More"
            image "symbol:forward-chevron" testId="more-beside-time"
        list popover="auto" id="menu-list"
          row
            text "In a list that is a popover"
            image "symbol:forward-chevron" testId="in-popover-list"
"#,
    );
    semantic(&r, "carried");
    semantic(&r, "inner");
    semantic(&r, "carried-p");
    for site in [
        "more",
        "expander",
        "pressed",
        "in-popover",
        "computed",
        "more-beside-p",
        "more-beside-time",
        "in-popover-list",
    ] {
        artwork(&r, site);
    }
}

/// Every portable alias of the forward chevron is a mark.
#[test]
fn every_alias_of_the_forward_chevron_is_marked() {
    let aliases = [
        "forward-chevron",
        "sf/chevron.forward",
        "sf/chevron.right",
        "chevron.forward",
        "chevron.right",
        "chevron-right",
        "arrow-forward-ios",
        "navigate-next",
    ];
    let rows: String = aliases
        .iter()
        .enumerate()
        .map(|(i, a)| format!("        row\n          text \"R{i}\"\n          image \"symbol:{a}\" testId=\"alias-{i}\"\n"))
        .collect();
    let r = boot(&format!(
        "component App\n  view\n    column testId=\"root\"\n      list\n{rows}"
    ));
    for i in 0..aliases.len() {
        semantic(&r, &format!("alias-{i}"));
    }
}

/// `disclosure="always"` on a mark says `always`; elsewhere it is refused
/// alone, at the attribute (a presentation's own image included), and any other
/// value at the value, among the view's other refusals.
#[test]
fn disclosure_always_is_a_marks_mode_and_is_refused_elsewhere() {
    let r = boot(
        r#"component App
  view
    column testId="root"
      list
        row
          text "Always"
          image "symbol:forward-chevron" disclosure="always" testId="always"
"#,
    );
    assert_eq!(mark(&r, "always").0.as_deref(), Some("always"));
    let row = |line: &str| {
        format!("component App\n  view\n    column\n      list\n        row\n          {line}\n      text \"x\" bogus=1\n")
    };
    let placement = "lower-disclosure-placement".to_owned();
    let (value, unknown) = (
        "lower-disclosure-value".to_owned(),
        "lower-unknown-attr".to_owned(),
    );
    let bogus = |s: &str| (unknown.clone(), span_of(s, 7, "bogus"));
    let misplaced = row("text \"A\" disclosure=\"none\"");
    assert_eq!(
        refusals(&misplaced),
        [
            (placement.clone(), span_of(&misplaced, 6, "disclosure")),
            bogus(&misplaced)
        ]
    );
    let popover =
        row("image \"symbol:forward-chevron\" popover=\"auto\" id=\"pop\" disclosure=\"always\"");
    assert_eq!(
        refusals(&popover),
        [
            (placement, span_of(&popover, 6, "disclosure")),
            bogus(&popover)
        ]
    );
    // A section that is itself a presentation holds no rows
    let section = "component App\n  view\n    column\n      list appearance=\"none\"\n        section popover=\"auto\" id=\"details\"\n          row\n            image \"symbol:forward-chevron\" disclosure=\"always\"\n      text \"x\" bogus=1\n";
    assert_eq!(
        refusals(section),
        [
            (
                "lower-disclosure-placement".to_owned(),
                span_of(section, 7, "disclosure")
            ),
            (unknown.clone(), span_of(section, 8, "bogus"))
        ]
    );
    let none = row("image \"symbol:forward-chevron\" disclosure=\"none\"");
    assert_eq!(
        refusals(&none),
        [(value.clone(), span_of(&none, 6, "\"none\"")), bogus(&none)]
    );
    let computed = row("image \"symbol:forward-chevron\" disclosure=true");
    assert_eq!(
        refusals(&computed),
        [(value, span_of(&computed, 6, "true")), bogus(&computed)]
    );
    let message = |source: &str| {
        contract::compile_path_source_all(std::path::Path::new("app.contract"), source, false)
            .err()
            .unwrap()[0]
            .message
            .clone()
    };
    assert!(message(&misplaced).contains("Remove the attribute"));
    assert!(message(&none).contains("Omit the attribute"));
    assert!(message(&computed).contains("literal string"));
}

/// A cell row that is a chevron image with `delete=` compiles: its swipe scroll
/// keeps the image's identity and takes no mark, its image does.
#[test]
fn a_chevron_image_row_with_a_delete_keeps_its_mark_on_the_image() {
    let r = boot(
        r#"component App
  state n = 0
  action remove
    n = n + 1
  view
    column testId="root"
      list
        image "symbol:forward-chevron" delete=remove testId="image-row"
"#,
    );
    semantic(&r, "image-row");
    let k = r.kernel();
    let scroll = k.node(id(&r, "image-row")).unwrap().parent.unwrap();
    let scroll = &k.node(scroll).unwrap().props;
    assert!(
        scroll.str(PropId::SwipeContent).is_some() && scroll.str(PropId::SwipeTrailing).is_some()
    );
    assert_eq!(scroll.str(PropId::RowDisclosure), None);
}

/// A hand-written swipe row's content is found by the name the kernel matches,
/// a template with nothing computed in it as the text it folds to, and its
/// trailing action's chevron is artwork.
#[test]
fn a_hand_written_swipe_rows_named_content_holds_its_mark() {
    let r = boot(
        r#"component App
  state n = 0
  action remove
    n = n + 1
  view
    column testId="root"
      list
        scroll swipeContent="body-1" swipeTrailing="act-a" height=44 width=300 overflow-x="scroll" overflow-y="hidden"
          row
            link href="/a" id=`body-${1}` width=300
              text "A"
              image "symbol:forward-chevron" disclosure="always" testId="content"
            button id="act-a" press=remove width=80
              text "Delete"
              image "symbol:forward-chevron" testId="action"
"#,
    );
    assert_eq!(mark(&r, "content").0.as_deref(), Some("always"));
    artwork(&r, "action");
}

/// A swipe row's named content keeps D1's boundaries: a presentation it names
/// is no row, so its chevron is artwork; a list it names walks its own rows,
/// where a glyph-only button row's chevron is the row's.
#[test]
fn a_swipe_rows_named_presentation_or_list_keeps_its_boundary() {
    let r = boot(
        r#"component App
  state n = 0
  action remove
    n = n + 1
  view
    column testId="root"
      list
        scroll swipeContent="pop" swipeTrailing="act-p" height=44 width=300 overflow-x="scroll" overflow-y="hidden"
          row
            column popover="auto" id="pop" width=300
              text "In the popover"
              image "symbol:forward-chevron" testId="in-popover"
            button id="act-p" press=remove width=80
              text "Delete"
        scroll swipeContent="body" swipeTrailing="act-l" height=44 width=300 overflow-x="scroll" overflow-y="hidden"
          row
            list id="body" width=300
              button press=remove aria-label="Open" testId="glyph-row"
                image "symbol:forward-chevron" testId="glyph"
            button id="act-l" press=remove width=80
              text "Delete"
"#,
    );
    artwork(&r, "in-popover");
    semantic(&r, "glyph");
}

/// The row's texts are those outside its presentations and nested lists: a
/// title link beside a popover with its own label still holds them all; a list
/// in a row walks its own rows; a row whose root's face is the chevron alone is
/// still a row.
#[test]
fn a_rows_texts_stop_at_its_popovers_and_nested_lists() {
    let r = boot(
        r#"component App
  state n = 0
  action open
    n = n + 1
  view
    column testId="root"
      list
        row
          link href="/t" flex=1
            text "Title"
            image "symbol:forward-chevron" testId="titled"
          column popover="auto" id="pop"
            text "In the popover"
            image "symbol:forward-chevron" testId="in-popover"
        row
          text "Holds a list"
          list
            row
              text "Nested"
              image "symbol:forward-chevron" testId="nested"
        button press=open aria-label="Open" testId="glyph-row"
          image "symbol:forward-chevron" testId="glyph-root"
"#,
    );
    semantic(&r, "titled");
    semantic(&r, "nested");
    semantic(&r, "glyph-root");
    artwork(&r, "in-popover");
}

/// Each site keeps its own mode across branch arms and repeated instances.
#[test]
fn each_site_keeps_its_mode_across_branches_and_repeats() {
    let r = boot(
        r#"component App
  view
    column testId="root"
      list
        for x in ["a", "b"] key=x
          row
            text x
            if x == "a"
              image "symbol:forward-chevron" disclosure="always" testId=`each-${x}`
            else
              image "symbol:forward-chevron" testId=`each-${x}`
"#,
    );
    assert_eq!(mark(&r, "each-a").0.as_deref(), Some("always"));
    assert_eq!(mark(&r, "each-b").0.as_deref(), Some("semantic"));
}

/// A slot's one fill placed both in a row and outside every row has one
/// identity and is artwork: its `disclosure` is refused once, at the attribute,
/// whatever its value; one placed twice outside lists is refused once too.
#[test]
fn a_slot_fill_placed_twice_is_refused_once() {
    let twice = |places: &str, value: &str| {
        format!("component App\n  view\n    column\n      Twice()\n        image \"symbol:forward-chevron\" disclosure=\"{value}\"\n\ncomponent Twice\n  slot\n  view\n    column\n{places}      children\n")
    };
    for source in [
        twice("      list\n        row\n          children\n", "none"),
        twice("      children\n", "always"),
    ] {
        assert_eq!(
            refusals(&source),
            [(
                "lower-disclosure-placement".to_owned(),
                span_of(&source, 5, "disclosure")
            )]
        );
    }
}
