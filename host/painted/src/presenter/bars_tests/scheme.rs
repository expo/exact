//! Each bar is in the scheme of the node it shows (LLP 1034 §8): the top
//! bar its header's, the navigation bar its tablist's (expo/exact#1469).
use super::*;
use crate::presenter::top::look::Role;

#[test]
fn each_bar_is_in_its_nodes_scheme() {
    let root = r#"main navigationKey="#;
    let tabs = r#"row role="tablist" testId="tabs""#;
    // Where the scheme is written, the device's scheme, and the top bar's
    // scheme and the navigation bar's.
    for (on_root, on_tabs, device, want) in [
        (r#"color-scheme="dark""#, "", false, (true, true)),
        ("", r#"color-scheme="dark""#, false, (false, true)),
        ("", "", true, (true, true)),
        (r#"color-scheme="light""#, "", true, (false, false)),
    ] {
        let source = APP
            .replace(root, &format!("main {on_root} navigationKey="))
            .replace(tabs, &format!("{tabs} {on_tabs}"));
        let (mut p, pictures) = barred(&source);
        p.set_system_scheme(device);
        p.reader_bars(1.0).unwrap();
        let all = pictures.borrow();
        let colour = |height: u32| {
            let bar = (all.iter().rev())
                .find(|p| p.height() == height)
                .expect("the bar was painted");
            let c = bar.pixel(2, 2).unwrap();
            [c.red(), c.green(), c.blue(), c.alpha()]
        };
        let bottom = (nav_height(1.0) + WINDOW.bars[2]) as u32;
        let case = format!("{on_root} {on_tabs} device dark {device}");
        assert_eq!(colour(116), Role::Surface.color(want.0), "top: {case}");
        assert_eq!(
            colour(bottom),
            Role::SurfaceContainer.color(want.1),
            "bottom: {case}"
        );
    }
}

fn has_ink(bar: &Pixmap, columns: std::ops::Range<u32>, ink: [u8; 4]) -> bool {
    columns.into_iter().any(|x| {
        (0..bar.height()).any(|y| {
            let c = bar.pixel(x, y).unwrap();
            [c.red(), c.green(), c.blue(), c.alpha()] == ink
        })
    })
}

#[test]
fn the_lifted_elements_supply_their_bands_ink_and_system_icon_appearance() {
    let source = APP
        .replace(
            "header display",
            r##"header background-color="#173529" display"##,
        )
        .replace(
            r#"testId="tabs""#,
            r##"testId="tabs" background-color="#ede4cf" color="#483923""##,
        );
    let (mut p, pictures) = barred(&source);
    p.set_system_scheme(false);
    assert!(p.chrome().status_light_icons);
    assert!(!p.chrome().navigation_light_icons);
    p.reader_bars(1.0).unwrap();
    let all = pictures.borrow();
    let top = &all[all.len() - 2];
    let bottom = all.last().unwrap();
    let pixel = |bar: &Pixmap, y| {
        let c = bar.pixel(0, y).unwrap();
        [c.red(), c.green(), c.blue(), c.alpha()]
    };
    assert_eq!(pixel(top, 0), [0x17, 0x35, 0x29, 255]);
    assert_eq!(pixel(bottom, bottom.height() - 1), [0xed, 0xe4, 0xcf, 255]);
    assert!(has_ink(top, 0..200, [255; 4]), "heading");
    assert!(has_ink(top, 350..412, [255; 4]), "action icon");
    assert!(
        has_ink(bottom, 210..412, [0x48, 0x39, 0x23, 255]),
        "unselected destination"
    );
    drop(all);
    let list = id(&p, "list");
    p.scroll_to(list, (0., 100.));
    p.reader_bars(1.0).unwrap();
    assert_eq!(bar_colour(&pictures), [0x17, 0x35, 0x29, 255]);

    p.set_system_scheme(true);
    assert!(
        !p.chrome().navigation_light_icons,
        "dark scheme, dark authored ink"
    );
    let explicit = source.replace("main navigationKey", r#"main status-bar-style="dark-content" navigation-bar-style="light-content" navigationKey"#);
    let (p, _) = barred(&explicit);
    assert!(!p.chrome().status_light_icons);
    assert!(p.chrome().navigation_light_icons);
}

#[test]
fn readable_midtone_ink_picks_opposite_system_icons_on_opposite_bands() {
    let source = APP
        .replace(
            "header display",
            r##"header color="#757575" background-color="white" display"##,
        )
        .replace(
            r#"testId="tabs""#,
            r##"testId="tabs" color="#757575" background-color="black""##,
        );
    let (p, _) = barred(&source);
    assert!(!p.chrome().status_light_icons);
    assert!(p.chrome().navigation_light_icons);
}

#[test]
fn repainting_the_header_preserves_a_finger_on_a_destination() {
    let source = APP
        .replace(
            "header display",
            r##"header background-color=(said == "nothing" ? "#ffffff" : "#173529") display"##,
        )
        .replace(r#"press=open("/detail")"#, "press=edit");
    let (mut p, _) = barred(&source);
    let tab = id(&p, "tab-other");
    let rect = p
        .bars()
        .1
        .unwrap()
        .targets
        .iter()
        .find(|t| t.what == What::Tab(tab))
        .unwrap()
        .rect;
    let (x, y) = (rect.0 + rect.2 / 2., rect.1 + rect.3 / 2.);
    p.bar_event(BOTTOM, 0, x, y, 0.);
    tap(&mut p, "open").unwrap();
    p.reader_bars(1.).unwrap();
    assert!(p.chrome().status_light_icons);
    p.bar_event(BOTTOM, 1, x, y, 1.);
    assert_eq!(top(&mut p).title, "Other");
}

#[test]
fn translucent_bar_ink_reads_the_window_behind_the_band() {
    let source = APP
        .replace(
            "main navigationKey",
            r#"main background-color="white" navigationKey"#,
        )
        .replace(
            "header display",
            r#"header background-color="rgba(23,53,41,0.5)" display"#,
        );
    let (mut p, pictures) = barred(&source);
    p.reader_bars(1.).unwrap();
    let all = pictures.borrow();
    let top = &all[all.len() - 2];
    assert_eq!(top.pixel(0, 0).unwrap().alpha(), 128);
    assert!(has_ink(top, 0..200, [0, 0, 0, 255]));
    assert!(!p.chrome().status_light_icons);
}

#[test]
fn changing_only_the_seed_keeps_an_icon_only_headers_layout() {
    let source = APP
        .replace(
            "main navigationKey",
            r##"main accent-color=(said == "nothing" ? "#245c46" : "#bb2222") navigationKey"##,
        )
        .replace(r#"press=open("/detail")"#, "press=edit");
    let (mut p, pictures) = barred(&source);
    p.reader_bars(1.).unwrap();
    let top_targets = p.bars.top.as_ref().unwrap().layout.targets.as_ptr();
    let nav = pictures.borrow().last().unwrap().clone();
    tap(&mut p, "open").unwrap();
    p.reader_bars(1.).unwrap();
    assert_eq!(
        top_targets,
        p.bars.top.as_ref().unwrap().layout.targets.as_ptr()
    );
    assert_ne!(nav.data(), pictures.borrow().last().unwrap().data());
}
