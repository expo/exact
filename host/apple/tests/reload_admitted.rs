//! LLP 1047.001 D6: a development binary links every family, so a reload
//! from hello's plan into a plan that uses one is committed as the next
//! generation. `reload_refused.rs` is hello's composition.

use exact_apple::abi::{Bridge, Hooks};

#[path = "reload/common.rs"]
mod common;
use common::{boot, digest, plan, NoData};

#[test]
fn an_all_linked_binary_reloads_from_hello_into_each_family() {
    for view in [
        "text \"a\" filter=\"blur(2px)\"",
        "svg viewBox=\"0 0 10 10\"\n      circle cx=5 cy=5 r=4",
        "view hatch=\"avatar\"",
    ] {
        let mut bridge = Bridge::<NoData>::new();
        boot(&mut bridge, &plan("text \"Hello world\""));
        let before = digest(&mut bridge);
        let next = plan(view);
        let len = bridge.input_write(&next);
        bridge.prepare_plan(len, NoData, Hooks::none(), 390.0, 844.0);
        let out = bridge.commit_plan();
        let reply = String::from_utf8_lossy(bridge.output_bytes(out as usize)).into_owned();
        assert!(!reply.contains("Unlinked"), "{view}: {reply}");
        assert_ne!(digest(&mut bridge), before, "{view}");
    }
}
