//! What an owner's boxes observe of a band that holds Exact's own writes
//! (LLP 1123 D11, "Provenance").
use super::*;
use crate::kernel::{Contribution, Operation};
use crate::style::targets::{ChainEntry, Kind, NativeTarget, NativeTargets};

/// The owner 2, which pads its top by its observation, holding 3, which
/// pads its top by its controller's raw top inset; and 4 outside it.
fn setup() -> Kernel {
    let raw = observe(Target::Kind(Kind::Owner), Edge::Top, Some(7.0));
    let mut ops = create(3..=4);
    ops.push(style(
        2,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &[],
    ));
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, raw)],
        &[(StyleId::Height, "10")],
    ));
    ops.extend([children(1, &[2, 4]), children(2, &[3])]);
    owner(390.0, 844.0, ops)
}

/// `node`'s write of `top` at the top.
fn wrote(node: ViewId, operation: Operation, top: f32) -> Contribution {
    Contribution {
        node,
        operation,
        effect: [top, 0.0, 0.0, 0.0],
    }
}

/// Report `raw` at the top as the owner 2's band and its controller's
/// insets, with `contributions`, and lay out: whether the band owed a
/// layout, and the tops 2 and 3 pad by.
fn report(k: &mut Kernel, raw: f32, contributions: &[Contribution]) -> (bool, f32, f32) {
    let band = [raw, 0.0, 0.0, 0.0];
    let controller = ChainEntry {
        target: NativeTarget {
            id: 1,
            generation: 1,
            insets: Some(band),
            frame: None,
        },
        kinds: 0,
    };
    let targets = NativeTargets {
        chain: vec![controller],
        relations: Vec::new(),
    };
    k.set_owner_targets(2, targets).unwrap();
    let owed = k.set_owner_band(2, Some(band), contributions).unwrap();
    lay_out(k);
    (owed, top(k, 2), top(k, 3))
}

#[test]
fn an_owners_boxes_observe_its_raw_band_less_exacts_contributions() {
    let mut k = setup();
    let additional = |top| wrote(2, Operation::AdditionalInsets, top);
    assert_eq!(report(&mut k, 34.0, &[]), (true, 34.0, 34.0));
    // Exact's addition of 20, which UIKit gives back in the raw band: the
    // owner's boxes observe what they did, with no layout owed, and its
    // controller's raw insets hold it
    assert_eq!(
        report(&mut k, 54.0, &[additional(20.0)]),
        (false, 34.0, 54.0)
    );
    // An app's own write of 10 is the band's
    assert_eq!(
        report(&mut k, 64.0, &[additional(20.0)]),
        (true, 44.0, 64.0)
    );
    // The root's padding, the same node's other contribution, comes out too
    let padding = wrote(2, Operation::SafeAreaPadding, 8.0);
    let both = [additional(20.0), padding];
    assert_eq!(report(&mut k, 72.0, &both), (false, 44.0, 72.0));
    // D11's worked example: an addition of -50 over a baseline of 34 gives
    // raw 0, an effect of -34; over 60, raw 10, the same write's effect -50
    assert_eq!(report(&mut k, 0.0, &[additional(-34.0)]).1, 34.0);
    assert_eq!(report(&mut k, 10.0, &[additional(-50.0)]).1, 60.0);
    // Rounding in the host's subtractions leaves the band at 0, not below
    assert_eq!(report(&mut k, 10.0, &[additional(10.001)]).1, 0.0);
    // Each edge's effect comes out of that edge
    let effect = [20.0, 10.0, 0.0, -4.0];
    let all = Contribution {
        effect,
        ..additional(0.0)
    };
    k.set_owner_band(2, Some([60.0, 10.0, 34.0, 20.0]), &[all])
        .unwrap();
    let band = k.arena.observations.band(slot(&k, 2));
    assert_eq!(band, Some([40.0, 0.0, 34.0, 24.0]));
}

#[test]
fn an_update_with_a_contribution_it_cannot_take_is_refused_whole() {
    let mut k = setup();
    let additional = |top| wrote(2, Operation::AdditionalInsets, top);
    let update = |k: &mut Kernel, contributions: &[Contribution]| {
        k.set_owner_band(2, Some([60.0, 0.0, 0.0, 0.0]), contributions)
    };
    update(&mut k, &[additional(20.0)]).unwrap();
    // A nonfinite effect, and an identity twice
    for contributions in [
        vec![additional(f32::NAN)],
        vec![additional(20.0), additional(6.0)],
    ] {
        let result = update(&mut k, &contributions);
        let refusal = matches!(
            result,
            Err(KernelError::Layout(LayoutError::Contribution(_)))
        );
        assert!(refusal, "{contributions:?}: {result:?}");
    }
    let band = |k: &Kernel| k.arena.observations.band(slot(k, 2)).unwrap();
    assert_eq!(
        band(&k),
        [40.0, 0.0, 0.0, 0.0],
        "a refused update keeps the last"
    );
    // A contribution of a node the kernel does not have (9) still comes
    // out: its write is in the raw band until the host removes it
    update(&mut k, &[wrote(9, Operation::KeyboardInsets, 26.0)]).unwrap();
    assert_eq!(band(&k), [34.0, 0.0, 0.0, 0.0]);
}

#[test]
fn an_owner_roots_negative_inset_is_cleared_as_uikit_applied_it() {
    // 2, which takes none of its band, adds -50 on top under a 34-point band and pads 8 past it:
    // with no write measured its host clears what CSS says, 8; with its write measured to have no
    // effect, as a child controller's floor keeps its band, it clears the band and the padding, 42
    let mut ops = create(3..=3);
    let rows = [
        (StyleId::SafeAreaIgnore, "none"),
        (StyleId::AdditionalSafeAreaTop, "-50"),
        (StyleId::SafeAreaPaddingTop, "8"),
    ];
    ops.push(style(2, vec![], &rows));
    ops.push(style(3, vec![], &[(StyleId::Height, "10")]));
    ops.push(children(2, &[3]));
    let mut k = owner(390.0, 844.0, ops);
    assert_eq!(
        k.owner_writes(2),
        Some(vec![
            (2, Operation::AdditionalInsets, [-50.0, 0.0, 0.0, 0.0]),
            (2, Operation::SafeAreaPadding, [8.0, 0.0, 0.0, 0.0]),
        ])
    );
    assert_eq!(k.owner_writes(3), None, "no owner");
    let band = Some([34.0, 0.0, 0.0, 0.0]);
    k.set_owner_band(2, band, &[]).unwrap();
    lay_out(&mut k);
    let clear = |k: &Kernel| crate::kernel::block::clearance(&k.arena, slot(k, 2))[0];
    assert_eq!(
        clear(&k),
        8.0,
        "the band less 50, at least 0, then the padding"
    );
    let floored = wrote(2, Operation::AdditionalInsets, 0.0);
    assert!(
        k.set_owner_band(2, band, &[floored]).unwrap(),
        "a new effect owes a layout"
    );
    lay_out(&mut k);
    assert_eq!(clear(&k), 42.0, "the band the floor kept, then the padding");
}
