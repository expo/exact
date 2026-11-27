//! The packages this binary composes (`crate::package`), and the native
//! slot's one handler that reaches them (LLP 1133.022 D10 item 5). The
//! handler queues every body with its reply, from any thread, and wakes the
//! loop; the loop's turn (`modules_in`) hands a body that names a composed
//! package's extension to that package's instance on the loop's thread, a
//! shell's (Android's reader's Kotlin) to the shell, and answers the rest
//! `NotSupportedError`. So a shell and the packages never replace each
//! other's handler.

use crate::package::{Context, Entry, Json, Package, Reply, Slot};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::{Arc, Mutex};

/// A call whose `admit` status probe is in flight or queued for the next turn.
struct PendingAdmit {
    extension: String,
    name: String,
    origin: String,
    request: Option<u64>,
    args: Json,
    preferred: Option<Json>,
    reply: Reply,
}

#[derive(Default)]
struct Admitting {
    next: u64,
    pending: BTreeMap<u64, PendingAdmit>,
    ready: Vec<u64>,
}

/// The composed packages and their instances in this session.
#[derive(Default)]
pub(in crate::presenter) struct Packages {
    table: Vec<Entry>,
    development: bool,
    made: Vec<(&'static str, Box<dyn Package>, Context)>,
    /// Extensions told they run under the agent (`--bind` names the others).
    agent: Vec<&'static str>,
    now: Rc<Cell<f64>>,
    lines: Rc<RefCell<Vec<String>>>,
    /// The runner's native slot, which every instance announces into.
    native: Slot,
    publisher: std::sync::Weak<crate::image::Publisher>,
    pictures: Arc<Mutex<Vec<String>>>,
    admitting: Arc<Mutex<Admitting>>,
}

/// The extension a body names.
pub(super) fn extension_of(body: &[u8]) -> Option<String> {
    let v: Json = serde_json::from_slice(body).ok()?;
    Some(v.get("extension")?.as_str()?.to_owned())
}

/// Whether a module call is a tell: one that names no `source` or `send`,
/// so no request in flight stands for it.
pub(super) fn told(body: &[u8]) -> bool {
    serde_json::from_slice::<Json>(body).is_ok_and(|v| {
        v.get("extension").is_some() && v.get("source").is_none() && v.get("send").is_none()
    })
}

impl Packages {
    /// The table the entry handed over; which of them run told they are
    /// under the agent: under the agent, all but those `EXACT_AGENT_BIND`
    /// names (`--bind`).
    pub(super) fn set(&mut self, table: Vec<Entry>, agent: bool, development: bool) {
        self.development = development;
        let bound = std::env::var("EXACT_AGENT_BIND").unwrap_or_default();
        self.agent = (table.iter())
            .map(|e| e.extension)
            .filter(|e| agent && !bound.split(',').any(|b| b == *e))
            .collect();
        self.table = table;
    }

    /// The extensions bound as in production, and those bound under the
    /// agent, for `Runner::bind_extensions`.
    pub(super) fn bound(&self) -> (Vec<&'static str>, Vec<&'static str>) {
        (self.table.iter().map(|e| e.extension)).partition(|e| !self.agent.contains(e))
    }

    /// The instances announce into `native` from now on.
    pub(super) fn slot(&self, native: exact_runner::Native) {
        *self.native.lock().unwrap_or_else(|e| e.into_inner()) = native;
    }

    /// The instances publish written images to `publisher` from now on.
    pub(super) fn publish_to(&mut self, publisher: &std::sync::Arc<crate::image::Publisher>) {
        self.publisher = std::sync::Arc::downgrade(publisher);
        for (_, _, context) in &self.made {
            context.0.notifier.borrow_mut().publisher = self.publisher.clone();
        }
    }

    pub(super) fn is_empty(&self) -> bool {
        self.table.is_empty()
    }

    pub(super) fn serves(&self, extension: &str) -> bool {
        self.table.iter().any(|e| e.extension == extension)
    }

    /// The journal lines the instances wrote since the last call.
    pub(super) fn take_lines(&self) -> Vec<String> {
        std::mem::take(&mut *self.lines.borrow_mut())
    }

    pub(super) fn take_pictures(&self) -> Vec<String> {
        std::mem::take(&mut *self.pictures.lock().unwrap())
    }

    /// Admitted calls whose status probe succeeded and are ready for this turn.
    pub(super) fn take_ready(&self) -> Vec<u64> {
        std::mem::take(
            &mut self
                .admitting
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .ready,
        )
    }

    /// Dispatch admitted calls snapshotted at the start of this turn, skipping
    /// any cancelled before reaching this point.
    pub(super) fn dispatch_admitted(
        &mut self,
        ready: Vec<u64>,
        now: f64,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) {
        self.now.set(now);
        for ticket in ready {
            let entry = self
                .admitting
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .pending
                .remove(&ticket);
            let Some(entry) = entry else {
                continue;
            };
            let Some(at) = self.instance(&entry.extension, wake) else {
                entry.reply.error("NotSupportedError", None);
                continue;
            };
            let (_, package, context) = &mut self.made[at];
            *context.0.origin.borrow_mut() = entry.origin;
            context.0.request.set(entry.request);
            package.later(
                &entry.name,
                &entry.args,
                entry.preferred.as_ref(),
                entry.reply,
            );
        }
    }

    /// Body `body` of a composed package's extension, at `now` on the
    /// session's clock: its instance (made now if this is the session's
    /// first call) is called. A call that the runner asks to admit first
    /// (`admit`: a tool's or a wake's call that would ask) reads the
    /// instance's own status, and runs on the next turn only if status proves
    /// a non-prompting state and the request was not cancelled meanwhile.
    pub(super) fn dispatch(
        &mut self,
        body: &[u8],
        reply: exact_runner::Reply,
        now: f64,
        wake: &Arc<dyn Fn() + Send + Sync>,
    ) {
        self.now.set(now);
        let Ok(request) = serde_json::from_slice::<Json>(body) else {
            return Reply::new(reply).error("NotSupportedError", Some("not a module call"));
        };
        let extension = request["extension"].as_str().unwrap_or_default();
        let Some(at) = self.instance(extension, wake) else {
            return Reply::new(reply).error("NotSupportedError", None);
        };
        let (_, package, context) = &mut self.made[at];
        let reply = Reply::new(reply);
        let args = request.get("args");
        let named = request.get("source").or(request.get("send"));
        if let Some(name) = request["release"].as_str() {
            package.release(name, args);
            reply.send(Json::Object(Default::default()));
        } else if let (Some(values), None) = (request.get("preferred"), named) {
            package.preferred(values, &request["permissions"]);
            reply.send(Json::Object(Default::default()));
        } else if let Some(ids) = request["dispose"].as_array() {
            let ids: Vec<String> = (ids.iter())
                .filter_map(|i| i.as_str().map(str::to_owned))
                .collect();
            package.dispose(&ids);
            reply.send(Json::Object(Default::default()));
        } else if let Some(n) = request["cancel"].as_u64() {
            if !cancel_admitting(&self.admitting, extension, n) {
                package.cancel(n);
            }
            reply.send(Json::Object(Default::default()));
        } else if let Some(shown) = request["foreground"].as_bool() {
            context.0.foreground.set(shown);
            package.foreground(shown);
            reply.send(Json::Object(Default::default()));
        } else if let Some(name) = named.and_then(Json::as_str) {
            // The runner names every call's origin; one that names none is no
            // one's (LLP 1133.023 D7a).
            let origin = request["origin"].as_str().unwrap_or("none");
            *context.0.origin.borrow_mut() = origin.to_owned();
            let req_id = request["request"].as_u64();
            let args = args.cloned().unwrap_or(Json::Object(Default::default()));
            match admission(&request) {
                Some((status, field, denied)) => {
                    // The instance's own status first (LLP 1133.002 D12):
                    // only a person may answer its prompt.
                    context.0.request.set(None);
                    let ticket = {
                        let mut state = self.admitting.lock().unwrap_or_else(|e| e.into_inner());
                        let ticket = state.next;
                        state.next += 1;
                        state.pending.insert(
                            ticket,
                            PendingAdmit {
                                extension: extension.to_owned(),
                                name: name.to_owned(),
                                origin: origin.to_owned(),
                                request: req_id,
                                args,
                                preferred: request.get("preferred").cloned(),
                                reply,
                            },
                        );
                        ticket
                    };
                    let (admitting, wake) = (self.admitting.clone(), wake.clone());
                    let asked = exact_runner::Reply::new(move |outcome| {
                        let mut state = admitting.lock().unwrap_or_else(|e| e.into_inner());
                        if !state.pending.contains_key(&ticket) {
                            return;
                        }
                        if !admitted(&outcome, &field) {
                            let entry = state.pending.remove(&ticket);
                            drop(state);
                            if let Some(entry) = entry {
                                entry.reply.error(&denied, Some("person-required"));
                            }
                            return;
                        }
                        state.ready.push(ticket);
                        drop(state);
                        wake();
                    });
                    let empty = Json::Object(Default::default());
                    package.later(&status, &empty, None, Reply::new(asked));
                }
                None => {
                    context.0.request.set(req_id);
                    package.later(name, &args, request.get("preferred"), reply);
                }
            }
        } else {
            // A view's or a surface's report, a wake's outcome: nothing
            // these hosts' packages serve yet.
            reply.send(Json::Object(Default::default()));
        }
    }

    /// The index of `extension`'s instance in this session, made now if
    /// none is.
    fn instance(
        &mut self,
        extension: &str,
        wake: &std::sync::Arc<dyn Fn() + Send + Sync>,
    ) -> Option<usize> {
        if let Some(at) = self.made.iter().position(|(e, ..)| *e == extension) {
            return Some(at);
        }
        let entry = *self.table.iter().find(|e| e.extension == extension)?;
        #[cfg(windows)]
        crate::com::ensure_sta();
        let (pictures, wake) = (self.pictures.clone(), wake.clone());
        let context = Context::new(
            entry.extension,
            self.agent.contains(&entry.extension),
            self.development,
            self.native.clone(),
            self.now.clone(),
            self.lines.clone(),
            std::sync::Arc::new(move |source| {
                let mut pending = pictures.lock().unwrap();
                if !pending.iter().any(|s| s == source) {
                    pending.push(source.to_owned());
                }
                drop(pending);
                wake();
            }),
        );
        context.0.notifier.borrow_mut().publisher = self.publisher.clone();
        let package = entry.make(context.clone());
        self.made.push((entry.extension, package, context));
        Some(self.made.len() - 1)
    }

    /// The session ends: each instance's `destroy`, in the order they were
    /// made, and none called again.
    pub(super) fn destroy(&mut self) {
        let pending = {
            let mut state = self.admitting.lock().unwrap_or_else(|e| e.into_inner());
            state.ready.clear();
            std::mem::take(&mut state.pending)
        };
        for (_, entry) in pending {
            entry.reply.error("AbortError", None);
        }
        for (_, mut package, context) in std::mem::take(&mut self.made) {
            package.destroy();
            context.end();
        }
    }
}

/// Cancel pending admission entries matching `(extension, request)`, replying
/// `AbortError`. Returns whether any pending admission matched.
fn cancel_admitting(admitting: &Arc<Mutex<Admitting>>, extension: &str, request: u64) -> bool {
    let cancelled: Vec<PendingAdmit> = {
        let mut state = admitting.lock().unwrap_or_else(|e| e.into_inner());
        let tickets: Vec<u64> = state
            .pending
            .iter()
            .filter(|(_, p)| p.extension == extension && p.request == Some(request))
            .map(|(t, _)| *t)
            .collect();
        tickets
            .into_iter()
            .filter_map(|t| state.pending.remove(&t))
            .collect()
    };
    let found = !cancelled.is_empty();
    for entry in cancelled {
        entry.reply.error("AbortError", None);
    }
    found
}

/// Whether a status probe proved permission is in a non-prompting state.
fn admitted(outcome: &exact_runner::Outcome, field: &str) -> bool {
    let exact_runner::Outcome::Response(r) = outcome else {
        return false;
    };
    if !(200..300).contains(&r.status) {
        return false;
    }
    let Ok(body) = serde_json::from_slice::<Json>(&r.body) else {
        return false;
    };
    if body.get("error").is_some() {
        return false;
    }
    matches!(
        body.get(field).and_then(Json::as_str),
        Some("granted" | "limited" | "denied")
    )
}

/// What a call's `admit` asks: the status source's name, the field that
/// reads `prompt`, and the error to fail with.
fn admission(request: &Json) -> Option<(String, String, String)> {
    let admit = request.get("admit")?;
    Some((
        admit["status"].as_str()?.to_owned(),
        admit["field"].as_str()?.to_owned(),
        admit["denied"]
            .as_str()
            .unwrap_or("NotAllowedError")
            .to_owned(),
    ))
}
