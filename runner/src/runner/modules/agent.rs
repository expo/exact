//! The agent's side of the module seam (LLP 1133.002 §6, LLP 1124 D7):
//! `state.extensions`, and `prefer extension` and `prefer permission`.

use super::super::{CommitReceipt, DataSource, Runner};
use super::values::fill_with;
use super::{error_code, Json, ModulesRow, Route, Waits};
use serde_json::{json, Map};

impl<D: DataSource> Runner<D> {
    /// `state.extensions` (LLP 1133.002 §6, Disclosure): each module the plan
    /// names, by its extension: whether a host's implementation answers it
    /// (`bound`, and `agent` where it answers under the agent), its
    /// permission's state, the preferred values, each source's readers (a
    /// resource that reads it, or whose data code did), the tickets held
    /// with their calls' origins, and its ceremonies and signals, each by
    /// name, RP ID, origin and outcome, never a response (LLP 1133.011 §9).
    /// `None` for a plan that names none.
    pub(super) fn extensions_json_linked(&self) -> Option<String> {
        if self.plan.modules.is_empty() && self.plan.views.is_empty() {
            return None;
        }
        let mut out = Map::new();
        for row in &self.plan.modules {
            let extension = self.plan.str(row.extension).to_owned();
            let key = self.key(row);
            let declared = key.split_once('.').unwrap().1.to_owned();
            let bound = self.extensions.bound.contains(&extension);
            let entry = out.entry(extension.clone()).or_insert_with(
                || json!({ "bound": bound, "preferred": {}, "readers": {}, "held": [] }),
            );
            if self.route(row) == Route::Agent {
                entry["agent"] = json!(true);
            }
            self.devices_json(&extension, entry);
            // Each permission by name; `permission` too where it has one.
            let sub = self.substitute(row);
            if sub["owner"].is_string() {
                entry["names"][&declared] = json!({ "owner": sub["owner"], "route": match self.route(row) { Route::Real => "real", Route::Agent => "agent", Route::Substitute => "substitute" } });
            }
            let by = sub["permissionBy"].as_object().into_iter().flatten();
            let list = sub["permissionList"].as_array().into_iter().flatten();
            for p in std::iter::once(&sub["permission"])
                .chain(by.map(|(_, p)| p))
                .chain(list)
            {
                // A `<prefix>.*` permission is shown as each one set under it.
                match p["name"].as_str().map(|n| (n, n.strip_suffix('*'))) {
                    Some((_, Some(pre))) => {
                        for name in self
                            .extensions
                            .permission
                            .keys()
                            .filter(|k| k.starts_with(pre))
                        {
                            entry["permissions"][name] = json!(self.permission_state(name));
                        }
                    }
                    Some((name, None)) => {
                        entry["permissions"][name] = json!(self.permission_state(name));
                    }
                    None => {}
                }
            }
            if let Some(v) = self.extensions.preferred.get(&key) {
                entry["preferred"][&declared] = v.clone();
            }
            if !row.send {
                let readers: Vec<&str> = (0..self.resources.len())
                    .filter(|&i| {
                        self.resource_row(i).source == self.plan.sources[row.source.0 as usize].name
                            || self.resources[i].watching.contains(&key)
                    })
                    .map(|i| self.plan.str(self.resource_row(i).name))
                    .collect();
                entry["readers"][&declared] = json!(readers);
            }
            if let Some(facts) = self.facts_of(&extension) {
                entry["facts"] = facts;
            }
            let ceremonies = self.ceremonies_of(&extension);
            if !ceremonies.is_empty() {
                entry["ceremonies"] = json!(ceremonies);
            }
            for h in self.extensions.held.iter().filter(|h| {
                h.source == self.plan.str(self.plan.sources[row.source.0 as usize].name)
                    && !h.queued
            }) {
                if let Some(held) = entry["held"]
                    .as_array_mut()
                    .filter(|_| h.waits != Waits::Span)
                {
                    held.push(json!({ "ticket": h.hold, "name": declared, "origin": h.cause.origin().word(), "waits": h.waits.word() }));
                }
            }
        }
        for (extension, entry) in out.iter_mut() {
            // Its streams, sampled readers and canvas inputs (LLP 1133.013 §9).
            if let Some(samples) = self.samples_json(extension) {
                for (k, v) in samples.as_object().into_iter().flatten() {
                    entry[k] = v.clone();
                }
            }
            if let Some(pictures) = self.pictures_json(extension) {
                entry["pictures"] = pictures;
            }
            for (state, calls) in self.played_json(extension).into_iter().flatten() {
                entry[&state] = calls;
            }
            if let Some(claims) = self.services_json(extension) {
                entry["foregroundService"] = claims;
            }
            // The canvases its inputs feed, and their leases (D6).
            if let Some(surfaces) = self.fed_json(extension) {
                entry["surfaces"] = surfaces;
            }
            self.reach_json(extension, entry);
            self.arrival_json(extension, entry);
            let one = (entry["permissions"].as_object())
                .filter(|p| p.len() == 1)
                .and_then(|p| p.values().next().cloned());
            if let Some(state) = one {
                entry["permission"] = state;
            }
        }
        // Each module event not yet heard (LLP 1133.004 D6).
        for (extension, event) in self.queued_events() {
            let entry = out.entry(extension).or_insert_with(
                || json!({ "bound": false, "preferred": {}, "readers": {}, "held": [] }),
            );
            match entry["queued"].as_array_mut() {
                Some(queued) => queued.push(event),
                None => entry["queued"] = json!([event]),
            }
        }
        // What each bound implementation disclosed of its own (D8 item 9).
        for extension in self.disclosed_extensions() {
            let bound = self.extensions.bound.contains(&extension);
            let entry = out.entry(extension.clone()).or_insert_with(
                || json!({ "bound": bound, "preferred": {}, "readers": {}, "held": [] }),
            );
            entry["implementation"] = self.disclosed_json(&extension).unwrap_or_default();
        }
        // Each package view, and who made it displayed (LLP 1133.008 D8 item 2).
        for row in &self.plan.views {
            let extension = self.plan.str(row.extension);
            let views = self.views_json(extension);
            let bound = self.extensions.bound.contains(extension);
            let entry = (out.entry(extension.to_owned())).or_insert_with(
                || json!({ "bound": bound, "preferred": {}, "readers": {}, "held": [] }),
            );
            entry["views"] = json!(views);
            entry["preferred"] = json!(self
                .extensions
                .preferred
                .iter()
                .filter_map(|(key, value)| {
                    Some((
                        key.strip_prefix(&format!("{extension}."))?.to_owned(),
                        value.clone(),
                    ))
                })
                .collect::<Map<String, Json>>());
            if self.extensions.agent.contains(extension) && !bound {
                entry["agent"] = json!(true);
            }
        }
        Some(Json::Object(out).to_string())
    }

    /// The agent's `prefer extension` and `prefer permission` (LLP 1124 D7,
    /// LLP 1069.007 D2): `{"op":"prefer","extension":{"geolocation.watchPosition":
    /// {…}|"clear"}}` sets a source's answer; `{"op":"prefer","permission":
    /// {"geolocation":"granted"}}` a permission's state. Readers are asked
    /// again. `None` for any other request.
    pub(super) fn extension_agent_linked(
        &mut self,
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
            for (name, value) in group {
                // A state, or a fact of one the platform hides (`keyed.rs`);
                // a name may stand for each permission under it (`health`).
                let Some(value) = value.as_str() else {
                    return refuse(format!(
                        "prefer permission.{name}: granted, denied or prompt"
                    ));
                };
                if let Err(e) = self.prefer_permission(name, value) {
                    return refuse(e);
                }
            }
            if v.get("extension").is_none() {
                let (receipts, error) = self.apply_announced();
                if let Some(e) = error {
                    return refuse(format!("prefer permission: {e:?}"));
                }
                let reply = json!({ "permission": self.extensions.permission });
                return Some((receipts, Ok(reply.to_string())));
            }
        }
        let Some(group) = v.get("extension")?.as_object() else {
            return refuse("prefer extension: an object of `extension.source`".into());
        };
        let literal = v["literal"] == true;
        if let Err(why) = self.check_substitutes(group, literal, false) {
            return refuse(why);
        }
        let mut told = std::collections::BTreeSet::new();
        let mut moved = Vec::new();
        for (key, value) in group {
            moved.push((key, self.extensions.preferred.get(key).cloned()));
            // A list whose changes are the device's events (`spans.rs`).
            let listed = (self.plan.modules.iter())
                .find(|m| self.key(m) == *key)
                .cloned();
            let before = listed.as_ref().and_then(|r| self.changes_value(r));
            if !literal && value == "clear" {
                self.extensions.preferred.remove(key);
            } else {
                self.extensions.preferred.insert(key.clone(), value.clone());
            }
            if let Some(row) = listed {
                let after = self.changes_value(&row);
                if before != after {
                    self.preferred_changed(&row, before, after);
                }
            }
            self.note_track(
                key,
                if !literal && value == "clear" {
                    &Json::Null
                } else {
                    value
                },
            );
            self.devices_preferred(key);
            self.log(format!("prefer extension {key}"));
            self.announce(key);
            for dependent in self.network_dependents(key) {
                self.announce(&dependent);
            }
            for status in self.app_icon_statuses(key) {
                self.announce(&status);
            }
            // An agent word is its extension's: any of its names stands for it.
            let extension = key.split_once('.').map_or("", |(e, _)| e);
            let row = (self.plan.modules.iter()).find(|m| {
                self.key(m) == *key
                    || (self.plan.str(m.extension) == extension && self.agent_word(key).is_some())
            });
            if let Some(row) = row.filter(|r| self.route(r) == Route::Agent) {
                told.insert(self.plan.str(row.extension).to_owned());
            } else if self.agent_word(key).is_some()
                && self.extensions.agent.contains(extension)
                && !self.extensions.bound.contains(extension)
                && self
                    .plan
                    .views
                    .iter()
                    .any(|v| self.plan.str(v.extension) == extension)
            {
                told.insert(extension.to_owned());
            }
        }
        for extension in told {
            self.tell_preferred(&extension);
        }
        // A field word's change is an arrival, once every word is set.
        for (key, before) in moved {
            let after = self.extensions.preferred.get(key).cloned();
            self.arrival_field(key, before, after);
        }
        self.settle_values();
        let (receipts, error) = self.apply_announced();
        if let Some(e) = error {
            return refuse(format!("prefer extension: {e:?}"));
        }
        let reply = json!({ "extension": self.extensions.preferred, "permission": self.extensions.permission });
        Some((receipts, Ok(reply.to_string())))
    }

    pub(super) fn check_substitutes(
        &self,
        group: &Map<String, Json>,
        literal: bool,
        restoring: bool,
    ) -> Result<(), String> {
        for (key, value) in group {
            let clear = !literal && value == "clear";
            if let Some(ty) = self.agent_word(key) {
                if !clear && !word_fits(&ty, value) {
                    return Err(format!("prefer extension: `{key}` is a {ty}"));
                }
                if !clear && !restoring {
                    if let Some(e) = self.arrival_refusal(key, value, group) {
                        return Err(e);
                    }
                }
                continue;
            }
            let Some(row) = self
                .plan
                .modules
                .iter()
                .find(|m| self.key(m) == *key)
                .cloned()
            else {
                let known: Vec<String> = (self.plan.modules.iter())
                    .filter(|m| !m.send)
                    .map(|m| self.key(m))
                    .collect();
                return Err(format!(
                    "prefer extension: no source `{key}` in this app; its module sources are {}",
                    if known.is_empty() {
                        "none".into()
                    } else {
                        known.join(", ")
                    }
                ));
            };
            let sub = self.substitute(&row);
            // Interactive drives also accept the numeric shorthand of a declared, ticketed source.
            if !literal
                && !row.send
                && value.is_number()
                && sub["short"].is_array()
                && sub["partial"] != true
                && sub["implementation"] != true
                && sub["track"] != true
                && sub["network"] != "browse"
            {
                self.typed(&row, &value.to_string())
                    .map_err(|e| format!("prefer extension: {e}"))?;
            } else {
                check_source_substitute(
                    &self.plan,
                    &row,
                    key,
                    &sub,
                    (!clear).then_some(value),
                    self.host_target(),
                    |name| Some(self.permission_state(name)),
                )
                .map_err(|e| format!("prefer extension: {e}"))?;
            }
        }
        Ok(())
    }

    /// The type of `key` (`speech-recognition.heard`) where it is one of the
    /// words its interface declares for the agent path alone (`agent`, LLP
    /// 1133.012 §9) and is no name of the interface.
    fn agent_word(&self, key: &str) -> Option<String> {
        let (extension, word) = key.split_once('.')?;
        if (self.plan.modules.iter()).any(|m| self.key(m) == key) {
            return None;
        }
        let words = if let Some(row) = self
            .plan
            .modules
            .iter()
            .find(|m| self.plan.str(m.extension) == extension)
        {
            self.substitute(row)["agent"].clone()
        } else {
            let view = self
                .plan
                .views
                .iter()
                .find(|v| self.plan.str(v.extension) == extension)?;
            serde_json::from_str(self.plan.str(view.agent)).ok()?
        };
        words[word].as_str().map(str::to_owned)
    }
}

/// Whether `value` is one of `ty`'s: a string, number or bool by its kind,
/// a list of them, and anything else as JSON.
fn word_fits(ty: &str, value: &Json) -> bool {
    match ty.trim() {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "bool" => value.is_boolean(),
        list if list.ends_with("[]") => (value.as_array())
            .is_some_and(|a| a.iter().all(|v| word_fits(&list[..list.len() - 2], v))),
        _ => true,
    }
}

fn check_shape(
    plan: &exact_plan::Plan,
    ty: exact_plan::TypesId,
    at: &str,
    value: &Json,
    partial: bool,
) -> Result<(), String> {
    let row = plan.type_(ty);
    if partial {
        if let (Some(map), exact_plan::TypeKind::Record) = (value.as_object(), row.kind) {
            for (name, v) in map {
                let field = row
                    .fields
                    .iter()
                    .map(|f| plan.field(f))
                    .find(|f| plan.str(f.name) == name)
                    .ok_or_else(|| format!("{at}: field `{name}` is not in the shape"))?;
                check_shape(plan, field.ty, &format!("{at}.{name}"), v, true)?;
            }
            return Ok(());
        }
        if row.kind == exact_plan::TypeKind::Option && !value.is_null() {
            return check_shape(plan, row.elem.unwrap(), at, value, true);
        }
    }
    let shape = exact_js_value::Shape::from_plan(plan, ty)?;
    let mut full = value.clone();
    super::absent_is_none(&mut full, &shape);
    let v = exact_js_value::from_json(&full, &shape).map_err(|why| format!("{at}: {why}"))?;
    if v.conforms(plan, ty) {
        Ok(())
    } else {
        Err(format!("{at}: {}", crate::conform::mismatch(plan, &v, ty)))
    }
}

impl<D: DataSource> Runner<D> {
    /// `row`'s declared `fill`, where `value` leaves a path out. A filler
    /// `"now"` is the agent clock's date; `{"table", "by"}` is the entry of
    /// the declaration's `tables.<table>` that the filled object's field
    /// `by` names, filled after the plain fillers so `by` can be one. A
    /// table filler with `"on"`, a list of targets, fills only there: a
    /// platform's record (a gamepad's `apple`) on that platform's hosts.
    pub(super) fn fill_all(&self, row: &ModulesRow, value: &mut Json) {
        let sub = self.substitute(row);
        let now = json!(self.time.epoch_at_zero + self.now_ms);
        fill_declared(&sub, self.host_target(), &now, value);
    }
}

fn fill_declared(sub: &Json, target: &str, now: &Json, value: &mut Json) {
    let here = |f: &Json| {
        f["on"]
            .as_array()
            .is_none_or(|on| on.iter().any(|t| t == target))
    };
    let fills = sub["fill"].as_object().cloned().unwrap_or_default();
    for tabled in [false, true] {
        for (path, filler) in fills
            .iter()
            .filter(|(_, f)| f["table"].is_string() == tabled && here(f))
        {
            let made = |object: &Map<String, Json>| match filler {
                f if f == "now" => Some(now.clone()),
                f if tabled => {
                    let by = object.get(f["by"].as_str()?)?.as_str()?;
                    sub["tables"][f["table"].as_str()?].get(by).cloned()
                }
                f => Some(f.clone()),
            };
            fill_with(value, path, &made);
        }
    }
}

/// Check the JSON a test or drive supplies for one declared module name.
/// A source may take a partial answer, a sampled track or a network fixture.
pub fn check_source_substitute(
    plan: &exact_plan::Plan,
    row: &ModulesRow,
    key: &str,
    sub: &Json,
    value: Option<&Json>,
    target: &str,
    permission_state: impl Fn(&str) -> Option<String>,
) -> Result<(), String> {
    if row.send && sub["from"] != plan.str(row.declared) {
        return Err(format!("`{key}` is a send, which the module answers"));
    }
    let Some(value) = value else { return Ok(()) };
    let ty = plan.sources[row.source.0 as usize].ty;
    let mut checked = value.clone();
    for permission in super::permits::statuses(sub, plan.str(row.declared)) {
        if permission["targets"]
            .as_array()
            .is_some_and(|on| !on.iter().any(|t| t == target))
        {
            continue;
        }
        let (Some(field), Some(name)) = (permission["field"].as_str(), permission["name"].as_str())
        else {
            continue;
        };
        if field
            .split('.')
            .try_fold(value, |v, part| v.get(part))
            .is_some()
        {
            return Err(format!(
                "`{key}`'s `{field}` is set by `prefer permission.{name}`"
            ));
        }
        // During compilation any member of the field's enum stands for the host's state.
        let state = super::permits::fixed_state(sub, target, field)
            .map(str::to_owned)
            .or_else(|| permission_state(name))
            .or_else(|| {
                let field_ty = field.split('.').try_fold(ty, |ty, part| {
                    let mut record = plan.type_(ty);
                    if record.kind == exact_plan::TypeKind::Option {
                        record = plan.type_(record.elem?);
                    }
                    record
                        .fields
                        .iter()
                        .map(|id| plan.field(id))
                        .find(|f| plan.str(f.name) == part)
                        .map(|f| f.ty)
                })?;
                let case = plan.type_(field_ty).fields.iter().next()?;
                Some(plan.str(plan.field(case).name).to_owned())
            })
            .unwrap_or_else(|| "prompt".into());
        super::set_path(&mut checked, field, state.into());
    }
    if error_code(value).is_some() {
        return Ok(());
    }
    if sub["network"] == "browse" {
        return super::network::network_checked(value);
    }
    let check = |v: &Json| check_shape(plan, ty, key, v, false);
    if sub["track"] == true {
        return super::samples::track_checked(
            sub,
            key,
            |reading| {
                let mut full = reading.clone();
                fill_declared(sub, target, &json!(0), &mut full);
                check(&full)
            },
            &checked,
        );
    }
    if sub["implementation"] == true || sub["partial"] == true {
        return check_shape(plan, ty, key, &checked, true);
    }
    fill_declared(sub, target, &json!(0), &mut checked);
    check(&checked)
}
