//! A boxed source is a source: every call forwarded to the source in the box,
//! so `Box<dyn DataSource>` erases a source's type. A runner is compiled once
//! for each source type it runs, and erasing puts any number of sources
//! behind one (the compiler's integration tests boot hundreds).

use super::{DataSource, DataSourceWrapper};
use crate::{Answer, DataError, InFlight, Outcome, Overlaid, Store, Target, Write};
use exact_plan::Value;

// Every method is forwarded but `replacement`, which needs the boxed type
// itself: a box answers it `Unavailable`, and a caller holding
// `Box<Concrete>` reaches the box's before `Concrete`'s, so box a source only
// to erase its type. A wrapper must write each method but `more_for` and
// `parse_fetch_for`, which default to the single-request seam.
impl<T: DataSource + ?Sized> DataSourceWrapper for Box<T> {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        (**self).query(source, args)
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        (**self).answer(store, source, args)
    }
    fn answer_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        (**self).answer_for(target, store, source, args)
    }
    fn overlay(
        &mut self,
        source: &str,
        args: &[Value],
        answer: &Value,
        writes: &[Write<'_>],
    ) -> Result<Option<Overlaid>, DataError> {
        (**self).overlay(source, args, answer, writes)
    }
    fn adopt(&mut self, source: &str, args: &[Value], value: &Value) {
        (**self).adopt(source, args, value)
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        (**self).parse(store, source, args, outcome)
    }
    fn more_for(&mut self, target: Target) -> Vec<crate::Request> {
        (**self).more_for(target)
    }
    fn parse_fetch_for(
        &mut self,
        target: Target,
        fetch: Option<u64>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Option<Answer>, DataError> {
        (**self).parse_fetch_for(target, fetch, store, source, args, outcome)
    }
    fn parse_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        (**self).parse_for(target, store, source, args, outcome)
    }
    fn app_id(&self) -> &str {
        (**self).app_id()
    }
    fn launch(&self) -> Option<crate::request::Launch> {
        (**self).launch()
    }
    fn preloads(&self) -> &[crate::request::Preload] {
        (**self).preloads()
    }

    fn grants(&self) -> &str {
        (**self).grants()
    }
    fn revision(&self) -> Option<&str> {
        (**self).revision()
    }
    fn bind(&mut self, plan: &exact_plan::Plan) {
        (**self).bind(plan)
    }
    fn ready(&self) -> bool {
        (**self).ready()
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        (**self).canvas_surfaces()
    }
    fn prepare_snapshot(
        &mut self,
        module: Option<crate::SnapshotModule<'_>>,
        deadline: std::time::Instant,
    ) -> Result<(), DataError> {
        (**self).prepare_snapshot(module, deadline)
    }
    fn take_storage_changed(&mut self) -> bool {
        (**self).take_storage_changed()
    }
    fn note_storage_changed(&mut self) {
        (**self).note_storage_changed()
    }
    fn set_document_owner(&mut self, owner: u64) {
        (**self).set_document_owner(owner)
    }
    fn render_web(&mut self) {
        (**self).render_web()
    }
    fn configure_storage(
        &mut self,
        data: std::path::PathBuf,
        cache: std::path::PathBuf,
        temporary: std::path::PathBuf,
    ) -> Result<(), DataError> {
        (**self).configure_storage(data, cache, temporary)
    }
    fn configure_secrets(
        &mut self,
        secrets: std::sync::Arc<dyn crate::SharedSecrets>,
    ) -> Result<(), DataError> {
        (**self).configure_secrets(secrets)
    }
    fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
        (**self).continuation(token)
    }
    fn host_continuation(&self, token: u64) -> bool {
        (**self).host_continuation(token)
    }
    fn placement(&self) -> crate::Placement {
        (**self).placement()
    }
    fn dispatch(&mut self, token: u64, store: &Store) -> crate::Dispatch {
        (**self).dispatch(token, store)
    }
    fn release(&mut self, store: &Store) -> Vec<(u64, crate::Dispatch)> {
        (**self).release(store)
    }
    fn discard(&mut self, token: u64) {
        (**self).discard(token)
    }
    fn keeps_sent(&self, fetch: u64) -> bool {
        (**self).keeps_sent(fetch)
    }
    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<crate::Request>, DataError> {
        (**self).module_delivery(store, token, outcome)
    }
    fn background(&mut self, store: &Store) -> Option<crate::Request> {
        (**self).background(store)
    }
    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<crate::Request>, DataError> {
        (**self).background_landed(store, outcome)
    }
    fn keep_sent(
        &mut self,
        store: &Store,
        let_go: &[InFlight<'_>],
        refused: bool,
    ) -> Vec<Option<u64>> {
        (**self).keep_sent(store, let_go, refused)
    }
    fn timer_before_reply(
        &mut self,
        store: &Store,
        in_flight: InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, crate::Request)> {
        (**self).timer_before_reply(store, in_flight, fetch)
    }
    fn module_landed(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        (**self).module_landed(store, token, outcome)
    }
    fn take_reports(&mut self) -> Vec<crate::Report> {
        (**self).take_reports()
    }
    fn background_state(&self) -> Option<crate::BackgroundState> {
        (**self).background_state()
    }
    fn take_logs(&mut self) -> Vec<String> {
        (**self).take_logs()
    }
    fn parse_tool(
        &mut self,
        fetch: Option<u64>,
        continuation: Option<u64>,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        (**self).parse_tool(fetch, continuation, store, source, args, outcome)
    }

    fn module_identity(&self, source: &str) -> Option<crate::ModuleIdentity> {
        (**self).module_identity(source)
    }

    fn take_aborted(&mut self) -> Vec<u64> {
        (**self).take_aborted()
    }
    fn abort_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        (**self).abort_wake(wake)
    }
    fn activate(&mut self) -> Result<(), DataError> {
        (**self).activate()
    }
    fn activate_for_validation(&mut self) -> Result<(), DataError> {
        (**self).activate_for_validation()
    }
    fn preload(&self) -> Result<bool, DataError> {
        (**self).preload()
    }
    fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
        (**self).when_preloaded(wake)
    }
    fn replacement(&self, plan: &[u8], receipt: &str, module: Vec<u8>) -> Result<Self, DataError> {
        let _ = (plan, receipt, module);
        Err(crate::DataError::Unavailable(
            "a boxed source is not replaced".into(),
        ))
    }
    fn forgotten(&mut self, store: &Store, in_flight: &[InFlight<'_>]) {
        (**self).forgotten(store, in_flight)
    }
    fn interrupt(&self) -> Option<crate::Interrupt> {
        (**self).interrupt()
    }
    fn native(&self) -> Option<crate::Native> {
        (**self).native()
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[Value],
        ctx: &crate::exact_canvas::Context2d,
        frame: &crate::exact_canvas::Frame,
    ) -> Result<bool, crate::exact_canvas::DrawError> {
        (**self).draw_2d(surface, args, ctx, frame)
    }
    fn draw(
        &mut self,
        request: &crate::DrawRequest<'_>,
        ctx: &crate::exact_canvas::Context2d,
    ) -> crate::Drawn {
        (**self).draw(request, ctx)
    }
    fn canvases_retired(&mut self, retired: &[(u64, u32)]) {
        (**self).canvases_retired(retired)
    }
}
