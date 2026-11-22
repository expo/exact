//! Tool calls (LLP 1133.023 D7, LLP 1111 D4): a door hands the runner
//! `{tool, args, door}`, and the runner runs it as an invocation of origin
//! `tool`, as a wake's event is: one at a time with the wakes, as its own
//! commit, done when it owns nothing.
//!
//! - **Waits** while a wake or another call runs, up to [`WAIT`] on the
//!   session's clock, then is refused `busy`; and while the data source is
//!   not ready, as a cold call is delivered after the data module loads.
//! - **Validate:** the tool exists and is available, the door is one its
//!   face names (any door is the agent's), the face's digest matches, the
//!   arguments are its action's (`crate::tools`), and neither `returns` nor
//!   `fails when` reads a pending resource, nor is the module's background
//!   storage out (`busy`). A refusal here is certainty `none`.
//! - **Dispatch:** the action runs as one commit of origin `tool`, stamped
//!   with the call's invocation id (`provenance.rs`), which every request,
//!   `then` and queued send it makes carries, and every commit that answers
//!   one of those. The commands D4a skips are not run and are listed.
//! - **Complete** when no work stamped with its id is in flight: `fails
//!   when` and `returns` are read once. Background storage out then is
//!   `failed`, `uncertain` (`background`). Background storage is the
//!   module's ticket for storage no answer awaits (`background.rs`, LLP
//!   1097 D5), which no invocation can own; an answer's own storage is in
//!   its answer, which the call owns or does not.
//! - **Superseded** when a commit not its own lets go of work it owns,
//!   taken when that commit stands: `uncertain` for a send, `applied` for a
//!   resource's ask. **Timeout** [`BOUND`] after dispatch. A name that needs
//!   a person refuses the call `person-required`, `uncertain`.
//! - **Cancelled** by its caller (`{"op":"cancel","id"}`): one still
//!   waiting is dropped, `refused`, `none` (`cancelled`); one dispatched
//!   runs to its end, since its commit has landed and what it started is the
//!   app's to finish, and its record says the caller left (`cancelled`).

use super::provenance::CallOrigin;
use super::seams::Tools;
use super::{CommitReceipt, DataSource, Runner, RunnerError};
use serde_json::json;
use serde_json::Value as Json;
use std::collections::VecDeque;

/// How long a call waits for the invocation before it (D7).
pub(in crate::runner) const WAIT: f64 = 5_000.0;
/// A call's bound from its dispatch (LLP 1111 D4).
const BOUND: f64 = 10_000.0;
/// The calls `state.intents` keeps, newest last.
const KEPT: usize = 20;

/// The commands a call's commit does not run (LLP 1111 D4a): each moves
/// the person's view.
const SKIPPED: &[&str] = &[
    "focus",
    "blur",
    "scrollIntoView",
    "selectText",
    "setSelectionRange",
    "fastSeek",
    "load",
    "haptic",
];
/// The commands it runs as a press would; any other is refused.
const RUN: &[&str] = &[
    "postMessage",
    "stopSounds",
    "setSounds",
    "preventDefault",
    "stopPropagation",
];

/// How a door hands the runner a call.
#[derive(Clone, Debug, Default)]
pub struct CallHow {
    /// The door: `agent`, `apple`, `android`, `web`.
    pub door: String,
    /// The host booted the session for the call.
    pub cold: bool,
    /// No view of the session is visible: timers stop while it runs.
    pub hidden: bool,
    /// The face's digest the door was compiled with, where it has one.
    pub digest: Option<String>,
}

struct Call {
    id: u64,
    /// Its row in the plan's `tools`.
    tool: usize,
    args: Json,
    how: CallHow,
    arrived: f64,
    dispatched: Option<f64>,
    skipped: Vec<String>,
    /// Its caller cancelled it.
    cancelled: bool,
}

#[derive(Default)]
pub(in crate::runner) struct Calls {
    waiting: VecDeque<Call>,
    current: Option<Call>,
    /// The calls that ended, for `state.intents` and the door's `called`.
    ended: VecDeque<Json>,
}

impl<D: DataSource> Runner<D> {
    /// Refuse `busy` each waiting call whose [`WAIT`] has run out, whether
    /// a wake, another call or the data source held it.
    fn expire_waiting_calls(&mut self) {
        let now = self.now_ms;
        let waiting = |r: &mut Self| r.calls.waiting.pop_front_if(|c| now >= c.arrived + WAIT);
        while let Some(call) = waiting(self) {
            let name = self.plan.str(self.plan.tools[call.tool].name).to_owned();
            let message = "a wake or another call is running";
            self.log(format!("tool {name}: refused, busy ({message})"));
            self.finish_call(call, "refused", "none", "busy", json!(message));
        }
    }

    /// Validate the current call and run its action (module doc).
    fn dispatch_call(&mut self) -> Result<Option<CommitReceipt>, RunnerError> {
        let call = self.calls.current.as_ref().expect("a call");
        let (id, row, door) = (call.id, call.tool, call.how.door.clone());
        let tool = self.plan.tools[row].clone();
        let derive = |d: Option<exact_plan::DerivesId>| d.map(|d| self.derive_at(d.0 as usize));
        if derive(tool.available) == Some(Some(&exact_plan::Value::Bool(false))) {
            return Ok(self.refuse_call("unavailable", "it is not available now"));
        }
        if let Some(why) = self.face_refused(row, &door, call.how.digest.as_deref()) {
            return Ok(self.refuse_call("unavailable", &why));
        }
        let action = self.plan.action(tool.action).clone();
        let mut fields = Vec::new();
        for p in action.params.iter() {
            let param = self.plan.param(p);
            let name = param.declared_name(&self.plan).to_owned();
            match exact_js_value::Shape::from_plan(&self.plan, param.ty) {
                Ok(shape) => fields.push((name, shape)),
                Err(e) => return Ok(self.refuse_call("arguments", &e)),
            }
        }
        let call_args = call.args.clone();
        let shape = exact_js_value::Shape::Record(fields);
        if let Err(why) = crate::tools::check(&call_args, &shape, "") {
            return Ok(self.refuse_call("arguments", &why));
        }
        let mut args = Vec::new();
        for p in action.params.iter() {
            let param = self.plan.param(p);
            let json = &call_args[param.declared_name(&self.plan)];
            match self.event_arg(param.ty, json) {
                Ok(v) => args.push(v),
                Err(why) => return Ok(self.refuse_call("arguments", &why)),
            }
        }
        let read = [tool.returns, tool.fails];
        if read.iter().any(|d| derive(*d) == Some(None)) {
            return Ok(self.refuse_call("busy", "what it reports is still being answered"));
        }
        if self.background_ticket().is_some() {
            return Ok(self.refuse_call("busy", "the app's storage is still writing"));
        }
        let name = self.plan.str(tool.name).to_owned();
        let call = self.calls.current.as_mut().expect("a call");
        call.dispatched = Some(self.now_ms);
        self.log(format!("tool:system:{door} {name} dispatched"));
        let result = self.as_invocation(CallOrigin::Tool, id, |r| {
            r.run_action(tool.action, args, &[])
        });
        match result {
            Ok(receipt) => Ok(Some(receipt)),
            Err(e) => {
                self.end_call("failed", "uncertain", "refused-commit", Json::Null);
                Err(e)
            }
        }
    }

    /// Why the face refuses the current call at `door`, if it does (LLP
    /// 1133.023 D8): a door it does not name, or a digest not the plan's.
    /// The agent's door takes every tool.
    fn face_refused(&self, row: usize, door: &str, digest: Option<&str>) -> Option<String> {
        if door == "agent" {
            return None;
        }
        let face: Json =
            serde_json::from_str(self.plan.str(self.plan.tools[row].face)).unwrap_or_default();
        if face.is_null() {
            return Some(format!(
                "it has no face, so the {door} door does not offer it"
            ));
        }
        let named = face["doors"]
            .as_array()
            .is_none_or(|d| d.iter().any(|x| x == door));
        if !named {
            return Some(format!("its face does not name the {door} door"));
        }
        let ours = face["digest"].as_str().unwrap_or_default();
        match digest {
            Some(given) if given != ours => Some(format!(
                "face: the door's digest {given} is not the plan's {ours}"
            )),
            _ => None,
        }
    }

    /// The current call is refused before dispatch.
    fn refuse_call(&mut self, reason: &str, message: &str) -> Option<CommitReceipt> {
        let name = self.current_tool_name();
        self.log(format!("tool {name}: refused, {reason} ({message})"));
        self.end_call("refused", "none", reason, json!(message));
        None
    }

    fn current_tool_name(&self) -> String {
        (self.calls.current.as_ref())
            .map(|c| self.plan.str(self.plan.tools[c.tool].name).to_owned())
            .unwrap_or_default()
    }

    /// The current call owns nothing: read what it reports, once. `false`
    /// while `fails when` or `returns` reads a pending resource.
    fn complete_call(&mut self) -> bool {
        let row = self.calls.current.as_ref().expect("a call").tool;
        let tool = self.plan.tools[row].clone();
        if self.background_ticket().is_some() {
            self.end_call("failed", "uncertain", "background", Json::Null);
            return true;
        }
        let fails = tool.fails.map(|d| self.derive_at(d.0 as usize).cloned());
        let returns = tool.returns.map(|d| self.derive_at(d.0 as usize).cloned());
        if matches!(fails, Some(None)) || matches!(returns, Some(None)) {
            return false;
        }
        let value = match (tool.returns, returns.flatten()) {
            (Some(d), Some(v)) => {
                let mut s = String::new();
                crate::json::typed_json(&self.plan, self.plan.derives[d.0 as usize].ty, &v, &mut s);
                serde_json::from_str(&s).unwrap_or(Json::Null)
            }
            _ => Json::Null,
        };
        match fails.flatten() {
            Some(exact_plan::Value::Bool(true)) => self.end_call("failed", "uncertain", "", value),
            _ => self.end_call("ok", "applied", "", value),
        }
        true
    }

    /// The current call ends `outcome`, with its certainty and reason.
    fn end_call(&mut self, outcome: &str, certainty: &str, reason: &str, value: Json) {
        if let Some(call) = self.calls.current.take() {
            self.finish_call(call, outcome, certainty, reason, value);
        }
    }

    /// `call` ends `outcome`, its record joining the ended calls'. A
    /// refusal's `value` is its message.
    fn finish_call(
        &mut self,
        call: Call,
        outcome: &str,
        certainty: &str,
        reason: &str,
        value: Json,
    ) {
        let name = self.plan.str(self.plan.tools[call.tool].name).to_owned();
        let message = match (outcome, value.as_str()) {
            ("refused", Some(m)) => m.to_owned(),
            (_, _) if certainty == "uncertain" => {
                "It may or may not have taken effect: read before calling again.".into()
            }
            _ => String::new(),
        };
        let value = if outcome == "refused" {
            Json::Null
        } else {
            value
        };
        self.log(format!("tool {name} {outcome} {certainty}"));
        let ended = json!({
            "id": call.id,
            "tool": name,
            "door": call.how.door,
            "origin": CallOrigin::Tool.word(),
            "cold": call.how.cold,
            "outcome": outcome,
            "certainty": certainty,
            "reason": reason,
            "stale": false,
            "skipped": call.skipped,
            "cancelled": call.cancelled,
            "value": value,
            "message": message,
            "at": self.time.epoch_at_zero + self.now_ms,
        });
        let calls = &mut self.calls;
        calls.ended.push_back(ended);
        while calls.ended.len() > KEPT {
            calls.ended.pop_front();
        }
        self.native_slot().changed("");
    }
}

/// The tool seam, linked: the calls above.
pub(crate) struct LinkedTools;

impl<D: DataSource> Tools<D> for LinkedTools {
    /// A door's call of `tool` with `args`, a JSON object: queued, and its
    /// invocation id answered. `Err` for a tool the plan does not name.
    fn call_tool(
        &self,
        r: &mut Runner<D>,
        tool: &str,
        args: &str,
        how: CallHow,
    ) -> Result<u64, String> {
        let Some(row) = (r.plan.tools.iter()).position(|t| r.plan.str(t.name) == tool) else {
            let names: Vec<&str> = (r.plan.tools.iter()).map(|t| r.plan.str(t.name)).collect();
            return Err(match names.is_empty() {
                true => format!("no tool `{tool}`: this app declares none"),
                false => format!("no tool `{tool}`; its tools: {}", names.join(", ")),
            });
        };
        let args = serde_json::from_str(args).unwrap_or(Json::Null);
        let id = r.next_invocation();
        r.log(format!("tool:system:{} {tool}: arrived", how.door));
        r.calls.waiting.push_back(Call {
            id,
            tool: row,
            args,
            how,
            arrived: r.now_ms,
            dispatched: None,
            skipped: Vec::new(),
            cancelled: false,
        });
        r.native_slot().changed("");
        Ok(id)
    }

    /// Whether a call is between dispatch and its end.
    fn call_running(&self, r: &Runner<D>) -> bool {
        r.calls.current.is_some()
    }

    /// Run what is due of the calls: the next once nothing else runs, and
    /// the end of the one running once it owns nothing or its time is up.
    fn step_calls(&self, r: &mut Runner<D>) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        let mut receipts = Vec::new();
        loop {
            r.expire_waiting_calls();
            if let Some(call) = r.calls.current.as_ref() {
                let (id, at) = (call.id, call.dispatched.unwrap_or(r.now_ms));
                if r.now_ms >= at + BOUND {
                    r.end_call("timeout", "uncertain", "", Json::Null);
                    continue;
                }
                if r.invocation_owns(id) || !r.complete_call() {
                    break;
                }
                continue;
            }
            if r.calls.waiting.is_empty() || r.wake_running() || !r.data.ready() {
                break;
            }
            let call = r.calls.waiting.pop_front().expect("front");
            r.calls.current = Some(call);
            match r.dispatch_call() {
                Ok(Some(receipt)) => receipts.push(receipt),
                Ok(None) => {}
                Err(e) => return (receipts, Some(e)),
            }
        }
        (receipts, None)
    }

    /// Invocation `id` was let go of by a commit that stood: the running
    /// call ends `superseded`, `uncertain` when what was let go was a send.
    fn call_superseded(&self, r: &mut Runner<D>, id: u64, send: bool) {
        if (r.calls.current.as_ref()).is_some_and(|c| c.id == id) {
            let certainty = if send { "uncertain" } else { "applied" };
            r.end_call("superseded", certainty, "", Json::Null);
        }
    }

    /// A name that needs a person was called by the running call's work.
    fn call_person_required(&self, r: &mut Runner<D>) {
        let id = r.invocation_id();
        if (r.calls.current.as_ref()).is_some_and(|c| c.id == id && id != 0) {
            r.end_call("refused", "uncertain", "person-required", Json::Null);
        }
    }

    /// Take off this commit's commands from `first` that a call does not
    /// run, when the commit is the running call's (LLP 1111 D4a).
    fn skip_in_call(&self, r: &mut Runner<D>, first: usize) {
        let id = r.invocation_id();
        if id == 0 || !(r.calls.current.as_ref()).is_some_and(|c| c.id == id) {
            return;
        }
        let mut i = first;
        while i < r.commands.len() {
            let name = r.commands[i].name.clone();
            if RUN.contains(&name.as_str()) {
                i += 1;
                continue;
            }
            r.commands.remove(i);
            if SKIPPED.contains(&name.as_str()) {
                let call = r.calls.current.as_mut().expect("a call");
                call.skipped.push(name.clone());
                r.log(format!("{name}: skipped in a tool call"));
            } else {
                r.log(format!("{name}: refused: in a tool call"));
            }
        }
    }

    /// Whether a running call is in a session no one sees: timers stop.
    fn call_stops_timers(&self, r: &Runner<D>) -> bool {
        (r.calls.current.as_ref()).is_some_and(|c| c.how.hidden)
    }

    /// When the host should next advance for the calls.
    fn call_due(&self, r: &Runner<D>) -> Option<f64> {
        let calls = &r.calls;
        let waited = calls.waiting.front().map(|c| c.arrived + WAIT);
        if let Some(call) = calls.current.as_ref() {
            let deadline = call.dispatched.unwrap_or(r.now_ms) + BOUND;
            let due = match r.invocation_owns(call.id) {
                true => deadline,
                false => r.now_ms,
            };
            return Some(waited.map_or(due, |w| due.min(w)));
        }
        let waited = waited?;
        Some(match r.wake_running() || !r.data.ready() {
            true => waited,
            false => r.now_ms,
        })
    }

    /// Call `id`'s caller cancelled it (module doc): `{call, state}`, the
    /// state `ended` once it has ended, `running` while it runs to its end.
    fn cancel_call(&self, r: &mut Runner<D>, id: u64) -> Result<String, String> {
        let calls = &mut r.calls;
        if let Some(i) = calls.waiting.iter().position(|c| c.id == id) {
            let mut call = calls.waiting.remove(i).expect("found");
            call.cancelled = true;
            let name = r.plan.str(r.plan.tools[call.tool].name).to_owned();
            r.log(format!(
                "tool:system:{} {name}: cancelled before it ran",
                call.how.door
            ));
            let message = json!("Its caller cancelled it before it ran.");
            r.finish_call(call, "refused", "none", "cancelled", message);
            return Ok(json!({ "call": id, "state": "ended" }).to_string());
        }
        if let Some(call) = calls.current.as_mut().filter(|c| c.id == id) {
            if !std::mem::replace(&mut call.cancelled, true) {
                let door = call.how.door.clone();
                let name = r.current_tool_name();
                r.log(format!(
                    "tool:system:{door} {name}: its caller cancelled; it runs to its end"
                ));
            }
            return Ok(json!({ "call": id, "state": "running" }).to_string());
        }
        match calls.ended.iter().any(|c| c["id"] == id) {
            true => Ok(json!({ "call": id, "state": "ended" }).to_string()),
            false => Err(format!("no call {id}")),
        }
    }

    /// Call `id` as its door polls it: LLP 1111 D4's envelope once it has
    /// ended, else `{call, state}` with `waiting` or `running`.
    fn call_answer(&self, r: &Runner<D>, id: u64) -> Result<String, String> {
        let calls = &r.calls;
        if let Some(ended) = calls.ended.iter().find(|c| c["id"] == id) {
            return Ok(ended.to_string());
        }
        let state = match () {
            _ if calls.current.as_ref().is_some_and(|c| c.id == id) => "running",
            _ if calls.waiting.iter().any(|c| c.id == id) => "waiting",
            _ => return Err(format!("no call {id}")),
        };
        Ok(json!({ "call": id, "state": state }).to_string())
    }

    /// `state.intents`: each tool with its word, modifiers, availability
    /// and face, and the calls running, waiting and ended, never their
    /// arguments (LLP 1133.023 §10).
    fn intents_state(&self, r: &Runner<D>) -> Option<Json> {
        if r.plan.tools.is_empty() {
            return None;
        }
        let tools: Vec<Json> = (r.plan.tools.iter())
            .map(|t| {
                let available = t.available.map(|d| r.derive_at(d.0 as usize).cloned());
                let face: Json = serde_json::from_str(r.plan.str(t.face)).unwrap_or_default();
                json!({
                    "name": r.plan.str(t.name),
                    "consequence": r.plan.str(t.consequence),
                    "additive": t.additive,
                    "idempotent": t.idempotent,
                    "closed": t.closed,
                    "description": r.plan.str(t.description),
                    "action": r.plan.str(r.plan.action(t.action).name),
                    "available": !matches!(available, Some(Some(exact_plan::Value::Bool(false)))),
                    "face": face,
                })
            })
            .collect();
        let calls = &r.calls;
        let open = |c: &Call, state: &str| json!({ "id": c.id, "tool": r.plan.str(r.plan.tools[c.tool].name), "door": c.how.door, "origin": "tool", "state": state });
        let mut all: Vec<Json> = calls.ended.iter().cloned().collect();
        all.extend(calls.current.iter().map(|c| open(c, "running")));
        all.extend(calls.waiting.iter().map(|c| open(c, "waiting")));
        Some(json!({ "tools": tools, "calls": all }))
    }
}
