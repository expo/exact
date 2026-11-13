//! A render's environment (LLP 1048 D3, 1048.000 D9): the app's own data
//! source, holding nothing private. Its grants are the app's `net.fetch`
//! lines and nothing else — no kept secrets (the store is empty and can't
//! be written), no files, no SQLite, no key-value storage — so a render
//! answers what a new anonymous device would see. A render never configures
//! storage and never replaces the logic it booted with.
//!
//! A scheme-wide line (`net.fetch https:`) is not among them: a request's
//! route can choose what a source fetches, and on the server nothing stands
//! between that choice and the operator's own network. Such a source keeps
//! its placeholder, and the client fetches it after adoption. So is
//! `net.fetch local-network` (LLP 1133.036 D5): with it a visitor could point
//! the server at the operator's own network.
use exact_plan::{Plan, Value};
use exact_runner::{
    Answer, DataError, DataSource, Dispatch, InFlight, Interrupt, Outcome, Placement, Store, Target,
};

/// The app's data source in a render's environment.
pub struct Anonymous<D> {
    inner: D,
    grants: String,
}

impl<D: DataSource> Anonymous<D> {
    /// Wrap `inner`; its grants narrow to what a render holds.
    pub fn new(inner: D) -> Self {
        let grants = held(inner.grants());
        Self { inner, grants }
    }
}

/// The lines of `spec` a render holds (module doc).
fn held(spec: &str) -> String {
    spec.lines()
        .map(str::trim)
        .filter(|line| {
            let mut words = line.split_whitespace();
            words.next() == Some("net.fetch")
                && words.next().is_some_and(|target| {
                    (!target.ends_with(':') || target.contains('/')) && target != "local-network"
                })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
#[test]
fn a_render_holds_no_scheme_wide_or_local_network_line() {
    let spec = "net.fetch https://api.test\nnet.fetch https:\nnet.fetch local-network\n\
                net.fetch local-network development\nnet.websocket wss://a.test\nfs.read /x";
    assert_eq!(held(spec), "net.fetch https://api.test");
}

impl<D: DataSource> exact_runner::DataSourceWrapper for Anonymous<D> {
    fn prepare_snapshot(
        &mut self,
        module: Option<exact_runner::SnapshotModule<'_>>,
        deadline: std::time::Instant,
    ) -> Result<(), DataError> {
        self.inner.prepare_snapshot(module, deadline)
    }
    fn take_storage_changed(&mut self) -> bool {
        self.inner.take_storage_changed()
    }

    fn note_storage_changed(&mut self) {
        self.inner.note_storage_changed();
    }

    // A render acquires no host storage, secrets or document handles.
    fn set_document_owner(&mut self, _: u64) {}

    fn configure_storage(
        &mut self,
        _: std::path::PathBuf,
        _: std::path::PathBuf,
        _: std::path::PathBuf,
    ) -> Result<(), DataError> {
        Ok(())
    }

    fn configure_secrets(
        &mut self,
        _: std::sync::Arc<dyn exact_runner::SharedSecrets>,
    ) -> Result<(), DataError> {
        Ok(())
    }

    fn replacement(&self, _: &[u8], _: &str, _: Vec<u8>) -> Result<Self, DataError> {
        Err(DataError::Unavailable(
            "a render cannot replace its data source".into(),
        ))
    }

    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        self.inner.query(source, args)
    }
    fn app_id(&self) -> &str {
        self.inner.app_id()
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.inner.answer(store, source, args)
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.inner.parse(store, source, args, outcome)
    }
    fn answer_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.inner.answer_for(target, store, source, args)
    }
    fn more_for(&mut self, target: Target) -> Vec<exact_runner::Request> {
        self.inner.more_for(target)
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
        self.inner
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
        self.inner.parse_for(target, store, source, args, outcome)
    }
    fn launch(&self) -> Option<exact_runner::request::Launch> {
        self.inner.launch()
    }
    fn preloads(&self) -> &[exact_runner::request::Preload] {
        &[]
    }

    fn grants(&self) -> &str {
        &self.grants
    }
    fn revision(&self) -> Option<&str> {
        self.inner.revision()
    }
    fn bind(&mut self, plan: &Plan) {
        self.inner.bind(plan);
    }
    fn adopt(&mut self, source: &str, args: &[Value], value: &Value) {
        self.inner.adopt(source, args, value);
    }
    fn ready(&self) -> bool {
        self.inner.ready()
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        self.inner.canvas_surfaces()
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[Value],
        ctx: &exact_runner::exact_canvas::Context2d,
        frame: &exact_runner::exact_canvas::Frame,
    ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
        self.inner.draw_2d(surface, args, ctx, frame)
    }

    fn draw(
        &mut self,
        request: &exact_runner::DrawRequest<'_>,
        ctx: &exact_runner::exact_canvas::Context2d,
    ) -> exact_runner::Drawn {
        self.inner.draw(request, ctx)
    }
    fn canvases_retired(&mut self, retired: &[(u64, u32)]) {
        self.inner.canvases_retired(retired)
    }
    fn preload(&self) -> Result<bool, DataError> {
        self.inner.preload()
    }
    fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
        self.inner.when_preloaded(wake)
    }
    fn activate(&mut self) -> Result<(), DataError> {
        self.inner.activate()
    }
    fn activate_for_validation(&mut self) -> Result<(), DataError> {
        self.inner.activate_for_validation()
    }
    fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
        self.inner.continuation(token)
    }
    fn host_continuation(&self, token: u64) -> bool {
        self.inner.host_continuation(token)
    }
    fn placement(&self) -> Placement {
        self.inner.placement()
    }
    fn interrupt(&self) -> Option<Interrupt> {
        self.inner.interrupt()
    }
    fn native(&self) -> Option<exact_runner::Native> {
        self.inner.native()
    }
    fn overlay(
        &mut self,
        source: &str,
        args: &[Value],
        answer: &Value,
        writes: &[exact_runner::Write<'_>],
    ) -> Result<Option<exact_runner::Overlaid>, DataError> {
        self.inner.overlay(source, args, answer, writes)
    }

    fn forgotten(&mut self, store: &exact_runner::Store, in_flight: &[InFlight<'_>]) {
        self.inner.forgotten(store, in_flight);
    }
    fn dispatch(&mut self, token: u64, store: &Store) -> Dispatch {
        self.inner.dispatch(token, store)
    }
    fn release(&mut self, store: &Store) -> Vec<(u64, Dispatch)> {
        self.inner.release(store)
    }
    fn discard(&mut self, token: u64) {
        self.inner.discard(token);
    }
    fn background(&mut self, store: &Store) -> Option<exact_runner::Request> {
        self.inner.background(store)
    }
    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.inner.background_landed(store, outcome)
    }
    fn background_state(&self) -> Option<exact_runner::BackgroundState> {
        self.inner.background_state()
    }
    fn keep_sent(
        &mut self,
        store: &Store,
        let_go: &[exact_runner::InFlight<'_>],
        refused: bool,
    ) -> Vec<Option<u64>> {
        self.inner.keep_sent(store, let_go, refused)
    }
    fn keeps_sent(&self, fetch: u64) -> bool {
        self.inner.keeps_sent(fetch)
    }
    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.inner.module_delivery(store, token, outcome)
    }
    fn timer_before_reply(
        &mut self,
        store: &Store,
        in_flight: InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, exact_runner::Request)> {
        self.inner.timer_before_reply(store, in_flight, fetch)
    }
    fn module_landed(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        self.inner.module_landed(store, token, outcome)
    }
    fn take_logs(&mut self) -> Vec<String> {
        self.inner.take_logs()
    }

    fn take_aborted(&mut self) -> Vec<u64> {
        self.inner.take_aborted()
    }

    fn abort_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.inner.abort_wake(wake);
    }
}
