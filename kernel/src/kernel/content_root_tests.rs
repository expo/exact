//! A content owner's root laid out in its content root: its allocation less
//! the band its host clears (LLP 1123 D3), and the edges its root takes
//! (D4).
use super::*;

/// The owner 2, whose root clears its band and has the rows `root`,
/// holding 3, which pads its top by its own observation.
fn clearing(root: &[(StyleId, &str)]) -> Kernel {
    let mut rows = vec![(StyleId::SafeAreaIgnore, "none")];
    rows.extend_from_slice(root);
    let pad = vec![(StyleId::PaddingTop, observed(Edge::Top, None))];
    let mut ops = create(3..=3);
    ops.push(style(2, vec![], &rows));
    ops.push(style(3, pad, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    owner(390.0, 844.0, ops)
}

/// `id`'s frame: x, y, width and height in its layout's space.
fn frame(k: &Kernel, id: ViewId) -> (f32, f32, f32, f32) {
    let f = k.node(id).unwrap().frame;
    (f.x, f.y, f.width, f.height)
}

const BAND: [f32; 4] = [47.0, 10.0, 34.0, 20.0];

#[test]
fn an_owners_root_is_laid_out_in_its_allocation_less_its_band() {
    let mut k = clearing(&[
        (StyleId::Width, "100%"),
        (StyleId::Height, "100%"),
        (StyleId::PaddingLeft, "16"),
        (StyleId::PaddingRight, "16"),
    ]);
    // D3's example: 83 cleared at the bottom of 844 leaves a `height: 100%`
    // root 761 tall, and a `content-box` root's padding overflows its width
    // by 32, as anywhere
    k.set_owner_band(2, Some([0.0, 0.0, 83.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 0.0, 422.0, 761.0));
    // The root sits inside a band on every edge, and the box in it, clear
    // of the band, observes 0: padding by its observation pads nothing
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (20.0, 47.0, 392.0, 763.0));
    assert_eq!((frame(&k, 3).1, top(&k, 3)), (47.0, 0.0));
    assert_eq!(k.host_block_size(2), Some((390.0, 844.0)));
    // A host weighing another block measures what layout would give
    let measured = k.measure_block(2, block(BlockAxis::Is(390.0), BlockAxis::Is(600.0)));
    assert_eq!(measured.unwrap().border, (392.0, 519.0));
    // A band that moves the content root without resizing it moves the
    // root's subtree with it
    k.set_owner_band(2, Some([57.0, 10.0, 24.0, 20.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 2).1, frame(&k, 3).1), (57.0, 57.0));
    // An unavailable band clears nothing, nor does a band once the block
    // is given back, the root laid out in the document again
    k.set_owner_band(2, None, &[]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 0.0, 422.0, 844.0));
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    k.set_host_block(2, None).unwrap();
    lay_out(&mut k);
    assert_eq!((frame(&k, 2).0, frame(&k, 2).1), (0.0, 0.0));
}

#[test]
fn a_root_is_not_cleared_where_it_takes_its_container_region() {
    let mut k = clearing(&[(StyleId::Height, "100%")]);
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    // `end` is the inline end under the direction the root inherits
    let ops = [
        style(1, vec![], &[(StyleId::Direction, "rtl")]),
        style(2, vec![], &[(StyleId::SafeAreaIgnore, "container end")]),
    ];
    k.apply(0, 2, &ops).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 47.0, 380.0, 763.0));
    let mut txn = 2;
    let mut takes = |k: &mut Kernel, ignore: &str, direction: &str| {
        txn += 1;
        let rows = [
            (StyleId::SafeAreaIgnore, ignore),
            (StyleId::Direction, direction),
        ];
        k.apply(0, txn, &[style(2, vec![], &rows)]).unwrap();
        lay_out(k);
        frame(k, 2)
    };
    assert_eq!(takes(&mut k, "none", "ltr"), (20.0, 47.0, 360.0, 763.0));
    assert_eq!(takes(&mut k, "container", "ltr"), (0.0, 0.0, 390.0, 844.0));
    // Naming no region takes both, and the keyboard's leaves the band
    let top = (20.0, 0.0, 360.0, 810.0);
    assert_eq!(takes(&mut k, "container top", "ltr"), top);
    assert_eq!(takes(&mut k, "top", "ltr"), top);
    assert_eq!(takes(&mut k, "keyboard", "ltr"), (20.0, 47.0, 360.0, 763.0));
    // `start` is the inline start: the left, or under `rtl` the right
    let start = takes(&mut k, "container start", "ltr");
    assert_eq!(start, (0.0, 47.0, 380.0, 763.0));
    let start = takes(&mut k, "container start", "rtl");
    assert_eq!(start, (20.0, 47.0, 370.0, 763.0));
    // An ignore on the root applies to its content root, whatever the root's
    // size: half of what is left once the bottom is not cleared
    let rows = [(StyleId::Height, "50%")];
    k.apply(0, 20, &[style(2, vec![], &rows)]).unwrap();
    let half = takes(&mut k, "container bottom", "ltr");
    assert_eq!(half, (20.0, 47.0, 360.0, 398.5));
}

#[test]
fn a_band_change_owes_a_layout_where_it_moves_the_content_root() {
    // No box reads an observation, and the root takes the top
    let rows = [
        (StyleId::SafeAreaIgnore, "container top"),
        (StyleId::Height, "100%"),
    ];
    let mut k = owner(
        390.0,
        844.0,
        vec![children(1, &[2]), style(2, vec![], &rows)],
    );
    let band = |k: &mut Kernel, band| k.set_owner_band(2, Some(band), &[]).unwrap();
    assert!(!band(&mut k, [47.0, 0.0, 0.0, 0.0]), "a taken edge");
    assert!(band(&mut k, [47.0, 0.0, 34.0, 0.0]), "a cleared edge");
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (0.0, 0.0, 390.0, 810.0));
    // A band that moves the root without resizing it, where no reader's
    // layout changes, still places it anew
    band(&mut k, [47.0, 10.0, 34.0, 20.0]);
    lay_out(&mut k);
    band(&mut k, [47.0, 20.0, 34.0, 10.0]);
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (10.0, 0.0, 360.0, 810.0));
}

#[test]
fn a_sheets_content_root_leaves_its_band_inside_its_maximum() {
    let mut k = clearing(&[(StyleId::Height, "300")]);
    let sheet = |most| block(BlockAxis::Is(390.0), BlockAxis::AtMost(most));
    k.set_host_block(2, Some(sheet(800.0))).unwrap();
    k.set_owner_band(2, Some([0.0, 0.0, 34.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    // The sheet is as tall as its content and the band below it
    assert_eq!(k.host_block_size(2), Some((390.0, 334.0)));
    // Content taller than the maximum stops at the maximum less the band
    let rows = [(StyleId::Height, "900")];
    k.apply(0, 2, &[style(2, vec![], &rows)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2).3, 766.0);
    assert_eq!(k.host_block_size(2), Some((390.0, 800.0)));
    // A deeper band lowers that ceiling
    k.set_owner_band(2, Some([0.0, 0.0, 50.0, 0.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2).3, 750.0);
    // A host weighing another maximum measures what layout would give
    let measured = k.measure_block(2, sheet(600.0)).unwrap();
    assert_eq!(
        (measured.used, measured.border),
        ((390.0, 600.0), (390.0, 550.0))
    );
}

#[test]
fn a_top_layer_box_is_no_owner_and_is_not_cleared() {
    // The popover 3, under a band reported for it
    let rows = [(StyleId::Width, "50"), (StyleId::Height, "10")];
    let mut ops = create(3..=3);
    ops.extend([style(3, vec![], &rows), popover(3)]);
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    k.set_owner_band(3, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    // Its block is the viewport the kernel implies, no host's
    assert_eq!(frame(&k, 3), (0.0, 0.0, 50.0, 10.0));
    let given = block(BlockAxis::Is(100.0), BlockAxis::Is(100.0));
    k.set_host_block(3, Some(given)).unwrap();
    lay_out(&mut k);
    assert!(k.is_top_layer(3));
    assert_eq!(frame(&k, 3), (0.0, 0.0, 50.0, 10.0));
}

#[test]
fn a_fitted_block_and_a_width_maximum_hold_their_band() {
    // The root asks for 500 by 300, in a width at most 300 and a fitted height
    let mut k = clearing(&[(StyleId::Width, "500"), (StyleId::Height, "300")]);
    let fitted = block(BlockAxis::AtMost(300.0), BlockAxis::Fit(844.0));
    k.set_host_block(2, Some(fitted)).unwrap();
    k.set_owner_band(2, Some(BAND), &[]).unwrap();
    lay_out(&mut k);
    // Its width stops at the maximum less the band, its authored height
    // stands, and the block holds the band around both
    assert_eq!(frame(&k, 2), (20.0, 47.0, 270.0, 300.0));
    assert_eq!(k.host_block_size(2), Some((300.0, 381.0)));
    // In a region its authored width stands; a percentage margin is of the
    // content root's width
    let region = block(BlockAxis::Region(300.0), BlockAxis::Fit(844.0));
    k.set_host_block(2, Some(region)).unwrap();
    let rows = [(StyleId::MarginLeft, "10%")];
    k.apply(0, 2, &[style(2, vec![], &rows)]).unwrap();
    lay_out(&mut k);
    assert_eq!(frame(&k, 2), (47.0, 47.0, 500.0, 300.0));
    assert_eq!(k.host_block_size(2), Some((557.0, 381.0)));
}
