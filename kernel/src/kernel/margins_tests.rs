//! Margins and the layout regions per box (LLP 1123 D8, D10): what a box reads of its margins
//! and its readable content, by UIKit's rule as plain UIKit apps give them on the iPhone 18 Pro
//! (iOS 27.0) and the iPhone Duo (iOS 27.1).
use super::*;

#[test]
fn a_host_that_reports_no_margins_gives_each_region_the_safe_area() {
    // 3, at the top of 2's content root under a 47-point band, reads each region's top as its
    // name lowers it; with no margins reported, each is the box's safe area (D10, "Other
    // hosts")
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "container")]));
    for (row, family) in [
        (StyleId::PaddingTop, "layout-margins"),
        (StyleId::PaddingRight, "readable-content"),
        (StyleId::PaddingBottom, "readable-content-guide"),
        (StyleId::PaddingLeft, "layout-margins-corners"),
    ] {
        let read = format!("var(--exact-{family}-inset-top)");
        ops.push(style(3, vec![], &[(row, read.as_str())]));
    }
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    let padding = k.resolved_padding(k.arena.key(slot(&k, 3))).unwrap();
    assert_eq!(padding, (47.0, 47.0, 47.0, 47.0));
}

/// The owner 2 as the iPhone 18 Pro's controller in portrait (iOS 27.0), 402 by 874 under a
/// 62-point band and a 34-point home indicator, its root filling the allocation and preserving
/// the controller root view's margins, as a view filling a controller's view lies in it, with
/// the margins UIKit reported there: the root view's 62, 16, 34, 16 from the allocation's
/// edges, a plain view's 8, the system minimum's 16 on the sides.
fn pro(ops: Vec<Op>) -> Kernel {
    let mut all = vec![style(
        2,
        vec![],
        &[
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::PositionType, "relative"),
            (StyleId::Height, "100%"),
            (StyleId::LayoutMarginsPreserved, "true"),
        ],
    )];
    all.extend(ops);
    all.push(children(1, &[2]));
    let mut k = owner(402.0, 874.0, all);
    k.set_owner_band(2, Some([62.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    let reported = crate::kernel::OwnerMargins {
        root: [62.0, 16.0, 34.0, 16.0],
        plain: [8.0; 4],
        minimum: [0.0, 16.0, 0.0, 16.0],
        readable_width: 672.0,
    };
    k.set_owner_margins(2, Some(reported)).unwrap();
    lay_out(&mut k);
    k
}

/// The box `id` placed at `(x, y)`, `w` by `h`, in its parent, reading `family`'s four edges
/// as its padding, `border-box` so that its frame stays put, with `rows`.
fn probed(
    id: ViewId,
    (x, y, w, h): (&'static str, &'static str, &'static str, &'static str),
    family: &str,
    rows: &[(StyleId, &'static str)],
) -> Op {
    let read = |edge: &str| {
        Box::leak(format!("var(--exact-{family}-inset-{edge})").into_boxed_str()) as &'static str
    };
    let mut all = vec![
        (StyleId::PositionType, "absolute"),
        (StyleId::Left, x),
        (StyleId::Top, y),
        (StyleId::Width, w),
        (StyleId::Height, h),
        (StyleId::BoxSizing, "border-box"),
        (StyleId::PaddingTop, read("top")),
        (StyleId::PaddingRight, read("right")),
        (StyleId::PaddingBottom, read("bottom")),
        (StyleId::PaddingLeft, read("left")),
    ];
    all.extend_from_slice(rows);
    style(id, vec![], &all)
}

/// What `id` reads of its family, top right bottom left.
fn read(k: &Kernel, id: ViewId) -> [f32; 4] {
    let (left, top, right, bottom) = k.resolved_padding(k.arena.key(slot(k, id))).unwrap();
    [top, right, bottom, left]
}

#[test]
fn a_plain_box_adds_its_safe_area_to_its_own_margins() {
    // On the 18 Pro a view filling the screen has 70 and 42 above and below, 8 at the sides; one
    // whose margins ignore the safe area has 8; one away from the edges 8; one along the bottom
    // 42 below (`design/probes/margins`, iOS 27.0)
    let mut ops = create(3..=6);
    ops.push(probed(3, ("0", "0", "402", "874"), "layout-margins", &[]));
    ops.push(probed(
        4,
        ("0", "0", "402", "874"),
        "layout-margins",
        &[(StyleId::LayoutMarginsFromSafeArea, "false")],
    ));
    ops.push(probed(
        5,
        ("40", "300", "322", "120"),
        "layout-margins",
        &[],
    ));
    ops.push(probed(6, ("0", "814", "402", "60"), "layout-margins", &[]));
    ops.push(children(2, &[3, 4, 5, 6]));
    let k = pro(ops);
    assert_eq!(read(&k, 3), [70.0, 8.0, 42.0, 8.0]);
    assert_eq!(read(&k, 4), [8.0; 4]);
    assert_eq!(read(&k, 5), [8.0; 4]);
    assert_eq!(read(&k, 6), [8.0, 8.0, 42.0, 8.0]);
}

#[test]
fn a_preserving_box_projects_its_superviews_margins() {
    // 4 points in from the root, whose sides are the controller view's 16, a preserving box's
    // sides are 12; 30 in, its own 8 stand; its own 24 stand over 12; at the bottom its own 42
    // stand over the root's 34; and a preserving box 2 right of and 10 below a preserving parent
    // with 24-point margins projects that parent's, 22 and 14
    let mut ops = create(3..=8);
    let keep = (StyleId::LayoutMarginsPreserved, "true");
    let wide = [
        keep,
        (StyleId::LayoutMarginTop, "24"),
        (StyleId::LayoutMarginRight, "24"),
        (StyleId::LayoutMarginBottom, "24"),
        (StyleId::LayoutMarginLeft, "24"),
    ];
    ops.push(probed(
        3,
        ("4", "520", "394", "60"),
        "layout-margins",
        &[keep],
    ));
    ops.push(probed(
        4,
        ("30", "590", "342", "60"),
        "layout-margins",
        &[keep],
    ));
    ops.push(probed(
        5,
        ("4", "660", "394", "60"),
        "layout-margins",
        &wide,
    ));
    ops.push(probed(
        6,
        ("4", "824", "394", "50"),
        "layout-margins",
        &[keep],
    ));
    ops.push(probed(
        7,
        ("4", "730", "394", "70"),
        "layout-margins",
        &wide,
    ));
    ops.push(probed(
        8,
        ("2", "10", "390", "40"),
        "layout-margins",
        &[keep],
    ));
    ops.extend([children(2, &[3, 4, 5, 6, 7]), children(7, &[8])]);
    let k = pro(ops);
    assert_eq!(read(&k, 3), [8.0, 12.0, 8.0, 12.0]);
    assert_eq!(read(&k, 4), [8.0; 4]);
    assert_eq!(read(&k, 5), [24.0; 4]);
    assert_eq!(read(&k, 6), [8.0, 12.0, 42.0, 12.0]);
    assert_eq!(read(&k, 8), [14.0, 22.0, 8.0, 22.0]);
}

#[test]
fn readable_content_is_the_boxs_width_less_its_own_margins_centred() {
    // Directional margins of 4, leading 30, 6 and trailing 5: the margins, and the readable
    // region raising the smaller side to the mean, 17.5; mirrored right to left. The readable
    // guide is the margins where the readable width does not limit
    let mut ops = create(3..=4);
    let directional = [
        (StyleId::LayoutMarginTop, "4"),
        (StyleId::LayoutMarginLeading, "30"),
        (StyleId::LayoutMarginBottom, "6"),
        (StyleId::LayoutMarginTrailing, "5"),
    ];
    ops.push(probed(
        3,
        ("40", "140", "322", "50"),
        "readable-content",
        &directional,
    ));
    let mut rtl = directional.to_vec();
    rtl.push((StyleId::Direction, "rtl"));
    ops.push(probed(
        4,
        ("40", "200", "322", "50"),
        "readable-content",
        &rtl,
    ));
    ops.push(children(2, &[3, 4]));
    let k = pro(ops);
    assert_eq!(read(&k, 3), [4.0, 17.5, 6.0, 30.0]);
    assert_eq!(read(&k, 4), [4.0, 30.0, 6.0, 17.5]);
}

#[test]
fn the_duo_gives_a_plain_view_no_margins_of_its_own() {
    // The open iPhone Duo (iOS 27.1), 951 by 669 with an 84-point safe area on the right and the
    // 34-point home indicator, reports a plain view's margins as none and the root view's as
    // 20 on the left and its safe area elsewhere: a view filling the screen has its safe area
    // for margins and a readable region 42 from the left, the mean of 0 and 84; a preserving
    // view 4 in from the root has 16 on the left and a readable region 40 from it, the mean of
    // its own 0 and 80 (`design/probes/margins`)
    let mut ops = vec![style(
        2,
        vec![],
        &[
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::PositionType, "relative"),
            (StyleId::Height, "100%"),
            (StyleId::LayoutMarginsPreserved, "true"),
        ],
    )];
    ops.extend(create(3..=7));
    ops.push(probed(3, ("0", "0", "951", "669"), "layout-margins", &[]));
    // A preserving box with its own 24 that runs 51 past the bottom: its bottom edge takes the
    // root's 34 there and no more, and its own 24 over its 34 of safe area stand. It reads its
    // margins in a corner's radius, which moves no frame, so its measurement is what it reads
    let wide = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Left, "4"),
        (StyleId::Top, "660"),
        (StyleId::Width, "943"),
        (StyleId::Height, "60"),
        (
            StyleId::BorderRadiusTopLeft,
            "var(--exact-layout-margins-inset-bottom)",
        ),
        (StyleId::LayoutMarginsPreserved, "true"),
        (StyleId::LayoutMarginTop, "24"),
        (StyleId::LayoutMarginRight, "24"),
        (StyleId::LayoutMarginBottom, "24"),
        (StyleId::LayoutMarginLeft, "24"),
    ];
    ops.push(style(7, vec![], &wide));
    ops.push(probed(4, ("0", "0", "951", "669"), "readable-content", &[]));
    let keep = [(StyleId::LayoutMarginsPreserved, "true")];
    ops.push(probed(
        5,
        ("4", "520", "943", "60"),
        "layout-margins",
        &keep,
    ));
    ops.push(probed(
        6,
        ("4", "520", "943", "60"),
        "readable-content",
        &keep,
    ));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5, 6, 7])]);
    let mut k = owner(951.0, 669.0, ops);
    k.set_owner_band(2, Some([0.0, 84.0, 34.0, 0.0]), &[])
        .unwrap();
    let duo = crate::kernel::OwnerMargins {
        root: [0.0, 84.0, 34.0, 20.0],
        plain: [0.0; 4],
        minimum: [0.0, 0.0, 0.0, 20.0],
        readable_width: 1000.0,
    };
    k.set_owner_margins(2, Some(duo)).unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 3), [0.0, 84.0, 34.0, 0.0]);
    assert_eq!(read(&k, 4), [0.0, 84.0, 34.0, 42.0]);
    assert_eq!(read(&k, 5), [0.0, 80.0, 0.0, 16.0]);
    assert_eq!(read(&k, 6), [0.0, 80.0, 0.0, 40.0]);
    let measured = k
        .observation_of(7, crate::style::calc::Family::LayoutMargins)
        .unwrap();
    assert_eq!(measured, [Some(24.0), Some(104.0), Some(58.0), Some(24.0)]);
}

#[test]
fn a_report_or_a_restyle_measures_the_readers_again() {
    // 3 fills the screen: 70 and 8 on the 18 Pro; a host that reports 0 for a plain view gives
    // 62; its own 20 give 82; the system minimum is a raw reading of the controller's
    let mut ops = create(3..=4);
    ops.push(probed(3, ("0", "0", "402", "874"), "layout-margins", &[]));
    ops.push(probed(
        4,
        ("0", "0", "402", "874"),
        "system-minimum-layout-margins",
        &[],
    ));
    ops.push(children(2, &[3, 4]));
    let mut k = pro(ops);
    assert_eq!(read(&k, 3)[0], 70.0);
    assert_eq!(read(&k, 4), [0.0, 16.0, 0.0, 16.0]);
    let none = crate::kernel::OwnerMargins {
        root: [62.0, 16.0, 34.0, 16.0],
        plain: [0.0; 4],
        minimum: [0.0, 16.0, 0.0, 16.0],
        readable_width: 672.0,
    };
    assert!(k.set_owner_margins(2, Some(none)).unwrap());
    lay_out(&mut k);
    assert_eq!(read(&k, 3), [62.0, 0.0, 34.0, 0.0]);
    k.apply(
        0,
        9,
        &[style(3, vec![], &[(StyleId::LayoutMarginTop, "20")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 3)[0], 82.0);
}

#[test]
fn the_readable_guide_centres_the_readable_width_in_the_margins() {
    // A readable width of 300 in a view 402 wide with 8-point side margins leaves 43 on each
    // side of the margins rectangle
    let mut ops = create(3..=3);
    ops.push(probed(
        3,
        ("0", "300", "402", "100"),
        "readable-content-guide",
        &[],
    ));
    ops.push(children(2, &[3]));
    let mut k = pro(ops);
    let narrow = crate::kernel::OwnerMargins {
        root: [62.0, 16.0, 34.0, 16.0],
        plain: [8.0; 4],
        minimum: [0.0, 16.0, 0.0, 16.0],
        readable_width: 300.0,
    };
    k.set_owner_margins(2, Some(narrow)).unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 3), [8.0, 51.0, 8.0, 51.0]);
}

#[test]
fn what_moves_a_preserving_or_leading_margin_measures_it_again() {
    // 8, 2 in from the left of 7, which has 24-point margins, reads its own 8 on its right while
    // 7 is 394 wide; 7 narrowed to 110 brings its right edge 8 inside 7's, which projects 16
    // with no change to 8's frame. 3's leading 30 lies on its left, then on its right once the
    // document turns right to left
    let mut ops = create(3..=8);
    let wide = [
        (StyleId::LayoutMarginTop, "24"),
        (StyleId::LayoutMarginRight, "24"),
        (StyleId::LayoutMarginBottom, "24"),
        (StyleId::LayoutMarginLeft, "24"),
    ];
    ops.push(probed(
        7,
        ("4", "600", "394", "70"),
        "layout-margins",
        &wide,
    ));
    ops.push(probed(
        8,
        ("2", "10", "100", "40"),
        "layout-margins",
        &[(StyleId::LayoutMarginsPreserved, "true")],
    ));
    let leading = [(StyleId::LayoutMarginLeading, "30")];
    ops.push(probed(
        3,
        ("40", "140", "322", "50"),
        "layout-margins",
        &leading,
    ));
    ops.extend([children(2, &[3, 7]), children(7, &[8])]);
    let mut k = pro(ops);
    assert_eq!(read(&k, 8)[1], 8.0);
    assert_eq!((read(&k, 3)[1], read(&k, 3)[3]), (8.0, 30.0));
    k.apply(0, 9, &[style(7, vec![], &[(StyleId::Width, "110")])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 8)[1], 16.0);
    k.apply_document(1, 10, &[], Some(("ar", crate::Direction::Rtl)))
        .unwrap();
    lay_out(&mut k);
    assert_eq!((read(&k, 3)[1], read(&k, 3)[3]), (30.0, 8.0));
}

#[test]
fn a_margin_that_reads_a_custom_property_follows_it() {
    // 3 declares `--gutter` and 4, inside it, writes its top margin as `var(--gutter)`: 8, then
    // 20 when 3 declares it so
    let mut ops = create(3..=4);
    ops.push(style(
        3,
        vec![],
        &[(StyleId::SafeAreaProperties, "--gutter: 8px")],
    ));
    ops.push(probed(
        4,
        ("0", "300", "402", "100"),
        "layout-margins",
        &[(StyleId::LayoutMarginTop, "var(--gutter)")],
    ));
    ops.extend([children(2, &[3]), children(3, &[4])]);
    let mut k = pro(ops);
    assert_eq!(read(&k, 4)[0], 8.0);
    k.apply(
        0,
        9,
        &[style(
            3,
            vec![],
            &[(StyleId::SafeAreaProperties, "--gutter: 20px")],
        )],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 4)[0], 20.0);
}

#[test]
fn a_freed_owners_margins_are_forgotten() {
    // The owner 2 is destroyed and a new one takes its slot: its boxes read their safe area until
    // its host reports margins, as an owner whose host reports none does
    let mut ops = create(3..=3);
    ops.push(probed(3, ("0", "0", "402", "874"), "layout-margins", &[]));
    ops.push(children(2, &[3]));
    let mut k = pro(ops);
    assert_eq!(read(&k, 3)[0], 70.0);
    let slot = k.arena.slot_of(2).unwrap();
    k.apply(0, 9, &[children(1, &[])]).unwrap();
    k.apply(
        0,
        10,
        &[Op::DestroyView { id: 3 }, Op::DestroyView { id: 2 }],
    )
    .unwrap();
    let mut ops = create(12..=13);
    ops.push(style(
        12,
        vec![],
        &[
            (StyleId::SafeAreaIgnore, "container"),
            (StyleId::PositionType, "relative"),
            (StyleId::Height, "100%"),
        ],
    ));
    ops.push(probed(13, ("0", "0", "402", "874"), "layout-margins", &[]));
    ops.extend([children(1, &[12]), children(12, &[13])]);
    k.apply(0, 11, &ops).unwrap();
    assert_eq!(k.arena.slot_of(12), Some(slot), "the freed slot is reused");
    k.set_host_block(12, Some(block(BlockAxis::Is(402.0), BlockAxis::Is(874.0))))
        .unwrap();
    k.set_owner_band(12, Some([62.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 13), [62.0, 0.0, 34.0, 0.0]);
}

#[test]
fn a_box_placed_by_its_own_margins_settles() {
    // An in-flow preserving box whose left margin is its own left margins reading, in a root
    // whose sides are 16: at 0 it reads 16, at 16 its own 8, and it settles at 8, where the
    // root's 16 projects 8
    let mut ops = create(3..=3);
    let rows = [
        (StyleId::LayoutMarginsPreserved, "true"),
        (
            StyleId::MarginLeft,
            "var(--exact-layout-margins-inset-left)",
        ),
        (StyleId::Height, "20"),
        (StyleId::Width, "100"),
    ];
    ops.push(style(3, vec![], &rows));
    ops.push(children(2, &[3]));
    let k = pro(ops);
    assert_eq!(frame(&k, 3).0, 8.0);
    assert!(k.safe_area_unsettled().is_empty());
}

#[test]
fn a_margin_that_reads_a_capture_the_solve_moves_settles_with_it() {
    // 8 preserves 7's 24-point margins, 2 in from 7's left, and captures its left margin, 22, as
    // `--side`; 9 inside it writes its left margin as `var(--side)`: it reads 22 in the same
    // layout, settled
    let mut ops = create(3..=9);
    let wide = [
        (StyleId::LayoutMarginTop, "24"),
        (StyleId::LayoutMarginRight, "24"),
        (StyleId::LayoutMarginBottom, "24"),
        (StyleId::LayoutMarginLeft, "24"),
    ];
    ops.push(probed(
        7,
        ("4", "600", "394", "70"),
        "layout-margins",
        &wide,
    ));
    let captures = [
        (StyleId::LayoutMarginsPreserved, "true"),
        (
            StyleId::SafeAreaProperties,
            "--side: var(--exact-layout-margins-inset-left)",
        ),
    ];
    ops.push(probed(
        8,
        ("2", "10", "100", "40"),
        "layout-margins",
        &captures,
    ));
    ops.push(probed(
        9,
        ("0", "0", "50", "20"),
        "layout-margins",
        &[(StyleId::LayoutMarginLeft, "var(--side)")],
    ));
    ops.extend([children(2, &[7]), children(7, &[8]), children(8, &[9])]);
    let k = pro(ops);
    assert_eq!(read(&k, 8)[3], 22.0);
    assert_eq!(read(&k, 9)[3], 22.0);
    assert!(k.safe_area_unsettled().is_empty());
}
