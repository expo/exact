//! The runner: boot, actions, events, resources, timers, the clock.
//!
//! @ref LLP 1004 D3 (the refusal tuple) / D4 (the data seam) / D5 (restart)
//! Each event settles derives and changed resource arguments, evaluates all
//! sites, and applies one atomic kernel batch; hosts then lay out and paint.
//! Kernel validation precedes every write; a refusal leaves the kernel untouched.
mod admission;
mod advance;
pub use advance::{Advanced, Timed};
mod capture;
mod native_timers;
mod retirement;
pub use native_timers::NativeTimer;
use native_timers::Timer;
pub use retirement::{ModuleIdentity, ModuleInstance};
mod background;
mod commit;
mod console;
mod drag_files;
pub use console::Console;
mod completion;
mod control;
mod errors;
mod reply;
pub use completion::{Completion, CompletionCapability, CompletionReply, CompletionSubscription};
#[cfg(test)]
mod request_set_tests;
#[cfg(test)]
mod shared_plan_tests;
pub use errors::RunnerError;
mod event;
pub mod faults;
mod fetches;
mod field;
mod fonts;
pub mod foresee;
mod in_flight;
mod mapper_turns;
mod module_work;
mod trace;
pub use field::{FieldSelection, InputDetails, SelectionDirection};
mod host_kinds;
pub use host_kinds::{HistorySwipe, ScrollDrag};
mod pointer;
pub use pointer::{AtomEvent, DropEvent, MouseEvent, PointerEvent, WheelEvent};
mod intersect;
mod mounted_reorder;
mod reorder;
mod reorder_codec;
mod reorder_group;
pub use intersect::Intersection;
pub use shown::BlockPlacement;
mod box_resize;
mod geometry_facts;
mod resize;
pub use resize::{ResizeRect, UNDELIVERED as RESIZE_UNDELIVERED};
mod held;
mod live;
mod mode;
pub use held::Asked;
mod audio_seam;
#[cfg(test)]
mod audio_seam_tests;
mod forma;
mod root_font;
mod shown;
mod sounds;
mod surface_colors;
mod theme;
pub use event::{
    ActionBinding, ActionBindingError, ActionBindingRefusal, ControlValue, Event, KeyModifiers,
    KeyboardEvent, ScrollEvent,
};
pub use forma::{
    Color as ColorKind, Font, Forma, Kind, Length as LengthKind, Number as NumberKind, Place,
    Token, Type as TypeKind,
};
pub use live::{LiveApplied, LiveRefusal};
mod canvas2d;
pub use canvas2d::{
    engine as canvas_engine, CanvasEngine, CanvasList, DrawReply, DrawRequest, Drawn, Geometry,
    Limits,
};
mod carry;
mod checkpoint;
mod collection;
mod snapshot;
mod source;
mod tabs;
pub use source::{
    Announce, BackgroundState, DataError, DataSource, DataSourceWrapper, InFlight, Interrupt,
    Native, NativeCall, NativeHandler, Overlaid, PreloadWake, QueuedSendReplacer, Report, Reporter,
    ResourceChange, SnapshotModule, Target, Write, BACKGROUND,
};
mod delivery;
mod links;
pub use links::{CanvasLink, FormatLink, GeometryLink, RouterLink, RunnerLinks, SurfaceAnswer};
pub use text_axes::TextAxesLink;
mod device;
mod device_links;
mod document;
pub use device::{Hold, HoldAnswer};
pub use device_links::{AuthLinks, DeviceLinks, PickerLinks};
pub mod picker;
pub use picker::{Picked, PickerRequest, PICKED};
mod dated;
mod each;
mod facts;
mod fragments;
mod gates;
pub(crate) mod resources;
mod waits;
pub use waits::{
    frame_at_or_after, virtual_frame, Admitted, IdleObservation, TaskTiming, Wait, WaitKinds, Waits,
};
mod into_view;
pub(crate) mod kept;
#[cfg(test)]
mod kept_tests;
mod kept_wait;
mod lines;
mod list_seam;
#[cfg(test)]
mod list_seam_tests;
mod lists;
mod media_session;
pub mod outstanding;
mod overrun;
mod owned;
mod owned_sends;
mod owned_tasks;
mod page;
mod parallel;
mod perf;
pub mod persist;
mod query;
mod queue;
mod render;
mod resource_inspection;
mod world_origin;
pub use queue::QUEUE_BOUND;
mod history;
pub mod router;
pub use crate::list_api::{ListTextPolicy, ListTextPosition};
mod revalidation;
mod settlement;
pub(crate) use settlement::zero;
mod sockets;
pub use sockets::{SocketReader, SocketState};
mod activities;
mod apply;
mod live_time;
pub mod midi;
mod modules;
pub use modules::{check_arrival_field, check_source_substitute, AppState};
mod calls;
pub use calls::CallHow;
mod module_seam;
pub use module_seam::Modules;
mod presence_secrets;
pub use presence_secrets::{PresenceAnswer, PresenceAsk};
mod processes;
mod provenance;
mod seams;
pub use audio_seam::{Audio, LinkedAudio, NoAudio};
pub use list_seam::Lists;
pub(crate) use processes::held as process_held;
pub use provenance::CallOrigin;
pub use seams::{ModuleLinks, PresenceSecrets, TextAxes, Tools};
mod wakes;
pub use wakes::WakeHow;
#[cfg(test)]
mod abort_tests;
mod canvas_performance;
mod split;
mod surface_record;
mod text_axes;
mod time;
mod transport;
#[cfg(test)]
mod transport_tests;
mod uploads;
mod viewport;
mod writes;
use crate::instance::{Ids, InstanceError, InstanceStep, SurfaceUpdate, Tree, Update};
use crate::request::{Answer, Dispatch, Outcome, Request, RequestClass, RequestOut};
use crate::store::{Store, StoreWrite};
use crate::vm::{self, Env, Frame, RowSlots};
pub use carry::Carried;
pub use checkpoint::Checkpoint;
use exact_kernel::{CommitReceipt, Kernel, ViewId};
use exact_plan::{ActionsId, Code, EventKind, MutationMode, MutationsId, NodesId, Plan, Value};
pub use router::{launch_matches, routing, RouterChange, Routing};
pub use split::{press_writes_router, RouteInspector, StackSplit};
pub use uploads::{UploadEnd, UploadMark};

pub use crate::commands::Command;

#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
struct ResourceState {
    /// Development inspection only; zero means the request was not observed.
    inspection_generation: u64,
    args: Vec<Value>,
    value: crate::held::Held,
    /// Store revision this answer observed; checked only for known readers.
    store_revision: u64,
    /// A placeholder shown before an answer, including after failure, never an answer
    /// to reuse, carry or compile (LLP 1054.000.002 D4).
    placeholder: bool,
    /// Only a persisted boot seed can match identifying arguments before
    /// readiness (LLP 1027.005 D3). Settlement and rollback copy this with
    /// its value; activation or losing that identity consumes it.
    kept_seed: bool,
    /// The rendered page's answer, seeded from its checkpoint (LLP 1048.000
    /// D6), until another value replaces it: what the first frame showed is
    /// the page's, not the last session's.
    from_page: bool,
    /// The sequence when this answer was asked (0 for a compiled, kept,
    /// carried or placeholder value): a write that landed before it is
    /// answered by it (`writes.rs`).
    origin: u64,
    /// The arguments `value` answers when they are not `args`: the bake's,
    /// for a compiled answer standing in for newer arguments until the
    /// source can answer. An overlay lays writes over it for these.
    answered_for: Option<Vec<Value>>,
    /// Its `when` was false at the last settlement: idle, asked nothing
    /// (LLP 1161.002 D2).
    asleep: bool,
    /// The value was kept across idleness: no answer to the arguments it
    /// has now until a fresh one lands (LLP 1161.001 D1).
    slept: bool,
}

impl ResourceState {
    fn answered_args(&self) -> &[Value] {
        self.answered_for.as_deref().unwrap_or(&self.args)
    }

    /// This state, idle (LLP 1161.002 D2) and kept across idleness: no
    /// answer until a fresh one after the wake (LLP 1161.001 D1).
    fn sleeping(mut self) -> Self {
        self.asleep = true;
        self.slept = true;
        self
    }
}

/// What a boot starts from besides the plan and the launch.
#[derive(Clone, Copy)]
enum Seed<'a> {
    /// The plan's initial state.
    Fresh,
    /// Fresh state, keeping a running session's sequence.
    Reboot(&'a Carried),
    /// Fresh state evaluated at the supplied render clock.
    At(f64),
    /// A reload's carried state (LLP 1007 §6).
    Carried(&'a Carried),
    /// A rendered document's answers (LLP 1048.000 D6).
    Checkpoint(&'a Checkpoint, Option<&'a Carried>),
}

/// A request the host is running: the ticket its reply carries, what it
/// answers, and the arguments it was asked with (what `parse` sees).
#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
struct PendingReq {
    completion: Option<CompletionReply>,
    module: Option<ModuleInstance>,
    inspection_generation: u64,
    refusal: Option<(String, bool)>,
    /// The host refused this ordered request before execution (LLP 1041 §8.4).
    /// Earlier source turns may already have executed effects.
    refused: bool,
    ticket: u64,
    target: Target,
    source: String,
    args: Vec<Value>,
    /// The source's continuation token, when the request is one.
    continuation: Option<u64>,
    /// The module's own ticket for the fetch this request is
    /// ([`crate::Request::fetch`]): what an app's abort names it by.
    fetch: Option<u64>,
    /// The request itself when a re-ask may keep it: a resource's plain
    /// HTTP request (LLP 1054.000.000 D3).
    keepable: Option<Request>,
    /// An answer that keeps coming (LLP 1016.000): what it has delivered.
    stream: Option<StreamCount>,
    stream_url: Option<Box<str>>,
    /// An upload's answer ([`Request::upload_progress`]), including its reply's later turns.
    upload: bool,
    /// The upload has begun to go out (`UPLOAD_SENDING`, or its first bytes
    /// sent): a failure before that never reached the network (D10).
    sent: bool,
    /// A stream's: the grant line that admits it (`state.streams`).
    grant: Option<String>,
    /// A topic its resource watches changed while it was in flight: its
    /// reply lands, then the resource is asked again (LLP 1016.002 D4).
    ask_again: bool,
    /// Module work wrote the store while it was in flight, leaving this store
    /// revision: until another write, its answer stands (LLP 1146 §2.3).
    module_again: Option<u64>,
    /// May change its server ([`crate::Request::has_effects`]): kept when let go (LLP 1146 D2).
    effects: bool,
    /// An earlier round may have changed its server; the answer cannot be replayed.
    replay_unsafe: bool,
    /// The sequence when its answer was first asked; its later rounds keep
    /// it. Only an answer asked after a write landed retires that write.
    origin: u64,
    /// A mutation's: the write it carries (`writes.rs`), which only its
    /// reply lands or ends.
    write: Option<u64>,
    /// Held (LLP 1016.000 D9, [`Request::is_held`]): pending, but no clock waits on it. A long poll (D8),
    /// `replied` once it has a reply of any kind, is held and named too.
    held: bool,
    long_poll: bool,
    replied: bool,
    /// Never sent: when a resource an overrun refused is asked again (`overrun.rs`).
    reask_at: Option<f64>,
    /// The launch's own ask (its target was in `Runner::launch` when asked),
    /// and what its replies ask for next: a wait for it, at the ask or after
    /// its reply's turn over its budget, asks again as the launch's. A refresh
    /// clears it (`force_refresh`).
    launch: bool,
    /// The re-asks after a turn over its budget this ask's streak has made.
    reasks: usize,
}

/// An open stream's committed messages, and those the host coalesced away
/// (LLP 1016.000 D4, D5). One message landed ends its `pending`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct StreamCount {
    /// Messages whose mapped values committed.
    pub messages: u64,
    /// Messages the host dropped for a newer one before delivery.
    pub coalesced: u64,
    /// A two-way socket's state (LLP 1016.000 D7); `None` for a listener.
    pub socket: Option<SocketState>,
}

impl PendingReq {
    /// In flight: every request, a stream until its first value commits, and an
    /// upload until its reply. What `pending(…)` reads, held requests included.
    fn in_flight(&self) -> bool {
        self.upload || self.stream.is_none_or(|s| s.messages == 0)
    }

    /// Waited on by a clock (a jump, `clock settle`, `clock data`): in flight, not held (D8, D9),
    /// not a re-ask's wait (no I/O: the agent's settle goes to its time, `overrun.rs`).
    fn waited(&self) -> bool {
        self.in_flight() && !self.quiet() && !self.replied && self.reask_at.is_none()
    }

    /// A request with effects the host has not refused at admission, and no
    /// re-ask's wait (never sent), so it may reach its server: one let go is
    /// offered to its source to keep, and the app is held open while it is
    /// out (LLP 1146 D2).
    fn admitted_effects(&self) -> bool {
        self.effects && self.refusal.is_none() && !self.refused && self.reask_at.is_none()
    }
}

type FocusPath = Vec<(ViewId, NodesId, Vec<Frame>)>;

/// A send made before the data source was ready: its target, source,
/// arguments and the cause `data_ready` sends it under (LLP 1133.023 D7a).
type Unsent = (Target, String, Vec<Value>, provenance::Cause);
/// A send's request this commit hands out once it stands.
type Later = (Target, String, Vec<Value>, Request);

/// One plan, one data source, one kernel.
pub struct Runner<D: DataSource> {
    plan: Plan,
    /// The plan's strings as values, made as they are read (`vm::intern`).
    strings: vm::Strings,
    sites: crate::instance::SiteIndex,
    inspection_digest: std::cell::OnceCell<String>,
    /// Panel templates with a route that can request a split.
    split_panel_sites: std::cell::OnceCell<Vec<NodesId>>,
    /// The plan's tab panel sites and whether each needs a navigation controller.
    panel_navigation: std::cell::OnceCell<(Vec<NodesId>, Vec<bool>)>,
    /// [`Runner::keeps_midi`], made at the first agent read.
    keeps_midi: std::cell::OnceCell<bool>,
    action_binding_origin: std::rc::Rc<()>,
    detached_focus: Vec<(ViewId, FocusPath)>,
    data: D,
    bake_warnings: Vec<String>,
    kernel: Kernel,
    generation: u64,
    replacement: Option<carry::Replacement>,
    /// What a waiting replacement waits for, as the host says
    /// ([`Runner::set_replacement_pending`]).
    replacement_pending: Option<&'static str>,
    slots: Vec<Value>,
    derives: Vec<Option<Value>>,
    resources: Vec<resources::ResourceRow>,
    tree: Option<Tree>,
    reorder_owner: Option<exact_kernel::NodeKey>,
    reorder_target: Option<exact_kernel::NodeKey>,
    reorder_ops: Vec<exact_kernel::Op>,
    /// The wide colours the queued patches hold, kept from the table's sweep.
    reorder_kept: exact_kernel::style::wide::Keep,
    /// The design digest a live batch left (LLP 1121 stage 4); `None`: its plan's own.
    design: Option<String>,
    /// A development build admitting live batches ([`Runner::admit_live`]), and its last scrub frame.
    live: live::Live,
    /// What a binding was told of this plan's tokens (a name it lacks), each said once (LLP 1121 D12).
    forma_said: Vec<String>,
    /// How many times a token's value moved (`set_tokens`): a binding's input.
    design_moves: u64,
    /// Its role overrides, its own (LLP 1121 stage 5; [`Runner::roles`]).
    roles: exact_kernel::style::roles::Installed,
    /// The session the owner's token names (LLP 1094 D4).
    /// Only the list engine makes one (`reorder_group::Session`).
    reorder_session: Option<Box<dyn std::any::Any>>,
    /// A grouped drop's source while its `reorderdrop` runs (D2).
    dropping_from: Option<exact_kernel::NodeKey>,
    ids: Ids,
    pub(crate) orientation: crate::orientation::Said, // the host's readback, what was journaled (LLP 1133.029)
    now_ms: f64,
    timers: Vec<Timer>,
    clock: advance::Clock,
    batch: u64,
    commands: Vec<Command>,
    /// `scrollIntoView` commands an action stated, run after its update.
    into_view: Vec<crate::list_api::IntoView>,
    /// Where the host last showed each scroller (`frame`, LLP 1051.000 D1).
    scrolled: crate::geometry::Scrolled,
    /// Each `resize` handler's node and the content box last delivered.
    resized: resize::Resized,
    intersections: intersect::Intersections,
    /// Refused requests, for `state` (LLP 1070.000 §2.2).
    into_view_refused: std::collections::VecDeque<String>,
    surfaces: Vec<SurfaceUpdate>,
    surface_colors: surface_colors::Bindings,
    /// Views the next applied batch renews (LLP 1078): rebound list rows.
    renewed: Vec<ViewId>,
    /// Rows the next batch mounts out of their port, and those that showed (LLP 1055 D13).
    shown: crate::list_api::RowsShown,
    /// Retiring list rows are rebound to new items ([`Runner::set_row_reuse`]).
    reuse: bool,
    /// The 2D canvases (LLP 1056 D4), when Canvas 2D is linked.
    canvases: Option<Box<dyn canvas2d::CanvasEngine>>,
    canvas_owner: std::sync::Arc<()>,
    canvas_captures: Vec<(
        crate::Request,
        std::sync::Weak<std::sync::Mutex<crate::canvas_capture::CaptureState>>,
    )>,
    /// Requests in flight (LLP 1016): at most one per resource or mutation.
    pending: Vec<PendingReq>,
    /// This commit let a request go: `conclude` tells the source what is still in flight.
    forgot: bool,
    /// The fetches the app aborted ([`DataSource::take_aborted`]) that name no
    /// request in flight yet (one a commit is still to hand out, a worker's
    /// turn still to land), by the module's ticket, with how many
    /// collections they have waited: matched as their request is enqueued.
    heard_aborts: Vec<(u64, u32)>,
    /// The requests whose fetch the app aborted, by ticket, until the host's
    /// next pump answers them aborted (`take_request_refusal`). Both lists are
    /// kept outside any commit's rollback: the module has already rejected
    /// an ordinary fetch, and an adopted one (LLP 1146 D2) waits, pending,
    /// for this answer to reject it in a module turn, so a refused aborting
    /// commit still delivers it on the next pump.
    module_aborts: Vec<u64>,
    failed_tickets: std::collections::BTreeMap<u64, String>,
    /// The tickets answered aborted, until the host cancels their requests
    /// whatever their method ([`Runner::take_explicit_aborts`]).
    explicit_aborts: Vec<u64>,
    /// Requests let go while still running, by module fetch ticket and
    /// runner ticket, until their reply ([`Runner::note_retired`]).
    retired_fetches: Vec<(u64, u64)>,
    /// The uploads asked and how each stands (LLP 1016.000 D10): none is
    /// asked again at its arguments.
    uploads: uploads::Uploads,
    /// The re-ask being committed, and the reply being landed (`overrun.rs`).
    overrun: overrun::Overrun,
    pending_mut: Vec<bool>,
    failed_mut: Vec<Option<crate::failure::Failure>>,
    /// Mutations whose answer landed in the commit being made; their `then` actions are armed once it stands (LLP 1016.001).
    landed: Vec<(usize, provenance::Cause)>,
    /// Sends made before the data source was ready, sent at `data_ready`
    /// (LLP 1027 D4): pending meanwhile, one per mutation but a parallel
    /// one, which keeps each.
    unsent: Vec<Unsent>,
    /// When each mutation's `then` action is due, as a one-shot timer: infinite until an answer lands.
    then_due: Vec<f64>,
    /// `queue` mutations' waiting sends, `next`s and stalls (LLP 1092).
    queues: queue::Queues,
    /// Each mutation row's instance: `None` for a declared row, a child's
    /// send for a row after them (LLP 1035.005.004 D6, `owned_sends.rs`).
    owned_sends: Vec<Option<owned_sends::OwnedSend>>,
    /// What this commit's sends wrote into instances, to put back if it is refused.
    send_undo: Vec<(
        std::rc::Weak<std::cell::RefCell<vm::Owned>>,
        u32,
        Option<Value>,
    )>,
    next_ticket: u64,
    completion_identity: std::cell::OnceCell<completion::Identity>,
    /// The module's background work's round, when one is out (LLP 1097 D5).
    background: background::Background,
    /// Module work's requests in flight (LLP 1146 D2).
    module_work: module_work::ModuleWork,
    mapper_turns: mapper_turns::MapperTurns,
    /// Files picked this run, for `app:/tmp/picked/` names (LLP 1069.002 D3).
    picked_count: u64,
    /// Second edges waiting for the first action's async targets to settle,
    /// and lists whose edge waits for their covered route to show (`collection.rs`).
    deferred_edges: Vec<(u32, Vec<Target>)>,
    held_edges: Vec<u32>,
    /// Requests for the host, since the last take.
    requests: Vec<RequestOut>,
    /// Socket operations of commits that stood, since the last take (D7).
    sockets: Vec<crate::SocketSend>,
    /// The host's live views of its sockets (D7) and listeners (LLP 1133.005).
    socket_reader: sockets::Readers,
    /// The host's live view of its long polls' backoff waits (D8), if it has one.
    retry_reader: Option<crate::RetryReader>,
    mutation_requests: u64,
    /// Resources an action asked to re-request; consumed by the next settle
    /// that can ask them (LLP 1054.000.000 D2).
    refresh_next: Vec<usize>,
    /// The `late` slots are initialized: a `late` resource settles from now
    /// on (LLP 1035.005.004 D2).
    late_ready: bool,
    /// The boot render's instances are making their requests (D8).
    owned_launch: bool,
    /// Resources whose ask in the commit being made is the launch's own,
    /// replacing a stand-in: nobody asked for fresh data, so the request is
    /// not sent as a refresh, and a reply fetched for this launch may answer
    /// it (LLP 1162.000 D1). Set for the boot's settlement, `data_ready`'s
    /// commit and the re-ask of a wait that carries it; empty otherwise.
    launch: Vec<usize>,
    /// Sends to mutations that declare `refreshes`, shown over the answers
    /// of the resources they change until they end or are answered.
    writes: writes::Writes,
    /// Resources asked again after a write ended, or asked at `data_ready`
    /// in place of a stand-in: a refusal of that ask marks the resource
    /// failed instead of refusing the commit.
    reconciling: Vec<usize>,
    /// Sends made before the source was ready that it refused when sent, by
    /// target and write: a refused commit does not bring them back.
    refused_unsent: Vec<(Target, Option<u64>)>,
    /// Durable client state (LLP 1018 D1): the host's snapshot, and the
    /// writes since for the host to persist.
    store: Store,
    persist_oversized: std::collections::BTreeMap<usize, String>,
    /// Topics the source's native module announced since the host last
    /// applied them, from any thread ([`Runner::listen`]).
    announced: std::sync::Arc<std::sync::Mutex<Vec<String>>>,
    /// The native slot for a source that has none of its own (a Rust source):
    /// the host installs the app module here too (LLP 1067.000 Q9).
    native: Native,
    /// Whether fresh answers of store-reading resources are kept for the
    /// next boot: only for a source that may not be ready at boot.
    keeping: kept::Keeping,
    /// Resources asked again and again with new arguments (a development build's warning).
    churn: query::Churn,
    /// The agent's preferred answers of native modules' sources (LLP 1124 D7).
    extensions: module_seam::ModuleState,
    /// Who each commit, request and armed `then` comes from, and the
    /// invocation each is part of (`provenance.rs`).
    origins: provenance::Origins,
    /// `@exact/child-process`'s children and the host's process work clock.
    processes: processes::Processes,
    /// Wakes and background actions, running and to come (LLP 1133.017 D5).
    wakes: wakes::Wakes,
    /// Tool calls, running and to come (LLP 1133.023 D7).
    calls: calls::Calls,
    /// Locked secrets: the host's backend and the calls on it.
    presence: seams::PresenceState,
    /// The host's target, once it said ([`Runner::set_module_target`]).
    module_target: Option<String>,
    /// The optional parts this runner links, for its plan.
    seams: ModuleLinks<D>,
    poisoned: bool,
    /// Evaluate every site on every update (the incremental update's reference; `set_full_evaluation`).
    full: bool,
    /// Row slots actions wrote, for the next update.
    row_writes: crate::instance::RowWrites,
    /// Journal lines an update produced, written once its batch applies.
    notes: Vec<String>,
    /// This commit's work outside the tree: rows the event's lookup compared, derives settled
    /// by evaluation, and the store's copied bytes when it began.
    lookup_rows: std::cell::Cell<usize>,
    derives_evaluated: usize,
    copied_at_checkpoint: usize,
    /// The inputs the published derives were computed against.
    settled: Option<settlement::Settled>,
    /// Per derive: its value depends on the durable store (bake provenance).
    derive_store_dependent: Vec<bool>,
    activities: activities::Activities,
    /// What this binary and its update store know about delivery (LLP 1030
    /// D4, D7): the embedded answer until a host says otherwise.
    delivery: crate::delivery::Delivery,
    // @ref LLP 1039 D2 — the layout size before settlement.
    viewport: crate::Viewport,
    viewport_geometry: crate::viewport::Geometry,
    // @ref LLP 1123 D6 — the geometry facts' answers and their bound.
    geometry: geometry_facts::Facts,
    // @ref LLP 1069.000 D2 — visibility, connectivity, the share sheet.
    platform: crate::page::Platform,
    page: crate::page::Page,
    presented: crate::page::Presented, // `exactPage().presented` (LLP 1069.000 D2)
    window_name: String,               // the page's `name` (LLP 1030.004 D2)
    // @ref LLP 1027.000.000 — the date, once the host says it.
    time: crate::time::WallTime,
    live_times: live_time::LiveTimes,
    text_axes: seams::TextAxesState,
    place: crate::time::Place,
    host_direction: Option<exact_kernel::Direction>, // the platform's, told at launch ([`Runner::direction`])
    surface_records: exact_kernel::SortedMap<String, String>,
    canvas_performance: exact_kernel::SortedMap<String, String>,
    surface_refusals: exact_kernel::SortedMap<String, String>,
    /// What the host links of the runner's own answers (LLP 1047 D3).
    links: RunnerLinks,
    router: Option<Box<dyn router::Routing>>,
    /// What happened, one line each, for the agent API's `logs`: the last
    /// [`JOURNAL_RING`] lines, and how many were dropped before them.
    journal: std::collections::VecDeque<String>,
    /// Where the app's `console` lines go (LLP 1027.009).
    console: console::Out,
    /// The driver's fetch faults (LLP 1103).
    faults: faults::Faults,
    /// Device requests held for the agent (LLP 1069.007 D3): not I/O.
    device_holds: Vec<device::Hold>,
    /// Replies landed for held requests, by target (LLP 1016.000 D9): `clock held`'s evidence.
    held_replies: Vec<(String, u64)>,
    /// The voice table (LLP 1096 D5), the transport (LLP 1125 D3) and the
    /// MIDI queue, behind the audio seam.
    audio: audio_seam::AudioState,
    /// The app's `setRootFontSize` over the host's size (LLP 1069.000 D3).
    root_font: root_font::RootFont,
    /// What a development host holds over the app's own, and what the app asked of it (LLP 1122 T7).
    held: held::Held,
    fonts: std::rc::Rc<std::cell::RefCell<crate::bridge::RuntimeFonts>>,
    /// Auth sessions (LLP 1069.006): live ones, and answers to deliver.
    auth: crate::auth::Sessions,
    /// The device capabilities linked (LLP 1047 D3): [`DeviceLinks`].
    device_links: DeviceLinks<D>,
    /// The view whose event is being dispatched, stamped on the commands
    /// its action issues (LLP 1069.003 D3).
    input_source: Option<ViewId>,
    journal_start: usize,
    journal_echoed: std::cell::Cell<usize>,
    flow_warned: exact_kernel::SortedSet<exact_kernel::NodeKey>,
    fragment_warned: exact_kernel::SortedSet<exact_kernel::NodeKey>,
    /// The lists already found conforming to their types, so a live answer
    /// is checked where it changed (LLP 1053 §0 G8).
    conformed: std::cell::RefCell<crate::conform::Conformed>,
}
/// How many journal lines the runner retains (about an hour of a one-second
/// timer); older ones are dropped, and `logs` reports where its window starts.
pub const JOURNAL_RING: usize = 4096;
/// Largest accepted clock value: JavaScript's exact integer domain in ms.
pub const MAX_CLOCK_MS: f64 = 9_007_199_254_740_991.0;

/// Maximum timer commits one call to [`Runner::advance_timed`] may perform.
pub const TIMER_FIRE_LIMIT: usize = 4096;

/// The seekable clock's virtual display: a frame every 1000/60 ms after a
/// frame task last fired (LLP 1073 D3).
pub const VIRTUAL_FRAME_MS: f64 = 1000.0 / 60.0;

impl<D: DataSource> Runner<D> {
    /// Boot: refuse a plan built against another kernel schema, evaluate
    /// initial state, settle resources (compiled data first, the source
    /// otherwise), realize the tree, and apply the first frame's ops.
    pub fn boot(
        plan: Plan,
        data: D,
        kernel: Kernel,
        viewport: crate::Viewport,
        launch: &str,
    ) -> Result<Runner<D>, RunnerError> {
        Runner::boot_inner(
            RunnerLinks::ALL,
            ModuleLinks::ALL,
            plan,
            data,
            kernel,
            Seed::Fresh,
            Vec::new(),
            Default::default(),
            viewport,
            launch,
        )
        .map(Runner::linking_every_device)
    }

    #[allow(clippy::too_many_arguments)] // the host boot facts
    fn boot_inner(
        links: RunnerLinks,
        seams: ModuleLinks<D>,
        plan: impl Into<crate::Program>,
        mut data: D,
        mut kernel: Kernel,
        seed: Seed<'_>,
        snapshot: Vec<(String, Vec<u8>)>,
        delivery: crate::delivery::Delivery,
        // @ref LLP 1039 D2 — the layout size before settlement.
        mut viewport: crate::Viewport,
        launch: &str,
    ) -> Result<Runner<D>, RunnerError> {
        let program = plan.into();
        let plan = &*program;
        if let Some(preferences) = crate::time::launch_preferences() {
            viewport.preferences = preferences;
        }
        viewport.device_pixel_ratio = kernel
            .reported_display_scale()
            .map(f64::from)
            .or(viewport.device_pixel_ratio);
        viewport.validate()?;
        // @ref LLP 1095 D9 — every platform colour the plan can show is known
        // before the host's first report, a branch not yet taken included.
        for text in &plan.strings {
            exact_kernel::style::roles::intern_literals(text);
        }
        let carried = match seed {
            Seed::Carried(carried) => Some(carried),
            _ => None,
        };
        let sequence = match seed {
            Seed::Carried(c) | Seed::Reboot(c) => Some(c),
            Seed::Checkpoint(_, sequence) => sequence,
            _ => None,
        };
        if let Some(c) = sequence {
            kernel.continue_epoch(c.epoch);
        }
        let now_ms = match seed {
            Seed::Fresh | Seed::Reboot(_) => crate::time::launch_clock(),
            Seed::At(now_ms) => now_ms,
            Seed::Carried(carried) => carried.now_ms,
            Seed::Checkpoint(checkpoint, _) => checkpoint.now_ms,
        };
        if !now_ms.is_finite() {
            return Err(RunnerError::NonFiniteClock);
        }
        if !(0.0..=MAX_CLOCK_MS).contains(&now_ms) {
            return Err(RunnerError::ClockOutOfRange);
        }
        // A plan is validated where it is made: `Plan::decode` and
        // `PlanBuilder::finish` (the compiler's and the bake's) both check
        // every cross-reference, so the runner takes it as it is.
        if plan.kernel_schema_digest != exact_kernel::SCHEMA_DIGEST {
            return Err(RunnerError::KernelSchemaMismatch {
                plan: plan.kernel_schema_digest,
                kernel: exact_kernel::SCHEMA_DIGEST,
            });
        }
        // The identity gate (LLP 1023 D5): a plan naming one app against a
        // data crate naming another is a poisoned boot — the seam's names
        // and shapes cannot be trusted to line up. Unnamed (empty, either
        // side) matches anything: fixtures and stand-ins stay bootable.
        if !plan.app_id.is_empty() && !data.app_id().is_empty() && plan.app_id != data.app_id() {
            return Err(RunnerError::AppMismatch {
                plan: plan.app_id.clone(),
                host: data.app_id().to_string(),
                fix: format!(
                    "the plan names {plan:?} and this binary's data source {host:?}. If the plan is this app's own \
                     (baked from its app.json), the data source is wrong: a Rust data crate's `fn app_id` (one copied \
                     from a sample keeps the sample's) or app.ts's `appId`, its own or one it re-exports from shared \
                     code, must say {plan:?}, or not be exported, to take app.json's. If the plan came from elsewhere (another app's dev server, \
                     update or link), the plan is the wrong one: change nothing in the app",
                    plan = plan.app_id,
                    host = data.app_id()
                ),
            });
        }
        let roots = plan
            .nodes
            .iter()
            .filter(|n| n.parent.is_none() && n.arm.is_none())
            .count()
            + plan
                .regions
                .iter()
                .filter(|r| r.parent.is_none() && r.arm.is_none())
                .count();
        if roots != 1 {
            return Err(RunnerError::NotOneRoot(roots));
        }
        if plan
            .regions
            .iter()
            .any(|r| r.parent.is_none() && r.arm.is_none())
        {
            return Err(RunnerError::RootRegion);
        }
        let same_logic = carried.is_none_or(|c| c.data_revision.as_deref() == data.revision());
        data.bind(plan);
        // A cold boot forgets the kept answers no resource seeds (`kept::obsolete`); an
        // app granting Health forgets them all, at every boot (LLP 1069.008.000 D7).
        let private = crate::device::keeps_no_answers(data.grants());
        let obsolete_kept = match carried {
            _ if private => kept::all(snapshot.iter().map(|(name, _)| name)),
            None => kept::obsolete(plan, snapshot.iter().map(|(name, _)| name)),
            Some(_) => Vec::new(),
        };
        let mut store = Store::new_linked(data.grants(), snapshot, links.grants);
        for name in &obsolete_kept {
            store.forget_kept(name);
        }
        if let Some(carried) = carried {
            // Forget incompatible seeds, not just their first use: a pending
            // replacement must not relabel an old answer on the next reload.
            for (name, _) in &carried.store {
                let Some(resource) = name.strip_prefix(Store::KEPT) else {
                    continue;
                };
                let compatible = same_logic && kept::compatible_on_reload(plan, carried, resource);
                let transient =
                    (plan.resources.iter()).any(|r| r.transient && plan.str(r.name) == resource);
                if private || !compatible || transient {
                    store.forget_kept(name);
                }
            }
        }
        let store_readers: Vec<bool> = plan
            .resources
            .iter()
            .map(|resource| {
                resource.reader
                    || exact_plan::runner_owned_source(plan.str(resource.source))
                    || (same_logic
                        && carried.is_some_and(|carried| {
                            let name = plan.str(resource.name);
                            let source = plan.str(resource.source);
                            carried
                                .store_readers
                                .iter()
                                .any(|(reader, origin)| reader == name && origin == source)
                        }))
            })
            .collect();
        // What each carried reader's last ask read (LLP 1069.005 D2): whether a store change asks it again.
        let read_contents: Vec<Option<bool>> = (plan.resources.iter())
            .map(|resource| {
                let (name, source) = (plan.str(resource.name), plan.str(resource.source));
                (carried.filter(|_| same_logic)?.read_contents.iter())
                    .find(|(reader, origin, _)| reader == name && origin == source)
                    .map(|(_, _, contents)| *contents)
            })
            .collect();
        let router = match links.router {
            Some(routing) => routing(plan)?,
            None => None,
        };
        let launched_place = crate::time::launch_place();
        let sites = program.sites(links.keyframes);
        let strings = program.strings();
        // Its own buffers before its role overrides, which forget the token
        // edits keyed on them: a dropped runner's edits may key the same addresses.
        let plan = program.into_plan();
        // Its own role overrides (LLP 1121 stage 5): bound while it boots, installed once it has.
        let roles = exact_kernel::style::roles::Roles::new(crate::bridge::role_overrides(&plan));
        let booting = exact_kernel::style::roles::bind(&roles);
        root_font::bold_text_facts(&mut kernel, &plan, viewport.preferences.bold_text); // LLP 1115.000 D12.3
        let viewport_geometry = crate::viewport::Geometry::of(&kernel);
        let seams = seams.for_plan(
            &plan,
            !store.presence_names().is_empty(),
            links.lists.is_some(),
        );
        // The instance tree's list table runs only beside the runner's half.
        let links = RunnerLinks {
            lists: links.lists.filter(|_| seams.lists.linked()),
            ..links
        };
        let mut runner = Runner {
            sites,
            strings,
            failed_mut: vec![None; plan.mutations.len()],
            audio: audio_seam::AudioState(seams.audio.state(&plan, &data)),
            page: crate::time::launch_page(&plan),
            plan,
            inspection_digest: std::cell::OnceCell::new(),
            split_panel_sites: std::cell::OnceCell::new(),
            panel_navigation: std::cell::OnceCell::new(),
            keeps_midi: std::cell::OnceCell::new(),
            action_binding_origin: std::rc::Rc::new(()),
            completion_identity: Default::default(),
            detached_focus: Vec::new(),
            data,
            kernel,
            slots: Vec::new(),
            derives: Vec::new(),
            resources: store_readers
                .into_iter()
                .zip(read_contents)
                .map(|(store_reader, contents)| resources::ResourceRow {
                    store_reader,
                    contents_reader: contents.unwrap_or(false),
                    reads_known: contents.is_some(),
                    ..Default::default()
                })
                .collect(),
            tree: None,
            bake_warnings: Vec::new(),
            reorder_owner: None,
            reorder_target: None,
            reorder_ops: Vec::new(),
            reorder_kept: Default::default(),
            design: None,
            live: Default::default(),
            forma_said: Vec::new(),
            design_moves: 0,
            roles: exact_kernel::style::roles::Installed::new(roles.clone()),
            reorder_session: None,
            dropping_from: None,
            ids: Ids::from_last(sequence.map_or(0, |c| c.last_view_id)),
            orientation: Default::default(),
            now_ms,
            timers: Vec::new(),
            clock: Default::default(),
            batch: 0,
            commands: Vec::new(),
            root_font: Default::default(),
            held: Default::default(),
            fonts: Default::default(),
            into_view: Vec::new(),
            scrolled: Default::default(),
            resized: Vec::new(),
            intersections: Vec::new(),
            into_view_refused: Default::default(),
            surfaces: Vec::new(),
            surface_colors: Default::default(),
            renewed: Vec::new(),
            shown: Default::default(),
            reuse: false,
            canvases: links.canvas.map(|engine| engine()),
            canvas_owner: std::sync::Arc::new(()),
            canvas_captures: Vec::new(),
            pending: Vec::new(),
            pending_mut: Vec::new(),
            announced: Default::default(),
            native: Native::default(),
            landed: Vec::new(),
            unsent: Vec::new(),
            then_due: Vec::new(),
            queues: Default::default(),
            owned_sends: Vec::new(),
            send_undo: Vec::new(),
            next_ticket: 1,
            background: Default::default(),
            module_work: Default::default(),
            mapper_turns: Default::default(),
            picked_count: carried.map_or(0, |c| c.picked_count),
            forgot: false,
            heard_aborts: Vec::new(),
            module_aborts: Vec::new(),
            failed_tickets: Default::default(),
            explicit_aborts: Vec::new(),
            retired_fetches: Vec::new(),
            uploads: Default::default(),
            overrun: Default::default(),
            deferred_edges: Vec::new(),
            held_edges: Vec::new(),
            requests: Vec::new(),
            sockets: Vec::new(),
            socket_reader: Default::default(),
            retry_reader: None,
            mutation_requests: 0,
            refresh_next: Vec::new(),
            late_ready: false,
            owned_launch: false,
            launch: Vec::new(),
            writes: writes::Writes::default(),
            reconciling: Vec::new(),
            refused_unsent: Vec::new(),
            store,
            persist_oversized: Default::default(),
            keeping: Default::default(),
            churn: Default::default(),
            activities: activities::Activities::default(),
            extensions: module_seam::ModuleState(seams.modules.state()),
            // Boot is the app's (LLP 1133.023 D7a), until `booted` below.
            origins: provenance::Origins::booting(),
            processes: Default::default(),
            wakes: Default::default(),
            calls: Default::default(),
            presence: seams::PresenceState(seams.presence.state()),
            module_target: None,
            seams,
            delivery,
            viewport,
            viewport_geometry,
            geometry: Default::default(),
            presented: crate::page::Presented::at_boot(),
            time: crate::time::launch_time(),
            live_times: Default::default(),
            text_axes: seams::TextAxesState(seams.text_axes.state()),
            place: launched_place.clone().unwrap_or_default(),
            platform: crate::page::Platform::at_boot(),
            window_name: crate::time::launch_window_name(),
            host_direction: crate::time::launch_direction(),
            surface_records: Default::default(),
            canvas_performance: Default::default(),
            surface_refusals: Default::default(),
            links,
            router,
            poisoned: false,
            full: false,
            row_writes: Default::default(),
            notes: Vec::new(),
            lookup_rows: Default::default(),
            derives_evaluated: 0,
            copied_at_checkpoint: 0,
            settled: None,
            derive_store_dependent: Vec::new(),
            generation: sequence.map_or(1, |c| {
                c.generation
                    .checked_add(1)
                    .expect("session generation exhausted")
            }),
            replacement: None,
            replacement_pending: None,
            journal: std::collections::VecDeque::new(),
            console: Default::default(),
            faults: faults::Faults::from_env().map_err(RunnerError::InvalidFaults)?,
            device_holds: Vec::new(),
            held_replies: Vec::new(),
            auth: Default::default(),
            device_links: DeviceLinks::CORE,
            input_source: None,
            journal_start: 0,
            journal_echoed: Default::default(),
            flow_warned: Default::default(),
            fragment_warned: Default::default(),
            conformed: Default::default(),
        };
        runner.seams.modules.launch_substitutes(&mut runner)?;
        runner.record_replacement(carried);
        runner.init_slots(carried, launch)?;
        runner.carry_modes(carried.map_or(&[], |c| &c.modes));
        runner.ask_kept_scheme();
        runner.derives = vec![None; runner.plan.derives.len()];
        // Resources: carry only the same named source and a value that still fits the declared shape; a carried value can refuse nothing.
        let carried_resources = (0..runner.plan.resources.len())
            .map(|i| {
                let name = runner.plan.str(runner.plan.resources[i].name);
                let source = runner.plan.str(runner.plan.resources[i].source);
                carried
                    .filter(|_| same_logic)
                    .and_then(|c| {
                        c.resources
                            .iter()
                            .find(|(n, s, _, _)| n == name && s == source)
                    })
                    .filter(|(_, _, _, value)| runner.check_shape(i, value).is_ok())
                    .map(|(_, _, args, value)| ResourceState {
                        inspection_generation: 0,
                        args: args.clone(),
                        value: crate::held::Held::new(value.clone()),
                        store_revision: runner.store.revision(),
                        placeholder: false,
                        kept_seed: false,
                        from_page: false,
                        answered_for: None,
                        asleep: false,
                        slept: false,
                        origin: 0,
                    })
            })
            .collect::<Vec<_>>();
        for (row, state) in runner.resources.iter_mut().zip(carried_resources) {
            row.state = state;
        }
        // @ref LLP 1016.000 D10 — the uploads asked, carried by resource,
        // source and arguments whatever the data module; a launch starts with none.
        runner.carry_uploads_in(carried.map_or(&[][..], |c| &c.uploads));
        // @ref LLP 1048.000 D6 — a document's answers seed their resources.
        // The device's own store is an input its render never had.
        let (seeded, mut again) = match seed {
            Seed::Checkpoint(checkpoint, _) => runner.seed_checkpoint(checkpoint),
            _ => (vec![false; runner.resources.len()], Vec::new()),
        };
        // A carried answer of a stream still open, as a checkpoint's.
        let streamed = |name: &str| carried.is_some_and(|c| c.streams.iter().any(|s| s == name));
        again.extend((0..runner.plan.resources.len()).filter(|&i| {
            runner.resources[i].state.is_some()
                && streamed(runner.plan.str(runner.plan.resources[i].name))
        }));
        // A data module's store readers when it is not ready (a TypeScript module before its host
        // loads it, LLP 1027 D4): the answer kept from the last launch seeds the first frame if its
        // identifying arguments still match and its value still fits; the compiled empty-store
        // placeholder is the fallback (settlement); either way the resource is asked again at
        // `data_ready`.
        let ready = runner.data.ready();
        runner.keeping.on = !private && (!ready || carried.is_some_and(|c| c.keeps_answers));
        runner.seed_at_boot(&seeded, ready, private);
        runner.carry_waiting_in(carried.filter(|_| same_logic), private);
        // A seeded answer asked again, an open stream's or, on a device with
        // its own store, a store reader's, shows until it answers: asked now
        // when its source can answer, else at `data_ready`.
        for i in again {
            runner.ask_at_launch(i);
        }
        runner.pending_mut = vec![false; runner.plan.mutations.len()];
        runner.then_due = vec![f64::INFINITY; runner.plan.mutations.len()];
        runner.queues = queue::Queues::new(runner.plan.mutations.len());
        runner.owned_sends = vec![None; runner.plan.mutations.len()];
        // A carried boot never takes compiled data: it was baked for the
        // initial state, and the carried state is not that.
        runner.settle(carried.is_none())?;
        let note = match seed {
            Seed::Checkpoint(checkpoint, _) => runner.end_checkpoint_seeding(checkpoint, &seeded),
            _ => String::new(),
        };
        runner.journal_kept_seeds();
        runner.init_late_slots(carried)?;
        runner.settle_late(carried.is_none())?;
        runner.timers = runner.boot_timers();
        runner.gate_step()?;
        runner.launch.clear();
        // No commit's end follows the boot's settlement: what it did here.
        runner.hear_forgotten();
        runner.keep_sent(false);
        let audio = runner.seams.audio;
        audio.transport_stood(&mut runner);
        let mut ids = std::mem::take(&mut runner.ids);
        ids.tabs.carried = carried.map(|c| c.panels.clone()).unwrap_or_default();
        let lists = runner.prepare_view_ids(&mut ids, carried)?;
        let (tree, ops, surfaces, notes, shown) = {
            let mut u = runner.instance_update(&mut ids);
            u.list_seeds = Some(&lists);
            u.discard = runner.kernel.is_detached();
            let tree = Tree::create(&mut u)?;
            (tree, u.ops, u.surfaces, u.notes, u.shown)
        };
        ids.finish_first_frame();
        (runner.notes, runner.shown) = (notes, shown);
        runner.ids = ids;
        runner.tree = Some(tree);
        let mut receipt = runner.apply(ops)?;
        // The instances the first frame made ask for what they own.
        runner.boot_owned(&mut receipt)?;
        runner.samples_stood();
        runner.publish_surfaces(surfaces);
        runner.pin_canvas_captures();
        let line = lines::boot(carried.is_some(), runner.kernel.live_count(), receipt.epoch);
        runner.log(line);
        runner.persist_boot();
        if let Some(spec) = carried.and_then(|c| c.faults.as_deref()) {
            runner.faults = faults::Faults::parse(spec).unwrap_or_default();
        }
        for (name, n) in carried
            .map(|c| c.forgot_waiting.as_slice())
            .unwrap_or_default()
        {
            runner.log(lines::forgot_waiting(*n, name));
        }
        if !note.is_empty() {
            runner.log(note);
        }
        runner.journal_build_time();
        // Grants that do not parse grant nothing: said once here, and in
        // each refusal (the store's, the host's).
        if let Some(why) = runner.store.unparsed() {
            let line = format!("{why}; nothing is granted");
            runner.log(line);
        }
        drop(booting);
        runner.roles.install();
        runner.origins.booted();
        Ok(runner)
    }

    /// The auth sessions this runner keeps (LLP 1069.006; [`crate::auth`]).
    pub fn auth_mut(&mut self) -> &mut crate::auth::Sessions {
        &mut self.auth
    }

    /// Append a line to the journal the agent API's `logs` reads, stamped
    /// with the clock. Hosts add their own lines here (an image loaded, a
    /// layout refusal) so one read sees everything in order.
    pub fn log(&mut self, line: impl Into<String>) {
        let line = line.into();
        // While measuring, a development host also writes it to its
        // platform's log (LLP 1143 open question 1).
        if !self.ids.work.sites.is_empty() {
            crate::perf::queue(&self.ids.work.journal, line.clone());
        }
        self.journal_only(line);
    }

    /// Append a line to the journal and nowhere else: an alert, which its
    /// own queue carries to the platform's log.
    pub(crate) fn journal_only(&mut self, line: String) {
        self.journal.push_back(lines::stamped(self.now_ms, &line));
        if self.journal.len() > JOURNAL_RING {
            self.journal.pop_front();
            self.journal_start += 1;
        }
    }

    /// Report unsupported auto-height flow once per leaf per boot.
    /// @ref LLP 1043.000 §4 stage 2 — M9 makes the deferred behavior visible.
    pub fn report_flow_skipped(&mut self, keys: &[exact_kernel::NodeKey]) {
        for &key in keys {
            if self.flow_warned.insert(key) {
                if let Some(node) = self.kernel.node_by_key(key) {
                    let why = node.flow_refusal().map_or("", |r| r.message());
                    self.log(format!(
                        "wrap-flow: text #{} has auto height and is not flowed: {why} (LLP 1043.000 §8)",
                        node.id
                    ));
                }
            }
        }
    }

    /// Journal an outcome. `was_poisoned` is the runner's state before the
    /// attempt: only the failure that poisons it is written as such.
    fn log_outcome(
        &mut self,
        what: &str,
        result: &Result<CommitReceipt, RunnerError>,
        was_poisoned: bool,
    ) {
        let line = match result {
            Ok(r) => lines::committed(
                what,
                r.epoch,
                r.created.len(),
                r.destroyed.len(),
                r.touched.len(),
            ),
            Err(e) if self.poisoned && !was_poisoned => {
                format!("{what} poisoned the runner: {e:?}")
            }
            Err(e) => format!("{what} refused: {e:?}"),
        };
        self.log_router_refusals();
        self.log(line);
    }

    /// The kernel, for layout and export.
    pub fn kernel(&self) -> &Kernel {
        &self.kernel
    }

    /// The kernel, mutably (a host lays out through it).
    pub fn kernel_mut(&mut self) -> &mut Kernel {
        &mut self.kernel
    }

    /// The data source.
    pub fn data(&mut self) -> &mut D {
        &mut self.data
    }

    /// Inspect data-source identity and readiness without executing app logic.
    pub fn data_ref(&self) -> &D {
        &self.data
    }

    /// The app's grants: the data module's, and the origins its declared
    /// sources carry (LLP 1162.001 D3). What a host endows its executor with.
    pub fn grants(&self) -> String {
        crate::grants::with_declared(self.data.grants(), &self.plan)
    }

    /// Current value of a slot by name.
    pub fn slot(&self, name: &str) -> Option<&Value> {
        self.plan
            .slots
            .iter()
            .position(|s| self.plan.str(s.name) == name && s.owner.is_none())
            .map(|i| &self.slots[i])
    }

    /// Current value of a derive by name.
    pub fn derive(&self, name: &str) -> Option<&Value> {
        self.plan
            .derives
            .iter()
            .position(|d| self.plan.str(d.name) == name)
            .and_then(|i| self.derives[i].as_ref())
    }

    /// The plan's `i`th slot and derive, for a walk of all of them (the
    /// agent's `state`): looking each up by name is quadratic in the slots.
    pub(crate) fn slot_at(&self, i: usize) -> Option<&Value> {
        self.slots.get(i)
    }

    pub(crate) fn derive_at(&self, i: usize) -> Option<&Value> {
        self.derives.get(i).and_then(Option::as_ref)
    }

    /// Each resource whose last request failed, by name, with why: what
    /// `failed(x)` reads, for the agent's `state.failed`.
    pub fn failed_resources(&self) -> Vec<(&str, &str)> {
        let failed = self
            .resources
            .iter()
            .take(self.plan.resources.len())
            .map(|row| (&row.failed_args, &row.failed_why))
            .enumerate();
        failed
            .filter(|(_, (args, _))| args.is_some())
            .map(|(i, (_, why))| {
                let name = self.plan.str(self.resource_row(i).name);
                (
                    name,
                    why.as_ref().map_or("it failed", |w| w.message.as_str()),
                )
            })
            .collect()
    }

    /// A resource's answer by name: what its source answered, without the writes laid over it.
    pub fn resource(&self, name: &str) -> Option<&Value> {
        self.resource_rows()
            .position(|r| self.plan.str(r.name) == name)
            .and_then(|i| {
                self.resources[i]
                    .state
                    .as_ref()
                    .map(|r| r.value.get(&self.plan))
            })
    }

    /// The clock, in milliseconds.
    pub fn now_ms(&self) -> f64 {
        self.now_ms
    }

    /// The plan site a view was instantiated from and its instance path (the `for` keys and
    /// active arms crossed) for `layout <node>` (LLP 1035.002 D6); `None` outside the tree.
    pub fn site_of(&self, view: ViewId) -> Option<(NodesId, Vec<InstanceStep>)> {
        self.tree.as_ref().and_then(|t| t.site(view))
    }

    /// The plan site a view was instantiated from, without its instance path.
    pub fn plan_site_of(&self, view: ViewId) -> Option<NodesId> {
        self.ids.site(view)
    }

    /// The event kinds a view handles, for a host that attaches listeners.
    pub fn handlers_of(&self, view: ViewId) -> Vec<EventKind> {
        let Some(node) = self
            .ids
            .site(view)
            .filter(|_| self.kernel.node(view).is_some())
        else {
            return Vec::new();
        };
        self.plan
            .node(node)
            .handlers
            .iter()
            .map(|h| self.plan.handler(h).event)
            .collect()
    }

    /// All live listener declarations (bulk host creation), from the views
    /// the runner created — no tree walk.
    pub fn handlers(&self) -> exact_kernel::SortedMap<ViewId, Vec<EventKind>> {
        self.ids
            .sites()
            .filter(|(view, node)| {
                self.plan.node(*node).handlers.len > 0 && self.kernel.node(*view).is_some()
            })
            .map(|(view, node)| {
                let handlers = self.plan.node(node).handlers;
                (
                    view,
                    handlers
                        .iter()
                        .map(|h| self.plan.handler(h).event)
                        .collect(),
                )
            })
            .collect()
    }

    /// The site owning `view` and the frames in force there, found along
    /// the kernel's parent chain.
    fn find(&self, view: ViewId) -> Option<(NodesId, Vec<Frame>)> {
        let mut scanned = 0;
        let found = self.tree.as_ref()?.find(
            view,
            |v| self.kernel.node(v).and_then(|n| n.parent),
            &mut scanned,
        );
        self.lookup_rows.set(scanned);
        found
    }

    /// The kernel roots.
    pub fn roots(&self) -> Vec<ViewId> {
        self.tree.as_ref().map(Tree::roots).unwrap_or_default()
    }

    /// Commands produced since the last take.
    /// Surface inputs that changed since the last take — a canvas node's
    /// arguments, evaluated — published only from commits that applied
    /// (LLP 1009 D2). The host hands them to the app's GPU module.
    pub fn take_surface_updates(&mut self) -> Vec<SurfaceUpdate> {
        std::mem::take(&mut self.surfaces)
    }

    /// The commands actions emitted since the last take, and the runner's
    /// own scroll to a fragment once its route's content has arrived.
    pub fn take_commands(&mut self) -> Vec<Command> {
        self.fragment_command();
        std::mem::take(&mut self.commands)
    }

    /// The store's writes since the last take, in order, for the host to
    /// persist (LLP 1018 D1) — only from commits that applied.
    pub fn take_store_writes(&mut self) -> Vec<StoreWrite> {
        let writes = self.store.take_writes();
        self.presence_writes(writes)
    }

    /// Deterministic instance work counters, separate from layout and host costs.
    pub fn last_instance_work(&self) -> crate::instance::InstanceWork {
        self.tree
            .as_ref()
            .map(|tree| tree.last_work)
            .unwrap_or_default()
    }

    /// Evaluate every site on every update instead of only those whose
    /// inputs changed: the reference an incremental update must equal
    /// (the differential test runs both). A runtime switch, not a feature.
    pub fn set_full_evaluation(&mut self, full: bool) {
        self.full = full;
    }

    /// Rebind a list row that leaves the window to the item the window
    /// needs next, instead of destroying it and building one (LLP 1078; off
    /// by default). The receipts name every view of a rebound row
    /// `renewed`: a host that turns this on resets what it keeps by view
    /// for each of them, as for a new one.
    pub fn set_row_reuse(&mut self, on: bool) {
        self.reuse = on;
    }

    /// Whether an update failed after the tree began to change (see
    /// [`RunnerError::Poisoned`]).
    pub fn is_poisoned(&self) -> bool {
        self.poisoned
    }
    fn env<'a>(&'a self, params: &'a [Value], frames: &'a [Frame]) -> Env<'a> {
        Env {
            plan: &self.plan,
            strings: &self.strings,
            router: self.router.as_deref(),
            lists: self.links.lists,
            format: self.links.format,
            geometry: None,
            slots: &self.slots,
            derives: &self.derives,
            resources: crate::vm::Column::rows(
                &self.resources[..self.plan.resources.len()],
                |row| &row.value,
            ),
            params,
            frames,
            now_ms: self.now_ms,
            pending_resources: crate::vm::Column::rows(
                &self.resources[..self.plan.resources.len()],
                |row| &row.pending,
            ),
            current_resources: crate::vm::Column::rows(
                &self.resources[..self.plan.resources.len()],
                |row| &row.current,
            ),
            failed_resources: crate::vm::Column::rows(
                &self.resources[..self.plan.resources.len()],
                |row| &row.failed_args,
            ),
            failed_why: crate::vm::Column::rows(
                &self.resources[..self.plan.resources.len()],
                |row| &row.failed_why,
            ),
            pending_mutations: &self.pending_mut,
            failed_mutations: &self.failed_mut,
            store_dependent_derives: &[],
            store_dependent_resources: (&[]).into(),
        }
    }
    fn eval(&self, code: Code, params: &[Value], frames: &[Frame]) -> Result<Value, RunnerError> {
        let env = self.env(params, frames);
        Ok(vm::eval(self.plan.code(code), &env, &[])?.value)
    }
}
#[cfg(test)]
mod held_tests;
#[cfg(test)]
mod stream_tests;
