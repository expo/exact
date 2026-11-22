//! The heavy list on the ordinary Windows host, with its baked Rust data source.
include!(concat!(env!("OUT_DIR"), "/window.rs"));
const PLAN: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/app.plan"));
const COMPAT: &str = include_str!(concat!(env!("OUT_DIR"), "/compat.json"));
fn main() {
    std::process::exit(exact_windows::run_baked::<exact_heavylist_data::Heavy, ADMIT_AGENT>(
        "Heavy list",
        window(),
        PLAN,
        COMPAT,
        Default::default(),
    ));
}
