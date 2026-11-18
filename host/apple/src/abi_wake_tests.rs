use super::*;

/// A session whose plan asks for no I/O has no executor, and is told all
/// the same when a candidate's image has loaded: with the producers' polls
/// gone, nothing else asks again (LLP 1029.000.002 D9).
#[test]
fn a_pending_image_wakes_a_session_that_has_no_executor() {
    use std::sync::atomic::{AtomicUsize, Ordering};
    static WOKEN: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn wake(ctx: *mut std::ffi::c_void) {
        WOKEN.fetch_add(ctx as usize, Ordering::SeqCst);
    }
    let bytes = contract::compile("component App\n  view\n    text \"x\"\n")
        .unwrap()
        .encode();
    let source = StorageModule::default();
    let mut links = crate::link::Links::ALL;
    links.io = None;
    let mut bridge = Bridge::with_links(links);
    let mut hooks = Hooks::none();
    hooks.wake = Some(wake);
    hooks.wake_ctx = 7 as *mut std::ffi::c_void;
    bridge.boot(&bytes, source.clone(), hooks, 390.0, 844.0);
    bridge.data_ready();
    assert!(bridge.executor.is_none());
    source.preparing.store(true, Ordering::SeqCst);
    bridge.input_write(&bytes);
    let count = bridge.prepare_module([bytes.len(), 0, 0], source.clone(), hooks, 390.0, 844.0);
    let batch = String::from_utf8_lossy(bridge.output_bytes(count as usize));
    assert!(batch.contains("\"pending\":true"), "{batch}");
    let loaded = std::mem::take(&mut *source.preloaded.lock().unwrap());
    assert_eq!(
        loaded.len(),
        1,
        "the loader is asked to say when it is done"
    );
    for wake in loaded {
        wake();
    }
    assert_eq!(WOKEN.load(Ordering::SeqCst), 7, "the session's own wake");
}
