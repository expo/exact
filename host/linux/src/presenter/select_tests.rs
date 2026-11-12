//! A select's painted menu and its keys (`select.rs`), as the WAI-ARIA
//! Authoring Practices' select-only combobox has them.
use super::*;
use std::path::PathBuf;

const PICK: &str = r#"component App
  state pick = "a"
  state log = ""
  action choose(v: string)
    pick = v
    log = `${log}${v} `
  view
    column
      select value=pick change=choose testId="pick"
        option "Alpha" value="a"
        option "Beta" value="b"
        option "Gamma" value="c"
      button testId="after"
        text "after"
"#;

fn boot(source: &str, viewport: (f32, f32)) -> Presenter<()> {
    let (p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        viewport,
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p
}

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn slot(p: &Presenter<()>, name: &str) -> String {
    let value = p.host.runner().slot(name).unwrap();
    value.as_str().unwrap().to_owned()
}

/// The form with its select focused.
fn focused(source: &str, viewport: (f32, f32)) -> (Presenter<()>, ViewId) {
    let mut p = boot(source, viewport);
    let id = named(&p, "pick");
    assert!(p.set_focus(Some(id), p.host.now()).is_none());
    (p, id)
}

fn key(p: &mut Presenter<()>, name: &str) {
    p.key_down(name, p.host.now());
}

/// The row the open menu's keys are at.
fn row(p: &mut Presenter<()>) -> Option<usize> {
    p.boxes();
    p.menu_paint().and_then(|m| m.chosen)
}

#[test]
fn a_focused_select_opens_on_its_keys_and_a_letter_chooses() {
    for name in [" ", "Enter", "ArrowDown", "ArrowUp"] {
        let (mut p, id) = focused(PICK, (300., 400.));
        key(&mut p, name);
        assert_eq!(p.menu, Some(id), "{name:?} opens the menu");
        assert_eq!(row(&mut p), Some(0), "{name:?}: at the chosen option");
        assert_eq!(slot(&p, "log"), "", "{name:?} chooses nothing");
    }
    let (mut p, _) = focused(PICK, (300., 400.));
    key(&mut p, "g");
    assert_eq!((p.menu, slot(&p, "pick")), (None, "c".into()), "Gamma");
    key(&mut p, "B");
    assert_eq!(slot(&p, "log"), "c b ", "a letter by its label, any case");
    let (mut p, _) = focused(PICK, (300., 400.));
    key(&mut p, "End");
    assert_eq!(row(&mut p), Some(2), "End opens at the last option");
}

#[test]
fn an_open_menu_takes_the_arrows_enter_space_and_letters() {
    let (mut p, id) = focused(PICK, (300., 400.));
    key(&mut p, "Enter");
    key(&mut p, "ArrowDown");
    assert_eq!(row(&mut p), Some(1));
    assert_eq!(slot(&p, "log"), "", "an arrow moves, and chooses nothing");
    key(&mut p, "Enter");
    assert_eq!((p.menu, slot(&p, "pick")), (None, "b".into()));
    key(&mut p, "ArrowDown");
    key(&mut p, "g");
    assert_eq!(row(&mut p), Some(2), "a letter moves to its option");
    key(&mut p, "Home");
    assert_eq!(row(&mut p), Some(0));
    key(&mut p, "End");
    key(&mut p, " ");
    assert_eq!((p.menu, slot(&p, "pick")), (None, "c".into()));
    key(&mut p, "ArrowDown");
    assert_eq!(p.menu, Some(id));
    key(&mut p, "ArrowUp");
    key(&mut p, "Escape");
    assert_eq!(p.menu, None, "Escape closes it");
    assert_eq!(slot(&p, "log"), "b c ", "and chooses nothing");
}

#[test]
fn tab_and_back_close_an_open_menu() {
    let (mut p, id) = focused(PICK, (300., 400.));
    key(&mut p, " ");
    key(&mut p, "Tab");
    assert_eq!(p.menu, None, "Tab closes the menu");
    assert!(p.set_focus(Some(id), p.host.now()).is_none());
    key(&mut p, " ");
    assert!(p.back_goes(), "an open menu takes Back");
    assert!(p.system_back(p.host.now()));
    assert_eq!(p.menu, None);
    assert!(!p.back_goes(), "then Back leaves the app");
    assert_eq!(slot(&p, "log"), "");
}

/// Sixty options, after `above` points of the page.
fn long(above: u32) -> String {
    let mut source = String::from(
        "component App\n  state pick = \"v0\"\n  action choose(v: string)\n    pick = v\n  view\n    column\n",
    );
    source.push_str(&format!("      box height={above}\n"));
    source.push_str("      select value=pick change=choose testId=\"pick\"\n");
    for i in 0..60 {
        source.push_str(&format!("        option \"Option {i}\" value=\"v{i}\"\n"));
    }
    source.push_str("      box height=400\n");
    source
}

#[test]
fn a_menu_longer_than_the_window_is_held_to_it_and_scrolls() {
    use crate::paint::control::MENU_PAD;
    for above in [0, 360] {
        let (mut p, _) = focused(&long(above), (300., 400.));
        key(&mut p, "ArrowDown");
        p.boxes();
        let menu = p.menu_paint().unwrap();
        let (x, y, w, h) = menu.rect;
        assert!(
            y >= 0. && y + h <= 400.,
            "{above}: {:?} in the window",
            menu.rect
        );
        key(&mut p, "End");
        key(&mut p, "Enter");
        assert_eq!(slot(&p, "pick"), "v59", "{above}: the keys reach the last");
        key(&mut p, "ArrowDown");
        key(&mut p, "Home");
        key(&mut p, "PageDown");
        let page = row(&mut p).unwrap();
        assert!(
            page > 5 && page < 20,
            "{above}: PageDown moves a page, to {page}"
        );
        key(&mut p, "Escape");
        // A wheel scrolls it to its end, where a press takes the last row.
        key(&mut p, "ArrowDown");
        p.wheel_at(x + w / 2., y + h / 2., 0., -100_000.);
        p.wheel_at(x + w / 2., y + h / 2., 0., 100_000.);
        let at = (x + w / 2., y + h - MENU_PAD - menu.row / 2.);
        let now = p.host.now();
        p.pointer_down(at.0, at.1, now).unwrap();
        p.pointer_up(at.0, at.1, now).unwrap();
        assert_eq!(p.menu, None);
        assert_eq!(
            slot(&p, "pick"),
            "v59",
            "{above}: a press on the scrolled row"
        );
        key(&mut p, "Home");
        p.wheel_at(x + w / 2., y + h / 2., 0., -100_000.);
        let at = (x + w / 2., y + MENU_PAD + menu.row / 2.);
        p.pointer_down(at.0, at.1, now).unwrap();
        p.pointer_up(at.0, at.1, now).unwrap();
        assert_eq!(
            slot(&p, "pick"),
            "v0",
            "{above}: and the first, scrolled back"
        );
    }
}

#[test]
fn a_select_menu_is_painted_chosen_and_dismissed_on_both_frame_paths() {
    use crate::paint::control::MENU_PAD;
    let mut failures = Vec::new();
    for display in [false, true] {
        for appearance in ["auto", "none"] {
            let source = PICK.replace(
                "select value",
                &format!("select appearance=\"{appearance}\" value"),
            );
            let mut p = boot(&source, (300., 400.));
            let paint = |p: &mut Presenter<()>| {
                if display {
                    let frame = p.display_frame().unwrap();
                    let pixels = frame.pixels.clone();
                    assert!(p.display_complete(&frame));
                    pixels
                } else {
                    p.frame()
                }
            };
            paint(&mut p);
            let id = named(&p, "pick");
            assert!(p.set_focus(Some(id), p.host.now()).is_none());
            let closed = paint(&mut p);
            key(&mut p, "Enter");
            let menu = p.menu_paint().unwrap();
            let opened = paint(&mut p);
            let (mx, my, mw, _) = menu.rect;
            for row in 0..3 {
                let y = my + MENU_PAD + row as f32 * menu.row;
                let changed = (y.ceil() as u32..(y + menu.row).floor() as u32).any(|y| {
                    (mx.ceil() as u32..(mx + mw).floor() as u32)
                        .any(|x| opened.pixel(x, y) != closed.pixel(x, y))
                });
                if !changed {
                    failures.push(format!(
                        "display={display} {appearance}: row {row} invisible"
                    ));
                }
            }
            let at = (mx + mw / 2., my + MENU_PAD + 1.5 * menu.row);
            let now = p.host.now();
            assert_eq!(p.press_at(at.0, at.1, now), Some(id));
            assert_eq!(slot(&p, "pick"), "b", "display={display} {appearance}");
            assert_eq!(p.menu, None);
            key(&mut p, "ArrowRight");
            let chosen = paint(&mut p);
            assert!(chosen.data() != closed.data(), "the selected label changes");
            key(&mut p, "Enter");
            paint(&mut p);
            key(&mut p, "Escape");
            let dismissed = paint(&mut p);
            assert_eq!(slot(&p, "pick"), "b", "display={display} {appearance}");
            assert!(
                dismissed.data() == chosen.data(),
                "dismissal leaves no menu pixels"
            );
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("; "));
}
