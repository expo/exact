//! Call provenance (LLP 1133.002 D12, LLP 1133.023 D7, LLP 1133.035 D16):
//! who each commit, request, armed `then` and queued send comes from, and
//! the invocation it is part of. Every commit has an origin, so this is the
//! core's; a module's admission (`modules/admit.rs`) is one of its readers.
//!
//! **Origin** (D12). Each commit has one: a person's input or the app's own
//! code (`app`), a tool invocation, a wake. Each request carries its
//! commit's, an answer's commit carries its request's, and a re-ask the
//! device causes carries each reader's own last ask's. A data-code call
//! cannot be traced to its answer here, so it is admitted under the least
//! privileged origin in flight. A permission ticket is never raised for a
//! tool or a wake: the call fails `person-required` instead.
//!
//! **Invocations** (LLP 1133.023 D7). A wake's event and a tool call each
//! run as an invocation with an id, which its dispatch commit stamps on
//! every request, armed `then`, held call and queued send it makes; a
//! commit that answers a stamped request, or runs a stamped `then` or
//! queued send, is stamped too. A resource's re-ask by the device and a
//! model's handler (`modules/invoke.rs`) carry none. What an invocation
//! owns, and what lets go of it, is read by id alone. The tables are part
//! of a commit's checkpoint, and an invocation is superseded only when the
//! commit that let go of its work stands (`let_go`).

use super::{CommitReceipt, DataSource, Event, Runner, RunnerError, Target};
use exact_plan::Value;
use std::collections::BTreeMap;

/// Who a call comes from (LLP 1133.002 D12), least privileged first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum CallOrigin {
    /// The device: an event a package emitted (LLP 1133.004 D6).
    Device,
    /// A background wake (LLP 1133.017 D5).
    Wake,
    /// A tool invocation (LLP 1111).
    Tool,
    /// A notification's background action the person chose (LLP 1133.004
    /// stage 2, as amended 2026-11-12): the person's act for that one
    /// declared action, run with no view. It may call a name held to the
    /// app (`"origin": "app"`) that shows nothing; it opens no UI, so a
    /// prompt, a presence check, a name only the foreground may call and
    /// the host commands a wake may not run are refused, as for a wake.
    Action,
    /// A person's input, or the app's own code in a foreground session.
    #[default]
    App,
}

impl CallOrigin {
    /// Its word in the journal, a ticket and `state.extensions`.
    pub fn word(self) -> &'static str {
        match self {
            CallOrigin::Device => "device",
            CallOrigin::Wake => "wake",
            CallOrigin::Tool => "tool",
            CallOrigin::Action => "action",
            CallOrigin::App => "app",
        }
    }
}

/// A set of origins: every cause a commit or a request has (LLP 1133.035
/// D16 item 6). A request that a later commit keeps with newer arguments
/// takes that commit's causes too, and its answer's commit, its `then` and
/// what they ask carry them all, so a rule that refuses a tool sees a
/// tool's arguments however they arrived. The rest of admission reads one
/// origin; only the `none` rule reads the set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::runner) struct Causes(u8);

impl Causes {
    fn bit(origin: CallOrigin) -> u8 {
        1 << origin as u8
    }

    pub(in crate::runner) fn with(self, origin: CallOrigin) -> Causes {
        Causes(self.0 | Self::bit(origin))
    }

    pub(in crate::runner) fn union(self, other: Causes) -> Causes {
        Causes(self.0 | other.0)
    }

    pub(in crate::runner) fn has(self, origin: CallOrigin) -> bool {
        self.0 & Self::bit(origin) != 0
    }

    /// Whether the app (a person, or its own code) is the only cause.
    fn app_only(self) -> bool {
        self.0 & !Self::bit(CallOrigin::App) == 0
    }
}

/// The origins the runner keeps. Only those other than `app` are kept, so
/// an app with no tool or wake keeps nothing.
#[derive(Default)]
pub(in crate::runner) struct Origins {
    /// The commit being made.
    now: CallOrigin,
    /// The commit being made's causes beside `now`: for an answer's commit
    /// and its `then`, the request's whole set.
    causes: Causes,
    /// Each request in flight's causes, by ticket, where one is not the app.
    ticket_causes: BTreeMap<u64, Causes>,
    /// Each resource's last ask's causes, where one is not the app.
    asked_causes: BTreeMap<usize, Causes>,
    /// Each armed `then`'s causes, where one is not the app.
    then_causes: BTreeMap<usize, Causes>,
    /// The causes of the call being admitted where the caller knows them
    /// better than the commit does: a data-code call's are every cause in
    /// flight (`data_causes`).
    calling: Option<Causes>,
    /// Each request in flight, by ticket.
    tickets: BTreeMap<u64, CallOrigin>,
    /// Each resource's last ask, by resource.
    asked: BTreeMap<usize, CallOrigin>,
    /// Each resource whose ask waits for the data source (`stale`), where
    /// that ask's origin is not the app's: the origin, and the arguments it
    /// asked. `data_ready` asks it under that origin.
    deferred: BTreeMap<usize, (CallOrigin, Vec<Value>)>,
    /// A re-ask the device causes is being made: each resource asks under
    /// its own last ask's origin, or its waiting ask's.
    reasking: bool,
    /// Each armed `then`, by mutation, whose answer's commit was not the
    /// app's: the `then` runs under it (LLP 1133.002 D12).
    thens: BTreeMap<usize, CallOrigin>,
    /// The invocation the commit being made is part of; 0 for none.
    id: u64,
    /// The last invocation id given.
    last_id: u64,
    /// Each stamped request in flight, by ticket.
    ids: BTreeMap<u64, u64>,
    /// Each stamped armed `then`, by mutation.
    then_ids: BTreeMap<usize, u64>,
    /// The invocations whose work a commit let go of, each with whether
    /// that work was a send; taken when the commit stands.
    let_go: Vec<(u64, bool)>,
    /// When the last activation-triggering input, or activation the host
    /// raised, arrived, on the session's clock ([`Runner::user_activation`]).
    activated: Option<f64>,
}

/// How long a person's input stands as user activation, in ms. HTML leaves
/// the transient activation duration to the user agent, "at most a few
/// seconds"; this is Chromium's.
const TRANSIENT_ACTIVATION_MS: f64 = 5_000.0;

/// Whether `event` is one of HTML's activation triggering input events
/// (HTML §6.4.1, "Tracking user activation"): a keydown other than Escape,
/// a mouse's `pointerdown`, a touch's or a pen's `pointerup`, and what a
/// host makes of those same acts where the app hears no pointer or key (a
/// press, a control's `input`, `change` and `submit`, a context menu).
fn activates(event: &Event) -> bool {
    match event {
        Event::Key(key) | Event::Keycapture(key) => key.key != "Escape",
        Event::Pointerdown(pointer) => pointer.pointer_type == "mouse",
        Event::Pointerup(pointer) => pointer.pointer_type != "mouse",
        Event::Press
        | Event::PressWith(_)
        | Event::Atompress(_)
        | Event::Dblclick
        | Event::Contextmenu
        | Event::ContextmenuAt(_)
        | Event::Input(_)
        | Event::Change(_)
        | Event::Submit => true,
        _ => false,
    }
}

/// The origin tables as a commit found them (`commit.rs`'s checkpoint).
pub(in crate::runner) struct OriginsSaved {
    tickets: BTreeMap<u64, CallOrigin>,
    asked: BTreeMap<usize, CallOrigin>,
    deferred: BTreeMap<usize, (CallOrigin, Vec<Value>)>,
    thens: BTreeMap<usize, CallOrigin>,
    ticket_causes: BTreeMap<u64, Causes>,
    asked_causes: BTreeMap<usize, Causes>,
    then_causes: BTreeMap<usize, Causes>,
    ids: BTreeMap<u64, u64>,
    then_ids: BTreeMap<usize, u64>,
    let_go: usize,
}

/// Every origin, least privileged first.
pub(in crate::runner) const ORIGINS: [CallOrigin; 5] = [
    CallOrigin::Device,
    CallOrigin::Wake,
    CallOrigin::Tool,
    CallOrigin::Action,
    CallOrigin::App,
];

impl<D: DataSource> Runner<D> {
    /// [`Runner::act`] as a commit of `origin`: a tool invocation's door
    /// (LLP 1111) or a wake's (LLP 1114.001) runs its action through this.
    pub fn act_from(
        &mut self,
        origin: CallOrigin,
        name: &str,
        args: Vec<Value>,
    ) -> Result<CommitReceipt, RunnerError> {
        let before = std::mem::replace(&mut self.origins.now, origin);
        let out = self.act(name, args);
        // Its data calls' reports keep its origin (LLP 1175 D3).
        self.take_reports();
        self.origins.now = before;
        out
    }

    /// The origin of the commit being made.
    pub fn call_origin(&self) -> CallOrigin {
        self.origins.now
    }

    /// The host delivers `event`, the app's own commit: an activation
    /// triggering one starts the window [`Self::user_activation`] reads.
    pub(in crate::runner) fn note_input(&mut self, event: &Event) {
        if self.call_origin() == CallOrigin::App && activates(event) {
            self.origins.activated = Some(self.now_ms);
        }
    }

    /// The host delivers an activation it raised as the app's
    /// (`modules/events.rs`): a person's act on the platform's own UI, a
    /// notification's tap, which no input event carries. It starts the window
    /// as an input does, at the clock's reading now, since no commit need
    /// have come before it.
    pub(in crate::runner) fn note_activation(&mut self) {
        self.sample();
        self.origins.activated = Some(self.now_ms);
    }

    /// Whether a call `origin` makes now has HTML's transient activation:
    /// the app makes it within [`TRANSIENT_ACTIVATION_MS`] of a person's
    /// input or of an activation the host raised. Code a timer or an answer runs inside the window has it too,
    /// as on the web, since a data-code call cannot be traced to the handler
    /// that led to it. Nothing consumes the window: one press may start
    /// several calls.
    pub(in crate::runner) fn user_activation(&self, origin: CallOrigin) -> bool {
        let at = self.origins.activated;
        origin == CallOrigin::App
            && at.is_some_and(|at| (0.0..TRANSIENT_ACTIVATION_MS).contains(&(self.now_ms - at)))
    }

    /// Run `f` as a commit of `origin`, part of invocation `id` (0: none).
    pub(in crate::runner) fn as_invocation<T>(
        &mut self,
        origin: CallOrigin,
        id: u64,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let before = std::mem::replace(&mut self.origins.id, id);
        let out = self.as_origin(origin, f);
        self.origins.id = before;
        out
    }

    /// The invocation the commit being made is part of; 0 for none.
    pub(in crate::runner) fn invocation_id(&self) -> u64 {
        self.origins.id
    }

    /// A new invocation's id.
    pub(in crate::runner) fn next_invocation(&mut self) -> u64 {
        self.origins.last_id += 1;
        self.origins.last_id
    }

    /// The invocation request `ticket` is stamped with; 0 for none.
    pub(in crate::runner) fn ticket_id(&self, ticket: u64) -> u64 {
        self.origins.ids.get(&ticket).copied().unwrap_or(0)
    }

    /// Whether invocation `id` owns work in flight: a request, an armed
    /// `then`, a native statement, a held call or a queued send stamped with it.
    pub(in crate::runner) fn invocation_owns(&self, id: u64) -> bool {
        !self.invocation_tickets(id).is_empty()
            || (self.origins.then_ids.iter())
                .any(|(m, i)| *i == id && self.then_due[*m].is_finite())
            || self.queued_of(id)
            || self.modules_own(id)
    }

    /// The requests in flight stamped with invocation `id`, by ticket.
    pub(in crate::runner) fn invocation_tickets(&self, id: u64) -> Vec<u64> {
        let o = &self.origins;
        (self.pending.iter())
            .filter(|p| o.ids.get(&p.ticket) == Some(&id))
            .map(|p| p.ticket)
            .collect()
    }

    /// Drop the `then`s stamped with invocation `id`.
    pub(in crate::runner) fn drop_invocation_thens(&mut self, id: u64) {
        let thens: Vec<usize> = (self.origins.then_ids.iter())
            .filter(|(_, i)| **i == id)
            .map(|(m, _)| *m)
            .collect();
        for m in thens {
            self.origins.then_ids.remove(&m);
            self.origins.thens.remove(&m);
            self.then_due[m] = f64::INFINITY;
        }
    }

    /// Request `ticket` of `target` is let go by the commit being made: an
    /// invocation's request, let go by a commit not its own (a device's
    /// re-ask is none's), supersedes it when that commit stands.
    pub(in crate::runner) fn note_let_go(&mut self, ticket: u64, target: Target) {
        let o = &self.origins;
        let owner = o.ids.get(&ticket).copied().unwrap_or(0);
        let by = match target {
            Target::Resource(_) if o.reasking => 0,
            _ => o.id,
        };
        if owner != 0 && owner != by {
            let send = target.mutation().is_some();
            self.origins.let_go.push((owner, send));
        }
    }

    /// The origin tables, for a commit's checkpoint.
    pub(in crate::runner) fn origins_saved(&self) -> OriginsSaved {
        let o = &self.origins;
        OriginsSaved {
            tickets: o.tickets.clone(),
            asked: o.asked.clone(),
            deferred: o.deferred.clone(),
            thens: o.thens.clone(),
            ticket_causes: o.ticket_causes.clone(),
            asked_causes: o.asked_causes.clone(),
            then_causes: o.then_causes.clone(),
            ids: o.ids.clone(),
            then_ids: o.then_ids.clone(),
            let_go: o.let_go.len(),
        }
    }

    /// A refused commit's: the tables as it found them.
    pub(in crate::runner) fn origins_restore(&mut self, saved: OriginsSaved) {
        let o = &mut self.origins;
        o.tickets = saved.tickets;
        o.asked = saved.asked;
        o.deferred = saved.deferred;
        o.thens = saved.thens;
        o.ticket_causes = saved.ticket_causes;
        o.asked_causes = saved.asked_causes;
        o.then_causes = saved.then_causes;
        o.ids = saved.ids;
        o.then_ids = saved.then_ids;
        o.let_go.truncate(saved.let_go);
    }

    /// The invocations superseded by commits that stood: each ends so.
    pub(in crate::runner) fn invocations_let_go(&mut self) {
        for (id, send) in std::mem::take(&mut self.origins.let_go) {
            self.wake_superseded(id);
            self.call_superseded(id, send);
        }
    }

    /// Run `f` as a commit of `origin`.
    pub(in crate::runner) fn as_origin<T>(
        &mut self,
        origin: CallOrigin,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        self.as_caused(origin, Causes::default(), f)
    }

    /// Run `f` as a commit of `origin` with `causes` besides.
    fn as_caused<T>(
        &mut self,
        origin: CallOrigin,
        causes: Causes,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let o = &mut self.origins;
        let before = std::mem::replace(&mut o.now, origin);
        let caused = std::mem::replace(&mut o.causes, causes);
        let out = f(self);
        // The reports its data calls made take its authority (LLP 1175 D3).
        self.take_reports();
        let o = &mut self.origins;
        o.now = before;
        o.causes = caused;
        out
    }

    /// [`Self::as_invocation`] with the causes of the request or `then`
    /// whose commit this is.
    pub(in crate::runner) fn as_invocation_caused<T>(
        &mut self,
        origin: CallOrigin,
        id: u64,
        causes: Causes,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let before = std::mem::replace(&mut self.origins.id, id);
        let out = self.as_caused(origin, causes, f);
        self.origins.id = before;
        out
    }

    /// The causes of the commit being made: its origin and its request's.
    pub(in crate::runner) fn commit_causes(&self) -> Causes {
        let o = &self.origins;
        o.causes.with(o.now)
    }

    /// The causes a request of `target` asked now carries: the commit's,
    /// or, for the device's re-ask of a resource, its last ask's.
    pub(in crate::runner) fn causes_for(&self, target: Target) -> Causes {
        let o = &self.origins;
        match target {
            Target::Resource(i) if o.reasking => {
                let causes = o.asked_causes.get(&i).copied().unwrap_or_default();
                causes.with(self.origin_for(Some(target)))
            }
            _ => self.commit_causes(),
        }
    }

    /// Request `ticket`'s causes.
    pub(in crate::runner) fn ticket_causes(&self, ticket: u64) -> Causes {
        let o = &self.origins;
        (o.ticket_causes.get(&ticket).copied().unwrap_or_default()).with(self.ticket_origin(ticket))
    }

    /// Request `ticket` of `target`, in flight, is kept with the newer
    /// arguments of the commit being made: it takes that commit's causes.
    pub(in crate::runner) fn note_kept(&mut self, target: Target, ticket: u64) {
        let causes = self.ticket_causes(ticket).union(self.causes_for(target));
        if !causes.app_only() {
            self.origins.ticket_causes.insert(ticket, causes);
        }
    }

    /// Run `f`, admitting the call it makes with `causes` as its own.
    pub(in crate::runner) fn calling<T>(
        &mut self,
        causes: Causes,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let before = self.origins.calling.replace(causes);
        let out = f(self);
        self.origins.calling = before;
        out
    }

    /// Run `f` as the device's re-ask of its readers (D12).
    pub(in crate::runner) fn reasking<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let before = std::mem::replace(&mut self.origins.reasking, true);
        let out = f(self);
        self.origins.reasking = before;
        out
    }

    /// The origin a request of `target` asked now carries: in a re-ask,
    /// the least privileged of a resource's last ask's and its waiting
    /// ask's.
    pub(in crate::runner) fn origin_for(&self, target: Option<Target>) -> CallOrigin {
        let o = &self.origins;
        match target {
            Some(Target::Resource(i)) if o.reasking => {
                let asked = o.asked.get(&i).copied().unwrap_or_default();
                o.deferred
                    .get(&i)
                    .map_or(asked, |(waiting, _)| asked.min(*waiting))
            }
            _ => o.now,
        }
    }

    /// Resource `i`'s ask at `args` waits for the data source (LLP 1027
    /// D4): its origin is kept for `data_ready`'s ask (D12), the least
    /// privileged of the commits that asked these arguments; a commit that
    /// changes them asks under its own.
    pub(in crate::runner) fn note_deferred(&mut self, i: usize, args: &[Value]) {
        let now = self.origin_for(Some(Target::Resource(i)));
        let deferred = &mut self.origins.deferred;
        let origin = match deferred.get(&i) {
            Some((was, asked)) if asked[..] == *args => now.min(*was),
            _ => now,
        };
        match origin {
            CallOrigin::App => deferred.remove(&i),
            _ => deferred.insert(i, (origin, args.to_vec())),
        };
    }

    /// Resource `i` answered at once: no ask of it waits.
    pub(in crate::runner) fn note_answered(&mut self, i: usize) {
        self.origins.deferred.remove(&i);
    }

    /// Resource `i` is to be asked again for the commit being made, at a
    /// later commit (a send `data_ready` makes): that ask is no more
    /// privileged than this commit (D12).
    pub(in crate::runner) fn note_reasked(&mut self, i: usize) {
        let o = &mut self.origins;
        if o.now != CallOrigin::App {
            let asked = o.asked.get(&i).copied().unwrap_or_default();
            o.asked.insert(i, asked.min(o.now));
        }
    }

    /// The origin of resource `i`'s last ask, when it was not the app's.
    pub(in crate::runner) fn origin_asked(&self, i: usize) -> Option<CallOrigin> {
        self.origins.asked.get(&i).copied()
    }

    /// The origin of request `ticket`'s answer, the commit it lands in.
    pub(in crate::runner) fn ticket_origin(&self, ticket: u64) -> CallOrigin {
        let o = &self.origins;
        o.tickets.get(&ticket).copied().unwrap_or_default()
    }

    /// Request `ticket` of `target` goes out: it keeps its origin, and a
    /// resource's ask is its readers' origin for the device's re-asks.
    pub(in crate::runner) fn note_asked(&mut self, target: Target, ticket: u64) {
        let origin = self.origin_for(Some(target));
        let causes = self.causes_for(target);
        let o = &mut self.origins;
        if origin != CallOrigin::App {
            o.tickets.insert(ticket, origin);
        }
        if !causes.app_only() {
            o.ticket_causes.insert(ticket, causes);
        }
        if let Target::Resource(i) = target {
            match causes.app_only() {
                true => o.asked_causes.remove(&i),
                false => o.asked_causes.insert(i, causes),
            };
            o.deferred.remove(&i);
        }
        let id = match target {
            Target::Resource(_) if o.reasking => 0,
            _ => o.id,
        };
        if id != 0 {
            o.ids.insert(ticket, id);
        }
        if let Target::Resource(i) = target {
            match origin {
                CallOrigin::App => o.asked.remove(&i),
                other => o.asked.insert(i, other),
            };
        }
    }

    /// A data-code call's origin: the least privileged of the commit being
    /// made, every request in flight and every handler running, since such
    /// a call cannot be traced to the answer that made it.
    pub(in crate::runner) fn data_origin(&self) -> CallOrigin {
        let o = &self.origins;
        // (a re-ask's wait runs nothing, `overrun.rs`)
        (self.pending.iter())
            .filter(|p| p.reask_at.is_none())
            .filter_map(|p| o.tickets.get(&p.ticket).copied())
            .chain(self.tool_origin())
            .fold(o.now, CallOrigin::min)
    }

    /// [`Self::data_origin`] for a call that arrives now, outside any commit:
    /// the clock is sampled first, so [`Self::user_activation`] reads the
    /// window against the time of the call and not of the last commit.
    pub(in crate::runner) fn arriving_data_origin(&mut self) -> CallOrigin {
        self.sample();
        self.data_origin()
    }

    /// Every input [`Self::data_origin`] folds, kept apart: the commit's
    /// causes, each request in flight's, and each handler running (LLP
    /// 1133.035 D16 item 6).
    pub(in crate::runner) fn data_causes(&self) -> Causes {
        let tools = self.tool_origin().into_iter();
        (self.pending.iter())
            .map(|p| self.ticket_causes(p.ticket))
            .fold(self.commit_causes(), Causes::union)
            .union(tools.fold(Causes::default(), Causes::with))
    }

    /// Mutation `m`'s `then` is armed by the commit being made.
    pub(in crate::runner) fn note_then(&mut self, m: usize) {
        let causes = self.commit_causes();
        let o = &mut self.origins;
        match o.now {
            CallOrigin::App => o.thens.remove(&m),
            other => o.thens.insert(m, other),
        };
        match causes.app_only() {
            true => o.then_causes.remove(&m),
            false => o.then_causes.insert(m, causes),
        };
        match o.id {
            0 => o.then_ids.remove(&m),
            id => o.then_ids.insert(m, id),
        };
    }

    /// The origin, invocation and causes mutation `m`'s armed `then` runs
    /// under, its answer's commit's; spent here.
    pub(in crate::runner) fn then_origin(&mut self, m: usize) -> (CallOrigin, u64, Causes) {
        let o = &mut self.origins;
        let origin = o.thens.remove(&m).unwrap_or_default();
        let causes = o.then_causes.remove(&m).unwrap_or_default();
        (origin, o.then_ids.remove(&m).unwrap_or(0), causes)
    }

    /// Request `to` carries on request `from`'s ask: its origin and invocation.
    pub(in crate::runner) fn restamp(&mut self, from: u64, to: u64) {
        let o = &mut self.origins;
        if let Some(origin) = o.tickets.remove(&from) {
            o.tickets.insert(to, origin);
        }
        if let Some(id) = o.ids.remove(&from) {
            o.ids.insert(to, id);
        }
    }

    /// Request `to` takes request `from`'s place: its origin, invocation and
    /// causes.
    pub(in crate::runner) fn pass_on(&mut self, from: u64, to: u64) {
        self.restamp(from, to);
        let o = &mut self.origins;
        if let Some(causes) = o.ticket_causes.remove(&from) {
            o.ticket_causes.insert(to, causes);
        }
    }

    /// Forget the origins of requests no longer in flight.
    pub(in crate::runner) fn prune_origins(&mut self) {
        let pending = &self.pending;
        let o = &mut self.origins;
        (o.tickets).retain(|t, _| pending.iter().any(|p| p.ticket == *t));
        (o.ticket_causes).retain(|t, _| pending.iter().any(|p| p.ticket == *t));
        (o.ids).retain(|t, _| pending.iter().any(|p| p.ticket == *t));
    }

    /// Resource row `i` is a new member's (`each.rs`): nothing an earlier
    /// member asked through it stands: its asks' origins, the arguments a
    /// child's start refused, and what the modules keep of it.
    pub(in crate::runner) fn forget_resource_row(&mut self, i: usize) {
        let o = &mut self.origins;
        o.asked.remove(&i);
        o.asked_causes.remove(&i);
        o.deferred.remove(&i);
        self.forget_refused_row(i);
        self.forget_module_row(i);
    }

    /// The causes the call being admitted has: the caller's, where it knows
    /// them better than the commit does ([`Self::calling`]), else the commit's.
    pub(in crate::runner) fn calling_causes(&self) -> Causes {
        (self.origins.calling).unwrap_or_else(|| self.commit_causes())
    }
}

#[cfg(test)]
mod tests {
    use crate::request::{Request, Response};
    use crate::{Answer, CallOrigin, DataError, DataSource, Outcome, Runner, Store, Value};
    use exact_kernel::Kernel;

    /// Every send answers later.
    struct Later;

    impl DataSource for Later {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!("answers through answer")
        }

        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Later(Request::native(b"{}".to_vec())))
        }

        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            _: Outcome,
        ) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::Number(0.0)))
        }
    }

    #[test]
    fn a_module_less_session_forgets_the_origin_of_a_request_that_landed() {
        let source = "component App\n  mutation m as shape number\n  action go\n    send m = save(1)\n  view\n    text \"x\"\n";
        let plan = contract::compile(source).unwrap();
        let mut r = Runner::boot(
            plan,
            Later,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.take_requests();
        r.act_from(CallOrigin::Tool, "go", vec![]).unwrap();
        let sent = r.take_requests();
        assert_eq!(sent.len(), 1);
        assert_eq!(r.origins.tickets.len(), 1);
        let ok = Outcome::Response(Response {
            status: 200,
            headers: Vec::new(),
            body: b"{}".to_vec(),
        });
        r.fulfill(sent[0].ticket, ok).unwrap();
        assert!(r.origins.tickets.is_empty() && r.origins.ids.is_empty());
    }
}
