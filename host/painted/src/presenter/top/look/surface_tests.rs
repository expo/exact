//! Tests for Android projected dropdown menu container styling and sibling
//! leading-column alignment (LLP 1115.001 D10, #1956).
use super::*;
use crate::paint::look::luminance;
use crate::presenter::access::flag;
use crate::presenter::bars::look::ink::over;
use crate::presenter::top::SurfacePainter;
use crate::presenter::{PainterChoice, Presenter};
use std::cell::RefCell;
use std::path::PathBuf;
use std::rc::Rc;

struct CapturingPainter(Rc<RefCell<Vec<tiny_skia::Pixmap>>>);

impl SurfacePainter for CapturingPainter {
    fn record(
        &mut self,
        size: (f32, f32),
        scale: f32,
        paint: &mut dyn FnMut(&mut dyn Backend),
    ) -> Vec<u32> {
        let mut raster = crate::raster::Raster::transparent();
        raster.begin(size.0, size.1, scale);
        paint(&mut raster);
        let pixmap = raster.finish().unwrap();
        let words = vec![pixmap.width(), pixmap.height()];
        self.0.borrow_mut().push(pixmap);
        words
    }
}

fn contrast(ink: [u8; 4], ground: [u8; 4]) -> f32 {
    let fg = luminance(over(ink, ground)) + 0.05;
    let bg = luminance(ground) + 0.05;
    fg.max(bg) / fg.min(bg)
}

fn boot(
    source: &str,
    host: &str,
    size: (f32, f32),
    dark: bool,
) -> (Presenter<()>, Rc<RefCell<Vec<tiny_skia::Pixmap>>>) {
    let pixels = Rc::new(RefCell::new(Vec::new()));
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        size,
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.set_system_scheme(dark);
    if host == "windows" {
        p.serve_overlays();
    } else {
        p.serve_requests();
        p.serve_windows(Box::new(CapturingPainter(pixels.clone())));
    }
    p.frame();
    (p, pixels)
}

fn id(p: &Presenter<()>, test_id: &str) -> ViewId {
    let k = p.host().kernel();
    k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id
}

/// Open the menu via `trigger` and return the shell's reported `(radius_px, elevation_px)`.
fn open_menu(p: &mut Presenter<()>, trigger: &str) -> (f32, f32) {
    p.tell_shell();
    p.take_requests();
    let btn = id(p, trigger);
    p.tap(btn).unwrap();
    p.run_commands(|| ());
    p.frame();
    p.tell_shell();
    let words = p.take_requests().unwrap_or_default();
    let (mut shape, mut at) = (None, 0);
    while at < words.len() {
        let (kind, n) = (words[at], words[at + 2] as usize);
        let body = &words[at + 3..at + 3 + n];
        if kind == 7 && body[1] != 0 {
            shape = Some((
                f32::from_bits(body[7] as u32),
                f32::from_bits(body[8] as u32),
            ));
        }
        at += 3 + n;
    }
    shape.expect("opened menu surface request")
}

fn radius_logs(p: &Presenter<()>) -> usize {
    p.host()
        .runner()
        .journal()
        .filter(|l| l.contains("menu border-radius"))
        .count()
}

#[test]
fn unstyled_android_menu_matches_default_material_pixels_and_metrics() {
    let src = r#"component App
  state picked = "a"
  action pick(v: string)
    picked = v
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
        button "Rename" press=pick("a")
        button "Delete" destructive=true press=pick("b")
"#;
    let (mut p, px1) = boot(src, "android", (400., 800.), false);
    let shape1 = open_menu(&mut p, "open");
    assert_eq!(shape1, (4., 3.));
    let open = p.top.open.as_ref().unwrap();
    assert_eq!(open.layout.container, Role::SurfaceContainer.color(false));
    assert_eq!(open.layout.radius, 4.);
    assert!(!open.layout.non_uniform_radius);

    // Explicitly writing `background-color="transparent"` matches unstyled byte-for-byte.
    let transparent_src = src.replace(
        r#"role="menu""#,
        r#"role="menu" background-color="transparent""#,
    );
    let (mut p2, px2) = boot(&transparent_src, "android", (400., 800.), false);
    let shape2 = open_menu(&mut p2, "open");
    assert_eq!(shape1, shape2);
    assert_eq!(
        px1.borrow().last().unwrap().data(),
        px2.borrow().last().unwrap().data()
    );
}

#[test]
fn authored_background_color_and_uniform_radius_honor_contrast_and_theme() {
    let src = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="#ffffff" border-radius=16 border="3px solid #00ff00" color="#ff00ff" padding=40 width=320
        button press=noop
          image "symbol:pencil"
          text "Rename"
        button "Disabled" disabled=true press=noop
        hr
        button destructive=true press=noop
          image "symbol:delete"
          text "Delete"
"##;
    // Forced light surface inside a dark-mode app: container stays #ffffff,
    // ink flips to dark-on-light and every item meets its contrast floor.
    let (mut p, _) = boot(src, "android", (400., 800.), true);
    let shape = open_menu(&mut p, "open");
    let open = p.top.open.as_ref().unwrap();
    assert_eq!(open.layout.container, [255, 255, 255, 255]);
    assert_eq!(open.layout.radius, 16.);
    assert_eq!(shape, (16., 3.));
    // Unhonored authored properties stay ignored: no border, width capped to Material's 280 dp.
    assert_eq!(open.layout.border, None);
    assert!(open.layout.size.0 <= 280., "width: {}", open.layout.size.0);

    for item in &open.layout.items {
        match item {
            Item::Text(_, _, color, node) => {
                let enabled = open
                    .layout
                    .targets
                    .iter()
                    .find(|t| t.node == *node)
                    .map_or(true, |t| t.enabled);
                if enabled {
                    assert!(
                        contrast(*color, open.layout.container) >= 4.5,
                        "text ink {color:?} fails 4.5:1 on white"
                    );
                }
            }
            Item::Symbol(_, _, color, _) => {
                assert!(
                    contrast(*color, open.layout.container) >= 3.0,
                    "symbol ink {color:?} fails 3.0:1 on white"
                );
            }
            Item::Rule(_, color) => {
                assert!(
                    contrast(*color, open.layout.container) >= 1.2,
                    "rule {color:?} fails 1.2:1 on white"
                );
            }
        }
    }
    drop(p);

    // `light-dark(...)` and saturated backgrounds stay legible too, and huge radii clamp.
    let saturated = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="light-dark(#b3261e, #ffd8e4)" border-radius=999
        button press=noop
          image "symbol:pencil"
          text "Rename"
        hr
        button destructive=true press=noop
          image "symbol:delete"
          text "Delete"
"##;
    for (dark, expected_bg) in [
        (false, [0xb3, 0x26, 0x1e, 255]),
        (true, [0xff, 0xd8, 0xe4, 255]),
    ] {
        let (mut p, _) = boot(saturated, "android", (400., 800.), dark);
        let shape = open_menu(&mut p, "open");
        let open = p.top.open.as_ref().unwrap();
        assert_eq!(open.layout.container, expected_bg);
        let clamped = open.layout.size.0.min(open.layout.size.1) / 2.;
        assert!((open.layout.radius - clamped).abs() < 0.01);
        assert!((shape.0 - clamped).abs() < 0.01);
        for item in &open.layout.items {
            match item {
                Item::Text(_, _, color, _) => {
                    assert!(
                        contrast(*color, expected_bg) >= 4.5,
                        "dark={dark}: text {color:?} on {expected_bg:?}"
                    );
                }
                Item::Symbol(_, _, color, _) => {
                    assert!(
                        contrast(*color, expected_bg) >= 3.0,
                        "dark={dark}: symbol {color:?} on {expected_bg:?}"
                    );
                }
                Item::Rule(_, color) => {
                    assert!(
                        contrast(*color, expected_bg) >= 1.2,
                        "dark={dark}: rule {color:?} on {expected_bg:?}"
                    );
                }
            }
        }
    }
}

#[test]
fn translucent_background_composites_opaquely_over_surface_container() {
    let src = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="rgba(0, 128, 255, 0.5)"
        button "One" press=noop
"##;
    let (mut p, _) = boot(src, "android", (400., 800.), false);
    open_menu(&mut p, "open");
    let open = p.top.open.as_ref().unwrap();
    let expected = over([0, 128, 255, 128], Role::SurfaceContainer.color(false));
    assert_eq!(open.layout.container, expected);
    assert_eq!(open.layout.container[3], 255);
}

#[test]
fn non_uniform_and_elliptical_radii_fall_back_to_4dp_and_log_once() {
    for style in [
        r#"border-top-left-radius=16"#,
        r#"border-radius="16px 4px 16px 4px""#,
        r#"border-radius="50%""#,
    ] {
        let src = format!(
            r#"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" {style}
        button "One" press=noop
        button "Two" press=noop
        button "Three" press=noop
"#
        );
        let (mut p, _) = boot(&src, "android", (400., 800.), false);
        let shape = open_menu(&mut p, "open");
        assert_eq!(shape.0, 4., "{style}");
        assert_eq!(radius_logs(&p), 1, "{style}");
        // Resizing while the menu stays open does not repeat the warning.
        p.resize(380., 800.);
        p.frame();
        assert_eq!(radius_logs(&p), 1, "{style} after resize");
    }
}

#[test]
fn select_and_windows_menus_ignore_container_overrides() {
    let select_src = r##"component App
  state picked = "a"
  view
    column
      select value=picked testId="pick" background-color="#123456" border-radius=20
        option "Alpha" value="a"
        option "Beta" value="b"
"##;
    let (mut p, _) = boot(select_src, "android", (400., 800.), false);
    let shape = open_menu(&mut p, "pick");
    let open = p.top.open.as_ref().unwrap();
    assert_eq!(open.layout.container, Role::SurfaceContainer.color(false));
    assert_eq!(open.layout.radius, 4.);
    assert_eq!(shape.0, 4.);
    drop(p);

    let menu_src = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="#123456" border-radius=20
        button "Alpha" press=noop
"##;
    let (mut win, _) = boot(menu_src, "windows", (400., 800.), false);
    let btn = id(&win, "open");
    win.tap(btn).unwrap();
    win.run_commands(|| ());
    win.frame();
    let open = win.top.open.as_ref().unwrap();
    assert_eq!(open.layout.container, Role::Surface.color(false));
    assert_eq!(open.layout.radius, 8.);
}

#[test]
fn rtl_material_menu_mirrors_directional_symbols_and_respects_item_direction() {
    let src = r#"component App
  action noop
    let _ = 0
  view
    column direction="rtl"
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
        button press=noop
          image "symbol:send"
          text "Send"
        button press=noop
          image "symbol:send" direction="ltr"
          text "Raw"
        button role="menuitemcheckbox" aria-checked=true press=noop
          text "Checked"
"#;
    let (mut p, _) = boot(src, "android", (400., 800.), false);
    open_menu(&mut p, "open");
    let open = p.top.open.as_ref().unwrap();
    let mut raster = crate::raster::Raster::new();
    raster.begin(open.layout.size.0, open.layout.size.1, 2.0);
    open.layout.paint(
        &mut raster,
        &mut crate::text::fixtures::engine(),
        Feedback::default(),
    );
    let pix = raster.finish().unwrap();
    let icons: Vec<(String, Rect4, Direction)> = open
        .layout
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Symbol(role, rect, _, dir) => Some((role.clone(), *rect, *dir)),
            _ => None,
        })
        .collect();
    assert_eq!(icons.len(), 3, "{icons:?}");
    let dark_at = |rect: Rect4, u: f32, v: f32| {
        let px = ((rect.0 + u / 24.0 * rect.2) * 2.0) as u32;
        let py = ((rect.1 + v / 24.0 * rect.3) * 2.0) as u32;
        let c = pix.pixel(px, py).unwrap().demultiply();
        c.red() < 180
    };
    // Row 0: Material `send` in RTL mirrors (base at right, u=20).
    assert_eq!(icons[0].2, Direction::Rtl);
    assert!(dark_at(icons[0].1, 20., 6.) && !dark_at(icons[0].1, 4., 6.));
    // Row 1: `send` overridden to LTR stays pointing right (base at left, u=4).
    assert_eq!(icons[1].2, Direction::Ltr);
    assert!(dark_at(icons[1].1, 4., 6.) && !dark_at(icons[1].1, 20., 6.));
    // Row 2: `checkmark` stays unmirrored even in RTL (long right stroke at upper-right u=18, v=8).
    assert_eq!(icons[2].0, "checkmark");
    assert!(dark_at(icons[2].1, 18., 8.) && !dark_at(icons[2].1, 6., 8.));
}

#[test]
fn checkable_menu_aligns_all_sibling_labels_and_stays_stable_when_selection_changes() {
    let src = r#"component App
  state chosen = "one"
  action pick(v: string)
    chosen = v
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
        column role="group" aria-label="Sort by"
          button "Name" role="menuitemradio" aria-checked=(chosen == "one") press=pick("one") testId="one"
          button "Date" role="menuitemradio" aria-checked=(chosen == "two") press=pick("two") testId="two"
          button "Size" role="menuitemradio" aria-checked=(chosen == "three") press=pick("three") testId="three"
"#;
    let (mut p, _) = boot(src, "android", (400., 800.), false);
    open_menu(&mut p, "open");
    let positions = |p: &Presenter<()>| {
        let open = p.top.open.as_ref().unwrap();
        open.layout
            .texts()
            .into_iter()
            .map(|(_, rect)| rect.0)
            .collect::<Vec<_>>()
    };
    let before = positions(&p);
    assert_eq!(before.len(), 4);
    // Section header stays flush at 12 dp; all 3 items align at 12 + 36 = 48 dp.
    assert!((before[0] - 12.).abs() < 0.5, "header: {before:?}");
    for x in &before[1..] {
        assert!(
            (*x - 48.).abs() < 0.5,
            "items must align at 48 dp: {before:?}"
        );
    }
    // TalkBack accessibility retains CHECKABLE / CHECKED state on items.
    p.reader_access(1., true);
    let nodes = p.access_told();
    let checked_count = nodes
        .iter()
        .filter(|n| n.flags & flag::CHECKED != 0)
        .count();
    let checkable_count = nodes
        .iter()
        .filter(|n| n.flags & flag::CHECKABLE != 0)
        .count();
    assert_eq!((checkable_count, checked_count), (3, 1));

    // Choosing another item and reopening keeps every label's x-coordinate identical.
    let two = id(&p, "two");
    p.tap(two).unwrap();
    p.run_commands(|| ());
    p.frame();
    open_menu(&mut p, "open");
    let after = positions(&p);
    for (a, b) in before.iter().zip(&after) {
        assert!(
            (a - b).abs() < 0.01,
            "labels shifted: {before:?} vs {after:?}"
        );
    }

    // A menu with no symbols and no checkable items keeps labels at 12 dp.
    let plain = r#"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
        button "Copy" press=noop
        button "Share" press=noop
"#;
    let (mut p3, _) = boot(plain, "android", (400., 800.), false);
    open_menu(&mut p3, "open");
    for x in positions(&p3) {
        assert!((x - 12.).abs() < 0.5, "plain menu labels stay at 12 dp");
    }
}

#[test]
fn large_radius_clips_content_and_clamps_to_scrolling_height() {
    let mut items = String::new();
    for i in 0..12 {
        items.push_str(&format!("        button \"Item {i}\" press=noop\n"));
    }
    let src = format!(
        r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="#1e293b" border-radius=200
{items}"##
    );
    let (mut p, _) = boot(&src, "android", (360., 300.), false);
    let shape = open_menu(&mut p, "open");
    let open = p.top.open.as_ref().unwrap();
    assert!(open.layout.menu_scroll().is_some(), "long menu scrolls");
    let expected = open.layout.size.0.min(open.layout.size.1) / 2.;
    assert!((open.layout.radius - expected).abs() < 0.01);
    assert!((shape.0 - expected).abs() < 0.01);
    // Painted with a pressed first row, corner pixels outside the rounded shape stay transparent.
    let mut raster = crate::raster::Raster::transparent();
    raster.begin(open.layout.size.0, open.layout.size.1, 1.);
    open.layout.paint(
        &mut raster,
        &mut crate::text::fixtures::engine(),
        Feedback {
            pressed: Some(0),
            ..Default::default()
        },
    );
    let pixmap = raster.finish().unwrap();
    let corner = pixmap.pixel(0, 0).unwrap();
    assert_eq!(
        corner.alpha(),
        0,
        "pressed row is clipped inside container corners"
    );
    drop(p);

    // Non-scrolling large-radius Android menu (no `menu_clip`): corner arc at (1, 10)
    // still clips a pressed first row (which starts at y = 8 dp).
    let non_scroll = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="#1e293b" border-radius=24
        button "Alpha" press=noop
        button "Beta" press=noop
"##;
    let (mut p2, _) = boot(non_scroll, "android", (360., 600.), false);
    open_menu(&mut p2, "open");
    let open = p2.top.open.as_ref().unwrap();
    assert!(open.layout.menu_scroll().is_none());
    let mut raster = crate::raster::Raster::transparent();
    raster.begin(open.layout.size.0, open.layout.size.1, 1.);
    open.layout.paint(
        &mut raster,
        &mut crate::text::fixtures::engine(),
        Feedback {
            pressed: Some(0),
            ..Default::default()
        },
    );
    let pixmap = raster.finish().unwrap();
    // For a 24 dp corner circle centered at (24, 24), x=1, y=10 is distance ~26.9 > 24.
    assert_eq!(
        pixmap.pixel(1, 10).unwrap().alpha(),
        0,
        "non-scrolling large-radius menu clips pressed row at corner arc"
    );
}

#[test]
fn symbolic_checkable_rows_keep_width_and_labels_stable_across_selection_changes() {
    for dir in ["ltr", "rtl"] {
        let src = format!(
            r#"component App
  state chosen = "a"
  action pick(v: string)
    chosen = v
  view
    column direction="{dir}"
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
        button role="menuitemradio" aria-checked=(chosen == "a") press=pick("a") testId="a"
          image "symbol:pencil"
          text "Short"
        button role="menuitemradio" aria-checked=(chosen == "b") press=pick("b") testId="b"
          image "symbol:delete"
          text "A noticeably wider option label"
"#
        );
        let (mut p, _) = boot(&src, "android", (400., 800.), false);
        open_menu(&mut p, "open");
        let snapshot = |p: &Presenter<()>| {
            let open = p.top.open.as_ref().unwrap();
            (
                open.layout.size,
                open.layout
                    .texts()
                    .into_iter()
                    .map(|(_, r)| (r.0, r.2))
                    .collect::<Vec<_>>(),
            )
        };
        let before = snapshot(&p);
        let b = id(&p, "b");
        p.tap(b).unwrap();
        p.run_commands(|| ());
        p.frame();
        open_menu(&mut p, "open");
        let after = snapshot(&p);
        assert_eq!(before.0, after.0, "{dir}: menu size shifted on selection");
        assert_eq!(before.1, after.1, "{dir}: label rects shifted on selection");
    }
}

#[test]
fn midtone_background_keeps_text_contrast_under_focus_state_layer() {
    let src = r##"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="#777777"
        button press=noop
          image "symbol:pencil"
          text "Midtone"
        button destructive=true press=noop
          image "symbol:delete"
          text "Delete"
"##;
    let (mut p, _) = boot(src, "android", (400., 800.), false);
    open_menu(&mut p, "open");
    let open = p.top.open.as_ref().unwrap();
    let container = open.layout.container;
    let (_, mut tint) = open.layout.targets[0].layer;
    tint[3] = (f32::from(tint[3]) * STATE_LAYER).round() as u8;
    let lit = over(tint, container);
    for item in &open.layout.items {
        match item {
            Item::Text(_, _, color, _) => {
                assert!(
                    contrast(*color, container) >= 4.5 && contrast(*color, lit) >= 4.5,
                    "text {color:?} fails 4.5:1 on container {container:?} or focused {lit:?}"
                );
            }
            Item::Symbol(_, _, color, _) => {
                assert!(
                    contrast(*color, container) >= 3.0 && contrast(*color, lit) >= 3.0,
                    "symbol {color:?} fails 3.0:1 on container {container:?} or focused {lit:?}"
                );
            }
            Item::Rule(_, _) => {}
        }
    }
}

#[test]
fn scrolling_windows_menu_keeps_its_rounded_corner_clip() {
    let mut items = String::new();
    for i in 0..10 {
        items.push_str(&format!(
            "        button \"Item {i}\" press=noop\n        hr\n"
        ));
    }
    let src = format!(
        r#"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu"
{items}"#
    );
    let (mut win, _) = boot(&src, "windows", (400., 220.), false);
    let btn = id(&win, "open");
    win.tap(btn).unwrap();
    win.run_commands(|| ());
    win.frame();
    let open = win.top.open.as_mut().unwrap();
    assert!(open.layout.menu_scroll().is_some(), "windows menu scrolls");
    // Scroll so a full-width separator sits at y = 2 dp, right inside the 8 dp top corners.
    let first_rule_y = open
        .layout
        .items
        .iter()
        .find_map(|item| match item {
            Item::Rule(r, _) => Some(r.1),
            _ => None,
        })
        .unwrap();
    open.layout.scroll_content(first_rule_y - 2.);
    let mut raster = crate::raster::Raster::transparent();
    raster.begin(open.layout.size.0, open.layout.size.1, 1.);
    open.layout.paint(
        &mut raster,
        &mut crate::text::fixtures::engine(),
        Feedback::default(),
    );
    let pixmap = raster.finish().unwrap();
    // At (1, 2), inside the rectangular menu_clip (1..w-1, 2..h-2) but outside the 8 dp corner arc:
    // radius 8 centered at (8, 8) -> dist((1,2), (8,8)) = sqrt(49+36) = 9.22 > 8.
    assert_eq!(
        pixmap.pixel(1, 2).unwrap().alpha(),
        0,
        "scrolling windows menu clips full-width separator to its 8 dp container corners"
    );
}

#[test]
fn system_role_background_refreshes_on_palette_update_and_keeps_scroll_and_press() {
    if !crate::presenter::control::material_tests::material_process() {
        return;
    }
    let mut items = String::new();
    for i in 0..12 {
        items.push_str(&format!("        button \"Item {i}\" press=noop\n"));
    }
    let src = format!(
        r#"component App
  action noop
    let _ = 0
  view
    column
      button "Open" popovertarget="m" testId="open"
      column id="m" popover="auto" role="menu" background-color="AccentColor"
{items}"#
    );
    let (mut p, _) = boot(&src, "android", (360., 300.), false);
    let (p1, _) = crate::presenter::palette::Palette::seeded([[0x10, 0x60, 0xd0]; 2], 0.);
    p.set_palette(&p1);
    p.frame();
    open_menu(&mut p, "open");
    let before_bg = p.top.open.as_ref().unwrap().layout.container;
    // Scroll down and hold a press on row 2.
    {
        let open = p.top.open.as_mut().unwrap();
        assert!(open.layout.scroll_content(60.));
        open.pressed = Some(2);
    }
    let before_scroll = p.top.open.as_ref().unwrap().layout.menu_scroll().unwrap().0;
    // Update the reported palette while the menu stays open.
    let (p2, _) = crate::presenter::palette::Palette::seeded([[0xb0, 0x20, 0x30]; 2], 0.);
    p.set_palette(&p2);
    p.frame();
    p.tell_shell();
    let open = p.top.open.as_ref().unwrap();
    assert_ne!(
        open.layout.container, before_bg,
        "AccentColor menu refreshes when the reported palette changes"
    );
    assert_eq!(
        open.layout.menu_scroll().unwrap().0,
        before_scroll,
        "paint-only palette refresh preserves scroll offset"
    );
    assert_eq!(
        open.pressed,
        Some(2),
        "paint-only palette refresh preserves active press"
    );
}
