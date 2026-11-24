//! Each surface the painted presenter draws for a node is in that node's
//! scheme (LLP 1034 §8): its own `color-scheme`, else its nearest
//! ancestor's, else the session's. A menu, an alert, a sheet and a date
//! picker under a dark root are dark on a light device (expo/exact#1469).
use super::stage::tests::{after, id, tap};
use super::top::look::Role;
use super::top::{SurfaceKind, SurfacePainter};
use super::*;
use std::cell::RefCell;
use std::rc::Rc;

/// `{root}` goes on the root and `{own}` on the menu, the alert and the
/// sheet's route.
const APP: &str = r##"routes nav
  home "/"
    sheet "/sheet"
component App
  state day = "2026-10-16"
  action setDay(value)
    day = value
  action open
    nav = push(nav, "/sheet")
  action ask
    showModal("confirm")
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%" background-color="light-dark(#ffffff, #000000)" testId="root" {root}
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0
            button popovertarget="menu" testId="more" width=40 height=40
              text "More"
            button press=ask testId="ask" height=30
              text "Ask"
            button press=open testId="open" height=30
              text "Open"
            input type="date" value=day change=setDay accent-color="light-dark(#0000ff, #ff8800)" testId="day" height=30
        if e.name == "sheet"
          column navigationKey=`${e.id}` navigationPresentation="modal" navigationDetent="medium" position="absolute" inset=0 {own}
            text "In the sheet"
            row width=40 height=40 background-color="light-dark(#ffffff, #202020)" testId="swatch"
      column id="menu" popover="auto" role="menu" aria-label="Board" {own}
        button role="menuitem" press=open testId="dice"
          text "Dice"
      dialog id="confirm" role="alertdialog" aria-label="Delete this?" {own}
        text "It cannot be undone."
        button commandfor="confirm" command="close"
          text "OK"
"##;

/// Where the scheme is written, the device's scheme, and the scheme each
/// surface should be in.
const CASES: [(&str, &str, bool, bool); 6] = [
    ("color-scheme=\"dark\"", "", false, true),
    ("", "color-scheme=\"dark\"", false, true),
    ("", "", false, false),
    ("", "", true, true),
    ("color-scheme=\"light\"", "", true, false),
    (
        "color-scheme=\"dark\"",
        "color-scheme=\"light\"",
        false,
        false,
    ),
];

/// The shell's painter: it keeps each surface's pixels.
struct Pixels(Rc<RefCell<Vec<Pixmap>>>);
impl SurfacePainter for Pixels {
    fn record(
        &mut self,
        size: (f32, f32),
        scale: f32,
        paint: &mut dyn FnMut(&mut dyn Backend),
    ) -> Vec<u32> {
        let mut raster = crate::raster::Raster::transparent();
        raster.begin(size.0, size.1, scale);
        paint(&mut raster);
        let pixels = raster.finish().unwrap();
        let words = vec![pixels.width(), pixels.height()];
        self.0.borrow_mut().push(pixels);
        words
    }
}

/// A phone's shell, its device in `device_dark`, showing the app with its
/// scheme written where the case says.
fn phone(root: &str, own: &str, device_dark: bool) -> (Presenter<()>, Rc<RefCell<Vec<Pixmap>>>) {
    let source = APP.replace("{root}", root).replace("{own}", own);
    let (mut p, error) = Presenter::boot_with_fixture_fonts(
        &crate::test_plan(&source),
        (),
        (412., 916.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let pictures = Rc::new(RefCell::new(Vec::new()));
    p.set_system_scheme(device_dark);
    p.serve_requests();
    p.serve_windows(Box::new(Pixels(pictures.clone())));
    let bars = Rc::new(RefCell::new(Vec::new()));
    assert_eq!(p.serve_bars(Box::new(Pixels(bars))), None);
    assert_eq!(p.set_window(super::stage::tests::WINDOW), None);
    p.frame();
    (p, pictures)
}

fn rgb(c: tiny_skia::PremultipliedColorU8) -> [u8; 3] {
    [c.red(), c.green(), c.blue()]
}

fn role(role: Role, dark: bool) -> [u8; 3] {
    let [r, g, b, _] = role.color(dark);
    [r, g, b]
}

/// The container colour of the surface painted last, inside its top
/// padding.
fn container(p: &mut Presenter<()>, pictures: &Rc<RefCell<Vec<Pixmap>>>) -> [u8; 3] {
    p.tell_shell();
    let all = pictures.borrow();
    let last = all.last().expect("a surface was painted");
    rgb(last.pixel(last.width() / 2, 3).unwrap())
}

#[test]
fn a_menu_is_in_its_nodes_scheme() {
    for (root, own, device, dark) in CASES {
        let (mut p, pictures) = phone(root, own, device);
        tap(&mut p, "more");
        assert_eq!(p.surface().map(|s| s.kind), Some(SurfaceKind::Menu));
        let want = role(Role::SurfaceContainer, dark);
        assert_eq!(
            container(&mut p, &pictures),
            want,
            "{root} {own} device dark {device}"
        );
    }
}

#[test]
fn an_alert_is_in_its_nodes_scheme() {
    for (root, own, device, dark) in CASES {
        let (mut p, pictures) = phone(root, own, device);
        tap(&mut p, "ask");
        assert_eq!(p.surface().map(|s| s.kind), Some(SurfaceKind::Alert));
        let want = role(Role::SurfaceContainerHigh, dark);
        assert_eq!(
            container(&mut p, &pictures),
            want,
            "{root} {own} device dark {device}"
        );
    }
}

#[test]
fn a_sheet_is_in_its_routes_scheme_and_keeps_it_as_it_closes() {
    for (root, own, device, dark) in CASES {
        let (mut p, _) = phone(root, own, device);
        tap(&mut p, "open");
        after(&mut p, 1000.0);
        let (_, sheet) = p.top_sheet().expect("the sheet is open");
        let at = (200, (sheet.1 + sheet.3 - 40.0) as u32);
        let want = role(Role::SurfaceContainerLow, dark);
        let shown = rgb(p.frame().pixel(at.0, at.1).unwrap());
        assert_eq!(shown, want, "{root} {own} device dark {device}");
        // What the route paints is in that scheme too.
        let swatch = p.rect_of(id(&p, "swatch")).expect("painted");
        let (x, y) = ((swatch.0 + 20.0) as u32, (swatch.1 + 20.0) as u32);
        let ink = if dark { [0x20; 3] } else { [0xff; 3] };
        let shown = rgb(p.frame().pixel(x, y).unwrap());
        assert_eq!(shown, ink, "swatch: {root} {own} device dark {device}");
        // Halfway out, its route gone, it is still in that scheme.
        assert!(p.system_back(p.host().now()));
        p.run_commands(|| ());
        after(&mut p, 60.0);
        let shown = rgb(p.frame().pixel(at.0, at.1).unwrap());
        assert_eq!(shown, want, "closing: {root} {own} device dark {device}");
    }
}

#[test]
fn a_date_picker_is_asked_for_in_its_controls_scheme() {
    // The last word of the request: 0 follows the device, 1 light, 2 dark.
    for (root, device, word) in [
        ("color-scheme=\"dark\"", false, 2),
        ("color-scheme=\"light\"", true, 1),
        ("", true, 0),
        ("", false, 0),
    ] {
        let (mut p, _) = phone(root, "", device);
        tap(&mut p, "day");
        let words = p.take_requests().expect("a picker is asked for");
        assert_eq!(words[0], 9, "a pick");
        assert_eq!(words.last(), Some(&word), "{root} device dark {device}");
        // Its accent is resolved in the control's scheme too.
        let dark = word == 2 || (word == 0 && device);
        let accent = if dark { 0xffff_8800_u32 } else { 0xff00_00ff };
        assert_eq!(
            words[words.len() - 2] as u32,
            accent,
            "{root} device dark {device}"
        );
    }
    // `setScheme` is the app's word over the device's.
    let (mut p, _) = phone("", "", false);
    p.app_scheme(Some(true));
    tap(&mut p, "day");
    assert_eq!(p.take_requests().unwrap().last(), Some(&2));
}

#[test]
fn the_windows_background_is_the_roots_in_the_roots_scheme() {
    for (root, device, want) in [
        ("color-scheme=\"dark\"", false, [0, 0, 0, 255]),
        ("color-scheme=\"light\"", true, [255; 4]),
        ("", true, [0, 0, 0, 255]),
    ] {
        let (p, _) = phone(root, "", device);
        assert_eq!(p.chrome().background, want, "{root} device dark {device}");
    }
}

/// Windows draws an alert as its own ContentDialog over the page, its
/// command area `#202020` dark and `#f3f3f3` light, in the alert's scheme.
#[test]
fn a_windows_alert_is_in_its_ancestors_scheme() {
    const DIALOG: &str = r##"component App
  action ask
    showModal("confirm")
  view
    main width="100%" height="100%" {root}
      button press=ask testId="ask"
        text "Ask"
      dialog id="confirm" role="alertdialog" aria-label="Delete this?"
        text "It cannot be undone."
        button commandfor="confirm" command="close"
          text "OK"
"##;
    for (root, device, ink) in [
        ("color-scheme=\"dark\"", false, [0x20; 3]),
        ("color-scheme=\"light\"", true, [0xf3; 3]),
        ("", true, [0x20; 3]),
        ("", false, [0xf3; 3]),
    ] {
        let (mut p, error) = Presenter::boot_with_fixture_fonts(
            &crate::test_plan(&DIALOG.replace("{root}", root)),
            (),
            (400., 800.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.set_system_scheme(device);
        p.serve_overlays();
        tap(&mut p, "ask");
        // The command area's left, clear of its button.
        let shown = rgb(p.frame().pixel(70, 440).unwrap());
        assert_eq!(shown, ink, "{root} device dark {device}");
    }
}
