//! The module seam (LLP 1047.001 D11 item 2): every entry the core and the
//! hosts reach of the native modules (`modules/`). `modules/seam.rs`
//! implements it with the modules' code, and only an entry's
//! [`super::ModuleLinks`] names that, so a binary that does not link the
//! modules holds none of their code. The provided methods are what the core
//! does with no module, view or event row, as an empty table did before:
//! the boot uses them for a plan with none, whatever the binary links.
//! The runner's methods that reach the seam are in `module_seam/forward.rs`.

use super::provenance::{CallOrigin, Cause};
use super::{AppState, CommitReceipt, DataError, DataSource, Runner, RunnerError, Target};
use crate::request::{Answer, Dispatch, Outcome, Request, Work};
use crate::HoldAnswer;
use exact_kernel::ViewId;
use exact_plan::{ModulesRow, TypesId, Value};
use serde_json::{json, Value as Json};
use std::any::Any;
use std::collections::BTreeMap;
use std::sync::Arc;

mod forward;

/// The native modules' state, owned by the runner so that a module's calls
/// into the core and the core's back into the module find it where they
/// left it. Its type is the modules' own, made by the linked seam
/// ([`Modules::state`]) and reached only by module code (`modules.rs`), so
/// a binary without the modules holds none of its code, its drop included.
pub(in crate::runner) struct ModuleState(pub(in crate::runner) Box<dyn Any>);

/// The module readers a commit's checkpoint keeps: each bound live
/// source's readers, by resource, with the arguments each last asked with.
pub(in crate::runner) type Readers = BTreeMap<String, BTreeMap<usize, String>>;

/// Refuse a data-code request whose package, name or verb has no matching row.
pub(super) fn unsupported_call() -> Dispatch {
    Dispatch::Run(Work::Now(Box::new(|| {
        Outcome::module_refusal("NotSupportedError", None)
    })))
}

/// The Android reader uses a nonempty source first, then a send. Admit the
/// same name before handing the request to a package.
pub(super) fn declared_call(v: &Json) -> Option<(&str, bool)> {
    let source = v["source"].as_str().filter(|name| !name.is_empty());
    let send = v["send"].as_str().filter(|name| !name.is_empty());
    source
        .map(|name| (name, false))
        .or_else(|| send.map(|name| (name, true)))
}

/// The target this runner is built for, as a substitute's `targets` and a
/// ceremony's `clientOrigin` name it.
pub(in crate::runner) fn build_target() -> &'static str {
    if cfg!(target_arch = "wasm32") {
        "web"
    } else if cfg!(target_os = "ios") {
        "ios"
    } else if cfg!(target_os = "tvos") {
        "tvos"
    } else if cfg!(target_os = "macos") {
        "macos"
    } else if cfg!(target_os = "android") {
        "android"
    } else if cfg!(target_os = "windows") {
        "windows"
    } else {
        "linux"
    }
}

/// The module seam. Each method takes the runner; each provided body is the
/// answer with no module, view or event row, for what the core can pass it
/// then: no ticket, token, handle, row or permission a module made reaches
/// it, since none exists. One answer narrows: a request whose body names an
/// extension's numbered send, which no module row made, is not cancelled at
/// that extension when it is let go.
#[allow(unused_variables)]
pub trait Modules<D: DataSource> {
    /// The modules' state for a new runner, which it keeps: none here. A
    /// seam's other methods run on the state its own `state` made, so an
    /// implementation that delegates them delegates this too.
    fn state(&self) -> Box<dyn Any> {
        Box::new(())
    }

    /// Install the driver’s current substitutes before the first resource reads.
    fn launch_substitutes(&self, r: &mut Runner<D>) -> Result<(), RunnerError> {
        Ok(())
    }

    /// Invocation `id`'s held calls end `AbortError` with `message`.
    fn abort_held_of(&self, r: &mut Runner<D>, id: u64, message: &str) {}

    /// Admission of an `img` source of a package's scheme, on `target` (the
    /// host's: `ios`, `macos`, `android`, `web`): `{"extension", "scheme",
    /// "agent", "owned"?}` to ask that extension's implementation for the
    /// picture, or `{"error", "message"}`, which the host shows as an
    /// unresolved image. `None` for a source of no package's scheme.
    fn admit_picture(&self, r: &mut Runner<D>, source: &str, target: &str) -> Option<Json> {
        None
    }

    /// The app left the foreground (`hidden`) or came back (D6's table): a
    /// waiting write fails `AbortError`; a scan behind the system's sheet
    /// (`readerUI` `system`) ends `interrupted`, and one the app draws for
    /// is suspended, and on return reads a tag still in the field.
    fn arrival_visibility(&self, r: &mut Runner<D>, hidden: bool) {}

    /// The host's implementations answer `real` as in production, and
    /// `agent`, told they run under the agent, answer an interface that is
    /// its own substitute: their names' readers are asked again, of the
    /// module now, and each `agent` implementation is given the preferred
    /// values the agent set.
    fn bind_extensions(&self, r: &mut Runner<D>, real: &[&str], agent: &[&str]) {
        r.presence_rebound();
        if real.is_empty() && agent.is_empty() {
            return;
        }
        if !real.is_empty() {
            r.log(format!("modules bound here: {}", real.join(", ")));
        }
        r.native_slot().sync(BTreeMap::new());
    }

    /// Whether the host bound its own implementation of `extension`.
    fn bound_here(&self, r: &Runner<D>, extension: &str) -> bool {
        false
    }

    /// Admission without counting a draw: hosts recheck refused pictures
    /// after commits that may have changed admission.
    fn check_picture(&self, r: &mut Runner<D>, source: &str, target: &str) -> Option<Json> {
        None
    }

    /// A provider reports its physical name; every active declaration of
    /// that name watches its own qualified topic.
    fn declaration_topics(&self, r: &Runner<D>, topics: Vec<String>) -> Vec<String> {
        topics
    }

    /// What the app's literals declare, by `<extension>.<name>`: each row's
    /// `declared` (LLP 1133.004 stage 2), `{option: [value, …]}`.
    fn declared_values(&self, r: &Runner<D>) -> Json {
        Json::Object(Default::default())
    }

    /// Whether `action`, a notification action's record as its post carried
    /// it, is one this plan declares under `extension` ([`Self::declared_values`]):
    /// a host whose platform hands back the record rather than its own
    /// registration (Android's, LLP 1133.004.000 D5) raises the action only
    /// then, so a button an update removed or changed raises nothing.
    fn declares_action(&self, r: &Runner<D>, extension: &str, action: &Json) -> bool {
        false
    }

    /// Each queued event, in order, one commit each: its action run with as
    /// many of its arguments as the action takes, under its origin.
    fn deliver_module_events(
        &self,
        r: &mut Runner<D>,
    ) -> (Vec<CommitReceipt>, Option<RunnerError>) {
        (Vec::new(), None)
    }

    /// [`Runner`]'s `discard_failed_tools`, answered by the modules.
    fn discard_failed_tools(&self, r: &mut Runner<D>) {}

    /// A disclosure an implementation said: its fields kept.
    fn disclosed_told(&self, r: &mut Runner<D>, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        if !(v["extension"].is_string() && v["state"].is_object()) {
            r.log(format!("an implementation's state is unreadable: {body}"));
        }
    }

    /// A reply carrying `handles` was not delivered (`why`): its
    /// implementation disposes of them.
    fn dispose(&self, r: &mut Runner<D>, handles: (String, Vec<String>), why: &str) {}

    /// Replies with handles whose request is no longer in flight: dropped,
    /// their handles disposed of. Requests let go before they were sent are
    /// forgotten.
    fn dispose_dropped(&self, r: &mut Runner<D>) {}

    /// Invocation `id`'s native statements are dropped: it expired.
    fn drop_statements_of(&self, r: &mut Runner<D>, id: u64) {}

    /// The agent's `prefer extension` and `prefer permission` (LLP 1124 D7,
    /// LLP 1069.007 D2): `{"op":"prefer","extension":{"geolocation.watchPosition":
    /// {…}|"clear"}}` sets a source's answer; `{"op":"prefer","permission":
    /// {"geolocation":"granted"}}` a permission's state. Readers are asked
    /// again. `None` for any other request.
    fn extension_agent(
        &self,
        r: &mut Runner<D>,
        request: &str,
    ) -> Option<(Vec<CommitReceipt>, Result<String, String>)> {
        let v: Json = serde_json::from_str(request).ok()?;
        if v.get("op").and_then(|o| o.as_str()) != Some("prefer") {
            return None;
        }
        let refuse = |e: String| Some((Vec::new(), Err(e)));
        if let Some(group) = v.get("permission") {
            let Some(group) = group.as_object() else {
                return refuse(
                    "prefer permission: an object of `<name>`: granted|denied|prompt".into(),
                );
            };
            if let Some((name, value)) = group.iter().next() {
                return refuse(match value.is_string() {
                    true => format!("prefer permission: no module here asks for `{name}`"),
                    false => format!("prefer permission.{name}: granted, denied or prompt"),
                });
            }
            if v.get("extension").is_none() {
                let (receipts, error) = r.apply_announced();
                if let Some(e) = error {
                    return refuse(format!("prefer permission: {e:?}"));
                }
                return Some((receipts, Ok(json!({ "permission": {} }).to_string())));
            }
        }
        let Some(group) = v.get("extension")?.as_object() else {
            return refuse("prefer extension: an object of `extension.source`".into());
        };
        if let Some(key) = group.keys().next() {
            return refuse(format!(
                "prefer extension: no source `{key}` in this app; its module sources are none"
            ));
        }
        let (receipts, error) = r.apply_announced();
        if let Some(e) = error {
            return refuse(format!("prefer extension: {e:?}"));
        }
        Some((
            receipts,
            Ok(json!({ "extension": {}, "permission": {} }).to_string()),
        ))
    }

    /// `state.extensions` (LLP 1133.002 §6, Disclosure): each module the plan
    /// names, by its extension: whether a host's implementation answers it
    /// (`bound`, and `agent` where it answers under the agent), its
    /// permission's state, the preferred values, each source's readers (a
    /// resource that reads it, or whose data code did), the tickets held
    /// with their calls' origins, and its ceremonies and signals, each by
    /// name, RP ID, origin and outcome, never a response (LLP 1133.011 §9).
    /// `None` for a plan that names none.
    fn extensions_json(&self, r: &Runner<D>) -> Option<String> {
        None
    }

    /// [`Runner`]'s `fail_tool`, answered by the modules.
    fn fail_tool(&self, r: &mut Runner<D>, ticket: u64, message: &str) {}

    /// [`Runner`]'s `failed_tool_tickets`, answered by the modules.
    fn failed_tool_tickets(&self, r: &Runner<D>) -> Vec<(u64, String)> {
        Vec::new()
    }

    /// The GPU canvases a commit published: their surface names, kept by view.
    fn fed_canvases(&self, r: &mut Runner<D>, updates: &[crate::SurfaceUpdate]) {}

    /// A claim an implementation told: journaled, and listed while held.
    fn foreground_service_told(&self, r: &mut Runner<D>, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        if !(v["extension"].is_string() && v["kind"].is_string()) {
            r.log(format!("a foreground-service claim is unreadable: {body}"));
            return;
        }
        let types: Vec<&str> = (v["types"].as_array().into_iter().flatten())
            .filter_map(Json::as_str)
            .collect();
        let named = types.join("|");
        let line = match (v["refused"].as_str(), v["claimed"] == true) {
            (Some(why), _) => format!("refused: {why}"),
            (None, true) => "claimed".to_owned(),
            (None, false) => "released".to_owned(),
        };
        r.log(format!("foreground service {named}: {line}"));
    }

    /// Resource row `i` is a new member's: its last poll answer no longer
    /// stands.
    fn forget_module_row(&self, r: &mut Runner<D>, i: usize) {}

    /// The host runs a module request whose reply carries handles, the
    /// oldest such request with this body not yet run: its implementation's
    /// reply is watched, whether or not the request is still in flight (a
    /// host may run one the runner has let go). `None` for any other body.
    fn handled_work(&self, r: &mut Runner<D>, body: &[u8]) -> Option<Dispatch> {
        None
    }

    /// Whether an ordered handler request's refusal is waiting: it fences ordered work.
    fn has_ordered_tool_refusal(&self, r: &Runner<D>) -> bool {
        false
    }

    /// Whether a handler's refused request may settle now.
    fn has_tool_refusal(&self, r: &Runner<D>, allow_ordered: bool) -> bool {
        false
    }

    /// Whether a host's implementation answers some name of `extension`.
    fn implemented(&self, r: &Runner<D>, extension: &str) -> bool {
        false
    }

    /// The entries of `kind` whose title or description contains `matching`,
    /// case- and diacritic-insensitive, in the index's order (D9).
    fn index_entities(&self, r: &Runner<D>, kind: &str, matching: Option<&str>) -> Vec<Json> {
        Vec::new()
    }

    /// An implementation's call of the app's handler, `{"extension",
    /// "handle", "handler", "args"}`: refused unless the call that made
    /// `handle` listed `handler`, else run; `reply` is given `{"text"}`,
    /// the handler's value as JSON, or `{"error", "message"}`, once.
    fn invoke_module(&self, r: &mut Runner<D>, body: &str, reply: Box<dyn FnOnce(String) + Send>) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        let text = |field: &str| v[field].as_str().unwrap_or_default().to_owned();
        let (extension, handle, handler) = (text("extension"), text("handle"), text("handler"));
        let at = format!("{extension}: {handler} for {handle}");
        let anchored = handle
            .strip_prefix("request:")
            .and_then(|n| n.parse::<u64>().ok());
        let message = match anchored {
            Some(n) => {
                r.log(format!(
                    "{at}: refused, no pending call numbered {n} names the handler"
                ));
                format!("no pending call numbered {n} names {handler}")
            }
            None => {
                r.log(format!(
                    "{at}: refused, no call made for it lists the handler"
                ));
                format!("no call that made {handle} lists {handler}")
            }
        };
        reply(json!({ "error": "NotAllowedError", "message": message }).to_string())
    }

    /// Whether `reference` is a listed entry's `ref`.
    fn is_listed(&self, r: &Runner<D>, reference: u64) -> bool {
        false
    }

    /// Whether `ticket` is a handler's request.
    fn is_tool(&self, r: &Runner<D>, ticket: u64) -> bool {
        false
    }

    /// The session ends: each event still queued is journaled, unheard (D6).
    fn journal_unheard_events(&self, r: &mut Runner<D>) {}

    /// Request `ticket` runs on with no answer awaiting it (module work): its
    /// send is not cancelled, and its anchor goes with its reply.
    fn keep_send(&self, r: &mut Runner<D>, ticket: u64) {}

    /// The handles request `ticket`'s reply carried, as it lands: no longer
    /// provisional, the commit's to deliver.
    fn landed_handles(&self, r: &mut Runner<D>, ticket: u64) -> Option<(String, Vec<String>)> {
        None
    }

    /// Request `ticket`'s reply landed: its send is no longer cancelled.
    fn landed_send(&self, r: &mut Runner<D>, ticket: u64) {}

    /// A relaunched app's launch flag (`--notification '<entry>'`): the
    /// entry as the agent's state printed it, delivered as its tap would be
    /// (LLP 1133.004 §7), held until the session can hear it.
    fn launch_listed(
        &self,
        r: &mut Runner<D>,
        launch: &str,
        entry: &str,
    ) -> Result<String, String> {
        let _: Json =
            serde_json::from_str(entry).map_err(|e| format!("--{launch}: not JSON: {e}"))?;
        Err(format!(
            "--{launch}: no module of this app lists entries a launch hands over"
        ))
    }

    /// `--login`: this launch is the login item's (LLP 1133.025 §11), and
    /// `hidden` what its registration asked.
    fn launch_login(&self, r: &mut Runner<D>, hidden: bool) -> Result<String, String> {
        r.log(format!(
            "launch --login{}",
            if hidden { " hidden" } else { "" }
        ));
        Ok(json!({ "launch": "login", "hidden": hidden }).to_string())
    }

    /// A bound implementation's choice (module doc), applied as the
    /// person's; journaled when refused.
    fn listed_choice(&self, r: &mut Runner<D>, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        let (Some(extension), Some(state), Some(_)) = (
            v["extension"].as_str(),
            v["state"].as_str(),
            v["choice"].as_str(),
        ) else {
            r.log(format!("{} not a choice: {body}", super::modules::CHOSEN));
            return;
        };
        r.log(format!("{extension}.{state}: a choice from an implementation whose list the runner does not keep here, refused"));
    }

    /// The next local presentation on the session clock.
    fn next_list_show(&self, r: &Runner<D>) -> Option<f64> {
        None
    }

    /// Mark the entry presented and emit its event if the page is visible.
    fn show_listed(&self, r: &mut Runner<D>) -> Result<Option<CommitReceipt>, RunnerError> {
        Ok(None)
    }

    /// The agent clock reached `now_ms`: each entry of a list with a due
    /// time that is now due fires, once; a list another session changed is
    /// announced to this one's readers.
    fn listed_due(&self, r: &mut Runner<D>, now_ms: f64) {}

    /// The agent's operations on listed entries (§7): `{"op":"tap","ticket":ref}`
    /// on one, or `{"op":"launchEntry","launch","entry"}`. `None` for any other.
    fn listed_op(
        &self,
        r: &mut Runner<D>,
        op: &str,
        request: &str,
    ) -> Option<Result<String, String>> {
        let v: Json = serde_json::from_str(request).ok()?;
        match op {
            "launchEntry" if v["launch"] == "wake" => {
                let entry = match &v["entry"] {
                    Json::String(text) => serde_json::from_str(text).unwrap_or(Json::Null),
                    entry => entry.clone(),
                };
                Some(match entry.is_object() {
                    true => Err(format!(
                        "--wake: no module of this app lists registrations of kind `{}`",
                        entry["kind"].as_str().unwrap_or_default()
                    )),
                    false => {
                        Err("--wake: the entry is a JSON object, as `state` printed it".into())
                    }
                })
            }
            "launchEntry" if v["launch"] == "call" => None,
            "launchEntry" if v["launch"] == "login" => Some(r.launch_login(v["entry"] == "hidden")),
            "launchEntry" => {
                let entry = v["entry"]
                    .as_str()
                    .map_or_else(|| v["entry"].to_string(), str::to_owned);
                Some(r.launch_listed(v["launch"].as_str().unwrap_or_default(), &entry))
            }
            _ => None,
        }
    }

    /// What answers resource `i`, whose source `source` is a module's name.
    fn module_answer(
        &self,
        r: &mut Runner<D>,
        i: usize,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, RunnerError> {
        let resource = r.plan.str(r.resource_row(i).name).to_string();
        let error = DataError::UnknownSource(source.into());
        Err(RunnerError::Data { resource, error })
    }

    /// The agent answered hold `hold`, already spent: §6's transitions.
    fn module_answered(&self, r: &mut Runner<D>, hold: u64, answer: &HoldAnswer) {}

    /// A call from data code (LLP 1133.002 D14): a native request whose body
    /// names a module's source or send, `{"extension": …, "source": …,
    /// "args": {…}}`, admitted here as a Contract read or send is, before any
    /// module. `None` for any other body.
    fn module_call(&self, r: &mut Runner<D>, native: &crate::Request) -> Option<Dispatch> {
        let v: Json = serde_json::from_slice(&native.body).ok()?;
        let extension = v.get("extension")?.as_str()?;
        let (declared, _) = declared_call(&v)?;
        let owner = v["owner"]
            .as_str()
            .map(|s| format!("#{s}"))
            .unwrap_or_default();
        let key = format!("{extension}.{declared}{owner}");
        r.log(format!("{key}: not a module this app's plan names"));
        Some(unsupported_call())
    }

    /// A typed answer to `ticket`, checked before the hold is spent, so it
    /// can be corrected. `None` when the hold is not a module's.
    fn module_check(&self, r: &Runner<D>, ticket: u64, text: &str) -> Option<Result<(), String>> {
        None
    }

    /// Value tickets whose timeout has passed on the agent's clock (§6):
    /// each fails `TIMEOUT`, its ticket retired.
    fn module_deadlines(&self, r: &mut Runner<D>, now_ms: f64) {
        if !r.native_slot().synced() {
            r.native_slot().sync(BTreeMap::new());
        }
    }

    /// Each list and value the plan's modules declare, by its state name:
    /// [`Self::declared_state`]'s module part.
    fn module_declared_state(&self, r: &Runner<D>) -> BTreeMap<String, Json> {
        BTreeMap::new()
    }

    /// The host dispatches held module request `token`: answered already,
    /// or held until the agent answers. `None` for a data source's token.
    fn module_dispatch(&self, r: &mut Runner<D>, token: u64) -> Option<Dispatch> {
        super::modules::TOKENS
            .contains(&token)
            .then_some(Dispatch::Missing)
    }

    /// The reply to a held module request, as `fulfill` parses it.
    fn module_parse(
        &self,
        r: &mut Runner<D>,
        target: Target,
        ty: TypesId,
        source: &str,
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    /// Held module requests the agent answered since the host dispatched them.
    fn module_release(&self, r: &mut Runner<D>) -> Vec<(u64, Dispatch)> {
        Vec::new()
    }

    /// What answers a send (`target`) to `source`, a module's name.
    fn module_send(
        &self,
        r: &mut Runner<D>,
        target: Target,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    /// The topic a data-code read of a module's source watches, its key.
    fn module_topic(&self, r: &Runner<D>, body: &[u8]) -> Option<String> {
        None
    }

    /// Whether invocation `id` owns module work: a native statement or a
    /// held call stamped with it ([`Self::invocation_owns`]).
    fn modules_own(&self, r: &Runner<D>, id: u64) -> bool {
        false
    }

    /// The modules' part of [`Self::collection_output_quiet`]: no package
    /// view, fed canvas or held call.
    fn modules_quiet(&self, r: &Runner<D>) -> bool {
        true
    }

    /// Request `ticket` of `source` went out with `body`: a reply to a name
    /// that declares handles is provisional until it lands.
    fn note_handles(&self, r: &mut Runner<D>, ticket: u64, source: &str, body: &[u8]) {}

    /// Request `ticket` went out with `body`: a send to an implementation
    /// is kept by its number until its reply lands.
    fn note_send(&self, r: &mut Runner<D>, ticket: u64, body: &[u8]) {}

    /// `request`, about to be handed out, numbered when it is data code's
    /// call of one of this plan's module sends that has no number yet.
    fn number_data_call(&self, r: &mut Runner<D>, request: &mut Request) {}

    /// The schemes the plan's interfaces serve pictures by, each once: the
    /// `img` sources a host's raster loader gives [`Runner::admit_picture`]
    /// rather than opening as files.
    fn picture_schemes(&self, r: &Runner<D>) -> Vec<String> {
        Vec::new()
    }

    /// After an advance: each polled source whose period passed and whose
    /// last ask has landed is asked again, once.
    fn poll(&self, r: &mut Runner<D>) -> Result<Vec<CommitReceipt>, RunnerError> {
        Ok(Vec::new())
    }

    /// When the next poll is due, for a host on the live clock to wake at.
    fn poll_due_ms(&self, r: &Runner<D>) -> Option<f64> {
        None
    }

    /// Whether a polled source's ask is in flight, or a stale answer's
    /// (`stale.rs`): what an agent's jump waits for before it replies.
    fn polls_asking(&self, r: &Runner<D>) -> bool {
        false
    }

    /// An event a bound implementation emitted: the device's origin, whatever
    /// it claims.
    fn raise_emitted(&self, r: &mut Runner<D>, body: &str) {
        let v: Json = serde_json::from_str(body).unwrap_or_default();
        let (Some(extension), Some(event)) = (v["extension"].as_str(), v["event"].as_str()) else {
            r.log(format!("an emitted event is unreadable: {body}"));
            return;
        };
        let args = v["args"].to_string();
        if let Err(e) = r.raise_module_event(extension, event, &args, CallOrigin::Device) {
            r.log(e);
        }
    }

    /// The host raises event `name` of `extension` with `args`, a JSON
    /// array: `origin` is [`CallOrigin::App`], a person's, only for an
    /// activation the host's own platform callback reported. Queued for
    /// [`Runner::apply_announced`]; `Err` for an event the plan does not
    /// know or arguments that are not an array.
    fn raise_module_event(
        &self,
        r: &mut Runner<D>,
        extension: &str,
        name: &str,
        args: &str,
        origin: CallOrigin,
    ) -> Result<(), String> {
        let key = format!("{extension}.{name}");
        match serde_json::from_str(args) {
            Ok(Json::Array(_)) => {}
            _ => return Err(format!("{key}: the arguments are a JSON array")),
        }
        r.log(format!("{key}: no handler hears it"));
        Ok(())
    }

    /// The host refused a handler's request at admission: it settles in
    /// its turn, as a failure, an ordered one only once earlier ordered work
    /// has.
    fn refuse_tool(&self, r: &mut Runner<D>, ticket: u64, reason: &'static str, ordered: bool) {}

    /// The substitute's readers no resource holds any longer are let go, with
    /// their grids and cursors (a released source's acquisition stops).
    fn release_sampled(&self, r: &mut Runner<D>) {}

    /// Report each package view's change since the last report (module doc).
    /// A host calls this again after it changes what is displayed without a
    /// commit (a cover set as it lays the commit out), as it reads the wake
    /// lock's holders.
    fn report_views(&self, r: &mut Runner<D>) {}

    /// Held module requests whose request is gone (superseded, its reader
    /// released, a refused commit): retired with their tickets (§6).
    fn retire_modules(&self, r: &mut Runner<D>) {}

    /// Run the effects a commit stated, in order, once it stands.
    fn run_module_effects(&self, r: &mut Runner<D>, effects: Vec<(ModulesRow, Vec<Value>)>) {}

    /// The commit was refused: its answers delivered nothing.
    fn samples_refused(&self, r: &mut Runner<D>) {}

    /// The commit stood: the cursors of the answers it delivered hold.
    fn samples_stood(&self, r: &mut Runner<D>) {}

    /// The page was hidden or shown at the session clock's now.
    fn samples_visibility(&self, r: &mut Runner<D>, hidden: bool) {}

    /// Share the host's app-wide state with this session (§7: `"scope":
    /// "app"`), before anything sets it.
    fn share_app_state(&self, r: &mut Runner<D>, state: AppState) {}

    /// The soonest span's end, for the clock's next wake.
    fn span_due(&self, r: &Runner<D>) -> Option<f64> {
        None
    }

    /// What the substitute's canvas inputs owe their surfaces up to `to_ms`
    /// on the session clock, oldest first: each a surface's name and its
    /// input event as the hosts write it, `{"t":"sensor","sensor","values",
    /// "dropped","at"}` or `{"t":"sensorEnded","sensor","at"}` (D6). Values
    /// are the declared `values` fields of the sensor's source, a list one
    /// spread, padded with zeros to four.
    fn surface_sensor_events(&self, r: &mut Runner<D>, to_ms: f64) -> Vec<(String, String)> {
        Vec::new()
    }

    /// Whether the substitute feeds a canvas input: a host advancing the
    /// agent's clock then steps each world a display frame at a time.
    fn surface_sensor_inputs(&self, r: &Runner<D>) -> bool {
        false
    }

    /// The commands of this commit from `first` that are a module's sends
    /// answering nothing: taken off, to run once the commit stands.
    fn take_module_effects(
        &self,
        r: &mut Runner<D>,
        first: usize,
    ) -> Vec<(ModulesRow, Vec<Value>)> {
        Vec::new()
    }

    /// `body`, a reply to live source `key`, without its stale time, which
    /// is noted: a list answer's envelope is unwrapped when nothing else is
    /// left in it. A body that names none is returned as it came.
    fn take_stale(&self, r: &mut Runner<D>, key: &str, body: Vec<u8>) -> Vec<u8> {
        body
    }

    /// `tap @ref` on a listed entry (§7): what the platform's click would
    /// do. `Ok` is the reply: `{"navigate": location}` for the host to open,
    /// or the event raised; the entry is retired either way.
    fn tap_listed(&self, r: &mut Runner<D>, reference: u64) -> Result<String, String> {
        Err(format!("@{reference} is no listed entry"))
    }

    /// [`Self::tap_listed`] with the person's choice, for an entry whose
    /// list declares choices (LLP 1133.030 D12 item 2).
    fn tap_listed_as(
        &self,
        r: &mut Runner<D>,
        reference: u64,
        choice: Option<&str>,
    ) -> Result<String, String> {
        Err(format!("@{reference} is no listed entry"))
    }

    /// The app went to the foreground or left it: each implementation that
    /// answers here is told, `{"extension", "foreground": bool}`, as the
    /// fact admission reads (LLP 1133.009 D10 item 3). An implementation
    /// begins in the foreground.
    fn tell_foreground(&self, r: &Runner<D>) {}

    /// [`Runner`]'s `tool_completion`, answered by the modules.
    fn tool_completion<'a>(
        &self,
        r: &'a mut Runner<D>,
        ticket: u64,
    ) -> Option<&'a mut Option<crate::CompletionReply>> {
        None
    }

    /// Whether a handler's request with effects is in flight, one the host
    /// refused at admission aside.
    fn tool_effects(&self, r: &Runner<D>) -> bool {
        false
    }

    /// A handler's request came back: its turn resumes.
    fn tool_landed(&self, r: &mut Runner<D>, ticket: u64, outcome: Outcome) {}

    /// The cause of the least privileged handler running now, if one is.
    fn tool_cause(&self, r: &Runner<D>) -> Option<super::provenance::Cause> {
        None
    }

    /// A handler's refused request that may settle now.
    fn tool_refusal(&self, r: &mut Runner<D>, allow_ordered: bool) -> Option<(u64, &'static str)> {
        None
    }

    /// The handlers' requests in flight, for `pending` and `in_flight`.
    fn tool_tickets(&self, r: &Runner<D>) -> Vec<(String, u64)> {
        Vec::new()
    }

    /// The cause of the commit that made view `id`'s displayed state, or
    /// set its binding `prop`, as the runner reported it: what an
    /// acquisition the view makes is admitted under (LLP 1133.008 D3).
    fn view_origin(&self, r: &Runner<D>, id: ViewId, prop: Option<&str>) -> Option<Cause> {
        None
    }

    /// Install the host's observer of committed device bindings.
    fn observe_view_bindings(
        &self,
        r: &mut Runner<D>,
        props: &[(&str, &str)],
        observer: Option<crate::Announce>,
    ) {
    }

    /// After a commit is applied: its origin is the one a change it made is
    /// reported with.
    fn views_committed(&self, r: &mut Runner<D>) {}

    /// Entry `reference`'s invocation ended `state` (module doc).
    fn wake_entry_ended(&self, r: &mut Runner<D>, reference: u64, state: &str, delivered: bool) {}

    /// Entry `reference`'s invocation was delivered.
    fn wake_entry_running(&self, r: &mut Runner<D>, reference: u64) {}

    /// A module row's substitute, as its declaration reads on this target.
    fn module_substitute(&self, r: &Runner<D>, row: &ModulesRow) -> Arc<Json> {
        Arc::new(Json::Null)
    }

    /// What a module row answers now, by its substitute.
    fn module_value_of(&self, r: &Runner<D>, row: &ModulesRow) -> Option<Json> {
        None
    }

    /// A declared permission's state.
    fn module_permission_state(&self, r: &Runner<D>, name: &str) -> String {
        "prompt".into()
    }

    /// A declared permission's state is set.
    fn set_module_permission(&self, r: &mut Runner<D>, name: &str, state: &str) {}

    /// A module row's topic: `extension.name`, with its owner.
    fn module_key(&self, r: &Runner<D>, row: &ModulesRow) -> String {
        format!("{}.{}", r.plan.str(row.extension), r.plan.str(row.declared))
    }

    /// The local-network permission's state as the substitute answers it, where it stands for the host's fact.
    fn local_network_substitute(&self, r: &Runner<D>) -> Option<crate::local_network::State> {
        None
    }

    /// Whether `token` is a held module request.
    fn holds_token(&self, r: &Runner<D>, token: Option<u64>) -> bool {
        false
    }

    /// The module readers as a commit found them, for its checkpoint.
    fn readers_saved(&self, r: &Runner<D>) -> Readers {
        Readers::new()
    }

    /// A refused commit's: the module readers as it found them.
    fn readers_restore(&self, r: &mut Runner<D>, saved: Readers) {}

    /// The host's target changed: the substitutes read for the old one go
    /// (`Runner::substitute`), and every module's readers ask again.
    fn retarget(&self, r: &mut Runner<D>) {}
}

/// The module seam, not linked.
pub(in crate::runner) struct NoModules;

impl<D: DataSource> Modules<D> for NoModules {}

#[cfg(test)]
mod tests {
    use crate::request::{Dispatch, Request, Work};
    use crate::{CallOrigin, DataError, DataSource, Outcome, Runner, Value};
    use exact_kernel::Kernel;
    use std::sync::{Arc, Mutex};

    struct Empty;

    impl DataSource for Empty {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
    }

    fn boot() -> Runner<Empty> {
        let source = "component App\n  view\n    text \"hello\"\n";
        let plan = contract::compile(source).unwrap();
        Runner::boot(
            plan,
            Empty,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap()
    }

    fn said(r: &Runner<Empty>, line: &str) -> bool {
        r.journal().any(|l| l.ends_with(line))
    }

    #[test]
    fn a_host_reaches_a_plan_with_no_module_as_an_empty_table_answered_it() {
        let mut r = boot();
        // A handler's call is refused once, through its reply.
        let replies = Arc::new(Mutex::new(Vec::new()));
        let to = replies.clone();
        let body = r#"{"extension":"chat","handle":"h1","handler":"say","args":[]}"#;
        r.invoke_module(body, Box::new(move |s| to.lock().unwrap().push(s)));
        let replies = replies.lock().unwrap().clone();
        assert_eq!(replies.len(), 1);
        assert!(
            replies[0].contains("no call that made h1 lists say"),
            "{replies:?}"
        );
        assert!(said(
            &r,
            "chat: say for h1: refused, no call made for it lists the handler"
        ));
        // An event: heard by no one, or refused for its arguments.
        assert_eq!(
            r.raise_module_event("bell", "chime", "[]", CallOrigin::App),
            Ok(())
        );
        assert!(said(&r, "bell.chime: no handler hears it"));
        assert!(r
            .raise_module_event("bell", "chime", "{}", CallOrigin::App)
            .is_err());
        // Binding names the host's own, and nothing else.
        r.bind_extensions(&["camera"], &[]);
        assert!(said(&r, "modules bound here: camera"));
        // Data code's call of a name no row declares.
        let call = Request::native(br#"{"extension":"geo","source":"where","args":{}}"#.to_vec());
        let Some(Dispatch::Run(Work::Now(answer))) = r.module_call(&call) else {
            panic!("refused now");
        };
        let Outcome::Response(response) = answer() else {
            panic!("a response");
        };
        assert_eq!(response.status, 500);
        assert!(String::from_utf8_lossy(&response.body).contains("NotSupportedError"));
        assert!(said(&r, "geo.where: not a module this app's plan names"));
        // Announced topics pass through untouched.
        assert_eq!(r.declaration_topics(vec!["a.b".into()]), ["a.b"]);
        // The agent's operations.
        let prefer = r#"{"op":"prefer","extension":{"geo.where":{}}}"#;
        let (_, reply) = r.extension_agent(prefer).unwrap();
        assert_eq!(
            reply,
            Err(
                "prefer extension: no source `geo.where` in this app; its module sources are none"
                    .into()
            )
        );
        let wake = r#"{"launch":"wake","entry":{"kind":"fetch"}}"#;
        assert_eq!(
            r.listed_op("launchEntry", wake),
            Some(Err(
                "--wake: no module of this app lists registrations of kind `fetch`".into()
            ))
        );
        assert_eq!(r.tap_listed(7), Err("@7 is no listed entry".into()));
        // A module's continuation token names no work here.
        assert!(matches!(
            r.module_dispatch(1 << 62),
            Some(Dispatch::Missing)
        ));
        assert!(r.module_dispatch(7).is_none());
        // The host's target is the core's.
        r.set_module_target("windows");
        assert!(r.places_status_items());
        assert!(r.extensions_json().is_none() && r.admit_picture("photo:1", "ios").is_none());
    }
}
