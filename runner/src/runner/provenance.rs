//! Call provenance (LLP 1133.002 D12, LLP 1133.023 D7 and D7a, LLP
//! 1133.035 D16): who caused each commit, request, armed `then`, queued
//! send, task and continuation, and the invocation it is part of. Every
//! commit has a cause, so this is the core's; a module's admission
//! (`modules/admit.rs`) is one of its readers.
//!
//! **A cause is a value** ([`Cause`]): an origin (D12), the invocation it is
//! part of, its set of causes (D16 item 6) and whether that invocation is a
//! call. Each primitive that schedules work captures the cause of the commit
//! scheduling it and installs it when the work runs ([`Runner::as_cause`]);
//! there is no default. A commit with none installed is refused what the
//! least privileged origin is refused and holds no grant: it never reads as
//! the app's. Boot, the host's delivery of the person's act in the app's own
//! UI and its report of what the person sees while the page shows start an
//! `app` cause ([`Cause::person`]).
//!
//! **Provenance is permanent; grants end with the invocation** (D7a). Work
//! keeps its origin's restrictions for its whole life. An invocation's
//! grants (for a background action, the names held to the app and a
//! recording press's two captures) hold only while it is live
//! ([`Runner::invocation_live`]).
//!
//! **Ownership** (D7) is separate: an invocation owns the requests, armed
//! `then`s, held calls and queued sends its commits stamped
//! ([`Cause::owner`]). A resource's re-ask by the device carries its last
//! ask's cause, invocation included, but is no invocation's work. What an
//! invocation owns, and what lets go of it, is read by its owner alone,
//! which a join keeps apart from the invocation whose grants it holds. The tables
//! are part of a commit's checkpoint, and an invocation is superseded only
//! when the commit that let go of its work stands (`let_go`).

use super::{CommitReceipt, DataSource, Event, Runner, RunnerError, Target};
use exact_plan::Value;
use std::collections::BTreeMap;

/// Who a call comes from (LLP 1133.002 D12), least privileged first. It has
/// no default: a cause is always someone's (LLP 1133.023 D7a).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
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
/// is the join of both ([`Runner::join_causes`]), and its answer's commit, its
/// `then` and what they ask carry it, so a rule that refuses a tool sees a
/// tool's arguments however they arrived. The `none` rule, the foreground
/// rule and a wake's refused commands read the set; the rest of admission
/// reads the folded origin.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(in crate::runner) struct Causes(u8);

impl Causes {
    const fn bit(origin: CallOrigin) -> u8 {
        1 << origin as u8
    }

    pub(in crate::runner) fn union(self, other: Causes) -> Causes {
        Causes(self.0 | other.0)
    }

    pub(in crate::runner) fn has(self, origin: CallOrigin) -> bool {
        self.0 & Self::bit(origin) != 0
    }

    /// The set of `origin` alone.
    pub(in crate::runner) const fn of(origin: CallOrigin) -> Causes {
        Causes(Self::bit(origin))
    }
}

impl Origins {
    /// A booting runner's: boot is the app's.
    pub(in crate::runner) fn booting() -> Origins {
        Origins {
            current: Some(Cause::person()),
            ..Default::default()
        }
    }

    /// Boot is done: work after it brings its own cause.
    pub(in crate::runner) fn booted(&mut self) {
        self.current = None;
    }
}

/// A cause's folded origin, read in place: it borrows its cause, so code
/// outside this module can compare it, name it and tell it to the host, but
/// cannot keep it as a field in place of the cause it folds.
#[derive(Clone, Copy, Debug)]
pub(in crate::runner) struct Folded<'a>(CallOrigin, std::marker::PhantomData<&'a Cause>);

impl Folded<'_> {
    /// Its word in the journal, a ticket and `state.extensions`.
    pub(in crate::runner) fn word(self) -> &'static str {
        self.0.word()
    }
}

impl PartialEq<CallOrigin> for Folded<'_> {
    fn eq(&self, other: &CallOrigin) -> bool {
        self.0 == *other
    }
}

/// Who caused a piece of work (LLP 1133.023 D7a): captured when it is
/// scheduled and installed when it runs.
///
/// Its fields are this module's alone. A cause begins only as the
/// constructors below say (boot or the person's act, the device, an
/// invocation), several combine only through [`Runner::join_causes`], and
/// its origin is the one they fold: no other code can write an origin,
/// a flag or a contributor, or copy one cause's onto another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Cause {
    origin: CallOrigin,
    /// The invocation whose grants it holds while that invocation is live;
    /// 0 for none.
    id: u64,
    /// Every cause it has, its origin's included.
    causes: Causes,
    /// The invocation is a call: LLP 1111 D4a's command rules hold for its
    /// work, after the call too.
    call: bool,
    /// The invocation whose work it is (D7's ownership), whose id its
    /// commits stamp on what they start; 0 for none. A device's re-ask is
    /// no invocation's.
    owner: u64,
    /// A data-code call admitted under the causes in flight: it holds no
    /// press record (LLP 1126.004 D2).
    data: bool,
}

impl Cause {
    /// The app's: boot, or the person's act in the app's own UI or on the
    /// platform's UI for the app.
    pub(in crate::runner) const fn person() -> Cause {
        Cause::begin(CallOrigin::App, 0, false)
    }

    /// The device's: an event a package emits. Work with no cause installed
    /// runs as this, the least privileged origin with no grant.
    pub(in crate::runner) const fn device() -> Cause {
        Cause::begin(CallOrigin::Device, 0, false)
    }

    /// No cause installed: [`Cause::device`].
    pub(in crate::runner) const fn none() -> Cause {
        Cause::device()
    }

    /// Wake `id`'s event (LLP 1133.017 D5).
    pub(in crate::runner) const fn wake(id: u64) -> Cause {
        Cause::begin(CallOrigin::Wake, id, false)
    }

    /// Notification background action `id`, the person's act for that one
    /// action, run with no view (LLP 1133.004).
    pub(in crate::runner) const fn background_action(id: u64) -> Cause {
        Cause::begin(CallOrigin::Action, id, false)
    }

    /// A wake's invocation `id` of `origin`: a wake's event, or a
    /// notification's background action; nothing else.
    pub(in crate::runner) fn wake_event(origin: CallOrigin, id: u64) -> Cause {
        match origin {
            CallOrigin::Action => Cause::background_action(id),
            _ => Cause::wake(id),
        }
    }

    /// Call `id`: a tool's (LLP 1111) or, for a control's press, an
    /// `action` (LLP 1126.004 D2). D4a's command rules hold for its work.
    pub(in crate::runner) fn call(press: bool, id: u64) -> Cause {
        let origin = if press {
            CallOrigin::Action
        } else {
            CallOrigin::Tool
        };
        Cause::begin(origin, id, true)
    }

    /// A remote Play or Pause (the lock screen, headphones, Control Center,
    /// media keys): the person's act on one declared control outside the
    /// app's UI, so an `action` as a control's press is, with D4a's command
    /// rules and no invocation's grants (LLP 1133.023 D7a).
    pub(in crate::runner) const fn remote_control() -> Cause {
        Cause::begin(CallOrigin::Action, 0, true)
    }

    /// A model's tool handler (LLP 1133.016 D10): a tool's, no invocation's.
    pub(in crate::runner) const fn model_tool() -> Cause {
        Cause::begin(CallOrigin::Tool, 0, false)
    }

    /// A world's message carrying the origins posted into it (LLP 1133.022
    /// D6): each one's ended invocation, joined, with a call's command rules
    /// where a tool or a press may be among them. It holds no grant, since a
    /// mark names no invocation, and is never the app's.
    pub(in crate::runner) fn marked(causes: Causes) -> Cause {
        let origin = (ORIGINS.into_iter())
            .find(|o| causes.has(*o) && *o != CallOrigin::App)
            .unwrap_or(CallOrigin::Device);
        Cause {
            causes: causes.union(Causes(Causes::bit(origin))),
            call: causes.has(CallOrigin::Tool) || causes.has(CallOrigin::Action),
            ..Cause::begin(origin, 0, false)
        }
    }

    /// [`Runner::act_from`]'s: an invocation of `origin` with no id, for a
    /// door and a test that name only the origin.
    fn of_origin(origin: CallOrigin) -> Cause {
        Cause::begin(origin, 0, origin == CallOrigin::Tool)
    }

    /// A module event the host raised with `origin`, its word for who caused
    /// it: `app` only for an activation its platform callback reported.
    pub(in crate::runner) const fn raised(origin: CallOrigin) -> Cause {
        Cause::begin(origin, 0, false)
    }

    const fn begin(origin: CallOrigin, id: u64, call: bool) -> Cause {
        Cause {
            origin,
            id,
            causes: Causes(Causes::bit(origin)),
            call,
            owner: id,
            data: false,
        }
    }

    /// The origin, folded from every contributor: the one the host is told.
    /// It is read in place ([`Folded`]), never kept apart from the cause.
    pub(in crate::runner) fn origin(&self) -> Folded<'_> {
        Folded(self.origin, std::marker::PhantomData)
    }

    /// The invocation whose grants it holds while live, 0 for none.
    pub(in crate::runner) fn invocation(self) -> u64 {
        self.id
    }

    /// Every origin among its contributors.
    pub(in crate::runner) fn causes(self) -> Causes {
        self.causes
    }

    /// Whether `origin` is its only contributor's.
    pub(in crate::runner) fn only(self, origin: CallOrigin) -> bool {
        self.causes == Causes(Causes::bit(origin))
    }

    /// Whether D4a's command rules hold for its work.
    pub(in crate::runner) fn is_call(self) -> bool {
        self.call
    }

    /// Whether it is a data-code call's, which holds no press record.
    pub(in crate::runner) fn is_data(self) -> bool {
        self.data
    }

    /// The same cause, not owned by its invocation: a device's re-ask.
    pub(in crate::runner) fn unowned(self) -> Cause {
        Cause { owner: 0, ..self }
    }

    /// The same cause, owned by invocation `owner` (D7's ownership, which
    /// is separate from provenance): a held call or a report a running
    /// invocation must wait for.
    pub(in crate::runner) fn owned_by(self, owner: u64) -> Cause {
        Cause { owner, ..self }
    }

    /// The same cause as a fresh invocation `id`'s: it owns its work and
    /// holds no live invocation's grants, since `id` is no call's or wake's.
    pub(in crate::runner) fn fresh(self, id: u64) -> Cause {
        Cause {
            id,
            owner: id,
            ..self
        }
    }

    /// The same cause with no invocation's grants and no owner: work of
    /// its origins that no invocation is.
    pub(in crate::runner) fn without_invocation(self) -> Cause {
        Cause {
            id: 0,
            owner: 0,
            ..self
        }
    }

    /// The invocation that owns the work, 0 for none.
    pub(in crate::runner) fn owner(self) -> u64 {
        self.owner
    }
}

/// The causes the runner keeps: every request's, resource ask's and armed
/// `then`'s, the app's included, so a missing one is no cause.
#[derive(Default)]
pub(in crate::runner) struct Origins {
    /// The cause of the commit being made, or of the work running.
    current: Option<Cause>,
    /// Each request in flight's cause, by ticket.
    tickets: BTreeMap<u64, Cause>,
    /// Each resource's last ask's cause, by resource.
    asked: BTreeMap<usize, Cause>,
    /// Each resource whose ask waits for the data source (`stale`): that
    /// ask's cause, and the arguments it asked. `data_ready` asks it so.
    deferred: BTreeMap<usize, (Cause, Vec<Value>)>,
    /// A re-ask the device causes is being made: each resource asks under
    /// its own last ask's cause, or its waiting ask's.
    reasking: bool,
    /// Each armed `then`'s cause, its answer's commit's, by mutation.
    thens: BTreeMap<usize, Cause>,
    /// The last invocation id given.
    last_id: u64,
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
    tickets: BTreeMap<u64, Cause>,
    asked: BTreeMap<usize, Cause>,
    deferred: BTreeMap<usize, (Cause, Vec<Value>)>,
    thens: BTreeMap<usize, Cause>,
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
        self.as_cause(Cause::of_origin(origin), |r| r.act_named(name, args))
    }

    /// Whether a cause is installed.
    #[cfg(test)]
    pub(in crate::runner) fn has_cause(&self) -> bool {
        self.origins.current.is_some()
    }

    /// The cause of the commit being made, or [`Cause::none`].
    pub(in crate::runner) fn cause(&self) -> Cause {
        self.origins.current.unwrap_or(Cause::none())
    }

    /// The host delivers `event`, the app's own commit: an activation
    /// triggering one starts the window [`Self::user_activation`] reads.
    pub(in crate::runner) fn note_input(&mut self, event: &Event) {
        if self.cause().origin == CallOrigin::App && activates(event) {
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

    /// Whether a call `cause` makes now has HTML's transient activation:
    /// the app makes it within [`TRANSIENT_ACTIVATION_MS`] of a person's
    /// input or of an activation the host raised. Code a timer or an answer runs inside the window has it too,
    /// as on the web, since a data-code call cannot be traced to the handler
    /// that led to it. Nothing consumes the window: one press may start
    /// several calls.
    pub(in crate::runner) fn user_activation(&self, cause: Cause) -> bool {
        let at = self.origins.activated;
        cause.origin() == CallOrigin::App
            && at.is_some_and(|at| (0.0..TRANSIENT_ACTIVATION_MS).contains(&(self.now_ms - at)))
    }

    /// What a request asked under `cause` carries to the host, whose own
    /// admission reads its folded origin: the one place an origin leaves a
    /// cause, besides [`Self::view_origin`]'s answer outside the runner.
    pub(in crate::runner) fn reach_of(&self, cause: Cause) -> crate::local_network::Reach {
        crate::local_network::Reach {
            origin: cause.origin,
            substitute: self.network_substitute(),
        }
    }

    /// The origin the runner reported for view `id`'s displayed state, or for
    /// its binding `prop`: what an acquisition the view makes is admitted
    /// with (LLP 1133.008 D3).
    pub fn view_origin(&self, id: exact_kernel::ViewId, prop: Option<&str>) -> Option<CallOrigin> {
        let cause = self.seams.modules.view_origin(self, id, prop)?;
        Some(cause.origin)
    }

    /// Run `f` under `cause`: the one way work gets a cause (D7a).
    pub(in crate::runner) fn as_cause<T>(
        &mut self,
        cause: Cause,
        f: impl FnOnce(&mut Self) -> T,
    ) -> T {
        let before = self.origins.current.replace(cause);
        let out = f(self);
        // The reports its data calls made take its authority (LLP 1175 D3).
        self.take_reports();
        self.origins.current = before;
        out
    }

    /// Run `f` as the person's act in the app's own UI (LLP 1133.023 D7a):
    /// for an act a host delivers other than through [`Runner::dispatch`],
    /// and a test of the app's own data code.
    pub fn as_person<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        self.as_cause(Cause::person(), f)
    }

    /// The invocation that owns the commit being made; 0 for none.
    pub(in crate::runner) fn invocation_id(&self) -> u64 {
        self.cause().owner()
    }

    /// A new invocation's id.
    pub(in crate::runner) fn next_invocation(&mut self) -> u64 {
        self.origins.last_id += 1;
        self.origins.last_id
    }

    /// Whether invocation `id` is live: a call running uncancelled within
    /// its bound, or the current wake's running invocation. Its grants hold
    /// only while it is (D7a).
    pub(in crate::runner) fn invocation_live(&self, id: u64) -> bool {
        id != 0 && (self.call_live(id) || self.wake_invocation_live(id))
    }

    /// Whether work of `cause` holds the person's grants: the app's always,
    /// a background action's while its invocation is live.
    pub(in crate::runner) fn persons(&self, cause: Cause) -> bool {
        match cause.origin {
            CallOrigin::App => true,
            CallOrigin::Action => self.invocation_live(cause.id),
            _ => false,
        }
    }

    /// The cause request `ticket` was asked under; none for a ticket the
    /// runner did not ask.
    pub(in crate::runner) fn ticket_cause(&self, ticket: u64) -> Cause {
        (self.origins.tickets.get(&ticket).copied()).unwrap_or(Cause::none())
    }

    /// The invocation request `ticket` is owned by; 0 for none.
    pub(in crate::runner) fn ticket_id(&self, ticket: u64) -> u64 {
        self.ticket_cause(ticket).owner()
    }

    /// Whether invocation `id` owns work in flight: a request, an armed
    /// `then`, a native statement, a held call or a queued send stamped with it.
    pub(in crate::runner) fn invocation_owns(&self, id: u64) -> bool {
        !self.invocation_tickets(id).is_empty()
            || (self.origins.thens.iter())
                .any(|(m, c)| id != 0 && c.owner() == id && self.then_due[*m].is_finite())
            || self.queued_of(id)
            || self.modules_own(id)
    }

    /// The requests in flight stamped with invocation `id`, by ticket; none
    /// for id 0, which owns nothing.
    pub(in crate::runner) fn invocation_tickets(&self, id: u64) -> Vec<u64> {
        (self.pending.iter())
            .filter(|p| id != 0 && self.ticket_id(p.ticket) == id)
            .map(|p| p.ticket)
            .collect()
    }

    /// Drop the `then`s stamped with invocation `id`.
    pub(in crate::runner) fn drop_invocation_thens(&mut self, id: u64) {
        let thens: Vec<usize> = (self.origins.thens.iter())
            .filter(|(_, c)| id != 0 && c.owner() == id)
            .map(|(m, _)| *m)
            .collect();
        for m in thens {
            self.origins.thens.remove(&m);
            self.then_due[m] = f64::INFINITY;
        }
    }

    /// Request `ticket` of `target` is let go by the commit being made: an
    /// invocation's request, let go by a commit not its own (a device's
    /// re-ask is none's), supersedes it when that commit stands.
    pub(in crate::runner) fn note_let_go(&mut self, ticket: u64, target: Target) {
        let owner = self.ticket_id(ticket);
        let by = self.cause_for(target).owner();
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
        o.let_go.truncate(saved.let_go);
    }

    /// The invocations superseded by commits that stood: each ends so.
    pub(in crate::runner) fn invocations_let_go(&mut self) {
        for (id, send) in std::mem::take(&mut self.origins.let_go) {
            self.wake_superseded(id);
            self.call_superseded(id, send);
        }
    }

    /// The cause a request of `target` asked now carries: the commit's, or,
    /// for a resource asked by the device's re-ask or by a commit with no
    /// cause (a clock's, a reconciliation's), the join of its last ask's and
    /// its waiting ask's, and of the commit's where it has one, owned by no
    /// invocation.
    pub(in crate::runner) fn cause_for(&self, target: Target) -> Cause {
        let o = &self.origins;
        match target {
            Target::Resource(i) if o.reasking || o.current.is_none() => {
                let asked = o.asked.get(&i).copied();
                let waiting = o.deferred.get(&i).map(|(c, _)| *c);
                if asked.is_none() && waiting.is_none() {
                    // Its first ask: the commit's own.
                    return self.cause();
                }
                let cause = (asked.into_iter().chain(waiting).chain(o.current))
                    .reduce(|a, b| self.join_causes(a, b));
                cause.unwrap_or(Cause::none()).unowned()
            }
            _ => self.cause(),
        }
    }

    /// The join of two causes, for work that depends on both (LLP 1133.023
    /// D7a, as amended): the one function that makes a cause of several.
    /// Its authority is what both allow. Its origin, the one the host is
    /// told, is the lower, else the one that holds no grant now, so it
    /// holds a grant only while both would and is the person's only when
    /// both are (one invocation runs at a time, so two that are not the
    /// app's never both hold a grant). Every origin of both stays among its
    /// causes, whose own rules apply; a call's command rules and a data
    /// call's want of a press record hold if either has them. It is an
    /// invocation's work only if both are that invocation's.
    pub(in crate::runner) fn join_causes(&self, a: Cause, b: Cause) -> Cause {
        let grants = |c: Cause| matches!(c.origin, CallOrigin::App | CallOrigin::Action);
        let rank = |c: Cause| (c.origin, !grants(c) || self.persons(c));
        let least = if rank(b) < rank(a) { b } else { a };
        Cause {
            causes: a.causes.union(b.causes),
            call: a.call || b.call,
            data: a.data || b.data,
            owner: if a.owner == b.owner { a.owner } else { 0 },
            ..least
        }
    }

    /// Request `ticket` of `target`, in flight, is kept with the newer
    /// arguments of the commit being made: its cause is the join of its
    /// ask's and that commit's, and it stays its first asker's work (D7:
    /// it is not let go).
    pub(in crate::runner) fn note_kept(&mut self, target: Target, ticket: u64) {
        let was = self.ticket_cause(ticket);
        let cause = self.join_causes(was, self.cause_for(target));
        self.origins
            .tickets
            .insert(ticket, cause.owned_by(was.owner));
    }

    /// The cause of `data_ready`'s commit (LLP 1027 D4): the join of the
    /// work that waited for the data source and shows in it. That is each
    /// send made before it (`unsent`), each optimistic write, which only the
    /// data source can lay over an answer so this commit first shows, each
    /// resource it asks again by the ask that waited, else its last (boot's
    /// own asks, at a placeholder or a checkpoint's answer, and a reader whose
    /// gate never opened, record none and are the app's), and a peer's change
    /// to the store, which is no one's.
    pub(in crate::runner) fn ready_cause(&self, stale: &[usize], store_changed: bool) -> Cause {
        let sends = self.unsent.iter().map(|u| u.3);
        let writes = self.writes.records.iter().map(|w| w.cause);
        let o = &self.origins;
        let asks = (stale.iter()).map(|i| {
            (o.deferred.get(i).map(|(c, _)| *c))
                .or(o.asked.get(i).copied())
                .unwrap_or(Cause::person())
        });
        let peer = store_changed.then(Cause::none);
        (sends.chain(writes).chain(asks).chain(peer))
            .reduce(|a, b| self.join_causes(a, b))
            .unwrap_or(Cause::none())
    }

    /// Run `f` as the device's re-ask of its readers (D12).
    pub(in crate::runner) fn reasking<T>(&mut self, f: impl FnOnce(&mut Self) -> T) -> T {
        let before = std::mem::replace(&mut self.origins.reasking, true);
        let out = f(self);
        self.origins.reasking = before;
        out
    }

    /// Resource `i`'s ask at `args` waits for the data source (LLP 1027
    /// D4): its cause is kept for `data_ready`'s ask (D12), the join of the
    /// commits that asked these arguments; a commit that changes them asks
    /// under its own.
    pub(in crate::runner) fn note_deferred(&mut self, i: usize, args: &[Value]) {
        let now = self.cause_for(Target::Resource(i));
        let cause = match self.origins.deferred.get(&i) {
            Some((was, asked)) if asked[..] == *args => self.join_causes(now, *was),
            _ => now,
        };
        self.origins.deferred.insert(i, (cause, args.to_vec()));
    }

    /// Resource `i`'s gate closed: no ask of it waits.
    pub(in crate::runner) fn forget_deferred(&mut self, i: usize) {
        self.origins.deferred.remove(&i);
    }

    /// Resource `i` was asked, however it answered: its readers' cause is
    /// this ask's for the device's re-asks.
    pub(in crate::runner) fn note_ask(&mut self, i: usize) {
        let cause = self.cause_for(Target::Resource(i)).unowned();
        self.origins.asked.insert(i, cause);
    }

    /// Resource `i` answered at once: no ask of it waits.
    pub(in crate::runner) fn note_answered(&mut self, i: usize) {
        self.origins.deferred.remove(&i);
    }

    /// Resource `i` is to be asked again for the commit being made, at a
    /// later commit (a send `data_ready` makes): that ask is no more
    /// privileged than this commit (D12).
    pub(in crate::runner) fn note_reasked(&mut self, i: usize) {
        let now = self.cause().unowned();
        let cause = match self.origins.asked.get(&i) {
            Some(asked) => self.join_causes(*asked, now),
            None => now,
        };
        self.origins.asked.insert(i, cause);
    }

    /// The cause of resource `i`'s last ask, if it was asked.
    pub(in crate::runner) fn asked_cause(&self, i: usize) -> Option<Cause> {
        self.origins.asked.get(&i).copied()
    }

    /// Request `ticket` of `target` goes out: it keeps its cause, and a
    /// resource's ask is its readers' cause for the device's re-asks.
    pub(in crate::runner) fn note_asked(&mut self, target: Target, ticket: u64) {
        let cause = self.cause_for(target);
        let o = &mut self.origins;
        o.tickets.insert(ticket, cause);
        if let Target::Resource(i) = target {
            o.deferred.remove(&i);
            o.asked.insert(i, cause.unowned());
        }
    }

    /// A data-code call's cause, since such a call cannot be traced to the
    /// answer that made it: the least privileged of the commit being made,
    /// every request in flight and every handler running, with all their
    /// causes; none when nothing is in flight. It holds no press record.
    pub(in crate::runner) fn data_cause(&self) -> Cause {
        // (a re-ask's wait runs nothing, `overrun.rs`)
        let tickets = (self.pending.iter())
            .filter(|p| p.reask_at.is_none())
            .map(|p| self.ticket_cause(p.ticket));
        let folded = (self.origins.current.into_iter())
            .chain(tickets)
            .chain(self.tool_cause())
            .reduce(|a, b| self.join_causes(a, b));
        // Its invocation ranks it, but the call is not its own: it owns
        // nothing.
        Cause {
            owner: 0,
            data: true,
            ..folded.unwrap_or(Cause::none())
        }
    }

    /// [`Self::data_cause`] for a call that arrives now, outside any commit:
    /// the clock is sampled first, so [`Self::user_activation`] and
    /// [`Self::invocation_live`] read the time of the call and not of the
    /// last commit.
    pub(in crate::runner) fn arriving_data_cause(&mut self) -> Cause {
        self.sample();
        self.data_cause()
    }

    /// Mutation `m`'s `then` is armed under `cause`, that of the commit its
    /// answer landed in.
    pub(in crate::runner) fn note_then(&mut self, m: usize, cause: Cause) {
        self.origins.thens.insert(m, cause);
    }

    /// The cause mutation `m`'s armed `then` runs under, its answer's
    /// commit's; spent here.
    pub(in crate::runner) fn then_cause(&mut self, m: usize) -> Cause {
        (self.origins.thens.remove(&m)).unwrap_or(Cause::none())
    }

    /// Request `to` carries on request `from`'s ask: its cause.
    pub(in crate::runner) fn restamp(&mut self, from: u64, to: u64) {
        let o = &mut self.origins;
        if let Some(cause) = o.tickets.remove(&from) {
            o.tickets.insert(to, cause);
        }
    }

    /// Forget the causes of requests no longer in flight.
    pub(in crate::runner) fn prune_origins(&mut self) {
        let pending = &self.pending;
        (self.origins.tickets).retain(|t, _| pending.iter().any(|p| p.ticket == *t));
    }

    /// Resource row `i` is a new member's (`each.rs`): nothing an earlier
    /// member asked through it stands: its asks' causes, the arguments a
    /// child's start refused, and what the modules keep of it.
    pub(in crate::runner) fn forget_resource_row(&mut self, i: usize) {
        let o = &mut self.origins;
        o.asked.remove(&i);
        o.deferred.remove(&i);
        self.forget_refused_row(i);
        self.forget_module_row(i);
    }
}

#[cfg(test)]
#[path = "cause_tests.rs"]
mod cause_tests;

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
        assert!(r.origins.tickets.is_empty());
    }

    /// LLP 1133.023 D7a: the device's re-ask of a resource an invocation
    /// asked carries that invocation's cause, but is not its work.
    #[test]
    fn a_device_reask_keeps_the_invocation_s_cause_and_is_not_its_work() {
        let source = "component App\n  state k = 0\n  resource v = load(k) as shape number\n  action go\n    k = 1\n  view\n    text \"x\"\n";
        let plan = contract::compile(source).unwrap();
        let mut r = Runner::boot(
            plan,
            Later,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let ok = || {
            Outcome::Response(Response {
                status: 200,
                headers: Vec::new(),
                body: b"{}".to_vec(),
            })
        };
        let boot = r.take_requests();
        r.fulfill(boot[0].ticket, ok()).unwrap();
        let tool = super::Cause::call(false, 7);
        r.as_cause(tool, |r| r.act_named("go", vec![])).unwrap();
        let asked = r.take_requests();
        assert_eq!(r.ticket_id(asked[0].ticket), 7, "the call's own ask");
        r.fulfill(asked[0].ticket, ok()).unwrap();
        r.reasking(|r| r.commit_again(vec![0], "changed")).unwrap();
        let again = r.take_requests();
        let cause = r.ticket_cause(again[0].ticket);
        assert_eq!((cause.origin, cause.id), (CallOrigin::Tool, 7));
        assert!(!r.invocation_owns(7), "a re-ask is no invocation's work");
    }
}
