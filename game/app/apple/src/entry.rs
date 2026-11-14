#[derive(Default)]
struct ExactEmptyData;
impl exact_runner::DataSource for ExactEmptyData {
    fn app_id(&self) -> &str { app_id() }
    fn query(&mut self, source: &str, args: &[exact_runner::Value])
        -> Result<exact_runner::Value, exact_runner::DataError>
    {
        exact_runner::DataSource::query(&mut (), source, args)
    }
}
exact_logic::configured!(AppData, ExactEmptyData, || exact_logic::Swappable::off({ #[cfg(not(target_arch = "wasm32"))] ibex2_sqlite::link(); Default::default() }).named(app_id()));
#[allow(dead_code)]
fn app_data() -> AppData { Default::default() }

exact_apple::host!(AppData, plan(), compat(), None, std::ptr::null(), app_data);
