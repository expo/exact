//! The desktop's modules substituted (LLP 1133.025 §11): a login item and
//! global shortcuts. Under the agent no login item is written and no hot
//! key registered; the runner keeps what the platform would say.
//!
//! - **`"login": "status"`** on a source answers its value with the
//!   registration the app made laid over it, and `launchedAtLogin` for a
//!   launch the agent's `--login` made. **`"register"`** and
//!   **`"unregister"`** on a send record the registration (`state`
//!   `enabled` with its `hidden`, or `disabled`) for the app, keeping a
//!   preferred `requires-approval`, and answer the status.
//! - **`"shortcut": "register"`** on a live source is a registration while
//!   something reads it: an entry `{id, keys, registered, reason}` of the
//!   source's `list`, answered as the entry. A reader's new `keys` lists the
//!   new entry before the old one goes, and the last reader's release
//!   unlists it (`release_idle`). The preferred value of the source sets
//!   `registered` and `reason` for every registration and check, as another
//!   app holding the chord would. `tap @ref` on a registered entry raises
//!   the list's event with a person's origin, as the press would, and keeps
//!   the entry (`"keep": true`). **`"check"`** on a send answers whether
//!   `keys` could be registered, listing nothing.

use super::super::super::provenance::Cause;
use super::super::super::{DataSource, Runner};
use super::super::{Json, ModulesRow};
use super::Listed;
use crate::CallOrigin;
use serde_json::json;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};

/// ARIA's modifier names (`aria-keyshortcuts`).
const MODIFIERS: &[&str] = &["Alt", "Control", "Shift", "Meta"];

/// Why `keys` is not one `aria-keyshortcuts` chord, or `None` when it is:
/// modifiers from [`MODIFIERS`], each once, then one key, joined by `+`; or
/// a modifier pair (`Shift+Shift`), both of one modifier's keys held (LLP
/// 1133.025 D3). ARIA's space-separated alternatives are refused: one chord
/// per registration.
pub fn chord_refusal(keys: &str) -> Option<&'static str> {
    if crate::desktop::single_side_shortcut(keys).is_err() {
        return Some("invalid");
    }
    if keys.is_empty() || keys.contains(char::is_whitespace) {
        return Some("invalid");
    }
    let parts: Vec<&str> = keys.split('+').collect();
    if let [a, b] = parts[..] {
        if a == b && MODIFIERS.contains(&a) {
            return None;
        }
    }
    let Some((key, modifiers)) = parts.split_last() else {
        return Some("invalid");
    };
    let distinct = modifiers
        .iter()
        .enumerate()
        .all(|(i, m)| MODIFIERS.contains(m) && !modifiers[..i].contains(m));
    if key.is_empty() || MODIFIERS.contains(key) || !distinct {
        return Some("invalid");
    }
    None
}

/// An untagged callback may predate a reader's release or rebind. Keep
/// only the fields that have agreed at this site throughout the session.
pub(super) struct Registration {
    source: Option<u32>,
    id: Option<String>,
    keys: Option<String>,
    cause: Option<Cause>,
}

#[derive(Clone, Copy)]
pub(in crate::runner::modules) enum ShortcutPress {
    Native(u64),
    Listed(u64, u64),
}

pub(in crate::runner::modules) struct PointerChord {
    generation: u64,
    confirmed: Arc<AtomicBool>,
    stamp: u64,
    restricted: bool,
    registrars: std::collections::BTreeMap<usize, bool>,
}

fn next_generation() -> u64 {
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

impl Default for PointerChord {
    fn default() -> Self {
        Self {
            generation: next_generation(),
            stamp: next_generation(),
            restricted: false,
            confirmed: Arc::default(),
            registrars: Default::default(),
        }
    }
}

pub(in crate::runner::modules) fn pointer_event(package: &str, name: &str) -> bool {
    package == "@exact/global-shortcuts" && matches!(name, "globalpointerdown" | "globalpointerup")
}

/// A release identifies the same observation as the read, even when a
/// replacement runner uses the same native package instance.
pub(in crate::runner::modules) struct PointerObservation {
    generation: u64,
    listening: AtomicBool,
    native: Option<crate::runner::source::NativeHandler>,
}

fn confirmed(outcome: &crate::Outcome, field: &str) -> bool {
    matches!(outcome, crate::Outcome::Response(r) if serde_json::from_slice::<Json>(&r.body)
        .is_ok_and(|v| v[field] == true))
}

impl crate::Reply {
    /// Only a runner-owned pointer read or its release may reach the OS package.
    pub fn owns_pointer_observation(&self) -> bool {
        self.pointer_owner.as_ref().is_some_and(|live| live())
    }
}

impl Drop for PointerObservation {
    fn drop(&mut self) {
        if let Some(handler) = &self.native {
            let release = json!({"extension":"global-shortcuts", "release":"globalPointer", "args":{"observation":self.generation}});
            let mut reply = crate::Reply::new(|_| {});
            reply.pointer_owner = Some(Box::new(|| true));
            handler(release.to_string().into_bytes(), reply);
        }
    }
}

impl<D: DataSource> Runner<D> {
    /// Where the app's login item registration is kept: in the app's
    /// state, which every session reads.
    fn login_key(&self) -> String {
        format!("{}/login-item/registration", self.data.app_id())
    }

    /// The status a `"login": "status"` source answers, from `value`, its
    /// default and preferred value.
    pub(in crate::runner) fn login_status(&self, mut value: Json) -> Json {
        if super::super::error_code(&value).is_some() || !value.is_object() {
            return value;
        }
        let app = self.extensions.declared.app.as_ref();
        let kept = app.and_then(|a| {
            let a = a.lock().unwrap_or_else(|e| e.into_inner());
            a.get(&self.login_key()).cloned()
        });
        if let Some(kept) = kept {
            if value["state"] != "requires-approval" {
                value["state"] = kept["state"].clone();
            }
            value["hidden"] = kept["hidden"].clone();
        }
        let launch = self.extensions.declared.login_launch;
        value["launchedAtLogin"] = json!(launch.is_some());
        if let (Some(hidden), Json::Null) = (launch, &value["hidden"]) {
            value["hidden"] = json!(hidden);
        }
        value
    }

    /// Whether `row` is answered by this file's forms, here.
    pub(in crate::runner) fn desktop_form(&self, row: &ModulesRow) -> bool {
        let sub = self.substitute(row);
        let form = matches!(sub["login"].as_str(), Some("register" | "unregister"))
            || matches!(sub["shortcut"].as_str(), Some("register" | "check"));
        (form || self.pointer_source(row)) && !self.unsupported_here(row)
    }

    /// A call of a desktop form: its answer, and what it changes.
    pub(in crate::runner) fn desktop_answer(
        &mut self,
        row: &ModulesRow,
        args: &Json,
        cause: Cause,
    ) -> Json {
        let sub = self.substitute(row);
        if self.pointer_source(row) {
            self.note_registration_reader(row, args);
            self.pointer_generation(false);
            return json!({ "listening": true });
        }
        match (sub["login"].as_str(), sub["shortcut"].as_str()) {
            (Some(op), _) => self.login_change(row, op == "register", args),
            (_, Some("register")) => self.shortcut_register(row, args, cause),
            _ => self.shortcut_check(row, args),
        }
    }

    /// `registerLoginItem` or `unregisterLoginItem`: the registration kept
    /// for the app, the status answered and its readers asked again.
    fn login_change(&mut self, row: &ModulesRow, register: bool, args: &Json) -> Json {
        let Some(status) = (self.plan.modules.iter())
            .find(|m| {
                self.plan.str(m.extension) == self.plan.str(row.extension)
                    && self.substitute(m)["login"] == "status"
            })
            .cloned()
        else {
            return json!({ "error": "NotSupportedError" });
        };
        let kept = json!({
            "state": if register { "enabled" } else { "disabled" },
            "hidden": register && args["hidden"] == true,
        });
        let key = self.login_key();
        let app = self.app_state();
        app.lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(key, kept.clone());
        let answer = self.login_status(self.value_of(&status).unwrap_or(json!({})));
        let declared = self.plan.str(row.declared).to_owned();
        self.log(format!(
            "{declared}: {}",
            answer["state"].as_str().unwrap_or("")
        ));
        self.announce(&self.key(&status));
        answer
    }

    pub(in crate::runner) fn pointer_source(&self, row: &ModulesRow) -> bool {
        self.plan.str(row.package) == "@exact/global-shortcuts"
            && self.plan.str(row.declared) == "globalPointer"
    }

    pub(in crate::runner::modules) fn unowned_pointer_release(&self, call: &Json) -> bool {
        call["extension"] == "global-shortcuts"
            && call["release"] == "globalPointer"
            && self.plan.modules.iter().any(|row| self.pointer_source(row))
    }

    fn live_pointer_resource(&self, i: usize) -> bool {
        self.resources.get(i).is_some_and(|r| {
            r.state.as_ref().is_some_and(|s| !s.asleep)
                && (r.reading_pointer || r.pending && r.refreshing_pointer)
        }) && !self.settles_to_nothing(i)
    }

    pub(in crate::runner::modules) fn refused_pointer_reader(&mut self, resource: Option<usize>) {
        if let Some(i) = resource {
            if let Some(readers) = self
                .extensions
                .readers
                .get_mut("global-shortcuts.globalPointer")
            {
                readers.remove(&i);
            }
            self.prune_pointer_readers();
        }
    }

    pub(in crate::runner::modules) fn prune_pointer_readers(&mut self) {
        const KEY: &str = "global-shortcuts.globalPointer";
        if self.extensions.declared.pointer.is_none() {
            return;
        }
        let mut readers = self.extensions.readers.remove(KEY).unwrap_or_default();
        readers.retain(|&i, _| self.live_pointer_resource(i));
        if readers.is_empty() {
            self.extensions.declared.pointer.take();
        } else {
            self.extensions.readers.insert(KEY.into(), readers);
        }
    }

    /// Only the read that opens observation needs its opener. A re-read of
    /// the same live resource can refresh its answer while the window is hidden.
    pub(in crate::runner::modules) fn pointer_reader(&self, row: &ModulesRow) -> bool {
        let Some((super::super::samples::Reader::Resource(i), _)) = &self.extensions.samples.asking
        else {
            return false;
        };
        self.extensions
            .declared
            .pointer
            .as_ref()
            .is_some_and(|p| p.listening.load(Ordering::Acquire))
            && self
                .extensions
                .readers
                .get(&self.key(row))
                .is_some_and(|r| r.contains_key(i))
    }

    pub(in crate::runner::modules) fn pointer_generation(&mut self, native: bool) -> u64 {
        let handler = native
            .then(|| self.native_slot().extension_handler())
            .flatten();
        let old = self
            .extensions
            .declared
            .pointer
            .as_ref()
            .and_then(|p| p.native.as_ref());
        let same_handler = match (old, &handler) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        if !same_handler {
            self.extensions.declared.pointer.take();
        }
        if native && handler.is_none() {
            return 0;
        }
        self.extensions
            .declared
            .pointer
            .get_or_insert_with(|| {
                Arc::new(PointerObservation {
                    generation: next_generation(),
                    listening: AtomicBool::new(handler.is_none()),
                    native: handler,
                })
            })
            .generation
    }

    /// A controlled drive can take over a running recorder. Retire the physical
    /// monitor before the first fixture edge and re-ask on the new route.
    pub(in crate::runner) fn pointer_route_changed(&mut self) {
        let Some(old) = self.extensions.declared.pointer.as_ref() else {
            return;
        };
        let Some(row) = self
            .plan
            .modules
            .iter()
            .find(|row| self.pointer_source(row))
        else {
            return;
        };
        let key = self.key(row);
        let substitute = self.route(row) == super::super::Route::Substitute;
        if self.unsupported_here(row)
            || (!substitute && old.native.is_none())
            || (substitute && old.native.is_some() && !old.listening.load(Ordering::Acquire))
        {
            self.extensions.declared.pointer.take();
        } else if substitute && old.native.is_some() {
            self.pointer_generation(false);
        }
        self.announce(&key);
    }

    /// Contract admitted its read in the commit. A data resource is admitted here,
    /// under that same commit, and only while its exact request remains pending.
    pub(in crate::runner::modules) fn module_request_cause(
        &mut self,
        row: &ModulesRow,
        request: &crate::Request,
    ) -> Result<(Cause, Option<usize>), crate::Dispatch> {
        if self.tracks_pointer_chord(row) {
            if let Some(pending) = request.reader_ticket.and_then(|reader| {
                self.pending
                    .iter()
                    .find(|p| p.ticket == reader.ticket && p.keepable.as_ref() == Some(request))
            }) {
                if let crate::runner::Target::Resource(i) = pending.target {
                    return Ok((self.ticket_cause(pending.ticket), Some(i)));
                }
            }
        }
        if !self.pointer_source(row) {
            let cause = self.arriving_data_cause().with_input_from(
                request
                    .mutation_ticket
                    .map(|t| self.ticket_cause(t))
                    .unwrap_or(Cause::none()),
            );
            return Ok((cause, None));
        }
        let owner = request.reader_ticket.and_then(|reader| {
            let pending = self
                .pending
                .iter()
                .find(|p| p.ticket == reader.ticket && p.keepable.as_ref() == Some(request))?;
            let crate::runner::Target::Resource(i) = pending.target else {
                return None;
            };
            self.live_pointer_resource(i).then_some((
                i,
                pending.pointer_opener.unwrap_or(Cause::none()),
                &pending.source,
            ))
        });
        if let Some((i, opener, source)) = owner {
            return if self.plan.module_named(source).is_some() {
                Err(self.pointer_work(request.body.clone()))
            } else {
                Ok((opener, Some(i)))
            };
        }
        Err(crate::Dispatch::Run(crate::Work::Now(Box::new(|| {
            crate::Outcome::module_refusal(
                "NotAllowedError",
                Some("globalPointer needs a live reader"),
            )
        }))))
    }

    /// A cancelled read may still be waiting on the executor. Hold the
    /// observation only while handing a live read to the package, so its
    /// release follows that handoff even if the runner ends concurrently.
    pub(in crate::runner::modules) fn pointer_work(&self, body: Vec<u8>) -> crate::Dispatch {
        let args: Json = serde_json::from_slice(&body).unwrap_or_default();
        let observation = self
            .extensions
            .declared
            .pointer
            .as_ref()
            .filter(|p| args["args"]["observation"].as_u64() == Some(p.generation))
            .map(Arc::downgrade);
        crate::Dispatch::Run(crate::Work::Later(Box::new(move |reply| {
            let Some(live) = observation.and_then(|p| p.upgrade()) else {
                return reply.send(crate::Outcome::module_refusal("AbortError", None));
            };
            let observation = Arc::downgrade(&live);
            match &live.native {
                Some(handler) => {
                    let owner = observation.clone();
                    let mut package_reply = crate::Reply::new(move |outcome| {
                        if let Some(p) = observation.upgrade() {
                            p.listening
                                .store(confirmed(&outcome, "listening"), Ordering::Release);
                        }
                        reply.send(outcome);
                    });
                    package_reply.pointer_owner = Some(Box::new(move || owner.strong_count() != 0));
                    handler(body, package_reply);
                }
                None => reply.send(crate::Outcome::module_refusal("NotSupportedError", None)),
            }
        })))
    }

    pub(in crate::runner::modules) fn pointer_observes(&self, args: &[Json]) -> bool {
        self.extensions
            .declared
            .pointer
            .as_ref()
            .is_some_and(|p| args.get(1).and_then(Json::as_u64) == Some(p.generation))
    }

    /// The drive submits a button edge to the session currently listening.
    pub(in crate::runner::modules) fn pointer_fixture(&mut self, key: &str, value: &Json) {
        let Some(("global-shortcuts", event @ ("globalpointerdown" | "globalpointerup"))) =
            key.split_once('.')
        else {
            return;
        };
        let Some(p) = &self.extensions.declared.pointer else {
            return;
        };
        if p.native.is_none() && value.is_object() {
            if let Err(why) = self.raise_module_event(
                "global-shortcuts",
                event,
                &json!([value, p.generation]).to_string(),
                CallOrigin::Device,
            ) {
                self.log(why);
            }
        }
    }

    /// `--login`: this launch is the login item's (LLP 1133.025 §11), and
    /// `hidden` what its registration asked.
    pub(in crate::runner::modules) fn launch_login_linked(
        &mut self,
        hidden: bool,
    ) -> Result<String, String> {
        self.extensions.declared.login_launch = Some(hidden);
        self.log(format!(
            "launch --login{}",
            if hidden { " hidden" } else { "" }
        ));
        let keys: Vec<String> = (self.plan.modules.iter())
            .filter(|m| self.substitute(m)["login"] == "status")
            .map(|m| self.key(m))
            .collect();
        for key in keys {
            self.announce(&key);
        }
        Ok(json!({ "launch": "login", "hidden": hidden }).to_string())
    }

    /// The registered and reason a chord answers: invalid, else the
    /// preferred value's, else registered.
    fn chord_answer(&self, row: &ModulesRow, keys: &str) -> (bool, Json) {
        if let Some(reason) = chord_refusal(keys) {
            return (false, json!(reason));
        }
        if crate::desktop::single_side_shortcut(keys) == Ok(true) && self.host_target() != "macos" {
            return (false, json!("unsupported"));
        }
        let register = (self.plan.modules.iter())
            .find(|m| {
                self.plan.str(m.extension) == self.plan.str(row.extension)
                    && self.substitute(m)["shortcut"] == "register"
            })
            .cloned();
        let preferred = register.and_then(|r| self.preferred(&r));
        match preferred {
            Some(p) if p["registered"] == false => (false, p["reason"].clone()),
            _ => (true, Json::Null),
        }
    }

    /// A Contract read of a registration counts its reader with its
    /// arguments, as a bound live source's does (`bound.rs`), so a change of
    /// `keys` and the last reader's going can be told apart; a data-code
    /// read holds the registration until no reader of the name is left.
    pub(in crate::runner) fn note_registration_reader(&mut self, row: &ModulesRow, args: &Json) {
        let key = self.key(row);
        let mut group = args.clone();
        if let Some(map) = group.as_object_mut() {
            map.retain(|_, v| !v.is_null());
        }
        let group = group.to_string();
        if let Some((super::super::samples::Reader::Resource(i), _)) =
            &self.extensions.samples.asking
        {
            let i = *i;
            (self.extensions.readers.entry(key.clone()).or_default()).insert(i, group.clone());
            (self.extensions.groups.entry(key.clone()).or_default()).insert(group);
        }
        self.extensions.live.insert(key);
    }

    /// A read of a registration: listed (replacing the entry of its `id`
    /// with other `keys` only once that reader's old keys are released) and
    /// answered as the entry.
    fn shortcut_register(&mut self, row: &ModulesRow, args: &Json, cause: Cause) -> Json {
        let id = args["id"].as_str().unwrap_or_default().to_owned();
        let keys = args["keys"].as_str().unwrap_or_default().to_owned();
        let (registered, reason) = self.chord_answer(row, &keys);
        let fields = json!({ "id": id, "keys": keys, "registered": registered, "reason": reason });
        let extension = self.plan.str(row.extension).to_owned();
        let state = self.substitute(row)["list"]["state"]
            .as_str()
            .unwrap_or("globalShortcuts")
            .to_owned();
        let source = self
            .plan
            .str(self.plan.sources[row.source.0 as usize].name)
            .to_owned();
        let same = |e: &Listed| {
            e.fields.get("id") == Some(&fields["id"])
                && e.fields.get("keys") == Some(&fields["keys"])
        };
        let listed = self.entries(&extension, &state).into_iter().find(same);
        let reference = match &listed {
            Some(e) => e.reference,
            None => {
                self.next_ticket += 1;
                self.next_ticket - 1
            }
        };
        if !registered {
            if let Some(old) = &listed {
                self.end_shortcut_hold(row, old);
            }
        }
        let previous = listed.as_ref().and_then(|e| e.registered_by);
        let registered_by = registered.then(|| previous.unwrap_or_else(|| cause.unowned()));
        if registered && previous.is_none() {
            self.note_shortcut_site(row, args, cause);
        }
        if registered {
            self.admitted_pointer_chord(row, args, cause);
        }
        let entry = Listed {
            registered_by,
            reference,
            source,
            fields: fields.as_object().cloned().unwrap_or_default(),
        };
        self.with_list(&extension, &state, |entries| {
            entries.retain(|e| !same(e));
            entries.push(entry);
            ((), true)
        });
        if listed.is_none() {
            let how = if registered {
                "registered".to_owned()
            } else {
                format!("not registered ({})", reason.as_str().unwrap_or(""))
            };
            self.log(format!(
                "{}: {keys} for {id}: {how} @{reference}",
                self.key(row)
            ));
        }
        fields
    }

    pub(in crate::runner::modules) fn note_shortcut_site(
        &mut self,
        row: &ModulesRow,
        args: &Json,
        by: Cause,
    ) {
        let reader = match &self.extensions.samples.asking {
            Some((super::super::samples::Reader::Resource(i), _)) => self.resource_site(*i),
            _ => self.plan.resources.len(),
        };
        let by = by.unowned().without_pointer_start();
        let sites = &mut self.extensions.declared.shortcut_sites;
        if sites.is_empty() {
            sites.resize_with(self.plan.resources.len() + 1, || None);
        }
        let site = sites[reader].get_or_insert_with(|| {
            Box::new(Registration {
                source: Some(row.source.0),
                id: args["id"].as_str().map(str::to_owned),
                keys: args["keys"].as_str().map(str::to_owned),
                cause: Some(by),
            })
        });
        if site.source != Some(row.source.0) {
            site.source = None;
        }
        if site.id.as_deref() != args["id"].as_str() {
            site.id = None;
        }
        if site.keys.as_deref() != args["keys"].as_str() {
            site.keys = None;
        }
        if site.cause != Some(by) {
            site.cause = None;
        }
    }

    /// The wire carries an app-chosen id and chord. A currently successful
    /// registration must match, and every site that could have emitted this
    /// payload, including a former reader, must agree on its cause.
    pub(in crate::runner::modules) fn native_shortcut_cause(
        &self,
        extension: &str,
        event: &str,
        args: &[Json],
    ) -> Option<Cause> {
        let [id, shortcut] = args else { return None };
        let keys = shortcut.get("keys")?.as_str()?;
        let id = id.as_str()?;
        if shortcut["id"] != id {
            return None;
        }
        let mut confirmed = false;
        let mut registrar = None;
        for row in self
            .plan
            .modules
            .iter()
            .filter(|m| self.plan.str(m.extension) == extension)
        {
            let sub = self.substitute(row);
            let tap = &sub["list"]["tap"];
            if sub["shortcut"] != "register"
                || (tap["event"] != event && tap["releaseEvent"] != event)
            {
                continue;
            }
            if let Some(state) = sub["list"]["state"].as_str() {
                confirmed |= self.entries(extension, state).iter().any(|e| {
                    e.fields.get("id").and_then(Json::as_str) == Some(id)
                        && e.fields.get("keys").and_then(Json::as_str) == Some(keys)
                        && e.fields.get("registered").and_then(Json::as_bool) == Some(true)
                });
            }
            for reader in self
                .extensions
                .readers
                .get(&self.key(row))
                .into_iter()
                .flat_map(|r| r.keys())
            {
                confirmed |= self.shortcut_registered(*reader, id, keys);
            }
            for site in self.extensions.declared.shortcut_sites.iter().flatten() {
                if site.source.is_some_and(|source| source != row.source.0) {
                    continue;
                }
                if site.id.as_deref().is_some_and(|s| s != id)
                    || site.keys.as_deref().is_some_and(|s| s != keys)
                {
                    continue;
                }
                let by = site.cause?;
                if registrar.is_some_and(|r| r != by) {
                    return None;
                }
                registrar = Some(by);
            }
        }
        registrar.filter(|_| confirmed)
    }

    pub(in crate::runner::modules) fn tracks_pointer_chord(&self, row: &ModulesRow) -> bool {
        self.plan.str(row.package) == "@exact/global-shortcuts"
            && self.plan.str(row.declared) == "globalShortcut"
            && self.plan.modules.iter().any(|r| self.pointer_source(r))
    }

    pub(in crate::runner::modules) fn admitted_pointer_chord(
        &mut self,
        row: &ModulesRow,
        args: &Json,
        cause: Cause,
    ) {
        if !self.tracks_pointer_chord(row) {
            return;
        }
        let Some((super::super::samples::Reader::Resource(i), _)) = self.extensions.samples.asking
        else {
            return;
        };
        let (Some(id), Some(keys)) = (args["id"].as_str(), args["keys"].as_str()) else {
            return;
        };
        let chord = self
            .extensions
            .declared
            .pointer_chords
            .entry((id.into(), keys.into()))
            .or_default();
        // A re-ask polls the same registrar.
        chord
            .registrars
            .entry(i)
            .or_insert(cause.only(CallOrigin::App));
        let reader = &mut self.resources[i];
        reader.session_subscription = true;
        if !reader.pointer_chords.contains(&chord.generation) {
            reader.pointer_chords.push(chord.generation);
        }
    }

    pub(in crate::runner::modules) fn listed_pointer_press(
        &self,
        entry: &Json,
        reference: u64,
    ) -> Option<ShortcutPress> {
        let key = (entry["id"].as_str()?.into(), entry["keys"].as_str()?.into());
        self.extensions
            .declared
            .pointer_chords
            .get(&key)
            .map(|chord| ShortcutPress::Listed(reference, chord.stamp))
    }

    pub(in crate::runner::modules) fn prune_pointer_chords(&mut self) {
        if self.extensions.declared.pointer_chords.is_empty() {
            return;
        }
        let current: std::collections::BTreeSet<_> = self
            .resources
            .iter()
            .enumerate()
            .filter(|(i, r)| {
                r.state.as_ref().is_some_and(|s| !s.asleep) && !self.settles_to_nothing(*i)
            })
            .flat_map(|(i, r)| r.pointer_chords.iter().map(move |g| (*g, i)))
            .collect();
        let native = self.plan.modules.iter().any(|row| {
            self.tracks_pointer_chord(row) && self.route(row) != super::super::Route::Substitute
        });
        let handler = native
            .then(|| self.native_slot().extension_handler())
            .flatten();
        let chords = &mut self.extensions.declared.pointer_chords;
        chords.retain(|_, chord| {
            chord
                .registrars
                .retain(|i, _| current.contains(&(chord.generation, *i)));
            !chord.registrars.is_empty()
        });
        let restricted: std::collections::BTreeSet<_> = chords
            .iter()
            .filter(|(_, c)| c.registrars.values().any(|app| !app))
            .map(|((_, keys), _)| keys.clone())
            .collect();
        for ((id, keys), chord) in chords {
            let mixed = restricted.contains(keys);
            if chord.restricted && !mixed {
                // A press queued while the chord was shared cannot acquire the remaining registrar's authority.
                chord.stamp = next_generation();
                chord.confirmed = Arc::default();
                if let Some(handler) = &handler {
                    let result = chord.confirmed.clone();
                    let read = json!({"extension":"global-shortcuts","source":"globalShortcut",
                        "args":{"id":id,"keys":keys,"registration":chord.stamp}});
                    handler(
                        read.to_string().into_bytes(),
                        crate::Reply::new(move |v| {
                            result.store(confirmed(&v, "registered"), Ordering::Release)
                        }),
                    );
                }
            }
            chord.restricted = mixed;
        }
    }

    pub(in crate::runner::modules) fn pointer_chord_work(
        &self,
        native: &crate::Request,
        args: &Json,
        body: Vec<u8>,
    ) -> crate::Dispatch {
        use crate::{Dispatch, Reply, Work};
        let key = (
            args["id"].as_str().unwrap_or_default().into(),
            args["keys"].as_str().unwrap_or_default().into(),
        );
        let mut envelope: Json = serde_json::from_slice(&body).expect("forwarded module call");
        // Only the runner can attach a registration to a package call.
        if let Some(args) = envelope["args"].as_object_mut() {
            args.remove("registration");
        }
        let reader = native.reader_ticket.and_then(|t| {
            self.pending
                .iter()
                .find(|p| p.ticket == t.ticket && p.keepable.as_ref() == Some(native))
        });
        let chord = self
            .extensions
            .declared
            .pointer_chords
            .get(&key)
            .filter(|chord| {
                reader.is_some_and(|p| match p.target {
                    crate::runner::Target::Resource(i) => {
                        self.resources[i].pointer_chords.contains(&chord.generation)
                    }
                    _ => false,
                })
            });
        let Some(chord) = chord else {
            return self.to_implementation(envelope.to_string().into_bytes());
        };
        envelope["args"]["registration"] = json!(chord.stamp);
        let result = chord.confirmed.clone();
        let handler = self.native_slot().extension_handler();
        Dispatch::Run(Work::Later(Box::new(move |reply| match handler {
            Some(handler) => handler(
                envelope.to_string().into_bytes(),
                Reply::new(move |outcome| {
                    result.store(confirmed(&outcome, "registered"), Ordering::Release);
                    reply.send(outcome);
                }),
            ),
            None => reply.send(crate::Outcome::module_refusal("NotSupportedError", None)),
        })))
    }

    pub(in crate::runner::modules) fn shortcut_opens_pointer(
        &self,
        press: ShortcutPress,
        args: &[Json],
    ) -> bool {
        let [id, shortcut] = args else { return false };
        let (Some(id), Some(keys)) = (id.as_str(), shortcut["keys"].as_str()) else {
            return false;
        };
        if shortcut["id"] != id {
            return false;
        }
        let Some(chord) = self
            .extensions
            .declared
            .pointer_chords
            .get(&(id.into(), keys.into()))
        else {
            return false;
        };
        if chord.restricted {
            return false;
        }
        match press {
            ShortcutPress::Native(generation) => {
                chord.stamp == generation
                    && chord.confirmed.load(Ordering::Acquire)
                    && self.read_by_any("global-shortcuts.globalShortcut")
            }
            ShortcutPress::Listed(reference, stamp) => {
                chord.stamp == stamp
                    && self.plan.modules.iter().any(|row| {
                        self.tracks_pointer_chord(row)
                            && self.route(row) == super::super::Route::Substitute
                    })
                    && self
                        .entries("global-shortcuts", "globalShortcuts")
                        .iter()
                        .any(|e| {
                            e.reference == reference
                                && e.fields["registered"] == true
                                && e.fields["id"] == id
                                && e.fields["keys"] == keys
                        })
            }
        }
    }

    fn shortcut_registered(&self, reader: usize, id: &str, keys: &str) -> bool {
        let Some(state) = self.resources.get(reader).and_then(|r| r.state.as_ref()) else {
            return false;
        };
        let crate::Value::Record(values) = state.value.get(&self.plan) else {
            return false;
        };
        let fields = self.plan.type_(self.resource_row(reader).ty).fields;
        let field = |name: &str| {
            fields
                .iter()
                .position(|f| self.plan.str(self.plan.field(f).name) == name)
                .and_then(|i| values.get(i))
        };
        !state.asleep
            && field("registered") == Some(&crate::Value::Bool(true))
            && field("id").and_then(crate::Value::as_str) == Some(id)
            && field("keys").and_then(crate::Value::as_str) == Some(keys)
    }

    /// `checkGlobalShortcut`: whether `keys` could be registered now.
    fn shortcut_check(&mut self, row: &ModulesRow, args: &Json) -> Json {
        let keys = args["keys"].as_str().unwrap_or_default().to_owned();
        let (registered, reason) = self.chord_answer(row, &keys);
        self.log(format!(
            "{}: {keys}: {}",
            self.key(row),
            if registered {
                "registered"
            } else {
                reason.as_str().unwrap_or("")
            }
        ));
        json!({ "keys": keys, "registered": registered, "reason": reason })
    }

    /// A registration source's readers no longer ask with `args`, or none
    /// is left (`None`): its entries go from the list.
    pub(in crate::runner) fn shortcut_released(&mut self, row: &ModulesRow, args: Option<&Json>) {
        let extension = self.plan.str(row.extension).to_owned();
        let Some(state) = self.substitute(row)["list"]["state"]
            .as_str()
            .map(str::to_owned)
        else {
            return;
        };
        let gone = |e: &Listed| {
            args.is_none_or(|a| {
                e.fields.get("id") == Some(&a["id"]) && e.fields.get("keys") == Some(&a["keys"])
            })
        };
        let removed = self.with_list(&extension, &state, |entries| {
            let removed: Vec<_> = entries.extract_if(.., |entry| gone(entry)).collect();
            let changed = !removed.is_empty();
            (removed, changed)
        });
        for entry in removed {
            let reference = entry.reference;
            self.end_shortcut_hold(row, &entry);
            self.log(format!("{}: unregistered @{reference}", self.key(row)));
        }
    }

    fn end_shortcut_hold(&mut self, row: &ModulesRow, entry: &Listed) {
        if self
            .extensions
            .declared
            .held_shortcuts
            .contains(&entry.reference)
        {
            if let Err(reason) = self.tap_registration(row, entry, Some("up")) {
                self.log(reason);
            }
        }
    }

    /// `tap @ref` on a registration (`"keep": true`): its event raised, as
    /// a press of the chord would, the entry kept; refused on one that is
    /// not registered.
    pub(super) fn tap_registration(
        &mut self,
        row: &ModulesRow,
        entry: &Listed,
        edge: Option<&str>,
    ) -> Result<String, String> {
        if entry.fields.get("registered") != Some(&json!(true)) {
            return Err(format!(
                "@{} is not registered ({}): no press reaches it",
                entry.reference,
                entry
                    .fields
                    .get("reason")
                    .and_then(Json::as_str)
                    .unwrap_or("unknown")
            ));
        }
        let tap = &self.substitute(row)["list"]["tap"];
        let edge = edge.filter(|_| tap["releaseEvent"].is_string());
        let up = tap["releaseEvent"].as_str().filter(|_| {
            tap["releaseTargets"]
                .as_array()
                .is_some_and(|ts| ts.iter().any(|t| t == self.host_target()))
        });
        let reference = entry.reference;
        let held = self.extensions.declared.held_shortcuts.contains(&reference);
        match edge {
            Some("down" | "up") if up.is_none() => {
                return Err("separate shortcut edges are unsupported here".into())
            }
            None | Some("down") if held => return Err(format!("@{reference} is already held")),
            Some("up") if !held => return Err(format!("@{reference} is not held")),
            None | Some("down" | "up") => (),
            Some(_) => return Err("a shortcut tap takes down, up, or no choice".into()),
        }
        let fields = Json::Object(entry.fields.clone());
        let mut result = String::new();
        if edge != Some("up") {
            result = self.activate_entry(row, &fields, reference, entry.registered_by)?;
            if edge == Some("down") {
                self.extensions.declared.held_shortcuts.insert(reference);
                return Ok(result);
            }
        }
        if let Some(up) = up {
            let extension = self.plan.str(row.extension).to_owned();
            self.queue_module_event(
                &extension,
                up,
                super::entry_args(tap, &fields),
                CallOrigin::App,
                false,
                (entry.registered_by, None),
            )?;
            if edge == Some("up") {
                self.extensions.declared.held_shortcuts.remove(&reference);
                return Ok(json!({"ref":reference,"event":up,"delivery":"substituted"}).to_string());
            }
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests {
    use super::chord_refusal;

    #[test]
    fn a_chord_is_one_aria_shortcut_or_a_modifier_pair() {
        for ok in [
            "Meta+Shift+2",
            "Control+Alt+K",
            "F12",
            "Shift+Shift",
            "Meta+Meta",
        ] {
            assert_eq!(chord_refusal(ok), None, "{ok}");
        }
        for bad in [
            "",
            "Meta+K Control+K",
            "Meta+",
            "Shift",
            "Meta+Meta+K",
            "Shift+Control",
            "Hyper+K",
        ] {
            assert_eq!(chord_refusal(bad), Some("invalid"), "{bad}");
        }
    }
}
