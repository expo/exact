//! A select's menu as the top layer's (`top/select.rs`): where a shell
//! presents the top layer, as Android's windows and Windows' overlays do,
//! a select opens a surface of its options, which chooses, dismisses, takes
//! the select-only combobox's keys and is a listbox to assistive technology.
use super::*;
use crate::presenter::access::{act, flag, role};
use std::path::PathBuf;

/// A select with a disabled option, wider than its menu would be, then a
/// button.
const PICK: &str = r#"component App
  state pick = "a"
  state log = ""
  action choose(v: string)
    pick = v
    log = `${log}${v} `
  view
    column
      select value=pick change=choose testId="pick" width=260
        option "Alpha" value="a"
        option "Beta" value="b" disabled=true
        option "Gamma" value="c"
      button testId="after"
        text "after"
"#;

/// The CPU painter standing in for a shell's: it paints each surface, and
/// says nothing of it.
struct Paints;
impl SurfacePainter for Paints {
    fn record(
        &mut self,
        size: (f32, f32),
        scale: f32,
        paint: &mut dyn FnMut(&mut dyn Backend),
    ) -> Vec<u32> {
        let mut raster = crate::raster::Raster::transparent();
        raster.begin(size.0, size.1, scale);
        paint(&mut raster);
        raster.finish().unwrap();
        Vec::new()
    }
}

/// The two hosts that present the top layer: Android's shell makes
/// windows, and Windows overlays the page.
const HOSTS: [&str; 2] = ["android", "windows"];

/// `source` on `host`, with its select focused.
fn boot(source: &str, host: &str, viewport: (f32, f32)) -> (Presenter<()>, ViewId) {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        viewport,
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    match host {
        "android" => {
            p.serve_requests();
            p.serve_windows(Box::new(Paints));
        }
        _ => p.serve_overlays(),
    }
    p.frame();
    let id = named(&p, "pick");
    assert!(p.set_focus(Some(id), p.host.now()).is_none());
    (p, id)
}

fn named(p: &Presenter<()>, name: &str) -> ViewId {
    let k = p.host.kernel();
    k.node_by_key(k.find_by_test_id(name)[0]).unwrap().id
}

fn slot(p: &Presenter<()>, name: &str) -> String {
    let value = p.host.runner().slot(name).unwrap();
    value.as_str().unwrap().to_owned()
}

fn key(p: &mut Presenter<()>, name: &str) {
    p.key_down(name, p.host.now());
}

/// The open surface's focused row.
fn focused(p: &Presenter<()>) -> Option<usize> {
    p.top.open.as_ref().and_then(|open| open.focused)
}

/// Each row of the open surface: its title, whether it is enabled and
/// whether it is checked.
fn rows(p: &Presenter<()>) -> Vec<(String, bool, bool)> {
    let shown = p.surface().expect("a surface is open");
    (shown.plan.rows())
        .map(|row| (row.title.clone(), row.enabled, row.checked))
        .collect()
}

#[test]
fn a_select_opens_its_options_as_the_top_layers_menu() {
    for host in HOSTS {
        let (mut p, id) = boot(PICK, host, (300., 400.));
        p.tap(id).unwrap();
        assert_eq!(p.menu, None, "{host}: not painted over the page");
        let shown = p.surface().expect("the select's menu is a surface");
        assert_eq!((shown.kind, shown.node), (SurfaceKind::Menu, id));
        let views: Vec<ViewId> = shown.targets.iter().map(|t| t.node).collect();
        let choices = p.host.kernel().select_choices(id);
        assert_eq!(views, choices.iter().map(|c| c.view).collect::<Vec<_>>());
        let select = p.box_of(id).unwrap().rect;
        let size = p.surface().unwrap().size;
        assert_eq!(size.0, select.2, "{host}: as wide as the select");
        let open = p.top.open.as_ref().unwrap();
        assert!(
            open.frame[1] as f32 >= select.1 + select.3,
            "{host}: hung under the select, {:?} under {select:?}",
            open.frame
        );
        assert_eq!(
            rows(&p),
            [
                ("Alpha".into(), true, true),
                ("Beta".into(), false, false),
                ("Gamma".into(), true, false),
            ],
            "{host}: the chosen one checked, the disabled one disabled"
        );
        assert_eq!(p.focus(), Some(id), "{host}: the select keeps the focus");
        assert_eq!(focused(&p), None, "{host}: a press focuses no row");
        p.frame();

        assert!(p.tap(views[1]).is_err(), "{host}: a disabled option");
        assert!(p.surface().is_some());
        p.tap(views[2]).unwrap();
        assert!(p.surface().is_none(), "{host}: a choice closes it");
        assert_eq!(slot(&p, "pick"), "c", "{host}");
        assert_eq!(p.focus(), Some(id), "{host}");

        // Reopened, it checks the new choice; Escape and Back close it
        // and choose nothing.
        p.tap(id).unwrap();
        assert_eq!(rows(&p)[2], ("Gamma".into(), true, true), "{host}");
        key(&mut p, "Escape");
        assert!(p.surface().is_none(), "{host}: Escape closes it");
        p.tap(id).unwrap();
        assert!(p.back_goes(), "{host}: an open menu takes Back");
        assert!(p.system_back(p.host.now()));
        assert!(p.surface().is_none(), "{host}: Back closes it");
        assert_eq!(slot(&p, "log"), "c ", "{host}");
    }
}

#[test]
fn an_open_select_menu_takes_the_combobox_keys() {
    for host in HOSTS {
        let (mut p, id) = boot(PICK, host, (300., 400.));
        key(&mut p, "ArrowDown");
        assert!(p.surface().is_some(), "{host}: an arrow opens it");
        assert_eq!(focused(&p), Some(0), "{host}: at the chosen option");
        key(&mut p, "ArrowDown");
        assert_eq!(focused(&p), Some(2), "{host}: past the disabled one");
        assert_eq!(slot(&p, "log"), "", "{host}: an arrow chooses nothing");
        key(&mut p, "Enter");
        assert!(p.surface().is_none());
        assert_eq!(slot(&p, "pick"), "c", "{host}: Enter chooses");

        key(&mut p, " ");
        assert_eq!(focused(&p), Some(2), "{host}: at the chosen option");
        key(&mut p, "a");
        assert_eq!(focused(&p), Some(0), "{host}: a letter moves to its option");
        key(&mut p, "End");
        assert_eq!(focused(&p), Some(2), "{host}");
        key(&mut p, "Home");
        assert_eq!(focused(&p), Some(0), "{host}");
        key(&mut p, " ");
        assert_eq!(slot(&p, "pick"), "a", "{host}: Space chooses");

        // A menu a press opened starts its arrows at the chosen option.
        p.tap(id).unwrap();
        key(&mut p, "ArrowDown");
        assert_eq!(focused(&p), Some(2), "{host}");
        key(&mut p, "Tab");
        assert!(p.surface().is_none(), "{host}: Tab closes it");
        assert_eq!(p.focus(), Some(named(&p, "after")), "{host}: and moves on");
        assert_eq!(slot(&p, "log"), "c a ", "{host}");
    }
}

/// HTML-AAM's `select` popup: the surface is a listbox of the options, each
/// placed in its set, the chosen one selected, and a service chooses one
/// as a press on its row does.
#[test]
fn an_open_select_menu_is_a_listbox_to_assistive_technology() {
    for host in HOSTS {
        let (mut p, id) = boot(PICK, host, (300., 400.));
        p.tap(id).unwrap();
        let now = p.host.now();
        assert!(p.reader_access(1., true).is_some());
        let tree = p.access_told().to_vec();
        let roles: Vec<(i32, &str)> = tree.iter().map(|n| (n.role, n.name.as_str())).collect();
        assert_eq!(
            roles[1..],
            [
                (role::OPTION, "Alpha"),
                (role::OPTION, "Beta"),
                (role::OPTION, "Gamma"),
            ],
            "{host}: the page under the menu is left out"
        );
        let (list, options) = (&tree[0], &tree[1..]);
        assert_eq!((list.role, list.parent), (role::LIST_BOX, 0), "{host}");
        for (at, option) in options.iter().enumerate() {
            assert_eq!(option.parent, list.id);
            assert_eq!(option.set, (at as i32 + 1, 3));
        }
        let flags: Vec<i32> = options.iter().map(|n| n.flags).collect();
        assert_eq!(
            flags,
            [flag::CLICK | flag::SELECTED, flag::DISABLED, flag::CLICK],
            "{host}"
        );
        assert!(!p.access_act(options[1].id, act::CLICK, 0., now));
        assert!(p.surface().is_some());
        assert!(p.access_act(options[2].id, act::CLICK, 0., now));
        assert!(p.surface().is_none(), "{host}");
        assert_eq!(slot(&p, "pick"), "c", "{host}");
    }
}

/// Sixty options.
fn long() -> String {
    let mut source = String::from(
        "component App\n  state pick = \"v0\"\n  action choose(v: string)\n    pick = v\n  view\n    column\n",
    );
    source.push_str("      select value=pick change=choose testId=\"pick\"\n");
    for i in 0..60 {
        source.push_str(&format!("        option \"Option {i}\" value=\"v{i}\"\n"));
    }
    source
}

/// A menu longer than the window scrolls: it opens at its chosen row,
/// PageDown moves a page, and a service scrolls its listbox.
#[test]
fn a_long_select_menu_opens_at_its_choice_and_scrolls() {
    for host in HOSTS {
        let (mut p, id) = boot(&long(), host, (300., 400.));
        key(&mut p, "PageDown");
        assert!(p.surface().is_none(), "{host}: PageDown opens no menu");
        key(&mut p, "ArrowDown");
        key(&mut p, "PageDown");
        let page = focused(&p).unwrap();
        assert!(page > 3 && page < 20, "{host}: PageDown to {page}");
        key(&mut p, "End");
        key(&mut p, "Enter");
        assert_eq!(slot(&p, "pick"), "v59", "{host}");

        p.tap(id).unwrap();
        let shown = p.surface().unwrap();
        let (offset, _) = shown.scroll.expect("it scrolls");
        let (clip, last) = (shown.clip.unwrap(), shown.targets[59].rect);
        assert!(
            offset > 0. && last.1 >= clip.1 && last.1 + last.3 <= clip.1 + clip.3,
            "{host}: the chosen row shows, {last:?} in {clip:?}"
        );
        p.reader_access(1., true);
        let list = p.access_told()[0].clone();
        assert_eq!(list.role, role::LIST_BOX);
        assert_ne!(list.flags & flag::BACKWARD, 0, "{host}: it scrolls back");
        assert!(p.access_act(list.id, act::BACKWARD, 0., p.host.now()));
        let (moved, _) = p.surface().unwrap().scroll.unwrap();
        assert!(moved < offset, "{host}: a service scrolls it");
    }
}
