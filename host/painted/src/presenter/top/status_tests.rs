//! Status items where the host places them (`top/status.rs`, LLP 1133.025
//! D4): the button leaves the page, its menu opens in the host's window and
//! chooses there, Exit ends it when the host asks (D7), and
//! its image is drawn for the host's icon.
use super::*;
use std::path::PathBuf;

const TRAY: &str = r#"component App
  state log = ""
  state on = false
  action open
    log = `${log}open `
  action toggle
    on = not on
  action ping
    log = `${log}ping `
  view
    column
      button id="tray" -exact-menu="status" aria-label="Snap" popovertarget="menu"
        image "symbol:star"
      button testId="plain" -exact-menu="status" press=ping
        text "Ping"
      column id="menu" popover="auto" role="menu" aria-label="Snap"
        button testId="open" press=open
          text "Open Snap"
        button testId="toggle" role="menuitemcheckbox" aria-checked=on press=toggle
          text "Start at Login"
        hr
        button testId="off" disabled=true press=open
          text "Unavailable"
      text testId="after" "after"
"#;

fn boot(source: &str) -> Presenter<()> {
    boot_at(source, PathBuf::new())
}

fn boot_at(source: &str, assets: PathBuf) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (300., 400.),
        1.,
        assets,
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.serve_overlays();
    p.frame();
    p
}

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    let key = k.find_by_test_id(name).first().copied().or_else(|| {
        let found = k.preorder_where(&k.roots(), |_, props| props.str(PropId::Id) == Some(name));
        found.first().and_then(|id| k.node(*id)).map(|n| n.key)
    });
    k.node_by_key(key.unwrap()).unwrap().id
}

fn slot(p: &Presenter<()>, name: &str) -> exact_plan::Value {
    p.host.runner().slot(name).unwrap().clone()
}

fn titles(p: &Presenter<()>) -> Vec<String> {
    let shown = p.surface().expect("a surface is open");
    shown.plan.rows().map(|row| row.title.clone()).collect()
}

/// Pointer down and up at row `index`'s middle, in the menu's own points.
fn click_row(p: &mut Presenter<()>, index: usize) {
    let (x, y, w, h) = p.surface().unwrap().targets[index].rect;
    let now = p.host.now();
    p.status_menu_pointer(0, x + w / 2., y + h / 2., now);
    p.status_menu_pointer(1, x + w / 2., y + h / 2., now);
}

#[test]
fn each_status_button_is_an_item_with_its_label_image_press_and_menu() {
    let p = boot(TRAY);
    let items = p.status_items();
    let tray = named(&p, "tray");
    let image = p.host.kernel().node(tray).unwrap().children()[0];
    assert_eq!(
        items,
        [
            StatusItem {
                node: tray,
                name: Some("tray".into()),
                label: "Snap".into(),
                disabled: false,
                press: false,
                menu: true,
                image: Some(image),
            },
            StatusItem {
                node: named(&p, "plain"),
                name: Some("plain".into()),
                label: "Ping".into(),
                disabled: false,
                press: true,
                menu: false,
                image: None,
            },
        ]
    );
    // With neither `id` nor `testId`, no name keeps its place.
    let unnamed = boot(&TRAY.replace(r#"button testId="plain" "#, "button "));
    assert_eq!(unnamed.status_items()[1].name, None);
}

#[test]
fn a_placed_status_button_takes_no_room_in_the_page() {
    let mut p = boot(TRAY);
    let after = named(&p, "after");
    let before = p.box_of(after).unwrap().rect.1;
    p.place_status_items();
    p.frame();
    let placed = p.box_of(after).unwrap().rect.1;
    assert!(placed < before, "{placed} under {before}");
    assert!(p.box_of(named(&p, "tray")).is_none_or(|b| b.rect.3 == 0.));
    assert_eq!(p.status_items().len(), 2, "still the host's to place");
}

#[test]
fn a_status_menu_opens_in_its_own_window_and_a_row_chosen_there_presses() {
    let mut p = boot(TRAY);
    p.place_status_items();
    p.frame();
    let tray = named(&p, "tray");
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), false, false, now));
    let menu = p.status_menu().expect("open in the host's window");
    assert_eq!(titles(&p), ["Open Snap", "Start at Login", "Unavailable"]);
    let pixels = p.status_menu_pixels(1.5).unwrap().unwrap();
    assert_eq!(
        (pixels.width(), pixels.height()),
        (
            (menu.size.0 * 1.5).round() as u32,
            (menu.size.1 * 1.5).round() as u32
        )
    );
    click_row(&mut p, 0);
    assert!(p.status_menu().is_none(), "a choice closes it");
    assert_eq!(slot(&p, "log"), exact_plan::Value::from("open "));

    // A row's state is read again while the menu is open.
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), false, false, now));
    let serial = p.status_menu().unwrap().serial;
    click_row(&mut p, 1);
    assert_eq!(slot(&p, "on"), exact_plan::Value::Bool(true));
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), false, false, now));
    assert!(p.status_menu().unwrap().serial > serial);
    assert!(p.surface().unwrap().plan.rows().nth(1).unwrap().checked);

    // The page's window is outside it: a press there dismisses it, and
    // presses nothing of the page.
    let now = p.host.now();
    assert!(p.overlay_pointer(0, 10., 10., now));
    assert!(p.status_menu().is_none());
}

#[test]
fn a_status_menu_opened_from_the_keyboard_focuses_its_first_row() {
    let mut p = boot(TRAY);
    let tray = named(&p, "tray");
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), false, true, now));
    assert_eq!(p.top.open.as_ref().unwrap().focused, Some(0));
    p.status_menu_key("Enter", now);
    assert!(p.status_menu().is_none());
    assert_eq!(slot(&p, "log"), exact_plan::Value::from("open "));
    assert!(p.open_status_menu(tray, (1920., 1040.), false, true, now));
    p.status_menu_key("Escape", now);
    assert!(p.status_menu().is_none());
    assert!(p.open_status_menu(tray, (1920., 1040.), false, false, now));
    p.dismiss_status_menu(now);
    assert!(p.status_menu().is_none());
    assert_eq!(slot(&p, "log"), exact_plan::Value::from("open "));
}

#[test]
fn a_status_menu_ends_with_exit_when_its_host_asks() {
    let mut p = boot(TRAY);
    let tray = named(&p, "tray");
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), true, false, now));
    assert_eq!(
        titles(&p),
        ["Open Snap", "Start at Login", "Unavailable", "Exit"]
    );
    assert!(!p.take_status_exit());
    click_row(&mut p, 3);
    assert!(p.status_menu().is_none());
    assert!(p.take_status_exit(), "the host's to end");
    assert!(!p.take_status_exit());
    assert_eq!(slot(&p, "log"), exact_plan::Value::from(""));

    let removed = TRAY.replace(
        "    column\n",
        "    column -exact-menu-remove=\"status-quit\"\n",
    );
    let mut p = boot(&removed);
    let tray = named(&p, "tray");
    let now = p.host.now();
    assert!(p.open_status_menu(tray, (1920., 1040.), true, false, now));
    assert_eq!(titles(&p), ["Open Snap", "Start at Login", "Unavailable"]);
}

#[test]
fn a_status_buttons_press_runs_without_a_menu() {
    let mut p = boot(TRAY);
    let plain = named(&p, "plain");
    let now = p.host.now();
    assert!(!p.open_status_menu(plain, (1920., 1040.), true, false, now));
    p.status_press(plain, now);
    assert_eq!(slot(&p, "log"), exact_plan::Value::from("ping "));
}

#[test]
fn a_status_menu_exposes_unchecked_items_and_keyboard_focus() {
    use crate::presenter::access::flag;
    let mut p = boot(TRAY);
    let tray = named(&p, "tray");
    assert!(p.open_status_menu(tray, (800., 600.), true, true, 0.));
    p.reader_access(1., true);
    let toggle = p
        .access_told()
        .iter()
        .find(|n| n.name == "Start at Login")
        .unwrap();
    assert_ne!(toggle.flags & flag::CHECKABLE, 0);
    assert_eq!(toggle.flags & flag::CHECKED, 0);
    let toggle_id = toggle.id;
    let first = p
        .access_told()
        .iter()
        .find(|n| n.name == "Open Snap")
        .unwrap()
        .id;
    assert_eq!(p.access_focus(), Some(first));
    p.status_menu_key("s", 0.5);
    p.reader_access(1., true);
    assert_eq!(p.access_focus(), Some(toggle_id));
    p.status_menu_key("ArrowUp", 0.75);
    p.status_menu_key("ArrowDown", 1.);
    p.reader_access(1., true);
    let toggle = p
        .access_told()
        .iter()
        .find(|n| n.name == "Start at Login")
        .unwrap()
        .id;
    assert_eq!(p.access_focus(), Some(toggle));
    assert!(p.access_act(toggle, crate::presenter::access::act::CLICK, 0., 2.));
    assert!(p.status_menu().is_none());
    assert_eq!(slot(&p, "on"), exact_plan::Value::Bool(true));
}

#[test]
fn a_status_img_uses_its_decoded_pixels_at_the_requested_icon_size() {
    let mut p = boot(&TRAY.replace("symbol:star", "status.png"));
    let image = p.status_items()[0].image.unwrap();
    let mut pixels = Pixmap::new(8, 4).unwrap();
    pixels.fill(tiny_skia::Color::from_rgba8(220, 40, 20, 255));
    let gate = exact_raster::Gate::new();
    let charge = gate
        .session()
        .reserve_allocation(8 * 4 * 4)
        .unwrap()
        .charge();
    p.images.bitmaps.insert(
        image,
        std::sync::Arc::new(crate::image::Bitmap::new(pixels, (8, 4), charge)),
    );
    for (side, scale) in [(16, 1.), (20, 1.25)] {
        let icon = p
            .status_icon(image, side, scale, false)
            .expect("img supplies the icon")
            .unwrap();
        assert_eq!((icon.width(), icon.height()), (side, side));
        assert!(icon.pixel(side / 2, side / 2).unwrap().red() > 200);
        assert_eq!(icon.pixel(0, 0).unwrap().alpha(), 0, "aspect ratio is kept");
    }
}

#[test]
fn a_status_items_symbol_is_drawn_in_the_taskbars_text_colour() {
    let svg = r#"component App
  view
    column
      button id="drawn" -exact-menu="status" aria-label="Drawn"
        svg viewBox="0 0 10 10" width=10 height=10
          rect x=0 y=0 width=10 height=10 fill="currentColor"
"#;
    for (source, name) in [(TRAY, "tray"), (svg, "drawn")] {
        let mut p = boot(source);
        let image = p.status_items()[0].image.unwrap();
        assert_eq!(p.status_items()[0].node, named(&p, name));
        for dark in [true, false] {
            let pixmap = p.status_icon(image, 24, 1.5, dark).unwrap().unwrap();
            assert_eq!((pixmap.width(), pixmap.height()), (24, 24), "{name}");
            let inked: Vec<_> = (pixmap.pixels().iter())
                .filter(|px| px.alpha() > 200)
                .map(|px| px.demultiply().red())
                .collect();
            assert!(!inked.is_empty(), "{name}: something is drawn");
            let light = inked.iter().filter(|r| **r > 128).count();
            assert_eq!(
                light > inked.len() / 2,
                dark,
                "{name}: light ink on a dark taskbar, dark on a light one"
            );
        }
    }
}

#[test]
fn a_placed_status_img_loads_without_a_box_and_reloads_at_the_new_dpi() {
    let assets = std::env::current_dir()
        .unwrap()
        .join("target")
        .join(format!("exact-status-img-{}", std::process::id()));
    std::fs::create_dir_all(&assets).unwrap();
    let mut png = Pixmap::new(64, 32).unwrap();
    png.fill(tiny_skia::Color::from_rgba8(20, 220, 40, 255));
    png.save_png(assets.join("status.png")).unwrap();
    let mut p = boot_at(&TRAY.replace("symbol:star", "status.png"), assets.clone());
    p.place_status_items();
    p.frame();
    let image = p.status_items()[0].image.unwrap();
    for (side, scale) in [(20, 1.25), (16, 1.)] {
        p.sync_status_images(&[(image, side)]);
        p.wait_images(std::time::Duration::from_secs(10));
        let icon = p.status_icon(image, side, scale, false).unwrap().unwrap();
        assert_eq!((icon.width(), icon.height()), (side, side));
        assert!(icon.pixel(side / 2, side / 2).unwrap().green() > 200);
    }
    std::fs::remove_dir_all(assets).unwrap();
}

#[test]
fn status_submenus_expose_expansion_and_return_to_their_parent() {
    use crate::presenter::access::{act, flag};
    let source = TRAY.replace("        hr", "        button testId=\"more\" popovertarget=\"more-menu\"\n          text \"More\"\n        hr")
        .replace("      text testId=\"after\" \"after\"", "      column id=\"more-menu\" popover=\"auto\" role=\"menu\" aria-label=\"More\"\n        button press=ping\n          text \"Ping\"");
    let mut p = boot(&source);
    let tray = named(&p, "tray");
    for (forward, back) in [("ArrowRight", "ArrowLeft"), ("Enter", "Escape")] {
        assert!(p.open_status_menu(tray, (800., 600.), true, true, 0.));
        p.reader_access(1., true);
        let more = p.access_told().iter().find(|n| n.name == "More").unwrap();
        assert_ne!(more.flags & flag::EXPANDABLE, 0);
        let more_id = more.id;
        assert!(p.access_focus_node(more_id, 0.));
        p.status_menu_key(forward, 1.);
        assert_eq!(titles(&p), ["Ping"]);
        p.reader_access(1., true);
        let expanded = p.access_told().iter().find(|n| n.id == more_id);
        assert!(expanded.is_some(), "the expanded item's provider survives");
        assert_ne!(expanded.unwrap().flags & flag::EXPANDED, 0);
        let ping = p.access_told().iter().find(|n| n.name == "Ping").unwrap();
        assert_eq!(ping.parent, more_id);
        assert_eq!(
            p.access_focus(),
            Some(ping.id),
            "{forward} focuses the submenu"
        );
        p.status_menu_key(back, 2.);
        p.reader_access(1., true);
        assert_eq!(p.access_focus(), Some(more_id));
        assert!(p.access_told().iter().any(|n| n.name == "Exit"));
        let exit = p
            .access_told()
            .iter()
            .find(|n| n.name == "Exit")
            .unwrap()
            .id;
        assert!(p.access_act(exit, act::CLICK, 0., 3.));
        assert!(p.take_status_exit());
    }
    assert!(p.open_status_menu(tray, (800., 600.), true, true, 4.));
    p.status_menu_key("m", 5.);
    p.status_menu_key("Enter", 6.);
    p.status_menu_key("Enter", 7.);
    assert!(p.status_menu().is_none());
    assert_eq!(slot(&p, "log"), exact_plan::Value::from("ping "));
}
