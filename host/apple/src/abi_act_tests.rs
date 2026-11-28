//! A person's act on the event ABI (LLP 1123 D6).

use super::*;

fn sent(bridge: &mut Bridge<StorageModule>, view: u32, kind: u32, payload: &[u8]) -> String {
    let len = bridge.input_write(payload);
    let length = bridge.dispatch(view, kind, len, 0.0);
    String::from_utf8_lossy(bridge.output_bytes(length as usize)).into_owned()
}

/// A marked press is the same press. The act's end is the mark on the reserved kind with no
/// payload; the reserved kind unmarked, or with a payload, is no kind.
#[test]
fn a_marked_press_is_the_same_press_and_its_end_is_the_marked_reserved_kind() {
    let bytes = contract::compile("component App\n  state count = 0\n  action increment\n    count = count + 1\n  view\n    button press=increment testId=\"increment\"\n      text `${count}`\n").unwrap().encode();
    let mut bridge = Bridge::<StorageModule>::new();
    bridge.boot(
        &bytes,
        StorageModule::default(),
        Hooks::none(),
        390.0,
        844.0,
    );
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let button = kernel
        .node_by_key(kernel.find_by_test_id("increment")[0])
        .unwrap()
        .id;
    for kind in [0, Event::ACT_KIND, Event::ACT_KIND | Event::ACT_END_KIND] {
        let batch = sent(&mut bridge, button, kind, b"");
        assert!(!batch.contains("unknown event kind"), "{kind}: {batch}");
    }
    let tree = exact_runner::agent::tree(bridge.host.as_ref().unwrap().runner());
    assert!(tree.contains("\"text\":\"2\""), "two presses: {tree}");
    let end = Event::ACT_KIND | Event::ACT_END_KIND;
    for (kind, payload) in [(Event::ACT_END_KIND, &b""[..]), (end, b"late")] {
        let batch = sent(&mut bridge, button, kind, payload);
        assert!(batch.contains("unknown event kind"), "{kind}: {batch}");
    }
}
