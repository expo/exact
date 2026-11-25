//! The owner update (LLP 1123 D2): one entry per owner, applied whole or refused whole, in
//! the hosts' shared wire form.
use super::*;
use crate::kernel::{
    BarRegion, Contribution, Keyboard, KeyboardReport, Operation, OwnerChrome, OwnerMargins,
    OwnerUpdate, VerticalBarEdge,
};
use crate::style::env::{Rect, SegmentGrid};

fn rect(x: f32, y: f32, width: f32, height: f32) -> Rect {
    Rect {
        x,
        y,
        width,
        height,
    }
}

/// An update with every field: a 47-point top band holding a 10-point contribution, a
/// navigation controller (20) above the owner's own (10) and its window (44, with its frame), margins, a
/// docked keyboard, two columns, a bottom bar region, and chrome with a trailing vertical bar and
/// no sidebar.
fn full() -> OwnerUpdate {
    let mut targets = controller(1, 10.0);
    targets.chain.push(entry(2, 20.0, Some(Kind::Navigation)));
    let window = NativeTarget {
        frame: Some([0.0, 0.0, 390.0, 844.0]),
        ..native(3, 44.0)
    };
    targets.relations.push((Relation::Window, window));
    OwnerUpdate {
        band: Some([47.0, 0.0, 34.0, 0.0]),
        contributions: vec![Contribution {
            node: 2,
            operation: Operation::AdditionalInsets,
            effect: [10.0, 0.0, 0.0, 0.0],
        }],
        targets,
        margins: Some(OwnerMargins {
            root: [47.0, 16.0, 34.0, 16.0],
            plain: [8.0; 4],
            minimum: [0.0, 16.0, 0.0, 16.0],
            readable_width: 672.0,
        }),
        keyboard: KeyboardReport::Shown(Keyboard {
            frame: [0.0, 500.0, 390.0, 344.0],
            docked: true,
        }),
        segments: Some(SegmentGrid {
            cols: 2,
            rows: 1,
            rects: vec![rect(0.0, 0.0, 190.0, 844.0), rect(210.0, 0.0, 180.0, 844.0)],
        }),
        bar_regions: [
            None,
            None,
            Some(BarRegion {
                distance: 24.0,
                sides: [16.0, 8.67],
            }),
            None,
        ],
        chrome: Some(OwnerChrome {
            vertical_bar_edge: Some(VerticalBarEdge::Trailing),
            sidebar_available: Some(false),
        }),
    }
}

#[test]
fn an_owner_update_round_trips_and_a_malformed_one_is_refused() {
    let update = full();
    let bytes = update.encode();
    assert_eq!(OwnerUpdate::decode(&bytes), Ok(update));
    assert_eq!(
        OwnerUpdate::decode(&OwnerUpdate::default().encode()),
        Ok(OwnerUpdate::default())
    );
    // Every truncation, a byte past the end, and a presence byte that is neither 0 nor 1
    for n in 0..bytes.len() {
        assert!(
            OwnerUpdate::decode(&bytes[..n]).is_err(),
            "{n} of {} bytes",
            bytes.len()
        );
    }
    assert!(OwnerUpdate::decode(&[bytes.clone(), vec![0]].concat()).is_err());
    let mut presence = bytes.clone();
    presence[0] = 2;
    assert!(OwnerUpdate::decode(&presence).is_err());
    // An operation past the five, at the contribution's byte 25, a relation past the seven, at the
    // first relation's byte 120, and a vertical bar edge past the three, at the third byte from the end
    for (at, unknown) in [(25, 5), (120, 7), (bytes.len() - 3, 3)] {
        let mut bad = bytes.clone();
        bad[at] = unknown;
        assert!(OwnerUpdate::decode(&bad).is_err(), "byte {at}");
    }
    // The keyboard's state, at an empty update's byte 14: unreported, dismissed, and none past
    // shown
    let mut empty = OwnerUpdate::default().encode();
    assert_eq!(empty[14], 0);
    empty[14] = 1;
    let dismissed = OwnerUpdate::decode(&empty).map(|u| u.keyboard);
    assert_eq!(dismissed, Ok(KeyboardReport::Dismissed));
    empty[14] = 3;
    assert!(OwnerUpdate::decode(&empty).is_err());
}

#[test]
fn an_owner_updates_wire_form_is_the_one_hosts_write() {
    // The numbering a host writes, by name: each operation's and relation's index, and the bits of
    // the kinds a host reports
    use Operation::*;
    let operations = [
        AdditionalInsets,
        SafeAreaPadding,
        BarReservation,
        KeyboardInsets,
        ContainerClearance,
    ];
    assert_eq!(operations.map(|o| o as u8), [0, 1, 2, 3, 4]);
    assert_eq!(Operation::ALL, operations);
    use Relation::*;
    let relations = [
        Presenter,
        Session,
        Root,
        Window,
        TabContent,
        WindowContent,
        Screen,
    ];
    assert_eq!(relations.map(|r| r as u8), [0, 1, 2, 3, 4, 5, 6]);
    assert_eq!(Relation::ALL, relations);
    let kinds = [Kind::Navigation, Kind::Tab, Kind::Split, Kind::Column];
    assert_eq!(kinds.map(Kind::bit), [4, 8, 16, 32]);
    // UIKit's raw `UIVerticalBarEdge` values
    use VerticalBarEdge::*;
    assert_eq!([Unspecified, Leading, Trailing].map(|e| e as u8), [0, 1, 2]);
    assert_eq!(VerticalBarEdge::ALL, [Unspecified, Leading, Trailing]);
    // Each field in order, as the format says, written out apart from the encoder
    let mut w = Vec::new();
    let f = |w: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|v| w.extend(v.to_le_bytes()));
    let target = |w: &mut Vec<u8>, id: u64, top: f32, frame: Option<[f32; 4]>| {
        w.extend(id.to_le_bytes());
        w.extend(1u64.to_le_bytes());
        w.push(1);
        f(w, &[top, 0.0, 0.0, 0.0]);
        w.push(u8::from(frame.is_some()));
        frame.inspect(|r| f(w, r));
    };
    w.push(1);
    f(&mut w, &[47.0, 0.0, 34.0, 0.0]);
    w.extend(1u32.to_le_bytes());
    w.extend(2u32.to_le_bytes());
    w.push(0);
    f(&mut w, &[10.0, 0.0, 0.0, 0.0]);
    w.extend(2u32.to_le_bytes());
    target(&mut w, 1, 10.0, None);
    w.push(0);
    target(&mut w, 2, 20.0, None);
    w.push(1 << 2);
    w.extend(1u32.to_le_bytes());
    w.push(3);
    target(&mut w, 3, 44.0, Some([0.0, 0.0, 390.0, 844.0]));
    w.push(1);
    f(
        &mut w,
        &[
            47.0, 16.0, 34.0, 16.0, 8.0, 8.0, 8.0, 8.0, 0.0, 16.0, 0.0, 16.0, 672.0,
        ],
    );
    w.push(2);
    f(&mut w, &[0.0, 500.0, 390.0, 344.0]);
    w.push(1);
    w.extend([1, 2, 1]);
    f(&mut w, &[0.0, 0.0, 190.0, 844.0, 210.0, 0.0, 180.0, 844.0]);
    // No region on the top or right, the bottom's, none on the left
    w.extend([0, 0, 1]);
    f(&mut w, &[24.0, 16.0, 8.67]);
    w.push(0);
    // The chrome: a trailing vertical bar, its raw value 2, and a sidebar that is not available
    w.extend([1, 1, 2, 1, 0]);
    assert_eq!(full().encode(), w);
}

#[test]
fn an_owner_update_applies_every_field() {
    // 3 reads the navigation controller's top inset, 4 the second column's left edge, and the
    // keyboard and margins are held for the owner 2
    let mut ops = create(3..=4);
    ops.push(reads(3, Target::Kind(Kind::Navigation)));
    let column = "var(--exact-viewport-segment-left-1-0, 7px)";
    ops.push(style(4, vec![], &[(StyleId::PaddingTop, column)]));
    ops.extend([children(1, &[2]), children(2, &[3, 4])]);
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 4)), (7.0, 7.0));
    assert!(k.update_owner(2, full()).unwrap());
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 4)), (20.0, 210.0));
    let slot = slot(&k, 2);
    let held = &k.arena.observations;
    // The band is held less Exact's contribution (D11), and the docked keyboard occupies the
    // owner's bottom 344 points
    assert!(held.margins.contains_key(&slot));
    assert_eq!(held.band(slot), Some([37.0, 0.0, 34.0, 0.0]));
    assert_eq!(crate::kernel::keyboard::occupancy(&k.arena, slot), 344.0);
    // A box in the owner reads its chrome, and one outside every owner none
    assert_eq!(
        (k.owner_chrome(3), k.owner_chrome(1)),
        (full().chrome, None)
    );
    // A later update giving the owner's controller an older generation is refused by the check
    assert!(k.check_owner_update(2, &full()).is_ok());
    let mut stale = full();
    stale.targets.chain[0].target.generation = 0;
    assert!(k.check_owner_update(2, &stale).is_err());
}

#[test]
fn an_owner_update_with_one_bad_field_changes_nothing() {
    // One bad value in each field refuses the update, so the target 3 reads and the segment 4
    // reads stay unavailable, and the owner holds no band, margins, keyboard or segments
    type Spoil = fn(&mut OwnerUpdate);
    let bad: [(&str, Spoil); 6] = [
        ("band", |u| u.band = Some([f32::NAN, 0.0, 0.0, 0.0])),
        ("contributions", |u| {
            u.contributions.push(u.contributions[0]);
        }),
        ("targets", |u| {
            u.targets.chain[1].kinds = 1 << Kind::ALL.len()
        }),
        ("margins", |u| {
            u.margins.as_mut().unwrap().readable_width = -1.0
        }),
        ("keyboard", |u| {
            if let KeyboardReport::Shown(k) = &mut u.keyboard {
                k.frame[1] = f32::NAN
            }
        }),
        ("segments", |u| {
            u.segments = Some(SegmentGrid {
                cols: 1,
                rows: 1,
                rects: vec![rect(0.0, 0.0, 390.0, 844.0)],
            })
        }),
    ];
    for (field, spoil) in bad {
        let mut ops = create(3..=4);
        ops.push(reads(3, Target::Kind(Kind::Navigation)));
        let column = "var(--exact-viewport-segment-left-1-0, 7px)";
        ops.push(style(4, vec![], &[(StyleId::PaddingTop, column)]));
        ops.extend([children(1, &[2]), children(2, &[3, 4])]);
        let mut k = owner(390.0, 844.0, ops);
        lay_out(&mut k);
        let mut update = full();
        spoil(&mut update);
        assert!(k.check_owner_update(2, &update).is_err(), "{field}");
        assert!(k.update_owner(2, update).is_err(), "{field}");
        lay_out(&mut k);
        assert_eq!((top(&k, 3), top(&k, 4)), (7.0, 7.0), "{field}");
        let (slot, held) = (slot(&k, 2), &k.arena.observations);
        assert_eq!(held.band(slot), None, "{field}");
        assert!(!held.margins.contains_key(&slot), "{field}");
        let keyboard = crate::kernel::keyboard::occupancy(&k.arena, slot);
        assert_eq!(keyboard, 0.0, "{field}");
        assert!(!held.segments.contains_key(&slot), "{field}");
    }
}

#[test]
fn a_registration_round_trips_and_a_malformed_one_is_refused() {
    use crate::kernel::{decode_registration, encode_registration};
    let target = native(7, 12.0);
    let bytes = encode_registration("map", Some(&target));
    assert_eq!(
        decode_registration(&bytes),
        Ok(("map".into(), Some(target)))
    );
    assert_eq!(
        decode_registration(&encode_registration("map", None)),
        Ok(("map".into(), None))
    );
    for n in 0..bytes.len() {
        assert!(
            decode_registration(&bytes[..n]).is_err(),
            "{n} of {} bytes",
            bytes.len()
        );
    }
    assert!(decode_registration(&[bytes, vec![0]].concat()).is_err());
}

#[test]
fn a_registrations_wire_form_is_the_one_hosts_write() {
    use crate::kernel::encode_registration;
    // The name's length and bytes, then the target after a presence byte: its id, its
    // generation, and its insets and frame each after their own
    let target = NativeTarget {
        id: 9,
        generation: 2,
        insets: Some([30.0, 0.0, 0.0, 0.0]),
        frame: Some([5.0, 6.0, 7.0, 8.0]),
    };
    let mut w = vec![3];
    w.extend(b"map");
    w.push(1);
    w.extend(9u64.to_le_bytes());
    w.extend(2u64.to_le_bytes());
    w.push(1);
    [30.0f32, 0.0, 0.0, 0.0]
        .iter()
        .for_each(|v| w.extend(v.to_le_bytes()));
    w.push(1);
    [5.0f32, 6.0, 7.0, 8.0]
        .iter()
        .for_each(|v| w.extend(v.to_le_bytes()));
    assert_eq!(encode_registration("map", Some(&target)), w);
    assert_eq!(encode_registration("map", None), [3, b'm', b'a', b'p', 0]);
}

#[test]
fn a_negative_raw_band_is_taken_and_its_incoming_band_is_at_least_0() {
    // A window root of 62 under Exact's −102 reads −40; less the effect, the owner's incoming band
    // is the window's 62
    let mut k = owner(390.0, 844.0, create(3..=3));
    lay_out(&mut k);
    let update = OwnerUpdate {
        band: Some([-40.0, 0.0, 34.0, 0.0]),
        contributions: vec![Contribution {
            node: 2,
            operation: Operation::AdditionalInsets,
            effect: [-102.0, 0.0, 0.0, 0.0],
        }],
        ..OwnerUpdate::default()
    };
    assert!(k.update_owner(2, update).is_ok());
    let slot = slot(&k, 2);
    assert_eq!(
        k.arena.observations.band(slot),
        Some([62.0, 0.0, 34.0, 0.0])
    );
}

#[test]
fn a_negative_raw_band_with_no_contribution_is_an_incoming_band_of_0() {
    // An app's own −102 on a 62-point window root: raw −40, and nothing of it is Exact's
    let mut k = owner(390.0, 844.0, create(3..=3));
    lay_out(&mut k);
    let update = OwnerUpdate {
        band: Some([-40.0, 0.0, 34.0, 0.0]),
        ..OwnerUpdate::default()
    };
    assert!(k.update_owner(2, update).is_ok());
    let slot = slot(&k, 2);
    assert_eq!(k.arena.observations.band(slot), Some([0.0, 0.0, 34.0, 0.0]));
}
