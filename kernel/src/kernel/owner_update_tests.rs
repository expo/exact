//! The owner update (LLP 1123 D2): one entry per owner, applied whole or refused whole, in
//! the hosts' shared wire form.
use super::*;
use crate::kernel::{
    BarRegion, Contribution, Keyboard, KeyboardReport, Operation, OwnerChrome, OwnerMargins,
    OwnerUpdate, VerticalBarEdge, WindowMap,
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
/// docked keyboard, two columns, a bottom bar region, chrome with a trailing vertical bar and no
/// sidebar, and a window map scaled and sheared 62 points down its window, its terms each another.
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
        window_map: Some(WindowMap([0.9, 0.1, 0.2, 0.8, 5.0, 62.0])),
        configuration: Some(7),
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
    // first relation's byte 120, and a vertical bar edge past the three, before the sidebar's two
    // bytes, the window map's 25 and the configuration's 9
    for (at, unknown) in [(25, 5), (120, 7), (bytes.len() - 37, 3)] {
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
    // The chrome: a trailing vertical bar, its raw value 2, and a sidebar that is not available;
    // then the window map
    w.extend([1, 1, 2, 1, 0]);
    w.push(1);
    f(&mut w, &[0.9, 0.1, 0.2, 0.8, 5.0, 62.0]);
    // and the configuration's identity
    w.push(1);
    w.extend(7u64.to_le_bytes());
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
    // A box in the owner reads its chrome and its window map, and one outside every owner none
    assert_eq!(
        (k.window_map(3), k.window_map(1)),
        (full().window_map, None)
    );
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

/// A session update with every field: a 47-point top band, a navigation controller (20) holding
/// the session view, a docked keyboard in the document's space, and `resizes-visual`.
fn session() -> crate::kernel::SessionUpdate {
    let mut targets = controller(1, 10.0);
    targets.chain.push(entry(2, 20.0, Some(Kind::Navigation)));
    crate::kernel::SessionUpdate {
        band: [47.0, 0.0, 34.0, 0.0],
        targets,
        keyboard: KeyboardReport::Shown(Keyboard {
            frame: [0.0, 516.0, 390.0, 328.0],
            docked: true,
        }),
        keyboard_policy: Some(crate::kernel::KeyboardPolicy::ResizesVisual),
        window_map: Some(WindowMap([1.0, 0.0, 0.0, 1.0, 0.0, 0.0])),
        configuration: Some(3),
    }
}

#[test]
fn a_session_update_round_trips_and_a_malformed_one_is_refused() {
    use crate::kernel::SessionUpdate;
    let update = session();
    let bytes = update.encode();
    assert_eq!(SessionUpdate::decode(&bytes), Ok(update));
    assert_eq!(
        SessionUpdate::decode(&SessionUpdate::default().encode()),
        Ok(SessionUpdate::default())
    );
    for n in 0..bytes.len() {
        assert!(SessionUpdate::decode(&bytes[..n]).is_err(), "{n} bytes");
    }
    assert!(SessionUpdate::decode(&[bytes.clone(), vec![0]].concat()).is_err());
    // The policy, before the window map's 25 bytes and the configuration's 9: 2 is
    // `resizes-visual`, and none past the three
    let policy = bytes.len() - 35;
    assert_eq!(bytes[policy], 2);
    let mut bad = bytes.clone();
    bad[policy] = 4;
    assert!(SessionUpdate::decode(&bad).is_err());
}

#[test]
fn a_session_update_reaches_the_boxes_outside_every_owner_whole_or_not_at_all() {
    // 3, outside the owner 2, reads the navigation controller holding the session view, 7 until
    // it is reported; an update with one bad field changes nothing
    let mut ops = create(3..=3);
    ops.push(reads(3, Target::Kind(Kind::Navigation)));
    ops.push(children(1, &[2, 3]));
    let mut k = owner(390.0, 844.0, ops);
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0);
    let mut bad = session();
    if let KeyboardReport::Shown(keyboard) = &mut bad.keyboard {
        keyboard.frame[0] = f32::NAN;
    }
    assert!(k.update_session(bad).is_err());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 7.0);
    assert_eq!(k.keyboard_facts(3), None);
    assert!(k.update_session(session()).unwrap());
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 20.0);
    assert_eq!(k.arena.observations.session(), [47.0, 0.0, 34.0, 0.0]);
    assert!(k.keyboard_facts(3).is_some_and(|f| f.docked));
    // A later one that says nothing of the keyboard leaves it unreported
    let quiet = crate::kernel::SessionUpdate {
        keyboard: KeyboardReport::Unreported,
        ..session()
    };
    k.update_session(quiet).unwrap();
    assert_eq!(k.keyboard_facts(3), None);
}

#[test]
fn a_window_map_places_a_rectangle_and_one_that_folds_the_plane_is_refused() {
    // A sheet UIKit scales to 90% behind another, 20 in and 40 down: a box at 10, 10 of 100 × 50
    // lies at 29, 49 of 90 × 45; a map that folds the plane refuses the whole update
    let scaled = WindowMap([0.9, 0.0, 0.0, 0.9, 20.0, 40.0]);
    let [x, y, w, h] = scaled.rect([10.0, 10.0, 100.0, 50.0]);
    let near = |a: f64, b: f64| (a - b).abs() < 1e-4;
    assert!(near(x, 29.0) && near(y, 49.0) && near(w, 90.0) && near(h, 45.0));
    let mut k = owner(390.0, 844.0, create(3..=3));
    lay_out(&mut k);
    let folded = OwnerUpdate {
        window_map: Some(WindowMap([1.0, 0.0, 1.0, 0.0, 0.0, 0.0])),
        ..full()
    };
    assert!(k.update_owner(2, folded).is_err());
    assert_eq!(k.window_map(2), None);
}

#[test]
fn an_observed_max_reader_keeps_the_largest_band_of_its_configuration() {
    // 3 pads by its top observation under `observed-max`: after a 100-point band and then a
    // 50-point one in configuration 1 it keeps 100 while its readout follows to 50;
    // configuration 2 starts again from 50, and a host that reports no configuration leaves it
    // reading the band live
    let mut ops = create(3..=3);
    let track = [
        (StyleId::Height, "10"),
        (StyleId::SafeAreaTrack, "observed-max"),
    ];
    ops.push(style(
        3,
        vec![(StyleId::PaddingTop, observed(Edge::Top, None))],
        &track,
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = owner(390.0, 844.0, ops);
    let band = |top: f32, configuration: Option<u64>| OwnerUpdate {
        band: Some([top, 0.0, 0.0, 0.0]),
        configuration,
        ..OwnerUpdate::default()
    };
    k.update_owner(2, band(100.0, Some(1))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 100.0);
    k.update_owner(2, band(50.0, Some(1))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 100.0, "the largest band of its configuration");
    assert_eq!(
        k.observation(3).map(|o| o[0]),
        Some(Some(50.0)),
        "its readout is live"
    );
    k.update_owner(2, band(50.0, Some(2))).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 3), 50.0);
    k.update_owner(2, band(100.0, Some(2))).unwrap();
    k.update_owner(2, band(50.0, None)).unwrap();
    lay_out(&mut k);
    assert_eq!(
        top(&k, 3),
        50.0,
        "with no configuration it reads the band live"
    );
}
