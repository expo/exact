//! The bake's composed source, as `contract::bake` takes it: every call
//! forwarded to the boxed source.

use super::Composed;
use exact_plan::Value;
use exact_runner::{Answer, DataError, InFlight, Outcome, Overlaid, Store, Target, Write};

// Every call forwarded to the composed source (`DataSourceWrapper` has no
// defaults, so one left out does not compile).
impl exact_runner::DataSourceWrapper for Composed {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        self.0.query(source, args)
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.0.answer(store, source, args)
    }
    fn answer_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.0.answer_for(target, store, source, args)
    }
    fn overlay(
        &mut self,
        source: &str,
        args: &[Value],
        answer: &Value,
        writes: &[Write<'_>],
    ) -> Result<Option<Overlaid>, DataError> {
        self.0.overlay(source, args, answer, writes)
    }
    fn adopt(&mut self, source: &str, args: &[Value], value: &Value) {
        self.0.adopt(source, args, value)
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.0.parse(store, source, args, outcome)
    }
    fn more_for(&mut self, target: Target) -> Vec<exact_runner::Request> {
        self.0.more_for(target)
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
        self.0
            .parse_fetch_for(target, fetch, store, source, args, outcome)
    }
    fn parse_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.0.parse_for(target, store, source, args, outcome)
    }
    fn app_id(&self) -> &str {
        self.0.app_id()
    }
    fn launch(&self) -> Option<exact_runner::request::Launch> {
        self.0.launch()
    }
    fn preloads(&self) -> &[exact_runner::request::Preload] {
        self.0.preloads()
    }

    fn grants(&self) -> &str {
        self.0.grants()
    }
    fn revision(&self) -> Option<&str> {
        self.0.revision()
    }
    fn bind(&mut self, plan: &exact_plan::Plan) {
        self.0.bind(plan)
    }
    fn ready(&self) -> bool {
        self.0.ready()
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        self.0.canvas_surfaces()
    }
    fn prepare_snapshot(
        &mut self,
        module: Option<exact_runner::SnapshotModule<'_>>,
        deadline: std::time::Instant,
    ) -> Result<(), DataError> {
        self.0.prepare_snapshot(module, deadline)
    }
    fn take_storage_changed(&mut self) -> bool {
        self.0.take_storage_changed()
    }
    fn note_storage_changed(&mut self) {
        self.0.note_storage_changed()
    }
    fn set_document_owner(&mut self, owner: u64) {
        self.0.set_document_owner(owner)
    }
    fn configure_storage(
        &mut self,
        data: std::path::PathBuf,
        cache: std::path::PathBuf,
        temporary: std::path::PathBuf,
    ) -> Result<(), DataError> {
        self.0.configure_storage(data, cache, temporary)
    }
    fn configure_secrets(
        &mut self,
        secrets: std::sync::Arc<dyn exact_runner::SharedSecrets>,
    ) -> Result<(), DataError> {
        self.0.configure_secrets(secrets)
    }
    fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
        self.0.continuation(token)
    }
    fn host_continuation(&self, token: u64) -> bool {
        self.0.host_continuation(token)
    }
    fn placement(&self) -> exact_runner::Placement {
        self.0.placement()
    }
    fn dispatch(&mut self, token: u64, store: &Store) -> exact_runner::Dispatch {
        self.0.dispatch(token, store)
    }
    fn release(&mut self, store: &Store) -> Vec<(u64, exact_runner::Dispatch)> {
        self.0.release(store)
    }
    fn discard(&mut self, token: u64) {
        self.0.discard(token)
    }
    fn keeps_sent(&self, fetch: u64) -> bool {
        self.0.keeps_sent(fetch)
    }
    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.0.module_delivery(store, token, outcome)
    }
    fn background(&mut self, store: &Store) -> Option<exact_runner::Request> {
        self.0.background(store)
    }
    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.0.background_landed(store, outcome)
    }
    fn keep_sent(
        &mut self,
        store: &Store,
        let_go: &[InFlight<'_>],
        refused: bool,
    ) -> Vec<Option<u64>> {
        self.0.keep_sent(store, let_go, refused)
    }
    fn timer_before_reply(
        &mut self,
        store: &Store,
        in_flight: InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, exact_runner::Request)> {
        self.0.timer_before_reply(store, in_flight, fetch)
    }
    fn module_landed(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        self.0.module_landed(store, token, outcome)
    }
    fn background_state(&self) -> Option<exact_runner::BackgroundState> {
        self.0.background_state()
    }
    fn take_logs(&mut self) -> Vec<String> {
        self.0.take_logs()
    }
    fn take_aborted(&mut self) -> Vec<u64> {
        self.0.take_aborted()
    }
    fn abort_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.0.abort_wake(wake)
    }
    fn activate(&mut self) -> Result<(), DataError> {
        self.0.activate()
    }
    fn activate_for_validation(&mut self) -> Result<(), DataError> {
        self.0.activate_for_validation()
    }
    fn preload(&self) -> Result<bool, DataError> {
        self.0.preload()
    }
    fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
        self.0.when_preloaded(wake)
    }
    fn replacement(&self, plan: &[u8], receipt: &str, module: Vec<u8>) -> Result<Self, DataError> {
        let _ = (plan, receipt, module);
        Err(exact_runner::DataError::Unavailable(
            "a composed bake source is not replaced".into(),
        ))
    }
    fn forgotten(&mut self, store: &Store, in_flight: &[InFlight<'_>]) {
        self.0.forgotten(store, in_flight)
    }
    fn interrupt(&self) -> Option<exact_runner::Interrupt> {
        self.0.interrupt()
    }
    fn native(&self) -> Option<exact_runner::Native> {
        self.0.native()
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[Value],
        ctx: &exact_runner::exact_canvas::Context2d,
        frame: &exact_runner::exact_canvas::Frame,
    ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
        self.0.draw_2d(surface, args, ctx, frame)
    }
    fn draw(
        &mut self,
        request: &exact_runner::DrawRequest<'_>,
        ctx: &exact_runner::exact_canvas::Context2d,
    ) -> exact_runner::Drawn {
        self.0.draw(request, ctx)
    }
    fn canvases_retired(&mut self, retired: &[(u64, u32)]) {
        self.0.canvases_retired(retired)
    }
}
