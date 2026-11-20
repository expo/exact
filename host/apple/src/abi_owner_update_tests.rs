//! A content owner's update and a registered target at the Apple doors (LLP 1123 D2, D9):
//! beside the blocks in one call of `exact_host_covers` or `exact_view`, every one checked
//! before the call changes anything.
use super::*;
use exact_kernel::style::env::{Rect, SegmentGrid};
use exact_kernel::style::targets::NativeTarget;
use exact_kernel::{Keyboard, OwnerUpdate};

/// A page whose route holds two readers: `column` pads by the second column's left edge, 7
/// where the owner lies on one segment, and `keyboard` by the docked keyboard's reach into it,
/// 0 while there is none. Below them, `moved` plays a layout move, which a host's fact does not.
/// The route has no block yet.
fn page() -> (Bridge<StorageModule>, u32, [u32; 2]) {
    let plan = contract::compile(
        "component Page\n  view\n    column\n      box testId=\"route\" width=390 height=844\n        box testId=\"column\" padding-top=\"var(--exact-viewport-segment-left-1-0, 7px)\"\n        box testId=\"keyboard\" height=700 padding-top=\"var(--exact-keyboard-inset-bottom, 7px)\"\n        box testId=\"moved\" height=10 -exact-layout-transition=\"1s linear\"\n",
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 390.0, 844.0);
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let id = |test: &str| k.node_by_key(k.find_by_test_id(test)[0]).unwrap().id;
    let (route, readers) = (id("route"), [id("column"), id("keyboard")]);
    (bridge, route, readers)
}

/// Two columns, the second from 210, and a keyboard docked over the bottom 344 points.
fn update() -> OwnerUpdate {
    let rect = |x: f32, width: f32| Rect {
        x,
        y: 0.0,
        width,
        height: 844.0,
    };
    OwnerUpdate {
        segments: Some(SegmentGrid {
            cols: 2,
            rows: 1,
            rects: vec![rect(0.0, 190.0), rect(210.0, 180.0)],
        }),
        keyboard: Some(Keyboard {
            frame: [0.0, 500.0, 390.0, 344.0],
            docked: true,
        }),
        ..OwnerUpdate::default()
    }
}

/// `route`'s block of `width` × 844, as `exact_host_covers` kind 4 and `exact_view`'s trailer
/// take it.
fn block(route: u32, width: f32) -> Vec<u8> {
    let words = [route, 4, width.to_bits(), 844.0_f32.to_bits(), 0, 0, 228];
    words.into_iter().flat_map(u32::to_le_bytes).collect()
}

/// `update`'s bytes as kind 21 frames them for `route`.
fn record(route: u32, update: &[u8]) -> Vec<u8> {
    let mut bytes: Vec<u8> = [route, 21, update.len() as u32]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    bytes.extend(update);
    bytes
}

fn covers(bridge: &mut Bridge<StorageModule>, bytes: &[u8]) -> String {
    bridge.input_write(bytes);
    let len = bridge.covers(bytes.len());
    String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned()
}

/// Each reader's top padding and the width of the route's block, `None` without one.
fn read(bridge: &Bridge<StorageModule>, route: u32, readers: [u32; 2]) -> ([f32; 2], Option<f32>) {
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let padding = |id: u32| k.resolved_padding(k.node(id).unwrap().key).unwrap().1;
    let width = k.host_block(route).map(|b| match b.width {
        exact_kernel::BlockAxis::Is(w) => w,
        other => panic!("{other:?}"),
    });
    (readers.map(padding), width)
}

#[test]
fn an_owner_update_rides_the_covers_door_after_its_block() {
    // The block and the update in one call: the column reader reads 210, and the keyboard's
    // reach is measured against the bounds the block gives
    let (mut bridge, route, readers) = page();
    assert_eq!(read(&bridge, route, readers).0, [7.0, 0.0]);
    let bytes = [block(route, 390.0), record(route, &update().encode())].concat();
    let batch = covers(&mut bridge, &bytes);
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(
        read(&bridge, route, readers),
        ([210.0, KEYBOARD], Some(390.0))
    );
}

/// The docked keyboard's reach into the keyboard reader, measured against the bounds the
/// route's block gives; an update applied before its block reads 0, measured against none.
const KEYBOARD: f32 = 344.0;

#[test]
fn a_bad_update_refuses_the_whole_call() {
    let (mut bridge, route, readers) = page();
    let bytes = [block(route, 390.0), record(route, &update().encode())].concat();
    covers(&mut bridge, &bytes);
    let before = read(&bridge, route, readers);
    // An update that does not decode, a presence byte of 2, beside another block
    let refused = covers(
        &mut bridge,
        &[block(route, 300.0), record(route, &[2])].concat(),
    );
    assert!(refused.contains("presence byte"), "{refused}");
    assert_eq!(read(&bridge, route, readers), before);
    // One the kernel refuses, a nonfinite keyboard frame, beside another block
    let mut bad = update();
    bad.keyboard.as_mut().unwrap().frame[1] = f32::NAN;
    let bytes = [block(route, 300.0), record(route, &bad.encode())].concat();
    let refused = covers(&mut bridge, &bytes);
    assert!(
        refused.contains("nothing in the call was applied"),
        "{refused}"
    );
    assert_eq!(read(&bridge, route, readers), before);
    // Two for one owner
    let twice = [
        record(route, &update().encode()),
        record(route, &update().encode()),
    ]
    .concat();
    let refused = covers(&mut bridge, &[block(route, 300.0), twice].concat());
    assert!(refused.contains("two for view"), "{refused}");
    assert_eq!(read(&bridge, route, readers), before);
}

#[test]
fn an_update_whose_block_is_refused_is_not_applied() {
    // After a block and an update, a block of nonfinite width is refused, so the update beside
    // it, a second column from 250 and no keyboard, is not applied and the readers read as
    // before
    let (mut bridge, route, readers) = page();
    covers(
        &mut bridge,
        &[block(route, 390.0), record(route, &update().encode())].concat(),
    );
    let mut moved = update();
    moved.segments.as_mut().unwrap().rects[1].x = 250.0;
    moved.keyboard = None;
    let bytes = [block(route, f32::NAN), record(route, &moved.encode())].concat();
    let batch = covers(&mut bridge, &bytes);
    assert!(batch.contains("block: "), "{batch}");
    assert_eq!(read(&bridge, route, readers).0, [210.0, KEYBOARD]);
}

#[test]
fn an_owner_update_rides_exact_views_trailer_beside_its_block() {
    let (mut bridge, route, readers) = page();
    // A view change of nothing but native blocks (mask 64): eight zero floats, no regions
    let mut wire: Vec<u8> = [64u32]
        .into_iter()
        .chain([0; 9])
        .flat_map(u32::to_le_bytes)
        .collect();
    wire.extend(block(route, 390.0));
    wire.extend(record(route, &update().encode()));
    let len = bridge.input_write(&wire);
    let len = bridge.view(len);
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(
        read(&bridge, route, readers),
        ([210.0, KEYBOARD], Some(390.0))
    );
    // One the kernel refuses changes nothing, the block beside it included
    let mut bad = update();
    bad.keyboard.as_mut().unwrap().frame[1] = f32::NAN;
    let mut wire: Vec<u8> = [64u32]
        .into_iter()
        .chain([0; 9])
        .flat_map(u32::to_le_bytes)
        .collect();
    wire.extend(block(route, 300.0));
    wire.extend(record(route, &bad.encode()));
    let len = bridge.input_write(&wire);
    let len = bridge.view(len);
    let refused = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(
        refused.contains("nothing in the call was applied"),
        "{refused}"
    );
    assert_eq!(
        read(&bridge, route, readers),
        ([210.0, KEYBOARD], Some(390.0))
    );
    // A malformed one refuses the view change
    let mut wire: Vec<u8> = [64u32]
        .into_iter()
        .chain([0; 9])
        .flat_map(u32::to_le_bytes)
        .collect();
    wire.extend(record(route, &[2]));
    let len = bridge.input_write(&wire);
    let len = bridge.view(len);
    let refused = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(refused.contains("view: an owner update"), "{refused}");
}

/// `update()` with its keyboard dismissed.
fn dismissed() -> OwnerUpdate {
    OwnerUpdate {
        keyboard: None,
        ..update()
    }
}

#[test]
fn an_update_alone_is_laid_out_at_either_door() {
    // After a block and an update, an update with no block beside it, the keyboard dismissed, is
    // laid out: at `exact_host_covers`
    let (mut bridge, route, readers) = page();
    covers(
        &mut bridge,
        &[block(route, 390.0), record(route, &update().encode())].concat(),
    );
    let batch = covers(&mut bridge, &record(route, &dismissed().encode()));
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(read(&bridge, route, readers).0, [210.0, 0.0]);
    // And in `exact_view`'s trailer, with nothing else changed: a host's fact, so the box the
    // keyboard reader's padding moves takes its place without playing its transition
    covers(&mut bridge, &record(route, &update().encode()));
    assert_eq!(read(&bridge, route, readers).0, [210.0, KEYBOARD]);
    let mut wire: Vec<u8> = [64u32]
        .into_iter()
        .chain([0; 9])
        .flat_map(u32::to_le_bytes)
        .collect();
    wire.extend(record(route, &dismissed().encode()));
    let len = bridge.input_write(&wire);
    let len = bridge.view(len);
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert!(batch.contains("\"motion\":false"), "{batch}");
    assert_eq!(read(&bridge, route, readers).0, [210.0, 0.0]);
}

#[test]
fn an_update_for_a_view_that_is_gone_refuses_only_itself() {
    // As a block for a gone view does: the route's block beside it is applied
    let (mut bridge, route, readers) = page();
    let gone = route + 100_000;
    let batch = covers(
        &mut bridge,
        &[block(route, 300.0), record(gone, &update().encode())].concat(),
    );
    assert!(batch.contains("UnknownView"), "{batch}");
    assert_eq!(read(&bridge, route, readers).1, Some(300.0));
}

/// `name`'s registration of `target`, or its disposal, as kind 22 frames it.
fn registration(name: &str, target: Option<&NativeTarget>) -> Vec<u8> {
    let bytes = exact_kernel::encode_registration(name, target);
    let mut out: Vec<u8> = [0, 22, bytes.len() as u32]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
    out.extend(bytes);
    out
}

/// A page whose `reader` pads by the registered target `map`'s top inset, 7 while it is
/// unavailable, and that reader's top padding.
fn registered_page() -> (Bridge<StorageModule>, u32) {
    let plan = contract::compile(
        "component Page\n  view\n    column\n      box testId=\"reader\" padding-top=\"var(--exact-target-map-safe-area-inset-top, 7px)\"\n",
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 390.0, 844.0);
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let reader = k.node_by_key(k.find_by_test_id("reader")[0]).unwrap().id;
    (bridge, reader)
}

fn top_padding(bridge: &Bridge<StorageModule>, id: u32) -> f32 {
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    k.resolved_padding(k.node(id).unwrap().key).unwrap().1
}

fn map(id: u64, generation: u64) -> NativeTarget {
    NativeTarget {
        id,
        generation,
        insets: Some([30.0, 0.0, 0.0, 0.0]),
    }
}

#[test]
fn a_registration_rides_the_covers_door_and_its_disposal_takes_it_back() {
    let (mut bridge, reader) = registered_page();
    let batch = covers(&mut bridge, &registration("map", Some(&map(9, 1))));
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(top_padding(&bridge, reader), 30.0);
    covers(&mut bridge, &registration("map", None));
    assert_eq!(top_padding(&bridge, reader), 7.0);
}

#[test]
fn a_refused_registration_refuses_the_whole_call() {
    let (mut bridge, reader) = registered_page();
    // A name D9 does not take, beside a good registration
    let bytes = [
        registration("map", Some(&map(9, 1))),
        registration("Map", Some(&map(8, 1))),
    ];
    let refused = covers(&mut bridge, &bytes.concat());
    assert!(
        refused.contains("nothing in the call was applied"),
        "{refused}"
    );
    assert_eq!(top_padding(&bridge, reader), 7.0);
    // Two for one name, and one that does not decode
    let twice = [
        registration("map", Some(&map(9, 1))),
        registration("map", None),
    ]
    .concat();
    assert!(covers(&mut bridge, &twice).contains("two for `map`"));
    let truncated = registration("map", Some(&map(9, 1)));
    let mut short = truncated[..truncated.len() - 1].to_vec();
    short[8..12].copy_from_slice(&((truncated.len() - 13) as u32).to_le_bytes());
    assert!(covers(&mut bridge, &short).contains("registered target is truncated"));
    // An older generation than the name's last, after it is registered
    covers(&mut bridge, &registration("map", Some(&map(9, 2))));
    let stale = covers(&mut bridge, &registration("map", Some(&map(9, 1))));
    assert!(stale.contains("nothing in the call was applied"), "{stale}");
    assert_eq!(top_padding(&bridge, reader), 30.0);
}

#[test]
fn a_registration_rides_exact_views_trailer() {
    let (mut bridge, reader) = registered_page();
    let mut wire: Vec<u8> = [64u32]
        .into_iter()
        .chain([0; 9])
        .flat_map(u32::to_le_bytes)
        .collect();
    wire.extend(registration("map", Some(&map(9, 1))));
    let len = bridge.input_write(&wire);
    let len = bridge.view(len);
    let batch = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert_eq!(top_padding(&bridge, reader), 30.0);
}
