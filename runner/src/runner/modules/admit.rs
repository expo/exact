//! One admission for every call of a module's name, from every caller
//! (LLP 1133.002 D14), before anything answers it: the substitute, a ticket,
//! or a bound implementation, in production or under the agent.
//!
//! In order: the grant lines the name or its arguments list (LLP 1133.009
//! D10 item 9; the interface's own lines are the bake's, `grant-missing`),
//! and those an argument's value must be the target of (LLP 1133.011 D6,
//! `ceremony.rs`), which refuse with their own code;
//! the name's `origin` (`"app"`: a tool invocation or a wake is refused,
//! `person-required`, item 8; `{"whenAny", "is": "app"}`, only when the call
//! gives one of the options named, LLP 1133.012 D12 item 5); its
//! `foreground` (refused while the page is
//! hidden, item 3); each conditioned by its `when` where it has one; then the
//! permission. Every other refusal answers `NotAllowedError`, with the
//! message `person-required` for the origin's.
//!
//! **Routes.** A name is answered by its implementation as in production
//! where the host bound it so (outside the agent, or `--bind`), and the
//! platform decides its permission; by its implementation told it runs
//! under the agent where the interface is its own substitute
//! (`"substitute": "implementation"`, LLP 1133.006 D8, LLP 1133.008 D8 item
//! 9), behind the runner's permission and its tickets, the agent's
//! preferred value for the name laid over each answer; else by its
//! declared substitute (§6).
//!
//! Each call is admitted under its origin and causes (`provenance.rs`).

use super::super::processes::{given, held_to_app};
use super::super::provenance::{CallOrigin, ORIGINS};
use super::super::{DataSource, Runner};
use super::{error_code, Decision, Json, ModulesRow, Waits};
use serde_json::json;

/// What answers a module's name.
#[derive(Clone, Copy, PartialEq, Eq)]
pub(in crate::runner) enum Route {
    /// The host's implementation, as in production.
    Real,
    /// The host's implementation, told it runs under the agent.
    Agent,
    /// The declared substitute.
    Substitute,
}

impl<D: DataSource> Runner<D> {
    /// Whether invocation `id` owns module work: a native statement or a
    /// held call stamped with it ([`Self::invocation_owns`]).
    pub(super) fn modules_own_linked(&self, id: u64) -> bool {
        (self.extensions.declared.statements.lock())
            .unwrap_or_else(|e| e.into_inner())
            .contains_key(&id)
            || (self.extensions.held.iter()).any(|h| h.id == id)
    }

    /// Whether the host bound its own implementation of `extension`.
    pub(super) fn bound_here_linked(&self, extension: &str) -> bool {
        self.extensions.bound.contains(extension)
    }

    /// Whether a host's implementation answers some name of `extension`.
    pub(super) fn implemented_linked(&self, extension: &str) -> bool {
        (self.plan.modules.iter())
            .any(|m| self.plan.str(m.extension) == extension && self.route(m) != Route::Substitute)
    }

    /// Invocation `id`'s native statements are dropped: it expired.
    pub(super) fn drop_statements_of_linked(&mut self, id: u64) {
        (self.extensions.declared.statements.lock())
            .unwrap_or_else(|e| e.into_inner())
            .remove(&id);
    }

    /// Invocation `id`'s held calls end `AbortError` with `message`.
    pub(super) fn abort_held_of_linked(&mut self, id: u64, message: &str) {
        let held: Vec<(u64, u64)> = (self.extensions.held.iter())
            .filter(|h| h.id == id && id != 0)
            .map(|h| (h.token, h.hold))
            .collect();
        for (token, hold) in held {
            self.retire_hold(hold);
            self.release(token, json!({ "error": "AbortError", "message": message }));
        }
    }

    /// Resource row `i` is a new member's: its last poll answer no longer
    /// stands.
    pub(super) fn forget_module_row_linked(&mut self, i: usize) {
        for within in self.extensions.within.values_mut() {
            within.remove(&i);
        }
    }

    pub(super) fn implementation_here(&self, row: &ModulesRow) -> bool {
        let sub = self.substitute(row);
        let target = self.host_target();
        sub["implementationTargets"]
            .as_array()
            .is_none_or(|targets| targets.iter().any(|t| t == target))
            && !sub["implementationOverrides"]
                .as_array()
                .is_some_and(|targets| targets.iter().any(|t| t == target))
    }

    /// What answers `row` now (module doc).
    pub(in crate::runner) fn route(&self, row: &ModulesRow) -> Route {
        let extension = self.plan.str(row.extension);
        if !self.implementation_here(row) {
            return Route::Substitute;
        }
        if self.extensions.bound.contains(extension) && !self.kept_by_runner(row) {
            Route::Real
        } else if self.extensions.bound.contains(extension) {
            // The runner keeps the list it hands over (`declared/handed.rs`).
            Route::Substitute
        } else if self.extensions.agent.contains(extension)
            && self.substitute(row)["implementation"] == true
        {
            Route::Agent
        } else {
            Route::Substitute
        }
    }

    /// Admission (module doc) and then what answers: a refusal or the
    /// substitute's answer, a ticket, or the implementation.
    pub(super) fn admitted(
        &mut self,
        row: &ModulesRow,
        args: &Json,
        origin: CallOrigin,
        send: bool,
        timeout: Option<f64>,
    ) -> Decision {
        // A member is an option of its owner's call, never a call of its own.
        if let Some(owner) = self.substitute(row)["memberOf"].as_str() {
            let why = format!(
                "{} is a member of {owner}: pass it to that call",
                self.key(row)
            );
            return Decision::Now(json!({ "error": "NotSupportedError", "message": why }));
        }
        // Presence is not delegable (LLP 1133.010 D12): a person's check
        // fails for a tool or a wake before anything is asked, in every state.
        if self.substitute(row)["origin"] == "person" && origin != CallOrigin::App {
            self.log(format!(
                "{}: refused, person-required ({} origin)",
                self.key(row),
                origin.word()
            ));
            self.call_person_required();
            return Decision::Now(
                json!({ "error": "PERMISSION_DENIED", "message": "person-required" }),
            );
        }
        // A grant matched by the argument's value fails with its own code
        // (`SecurityError` for an RP ID, LLP 1133.011 D6).
        if let Err((code, why)) = self.value_grants(row, args) {
            self.log(format!("{}: refused, {why}", self.key(row)));
            return Decision::Now(json!({ "error": code, "message": why }));
        }
        if let Some(refused) = self.undeclared(row, args) {
            return Decision::Now(refused);
        }
        if let Err(why) = self.admit(row, args, origin) {
            self.log(format!("{}: refused, {why}", self.key(row)));
            if why.starts_with("person-required") {
                self.call_person_required();
                return Decision::Now(
                    json!({ "error": "NotAllowedError", "message": "person-required" }),
                );
            }
            return Decision::Now(json!({ "error": "NotAllowedError" }));
        }
        // A member's option on the call, by the member's rules (LLP 1133.014 D10 item 1).
        if let Some(refused) = self.members_refused(row, args) {
            let why = refused["message"].as_str().unwrap_or_default();
            self.log(format!("{}: refused, {why}", self.key(row)));
            return Decision::Now(refused);
        }
        let decision = match self.route(row) {
            Route::Real => return Decision::Forward,
            // A target's own restriction holds under the agent too (D14 step 2).
            Route::Agent if self.unsupported_here(row) => {
                Decision::Now(json!({ "error": "NotSupportedError" }))
            }
            // No dialog: the implementation opens it as the app's own or fails.
            Route::Agent if self.owned_call(row) => Decision::Forward,
            // The permission's ticket, a sheet's, or the raise's on the
            // runner's own status (LLP 1133.018 D13 item 4); else the implementation.
            Route::Agent => (self.decide_permission(row, send, args))
                .or_else(|| self.decide_choose(row, send))
                .or_else(|| self.decide_raise(row, send))
                .unwrap_or(Decision::Forward),
            Route::Substitute
                if self.substitute(row)["shortcuts"] == "pin" && !self.unsupported_here(row) =>
            {
                Decision::Now(self.pin_shortcut(row, args))
            }
            // The app's icon (LLP 1163.000 D4, `declared/app_icon.rs`).
            Route::Substitute
                if self.substitute(row)["appIcon"] == "set" && !self.unsupported_here(row) =>
            {
                self.app_icon_decide(row, args)
            }
            // A login item's registration and a global shortcut's (LLP
            // 1133.025 §11, `declared/desktop.rs`).
            Route::Substitute if self.desktop_form(row) => {
                if !send && self.substitute(row)["shortcut"] == "register" {
                    self.note_registration_reader(row, args);
                }
                Decision::Now(self.desktop_answer(row, args))
            }
            // A host that names its target after boot: what the first read
            // registered goes with the answer it no longer has.
            Route::Substitute if self.substitute(row)["shortcut"] == "register" => {
                self.shortcut_released(row, None);
                self.decide_timed(row, send, timeout, args)
            }
            Route::Substitute => match self.device_decide(row, send, args) {
                Some(decision) => decision,
                None => self.decide_timed(row, send, timeout, args),
            },
        };
        // A name whose grants are per argument asks permission only for an
        // argument it was given: `getUserMedia(audio=false)` asks for no
        // device, and its implementation refuses it at once (LLP 1133.009
        // D3).
        let asks = match &self.substitute(row)["nameGrants"] {
            Json::Object(by_argument) => (by_argument.keys()).any(|a| given(args, a)),
            _ => true,
        };
        let decision = match decision {
            Decision::Hold(Waits::Permission) if !asks => match self.route(row) {
                Route::Agent => Decision::Forward,
                _ => Decision::Now(json!({ "error": "TypeError" })),
            },
            decision => decision,
        };
        let decision = match decision {
            // Only a person answers a prompt (D12).
            Decision::Hold(Waits::Permission) if origin != CallOrigin::App => {
                self.call_person_required();
                let code = self.permission_of(row, args)["denied"].clone();
                self.log(format!(
                    "{}: refused, person-required ({} origin)",
                    self.key(row),
                    origin.word()
                ));
                Decision::Now(json!({ "error": code }))
            }
            decision => decision,
        };
        self.span_admitted(row, args, origin, &decision);
        decision
    }

    /// An option whose values are the app's declarations takes one of them
    /// (LLP 1133.004 stage 2): the platform shows only what was registered
    /// at launch, so data code cannot add to it. The refusal, journaled.
    pub(super) fn undeclared(&mut self, row: &ModulesRow, args: &Json) -> Option<Json> {
        let sub = self.substitute(row);
        let declares = sub["declares"].as_object()?;
        let source = &self.plan.sources[row.source.0 as usize];
        let shape = self
            .plan
            .source_params
            .get(source.params.start as usize)
            .and_then(|p| exact_js_value::Shape::from_plan(&self.plan, p.ty).ok());
        let option = declares.iter().find(|(option, _)| {
            let field = match &shape {
                Some(exact_js_value::Shape::Record(fields)) => fields
                    .iter()
                    .find(|(name, _)| name == *option)
                    .map(|(_, field)| field),
                _ => None,
            };
            let normalize = |value: &Json| {
                let mut value = value.clone();
                if let Some(field) = field {
                    super::absent_is_none(&mut value, field);
                }
                value
            };
            let given = normalize(&args[option.as_str()]);
            let declared = sub["declared"][option.as_str()].as_array();
            !given.is_null() && !declared.is_some_and(|d| d.iter().any(|v| normalize(v) == given))
        })?;
        let why = format!(
            "{} not declared: only a literal the app's Contract writes declares them",
            option.0
        );
        self.log(format!("{}: refused, {why}", self.key(row)));
        Some(json!({ "error": "TypeError", "message": why }))
    }

    /// The permission a call of `row` with `args` asks: its declared one,
    /// or, where its interface declares several and the name's grants are a
    /// line per argument (`permissionBy`), the one of the arguments it was
    /// given that settles the call now: a denied one, else one still to ask,
    /// else the first (LLP 1133.008 D6, LLP 1133.009 D3).
    pub(in crate::runner) fn permission_of(&self, row: &ModulesRow, args: &Json) -> Json {
        // A permission chosen by an argument's value, or the first still to
        // ask of those a request asks together (`keyed.rs`).
        if let Some(p) = self.permission_by_value(row, args) {
            return p;
        }
        if let Some(names) = self.each_asked(row, args) {
            let first =
                (names.iter().find(|n| self.permission_state(n) == "prompt")).or(names.first());
            return first.map_or(Json::Null, |n| self.declared_permission(n));
        }
        let sub = self.substitute(row);
        // The first of the name's permissions whose grant line the app
        // declares (`"narrowest"`, LLP 1133.019 D4).
        if let Some(any) = sub["permissionAny"].as_array() {
            let declares = |p: &&Json| {
                (p["grants"].as_array().into_iter().flatten())
                    .filter_map(Json::as_str)
                    .any(|l| {
                        (self.data.grants().lines()).any(|g| g.split_whitespace().next() == Some(l))
                    })
            };
            return any
                .iter()
                .find(declares)
                .or(any.first())
                .cloned()
                .unwrap_or_default();
        }
        let Some(by) = sub["permissionBy"].as_object() else {
            return self.unsettled(row, &sub);
        };
        let given: Vec<&Json> = (by.iter())
            .filter(|(a, _)| given(args, a))
            .map(|(_, p)| p)
            .collect();
        let state = |p: &&Json| self.permission_state(p["name"].as_str().unwrap_or_default());
        (given.iter().find(|p| state(p) == "denied"))
            .or_else(|| given.iter().find(|p| state(p) == "prompt"))
            .or(given.first())
            .map_or_else(|| sub["permission"].clone(), |p| (*p).clone())
    }

    /// The permission a call of `row` asks of its own and those it requires
    /// on this target (LLP 1133.030 D12 item 1): a denied one, its own
    /// first; else the first required one still `prompt`; else its own. A
    /// status source answers its own.
    fn unsettled(&self, row: &ModulesRow, sub: &Json) -> Json {
        let own = &sub["permission"];
        let required = self.required(own);
        if required.is_empty() || own["status"] == self.plan.str(row.declared) {
            return own.clone();
        }
        let name = own["name"].as_str().unwrap_or_default();
        if self.own_permission_state(name, own) == "denied" {
            return own.clone();
        }
        let states: Vec<(String, String)> = (required.into_iter())
            .map(|r| {
                let state = self.permission_state(&r);
                (r, state)
            })
            .collect();
        (states.iter().find(|(_, s)| s == "denied"))
            .or_else(|| states.iter().find(|(_, s)| s == "prompt"))
            .map_or_else(|| own.clone(), |(r, _)| self.permission_named(row, r))
    }

    /// `row`'s substitute with the permission a call with `args` asks.
    pub(in crate::runner) fn substitute_for(&self, row: &ModulesRow, args: &Json) -> Json {
        let mut sub = (*self.substitute(row)).clone();
        if sub["permissionBy"].is_object()
            || sub["permissionAny"].is_array()
            || sub["permissionValue"].is_array()
            || sub["permission"]["requires"].is_object()
        {
            sub["permission"] = self.permission_of(row, args);
        }
        sub
    }

    /// [`Self::permission_of`]'s name; empty for none.
    pub(in crate::runner) fn permission_name(&self, row: &ModulesRow, args: &Json) -> String {
        // A request asking several together holds them all, comma-joined.
        if let Some(names) = self.each_asked(row, args).filter(|_| row.send) {
            return names.join(",");
        }
        (self.permission_of(row, args)["name"].as_str())
            .unwrap_or_default()
            .to_owned()
    }

    /// The permission `name` as `row` declares it: its own, one chosen by
    /// argument, or one its own requires on this target.
    pub(in crate::runner) fn permission_named(&self, row: &ModulesRow, name: &str) -> Json {
        let sub = self.substitute(row);
        // A name that asks by its arguments' values asks any of its own.
        if sub["permissionValue"].is_array() || sub["permissionEach"].is_array() {
            let p = self.declared_permission(name);
            let own = (sub["permissionList"].as_array().into_iter().flatten())
                .filter_map(|d| d["name"].as_str())
                .any(|d| d == name || d.strip_suffix('*').is_some_and(|pre| name.starts_with(pre)));
            return if own { p } else { Json::Null };
        }
        let by = sub["permissionBy"].as_object().into_iter().flatten();
        let any = sub["permissionAny"].as_array().into_iter().flatten();
        let required = self.required(&sub["permission"]);
        let requires = (sub["permissionList"].as_array().into_iter().flatten())
            .filter(|p| required.iter().any(|r| p["name"] == r.as_str()));
        (std::iter::once(&sub["permission"]).chain(by.map(|(_, p)| p)))
            .chain(any)
            .chain(requires)
            .find(|p| p["name"] == name)
            .cloned()
            .unwrap_or_default()
    }

    /// Whether `row` asks permission `name` for some call.
    pub(in crate::runner) fn asks_permission(&self, row: &ModulesRow, name: &str) -> bool {
        !self.permission_named(row, name).is_null()
    }

    /// Permission `granted` was just granted to held call `token`: when the
    /// call needs another that is not yet settled, it is denied with it, or
    /// held under its ticket, and `true`; `false` when nothing more is asked.
    pub(in crate::runner) fn ask_next(
        &mut self,
        row: &ModulesRow,
        send: bool,
        token: u64,
        granted: &str,
        args: &Json,
    ) -> bool {
        let name = self.permission_name(row, args);
        if name.is_empty() || name == granted {
            return false;
        }
        match self.decide_permission(row, send, args) {
            Some(Decision::Now(answer)) => self.release(token, answer),
            Some(Decision::Hold(waits)) => {
                if let Some((held_by, origin)) = self.held_name(token, row) {
                    let hold = self.device_hold(row, held_by, waits, origin, &name, args);
                    if let Some(h) = self.extensions.held.iter_mut().find(|h| h.token == token) {
                        h.hold = hold;
                        h.permission = name;
                    }
                }
            }
            _ => return false,
        }
        true
    }

    /// What held call `token` is held for, as its ticket names it (a
    /// resource or mutation, or a data-code call's key), and its origin.
    pub(in crate::runner) fn held_name(
        &self,
        token: u64,
        row: &ModulesRow,
    ) -> Option<(String, CallOrigin)> {
        let target = (self.pending.iter())
            .find(|p| p.continuation == Some(token))
            .map(|p| p.target);
        let held = self.extensions.held.iter().find(|h| h.token == token)?;
        let name = match (target, held.reply.is_some()) {
            (Some(target), _) => self.target_name(target),
            (None, true) => self.key(row),
            (None, false) => return None,
        };
        Some((name, held.origin))
    }

    /// The checks before any decision: why the call is refused, if it is.
    pub(super) fn admit(
        &self,
        row: &ModulesRow,
        args: &Json,
        origin: CallOrigin,
    ) -> Result<(), String> {
        let sub = self.substitute(row);
        let lines: Vec<&str> = (self.data.grants().lines())
            .filter_map(|l| l.split_whitespace().next())
            .collect();
        let granted = |v: &Json| match v {
            Json::String(line) => lines.contains(&line.as_str()),
            Json::Array(any) => any
                .iter()
                .any(|l| l.as_str().is_some_and(|l| lines.contains(&l))),
            _ => true,
        };
        let named = |v: &Json| match v {
            Json::Array(any) => (any.iter().filter_map(Json::as_str))
                .collect::<Vec<_>>()
                .join(" or "),
            other => other.as_str().unwrap_or_default().to_owned(),
        };
        match &sub["nameGrants"] {
            Json::Object(by_argument) => {
                for (argument, line) in by_argument {
                    if given(args, argument) && !granted(line) {
                        return Err(format!("grant-missing ({argument}= needs {})", named(line)));
                    }
                }
            }
            // An empty list needs no line (a picker's, LLP 1133.018 D4).
            line @ Json::Array(l) if !l.is_empty() && !granted(line) => {
                return Err(format!("grant-missing ({})", named(line)));
            }
            _ => {}
        }
        // A name any of several lines admits asks the permission of one the
        // app declares, or is let through as a read of the app's own item
        // (`owned`, LLP 1133.019 D11 item 4).
        if sub["nameGrants"].as_array().is_some_and(|l| l.len() > 1) && !self.owned_call(row) {
            let permission = self.permission_of(row, args);
            let lines: Vec<&Json> = permission["grants"]
                .as_array()
                .into_iter()
                .flatten()
                .collect();
            if !lines.is_empty() && !lines.iter().any(|l| granted(l)) {
                return Err(format!(
                    "grant-missing (permission {})",
                    permission["name"].as_str().unwrap_or_default()
                ));
            }
        }
        // A path option is the app's own file, within the calling source's
        // `fs.read` or `fs.write` lines (LLP 1133.009 D10 item 4); each of a
        // list's (LLP 1133.024 D16); a keyed shape's path keys (`keys`),
        // each item's, an item being the option or each of its list (LLP
        // 1133.006 D8 item 3). One declared a `directory` names a directory,
        // an app root itself included.
        for (option, path) in sub["paths"].as_object().into_iter().flatten() {
            let given: Vec<&str> = match (&args[option], path["keys"].as_array()) {
                (Json::Array(items), Some(keys)) => (items.iter())
                    .flat_map(|item| keys.iter().filter_map(|k| item[k.as_str()?].as_str()))
                    .collect(),
                (item, Some(keys)) => keys
                    .iter()
                    .filter_map(|k| item[k.as_str()?].as_str())
                    .collect(),
                (Json::String(one), None) => vec![one],
                (Json::Array(list), None) => list.iter().filter_map(Json::as_str).collect(),
                _ => continue,
            };
            let capability = format!("fs.{}", path["access"].as_str().unwrap_or("read"));
            let directory = path["directory"] == true;
            if let Some(given) = given.into_iter().find(|given| {
                let (plain, at) = match directory {
                    true => (
                        crate::save_file::plain_app_directory(given),
                        given.strip_suffix('/').unwrap_or(given),
                    ),
                    false => (crate::save_file::plain_app_path(given), *given),
                };
                !plain || !self.covers(&capability, at)
            }) {
                return Err(format!(
                    "grant-missing ({option}={given} needs {capability})"
                ));
            }
        }
        // An option's records name URLs and app files (`"admit"`, LLP
        // 1133.017 D3): each admitted as a fetch's URL or a path option is.
        for (option, fields) in sub["admits"].as_object().into_iter().flatten() {
            let records: Vec<&Json> = match &args[option] {
                Json::Array(list) => list.iter().collect(),
                record @ Json::Object(_) => vec![record],
                _ => Vec::new(),
            };
            for (i, record) in records.into_iter().enumerate() {
                for (field, capability) in fields.as_object().into_iter().flatten() {
                    let capability = capability.as_str().unwrap_or_default();
                    let at = format!("{option}[{i}]");
                    admit_field(record, field, &at, &|at, given| {
                        if capability == "net.fetch" {
                            if let Some(why) =
                                fetch_refusal(self.links.grants, self.data.grants(), at, given)
                            {
                                return Err(why);
                            }
                        } else if !crate::save_file::plain_app_path(given)
                            || !self.covers(capability, given)
                        {
                            return Err(format!("grant-missing ({at}={given} needs {capability})"));
                        }
                        Ok(())
                    })?;
                }
            }
        }
        let applies = sub["when"]
            .as_object()
            .is_none_or(|when| when.iter().all(|(k, v)| args.get(k) == Some(v)));
        let app_only = held_to_app(&sub, args);
        // `{"none": [origins]}`: refused while any of them is among the
        // call's possible causes, read before they are folded to one (LLP
        // 1133.035 D16 item 6).
        if let Some(none) = sub["origin"]["none"].as_array().filter(|_| applies) {
            let causes = self.calling_causes();
            let found = (none.iter().filter_map(Json::as_str))
                .find(|w| ORIGINS.iter().any(|o| o.word() == *w && causes.has(*o)));
            if let Some(word) = found {
                return Err(format!("person-required (a {word} among its causes)"));
            }
        }
        // A sampled source's tool or wake reader joins a stream the app's own
        // consumers run, as it runs; one that would start it is refused
        // (LLP 1133.013 D4, D11).
        // A reader of a `join` name joins a group the app's own readers run
        // (LLP 1133.036 D3).
        let joins = (sub["samples"]["stream"].as_str()).is_some_and(|s| self.app_rate(s).is_some())
            || self.joins_running(row, args);
        // A background action is the person's act: it may call a name held
        // to the app, which shows nothing there (the foreground rule below
        // refuses one that would).
        let persons = origin == CallOrigin::App
            || (origin == CallOrigin::Action && !self.press_revoked(self.invocation_id()));
        if applies && app_only && !persons && !joins {
            return Err(format!("person-required ({} origin)", origin.word()));
        }
        // A call the app holds admits a capture's start while hidden on iOS
        // and Android, whose platforms keep a call's microphone (LLP 1133.033
        // D10); the origin rule above has refused a tool's and a wake's.
        let calling = || {
            sub["inCall"] == true
                && matches!(super::ceremony::target(), "ios" | "android")
                && origin == CallOrigin::App
                && self.in_call()
        };
        if applies
            && sub["foreground"] == true
            && (origin == CallOrigin::Action || (self.page.in_background() && !calling()))
            && !(origin == CallOrigin::Action && self.press_starts_audio(row, args))
        {
            return Err("not in the foreground".into());
        }
        Ok(())
    }

    /// Whether `row` called with `args` is one of the two names a live press
    /// of a recording control starts while hidden (LLP 1126.004 D4, §6d):
    /// `getUserMedia` for audio and no video, and `startMediaRecorder`.
    /// Which tracks the recorder's stream holds, and the press's own Live
    /// Activity, are piece 4's checks.
    pub(super) fn press_starts_audio(&self, row: &ModulesRow, args: &Json) -> bool {
        let audio_only = match (self.plan.str(row.package), self.plan.str(row.declared)) {
            ("@exact/media-capture", "getUserMedia") => {
                (args["audio"] == true || args["audioConstraints"].is_object())
                    && args["video"] != true
                    && args["videoConstraints"].is_null()
            }
            ("@exact/recording", "startMediaRecorder") => true,
            _ => false,
        };
        audio_only && self.press_records(self.invocation_id())
    }

    /// The implementation's answer for `row` under the agent, the agent's
    /// preferred value laid over it field by field (LLP 1133.009 D10 item
    /// 11); any other answer as it is.
    pub(in crate::runner) fn overlaid(&self, row: &ModulesRow, mut answer: Json) -> Json {
        // An implementation that lays it itself is told it (LLP 1133.016 §9).
        let laid = self.substitute(row)["overlays"] == "implementation";
        if self.route(row) != Route::Agent || laid || error_code(&answer).is_some() {
            return answer;
        }
        if let Some(preferred) = self.extensions.preferred.get(&self.key(row)) {
            if error_code(preferred).is_some() {
                return preferred.clone();
            }
            overlay(&mut answer, preferred);
        }
        answer
    }
}

fn admit_field(
    record: &Json,
    field: &str,
    at: &str,
    admit: &impl Fn(&str, &str) -> Result<(), String>,
) -> Result<(), String> {
    if let Some((list, tail)) = field.split_once("[].") {
        if let Some(parts) = record[list].as_array() {
            for (i, part) in parts.iter().enumerate() {
                admit_field(part, tail, &format!("{at}.{list}[{i}]"), admit)?;
            }
        }
    } else if let Some(value) = record[field].as_str() {
        admit(&format!("{at}.{field}"), value)?;
    }
    Ok(())
}

/// Why a record's URL at `at` is refused under `grants`, naming it without
/// its userinfo, query or fragment: it has a user name or password, which a
/// browser's `Request` refuses too
/// (<https://fetch.spec.whatwg.org/#dom-request>), or no `net.fetch` line
/// admits it, read through `link` (none does without I/O).
fn fetch_refusal(
    link: crate::grants::GrantsLink,
    grants: &str,
    at: &str,
    given: &str,
) -> Option<String> {
    let (shown, userinfo) = crate::grants::redacted(given);
    if userinfo {
        return Some(format!(
            "userinfo ({at}={shown} has a user name or password)"
        ));
    }
    let web = given.starts_with("https://") || given.starts_with("http://");
    let ok = web
        && link
            .and_then(|link| (link.admitting)(grants, None, given))
            .is_some();
    (!ok).then(|| format!("grant-missing ({at}={shown} needs net.fetch)"))
}

/// `over`'s fields laid over `value`'s, record by record.
pub(in crate::runner) fn overlay(value: &mut Json, over: &Json) {
    match (value, over) {
        (Json::Object(at), Json::Object(over)) => {
            for (k, v) in over {
                match at.get_mut(k) {
                    Some(existing) if existing.is_object() && v.is_object() => overlay(existing, v),
                    _ => {
                        at.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        (value, over) => *value = over.clone(),
    }
}

#[cfg(test)]
mod tests {
    /// [`super::fetch_refusal`] in an archive that links I/O.
    fn fetch_refusal(grants: &str, at: &str, given: &str) -> Option<String> {
        super::fetch_refusal(Some(&crate::grants::LINKED), grants, at, given)
    }

    #[test]
    fn nested_grants_check_each_file_in_order() {
        let request = serde_json::json!({"form": [{"name":"x","value":"private"}, {"from":"app:/data/ok"}, {"from":"app:/cache/no"}]});
        let result = super::admit_field(&request, "form[].from", "requests[1]", &|at, path| {
            if path == "app:/data/ok" {
                Ok(())
            } else {
                Err(at.to_owned())
            }
        });
        assert_eq!(result, Err("requests[1].form[2].from".into()));
    }

    #[test]
    fn a_record_url_with_a_password_is_refused_without_showing_it() {
        let grants = "net.fetch https://example.com";
        let at = "requests[0].url";
        assert_eq!(
            fetch_refusal(grants, at, "https://example.com/f?token=t"),
            None
        );
        for url in [
            "https://alice:secret@example.com/f",
            "http://alice:secret@other.example/f?token=t",
            "https:alice:secret@example.com/f",
        ] {
            let why = fetch_refusal(grants, at, url).unwrap_or_default();
            assert!(
                why.starts_with("userinfo (") && !why.contains("alice") && !why.contains("secret"),
                "{why}"
            );
        }
        assert_eq!(
            fetch_refusal(grants, at, "https://other.example/f?token=t#x").as_deref(),
            Some("grant-missing (requests[0].url=https://other.example/f needs net.fetch)")
        );
        // Without I/O nothing is admitted, as under grants that name nothing.
        assert_eq!(
            super::fetch_refusal(None, grants, at, "https://example.com/f").as_deref(),
            Some("grant-missing (requests[0].url=https://example.com/f needs net.fetch)")
        );
    }
}
