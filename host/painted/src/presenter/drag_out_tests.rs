//! A long press on a node with `draggable` asks Android's reader for the
//! platform's drag (LLP 1094.000 D7, D8): the words it reads, which node a
//! long press drags, and a menu on the same node keeping the press. And
//! `share`'s files, which land with the drag's: the reader's sheet is given
//! `app:/` files, and still no element's picture.
use super::*;
use crate::presenter::top::SurfacePainter;
use exact_runner::{DataError, Value};

const APP: &str = r##"component App
  state files = ["assets/a.png", "app:/data/shared/fuji.png"]
  state menus = 0
  state blank = ""
  state level = 0.5
  state on = false
  action opened
    menus = menus + 1
  action turn(value)
    on = value
  action send
    share(text="Mount Fuji", files=["app:/data/shared/fuji.png"])
  action picture
    share(files=["#card"])
  view
    column
      button press=send testId="send"
        text "Send"
      button press=picture testId="picture"
        text "Picture"
      button id="card" testId="card" draggable=true dataTransfer=DataTransfer(text="Mount Fuji", url="https://example.com/1016", files=files) width=200 height=80
        text "Card"
      box testId="both" draggable=true dataTransfer=DataTransfer(text="b") contextmenu=opened width=200 height=80
        text "Both"
      box testId="around" contextmenu=opened width=200 height=120
        box testId="inner" draggable=true dataTransfer=DataTransfer(text="i") width=100 height=60
          text "Inner"
      box testId="form" draggable=true dataTransfer=DataTransfer(text="f") width=200 height=80
        input testId="field" width=150
      box testId="plain" draggable=false width=200 height=80
        text "Plain"
      box testId="held" draggable=true dataTransfer=DataTransfer(text="h") contextPopover="menu" width=200 height=60
        text "Held"
      text `${menus}` testId="menus"
      box testId="empty" draggable=true dataTransfer=DataTransfer(text=blank) contextPopover="menu" width=200 height=40
        text "Empty"
      button testId="off" draggable=true disabled=true dataTransfer=DataTransfer(text="o") width=200 height=40
        text "Off"
      button testId="row" press=opened contextmenu=opened width=200 height=60
        box testId="row-part" draggable=true dataTransfer=DataTransfer(text="r") width=200 height=60
      image "assets/a.png" testId="pic" draggable=true dataTransfer=DataTransfer(text="p") aria-label="Picture" width=40 height=40
      column testId="port" overflow="hidden" width=200 height=40
        box testId="tall" draggable=true dataTransfer=DataTransfer(text="t") width=200 height=100
          text "Tall"
      box testId="kids" draggable=true dataTransfer=DataTransfer(text="k") width=300 height=200
        button testId="kid-button" press=opened width=100 height=30
          text "Open"
        input type="checkbox" checked=on change=turn testId="kid-check"
        progress value=level testId="kid-progress" width=100
        input type="range" testId="kid-range" width=150
        input type="checkbox" role="switch" checked=on change=turn testId="kid-switch"
      column id="menu" popover="auto" role="menu"
        button press=opened popovertarget="menu" popovertargetaction="hide" testId="menu-copy"
          text "Copy"
"##;

/// A source that answers nothing and may read `app:/data/shared`.
#[derive(Default)]
struct Granted;
impl DataSource for Granted {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn grants(&self) -> &str {
        "fs.read app:/data/shared"
    }
}

/// A shell that makes windows and keeps nothing they draw.
struct Blank;
impl SurfacePainter for Blank {
    fn record(&mut self, _: (f32, f32), _: f32, _: &mut dyn FnMut(&mut dyn Backend)) -> Vec<u32> {
        Vec::new()
    }
}

fn boot() -> Presenter<Granted> {
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(APP),
        Granted,
        (400., 1600.),
        2.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_requests();
    p.serve_windows(Box::new(Blank));
    p.frame();
    p
}

fn id(p: &Presenter<Granted>, name: &str) -> ViewId {
    let kernel = p.host().kernel();
    kernel
        .node_by_key(kernel.find_by_test_id(name)[0])
        .unwrap()
        .id
}

/// A long press in `name`'s box, 10 points in from its corner, and the
/// bodies of the drag requests it queued.
fn long_press(p: &mut Presenter<Granted>, name: &str) -> (bool, Vec<Vec<i32>>) {
    let rect = p.box_of(id(p, name)).unwrap().rect;
    let took = p.long_press(rect.0 + 10., rect.1 + 10., 100.);
    p.run_commands(|| Granted);
    (took, taken(p, 23))
}

/// The bodies of the queued requests of `kind`.
fn taken(p: &mut Presenter<Granted>, kind: i32) -> Vec<Vec<i32>> {
    let words = p.take_requests().unwrap_or_default();
    let (mut bodies, mut at) = (Vec::new(), 0);
    while at < words.len() {
        let n = words[at + 2] as usize;
        if words[at] == kind {
            bodies.push(words[at + 3..at + 3 + n].to_vec());
        }
        at += 3 + n;
    }
    bodies
}

/// The string at `at` in a request's words, and the index after it.
fn string(words: &[i32], at: usize) -> (String, usize) {
    let n = words[at] as usize;
    let bytes: Vec<u8> = (0..n)
        .map(|k| (words[at + 1 + k / 4] >> (8 * (k % 4))) as u8)
        .collect();
    (String::from_utf8(bytes).unwrap(), at + 1 + n.div_ceil(4))
}

#[test]
fn a_long_press_on_a_draggable_node_asks_the_reader_for_its_drag() {
    let mut p = boot();
    let card = id(&p, "card");
    let rect = p.box_of(card).unwrap().rect;
    let (took, drags) = long_press(&mut p, "card");
    assert!(took);
    let [words] = drags.as_slice() else {
        panic!("one drag: {drags:?}")
    };
    assert_eq!(words[0], card as i32);
    let px = |v: f32| (v * 2.).round() as i32;
    assert_eq!(
        &words[1..5],
        &[px(rect.0), px(rect.1), px(rect.2), px(rect.3)],
        "its box in the view's pixels"
    );
    assert_eq!(
        &words[5..7],
        &[px(rect.0 + 10.), px(rect.1 + 10.)],
        "and the finger"
    );
    assert_eq!(words[7], 0, "a finger's, not a service's");
    let (text, next) = string(words, 8);
    let (url, next) = string(words, next);
    assert_eq!(
        (text.as_str(), url.as_str()),
        ("Mount Fuji", "https://example.com/1016")
    );
    assert_eq!(words[next], 2, "two files");
    let (shipped, next) = string(words, next + 1);
    let app = string(words, next).0;
    assert_eq!(
        [shipped.as_str(), app.as_str()],
        ["assets/a.png", "app:/data/shared/fuji.png"],
        "as the node names them"
    );
}

#[test]
fn a_menu_on_the_same_node_keeps_the_press_and_a_nearer_drag_takes_it() {
    let mut p = boot();
    let (took, drags) = long_press(&mut p, "both");
    assert!(
        took && drags.is_empty(),
        "the node's own menu, as iOS's held finger keeps it"
    );
    let (took, drags) = long_press(&mut p, "inner");
    assert!(took);
    assert_eq!(
        drags.len(),
        1,
        "a draggable node inside one with a menu drags"
    );
    assert_eq!(drags[0][0], id(&p, "inner") as i32);
}

#[test]
fn a_field_inside_keeps_its_contact_and_draggable_false_drags_nothing() {
    let mut p = boot();
    let field = p.box_of(id(&p, "field")).unwrap().rect;
    assert_eq!(
        p.drag_source_at(field.0 + 5., field.1 + 5.),
        None,
        "a text field keeps the contact (D8)"
    );
    let form = p.box_of(id(&p, "form")).unwrap().rect;
    assert_eq!(
        p.drag_source_at(form.0 + 190., form.1 + 70.),
        Some(id(&p, "form")),
        "the box around it drags"
    );
    let (took, drags) = long_press(&mut p, "plain");
    assert!(!took && drags.is_empty());
}

/// The reader's sheet takes `app:/` files once it says so
/// ([`Presenter::serve_share_sheet`]), and still no element's picture.
#[test]
fn the_share_sheet_is_given_app_files_where_it_says_so_and_no_picture() {
    let mut p = boot();
    let tap = |p: &mut Presenter<Granted>, name: &str| {
        p.tap(id(p, name)).unwrap();
        p.run_commands(|| Granted);
    };
    let says =
        |p: &Presenter<Granted>, line: &str| p.host().runner().journal().any(|l| l.contains(line));
    tap(&mut p, "send");
    assert!(taken(&mut p, 4).is_empty());
    assert!(says(
        &p,
        "share: refused: files are not carried to this host's share sheet"
    ));
    p.serve_share_sheet();
    let page = p.host().runner().page();
    assert_eq!(
        (
            page.can_share,
            page.can_share_files,
            page.can_share_pictures
        ),
        (true, true, false)
    );
    tap(&mut p, "send");
    let shares = taken(&mut p, 4);
    let [words] = shares.as_slice() else {
        panic!("one share: {shares:?}")
    };
    assert_eq!(words[0], 0b010, "the text alone of the three");
    let (mut at, mut strings) = (1, Vec::new());
    while at < words.len() {
        let (s, next) = string(words, at);
        strings.push(s);
        at = next;
    }
    assert_eq!(strings, ["", "Mount Fuji", "", "app:/data/shared/fuji.png"]);
    tap(&mut p, "picture");
    assert!(taken(&mut p, 4).is_empty());
    assert!(says(
        &p,
        "share: refused: a picture of an element is not carried to Android's share sheet yet"
    ));
}

/// A node a held press drags is one element with the drag action, a
/// button's and a box's alike, and a service's drag says it is one (D9).
#[test]
fn an_assistive_service_starts_a_nodes_drag_by_its_action() {
    use super::access::{act, flag, role};
    let mut p = boot();
    let (nodes, ..) = p.access_tree();
    let views: Vec<(&str, ViewId)> = ["card", "inner", "both", "plain", "form"]
        .into_iter()
        .map(|name| (name, id(&p, name)))
        .collect();
    let view = |name: &str| views.iter().find(|(n, _)| *n == name).unwrap().1;
    let of = |name: &str| nodes.iter().find(|n| n.view == view(name)).cloned();
    let card = of("card").expect("the card is an element");
    assert_eq!(
        card.flags & (flag::CLICK | flag::DRAG),
        flag::CLICK | flag::DRAG
    );
    let inner = of("inner").expect("a box that drags is an element");
    assert_eq!((inner.role, inner.name.as_str()), (role::GROUP, "Inner"));
    assert_eq!(inner.flags & (flag::CLICK | flag::DRAG), flag::DRAG);
    let both = of("both").unwrap();
    assert_eq!(
        both.flags & (flag::LONG_CLICK | flag::DRAG),
        flag::LONG_CLICK | flag::DRAG
    );
    assert!(of("plain").is_none(), "draggable=false adds no element");
    p.reader_access(1.0, false).expect("a statement");
    assert!(p.access_act(inner.id, act::DRAG, 0., 0.));
    p.run_commands(|| Granted);
    let drags = taken(&mut p, 23);
    let [words] = drags.as_slice() else {
        panic!("one drag: {drags:?}")
    };
    let rect = p.box_of(id(&p, "inner")).unwrap().rect;
    let middle = |at: f32, size: f32| ((at + size / 2.) * 2.).round() as i32;
    assert_eq!(
        &words[5..8],
        &[middle(rect.0, rect.2), middle(rect.1, rect.3), 1],
        "from its middle, a service's"
    );
    assert!(
        of("form").is_none(),
        "a box with no words of its own is no element"
    );
}

/// A node with a menu and a drag opens its menu on a held press; the
/// finger moving on past the slop closes the menu and lifts the drag, and
/// a lift ends the chance (LLP 1094.000 §7 item 6).
#[test]
fn a_held_finger_moving_on_from_a_menu_lifts_the_drag() {
    let mut p = boot();
    let held = id(&p, "held");
    let rect = p.box_of(held).unwrap().rect;
    let (x, y) = (rect.0 + 10., rect.1 + 10.);
    let before = p.long_presses();
    assert!(p.long_press(x, y, 100.));
    p.run_commands(|| Granted);
    let drags = taken(&mut p, 23);
    assert!(drags.is_empty() && p.surface().is_some(), "its menu");
    let slop = exact_motion::gesture::SLOP as f32;
    p.reader_touch(2, x + slop - 1., y, 120.).unwrap();
    assert!(
        taken(&mut p, 23).is_empty() && p.surface().is_some(),
        "within the slop"
    );
    p.reader_touch(2, x + slop + 1., y, 140.).unwrap();
    let drags = taken(&mut p, 23);
    assert_eq!(drags.len(), 1, "past it");
    assert_eq!(drags[0][0], held as i32);
    assert!(p.surface().is_none(), "the menu closed");
    assert_eq!(p.long_presses(), before.wrapping_add(1), "one long press");
    // A drag long press counts too, so a reader yields a hosted view's.
    long_press(&mut p, "inner");
    assert_eq!(p.long_presses(), before.wrapping_add(2));
    // Lifted, the finger lifts nothing more.
    assert!(p.long_press(x, y, 200.));
    p.reader_touch(1, x, y, 210.).unwrap();
    p.reader_touch(2, x + 40., y, 220.).unwrap();
    assert!(taken(&mut p, 23).is_empty());
}

/// Under the agent no drag lifts, as on the Mac: `tree` says what one
/// would carry (D10), and a held press on a node with a menu opens it.
#[test]
fn under_the_agent_a_held_press_lifts_no_drag() {
    let mut p = boot();
    p.agent = true;
    let (took, drags) = long_press(&mut p, "inner");
    assert!(
        took && drags.is_empty(),
        "the menu around it takes the press"
    );
    let (took, drags) = long_press(&mut p, "card");
    assert!(!took && drags.is_empty());
}

/// A rested finger on a button, a checkbox or a progress bar inside a
/// draggable node lifts the node; a slider, a switch, a text field and a
/// disabled node keep the contact (D8), as on iOS.
#[test]
fn a_press_or_a_picture_inside_lifts_the_node_and_a_control_that_follows_keeps_it() {
    let mut p = boot();
    let kids = id(&p, "kids");
    for name in ["kid-button", "kid-check", "kid-progress"] {
        let rect = p.box_of(id(&p, name)).unwrap().rect;
        assert_eq!(
            p.drag_source_at(rect.0 + 3., rect.1 + 3.),
            Some(kids),
            "{name}"
        );
    }
    for name in ["kid-range", "kid-switch", "off"] {
        let rect = p.box_of(id(&p, name)).unwrap().rect;
        assert_eq!(p.drag_source_at(rect.0 + 3., rect.1 + 3.), None, "{name}");
    }
    // A press the finger armed on the button presses nothing once the drag lifts.
    let rect = p.box_of(id(&p, "kid-button")).unwrap().rect;
    let (x, y) = (rect.0 + 3., rect.1 + 3.);
    p.reader_touch(0, x, y, 0.).unwrap();
    assert!(p.long_press(x, y, 500.));
    p.reader_touch(1, x, y, 600.).unwrap();
    p.run_commands(|| Granted);
    assert_eq!(taken(&mut p, 23).len(), 1);
    let menus = p.host().kernel().node(id(&p, "menus")).unwrap();
    assert_eq!(
        menus
            .text_runs()
            .iter()
            .map(|r| &*r.text)
            .collect::<String>(),
        "0"
    );
}

/// A node that carries nothing now keeps its menu when the finger moves
/// on, and nothing lifts; a down or a cancel ends a lift's chance.
#[test]
fn a_menu_stays_when_the_node_carries_nothing_and_a_new_contact_ends_the_lift() {
    let mut p = boot();
    let rect = p.box_of(id(&p, "empty")).unwrap().rect;
    let (x, y) = (rect.0 + 10., rect.1 + 10.);
    assert!(p.long_press(x, y, 100.));
    p.reader_touch(2, x + 40., y, 120.).unwrap();
    p.run_commands(|| Granted);
    assert!(
        taken(&mut p, 23).is_empty() && p.surface().is_some(),
        "the menu stays"
    );
    p.close_surface(130.);
    let rect = p.box_of(id(&p, "held")).unwrap().rect;
    let (x, y) = (rect.0 + 10., rect.1 + 10.);
    for end in [0, 3] {
        assert!(p.long_press(x, y, 200.));
        p.reader_touch(end, x, y, 210.).unwrap();
        p.reader_touch(2, x + 40., y, 220.).unwrap();
        p.run_commands(|| Granted);
        assert!(taken(&mut p, 23).is_empty(), "action {end}");
        p.close_surface(230.);
    }
}

/// An assistive service's long click opens the element's own menu and
/// lifts no drag, which is the element's other action; and the reader's
/// hit reply names a node that drags, so a hosted view yields its press.
#[test]
fn a_services_long_click_opens_the_menu_and_the_hit_reply_names_a_node_that_drags() {
    use super::access::act;
    let mut p = boot();
    let (nodes, ..) = p.access_tree();
    let row = nodes.iter().find(|n| n.view == id(&p, "row")).unwrap().id;
    p.reader_access(1.0, false).expect("a statement");
    assert!(p.access_act(row, act::LONG_CLICK, 0., 0.));
    p.run_commands(|| Granted);
    assert!(taken(&mut p, 23).is_empty());
    let menus = p.host().kernel().node(id(&p, "menus")).unwrap();
    assert_eq!(
        menus
            .text_runs()
            .iter()
            .map(|r| &*r.text)
            .collect::<String>(),
        "1"
    );
    let rect = p.box_of(id(&p, "card")).unwrap().rect;
    let words = p.reader_module((rect.0 + 10.) * 2., (rect.1 + 10.) * 2., 2.);
    assert_eq!(words[12], id(&p, "card") as i32);
}

/// The request's words, which the reader's `DragOut.read` reads back
/// (`DragOutTest.theRequestReadsAsThePresenterWritesIt`): keep the two in step.
#[test]
fn the_request_writes_the_words_the_reader_reads() {
    let drag = super::drag_out::DragOut {
        node: 7,
        rect: [10, 20, 30, 40],
        at: [15, 25],
        service: true,
        text: "Fuji".into(),
        url: "https://example.com/1".into(),
        files: vec!["assets/a.png".into(), "app:/data/b.pdf".into()],
    };
    let mut words = Vec::new();
    drag.encode(&mut words);
    assert_eq!(
        words,
        [
            7, 10, 20, 30, 40, 15, 25, 1, 4, 1768584518, 21, 1886680168, 791624307, 1835104357,
            778398832, 795701091, 49, 2, 12, 1702064993, 1630499700, 1735290926, 15, 980447329,
            1952539695, 778186593, 6710384
        ]
    );
}

/// A draggable image keeps its role with the drag action; a host that lifts
/// nothing (no shell) flags no drag and makes no element of a box that drags.
#[test]
fn an_image_keeps_its_role_and_a_host_that_lifts_nothing_flags_and_groups_nothing() {
    use super::access::{flag, role};
    let mut p = boot();
    let (nodes, ..) = p.access_tree();
    let pic = nodes
        .iter()
        .find(|n| n.view == id(&p, "pic"))
        .expect("the image");
    assert_eq!(
        (pic.role, pic.flags & flag::DRAG),
        (role::IMAGE, flag::DRAG)
    );
    let (mut bare, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(APP),
        Granted,
        (400., 1600.),
        2.,
        std::path::PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    bare.frame();
    let (nodes, ..) = bare.access_tree();
    assert!(nodes.iter().all(|n| n.flags & flag::DRAG == 0));
    let inner = id(&bare, "inner");
    assert!(
        nodes.iter().all(|n| n.view != inner),
        "a box that drags is no element there"
    );
}

/// A node its scroller cuts off drags as much of it as shows: the reader's
/// shadow is that part's picture.
#[test]
fn a_clipped_node_drags_as_much_of_it_as_shows() {
    let mut p = boot();
    let port = p.box_of(id(&p, "port")).unwrap().rect;
    let (took, drags) = long_press(&mut p, "tall");
    assert!(took);
    assert_eq!(
        drags[0][4],
        (port.3 * 2.).round() as i32,
        "its height within the port"
    );
}
