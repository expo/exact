//! A root's bar on UIKit's bar layout region (LLP 1123 D12, "Region").
use super::*;
use crate::kernel::BarRegion;

/// The owner root 2, a 951 × 669 column under the open iPhone Duo's 34-point bottom band, with
/// `rows`: 4 fills it in flow, then `bars` (3, then 5 on), then 9, pinned to its bottom, which pads
/// by what it observes there. Its bottom bar region, reported unless `region` is `None`, lies 24
/// points up in the band, 16 and 8.67 points in from the sides (measured on the Duo).
fn duo(
    rows: &[(StyleId, &'static str)],
    bars: &[&[(StyleId, &'static str)]],
    region: Option<f32>,
) -> Kernel {
    let mut all = vec![
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::Height, "100%"),
        (StyleId::Display, "flex"),
        (StyleId::FlexDirection, "column"),
    ];
    all.extend_from_slice(rows);
    let mut ops = create(3..=9);
    ops.push(style(2, vec![], &all));
    let ids: Vec<u32> = (0..bars.len() as u32)
        .map(|i| if i == 0 { 3 } else { 4 + i })
        .collect();
    for (id, rows) in ids.iter().zip(bars) {
        ops.push(style(*id, vec![], rows));
    }
    ops.push(style(4, vec![], &[(StyleId::FlexGrow, "1")]));
    let pinned = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Bottom, "0"),
        (StyleId::Height, "10"),
    ];
    ops.push(style(
        9,
        vec![(StyleId::PaddingLeft, observed(Edge::Bottom, None))],
        &pinned,
    ));
    let kids = [vec![4], ids, vec![9]].concat();
    ops.extend([children(1, &[2]), children(2, &kids)]);
    let mut k = owner(951.0, 669.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    let regions = [
        None,
        None,
        region.map(|distance| BarRegion {
            distance,
            sides: [16.0, 8.67],
        }),
        None,
    ];
    k.set_owner_bar_regions(2, regions).unwrap();
    lay_out(&mut k);
    k
}

const REGION: &[(StyleId, &str)] = &[
    (StyleId::SafeAreaBar, "bottom region"),
    (StyleId::Height, "50"),
];

/// `id`'s top and height, and its left and width to a hundredth of a point.
fn placed(k: &Kernel, id: u32) -> (f32, f32, f32, f32) {
    let (x, y, w, h) = frame(k, id);
    let r = |v: f32| (v * 100.0).round() / 100.0;
    (r(x), y, r(w), h)
}

#[test]
fn a_region_bar_lies_on_the_region_and_reserves_its_reach_past_the_band() {
    // The content root ends at 635; the bar lies on the region from 595 to 645, 10 points past
    // it, within the region's sides, and reserves the 40 points it reaches inside: 4 ends at 595
    // and 9 observes 40
    let k = duo(&[], &[REGION], Some(24.0));
    assert_eq!(placed(&k, 3), (16.0, 595.0, 926.33, 50.0));
    assert_eq!(frame(&k, 4).1 + frame(&k, 4).3, 595.0);
    assert_eq!(k.observation(9).unwrap()[2], Some(40.0));
}

#[test]
fn a_region_bar_with_no_region_lies_as_any_bar() {
    // Before iOS 27.1, and on other hosts: the slot on the content root's edge, its depth reserved
    let k = duo(&[], &[REGION], None);
    assert_eq!(placed(&k, 3), (0.0, 585.0, 951.0, 50.0));
    assert_eq!(k.observation(9).unwrap()[2], Some(50.0));
}

#[test]
fn a_region_bar_of_a_root_that_takes_its_edge_lies_on_the_same_region() {
    // The root reaches the allocation's bottom, its content under the band; the bar stays on
    // the region and reserves its 40-point reach past the band, so 4 ends at 629 and 9, at the
    // root's foot, observes the band and the reach
    let k = duo(
        &[(StyleId::SafeAreaIgnore, "container bottom")],
        &[REGION],
        Some(24.0),
    );
    assert_eq!(placed(&k, 3), (16.0, 595.0, 926.33, 50.0));
    assert_eq!(frame(&k, 4).1 + frame(&k, 4).3, 629.0);
    assert_eq!(k.observation(9).unwrap()[2], Some(74.0));
}

#[test]
fn a_region_bar_after_another_bar_on_its_edge_lies_past_it() {
    // 3, a 30-point bar, takes the content root's foot, 605 to 635; the region bar, 5, lies
    // past it, 555 to 605, and reserves its whole depth
    let plain: &[(StyleId, &str)] = &[(StyleId::SafeAreaBar, "bottom"), (StyleId::Height, "30")];
    let k = duo(&[], &[plain, REGION], Some(24.0));
    assert_eq!(placed(&k, 3).1, 605.0);
    assert_eq!(placed(&k, 5), (16.0, 555.0, 926.33, 50.0));
    assert_eq!(k.observation(9).unwrap()[2], Some(80.0));
}

#[test]
fn a_region_bar_rests_on_a_keyboard_the_root_clears() {
    // Under `resizes-content` the content root ends at the keyboard's top at 400, past the
    // region, so the bar rests there, 350 to 400, the region's sides kept
    let mut k = duo(&[], &[REGION], Some(24.0));
    k.set_keyboard_policy(Some(crate::kernel::KeyboardPolicy::ResizesContent));
    k.set_owner_keyboard(2, keyboard(400.0)).unwrap();
    lay_out(&mut k);
    assert_eq!(placed(&k, 3), (16.0, 350.0, 926.33, 50.0));
}

#[test]
fn a_region_change_owes_a_layout_and_a_bad_region_is_refused() {
    let mut k = duo(&[], &[REGION], Some(24.0));
    let at = |d: f32| {
        [
            None,
            None,
            Some(BarRegion {
                distance: d,
                sides: [16.0, 8.67],
            }),
            None,
        ]
    };
    assert!(
        !k.set_owner_bar_regions(2, at(24.0)).unwrap(),
        "the same region"
    );
    assert!(k.set_owner_bar_regions(2, at(20.0)).unwrap());
    lay_out(&mut k);
    assert_eq!(placed(&k, 3).1, 599.0);
    assert!(k.set_owner_bar_regions(2, at(-1.0)).is_err());
    assert!(k.set_owner_bar_regions(2, at(f32::NAN)).is_err());
    assert_eq!(placed(&k, 3).1, 599.0, "a refused region changes nothing");
}
