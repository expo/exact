//! The owner update (LLP 1123 D2): one entry per owner, applied whole or refused whole, in
//! the hosts' shared wire form.
use super::*;
use crate::kernel::{Contribution, Keyboard, Operation, OwnerMargins, OwnerUpdate};
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
/// navigation controller (20) above the owner's own (10) and its window (44), margins, a
/// docked keyboard and two columns.
fn full() -> OwnerUpdate {
    let mut targets = controller(1, 10.0);
    targets.chain.push(entry(2, 20.0, Some(Kind::Navigation)));
    targets.relations.push((Relation::Window, native(3, 44.0)));
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
        keyboard: Some(Keyboard {
            frame: [0.0, 500.0, 390.0, 344.0],
            docked: true,
        }),
        segments: Some(SegmentGrid {
            cols: 2,
            rows: 1,
            rects: vec![rect(0.0, 0.0, 190.0, 844.0), rect(210.0, 0.0, 180.0, 844.0)],
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
    // An operation past the four, at the contribution's byte 25, and a relation past the seven,
    // at the first relation's byte 118
    for (at, unknown) in [(25, 4), (118, 7)] {
        let mut bad = bytes.clone();
        bad[at] = unknown;
        assert!(OwnerUpdate::decode(&bad).is_err(), "byte {at}");
    }
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
    ];
    assert_eq!(operations.map(|o| o as u8), [0, 1, 2, 3]);
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
    // Each field in order, as the format says, written out apart from the encoder
    let mut w = Vec::new();
    let f = |w: &mut Vec<u8>, values: &[f32]| values.iter().for_each(|v| w.extend(v.to_le_bytes()));
    let target = |w: &mut Vec<u8>, id: u64, top: f32| {
        w.extend(id.to_le_bytes());
        w.extend(1u64.to_le_bytes());
        w.push(1);
        f(w, &[top, 0.0, 0.0, 0.0]);
    };
    w.push(1);
    f(&mut w, &[47.0, 0.0, 34.0, 0.0]);
    w.extend(1u32.to_le_bytes());
    w.extend(2u32.to_le_bytes());
    w.push(0);
    f(&mut w, &[10.0, 0.0, 0.0, 0.0]);
    w.extend(2u32.to_le_bytes());
    target(&mut w, 1, 10.0);
    w.push(0);
    target(&mut w, 2, 20.0);
    w.push(1 << 2);
    w.extend(1u32.to_le_bytes());
    w.push(3);
    target(&mut w, 3, 44.0);
    w.push(1);
    f(
        &mut w,
        &[
            47.0, 16.0, 34.0, 16.0, 8.0, 8.0, 8.0, 8.0, 0.0, 16.0, 0.0, 16.0, 672.0,
        ],
    );
    w.push(1);
    f(&mut w, &[0.0, 500.0, 390.0, 344.0]);
    w.push(1);
    w.extend([1, 2, 1]);
    f(&mut w, &[0.0, 0.0, 190.0, 844.0, 210.0, 0.0, 180.0, 844.0]);
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
            u.keyboard.as_mut().unwrap().frame[1] = f32::NAN
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
    // generation and its insets after their own
    let target = NativeTarget {
        id: 9,
        generation: 2,
        insets: Some([30.0, 0.0, 0.0, 0.0]),
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
    assert_eq!(encode_registration("map", Some(&target)), w);
    assert_eq!(encode_registration("map", None), [3, b'm', b'a', b'p', 0]);
}
