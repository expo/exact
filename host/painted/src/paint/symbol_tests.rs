//! A symbol role in the platform's shape (LLP 1115.001 D14): `back` is
//! Windows' and Android's arrow and `more` their bare dots, where the Linux
//! desktop draws the web's chevron and circled ellipsis.
use crate::presenter::{PainterChoice, Presenter};
use exact_kernel::SymbolSet;
use std::path::PathBuf;

const APP: &str = r##"component App
  view
    row width=300 height=100 background-color="#ffffff" color="#000000"
      image "symbol:back" width=96 height=96 font-size=96 testId="back"
      image "symbol:more" width=96 height=96 font-size=96 testId="more"
      image "symbol:share" width=96 height=96 font-size=96 testId="share"
"##;

/// A presenter whose painter draws `set`'s shapes from its first frame.
fn boot(set: SymbolSet) -> Presenter<()> {
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(APP),
        (),
        (300., 100.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    p.brush.symbols = set;
    p
}

/// Whether the symbol `test_id` inks the point (`u`, `v`) of its 24-unit box.
fn inked(p: &mut Presenter<()>, test_id: &str, (u, v): (f32, f32)) -> bool {
    let k = p.host().kernel();
    let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    let (x, y, w, h) = p.rect_of(id).unwrap();
    let (px, py) = (x + u / 24. * w, y + v / 24. * h);
    let c = p.frame().pixel(px as u32, py as u32).unwrap().demultiply();
    c.red() < 128
}

#[test]
fn windows_and_android_draw_their_own_back_and_more() {
    // The table's: a chevron's point, no shaft; a ring round three dots.
    let mut web = boot(SymbolSet::Web);
    assert!(inked(&mut web, "back", (8., 12.)) && !inked(&mut web, "back", (18., 12.)));
    assert!(inked(&mut web, "more", (12., 3.)) && !inked(&mut web, "more", (6., 12.)));
    // Fluent: an arrow's shaft; three dots in a row with no ring.
    let mut fluent = boot(SymbolSet::Fluent);
    assert!(inked(&mut fluent, "back", (18., 12.)) && inked(&mut fluent, "back", (5., 12.)));
    assert!(inked(&mut fluent, "back", (7.5, 8.5)) && !inked(&mut fluent, "back", (12., 8.5)));
    assert!(inked(&mut fluent, "more", (6., 12.)) && inked(&mut fluent, "more", (18., 12.)));
    assert!(!inked(&mut fluent, "more", (12., 3.)) && !inked(&mut fluent, "more", (12., 6.)));
    // Material: an arrow; three dots in a column.
    let mut material = boot(SymbolSet::Material);
    assert!(inked(&mut material, "back", (18., 12.)) && inked(&mut material, "back", (8., 8.)));
    assert!(inked(&mut material, "more", (12., 6.)) && inked(&mut material, "more", (12., 18.)));
    assert!(!inked(&mut material, "more", (6., 12.)) && !inked(&mut material, "more", (12., 3.)));
    // Material's share: three filled discs, linked, open between the links.
    for disc in [(6., 12.), (18., 5.), (18., 19.)] {
        assert!(inked(&mut material, "share", disc), "{disc:?}");
    }
    assert!(!inked(&mut material, "share", (14., 12.)));
}

#[test]
fn template_clip_preserves_fractional_edge_coverage() {
    let source = r#"component App
  view
    column width=24 height=121 align-items="start" padding-left=0.5 padding-top=0.5 background-color="white" color="black"
      image "symbol:icon||0 0 20 20|M0 0H20V20H0Z|" width=20 height=20 object-fit="contain"
      image "symbol:icon||0 0 20 20|M-2 0H22V20H-2Z|" width=20 height=20 object-fit="contain"
      image "symbol:icon||0 0 40 20|M0 0H40V20H0Z|" width=20 height=20 object-fit="cover"
      image "symbol:icon||0 0 20 20|M0 0H20V20H0Z|" width=20 height=20 object-fit="contain" border-radius=2
      image "symbol:icon||0 0 20 20|M-2 0H20V20H0V10H-2Z|" width=20 height=20 object-fit="contain"
      svg width=20 height=20 viewBox="0 0 20 20" overflow="visible"
        path d="M0 0H20V20H0Z"
"#;
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (24., 121.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.frame();
    let row = |y| {
        (0..24)
            .map(|x| frame.pixel(x, y).unwrap().red())
            .collect::<Vec<_>>()
    };
    let reference = row(110);
    assert!(reference[0] > 0 && reference[0] < 255, "{reference:?}");
    for y in [10, 30, 50, 70, 85, 95] {
        assert_eq!(row(y), reference, "row {y}");
    }
    assert!(frame.pixel(0, 61).unwrap().red() > reference[0]);
    assert_eq!(frame.pixel(10, 40), frame.pixel(10, 60));
}

#[test]
fn template_view_boxes_fit_and_position_before_the_paths_are_painted() {
    let mut source = String::from("component App\n  view\n    column width=160 gap=0 background-color=\"white\" color=\"black\"\n");
    let mut expected = Vec::new();
    for (w, h, contain, none) in [
        (799.1, 184., (0, 22, 159, 57), (70, 38, 89, 41)),
        (184., 799.1, (71, 0, 88, 79), (78, 30, 81, 49)),
    ] {
        for (fit, ink) in [
            ("contain", contain),
            ("fill", contain),
            ("cover", (0, 0, 159, 79)),
            ("none", none),
            ("scale-down", none),
        ] {
            let id = expected.len();
            source.push_str(&format!("      image \"symbol:icon||0 0 {w} {h}|M0 0H{w}V{h}H0Z|\" width=160 height=80 font-size=20 object-fit=\"{fit}\" testId=\"i{id}\"\n"));
            expected.push(ink);
        }
    }
    source.push_str("      image \"symbol:icon||0 0 400 100|M0 0H400V100H0Z|\" width=160 height=80 font-size=20 object-fit=\"contain\" object-position=\"left bottom\" testId=\"position\"\n");
    expected.push((0, 40, 159, 79));
    let (mut p, error) = Presenter::boot_with(
        &crate::test_plan(&source),
        (),
        (160., 880.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let frame = p.frame();
    for (i, want) in expected.into_iter().enumerate() {
        let mut ink = (160, 80, 0, 0);
        for y in 0..80 {
            for x in 0..160 {
                if frame
                    .pixel(x, y + i as u32 * 80)
                    .unwrap()
                    .demultiply()
                    .red()
                    < 128
                {
                    ink = (ink.0.min(x), ink.1.min(y), ink.2.max(x), ink.3.max(y));
                }
            }
        }
        // The two rasterizers can choose adjacent antialiased edge pixels.
        for (actual, expected) in [ink.0, ink.1, ink.2, ink.3]
            .into_iter()
            .zip([want.0, want.1, want.2, want.3])
        {
            assert!(
                actual.abs_diff(expected) <= 1,
                "icon {i}: {ink:?} != {want:?}"
            );
        }
    }
}

#[test]
fn sf_backed_icons_keep_a_square_intrinsic_box() {
    for (w, h) in [(799.1, 184.0), (184.0, 799.1)] {
        let source = format!("component App\n  view\n    row width=100 height=100 align-items=\"start\"\n      image \"symbol:icon|star|0 0 {w} {h}|M0 0H{w}V{h}H0Z|\" font-size=20 testId=\"icon\"\n");
        let (p, error) = Presenter::boot_with(
            &crate::test_plan(&source),
            (),
            (100., 100.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        let kernel = p.host().kernel();
        let node = kernel
            .node_by_key(kernel.find_by_test_id("icon")[0])
            .unwrap();
        let em = crate::paint::look::symbol_size(&node);
        assert_eq!(crate::paint::look::symbol_natural(&node), (em, em));
        assert_eq!((node.frame.width, node.frame.height), (em, em));
    }
}

#[test]
fn directional_symbols_mirror_in_rtl_across_sets_and_clips() {
    let source = r##"component App
  view
    column width=300 height=200 background-color="#ffffff" color="#000000"
      row direction="ltr" height=96
        image "symbol:back" width=48 height=48 font-size=48 testId="back-ltr"
        image "symbol:send" width=48 height=48 font-size=48 testId="send-ltr"
        image "symbol:play" width=48 height=48 font-size=48 testId="play-ltr"
        image "symbol:sf/chevron.backward" width=48 height=48 font-size=48 testId="sf-ltr"
      row direction="rtl" height=96
        image "symbol:back" width=48 height=48 font-size=48 testId="back-rtl"
        image "symbol:send" width=48 height=48 font-size=48 testId="send-rtl"
        image "symbol:play" width=48 height=48 font-size=48 testId="play-rtl"
        image "symbol:sf/chevron.backward" width=48 height=48 font-size=48 testId="sf-rtl"
        image "symbol:back" width=48 height=48 font-size=48 border-radius=8 testId="clipped-back-rtl"
        image "symbol:send" width=48 height=48 font-size=48 border-radius=8 testId="clipped-send-rtl"
"##;
    let boot_dir = |set: SymbolSet| {
        let (mut p, error) = Presenter::boot_with(
            &crate::test_plan(source),
            (),
            (300., 200.),
            1.,
            PathBuf::new(),
            PainterChoice::Cpu,
        )
        .unwrap();
        assert!(error.is_none(), "{error:?}");
        p.brush.symbols = set;
        p
    };
    let bitmap = |p: &mut Presenter<()>, test_id: &str| -> Vec<Vec<bool>> {
        let k = p.host().kernel();
        let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
        let (x, y, w, h) = p.rect_of(id).unwrap();
        let (x0, y0, iw, ih) = (
            x.round() as u32,
            y.round() as u32,
            w.round() as u32,
            h.round() as u32,
        );
        let frame = p.frame();
        (0..ih)
            .map(|dy| {
                (0..iw)
                    .map(|dx| frame.pixel(x0 + dx, y0 + dy).unwrap().demultiply().red() < 128)
                    .collect()
            })
            .collect()
    };
    let ink_count =
        |grid: &[Vec<bool>]| -> usize { grid.iter().flatten().filter(|&&on| on).count() };
    let flipped = |grid: &[Vec<bool>]| -> Vec<Vec<bool>> {
        grid.iter()
            .map(|row| row.iter().rev().copied().collect())
            .collect()
    };
    // Web set: `back` (stroked chevron `M15 5 8 12l7 7`) and `sf/chevron.backward` flip;
    // `send` (`arrow.up`, vertical) and `play` stay fixed; rounded clip keeps positive stroke width.
    let mut web = boot_dir(SymbolSet::Web);
    assert!(inked(&mut web, "back-ltr", (8., 12.)) && !inked(&mut web, "back-ltr", (16., 12.)));
    assert!(inked(&mut web, "back-rtl", (16., 12.)) && !inked(&mut web, "back-rtl", (8., 12.)));
    assert!(inked(&mut web, "sf-rtl", (16., 12.)) && !inked(&mut web, "sf-rtl", (8., 12.)));
    assert!(
        inked(&mut web, "clipped-back-rtl", (16., 12.))
            && !inked(&mut web, "clipped-back-rtl", (8., 12.)),
        "clipped stroked RTL symbol must render with positive stroke width on the mirrored side"
    );
    let web_play_ltr = bitmap(&mut web, "play-ltr");
    let web_play_rtl = bitmap(&mut web, "play-rtl");
    assert!(ink_count(&web_play_ltr) > 0, "web play must ink pixels");
    assert_eq!(
        web_play_rtl, web_play_ltr,
        "web play must not mirror in RTL"
    );
    let web_send_ltr = bitmap(&mut web, "send-ltr");
    let web_send_rtl = bitmap(&mut web, "send-rtl");
    assert!(ink_count(&web_send_ltr) > 0, "web send must ink pixels");
    assert_eq!(
        web_send_rtl, web_send_ltr,
        "web send (arrow.up) must not mirror in RTL"
    );
    // Material set: `back` (arrow_back) and `send` (paper plane) both flip; `play` stays fixed.
    let mut material = boot_dir(SymbolSet::Material);
    assert!(
        inked(&mut material, "back-ltr", (8., 8.)) && !inked(&mut material, "back-ltr", (16., 8.))
    );
    assert!(
        inked(&mut material, "back-rtl", (16., 8.)) && !inked(&mut material, "back-rtl", (8., 8.))
    );
    let mat_send_ltr = bitmap(&mut material, "send-ltr");
    let mat_send_rtl = bitmap(&mut material, "send-rtl");
    assert!(
        ink_count(&mat_send_ltr) > 0,
        "material send must ink pixels"
    );
    assert_ne!(
        mat_send_rtl, mat_send_ltr,
        "material send must mirror in RTL"
    );
    assert_eq!(
        mat_send_rtl,
        flipped(&mat_send_ltr),
        "material send RTL must be horizontal mirror of LTR"
    );
    assert!(
        inked(&mut material, "clipped-send-rtl", (20., 6.))
            && !inked(&mut material, "clipped-send-rtl", (4., 6.))
            && !inked(&mut material, "clipped-send-rtl", (16., 9.)),
        "clipped filled Material RTL symbol flips while preserving its inner cutout"
    );
    let mat_play_ltr = bitmap(&mut material, "play-ltr");
    let mat_play_rtl = bitmap(&mut material, "play-rtl");
    assert!(
        ink_count(&mat_play_ltr) > 0,
        "material play must ink pixels"
    );
    assert_eq!(
        mat_play_rtl, mat_play_ltr,
        "material play must not mirror in RTL"
    );
}
