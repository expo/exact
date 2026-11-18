//! What the reload tests share: a source with no data, a plan of one view,
//! a boot and the running plan's digest.

use exact_apple::abi::{Bridge, Hooks};

pub struct NoData;

impl exact_runner::DataSource for NoData {
    fn query(
        &mut self,
        name: &str,
        _: &[exact_plan::Value],
    ) -> Result<exact_plan::Value, exact_runner::DataError> {
        Err(exact_runner::DataError::UnknownSource(name.into()))
    }
}

pub fn plan(view: &str) -> Vec<u8> {
    contract::compile(&format!("component A\n  view\n    {view}\n"))
        .unwrap()
        .encode()
}

pub fn boot(bridge: &mut Bridge<NoData>, plan: &[u8]) {
    let out = bridge.boot(plan, NoData, Hooks::none(), 390.0, 844.0);
    let reply = String::from_utf8_lossy(bridge.output_bytes(out as usize)).into_owned();
    assert!(!reply.contains("\"error\":\""), "{reply}");
}

pub fn digest(bridge: &mut Bridge<NoData>) -> String {
    let out = bridge.plan_digest();
    String::from_utf8_lossy(bridge.output_bytes(out as usize)).into_owned()
}
