//! The runner's methods that reach the module seam: the core's calls and
//! the hosts' public ones, each forwarding to the linked or unlinked seam.

use super::super::provenance::CallOrigin;
use super::super::{AppState, CommitReceipt, DataError, DataSource, Runner, RunnerError, Target};
use super::{build_target, Readers};
use crate::local_network::Reach;
use crate::request::{Answer, Dispatch, Outcome, Request};
use crate::HoldAnswer;
use exact_kernel::ViewId;
use exact_plan::{ModulesRow, TypesId, Value};
use serde_json::Value as Json;
use std::collections::BTreeMap;
use std::sync::Arc;

impl<D: DataSource> Runner<D> {
    /// Invocation `id`'s held calls end `AbortError` with `message`.
    pub(in crate::runner) fn abort_held_of(&mut self, id: u64, message: &str) {
        let m = self.seams.modules;
        m.abort_held_of(self, id, message)
    }

    /// Admission of an `img` source of a package's scheme, on `target` (the
    /// host's: `ios`, `macos`, `android`, `web`): `{"extension", "scheme",
    /// "agent", "owned"?}` to ask that extension's implementation for the
    /// picture, or `{"error", "message"}`, which the host shows as an
    /// unresolved image. `None` for a source of no package's scheme.
    pub fn admit_picture(&mut self, source: &str, target: &str) -> Option<Json> {
        let m = self.seams.modules;
        m.admit_picture(self, source, target)
    }

    /// The app left the foreground (`hidden`) or came back (D6's table): a
    /// waiting write fails `AbortError`; a scan behind the system's sheet
    /// (`readerUI` `system`) ends `interrupted`, and one the app draws for
    /// is suspended, and on return reads a tag still in the field.
    pub(in crate::runner) fn arrival_visibility(&mut self, hidden: bool) {
        let m = self.seams.modules;
        m.arrival_visibility(self, hidden)
    }

    /// The host's implementations answer `real` as in production, and
    /// `agent`, told they run under the agent, answer an interface that is
    /// its own substitute: their names' readers are asked again, of the
    /// module now, and each `agent` implementation is given the preferred
    /// values the agent set.
    pub fn bind_extensions(&mut self, real: &[&str], agent: &[&str]) {
        let m = self.seams.modules;
        m.bind_extensions(self, real, agent)
    }

    /// Whether the host bound its own implementation of `extension`.
    pub(in crate::runner) fn bound_here(&self, extension: &str) -> bool {
        self.seams.modules.bound_here(self, extension)
    }

    /// Admission without counting a draw: hosts recheck refused pictures
    /// after commits that may have changed admission.
    pub fn check_picture(&mut self, source: &str, target: &str) -> Option<Json> {
        let m = self.seams.modules;
        m.check_picture(self, source, target)
    }

    /// A provider reports its physical name; every active declaration of
    /// that name watches its own qualified topic.
    pub(in crate::runner) fn declaration_topics(&self, topics: Vec<String>) -> Vec<String> {
        self.seams.modules.declaration_topics(self, topics)
    }

    /// What the app's literals declare, by `<extension>.<name>`: each row's
    /// `declared` (LLP 1133.004 stage 2), `{option: [value, …]}`.
    pub fn declared_values(&self) -> Json {
        self.seams.modules.declared_values(self)
    }

    /// Whether `action`, a notification action's record as its post carried
    /// it, is one this plan declares under `extension` ([`Self::declared_values`]):
    /// a host whose platform hands back the record rather than its own
    /// registration (Android's, LLP 1133.004.000 D5) raises the action only
    /// then, so a button an update removed or changed raises nothing.
    pub fn declares_action(&self, extension: &str, action: &Json) -> bool {
        self.seams.modules.declares_action(self, extension, action)
    }

    /// The module events raised meanwhile (LLP 1133.004 D6), then a wake's,
    /// one invocation at a time (LLP 1133.017 D5).
    pub(in crate::runner) fn deliver_module_events(
        &mut self,
    ) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        let m = self.seams.modules;
        let (mut receipts, error) = m.deliver_module_events(self);
        if error.is_some() {
            return (receipts, error);
        }
        let (woke, error) = self.step_invocations();
        receipts.extend(woke);
        (receipts, error)
    }

    /// [`Runner`]'s `discard_failed_tools`, answered by the modules.
    pub(in crate::runner) fn discard_failed_tools(&mut self) {
        let m = self.seams.modules;
        m.discard_failed_tools(self)
    }

    /// A disclosure an implementation said: its fields kept.
    pub(in crate::runner) fn disclosed_told(&mut self, body: &str) {
        let m = self.seams.modules;
        m.disclosed_told(self, body)
    }

    /// A reply carrying `handles` was not delivered (`why`): its
    /// implementation disposes of them.
    pub(in crate::runner) fn dispose(&mut self, handles: (String, Vec<String>), why: &str) {
        let m = self.seams.modules;
        m.dispose(self, handles, why)
    }

    /// Replies with handles whose request is no longer in flight: dropped,
    /// their handles disposed of. Requests let go before they were sent are
    /// forgotten.
    pub(in crate::runner) fn dispose_dropped(&mut self) {
        let m = self.seams.modules;
        m.dispose_dropped(self)
    }

    /// Invocation `id`'s native statements are dropped: it expired.
    pub(in crate::runner) fn drop_statements_of(&mut self, id: u64) {
        let m = self.seams.modules;
        m.drop_statements_of(self, id)
    }

    /// The agent's `prefer extension` and `prefer permission` (LLP 1124 D7,
    /// LLP 1069.007 D2): `{"op":"prefer","extension":{"geolocation.watchPosition":
    /// {…}|"clear"}}` sets a source's answer; `{"op":"prefer","permission":
    /// {"geolocation":"granted"}}` a permission's state. Readers are asked
    /// again. `None` for any other request.
    pub fn extension_agent(
        &mut self,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        let m = self.seams.modules;
        m.extension_agent(self, request)
    }

    /// `state.extensions` (LLP 1133.002 §6, Disclosure): each module the plan
    /// names, by its extension: whether a host's implementation answers it
    /// (`bound`, and `agent` where it answers under the agent), its
    /// permission's state, the preferred values, each source's readers (a
    /// resource that reads it, or whose data code did), the tickets held
    /// with their calls' origins, and its ceremonies and signals, each by
    /// name, RP ID, origin and outcome, never a response (LLP 1133.011 §9).
    /// `None` for a plan that names none.
    pub fn extensions_json(&self) -> Option<String> {
        self.seams.modules.extensions_json(self)
    }

    /// [`Runner`]'s `fail_tool`, answered by the modules.
    pub(in crate::runner) fn fail_tool(&mut self, ticket: u64, message: &str) {
        let m = self.seams.modules;
        m.fail_tool(self, ticket, message)
    }

    /// [`Runner`]'s `failed_tool_tickets`, answered by the modules.
    pub(in crate::runner) fn failed_tool_tickets(&self) -> Vec<(u64, String)> {
        self.seams.modules.failed_tool_tickets(self)
    }

    /// The GPU canvases a commit published: their surface names, kept by view.
    pub(in crate::runner) fn fed_canvases(&mut self, updates: &[crate::SurfaceUpdate]) {
        let m = self.seams.modules;
        m.fed_canvases(self, updates)
    }

    /// A claim an implementation told: journaled, and listed while held.
    pub(in crate::runner) fn foreground_service_told(&mut self, body: &str) {
        let m = self.seams.modules;
        m.foreground_service_told(self, body)
    }

    /// Resource row `i` is a new member's: its last poll answer no longer
    /// stands.
    pub(in crate::runner) fn forget_module_row(&mut self, i: usize) {
        let m = self.seams.modules;
        m.forget_module_row(self, i)
    }

    /// The host runs a module request whose reply carries handles, the
    /// oldest such request with this body not yet run: its implementation's
    /// reply is watched, whether or not the request is still in flight (a
    /// host may run one the runner has let go). `None` for any other body.
    pub(in crate::runner) fn handled_work(&mut self, body: &[u8]) -> Option<Dispatch> {
        let m = self.seams.modules;
        m.handled_work(self, body)
    }

    /// Whether an ordered handler request's refusal is waiting: it fences ordered work.
    pub(in crate::runner) fn has_ordered_tool_refusal(&self) -> bool {
        self.seams.modules.has_ordered_tool_refusal(self)
    }

    /// Whether a handler's refused request may settle now.
    pub(in crate::runner) fn has_tool_refusal(&self, allow_ordered: bool) -> bool {
        self.seams.modules.has_tool_refusal(self, allow_ordered)
    }

    /// Whether a host's implementation answers some name of `extension`.
    pub(in crate::runner) fn implemented(&self, extension: &str) -> bool {
        self.seams.modules.implemented(self, extension)
    }

    /// The entries of `kind` whose title or description contains `matching`,
    /// case- and diacritic-insensitive, in the index's order (D9).
    pub fn index_entities(&self, kind: &str, matching: Option<&str>) -> Vec<Json> {
        self.seams.modules.index_entities(self, kind, matching)
    }

    /// An implementation's call of the app's handler, `{"extension",
    /// "handle", "handler", "args"}`: refused unless the call that made
    /// `handle` listed `handler`, else run; `reply` is given `{"text"}`,
    /// the handler's value as JSON, or `{"error", "message"}`, once.
    pub fn invoke_module(&mut self, body: &str, reply: Box<dyn FnOnce(String) + Send>) {
        let m = self.seams.modules;
        m.invoke_module(self, body, reply)
    }

    /// Whether `reference` is a listed entry's `ref`.
    pub fn is_listed(&self, reference: u64) -> bool {
        self.seams.modules.is_listed(self, reference)
    }

    /// Whether `ticket` is a handler's request.
    pub(in crate::runner) fn is_tool(&self, ticket: u64) -> bool {
        self.seams.modules.is_tool(self, ticket)
    }

    /// The session ends: each event still queued is journaled, unheard (D6).
    pub fn journal_unheard_events(&mut self) {
        let m = self.seams.modules;
        m.journal_unheard_events(self)
    }

    /// Request `ticket` runs on with no answer awaiting it (module work): its
    /// send is not cancelled, and its anchor goes with its reply.
    pub(in crate::runner) fn keep_send(&mut self, ticket: u64) {
        let m = self.seams.modules;
        m.keep_send(self, ticket)
    }

    /// The handles request `ticket`'s reply carried, as it lands: no longer
    /// provisional, the commit's to deliver.
    pub(in crate::runner) fn landed_handles(
        &mut self,
        ticket: u64,
    ) -> Option<(String, Vec<String>)> {
        let m = self.seams.modules;
        m.landed_handles(self, ticket)
    }

    /// Request `ticket`'s reply landed: its send is no longer cancelled.
    pub(in crate::runner) fn landed_send(&mut self, ticket: u64) {
        let m = self.seams.modules;
        m.landed_send(self, ticket)
    }

    /// A relaunched app's launch flag (`--notification '<entry>'`): the
    /// entry as the agent's state printed it, delivered as its tap would be
    /// (LLP 1133.004 §7), held until the session can hear it.
    pub fn launch_listed(&mut self, launch: &str, entry: &str) -> Result<String, String> {
        let m = self.seams.modules;
        m.launch_listed(self, launch, entry)
    }

    /// `--login`: this launch is the login item's (LLP 1133.025 §11), and
    /// `hidden` what its registration asked.
    pub fn launch_login(&mut self, hidden: bool) -> Result<String, String> {
        let m = self.seams.modules;
        m.launch_login(self, hidden)
    }

    /// A bound implementation's choice (module doc), applied as the
    /// person's; journaled when refused.
    pub(in crate::runner) fn listed_choice(&mut self, body: &str) {
        let m = self.seams.modules;
        m.listed_choice(self, body)
    }

    /// The agent clock reached `now_ms`: each entry of a list with a due
    /// time that is now due fires, once; a list another session changed is
    /// announced to this one's readers.
    pub(in crate::runner) fn listed_due(&mut self, now_ms: f64) {
        let m = self.seams.modules;
        m.listed_due(self, now_ms)
    }

    /// The agent's operations on listed entries (§7): `{"op":"tap","ticket":ref}`
    /// on one, or `{"op":"launchEntry","launch","entry"}`. `None` for any other.
    pub fn listed_op(&mut self, op: &str, request: &str) -> Option<Result<String, String>> {
        let m = self.seams.modules;
        m.listed_op(self, op, request)
    }

    /// What answers resource `i`, whose source `source` is a module's name.
    pub(in crate::runner) fn module_answer(
        &mut self,
        i: usize,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, RunnerError> {
        let m = self.seams.modules;
        m.module_answer(self, i, source, args)
    }

    /// The agent answered hold `hold`, already spent: §6's transitions.
    pub(in crate::runner) fn module_answered(&mut self, hold: u64, answer: &HoldAnswer) {
        let m = self.seams.modules;
        m.module_answered(self, hold, answer)
    }

    /// A call from data code (LLP 1133.002 D14): a native request whose body
    /// names a module's source or send, `{"extension": …, "source": …,
    /// "args": {…}}`, admitted here as a Contract read or send is, before any
    /// module. `None` for any other body.
    pub(in crate::runner) fn module_call(&mut self, native: &crate::Request) -> Option<Dispatch> {
        let m = self.seams.modules;
        m.module_call(self, native)
    }

    /// A typed answer to `ticket`, checked before the hold is spent, so it
    /// can be corrected. `None` when the hold is not a module's.
    pub(in crate::runner) fn module_check(
        &self,
        ticket: u64,
        text: &str,
    ) -> Option<Result<(), String>> {
        self.seams.modules.module_check(self, ticket, text)
    }

    /// Value tickets whose timeout has passed on the agent's clock (§6):
    /// each fails `TIMEOUT`, its ticket retired.
    pub(in crate::runner) fn module_deadlines(&mut self, now_ms: f64) {
        let m = self.seams.modules;
        m.module_deadlines(self, now_ms)
    }

    /// Each list and value the plan's modules declare, by its state name:
    /// [`Self::declared_state`]'s module part.
    pub(in crate::runner) fn module_declared_state(&self) -> BTreeMap<String, Json> {
        self.seams.modules.module_declared_state(self)
    }

    /// The host dispatches held module request `token`: answered already,
    /// or held until the agent answers. `None` for a data source's token.
    pub(in crate::runner) fn module_dispatch(&mut self, token: u64) -> Option<Dispatch> {
        let m = self.seams.modules;
        m.module_dispatch(self, token)
    }

    /// The reply to a held module request, as `fulfill` parses it.
    pub(in crate::runner) fn module_parse(
        &mut self,
        target: Target,
        ty: TypesId,
        source: &str,
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let m = self.seams.modules;
        m.module_parse(self, target, ty, source, outcome)
    }

    /// Held module requests the agent answered since the host dispatched them.
    pub(in crate::runner) fn module_release(&mut self) -> Vec<(u64, Dispatch)> {
        let m = self.seams.modules;
        m.module_release(self)
    }

    /// What answers a send (`target`) to `source`, a module's name.
    pub(in crate::runner) fn module_send(
        &mut self,
        target: Target,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        let m = self.seams.modules;
        m.module_send(self, target, source, args)
    }

    /// The topic a data-code read of a module's source watches, its key.
    pub(in crate::runner) fn module_topic(&self, body: &[u8]) -> Option<String> {
        self.seams.modules.module_topic(self, body)
    }

    /// Whether invocation `id` owns module work: a native statement or a
    /// held call stamped with it ([`Self::invocation_owns`]).
    pub(in crate::runner) fn modules_own(&self, id: u64) -> bool {
        self.seams.modules.modules_own(self, id)
    }

    /// The modules' part of [`Self::collection_output_quiet`]: no package
    /// view, fed canvas or held call.
    pub(in crate::runner) fn modules_quiet(&self) -> bool {
        self.seams.modules.modules_quiet(self)
    }

    /// Request `ticket` of `source` went out with `body`: a reply to a name
    /// that declares handles is provisional until it lands.
    pub(in crate::runner) fn note_handles(&mut self, ticket: u64, source: &str, body: &[u8]) {
        let m = self.seams.modules;
        m.note_handles(self, ticket, source, body)
    }

    /// Request `ticket` went out with `body`: a send to an implementation
    /// is kept by its number until its reply lands.
    pub(in crate::runner) fn note_send(&mut self, ticket: u64, body: &[u8]) {
        let m = self.seams.modules;
        m.note_send(self, ticket, body)
    }

    /// `request`, about to be handed out, numbered when it is data code's
    /// call of one of this plan's module sends that has no number yet.
    pub(in crate::runner) fn number_data_call(&mut self, request: &mut Request) {
        let m = self.seams.modules;
        m.number_data_call(self, request)
    }

    /// The schemes the plan's interfaces serve pictures by, each once: the
    /// `img` sources a host's raster loader gives [`Runner::admit_picture`]
    /// rather than opening as files.
    pub fn picture_schemes(&self) -> Vec<String> {
        self.seams.modules.picture_schemes(self)
    }

    /// After an advance: each polled source whose period passed and whose
    /// last ask has landed is asked again, once.
    pub(in crate::runner) fn poll(&mut self) -> Result<Vec<CommitReceipt>, RunnerError> {
        let m = self.seams.modules;
        m.poll(self)
    }

    /// When the next poll is due, for a host on the live clock to wake at.
    pub(in crate::runner) fn poll_due_ms(&self) -> Option<f64> {
        self.seams.modules.poll_due_ms(self)
    }

    /// Whether a polled source's ask is in flight, or a stale answer's
    /// (`stale.rs`): what an agent's jump waits for before it replies.
    pub fn polls_asking(&self) -> bool {
        self.seams.modules.polls_asking(self)
    }

    /// An event a bound implementation emitted: the device's origin, whatever
    /// it claims.
    pub(in crate::runner) fn raise_emitted(&mut self, body: &str) {
        let m = self.seams.modules;
        m.raise_emitted(self, body)
    }

    /// The host raises event `name` of `extension` with `args`, a JSON
    /// array: `origin` is [`CallOrigin::App`], a person's, only for an
    /// activation the host's own platform callback reported. Queued for
    /// [`Runner::apply_announced`]; `Err` for an event the plan does not
    /// know or arguments that are not an array.
    pub fn raise_module_event(
        &mut self,
        extension: &str,
        name: &str,
        args: &str,
        origin: CallOrigin,
    ) -> Result<(), String> {
        let m = self.seams.modules;
        m.raise_module_event(self, extension, name, args, origin)
    }

    /// The host refused a handler's request at admission: it settles in
    /// its turn, as a failure, an ordered one only once earlier ordered work
    /// has.
    pub(in crate::runner) fn refuse_tool(
        &mut self,
        ticket: u64,
        reason: &'static str,
        ordered: bool,
    ) {
        let m = self.seams.modules;
        m.refuse_tool(self, ticket, reason, ordered)
    }

    /// The substitute's readers no resource holds any longer are let go, with
    /// their grids and cursors (a released source's acquisition stops).
    pub(in crate::runner) fn release_sampled(&mut self) {
        let m = self.seams.modules;
        m.release_sampled(self)
    }

    /// Report each package view's change since the last report (module doc).
    /// A host calls this again after it changes what is displayed without a
    /// commit (a cover set as it lays the commit out), as it reads the wake
    /// lock's holders.
    pub fn report_views(&mut self) {
        let m = self.seams.modules;
        m.report_views(self)
    }

    /// Held module requests whose request is gone (superseded, its reader
    /// released, a refused commit): retired with their tickets (§6).
    pub(in crate::runner) fn retire_modules(&mut self) {
        let m = self.seams.modules;
        m.retire_modules(self)
    }

    /// Run the effects a commit stated, in order, once it stands.
    pub(in crate::runner) fn run_module_effects(&mut self, effects: Vec<(ModulesRow, Vec<Value>)>) {
        let m = self.seams.modules;
        m.run_module_effects(self, effects)
    }

    /// The commit was refused: its answers delivered nothing.
    pub(in crate::runner) fn samples_refused(&mut self) {
        let m = self.seams.modules;
        m.samples_refused(self)
    }

    /// The commit stood: the cursors of the answers it delivered hold.
    pub(in crate::runner) fn samples_stood(&mut self) {
        let m = self.seams.modules;
        m.samples_stood(self)
    }

    /// The page was hidden or shown at the session clock's now.
    pub(in crate::runner) fn samples_visibility(&mut self, hidden: bool) {
        let m = self.seams.modules;
        m.samples_visibility(self, hidden)
    }

    /// Share the host's app-wide state with this session (§7: `"scope":
    /// "app"`), before anything sets it.
    pub fn share_app_state(&mut self, state: AppState) {
        let m = self.seams.modules;
        m.share_app_state(self, state)
    }

    /// The soonest span's end, for the clock's next wake.
    pub(in crate::runner) fn span_due(&self) -> Option<f64> {
        self.seams.modules.span_due(self)
    }

    /// What the substitute's canvas inputs owe their surfaces up to `to_ms`
    /// on the session clock, oldest first: each a surface's name and its
    /// input event as the hosts write it, `{"t":"sensor","sensor","values",
    /// "dropped","at"}` or `{"t":"sensorEnded","sensor","at"}` (D6). Values
    /// are the declared `values` fields of the sensor's source, a list one
    /// spread, padded with zeros to four.
    pub fn surface_sensor_events(&mut self, to_ms: f64) -> Vec<(String, String)> {
        let m = self.seams.modules;
        m.surface_sensor_events(self, to_ms)
    }

    /// Whether the substitute feeds a canvas input: a host advancing the
    /// agent's clock then steps each world a display frame at a time.
    pub fn surface_sensor_inputs(&self) -> bool {
        self.seams.modules.surface_sensor_inputs(self)
    }

    /// The commands of this commit from `first` that are a module's sends
    /// answering nothing: taken off, to run once the commit stands.
    pub(in crate::runner) fn take_module_effects(
        &mut self,
        first: usize,
    ) -> Vec<(ModulesRow, Vec<Value>)> {
        let m = self.seams.modules;
        m.take_module_effects(self, first)
    }

    /// `body`, a reply to live source `key`, without its stale time, which
    /// is noted: a list answer's envelope is unwrapped when nothing else is
    /// left in it. A body that names none is returned as it came.
    pub(in crate::runner) fn take_stale(&mut self, key: &str, body: Vec<u8>) -> Vec<u8> {
        let m = self.seams.modules;
        m.take_stale(self, key, body)
    }

    /// `tap @ref` on a listed entry (§7): what the platform's click would
    /// do. `Ok` is the reply: `{"navigate": location}` for the host to open,
    /// or the event raised; the entry is retired either way.
    pub fn tap_listed(&mut self, reference: u64) -> Result<String, String> {
        let m = self.seams.modules;
        m.tap_listed(self, reference)
    }

    /// [`Self::tap_listed`] with the person's choice, for an entry whose
    /// list declares choices (LLP 1133.030 D12 item 2).
    pub fn tap_listed_as(
        &mut self,
        reference: u64,
        choice: Option<&str>,
    ) -> Result<String, String> {
        let m = self.seams.modules;
        m.tap_listed_as(self, reference, choice)
    }

    /// The app went to the foreground or left it: each implementation that
    /// answers here is told, `{"extension", "foreground": bool}`, as the
    /// fact admission reads (LLP 1133.009 D10 item 3). An implementation
    /// begins in the foreground.
    pub(in crate::runner) fn tell_foreground(&self) {
        self.seams.modules.tell_foreground(self)
    }

    /// [`Runner`]'s `tool_completion`, answered by the modules.
    pub(in crate::runner) fn tool_completion(
        &mut self,
        ticket: u64,
    ) -> Option<&mut Option<crate::CompletionReply>> {
        let m = self.seams.modules;
        m.tool_completion(self, ticket)
    }

    /// Whether a handler's request with effects is in flight, one the host
    /// refused at admission aside.
    pub(in crate::runner) fn tool_effects(&self) -> bool {
        self.seams.modules.tool_effects(self)
    }

    /// A handler's request came back: its turn resumes.
    pub(in crate::runner) fn tool_landed(&mut self, ticket: u64, outcome: Outcome) {
        let m = self.seams.modules;
        m.tool_landed(self, ticket, outcome)
    }

    /// The least privileged origin of a handler running now, if one is.
    pub(in crate::runner) fn tool_origin(&self) -> Option<CallOrigin> {
        self.seams.modules.tool_origin(self)
    }

    /// A handler's refused request that may settle now.
    pub(in crate::runner) fn tool_refusal(
        &mut self,
        allow_ordered: bool,
    ) -> Option<(u64, &'static str)> {
        let m = self.seams.modules;
        m.tool_refusal(self, allow_ordered)
    }

    /// The handlers' requests in flight, for `pending` and `in_flight`.
    pub(in crate::runner) fn tool_tickets(&self) -> Vec<(String, u64)> {
        self.seams.modules.tool_tickets(self)
    }

    /// The origin the runner reported for view `id`'s displayed state, or for
    /// its binding `prop`: what an acquisition the view makes is admitted
    /// with (LLP 1133.008 D3).
    pub fn view_origin(&self, id: ViewId, prop: Option<&str>) -> Option<CallOrigin> {
        self.seams.modules.view_origin(self, id, prop)
    }

    /// After a commit is applied: its origin is the one a change it made is
    /// reported with.
    pub(in crate::runner) fn views_committed(&mut self) {
        let m = self.seams.modules;
        m.views_committed(self)
    }

    /// Entry `reference`'s invocation ended `state` (module doc).
    pub(in crate::runner) fn wake_entry_ended(
        &mut self,
        reference: u64,
        state: &str,
        delivered: bool,
    ) {
        let m = self.seams.modules;
        m.wake_entry_ended(self, reference, state, delivered)
    }

    /// Entry `reference`'s invocation was delivered.
    pub(in crate::runner) fn wake_entry_running(&mut self, reference: u64) {
        let m = self.seams.modules;
        m.wake_entry_running(self, reference)
    }

    /// A module row's substitute, as its declaration reads on this target.
    pub(in crate::runner) fn module_substitute(&self, row: &ModulesRow) -> Arc<Json> {
        self.seams.modules.module_substitute(self, row)
    }

    /// What a module row answers now, by its substitute.
    pub(in crate::runner) fn module_value_of(&self, row: &ModulesRow) -> Option<Json> {
        self.seams.modules.module_value_of(self, row)
    }

    /// A declared permission's state.
    pub(in crate::runner) fn module_permission_state(&self, name: &str) -> String {
        self.seams.modules.module_permission_state(self, name)
    }

    /// A declared permission's state is set.
    pub(in crate::runner) fn set_module_permission(&mut self, name: &str, state: &str) {
        let m = self.seams.modules;
        m.set_module_permission(self, name, state)
    }

    /// A module row's topic: `extension.name`, with its owner.
    pub(in crate::runner) fn module_key(&self, row: &ModulesRow) -> String {
        self.seams.modules.module_key(self, row)
    }

    /// What a request asked under `origin` carries: under the agent, with the
    /// app's local-network permission answered by the substitute, that
    /// permission's state stands for the host's fact (LLP 1133.036).
    pub(in crate::runner) fn reach_of(&self, origin: CallOrigin) -> Reach {
        Reach {
            origin,
            substitute: self.seams.modules.local_network_substitute(self),
        }
    }

    /// Whether `token` is a held module request (not I/O: no clock waits).
    pub(in crate::runner) fn holds_token(&self, token: Option<u64>) -> bool {
        self.seams.modules.holds_token(self, token)
    }

    /// The module readers as a commit found them, for its checkpoint.
    pub(in crate::runner) fn readers_saved(&self) -> Readers {
        self.seams.modules.readers_saved(self)
    }

    /// A refused commit's: the module readers as it found them.
    pub(in crate::runner) fn readers_restore(&mut self, saved: Readers) {
        let m = self.seams.modules;
        m.readers_restore(self, saved)
    }

    /// The host's target (`ios`, `macos`, `android`, `web`), which a name's
    /// `owned` and an interface's `images.owned` are by.
    /// A change asks every module name again, since its answers were by the
    /// target before (`targets`, a permission's `targets`).
    pub fn set_module_target(&mut self, target: &str) {
        let same = self.host_target() == target;
        self.module_target = Some(target.to_owned());
        if !same {
            let m = self.seams.modules;
            m.retarget(self)
        }
    }

    /// The target a declaration's per-target words are read by: the one the
    /// host said, else the one this runner was built for.
    pub(in crate::runner) fn host_target(&self) -> &str {
        match self.module_target.as_deref() {
            Some(target) => target,
            None => build_target(),
        }
    }

    /// Whether the target places a `-exact-menu="status"` button outside
    /// the window: macOS, in the menu bar, and Windows, in the notification
    /// area (LLP 1133.025 D4).
    pub fn places_status_items(&self) -> bool {
        matches!(self.host_target(), "macos" | "windows")
    }
}
