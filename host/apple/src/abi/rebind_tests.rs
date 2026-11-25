//! A restart's runner (a dev reload, an activated update: `commit_plan`) is
//! bound the extensions the app's module bound, which the module, made once
//! a session, does not bind again.
use super::super::{Bridge, Hooks};
use exact_plan::Value;
use exact_runner::{DataError, DataSource};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// Compiled beside an app in this checkout, where `@exact/geolocation` resolves.
fn plan() -> Vec<u8> {
    let source = "use geolocationStatus, GeolocationStatus from \"@exact/geolocation\"\ncomponent App\n  resource status = geolocationStatus() as shape GeolocationStatus else empty()\n  view\n    text \"Hi\"\n";
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../../apps/module-fixture/app.contract"
    );
    (contract::compile_path_source(std::path::Path::new(path), source))
        .unwrap()
        .encode()
}

fn bound(bridge: &Bridge<NoData>) -> bool {
    let runner = bridge.host.as_ref().unwrap().runner();
    let state = runner.extensions_json().expect("the plan names a module");
    serde_json::from_str::<serde_json::Value>(&state).unwrap()["geolocation"]["bound"] == true
}

#[test]
fn a_restarts_runner_is_bound_what_the_last_was() {
    let plan = plan();
    let mut bridge = Bridge::<NoData>::new();
    bridge.boot(&plan, NoData, Hooks::none(), 390.0, 844.0);
    let len = bridge.input_write(b"geolocation\n");
    bridge.bind_extensions(len);
    assert!(bound(&bridge));
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), NoData, Hooks::none(), 390.0, 844.0);
    assert!(bridge.prepared.is_some());
    bridge.commit_plan();
    assert!(bound(&bridge), "the restart's runner answers by the module");
}

#[test]
fn the_binding_observer_is_opted_in_and_follows_the_installed_runner() {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicUsize, Ordering};
    static REPORTS: AtomicUsize = AtomicUsize::new(0);
    extern "C" fn call(_: *mut c_void, bytes: *const u8, len: usize, slot: *mut c_void) {
        if slot.is_null() && !bytes.is_null() && len > 0 {
            REPORTS.fetch_add(1, Ordering::Relaxed);
        }
    }
    let plan = plan();
    let mut bridge = Bridge::<NoData>::new();
    bridge.boot(&plan, NoData, Hooks::none(), 390.0, 844.0);
    bridge.set_app_module(None, Some(call), std::ptr::null_mut());
    let len = bridge.input_write(b"geolocation");
    bridge.bind_extensions(len);
    assert_eq!(REPORTS.load(Ordering::Relaxed), 0);
    let len = bridge.input_write(b"view-bindings camera.track\ngeolocation");
    bridge.bind_extensions(len);
    assert_eq!(REPORTS.load(Ordering::Relaxed), 1);
    bridge.set_app_module(None, Some(call), std::ptr::null_mut());
    assert_eq!(
        REPORTS.load(Ordering::Relaxed),
        1,
        "reinstalling the same receiver must not publish a gap"
    );
    bridge.input_write(&plan);
    bridge.prepare_plan(plan.len(), NoData, Hooks::none(), 390.0, 844.0);
    bridge.commit_plan();
    assert!(bound(&bridge));
    assert_eq!(REPORTS.load(Ordering::Relaxed), 3);
}
