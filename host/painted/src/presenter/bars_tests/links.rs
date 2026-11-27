//! A header's link as a top app bar item (LLP 1141.004 D1): what it
//! follows, what its press reaches first, and when it is no enabled item.
use super::*;

const LINKED: &str = r#"routes nav
  home "/"
    history "/history"
component App
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=`${top(nav).id}` navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          header display="flex"
            text (e.name == "home" ? "Inbox" : "History") role="heading" flex=1
            if e.name == "home"
              link href="/history" aria-label="History" testId="history"
                image "symbol:history"
"#;

/// A link with nothing pressing it is a top app bar item that follows its
/// `href` (LLP 1141.004 D1), as a tap on it in the page does.
#[test]
fn a_link_item_follows_its_href() {
    let (mut p, _) = barred(LINKED);
    assert_eq!(top(&mut p).title, "Inbox");
    assert_eq!(top(&mut p).trailing.len(), 1, "the link, an item");
    touch(&mut p, TOP, Some("history"));
    assert_eq!(top(&mut p).title, "History");
}

#[test]
fn a_button_with_a_link_opens_its_dialog_in_the_page_and_the_bar() {
    let source = LINKED.replace(
        "link href=\"/history\"",
        "button href=\"/history\" commandfor=\"details\" command=\"show-modal\" width=100 height=48",
    ) + r#"          dialog id="details" role="alertdialog" aria-label="Details" testId="details"
            text "Details about this item."
            button commandfor="details" command="close"
              text "OK"
"#;
    let page = source.replace("header display=", "header appearance=\"none\" display=");
    let (mut p, _) = barred(&page);
    p.frame();
    let button = id(&p, "history");
    let (x, y, w, h) = p.rect_of(button).expect("the page's button");
    assert_eq!(p.press_at(x + w / 2., y + h / 2., 0.), Some(button));
    p.run_commands(|| Nothing);
    assert_eq!(p.surface().unwrap().node, id(&p, "details"));

    let (mut p, _) = barred(&source);
    let dialog = id(&p, "details");
    touch(&mut p, TOP, Some("history"));
    assert_eq!(
        (p.surface().map(|s| s.node), top(&mut p).title.as_str()),
        (Some(dialog), "Inbox")
    );
}

#[test]
fn a_bar_item_inside_an_invoking_title_presses_its_own_handler() {
    let source = r#"routes nav
  home "/"
component App
  state saved = 0
  state opened = 0
  action save
    saved = saved + 1
  action open
    opened = opened + 1
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          header
            button press=open commandfor="details" command="show-modal" testId="title"
              text "Inbox" role="heading"
              row press=save testId="save"
                text "Save"
          text `${saved}` testId="saved"
          text `${opened}` testId="opened"
          dialog id="details" role="alertdialog" aria-label="Details" testId="details"
            text "Details about this item."
            button commandfor="details" command="close"
              text "OK"
"#;
    let (mut bar, _) = barred(source);
    touch(&mut bar, TOP, Some("save"));
    assert_eq!(text(&bar, "saved"), "1");
    assert_eq!(text(&bar, "opened"), "0");
    assert!(bar.surface().is_none());

    let (mut page, _) = barred(&source.replace("header\n", "header appearance=\"none\"\n"));
    tap(&mut page, "title").unwrap();
    assert_eq!(text(&page, "saved"), "0");
    assert_eq!(text(&page, "opened"), "1");
    assert_eq!(page.surface().unwrap().node, id(&page, "details"));
}

/// A press on the link goes where the page sends it, a listener above it,
/// before the link is followed.
#[test]
fn a_link_items_press_reaches_a_listener_above_it_before_it_follows() {
    let source = LINKED
        .replace(
            "component App\n",
            "component App\n  state noted = 0\n  action note\n    noted = noted + 1\n",
        )
        .replace(
            "header display=\"flex\"",
            "header display=\"flex\" press=note",
        )
        + "          text `${noted}` testId=\"noted\"\n";
    let (mut p, _) = barred(&source);
    touch(&mut p, TOP, Some("history"));
    assert_eq!(top(&mut p).title, "History");
    assert_eq!(text(&p, "noted"), "1", "the header heard the press");
}

/// An inert link is no enabled item and follows nothing.
#[test]
fn an_inert_link_item_is_dimmed_and_follows_nothing() {
    let source = LINKED.replace("testId=\"history\"", "testId=\"history\" inert=true");
    let (mut p, _) = barred(&source);
    assert!(!top(&mut p).trailing[0].enabled, "inert: no enabled item");
    touch(&mut p, TOP, Some("history"));
    assert_eq!(top(&mut p).title, "Inbox");
}

/// A link under a disabled header is no enabled item and follows nothing,
/// as a press on it in the page reaches nothing: `disabled` holds for what
/// is inside the element that carries it.
#[test]
fn a_link_item_under_a_disabled_header_is_dimmed_and_follows_nothing() {
    let source = LINKED.replace(
        "header display=\"flex\"",
        "header display=\"flex\" disabled=true",
    );
    let (mut p, _) = barred(&source);
    assert_eq!(top(&mut p).trailing.len(), 1, "the link, an item");
    assert!(
        !top(&mut p).trailing[0].enabled,
        "disabled above: no enabled item"
    );
    touch(&mut p, TOP, Some("history"));
    assert_eq!(top(&mut p).title, "Inbox");
    let refused = tap(&mut p, "history").unwrap_err();
    assert!(refused.contains("is disabled in the top bar"), "{refused}");
    assert_eq!(top(&mut p).title, "Inbox");
}
