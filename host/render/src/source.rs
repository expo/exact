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
//! its placeholder, and the client fetches it after adoption. So are
//! `net.fetch local-network` (LLP 1133.036 D5) and `net.fetch loopback`
//! (LLP 1016 D6): with either, a visitor could point the server at the
//! operator's own network or host.
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
    /// Wrap `inner`; its grants narrow to what a render holds, and it reads
    /// what the browser's bundle reads (`render_web`).
    pub fn new(mut inner: D) -> Self {
        inner.render_web();
        let grants = held(inner.grants());
        Self { inner, grants }
    }

    /// Intersect a valid source scope with what a render holds, leaving an
    /// out-of-ceiling scope untouched so the executor still refuses it, and
    /// keeping an empty narrowed scope as `Some("")` rather than widening to
    /// `None`.
    fn narrow_request(&self, request: &mut exact_runner::Request) {
        let Some(scope) = request.grants.as_deref() else {
            return;
        };
        let within_inner = scope
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .all(|line| {
                self.inner
                    .grants()
                    .lines()
                    .map(str::trim)
                    .any(|grant| grant == line)
            });
        if within_inner {
            request.grants = Some(held(scope));
        }
    }

    fn narrow_answer(&self, mut answer: Answer) -> Answer {
        if let Answer::Later(request) = &mut answer {
            self.narrow_request(request);
        }
        answer
    }
}

/// The lines of `spec` a render holds (module doc): only exact-origin and
/// subdomain `net.fetch` grants, never `self`, scheme-wide, or host-class lines.
fn held(spec: &str) -> String {
    use ibex2::grant::{Grant, GrantSet};
    spec.lines()
        .map(str::trim)
        .filter(|line| {
            GrantSet::parse(line).is_ok_and(|set| {
                !set.is_empty()
                    && set
                        .iter()
                        .all(|g| matches!(g, Grant::Fetch(_) | Grant::FetchSubdomains(_)))
            })
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// LLP 1153.002 D2: a render's source reads what the browser's bundle
/// reads, told so before anything activates it.
#[cfg(test)]
#[test]
fn a_render_source_is_told_it_renders_the_web() {
    struct Leaf(bool);
    impl DataSource for Leaf {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn render_web(&mut self) {
            self.0 = true;
        }
    }
    assert!(Anonymous::new(Leaf(false)).inner.0);
}

#[cfg(test)]
#[test]
fn a_render_holds_only_origin_and_subdomain_fetch_lines() {
    let spec = "net.fetch https://api.test\nnet.fetch https://*.cdn.test\nnet.fetch https:\n\
                net.fetch self\nnet.fetch loopback\nnet.fetch local-network\n\
                net.fetch local-network development\nnet.websocket wss://a.test\n\
                net.websocket loopback\nfs.read /x";
    assert_eq!(
        held(spec),
        "net.fetch https://api.test\nnet.fetch https://*.cdn.test"
    );
}

#[cfg(test)]
#[test]
fn anonymous_narrows_valid_source_scopes_and_preserves_empty_or_out_of_ceiling() {
    use exact_runner::Request;
    struct MixedLike(&'static str);
    impl DataSource for MixedLike {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn grants(&self) -> &str {
            self.0
        }
        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            _: &[Value],
        ) -> Result<Answer, DataError> {
            let mut req = Request::get("https://api.test/data");
            req.grants = Some(match source {
                "both" => "net.fetch https://api.test\nnet.fetch loopback".into(),
                "loopback-only" => "net.fetch loopback".into(),
                "out-of-ceiling" => "net.fetch https://evil.test".into(),
                _ => return Ok(Answer::Now(Value::Bool(false))),
            });
            Ok(Answer::Later(req))
        }
    }
    let mut anon = Anonymous::new(MixedLike("net.fetch https://api.test\nnet.fetch loopback"));
    let mut store = Store::default();
    assert_eq!(anon.grants(), "net.fetch https://api.test");

    // Valid inner scope containing both exact-origin and loopback narrows to the held origin,
    // so an unrelated exact-origin fetch settles while loopback is refused.
    let Answer::Later(both) = anon.answer(&mut store, "both", &[]).unwrap() else {
        panic!("expected request");
    };
    assert_eq!(both.grants.as_deref(), Some("net.fetch https://api.test"));
    let scoped = exact_data::storage::scope(anon.grants(), both.grants.as_deref()).unwrap();
    assert_eq!(
        exact_runner::grants::admitting(anon.grants(), Some(scoped), "https://api.test/data"),
        Some("net.fetch https://api.test")
    );
    assert_eq!(
        exact_runner::grants::admitting(
            anon.grants(),
            Some(scoped),
            "http://127.0.0.1:49152/health"
        ),
        None
    );

    // Loopback-only source scope narrows to Some(""), never widening to None (app grants).
    let Answer::Later(only) = anon.answer(&mut store, "loopback-only", &[]).unwrap() else {
        panic!("expected request");
    };
    assert_eq!(only.grants.as_deref(), Some(""));
    let scoped = exact_data::storage::scope(anon.grants(), only.grants.as_deref()).unwrap();
    assert_eq!(
        exact_runner::grants::admitting(anon.grants(), Some(scoped), "https://api.test/data"),
        None
    );

    // Out-of-ceiling scope is left untouched so storage::scope still fails closed.
    let Answer::Later(evil) = anon.answer(&mut store, "out-of-ceiling", &[]).unwrap() else {
        panic!("expected request");
    };
    assert_eq!(evil.grants.as_deref(), Some("net.fetch https://evil.test"));
    assert!(exact_data::storage::scope(anon.grants(), evil.grants.as_deref()).is_err());
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

    fn render_web(&mut self) {
        self.inner.render_web();
    }

    fn ambient_taint(&self, source: &str) -> (bool, bool) {
        self.inner.ambient_taint(source)
    }

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
        self.inner
            .answer(store, source, args)
            .map(|a| self.narrow_answer(a))
    }
    fn parse(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.inner
            .parse(store, source, args, outcome)
            .map(|a| self.narrow_answer(a))
    }
    fn answer_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.inner
            .answer_for(target, store, source, args)
            .map(|a| self.narrow_answer(a))
    }
    fn more_for(&mut self, target: Target) -> Vec<exact_runner::Request> {
        let mut requests = self.inner.more_for(target);
        for request in &mut requests {
            self.narrow_request(request);
        }
        requests
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
            .map(|opt| opt.map(|a| self.narrow_answer(a)))
    }
    fn parse_for(
        &mut self,
        target: Target,
        store: &mut Store,
        source: &str,
        args: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.inner
            .parse_for(target, store, source, args, outcome)
            .map(|a| self.narrow_answer(a))
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
        self.inner.background(store).map(|mut r| {
            self.narrow_request(&mut r);
            r
        })
    }
    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        self.inner.background_landed(store, outcome).map(|opt| {
            opt.map(|mut r| {
                self.narrow_request(&mut r);
                r
            })
        })
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
        self.inner
            .module_delivery(store, token, outcome)
            .map(|opt| {
                opt.map(|mut r| {
                    self.narrow_request(&mut r);
                    r
                })
            })
    }
    fn timer_before_reply(
        &mut self,
        store: &Store,
        in_flight: InFlight<'_>,
        fetch: Option<u64>,
    ) -> Option<(u64, exact_runner::Request)> {
        self.inner
            .timer_before_reply(store, in_flight, fetch)
            .map(|(id, mut r)| {
                self.narrow_request(&mut r);
                (id, r)
            })
    }
    fn module_landed(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        self.inner.module_landed(store, token, outcome)
    }
    fn take_reports(&mut self) -> Vec<exact_runner::Report> {
        self.inner.take_reports()
    }
    fn take_logs(&mut self) -> Vec<String> {
        self.inner.take_logs()
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
        self.inner
            .parse_tool(fetch, continuation, store, source, args, outcome)
            .map(|a| self.narrow_answer(a))
    }

    fn module_identity(&self, source: &str) -> Option<exact_runner::ModuleIdentity> {
        self.inner.module_identity(source)
    }

    fn take_aborted(&mut self) -> Vec<u64> {
        self.inner.take_aborted()
    }

    fn abort_wake(&mut self, wake: std::sync::Arc<dyn Fn() + Send + Sync>) {
        self.inner.abort_wake(wake);
    }
}
