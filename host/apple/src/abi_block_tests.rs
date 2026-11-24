use super::*;

#[test]
fn block_axis_records_distinguish_size_maximum_and_route_fitting() {
    use exact_kernel::{BlockAxis, HostBlock};
    let plan = contract::compile(
        "component Page\n  view\n    column\n      box testId=\"route\" width=480 height=300\n",
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 800.0, 600.0);
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let id = k.node_by_key(k.find_by_test_id("route")[0]).unwrap().id;
    for (width, height, expected, size) in [
        (
            2.0_f32,
            2.0_f32,
            HostBlock {
                tactic: Default::default(),
                width: BlockAxis::Fit(400.0),
                height: BlockAxis::Fit(200.0),
            },
            (480.0, 300.0),
        ),
        (
            1.0,
            1.0,
            HostBlock {
                tactic: Default::default(),
                width: BlockAxis::AtMost(400.0),
                height: BlockAxis::AtMost(200.0),
            },
            (400.0, 200.0),
        ),
        (
            2.0,
            1.0,
            HostBlock {
                tactic: Default::default(),
                width: BlockAxis::Fit(400.0),
                height: BlockAxis::AtMost(200.0),
            },
            (480.0, 200.0),
        ),
        (
            0.0,
            0.0,
            HostBlock {
                tactic: Default::default(),
                width: BlockAxis::Is(400.0),
                height: BlockAxis::Is(200.0),
            },
            (400.0, 200.0),
        ),
    ] {
        let bytes: Vec<_> = [
            id,
            4,
            400.0_f32.to_bits(),
            200.0_f32.to_bits(),
            width.to_bits(),
            height.to_bits(),
            228,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect();
        bridge.input_write(&bytes);
        let len = bridge.covers(bytes.len());
        let batch = std::str::from_utf8(bridge.output_bytes(len as usize)).unwrap();
        assert!(batch.contains("\"error\":null"), "{batch}");
        let k = bridge.host.as_ref().unwrap().runner().kernel();
        assert_eq!(k.host_block(id), Some(expected));
        assert_eq!(k.host_block_size(id), Some(size));
    }
}

#[test]
fn a_blocks_tactic_reaches_the_trial_and_the_layout_through_the_abi() {
    let plan = contract::compile(
        "component Page\n  view\n    column\n      box testId=\"pop\" popover=\"manual\" width=60 height=180 margin-top=4 margin-right=\"5%\" margin-bottom=8 margin-left=\"10%\"\n",
    ).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 400.0, 600.0);
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let id = k.node_by_key(k.find_by_test_id("pop")[0]).unwrap().id;
    // flip-start: left, bottom, right, top become top, right, bottom, left.
    let tactic = 3 | (2 << 2) | (1 << 4);
    assert_eq!(bridge.measure_block(id, [260.0, 100.0], [3, 3], tactic), 32);
    let measured: Vec<_> = bridge
        .output
        .as_chunks::<4>()
        .0
        .iter()
        .map(|b| f32::from_le_bytes(*b))
        .collect();
    assert_eq!(&measured[2..], &[180.0, 60.0, 26.0, 8.0, 13.0, 4.0]);
    let record = |tactic| {
        [
            id,
            4,
            260.0_f32.to_bits(),
            100.0_f32.to_bits(),
            3.0_f32.to_bits(),
            3.0_f32.to_bits(),
            tactic,
        ]
        .into_iter()
        .flat_map(u32::to_le_bytes)
        .collect::<Vec<_>>()
    };
    let bytes = record(tactic);
    bridge.input_write(&bytes);
    bridge.covers(bytes.len());
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let frame = k.node(id).unwrap().frame;
    assert_eq!(
        (frame.x, frame.y, frame.width, frame.height),
        (4.0, 26.0, 180.0, 60.0)
    );
    assert_eq!(k.host_block_size(id), Some((measured[0], measured[1])));
    let held = k.host_block(id);
    for invalid in [0, 256, u32::MAX] {
        assert_eq!(bridge.measure_block(id, [260.0, 100.0], [3, 3], invalid), 0);
        let bytes = record(invalid);
        bridge.input_write(&bytes);
        bridge.covers(bytes.len());
        assert!(String::from_utf8_lossy(&bridge.output).contains("invalid try tactic"));
        assert_eq!(
            bridge
                .host
                .as_ref()
                .unwrap()
                .runner()
                .kernel()
                .host_block(id),
            held
        );
    }
}

#[path = "abi_owner_update_tests.rs"]
mod owner_update;

#[test]
fn a_split_says_how_each_block_shows_against_its_stacks_top() {
    use exact_kernel::BlockShown;
    let plan = contract::compile(
        "component Page\n  view\n    column\n      box testId=\"route\" width=480 height=300\n      box testId=\"loose\" width=10 height=10\n",
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 800.0, 600.0);
    let k = bridge.host.as_ref().unwrap().runner().kernel();
    let id = |test_id: &str| k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    let (route, loose) = (id("route"), id("loose"));
    let send = |bridge: &mut Bridge<StorageModule>, words: Vec<u32>| {
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        bridge.input_write(&bytes);
        let len = bridge.covers(bytes.len());
        let batch = std::str::from_utf8(bridge.output_bytes(len as usize)).unwrap();
        assert!(batch.contains("\"error\":null"), "{batch}");
    };
    // A block first, as a split's column gives its route one
    let size = |v: f32| v.to_bits();
    send(
        &mut bridge,
        vec![
            route,
            4,
            size(400.0),
            size(200.0),
            size(0.0),
            size(0.0),
            228,
        ],
    );
    let shown = |bridge: &Bridge<StorageModule>, view| {
        bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .kernel()
            .host_block_shown(view)
    };
    assert_eq!(shown(&bridge, route), BlockShown::InStack);
    for (float, expected) in [
        (1.0_f32, BlockShown::Beside),
        (2.0, BlockShown::Under),
        (0.0, BlockShown::InStack),
    ] {
        send(&mut bridge, vec![route, 23, size(float), 0, 0, 0]);
        assert_eq!(shown(&bridge, route), expected, "{float}");
    }
    // A box with no block shows in its page, whatever is said of it
    send(&mut bridge, vec![loose, 23, size(1.0), 0, 0, 0]);
    assert_eq!(shown(&bridge, loose), BlockShown::InStack);
}

/// LLP 1141.004 D3: a block the host says is a navigation bar's item
/// (record kind 27) is a readout in the bar's type, and the host sends its
/// subtree's rows again when that changes, outside any commit.
#[test]
fn a_bar_item_is_sent_again_in_the_bars_type() {
    let plan = contract::compile(
        "component Page\n  view\n    column font-size=30\n      header testId=\"head\"\n        row role=\"status\" testId=\"readout\"\n          text \"Updating\" testId=\"word\"\n",
    )
    .unwrap()
    .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, StorageModule::default(), Hooks::none(), 400.0, 600.0);
    let host = bridge.host.as_mut().unwrap();
    let mut env = host.runner().kernel().env();
    env.bar_item_font = Some(exact_kernel::ControlFont {
        family: "system-ui".into(),
        family_id: 0,
        size: 17.0,
        weight: 500,
        style: exact_kernel::FontStyle::Normal,
    });
    host.runner_mut().kernel_mut().set_env(env).unwrap();
    let k = host.runner().kernel();
    let id = |test_id: &str| k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    let (head, readout, word) = (id("head"), id("readout"), id("word"));
    let send = |bridge: &mut Bridge<StorageModule>, words: Vec<u32>| -> String {
        let bytes: Vec<_> = words.into_iter().flat_map(u32::to_le_bytes).collect();
        bridge.input_write(&bytes);
        let len = bridge.covers(bytes.len());
        let batch = std::str::from_utf8(bridge.output_bytes(len as usize))
            .unwrap()
            .to_owned();
        assert!(batch.contains("\"error\":null"), "{batch}");
        batch
    };
    let f = |v: f32| v.to_bits();
    let block = vec![readout, 4, f(300.0), f(44.0), f(1.0), f(1.0), 228];
    let given = send(&mut bridge, [vec![head, 26, 0, 0, 0, 0], block].concat());
    assert!(
        !given.contains("\"font_size\":17"),
        "a block alone is no bar item: {given}"
    );
    let item = send(&mut bridge, vec![readout, 27, f(1.0), 0, 0, 0]);
    let sent = |batch: &str, size: &str| {
        batch.split("{\"op\"").any(|op| {
            op.contains(&format!("\"id\":{word},")) && op.contains(&format!("\"font_size\":{size}"))
        })
    };
    assert!(
        sent(&item, "17"),
        "the word is sent in the bar's size: {item}"
    );
    let page = send(&mut bridge, vec![readout, 27, f(0.0), 0, 0, 0]);
    assert!(
        sent(&page, "30"),
        "and in the page's once it is no item: {page}"
    );
}
