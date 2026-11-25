//! Container units (LLP 1123 D16): a length in `cqw`, `cqh`, `cqi`, `cqb`, `cqmin` or `cqmax`
//! resolves against the box's nearest owner's content root on each axis its host fixes, and
//! against the small viewport where none does.
use super::*;
use crate::kernel::{Keyboard, KeyboardPolicy};
use crate::Dimension;

/// `id`'s width and height.
fn size(k: &Kernel, id: ViewId) -> (f32, f32) {
    let (_, _, w, h) = frame(k, id);
    (w, h)
}

#[test]
fn a_container_unit_is_of_the_owners_content_root() {
    // 2's host fixes its 390 by 844 block and clears a 47-point and a 34-point band, so its
    // content root is 390 by 763: 3 is half its width and a tenth of its height, 4 the smaller
    // and larger of the two, 5 a calculation of one
    let mut ops = create(3..=5);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "none")]));
    ops.push(style(
        3,
        vec![],
        &[(StyleId::Width, "50cqw"), (StyleId::Height, "10cqh")],
    ));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::Width, "10cqmin"), (StyleId::Height, "1cqmax")],
    ));
    ops.push(style(
        5,
        vec![],
        &[
            (StyleId::Width, "calc(50cqi - 10px)"),
            (StyleId::Height, "1cqb"),
        ],
    ));
    ops.extend([children(1, &[2]), children(2, &[3, 4, 5])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 3), (195.0, 76.3));
    assert_eq!(size(&k, 4), (39.0, 7.63));
    assert_eq!(size(&k, 5), (185.0, 7.63));
    // A band that moves what the host clears, and a block of another size, resize them
    k.set_owner_band(2, Some([97.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 3), (195.0, 71.3));
    k.set_host_block(2, Some(block(BlockAxis::Is(300.0), BlockAxis::Is(844.0))))
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 3).0, 150.0);
}

#[test]
fn a_percentage_beside_a_container_unit_reads_the_container() {
    // In 2's 300 by 600 content root, `100% - 10cqw` is 300 less 30, not less a tenth of the
    // 390-point viewport
    let mut ops = create(3..=4);
    ops.push(style(3, vec![], &[(StyleId::Width, "calc(100% - 10cqw)")]));
    // A text whose `env()` has a fallback holds the unit as written, an escape included
    let escaped = "calc(100% - env(safe-area-inset-top, 0px) - 10c\\71 w)";
    ops.push(style(4, vec![], &[(StyleId::Width, escaped)]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(300.0, 600.0, ops);
    lay_out(&mut k);
    assert_eq!((size(&k, 3).0, size(&k, 4).0), (270.0, 270.0));
}

#[test]
fn an_object_position_reads_its_owners_content_root() {
    let mut ops = create(3..=4);
    for (id, value) in [(3, "10cqw top"), (4, "calc(10cqw + 5px) top")] {
        ops.push(style(id, vec![], &[(StyleId::ObjectPosition, value)]));
    }
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(300.0, 600.0, ops);
    let offset = |k: &Kernel, id| {
        let env = k.env_of(id);
        k.node(id).unwrap().style.object_position.resolve(&env).x
    };
    lay_out(&mut k);
    assert_eq!(offset(&k, 3), Dimension::Points(30.0));
    assert_eq!(offset(&k, 4), Dimension::Points(35.0));

    k.set_host_block(2, Some(block(BlockAxis::Is(200.0), BlockAxis::Is(600.0))))
        .unwrap();
    lay_out(&mut k);
    assert_eq!(offset(&k, 3), Dimension::Points(20.0));
    assert_eq!(offset(&k, 4), Dimension::Points(25.0));
}

#[test]
fn a_percentage_beside_a_container_unit_resolves_where_no_container_is_known() {
    // An environment with no container reads the small viewport: `100% - 10cqw` is the whole
    // less a tenth of 390, and resolving it ends
    let env = crate::style::Env {
        viewport_width: 390.0,
        viewport_height: 844.0,
        ..Default::default()
    };
    let width = crate::StyleValue::Text("calc(100% - 10cqw)".into())
        .dimension(StyleId::Width, true)
        .unwrap();
    assert_eq!(
        width.resolve(&env),
        crate::style::Dimension::Calc(100.0, -39.0)
    );
}

#[test]
fn a_box_mounted_or_moved_later_reads_its_new_container() {
    // 6, styled before it has a parent and mounted in 2's 300-wide content root after a
    // layout, is half of it; moved into 7's 200-wide one it is half of that
    let mut ops = create(7..=7);
    ops.push(style(7, vec![], &[(StyleId::SafeAreaIgnore, "container")]));
    ops.extend([children(1, &[2, 7])]);
    let mut k = owner(300.0, 600.0, ops);
    k.set_host_block(7, Some(block(BlockAxis::Is(200.0), BlockAxis::Is(400.0))))
        .unwrap();
    lay_out(&mut k);
    let mut mount = create(6..=6);
    mount.push(style(6, vec![], &[(StyleId::Width, "50cqw")]));
    mount.push(children(2, &[6]));
    k.apply(0, 2, &mount).unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 6).0, 150.0);
    k.apply(0, 3, &[children(2, &[]), children(7, &[6])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 6).0, 100.0);
}

#[test]
fn what_the_host_clears_resizes_the_container() {
    // 2's 844-point block less its 34-point bottom band leaves an 810-point content root, and a
    // commit while the band is 134 derives 3 against 710, which the next layout, under 34
    // again, derives anew; the keyboard under `resizes-content` ends it at the keyboard's top,
    // 516 points down; a 20-point reservation on the root and its ignore of the container
    // region each move what the host clears too, and 3's tenth of the height follows each
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &[(StyleId::SafeAreaIgnore, "none")]));
    ops.push(style(3, vec![], &[(StyleId::Height, "10cqh")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    k.set_keyboard_policy(Some(KeyboardPolicy::ResizesContent));
    lay_out(&mut k);
    assert_eq!(size(&k, 3).1, 81.0);
    let band = |k: &mut Kernel, bottom: f32| {
        k.set_owner_band(2, Some([0.0, 0.0, bottom, 0.0]), &[])
            .unwrap();
    };
    band(&mut k, 134.0);
    k.apply(0, 2, &[style(3, vec![], &[(StyleId::Width, "100")])])
        .unwrap();
    band(&mut k, 34.0);
    lay_out(&mut k);
    assert_eq!(size(&k, 3).1, 81.0);
    let keyboard = Keyboard {
        frame: [0.0, 516.0, 390.0, 328.0],
        docked: true,
    };
    k.set_owner_keyboard(2, Some(keyboard)).unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 3).1, 51.6);
    k.set_owner_keyboard(2, None).unwrap();
    k.apply(
        0,
        3,
        &[style(2, vec![], &[(StyleId::SafeAreaPaddingBottom, "20")])],
    )
    .unwrap();
    lay_out(&mut k);
    let reserved = size(&k, 3).1;
    assert_eq!(
        reserved,
        (844.0 - crate::kernel::block::clearance(&k.arena, slot(&k, 2))[2]) / 10.0
    );
    assert_ne!(reserved, 81.0);
    k.apply(
        0,
        4,
        &[style(
            2,
            vec![],
            &[
                (StyleId::SafeAreaPaddingBottom, "0"),
                (StyleId::SafeAreaIgnore, "container"),
            ],
        )],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 3).1, 84.4);
}

#[test]
fn a_trial_block_resizes_the_container_it_measures() {
    // 2's root, sized by its content, holds 3 at half the content root's height: 422 in the
    // 844-point block, and 300 in a 600-point block a host weighs, which leaves the laid-out
    // tree as it was
    let mut ops = create(3..=3);
    ops.push(style(3, vec![], &[(StyleId::Height, "50cqh")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!(size(&k, 2).1, 422.0);
    let trial = k.measure_block(2, block(BlockAxis::Is(390.0), BlockAxis::Is(600.0)));
    assert_eq!(trial.unwrap().border.1, 300.0);
    lay_out(&mut k);
    assert_eq!(size(&k, 2).1, 422.0);
}

#[test]
fn an_owners_root_reads_its_own_content_root() {
    // 2's root is laid out in its 390 by 763 content root (D3), so `100cqh` is that, not the
    // 844-point block outside
    let ops = vec![
        style(
            2,
            vec![],
            &[
                (StyleId::SafeAreaIgnore, "none"),
                (StyleId::MinHeight, "100cqh"),
                (StyleId::Width, "50cqw"),
            ],
        ),
        children(1, &[2]),
    ];
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(2, Some([47.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 2), (195.0, 763.0));
}

#[test]
fn a_box_that_joins_the_top_layer_stops_fixing_its_containers() {
    // 3, whose host gives it a 200-wide block, is an owner, and 4 half its width; opened as a
    // popover it is no owner, and 4 is half of 2's 390-point content root
    let mut ops = create(3..=4);
    ops.push(style(4, vec![], &[(StyleId::Width, "50cqw")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_host_block(3, Some(block(BlockAxis::Is(200.0), BlockAxis::Is(400.0))))
        .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 4).0, 100.0);
    k.apply(0, 2, &[popover(3)]).unwrap();
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(size(&k, 4).0, 195.0);
}

#[test]
fn a_top_layer_box_reads_the_owner_it_is_written_in() {
    // The popover 3, written in 2, is laid out in the top layer, but its container is 2's
    // 300-wide content root (D7), not the 390-point viewport
    let mut ops = create(3..=3);
    ops.extend([style(3, vec![], &[(StyleId::Width, "50cqw")]), popover(3)]);
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(300.0, 600.0, ops);
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(size(&k, 3).0, 150.0);
}

#[test]
fn an_axis_the_host_does_not_fix_looks_further_out() {
    // 3, a sheet inside the owner 2 whose host fixes its width and fits its height, is the
    // container across; down, 2's 700-point content root is, and outside every owner the
    // small viewport, 390 by 844 here
    let mut ops = create(3..=5);
    ops.push(style(2, vec![], &[(StyleId::Height, "100%")]));
    ops.push(style(
        4,
        vec![],
        &[(StyleId::Width, "50cqw"), (StyleId::Height, "10cqh")],
    ));
    ops.push(style(
        5,
        vec![],
        &[(StyleId::Width, "10cqw"), (StyleId::Height, "1cqh")],
    ));
    ops.extend([children(1, &[2, 5]), children(2, &[3]), children(3, &[4])]);
    let mut k = owner(390.0, 700.0, ops);
    k.set_host_block(
        3,
        Some(block(BlockAxis::Is(200.0), BlockAxis::AtMost(600.0))),
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(size(&k, 4), (100.0, 70.0));
    assert_eq!(size(&k, 5), (39.0, 8.44));
}

#[test]
fn a_container_unit_crosses_the_wire_and_back() {
    for unit in crate::ViewportUnit::ALL
        .into_iter()
        .filter(|u| u.is_container())
    {
        let d = crate::style::Dimension::Viewport(unit, 12.5);
        let mut w = crate::wire::codec::Writer::new();
        w.dimension(d.clone());
        let bytes = w.into_vec();
        assert!((30..=35).contains(&bytes[0]), "{unit:?}");
        let mut r = crate::wire::codec::Reader::new(&bytes);
        assert_eq!(r.dimension(StyleId::Width, false).unwrap(), d, "{unit:?}");
    }
}

#[test]
fn a_relative_row_in_container_units_reads_its_owners_content_root() {
    // In 2's 300-wide content root, 3's font size of 10cqw is 30 and its letter spacing of 1cqi
    // 3; a block 200 wide makes them 20 and 2, and with its parent 4 moved out of every owner 3
    // reads the small viewport, 390 wide; its child 5's `2em` follows each
    let mut ops = create(3..=5);
    ops.push(style(5, vec![], &[(StyleId::FontSize, "2em")]));
    let rows = [
        (StyleId::FontSize, "10cqw"),
        (StyleId::LetterSpacing, "1cqi"),
    ];
    ops.push(style(3, vec![], &rows));
    ops.extend([
        children(1, &[2]),
        children(2, &[4]),
        children(4, &[3]),
        children(3, &[5]),
    ]);
    let mut k = owner(300.0, 600.0, ops);
    let mut env = k.arena.env().clone();
    (env.viewport_width, env.viewport_height) = (390.0, 844.0);
    k.set_env(env).unwrap();
    lay_out(&mut k);
    let read = |k: &Kernel| {
        let s = k.arena.computed_style(slot(k, 3), crate::StyleMask::ALL);
        (s.font_size, s.letter_spacing)
    };
    let child = |k: &Kernel| {
        k.arena
            .computed_style(slot(k, 5), crate::StyleMask::ALL)
            .font_size
    };
    assert_eq!((read(&k), child(&k)), ((30.0, 3.0), 60.0));
    k.set_host_block(2, Some(block(BlockAxis::Is(200.0), BlockAxis::Is(600.0))))
        .unwrap();
    lay_out(&mut k);
    assert_eq!((read(&k), child(&k)), ((20.0, 2.0), 40.0));
    k.apply(0, 2, &[children(2, &[]), children(1, &[2, 4])])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((read(&k), child(&k)), ((39.0, 3.9), 78.0));
}
