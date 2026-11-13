//! What the development producer knows about the Rust half of a mixed app,
//! which it cannot run (LLP 1027.002 §5 step 0).
use contract::DataSource;
use exact_js::Module;
use exact_plan::Value;
use exact_runner::{Answer, DataError, InFlight, Outcome, Overlaid, Store, Target, Write};
use std::collections::BTreeMap;
use std::path::Path;

/// What the development producer knows about the Rust half it cannot run:
/// the previous generation's plan and the sources Rust owns (from the last
/// Cargo bake's receipt). A Rust-owned resource keeps its previous
/// first-frame value until the next Cargo bake; one that has none refuses
/// by name.
pub struct Seed {
    values: BTreeMap<String, Result<exact_plan::Value, String>>,
    pub(crate) rust_sources: Vec<String>,
}

impl Seed {
    /// From the previous plan's bytes and the receipt's `rustSources`.
    pub fn new(plan: &[u8], rust_sources: &[String]) -> Result<Seed, String> {
        let plan = exact_plan::Plan::decode(plan).map_err(|e| format!("previous plan: {e:?}"))?;
        let mut values: BTreeMap<String, Result<exact_plan::Value, String>> = BTreeMap::new();
        for row in plan.resources.iter() {
            let source = plan.str(row.source).to_string();
            if !rust_sources.contains(&source) {
                continue;
            }
            let name = plan.str(row.name).to_string();
            let value = if row.initial.len > 0 {
                exact_plan::Value::from_bytes(plan.bytes(row.initial))
                    .map_err(|e| format!("`{name}`: previous value: {e:?}"))
            } else {
                Err(format!(
                    "`{name}` has no baked value; a Cargo bake supplies it"
                ))
            };
            values
                .entry(source)
                .and_modify(|v| {
                    *v = Err(format!(
                        "`{name}` shares its Rust source with another resource; a Cargo bake resolves it"
                    ))
                })
                .or_insert(value);
        }
        for source in rust_sources {
            values.entry(source.clone()).or_insert_with(|| {
                Err(format!(
                    "`{source}` has no seeded resource; checked by its source declaration"
                ))
            });
        }
        Ok(Seed {
            values,
            rust_sources: rust_sources.to_vec(),
        })
    }

    /// From the files on disk, when both exist; `None` otherwise.
    pub fn read(plan: Option<&Path>, receipt: Option<&Path>) -> Result<Option<Seed>, String> {
        let (Some(plan), Some(receipt)) = (plan, receipt) else {
            return Ok(None);
        };
        let (Ok(plan), Ok(receipt)) = (std::fs::read(plan), std::fs::read(receipt)) else {
            return Ok(None);
        };
        let receipt: serde_json::Value =
            serde_json::from_slice(&receipt).map_err(|e| format!("previous receipt: {e}"))?;
        let rust_sources: Vec<String> = receipt["rustSources"]
            .as_array()
            .map(|list| {
                list.iter()
                    .filter_map(|s| s.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();
        if rust_sources.is_empty() {
            return Ok(None);
        }
        Seed::new(&plan, &rust_sources).map(Some)
    }
}

/// The bake's module beside the seed: Rust-owned sources answer from the
/// previous plan; everything else is the module's.
pub(crate) struct Seeded<'a> {
    pub(crate) module: Module,
    pub(crate) seed: &'a Seed,
}

impl Seeded<'_> {
    fn seeds(&self, source: &str) -> bool {
        self.seed.values.contains_key(source)
    }
}

impl exact_runner::DataSourceWrapper for Seeded<'_> {
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        match self.seed.values.get(source) {
            Some(Ok(value)) => Ok(value.clone()),
            Some(Err(message)) => Err(DataError::Unavailable(message.clone())),
            None => self.module.query(source, args),
        }
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if self.seeds(source) {
            return DataSource::query(self, source, args).map(Answer::Now);
        }
        self.module.answer(store, source, args)
    }
    fn answer_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        if self.seeds(source) {
            return DataSource::query(self, source, args).map(Answer::Now);
        }
        self.module.answer_for(target, store, source, args)
    }
    fn overlay(
        &mut self,
        source: &str,
        args: &[Value],
        answer: &Value,
        writes: &[Write<'_>],
    ) -> Result<Option<Overlaid>, DataError> {
        if self.seeds(source) {
            return Ok(None);
        }
        self.module.overlay(source, args, answer, writes)
    }
    fn adopt(&mut self, source: &str, args: &[Value], value: &Value) {
        if !self.seeds(source) {
            self.module.adopt(source, args, value);
        }
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.module.parse(store, source, args, outcome)
    }
    fn more_for(&mut self, target: Target) -> Vec<exact_runner::Request> {
        self.module.more_for(target)
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
        self.module
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
        self.module.parse_for(target, store, source, args, outcome)
    }
    fn app_id(&self) -> &str {
        self.module.app_id()
    }
    fn preloads(&self) -> &[exact_runner::request::Preload] {
        self.module.preloads()
    }

    fn grants(&self) -> &str {
        self.module.grants()
    }
    fn revision(&self) -> Option<&str> {
        self.module.revision()
    }
    fn bind(&mut self, plan: &exact_plan::Plan) {
        self.module.bind(plan)
    }
    fn ready(&self) -> bool {
        self.module.ready()
    }
    fn canvas_surfaces(&self) -> Vec<(String, usize)> {
        self.module.canvas_surfaces()
    }
    fn prepare_snapshot(
        &mut self,
        module: Option<exact_runner::SnapshotModule<'_>>,
        deadline: std::time::Instant,
    ) -> Result<(), DataError> {
        self.module.prepare_snapshot(module, deadline)
    }
    fn take_storage_changed(&mut self) -> bool {
        self.module.take_storage_changed()
    }
    fn note_storage_changed(&mut self) {
        self.module.note_storage_changed()
    }
    fn set_document_owner(&mut self, owner: u64) {
        self.module.set_document_owner(owner)
    }
    fn configure_storage(
        &mut self,
        data: std::path::PathBuf,
        cache: std::path::PathBuf,
        temporary: std::path::PathBuf,
    ) -> Result<(), DataError> {
        self.module.configure_storage(data, cache, temporary)
    }
    fn configure_secrets(
        &mut self,
        secrets: std::sync::Arc<dyn exact_runner::SharedSecrets>,
    ) -> Result<(), DataError> {
        self.module.configure_secrets(secrets)
    }
    fn continuation(&mut self, token: u64) -> Option<Box<dyn FnOnce() -> Outcome + Send>> {
        self.module.continuation(token)
    }
    fn host_continuation(&self, token: u64) -> bool {
        self.module.host_continuation(token)
    }
    fn placement(&self) -> exact_runner::Placement {
        self.module.placement()
    }
    fn dispatch(&mut self, token: u64, store: &Store) -> exact_runner::Dispatch {
        self.module.dispatch(token, store)
    }
    fn release(&mut self, store: &Store) -> Vec<(u64, exact_runner::Dispatch)> {
        self.module.release(store)
    }
    fn discard(&mut self, token: u64) {
        self.module.discard(token)
    }
    fn keeps_sent(&self, fetch: u64) -> bool {
        self.module.keeps_sent(fetch)
    }
    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.module.module_delivery(store, token, outcome)
    }
    fn background(&mut self, store: &Store) -> Option<exact_runner::Request> {
        self.module.background(store)
    }
    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.module.background_landed(store, outcome)
    }
    fn keep_sent(
        &mut self,
        store: &Store,
        let_go: &[InFlight<'_>],
        refused: bool,
    ) -> Vec<Option<u64>> {
        self.module.keep_sent(store, let_go, refused)
    }
    fn timer_before_reply(
        &mut self,
        store: &Store,
        in_flight: InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, exact_runner::Request)> {
        self.module.timer_before_reply(store, in_flight, fetch)
    }
    fn module_landed(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        self.module.module_landed(store, token, outcome)
    }
    fn background_state(&self) -> Option<exact_runner::BackgroundState> {
        self.module.background_state()
    }
    fn take_logs(&mut self) -> Vec<String> {
        self.module.take_logs()
    }
    fn take_aborted(&mut self) -> Vec<u64> {
        self.module.take_aborted()
    }
    fn abort_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.module.abort_wake(wake)
    }
    fn activate(&mut self) -> Result<(), DataError> {
        self.module.activate()
    }
    fn activate_for_validation(&mut self) -> Result<(), DataError> {
        self.module.activate_for_validation()
    }
    fn preload(&self) -> Result<bool, DataError> {
        self.module.preload()
    }
    fn when_preloaded(&self, wake: Box<dyn FnOnce() + Send>) {
        self.module.when_preloaded(wake)
    }
    fn replacement(&self, plan: &[u8], receipt: &str, module: Vec<u8>) -> Result<Self, DataError> {
        let module = self.module.replacement(plan, receipt, module)?;
        Ok(Seeded {
            module,
            seed: self.seed,
        })
    }
    fn forgotten(&mut self, store: &Store, in_flight: &[InFlight<'_>]) {
        self.module.forgotten(store, in_flight)
    }
    fn interrupt(&self) -> Option<exact_runner::Interrupt> {
        self.module.interrupt()
    }
    fn native(&self) -> Option<exact_runner::Native> {
        self.module.native()
    }
    fn draw_2d(
        &mut self,
        surface: &str,
        args: &[Value],
        ctx: &exact_runner::exact_canvas::Context2d,
        frame: &exact_runner::exact_canvas::Frame,
    ) -> Result<bool, exact_runner::exact_canvas::DrawError> {
        self.module.draw_2d(surface, args, ctx, frame)
    }
    fn draw(
        &mut self,
        request: &exact_runner::DrawRequest<'_>,
        ctx: &exact_runner::exact_canvas::Context2d,
    ) -> exact_runner::Drawn {
        self.module.draw(request, ctx)
    }
    fn canvases_retired(&mut self, retired: &[(u64, u32)]) {
        self.module.canvases_retired(retired)
    }
}
