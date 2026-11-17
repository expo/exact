use super::*;
use std::sync::{Arc, Barrier};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

#[test]
fn concurrent_runners_share_indexing_and_keep_state_local() {
    let program = crate::Program::new(contract::compile(
        "component App\n  state count = 0\n  action add\n    count = count + 1\n  view\n    column\n      button press=add testId=\"add\"\n        text \"Add\"\n      text `${count}` testId=\"count\"\n",
    ).unwrap());
    let ready = Barrier::new(8);
    std::thread::scope(|scope| {
        for n in 0..8 {
            let program = &program;
            let ready = &ready;
            scope.spawn(move || {
                ready.wait();
                let mut runner = Runner::boot_at(
                    program,
                    NoData,
                    Kernel::with_monospace(),
                    Default::default(),
                    "/",
                    1000.0,
                    Default::default(),
                )
                .unwrap();
                let sites = program.sites(None);
                assert!(Arc::ptr_eq(&runner.sites.data, &sites.data));
                let add = runner.kernel.find_by_test_id("add")[0];
                let add = runner.kernel.node_by_key(add).unwrap().id;
                for _ in 0..n {
                    runner.dispatch(add, Event::Press).unwrap();
                }
                assert_eq!(runner.slots[0], Value::Number(n as f64));
                let fresh = Runner::boot_at(
                    program,
                    NoData,
                    Kernel::with_monospace(),
                    Default::default(),
                    "/",
                    1000.0,
                    Default::default(),
                )
                .unwrap();
                assert!(Arc::ptr_eq(&runner.sites.data, &fresh.sites.data));
                assert_eq!(fresh.slots[0], Value::Number(0.0));
            });
        }
    });
}
