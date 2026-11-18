//! What the core composes from the runner's optional parts (LLP 1047.001
//! D11): the native modules, tool calls, presence secrets and font-axis
//! motion each answer their own part, and the core joins them, so that a
//! part can be absent.
//!
//! Each part is a seam: a trait whose methods take the runner, implemented
//! by a unit struct of the part (`LinkedTools`), which only an entry's
//! [`ModuleLinks`] names, so a binary that does not link the part holds
//! none of its code. A seam's provided methods are what the core does
//! without the part, which is what an empty table did before: the boot
//! uses them whenever the plan has none of the part's rows. Font-axis
//! motion is chosen by the host alone, by whether it plays transitions and
//! animations.

use super::calls::{CallHow, LinkedTools};
use super::module_seam::{Modules, NoModules};
use super::modules::LinkedModules;
use super::presence_secrets::{presence_body, LinkedPresence, PresenceAnswer, PresenceAsk};
use super::reply::response;
use super::text_axes::LinkedTextAxes;
use super::{CommitReceipt, DataSource, Runner, RunnerError};
use crate::presence::{Backend, Occupancy};
use crate::request::{Dispatch, Work};
use crate::{HoldAnswer, StoreWrite};
use exact_plan::ModulesRow;
use serde_json::{json, Value as Json};
use std::any::Any;

/// What a runner links of its optional parts, chosen by an entry's
/// composition (LLP 1047.001 D11): every part for the plain boots
/// ([`ModuleLinks::ALL`]).
pub struct ModuleLinks<D: DataSource> {
    /// Native modules (LLP 1133.002), for a plan with module, view or event
    /// rows.
    pub modules: &'static dyn Modules<D>,
    /// Tool calls (LLP 1111), for a plan with tools.
    pub tools: &'static dyn Tools<D>,
    /// Locked secrets (LLP 1133.010), for grants with a presence policy.
    pub presence: &'static dyn PresenceSecrets<D>,
    /// The font rows the runner moves itself (LLP 1019 §13 D10), for a
    /// host that plays transitions and animations; they run only where
    /// [`super::RunnerLinks::text_axes`] makes their engine too.
    pub text_axes: &'static dyn TextAxes<D>,
}

impl<D: DataSource> Clone for ModuleLinks<D> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<D: DataSource> Copy for ModuleLinks<D> {}

impl<D: DataSource> ModuleLinks<D> {
    /// Every part.
    pub const ALL: ModuleLinks<D> = ModuleLinks {
        modules: &LinkedModules,
        tools: &LinkedTools,
        presence: &LinkedPresence,
        text_axes: &LinkedTextAxes,
    };

    /// No part: the core alone.
    pub const NONE: ModuleLinks<D> = ModuleLinks {
        modules: &NoModules,
        tools: &NoTools,
        presence: &NoPresence,
        text_axes: &NoTextAxes,
    };

    /// The parts `plan` uses of these, `presence` whether its store keeps a
    /// name with a presence policy: a part the plan does not use is left
    /// out, so the core answers as it would with the part's table empty.
    /// Font-axis motion is the host's choice alone, kept as it is.
    pub(in crate::runner) fn for_plan(self, plan: &exact_plan::Plan, presence: bool) -> Self {
        let modules =
            plan.modules.is_empty() && plan.views.is_empty() && plan.module_events.is_empty();
        ModuleLinks {
            modules: match modules {
                true => &NoModules,
                false => self.modules,
            },
            tools: match plan.tools.is_empty() {
                true => &NoTools,
                false => self.tools,
            },
            presence: match presence {
                true => self.presence,
                false => &NoPresence,
            },
            text_axes: self.text_axes,
        }
    }
}

/// The tool seam (LLP 1047.001 D11 item 3): the calls a door hands the
/// runner (`calls.rs`). Each provided method is the answer with no tool.
pub trait Tools<D: DataSource> {
    /// A door's call of `tool`: [`Runner::call_tool`].
    fn call_tool(&self, r: &mut Runner<D>, tool: &str, _: &str, _: CallHow) -> Result<u64, String> {
        let _ = r;
        Err(format!("no tool `{tool}`: this app declares none"))
    }
    /// Whether a call is between dispatch and its end.
    fn call_running(&self, _: &Runner<D>) -> bool {
        false
    }
    /// Run what is due of the calls.
    fn step_calls(&self, _: &mut Runner<D>) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        (Vec::new(), None)
    }
    /// Invocation `id`'s work was let go of by a commit that stood.
    fn call_superseded(&self, _: &mut Runner<D>, _: u64, _: bool) {}
    /// A name that needs a person was called by the running call's work.
    fn call_person_required(&self, _: &mut Runner<D>) {}
    /// Take off this commit's commands from `first` that a call does not run.
    fn skip_in_call(&self, _: &mut Runner<D>, _: usize) {}
    /// Whether a running call is in a session no one sees.
    fn call_stops_timers(&self, _: &Runner<D>) -> bool {
        false
    }
    /// When the host should next advance for the calls.
    fn call_due(&self, _: &Runner<D>) -> Option<f64> {
        None
    }
    /// Call `id`'s caller cancelled it.
    fn cancel_call(&self, _: &mut Runner<D>, id: u64) -> Result<String, String> {
        Err(format!("no call {id}"))
    }
    /// Call `id` as its door polls it.
    fn call_answer(&self, _: &Runner<D>, id: u64) -> Result<String, String> {
        Err(format!("no call {id}"))
    }
    /// `state.intents`.
    fn intents_state(&self, _: &Runner<D>) -> Option<Json> {
        None
    }
}

/// The tool seam, not linked.
struct NoTools;

impl<D: DataSource> Tools<D> for NoTools {}

/// The font-axis seam's state, owned by the runner as the native modules'
/// is ([`super::module_seam::ModuleState`]): made by the linked seam
/// ([`TextAxes::state`]) and reached only by its code (`text_axes.rs`).
pub(in crate::runner) struct TextAxesState(pub(in crate::runner) Box<dyn Any>);

/// The font-axis seam (LLP 1019 §13 D10, `text_axes.rs`): the runner's own
/// engine for `font-weight`, `font-stretch` and `font-variation-settings`
/// transitions and animations. Each provided method is the answer with
/// none: nothing moves. What a commit's ops change is read before they
/// apply ([`TextAxes::prepare`]) as a value only the seam reads.
pub trait TextAxes<D: DataSource> {
    /// The state for a new runner, which it keeps: none here. A seam's other
    /// methods run on the state its own `state` made, so an implementation
    /// that delegates them delegates this too.
    fn state(&self) -> Box<dyn Any> {
        Box::new(())
    }
    /// Whether a font axis is moving.
    fn running(&self, _: &Runner<D>) -> bool {
        false
    }
    /// When the last one ends, milliseconds.
    fn settle_ms(&self, _: &Runner<D>) -> Option<f64> {
        None
    }
    /// Before `ops` apply: what they change.
    fn prepare(&self, _: &Runner<D>, _: &[exact_kernel::Op]) -> Option<Box<dyn Any>> {
        None
    }
    /// Two phases' [`TextAxes::prepare`]s of one commit, joined; `created`
    /// are the views an earlier phase created.
    fn join(
        &self,
        first: Option<Box<dyn Any>>,
        later: Option<Box<dyn Any>>,
        _created: &std::collections::BTreeSet<exact_kernel::ViewId>,
    ) -> Option<Box<dyn Any>> {
        first.or(later)
    }
    /// After a commit applied: what it moves, written as one more commit.
    fn settle(
        &self,
        _: &mut Runner<D>,
        _before: Option<Box<dyn Any>>,
        _: &CommitReceipt,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
    /// Each moving font row at the clock, one commit for all of them.
    fn tick(&self, _: &mut Runner<D>) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
}

/// No font-axis motion: [`TextAxes`]' provided methods.
pub struct NoTextAxes;
impl<D: DataSource> TextAxes<D> for NoTextAxes {}

impl<D: DataSource> Runner<D> {
    pub(in crate::runner) fn text_axes_running(&self) -> bool {
        let t = self.seams.text_axes;
        t.running(self)
    }

    /// When the last font-axis transition or finite animation ends,
    /// milliseconds; a host's `settle` takes the later of this and its own
    /// engine's.
    pub fn text_axes_settle_ms(&self) -> Option<f64> {
        let t = self.seams.text_axes;
        t.settle_ms(self)
    }

    pub(in crate::runner) fn prepare_text_axes(
        &self,
        ops: &[exact_kernel::Op],
    ) -> Option<Box<dyn Any>> {
        let t = self.seams.text_axes;
        t.prepare(self, ops)
    }

    pub(in crate::runner) fn join_text_axes(
        &self,
        first: Option<Box<dyn Any>>,
        later: Option<Box<dyn Any>>,
        created: &std::collections::BTreeSet<exact_kernel::ViewId>,
    ) -> Option<Box<dyn Any>> {
        let t = self.seams.text_axes;
        t.join(first, later, created)
    }

    pub(in crate::runner) fn settle_text_axes(
        &mut self,
        before: Option<Box<dyn Any>>,
        receipt: &CommitReceipt,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let t = self.seams.text_axes;
        t.settle(self, before, receipt)
    }

    pub(in crate::runner) fn tick_text_axes(
        &mut self,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let t = self.seams.text_axes;
        t.tick(self)
    }
}

/// The presence seam's state, owned by the runner as the native modules'
/// is ([`super::module_seam::ModuleState`]): its type is the presence
/// code's own, made by the linked seam ([`PresenceSecrets::state`]) and
/// reached only by that code (`presence_secrets.rs`), so a binary without
/// presence holds none of its code, its drop included.
pub(in crate::runner) struct PresenceState(pub(in crate::runner) Box<dyn Any>);

/// The presence seam (LLP 1047.001 D11 item 4): locked secrets, the
/// person's check and the host's backend (`presence_secrets.rs`). Each
/// provided method is the answer with no presence name: nothing is kept
/// locked, and a store's presence call is refused.
pub trait PresenceSecrets<D: DataSource> {
    /// The presence state for a new runner, which it keeps: none here. A
    /// seam's other methods run on the state its own `state` made, so an
    /// implementation that delegates them delegates this too.
    fn state(&self) -> Box<dyn Any> {
        Box::new(())
    }
    /// [`Runner::install_presence`].
    fn install_presence(&self, _: &mut Runner<D>, _: Backend, _: Option<String>) {}
    /// Whether this session keeps locked secrets.
    fn presence_secrets(&self, _: &Runner<D>) -> bool {
        false
    }
    /// The local-auth module's status answered, with its presence facts.
    fn presence_status(&self, _: &mut Runner<D>, _: &ModulesRow, answer: Json) -> Json {
        answer
    }
    /// The local-auth implementation was bound.
    fn presence_rebound(&self, _: &mut Runner<D>) {}
    /// [`Runner::take_presence_asks`].
    fn take_presence_asks(&self, _: &mut Runner<D>) -> Vec<PresenceAsk> {
        Vec::new()
    }
    /// [`Runner::take_presence_asks_json`].
    fn take_presence_asks_json(&self, _: &mut Runner<D>) -> Vec<String> {
        Vec::new()
    }
    /// [`Runner::presence_answer`].
    fn presence_answer(&self, _: &mut Runner<D>, _: u64, _: PresenceAnswer) {}
    /// [`Runner::presence_reply`].
    fn presence_reply(&self, _: &mut Runner<D>, _: &[u8]) {}
    /// A request goes out.
    fn presence_noted(&self, _: &mut Runner<D>, _: u64, _: &crate::Request) {}
    /// Whether request `ticket` is a presence call waiting on a ticket.
    fn presence_holds(&self, _: &Runner<D>, _: u64) -> bool {
        false
    }
    /// A store's presence call: with no presence name, refused by its name.
    fn presence_call(&self, _: &mut Runner<D>, body: &[u8]) -> Option<Dispatch> {
        let (_, name, _) = presence_body(body)?;
        let message = format!("{name} is not a secret.keep name with a presence policy");
        let answer = json!({ "error": "NotSupportedError", "message": message });
        Some(Dispatch::Run(Work::Now(Box::new(move || {
            response(&answer)
        }))))
    }
    /// Committed writes for the host, a presence name's taken out.
    fn presence_writes(&self, _: &mut Runner<D>, writes: Vec<StoreWrite>) -> Vec<StoreWrite> {
        writes
    }
    /// The data source is ready.
    fn presence_ready(&self, _: &mut Runner<D>) {}
    /// When the backend next retries.
    fn presence_due_ms(&self, _: &Runner<D>) -> Option<f64> {
        None
    }
    /// Retry at the clock the advance reached.
    fn tick_presence(&self, _: &mut Runner<D>) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }
    /// Run what the backend can run now.
    fn pump_presence(&self, _: &mut Runner<D>) {}
    /// The agent answered a presence hold.
    fn presence_answered(&self, _: &mut Runner<D>, _: u64, _: &HoldAnswer) {}
    /// The backend's occupancy, adopted into the store.
    fn adopt_presence(&self, _: &mut Runner<D>) {}
}

/// The presence seam, not linked.
struct NoPresence;

impl<D: DataSource> PresenceSecrets<D> for NoPresence {}

impl<D: DataSource> Runner<D> {
    /// Whether nothing outside the collections waits to be output: no
    /// package view, fed canvas or held module call, no process work on the
    /// agent's clock, and no wake, tool call or presence secret due.
    pub(in crate::runner) fn collection_output_quiet(&self) -> bool {
        self.modules_quiet()
            && !self.has_work_clock()
            && self.wake_due().is_none()
            && self.call_due().is_none()
            && self.presence_due_ms().is_none()
    }

    /// Each list and value the plan's modules declare, by its state name,
    /// as the agent's state shows it (`"notifications": [{…, "ref": 7,
    /// "delivered": true}]`, `"badge": 3`), and the app's tools with the
    /// system's calls of them as `intents` (LLP 1133.023 §10). Empty for a
    /// plan that declares none.
    pub fn declared_state(&self) -> Vec<(String, Json)> {
        let mut out = self.module_declared_state();
        if let Some(intents) = self.intents_state() {
            out.insert("intents".into(), intents);
        }
        out.into_iter().collect()
    }

    /// [`Self::declared_state`] as the agent's state writes it: `,"name":value` each.
    pub fn declared_state_json(&self, s: &mut String) {
        for (name, value) in self.declared_state() {
            s.push(',');
            s.push_str(&Json::String(name).to_string());
            s.push(':');
            s.push_str(&value.to_string());
        }
    }

    /// A door's call of `tool` with `args`, a JSON object: queued, and its
    /// invocation id answered. `Err` for a tool the plan does not name.
    pub fn call_tool(&mut self, tool: &str, args: &str, how: CallHow) -> Result<u64, String> {
        let t = self.links_tools();
        t.call_tool(self, tool, args, how)
    }

    pub(in crate::runner) fn call_running(&self) -> bool {
        self.links_tools().call_running(self)
    }

    pub(in crate::runner) fn step_calls(&mut self) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        let t = self.links_tools();
        t.step_calls(self)
    }

    pub(in crate::runner) fn call_superseded(&mut self, id: u64, send: bool) {
        let t = self.links_tools();
        t.call_superseded(self, id, send)
    }

    pub(in crate::runner) fn call_person_required(&mut self) {
        let t = self.links_tools();
        t.call_person_required(self)
    }

    pub(in crate::runner) fn skip_in_call(&mut self, first: usize) {
        let t = self.links_tools();
        t.skip_in_call(self, first)
    }

    pub(in crate::runner) fn call_stops_timers(&self) -> bool {
        self.links_tools().call_stops_timers(self)
    }

    pub(in crate::runner) fn call_due(&self) -> Option<f64> {
        self.links_tools().call_due(self)
    }

    fn cancel_call(&mut self, id: u64) -> Result<String, String> {
        let t = self.links_tools();
        t.cancel_call(self, id)
    }

    fn call_answer(&self, id: u64) -> Result<String, String> {
        self.links_tools().call_answer(self, id)
    }

    /// `state.intents`: each tool with its word, modifiers, availability
    /// and face, and the calls running, waiting and ended, never their
    /// arguments (LLP 1133.023 §10).
    pub fn intents_state(&self) -> Option<Json> {
        self.links_tools().intents_state(self)
    }

    fn links_tools(&self) -> &'static dyn Tools<D> {
        self.seams.tools
    }

    /// The agent's `call` (`{"op":"call","tool","args","door"}`, queued
    /// as the door would hand it), a launch's `--call '<tool> <json>'`
    /// (`{"op":"launchEntry","launch":"call","entry"}`: a cold call in a
    /// session no one sees, as a system's call boots one), a door's poll of
    /// its call (`{"op":"called","id"}`), its caller's cancellation
    /// (`{"op":"cancel","id"}`). `None` for any other op.
    pub fn call_agent(&mut self, op: &str, request: &str) -> Option<Result<String, String>> {
        let v: Json = serde_json::from_str(request).ok()?;
        match op {
            "launchEntry" if v["launch"] == "call" => {
                let entry = v["entry"].as_str().unwrap_or_default().trim();
                let (tool, args) = entry
                    .split_once(char::is_whitespace)
                    .unwrap_or((entry, "{}"));
                let how = CallHow {
                    door: v["door"].as_str().unwrap_or("agent").to_owned(),
                    cold: true,
                    hidden: true,
                    digest: None,
                };
                self.log(format!("launch --call ({tool})"));
                let id = self.call_tool(tool, args.trim(), how);
                Some(id.map(|id| json!({ "call": id, "tool": tool, "cold": true }).to_string()))
            }
            "call" => {
                let tool = v["tool"].as_str().unwrap_or_default().to_owned();
                let args = match &v["args"] {
                    Json::String(text) => text.clone(),
                    args => args.to_string(),
                };
                let how = CallHow {
                    door: v["door"].as_str().unwrap_or("agent").to_owned(),
                    cold: v["cold"] == true,
                    hidden: v["hidden"] == true,
                    digest: v["digest"].as_str().map(str::to_owned),
                };
                let id = self.call_tool(&tool, &args, how);
                Some(
                    id.map(|id| {
                        json!({ "call": id, "tool": tool, "state": "waiting" }).to_string()
                    }),
                )
            }
            "called" => Some(match v["id"].as_u64() {
                Some(id) => self.call_answer(id),
                None => Err("called needs the call's id".into()),
            }),
            "cancel" => Some(match v["id"].as_u64() {
                Some(id) => self.cancel_call(id),
                None => Err("cancel needs the call's id".into()),
            }),
            _ => None,
        }
    }
}

impl<D: DataSource> Runner<D> {
    /// The host's presence backend for this session, reconciled before boot
    /// (its occupancy is in the boot snapshot). The runner runs it from now
    /// on: its queue, the checks, and every committed write of its names.
    /// `keystore`: where the platform's keystore keeps the items, when the
    /// vault is it, so a check the host makes reads the item itself.
    pub fn install_presence(&mut self, backend: Backend, keystore: Option<String>) {
        let p = self.seams.presence;
        p.install_presence(self, backend, keystore)
    }

    pub(in crate::runner) fn presence_secrets(&self) -> bool {
        self.seams.presence.presence_secrets(self)
    }

    pub(in crate::runner) fn presence_status(&mut self, row: &ModulesRow, answer: Json) -> Json {
        let p = self.seams.presence;
        p.presence_status(self, row, answer)
    }

    pub(in crate::runner) fn presence_rebound(&mut self) {
        let p = self.seams.presence;
        p.presence_rebound(self)
    }

    /// The checks the host's platform makes, since the last take.
    pub fn take_presence_asks(&mut self) -> Vec<PresenceAsk> {
        let p = self.seams.presence;
        p.take_presence_asks(self)
    }

    /// [`Runner::take_presence_asks`], each as its JSON
    /// ([`PresenceAsk::json`]), for a host that only sends them on.
    pub fn take_presence_asks_json(&mut self) -> Vec<String> {
        let p = self.seams.presence;
        p.take_presence_asks_json(self)
    }

    /// How a check the host made ended. A call let go meanwhile is dropped.
    pub fn presence_answer(&mut self, id: u64, answer: PresenceAnswer) {
        let p = self.seams.presence;
        p.presence_answer(self, id, answer)
    }

    /// How a check the host made ended, as a host says it:
    /// `{"call", "status": "passed"|"read"|"failed", "value"?, "code"?,
    /// "message"?}`, a `read` without a value being an item the platform
    /// dropped. The value goes to the waiting call alone; never journaled.
    pub fn presence_reply(&mut self, json: &[u8]) {
        let p = self.seams.presence;
        p.presence_reply(self, json)
    }

    pub(in crate::runner) fn presence_noted(&mut self, ticket: u64, request: &crate::Request) {
        let p = self.seams.presence;
        p.presence_noted(self, ticket, request)
    }

    pub(in crate::runner) fn presence_holds(&self, ticket: u64) -> bool {
        self.seams.presence.presence_holds(self, ticket)
    }

    pub(in crate::runner) fn presence_call(&mut self, body: &[u8]) -> Option<Dispatch> {
        let p = self.seams.presence;
        p.presence_call(self, body)
    }

    pub(in crate::runner) fn presence_writes(
        &mut self,
        writes: Vec<StoreWrite>,
    ) -> Vec<StoreWrite> {
        let p = self.seams.presence;
        p.presence_writes(self, writes)
    }

    pub(in crate::runner) fn presence_ready(&mut self) {
        let p = self.seams.presence;
        p.presence_ready(self)
    }

    pub(in crate::runner) fn presence_due_ms(&self) -> Option<f64> {
        self.seams.presence.presence_due_ms(self)
    }

    pub(in crate::runner) fn tick_presence(
        &mut self,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        let p = self.seams.presence;
        p.tick_presence(self)
    }

    pub(in crate::runner) fn pump_presence(&mut self) {
        let p = self.seams.presence;
        p.pump_presence(self)
    }

    pub(in crate::runner) fn presence_answered(&mut self, hold: u64, answer: &HoldAnswer) {
        let p = self.seams.presence;
        p.presence_answered(self, hold, answer)
    }

    pub(in crate::runner) fn adopt_presence(&mut self) {
        let p = self.seams.presence;
        p.adopt_presence(self)
    }

    /// The presence names, each with what the agent's `state` says of it:
    /// `locked, user-presence`, `pending`, `plain` or `absent`, never a value
    /// (§7, Disclosure).
    pub fn presence_list(&self) -> Vec<(String, String)> {
        (self.store.presence_names().into_iter())
            .map(|(name, presence)| {
                let said = match self.store.occupancy(name).unwrap_or(Occupancy::Absent) {
                    Occupancy::Locked => format!("locked, {}", presence.policy.word()),
                    other => other.word().to_owned(),
                };
                (name.to_owned(), said)
            })
            .collect()
    }

    /// `key`'s text in the strings table the app shows now, or the key.
    pub(in crate::runner) fn strings_text(&self, key: &str) -> String {
        (self.plan.localized(self.resolved_locale(), key))
            .unwrap_or(key)
            .to_owned()
    }

    /// A Rust source's answer that read a locked name with `get` fails, as
    /// data code's `store.get` throws (D5): its value is `unlock`'s alone.
    pub(in crate::runner) fn refuse_locked<T>(
        &self,
        answer: Result<T, super::DataError>,
    ) -> Result<T, super::DataError> {
        match self.store.take_locked_read() {
            Some(name) => Err(crate::StoreError::Locked(name).into()),
            None => answer,
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::{CallHow, DataError, DataSource, Runner, Value};
    use exact_kernel::Kernel;

    struct Empty;

    impl DataSource for Empty {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
    }

    #[test]
    fn a_plan_with_no_tools_answers_a_door_and_the_agent_without_the_tool_seam() {
        let source = "component App\n  view\n    text \"hello\"\n";
        let plan = contract::compile(source).unwrap();
        let mut r = Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        assert_eq!(
            r.call_tool("save", "{}", CallHow::default()),
            Err("no tool `save`: this app declares none".into())
        );
        let call = r#"{"tool":"save","args":{}}"#;
        assert_eq!(
            r.call_agent("call", call),
            Some(Err("no tool `save`: this app declares none".into()))
        );
        assert_eq!(
            r.call_agent("called", r#"{"id":3}"#),
            Some(Err("no call 3".into()))
        );
        assert_eq!(
            r.call_agent("cancel", r#"{"id":3}"#),
            Some(Err("no call 3".into()))
        );
        assert_eq!(r.intents_state(), None);
        assert!(r.declared_state().is_empty());
        assert!(!r.call_running() && r.call_due().is_none() && !r.call_stops_timers());
    }

    #[test]
    fn an_event_only_package_hears_its_event_through_the_module_seam() {
        use exact_plan::builder::PlanBuilder;
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let body = b.code(exact_plan::asm::Asm::new());
        let hear = b.action("hear", &[], &[], body);
        b.module_event(("bell", "@fixture/bell"), "chime", hear, "emit", 4, 0);
        b.node(
            exact_kernel::NodeType::View as u8,
            None,
            None,
            0,
            &[],
            &[],
            None,
        );
        let plan = b.finish().unwrap();
        assert!(plan.modules.is_empty() && plan.views.is_empty());
        let mut r = Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.raise_module_event("bell", "chime", "[]", crate::CallOrigin::Device)
            .unwrap();
        let (_, error) = r.apply_announced();
        assert!(error.is_none());
        assert!(r
            .journal()
            .any(|l| l.ends_with("bell.chime: heard by hear")));
    }

    #[test]
    fn a_view_only_package_links_the_module_seam_and_a_plan_without_one_does_not() {
        let source = "component App\n  view\n    text \"hello\"\n";
        let boot = |plan| {
            Runner::boot(
                plan,
                Empty,
                Kernel::with_monospace(),
                Default::default(),
                "/",
            )
            .unwrap()
        };
        let plain = boot(contract::compile(source).unwrap());
        assert!(plain.extensions_json().is_none());
        // The same app reloaded with a package view: the new runner links it.
        let mut plan = contract::compile(source).unwrap();
        plan.add_view("words", ("words", "@fixture/words"), false, &[], "{}");
        let mut viewed = boot(plan);
        assert!(viewed.extensions_json().is_some());
        viewed.bind_extensions(&[], &["words"]);
        assert!(viewed.extensions_json().unwrap().contains("words"));
    }

    #[test]
    fn a_plan_with_tools_and_no_modules_keeps_its_intents() {
        let source = "component App\n  state n = 0\n  tool bump writes \"Count up.\" = bump\n  action bump\n    n = n + 1\n  view\n    text \"x\"\n";
        let plan = contract::compile(source).unwrap();
        assert!(plan.modules.is_empty() && !plan.tools.is_empty());
        let mut r = Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let state = r.declared_state();
        let intents = (state.iter())
            .find(|(name, _)| name == "intents")
            .map(|(_, v)| v);
        assert_eq!(intents.unwrap()["tools"][0]["name"], "bump");
        let agent = CallHow {
            door: "agent".into(),
            ..CallHow::default()
        };
        let id = r.call_tool("bump", "{}", agent).unwrap();
        let (_, error) = r.step_calls();
        assert!(error.is_none());
        assert!(r.call_answer(id).unwrap().contains("\"outcome\":\"ok\""));
    }

    #[test]
    fn a_plan_with_no_presence_name_refuses_a_store_call_without_the_presence_seam() {
        let source = "component App\n  view\n    text \"hello\"\n";
        let plan = contract::compile(source).unwrap();
        let mut r = Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let body = br#"{"presence":"unlock","name":"token","reason":"Sign in"}"#;
        let Some(crate::request::Dispatch::Run(crate::request::Work::Now(answer))) =
            r.presence_call(body)
        else {
            panic!("a presence call is answered now");
        };
        let crate::Outcome::Response(response) = answer() else {
            panic!("a response");
        };
        let said: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        assert_eq!(said["error"], "NotSupportedError");
        assert_eq!(
            said["message"],
            "token is not a secret.keep name with a presence policy"
        );
        assert!(r.presence_call(br#"{"op":"other"}"#).is_none());
        assert!(r.presence_list().is_empty() && r.take_presence_asks().is_empty());
        assert!(!r.presence_secrets() && r.presence_due_ms().is_none());
    }
}
