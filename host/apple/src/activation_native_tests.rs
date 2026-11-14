use crate::abi::{Bridge, Hooks};
use exact_runner::DataSource;
use std::sync::mpsc::{channel, Sender};

#[path = "../../../logic/tests/fixtures/activation.rs"]
mod fixture;
use fixture::{Fixture, HANG};

fn boot(image: &Fixture, plan: &[u8], wake: &Sender<()>) -> Bridge<fixture::Source> {
    extern "C" fn notify(ctx: *mut std::ffi::c_void) {
        // The test retains the sender until after both runtimes retire.
        #[allow(unsafe_code)]
        let sender = unsafe { &*ctx.cast::<Sender<()>>() };
        let _ = sender.send(());
    }
    let mut bridge = Bridge::new();
    let hooks = Hooks {
        wake: Some(notify),
        wake_ctx: (wake as *const Sender<()>).cast_mut().cast(),
        ..Hooks::none()
    };
    let len = bridge.boot(plan, image.source(plan), hooks, 40., 40.);
    assert!(!batch(&bridge, len).contains("\"error\":\""));
    let len = bridge.data_ready();
    assert!(batch(&bridge, len).contains("\"pending\":true"));
    bridge
}

fn batch(bridge: &Bridge<fixture::Source>, len: u32) -> String {
    String::from_utf8(bridge.output_bytes(len as usize).to_vec()).unwrap()
}

#[test]
#[ignore = "async lane: builds and loads a native library"]
fn native_loader_completion_reaches_the_runtime_and_drops_retired_wakes() {
    complete(false);
}

#[test]
#[ignore = "async lane: builds and loads a native library"]
fn native_loader_failure_reaches_the_runtime_without_repeated_readiness() {
    complete(true);
}

fn complete(refuse: bool) {
    let mut image = Fixture::new(refuse);
    let plan = fixture::plan();
    let (wake, wakes) = channel();
    let mut bridge = boot(&image, &plan, &wake);
    image.entered();
    let (retired_wake, retired_wakes) = channel();
    let retired = boot(&image, &plan, &retired_wake);
    drop(retired);
    let (finished, finished_load) = channel();
    image.source(&plan).when_preloaded(Box::new(move || {
        let _ = finished.send(());
    }));
    image.release();
    wakes
        .recv_timeout(HANG)
        .expect("the loader must wake the session");
    // ExactSession retries data_ready only after its real executor wake.
    let len = bridge.data_ready();
    let frame = batch(&bridge, len);
    assert!(!frame.contains("\"pending\":true"), "{frame}");
    if refuse {
        assert!(frame.contains("Rust native export ABI differs"), "{frame}");
    } else {
        let batch: serde_json::Value = serde_json::from_str(&frame).unwrap();
        let ops = batch["ops"].as_array().unwrap();
        assert!(ops.iter().any(|op| op["set"]["text"] == "99"), "{frame}");
        assert!(
            ops.iter().any(|op| {
                op["style"]["background_color"] == serde_json::json!([0, 0, 255, 255])
            }),
            "{frame}"
        );
        assert!(!frame.contains("\"error\":\""), "{frame}");
    }
    finished_load.recv_timeout(HANG).unwrap();
    assert!(
        retired_wakes.try_recv().is_err(),
        "retired runtimes never call their hook"
    );
    assert!(
        wakes.try_recv().is_err(),
        "one loader completion wakes once"
    );
}
