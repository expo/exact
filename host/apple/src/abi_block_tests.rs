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
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
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
