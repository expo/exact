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
