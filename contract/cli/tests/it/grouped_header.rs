//! A grouped list's section `header` is one `text`, UIKit's own header, or
//! boxes of its author's (an image beside a text), whose views a host that
//! draws the platform's list carries into the header (`header_view`).

use super::grouped_list::{boot_source, id};
use exact_kernel::{GroupedMargins, Offer, TextLook};

const FAVORITES: &str = r##"component App
  state groups = ["Ryu", "Ken"]
  state moves = ["a", "b"]
  view
    column testId="root" width=402 height=874
      list id="favorites" testId="list" appearance="auto" listStyle="plain" flex=1 min-height=0
        for g in groups key=g
          section
            header testId=`header-${g}`
              row gap=8 align-items="center"
                column testId=`avatar-${g}` width=24 height=24 border-radius=4 overflow="hidden" background-color=(g == "Ryu" ? "#ff0000" : "#00ff00") aria-hidden=true
                  image `assets/characters/${g}.png` width="100%" height="100%" object-fit="contain"
                  text "hidden"
                text g testId=`name-${g}` font="-exact-subheadline" font-weight=600 color=(g == "Ryu" ? "#ff0000" : "#00ff00")
            for r in moves key=r
              link href=`/moves/${r}`
                text r
        section
          header testId="plain"
            text "Plain"
          row
            text "A row"
"##;

/// A section of one row with `label` (a `header`, or a `footer` after the row).
fn section(label: &str) -> String {
    let label = label
        .lines()
        .map(|l| format!("          {l}\n"))
        .collect::<String>();
    let row = "          row\n            text \"x\"\n";
    let parts = if label.trim_start().starts_with("footer") {
        format!("{row}{label}")
    } else {
        format!("{label}{row}")
    };
    format!(
        "component App\n  state on = true\n  view\n    column testId=\"root\" width=402 height=874\n      list appearance=\"auto\"\n        section\n{parts}"
    )
}

fn refused(label: &str) -> String {
    let e = contract::compile(&section(label)).unwrap_err();
    assert_eq!(e.id, "lower-grouped-list", "{}", e.message);
    e.message
}

#[test]
fn a_header_of_an_image_beside_a_text_is_custom_and_one_text_is_not() {
    let r = boot_source(FAVORITES);
    let k = r.kernel();
    let list = k.grouped_list(id(&r, "list")).unwrap();
    let (ryu, plain) = (&list.sections[0], &list.sections[2]);
    assert_eq!(ryu.header_view, Some(id(&r, "header-Ryu")));
    assert_eq!(
        ryu.header.as_deref(),
        Some("Ryu"),
        "its texts are its name, the `aria-hidden` avatar's none of it"
    );
    assert_eq!(
        ryu.header_look,
        TextLook::default(),
        "a custom header's texts are set by their own views"
    );
    assert_eq!(ryu.rows.len(), 2);
    assert_eq!(list.sections[1].header_view, Some(id(&r, "header-Ken")));

    assert_eq!(plain.header_view, None, "one text is UIKit's header");
    assert_eq!(plain.header.as_deref(), Some("Plain"));

    let frame = |t: &str| k.node(id(&r, t)).unwrap().frame;
    let (avatar, name) = (frame("avatar-Ryu"), frame("name-Ryu"));
    assert_eq!((avatar.width, avatar.height), (24.0, 24.0));
    assert_eq!(name.x - (avatar.x + avatar.width), 8.0, "the row's gap");
}

#[test]
fn a_custom_header_starts_at_the_platform_lists_margins() {
    let mut r = boot_source(FAVORITES);
    let content = |r: &exact_runner::Runner<super::grouped_list::NoData>, t: &str| {
        let k = r.kernel();
        let node = k.node(id(r, t)).unwrap();
        let (left, right) = {
            let p = k.resolved_padding(node.key).unwrap();
            (p.0, p.2)
        };
        (left, right)
    };
    let before = content(&r, "header-Ryu");
    assert_eq!(before.0, 16.0, "the sheet's stand-in");
    let list = id(&r, "list");
    let k = r.kernel_mut();
    k.set_grouped_margins(
        list,
        Some(GroupedMargins {
            card: [0.0, 0.0],
            cell: [20.0, 24.0],
        }),
    )
    .unwrap();
    let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
    k.compute_layout(root, Offer::definite(402.0, 874.0))
        .unwrap();
    assert_eq!(content(&r, "header-Ryu"), (20.0, 24.0));
}

/// UIKit's header view in an inset-grouped list spans the card, so a hand-built
/// header's own inset (16 points) is from the card's edge, not the list's: on
/// every host, before a host says its margins and after.
#[test]
fn a_custom_headers_written_inline_padding_is_from_the_cards_edge() {
    let source = |style: &str, padding: &str| {
        format!(
            "component App\n  view\n    column testId=\"root\" width=402 height=874\n      list testId=\"list\" appearance=\"auto\" listStyle=\"{style}\" flex=1\n        section\n          header testId=\"header\"{padding}\n            row gap=8 align-items=\"center\"\n              text \"Ken\" testId=\"title\"\n              text \"25\" testId=\"count\"\n          row\n            text \"x\"\n"
        )
    };
    // The header's content edges, from the section's (the list's) left.
    let ends = |style: &str, padding: &str, margins: Option<GroupedMargins>| {
        let mut r = boot_source(&source(style, padding));
        if margins.is_some() {
            let list = id(&r, "list");
            let k = r.kernel_mut();
            k.set_grouped_margins(list, margins).unwrap();
            let root = k.node_by_key(k.find_by_test_id("root")[0]).unwrap().id;
            k.compute_layout(root, Offer::definite(402.0, 874.0))
                .unwrap();
        }
        let k = r.kernel();
        let header = k.node(id(&r, "header")).unwrap();
        let (f, p) = (header.frame, k.resolved_padding(header.key).unwrap());
        (f.x + p.0, f.x + f.width - p.2)
    };
    let uikit = Some(GroupedMargins {
        card: [16.0, 16.0],
        cell: [20.0, 20.0],
    });
    let both = " padding-left=16 padding-right=16";
    assert_eq!(
        ends("inset-grouped", "", uikit),
        (36.0, 366.0),
        "the card's and the cell's"
    );
    assert_eq!(
        ends("inset-grouped", both, None),
        (32.0, 370.0),
        "the sheet's card, no host's"
    );
    assert_eq!(ends("inset-grouped", both, uikit), (32.0, 370.0));
    assert_eq!(
        ends("inset-grouped", " padding=\"12px 16px\"", uikit),
        (32.0, 370.0),
        "the shorthand"
    );
    assert_eq!(
        ends("inset-grouped", " padding-left=16", uikit),
        (32.0, 366.0),
        "an unsaid side keeps the platform's"
    );
    let uneven = Some(GroupedMargins {
        card: [10.0, 30.0],
        cell: [20.0, 20.0],
    });
    assert_eq!(
        ends("inset-grouped", both, uneven),
        (26.0, 356.0),
        "each side its own card"
    );
    let plain = Some(GroupedMargins {
        card: [0.0, 0.0],
        cell: [20.0, 20.0],
    });
    assert_eq!(
        ends("plain", both, plain),
        (16.0, 386.0),
        "no card: from the list's edge"
    );
}

#[test]
fn a_header_still_refuses_texts_with_no_layout_and_a_footer_boxes() {
    assert!(refused("header\n  text \"a\"\n  text \"b\"").contains("lay them out in a `row`"));
    assert!(
        refused("header\n  if on\n    text \"a\"").contains("two `section`s"),
        "a header that comes and goes"
    );
    assert!(
        refused("header\n  if on\n    text \"a\" color=\"#f00\"\n  else\n    text \"b\"")
            .contains("as one `text`"),
        "texts an `if` chooses, which do not fold"
    );
    let footer = refused("footer\n  row\n    image \"a.png\" width=24 height=24\n    text \"a\"");
    assert!(
        footer.contains("one `text`") && !footer.contains("boxes of your own"),
        "{footer}"
    );
    for header in [
        "header\n  row\n    image \"a.png\" width=24 height=24\n    text \"a\"",
        "header\n  image \"a.png\" width=24 height=24\n  if on\n    text \"a\"",
        "header\n  row\n    text \"a\"\n    text \"b\"",
    ] {
        contract::compile(&section(header)).unwrap_or_else(|e| panic!("{header}: {e}"));
    }
}

/// UIKit's plain header insets its content 10 points above and below (the
/// sheet's stand-in); an app whose header has its own (a hand-built header
/// view's 12) writes them on the `header`.
#[test]
fn a_custom_headers_block_padding_is_the_sheets_unless_its_author_writes_one() {
    let source = |padding: &str| {
        format!(
            "component App\n  view\n    column testId=\"root\" width=402 height=874\n      list appearance=\"auto\" listStyle=\"plain\" flex=1\n        section\n          header testId=\"header\"{padding}\n            row gap=8 align-items=\"center\"\n              column width=24 height=24\n              text \"Ryu\"\n          row\n            text \"x\"\n"
        )
    };
    let height = |padding: &str| {
        let r = boot_source(&source(padding));
        let frame = r.kernel().node(id(&r, "header")).unwrap().frame;
        frame.height
    };
    assert_eq!(height(""), 44.0);
    assert_eq!(height(" padding-top=12 padding-bottom=12"), 48.0);
}
