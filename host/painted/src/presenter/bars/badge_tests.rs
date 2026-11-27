use super::*;
use crate::canvas::{
    stream, Recorder, CLIP_RRECT, GLYPHS, IMAGE_DEF, IMAGE_RRECT, LAYER, PATH, RRECT,
    SURFACE_IMAGE_BASE,
};
use crate::presenter::access;
use crate::presenter::bars::look::{top_height, BarColors, Button, Inputs, Item, Layout, Top};
use crate::presenter::bars::tests::{barred, tap, APP};
use crate::text::TextEngine;
use exact_kernel::{Direction, ImageRendering};
use std::sync::Arc;
use tiny_skia::Pixmap;

fn solid_bitmap(w: u32, h: u32, rgba: [u8; 4]) -> Arc<Bitmap> {
    let mut pix = Pixmap::new(w, h).unwrap();
    for px in pix.pixels_mut() {
        *px =
            tiny_skia::PremultipliedColorU8::from_rgba(rgba[0], rgba[1], rgba[2], rgba[3]).unwrap();
    }
    let gate = exact_raster::Gate::new();
    let charge = gate
        .session()
        .reserve_allocation((w * h * 4) as u64)
        .unwrap()
        .charge();
    Arc::new(Bitmap::new(pix, (w, h), charge))
}

fn ops_kinds(ops: &[u32]) -> Vec<(usize, u32)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < ops.len() {
        out.push((i, ops[i]));
        i = stream::end(ops, i).expect("well-formed op");
    }
    out
}

fn avatar_app(box_body: &str) -> String {
    APP.replace(
        r#"                    text "Home" role="heading" aria-level=1 flex=1 testId="home-title""#,
        &format!(
            r##"                    button press=share aria-label="Account" testId="avatar"
                      box width=32 height=32 border-radius="50%" background-color="#2563eb"
{box_body}
                    text "Home" role="heading" aria-level=1 flex=1 testId="home-title""##
        ),
    )
}

#[test]
fn leading_initial_badge_paints_authored_circle_with_48dp_target_and_ripple_above() {
    let source =
        avatar_app(r##"                        text "CC" color="#ffffff" font-weight=600"##);
    let (mut p, _) = barred(&source);
    p.sync_bars();

    let (top_plan, _) = p.bar_plans();
    let top_plan = top_plan.expect("top bar");
    assert_eq!(top_plan.leading.len(), 1);
    let btn = &top_plan.leading[0];
    assert_eq!(btn.title, "Account");
    let badge = btn
        .badge
        .as_ref()
        .expect("leading button keeps its badge face");
    assert_eq!(badge.size, BADGE_SIZE);
    assert_eq!(badge.fill, [0x25, 0x63, 0xeb, 255]);
    assert_eq!(badge.ink, [255, 255, 255, 255]);
    assert_eq!(badge.text, "CC");
    assert!(badge.picture.is_none());
    assert!(badge.symbol.is_none());
    // 50% radius on 36 dp badge resolves to 18 dp circle on all four corners.
    assert_eq!(badge.corners, [(18.0, 18.0); 4]);

    // 48x48 dp touch target centered vertically in the 64 dp bar (below status bar edge 52).
    let (top_bar, _) = p.bars();
    let top_bar = top_bar.expect("shown top bar");
    let target = top_bar.targets[0];
    assert_eq!(target.rect.2, 48.0);
    assert_eq!(target.rect.3, 48.0);
    assert_eq!(target.rect.1, 52.0 + (top_height() - 48.0) / 2.0);

    // Pressing the leading badge target starts a ripple painted AFTER the badge face.
    let cx = target.rect.0 + target.rect.2 / 2.0;
    let cy = target.rect.1 + target.rect.3 / 2.0;
    p.bar_event(crate::presenter::bars::TOP, 0, cx, cy, 100.0);
    let ripple = p.bars.ripple.expect("press started ripple");
    let layout = &p.bars.top.as_ref().unwrap().layout;
    let mut engine = p.text.borrow_mut();
    let ops = Recorder::record_surface(1.0, |r| {
        layout.paint(
            r,
            &mut engine,
            Some((ripple.1, Some((ripple.2, 180.0, false)))),
        );
    });
    drop(engine);

    let kinds = ops_kinds(&ops);
    let blue = u32::from_be_bytes([255, 0x25, 0x63, 0xeb]);
    let badge_fill_pos = kinds
        .iter()
        .position(|&(i, k)| k == RRECT && ops[i + 1] == blue)
        .expect("badge blue circle RRECT");
    let badge_text_pos = kinds[badge_fill_pos..]
        .iter()
        .position(|&(_, k)| k == GLYPHS)
        .expect("badge initial GLYPHS")
        + badge_fill_pos;
    // The 40 dp ripple RRECT paints after the badge circle and initials so it is visible on top.
    let ripple_pos = kinds
        .iter()
        .rposition(|&(i, k)| {
            k == RRECT
                && (f32::from_bits(ops[i + 4]) - 40.0).abs() < 0.01
                && (f32::from_bits(ops[i + 5]) - 40.0).abs() < 0.01
        })
        .expect("40 dp state-layer ripple RRECT");
    assert!(badge_fill_pos < badge_text_pos);
    assert!(
        badge_text_pos < ripple_pos,
        "badge ripple must draw over the filled avatar face: kinds={kinds:?}"
    );

    // Lifting inside the target invokes the button's press handler (`said = "shared"`).
    p.bar_event(crate::presenter::bars::TOP, 1, cx, cy, 180.0);
    let kernel = p.host().kernel();
    let said_id = kernel
        .node_by_key(kernel.find_by_test_id("said")[0])
        .unwrap()
        .id;
    let said: String = kernel
        .node(said_id)
        .unwrap()
        .text_runs()
        .iter()
        .map(|r| &*r.text)
        .collect();
    assert_eq!(said, "shared");

    // Accessibility exposes one button with its authored aria-label, not its inner box.
    let (nodes, ..) = p.access_tree();
    let buttons: Vec<_> = nodes
        .iter()
        .filter(|n| n.role == access::role::BUTTON && n.name == "Account")
        .collect();
    assert_eq!(buttons.len(), 1);
}

#[test]
fn leading_symbol_badge_clips_material_glyph_to_authored_circle() {
    let source = avatar_app(r##"                        image "symbol:person" color="#ffffff""##);
    let (mut p, _) = barred(&source);
    p.sync_bars();

    let (top_plan, _) = p.bar_plans();
    let badge = top_plan.unwrap().leading[0]
        .badge
        .as_ref()
        .expect("symbol badge");
    assert_eq!(badge.symbol.as_deref(), Some("person"));
    assert!(badge.picture.is_none());

    let layout = &p.bars.top.as_ref().unwrap().layout;
    let mut engine = p.text.borrow_mut();
    let ops = Recorder::record_surface(1.0, |r| layout.paint(r, &mut engine, None));
    drop(engine);

    let kinds = ops_kinds(&ops);
    let blue = u32::from_be_bytes([255, 0x25, 0x63, 0xeb]);
    let fill_pos = kinds
        .iter()
        .position(|&(i, k)| k == RRECT && ops[i + 1] == blue)
        .expect("badge fill");
    let clip_pos = kinds[fill_pos..]
        .iter()
        .position(|&(_, k)| k == CLIP_RRECT)
        .expect("rounded clip pushed before symbol")
        + fill_pos;
    assert!(
        kinds[clip_pos..].iter().any(|&(_, k)| k == PATH),
        "material symbol PATH drawn inside badge clip"
    );
}

#[test]
fn decoded_badge_picture_paints_object_fit_repaints_on_late_load_and_emits_disjoint_surface_ids() {
    let source = avatar_app(
        r##"                        image "https://example.test/avatar.png" object-fit="cover"
                        text "CC" color="#ffffff""##,
    );
    let (mut p, _) = barred(&source);
    p.sync_bars();

    // Before decode finishes, the initial fallback renders and the bar queues the image size.
    let btn_id = p.bar_plans().0.unwrap().leading[0].node;
    let pic_id = p
        .host()
        .kernel()
        .header_badge(btn_id)
        .unwrap()
        .picture
        .expect("picture node id");
    assert!(
        p.images.bar_sizes.contains_key(&pic_id),
        "lifted header badge image must be tracked for decode"
    );
    assert!(p.bar_plans().0.unwrap().leading[0]
        .badge
        .as_ref()
        .unwrap()
        .picture
        .is_none());
    let rev_before = p.bars_revision();

    // Delivering a decoded rectangular bitmap during an active press triggers an in-place
    // top bar repaint WITHOUT cancelling the held press/ripple.
    let target_rect = p.bars.top.as_ref().unwrap().layout.targets[0].rect;
    let cx = target_rect.0 + target_rect.2 / 2.0;
    let cy = target_rect.1 + target_rect.3 / 2.0;
    p.bar_event(crate::presenter::bars::TOP, 0, cx, cy, 100.0);
    assert_eq!(p.bars.pressed, Some((crate::presenter::bars::TOP, 0)));
    let bmp = solid_bitmap(80, 40, [0x10, 0x80, 0x40, 255]);
    p.images.bitmaps.insert(pic_id, bmp);
    p.sync_bars();
    assert!(
        p.bars_revision() > rev_before,
        "late decoded badge image must bump bars_revision"
    );
    assert_eq!(
        p.bars.pressed,
        Some((crate::presenter::bars::TOP, 0)),
        "late badge decode must preserve an active bar press"
    );
    p.bar_event(crate::presenter::bars::TOP, 1, cx, cy, 120.0);
    assert!(p.bars.pressed.is_none());

    // object-fit: cover on an 80x40 image inside a 36x36 badge yields a 72x36 destination rect.
    let layout = &p.bars.top.as_ref().unwrap().layout;
    let Item::Badge(drawn) = &layout.items[0] else {
        panic!("expected leading badge item");
    };
    let (_, img_rect, _) = drawn.picture.as_ref().expect("decoded badge picture");
    assert!(
        (img_rect.2 - 72.0).abs() < 0.01,
        "cover width={}",
        img_rect.2
    );
    assert!(
        (img_rect.3 - 36.0).abs() < 0.01,
        "cover height={}",
        img_rect.3
    );
    assert!(
        drawn.text.is_none(),
        "fallback initials hide once the picture decodes"
    );

    // Recorder::for_surface emits disjoint positive surface IMAGE_DEF ids >= SURFACE_IMAGE_BASE.
    let mut engine = p.text.borrow_mut();
    let s1 = Recorder::record_surface(2.0, |r| layout.paint(r, &mut engine, None));
    let s2 = Recorder::record_surface(2.0, |r| layout.paint(r, &mut engine, None));
    drop(engine);
    let image_ids = |ops: &[u32]| -> (Vec<u32>, usize) {
        let mut defs = Vec::new();
        let mut draws = 0;
        for (i, k) in ops_kinds(ops) {
            if k == IMAGE_DEF {
                defs.push(ops[i + 1]);
            } else if k == IMAGE_RRECT {
                draws += 1;
            }
        }
        (defs, draws)
    };
    let (ids1, draws1) = image_ids(&s1);
    let (ids2, draws2) = image_ids(&s2);
    assert_eq!(ids1.len(), 1);
    assert_eq!(ids2.len(), 1);
    assert_eq!(draws1, 1, "badge image drawn clipped to rounded rect");
    assert_eq!(draws2, 1);
    assert!(ids1[0] >= SURFACE_IMAGE_BASE);
    assert!(ids2[0] > ids1[0], "each bar frame gets a fresh image id");

    // Navigating away clears the stale bar badge image entry.
    tap(&mut p, "open").unwrap();
    p.sync_bars();
    assert!(
        !p.images.bar_sizes.contains_key(&pic_id),
        "leaving the route drops its bar image size entry"
    );
}

#[test]
fn disabled_large_font_and_rtl_badges_behave_cleanly() {
    let mut engine = TextEngine::from_font_files(
        &[std::path::Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../scripts/fixtures/fonts/assets"
        ))],
        "DejaVu Sans",
    );
    let bmp = solid_bitmap(36, 36, [0x44, 0x55, 0x66, 255]);
    let badge = Badge {
        size: BADGE_SIZE,
        fill: [0x25, 0x63, 0xeb, 255],
        ink: [255, 255, 255, 255],
        corners: [(12.0, 12.0), (4.0, 4.0), (6.0, 6.0), (14.0, 14.0)],
        picture: None,
        symbol: None,
        symbol_direction: Direction::Ltr,
        text: "CC".into(),
        text_direction: Direction::Ltr,
        text_size: 16.0,
        text_weight: 600,
        text_node: 3,
    };
    let device_inks = Default::default();
    let inputs = |font_scale: f32| Inputs {
        device_inks: &device_inks,
        colors: BarColors::default(),
        dark: false,
        font_scale,
        width: 360.0,
        edge: 0.0,
        sides: (0.0, 0.0),
    };

    // 1. Disabled badge paints inside 38% opacity layer and large font scale stays bounded inside 36 dp.
    let top_disabled = Top {
        route: 1,
        heading: Some(2),
        direction: Direction::Ltr,
        title: "Home".into(),
        title_direction: Direction::Ltr,
        subtitle_node: None,
        subtitle: String::new(),
        subtitle_direction: Direction::Ltr,
        up: false,
        leading: vec![Button {
            node: 3,
            title: "Account".into(),
            direction: Direction::Ltr,
            symbol: None,
            symbol_direction: Direction::Ltr,
            enabled: false,
            destructive: false,
            tint: None,
            badge: Some(badge.clone()),
        }],
        trailing: vec![],
        scrolled: false,
    };
    let layout = Layout::top(&mut engine, &top_disabled, &inputs(2.0));
    let ops = Recorder::record_surface(1.0, |r| layout.paint(r, &mut engine, None));
    assert!(
        ops_kinds(&ops)
            .iter()
            .any(|&(i, k)| k == LAYER && ops[i + 1] == 0.38f32.to_bits()),
        "disabled badge paints inside 38% opacity LAYER"
    );
    let Item::Badge(drawn) = &layout.items[0] else {
        panic!("expected leading badge item");
    };
    let (p_text, _, _) = drawn.text.as_ref().expect("initial paragraph");
    assert!(
        p_text.height <= BADGE_SIZE,
        "2x font scale text height {} must fit inside {} dp badge",
        p_text.height,
        BADGE_SIZE
    );

    // 2. RTL moves the leading badge to the right edge while preserving physical
    // corner radii and physical internal object-position alignment.
    let mut pic_badge = badge;
    pic_badge.picture = Some(BadgePicture {
        bitmap: solid_bitmap(18, 36, [0x44, 0x55, 0x66, 255]),
        fit: ObjectFit::Contain,
        position: exact_kernel::object::Position::parse("left center").unwrap(),
        rendering: ImageRendering::Auto,
        env: exact_kernel::Env::default(),
    });
    let _ = bmp;
    let top_rtl = Top {
        direction: Direction::Rtl,
        leading: vec![Button {
            node: 3,
            title: "Account".into(),
            direction: Direction::Rtl,
            symbol: None,
            symbol_direction: Direction::Rtl,
            enabled: true,
            destructive: false,
            tint: None,
            badge: Some(pic_badge),
        }],
        ..top_disabled
    };
    let rtl_layout = Layout::top(&mut engine, &top_rtl, &inputs(1.0));
    let target = rtl_layout.targets[0].rect;
    assert!(
        target.0 > 300.0,
        "RTL leading badge target mirrors to the right side: {:?}",
        target
    );
    let Item::Badge(rtl_drawn) = &rtl_layout.items[0] else {
        panic!("expected RTL badge item");
    };
    assert_eq!(
        rtl_drawn.shape.radii,
        [(12.0, 12.0), (4.0, 4.0), (6.0, 6.0), (14.0, 14.0)],
        "RTL preserves physical CSS border-*-radius corners"
    );
    let (_, img_dst, _) = rtl_drawn.picture.as_ref().expect("RTL picture");
    assert!(
        (img_dst.0 - rtl_drawn.shape.rect.0).abs() < 0.01 && img_dst.2 > 0.0,
        "RTL preserves physical object-position: left inside badge box: box={:?}, img={:?}",
        rtl_drawn.shape.rect,
        img_dst
    );

    // 3. Descendant color-scheme resolves fill/ink in the badge box's own scheme.
    let scheme_app =
        avatar_app(r##"                        text "DK" color="light-dark(#111111, #eeeeee)""##)
            .replace(
                r##"background-color="#2563eb""##,
                r##"color-scheme="dark" background-color="light-dark(#f0f0f0, #101010)""##,
            );
    let (mut p_scheme, _) = barred(&scheme_app);
    p_scheme.sync_bars();
    let scheme_badge = p_scheme.bar_plans().0.unwrap().leading[0]
        .badge
        .as_ref()
        .expect("descendant color-scheme badge");
    assert_eq!(scheme_badge.fill, [0x10, 0x10, 0x10, 255]);
    assert_eq!(scheme_badge.ink, [0xee, 0xee, 0xee, 255]);

    // 4. Existing plain symbol/text buttons produce no Badge items.
    let (mut p_plain, _) = barred(APP);
    p_plain.sync_bars();
    let plain_top = &p_plain.bars.top.as_ref().unwrap().layout;
    assert!(
        plain_top.items.iter().all(|i| !matches!(i, Item::Badge(_))),
        "ordinary header buttons must stay unchanged"
    );
}
