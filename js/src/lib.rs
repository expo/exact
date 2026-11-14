//! The TypeScript data-source executor (LLP 1027).
//!
//! @ref LLP 1027 D1a (`fetch` over the host's ticket path) / D2 (marshaling
//! by the plan's shapes) / D3 (the executor) / D4 (after first pixel) / D10
//! (what the module can use)
//!
//! An app's `app.ts`, bundled and compiled to Hermes bytecode at bake, is a
//! [`Module`]: a [`DataSource`] like any Rust data crate, behind the same
//! seam, answering the same `resource` and `send` declarations. The lean
//! Hermes VM runs it — bytecode only; it cannot be handed source — through
//! about two hundred lines of C++ (`shim.cc`), and the plan's `sources` table
//! says what every argument and every answer looks like, so records cross as
//! objects keyed by their declared field names and nothing is guessed.
//!
//! The module's identity and grants are the bake's outputs beside the
//! bytecode, so a host reads them at boot without an engine; the engine is
//! created by [`Module::load`], which a host calls after its first pixel
//! (D4). Until then every answer is `Unavailable`, by name.
//!
//! **The seam, from the module's side (ABI 1).** `exact.abi` is `1`;
//! `exact.appId` and `exact.grants` are strings; `exact.answer(source, args,
//! store)` returns the answer's value — or a `Promise` of it, when it awaited
//! `fetch` — and throws for an error (an object with `kind` of
//! `UnknownSource`, `BadArguments`, or `Unavailable` and a `message`; any
//! other throw is `Unavailable`). In a running session, an answer's own
//! error becomes `Failed(Error, …)`: its resource fails or send ends unsent,
//! while the asking commit stands (LLP 1027.000 D3). Bake and replacement
//! validation still refuse it. `fetch(url, init)` is the web's, over the
//! host's ticket path: the module describes, the host runs under the grants,
//! the Promise resolves to a `Response` with `status`, `ok`, `headers`,
//! `text()`, `json()`, `arrayBuffer()`. Liveness is the module's, as a
//! browser's event loop has it (LLP 1027.003.000 §13, option 1; hn-reader
//! F7): an answer that awaits a promise another answer started (a fetch
//! memoized across answers, a queue behind another's storage) waits while
//! the module has work outstanding and is asked again when its chain moves (LLP 1162 D3);
//! one that waits with nothing outstanding is refused as pending on nothing.
//! A worker-placed source runs one turn to its end, so it still refuses one.
//! `store` is `{get, set, forget}` over the runner's [`Store`]: reads
//! counted, writes grant-checked, in Rust. `console` reaches the runner's
//! logs. Time and random seeds are ordinary source arguments (LLP
//! 1027.000): ambient Date/Math.random reads and Intl.DateTimeFormat
//! formatting without an explicit timestamp refuse, including at module
//! initialization and after await. Explicit-value Date construction and UTC
//! arithmetic remain available. An answer's own `setTimeout` (and
//! `AbortSignal.timeout`) is due on the runner's clock. Its continuation
//! wakes when due or when another turn changes its wait (LLP 1162 D3);
//! the other timers refuse (LLP 1027.000 D1). `scheduler.yield()`
//! ends an answer's turn: the host asks it again at once, and it goes on in
//! a new turn with a budget of its own (LLP 1141.001 D10).
//!
//! **Interrupts (LLP 1048.000 D10).** Another thread may stop a running call
//! through [`DataSource::interrupt`]'s handle: the bake compiles with async
//! break checks, so Hermes stops at the next loop iteration or call, and the
//! call is refused as `Unavailable`. The per-call budget is still measured
//! after a call returns; an interrupt is what ends one that doesn't.

#![deny(missing_docs)]

mod abort;
mod background;
mod compression;
mod crypto;
mod door;
mod engine;
mod module_work;
mod native;
mod native_storage;
mod paired;
#[cfg(all(test, exact_js_engine))]
mod phase_cost_tests;
mod pure;
mod reply;
mod resume;
mod retirement;
mod secrets;
mod socket;
mod source;
mod storage;
mod turn;
mod turns;
mod watch;
mod wire;

pub use engine::ENGINE_LINKED;
pub use exact_data::Placed;
pub use exact_js_value::{from_json, to_json, Shape};
pub use exact_runner::Placement;
pub use native::{json, Changed, Json, LaterHandler, NativeModule, NativeReply};
pub use native_storage::{NativeStorage, SqliteFile};
pub use paired::{Bytecode, Paired};
#[doc(hidden)]
pub use storage::CompressionHold;

use door::{c_string, host_door};
use engine::{Engine, HostFn};
use exact_plan::{Plan, Value};
use exact_runner::failure::FailureCode;
use exact_runner::{
    Answer, DataError, DataSource, Dispatch, InFlight, Interrupt, Outcome, Request, Store, Target,
    Work,
};
use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::Arc;
use std::time::Instant;
use turn::{TurnClock, TurnTime};
use watch::{Watch, Watched};
use wire::outcome_to_json;

type NativeFactory = fn(&str) -> Box<dyn NativeModule>;

/// The seam ABI this executor speaks. ABI 2 adds Canvas 2D's `draw` and
/// `surfaces` (LLP 1056 D1); a module without them still reports 1, and
/// this executor runs both.
pub const ABI: u32 = 2;

/// Whether a module's `exact.abi` is one this executor runs.
pub fn abi_supported(abi: &str) -> bool {
    matches!(abi, "1" | "2")
}
/// Authoritative Ibex2 storage declarations included by the TypeScript bake.
pub const STORAGE_TYPES: &str = ibex2::bindings::TYPESCRIPT;
/// The per-turn budget a module is held to, unless the host says otherwise: its own time, on the
/// calling thread's CPU clock where there is one, with a wall backstop of ten budgets (`turn.rs`).
pub const DEFAULT_BUDGET_MS: f64 = 100.0;
/// A module load longer than this is journalled: on an iPhone 17 Pro Max a 2 MB
/// catalog built at the top level took 314 ms, all of it after first pixel.
const SLOW_LOAD_MS: f64 = 50.0;
/// The runtime's heap ceiling, unless the host says otherwise.
pub const DEFAULT_MAX_HEAP: u32 = 64 << 20;

/// Bytecode the binary holds, aligned so Hermes reads it in place (the shim
/// copies a buffer that is not).
#[cfg(exact_js_engine)]
#[repr(C, align(16))]
struct Aligned<B: ?Sized>(B);

#[cfg(exact_js_engine)]
static PRELUDE_BYTES: &Aligned<[u8]> =
    &Aligned(*include_bytes!(concat!(env!("OUT_DIR"), "/prelude.hbc")));
#[cfg(exact_js_engine)]
const PRELUDE: &[u8] = &PRELUDE_BYTES.0;
#[cfg(not(exact_js_engine))]
const PRELUDE: &[u8] = &[];

/// Bytecode version compiled into this executor's prelude, available without
/// creating an engine. Zero means this build has no native executor.
pub const BYTECODE_VERSION: u32 = if PRELUDE.len() >= 12 {
    u32::from_le_bytes([PRELUDE[8], PRELUDE[9], PRELUDE[10], PRELUDE[11]])
} else {
    0
};

/// Receipt-bound archive identity of this executor's pinned lean engine.
/// The producer compares its selected Ibex bundle before compiling app bytecode.
/// The explicit refusing stub has no identity.
pub const ENGINE_INPUTS: Option<&str> = hermes_lean_sys::LEAN_ENGINE_DIGEST;

/// One source's signature, from the plan.
struct Sig {
    params: Vec<Shape>,
    result: Shape,
    rows: std::cell::RefCell<exact_js_value::Rows>, // its last reply's list elements, reused when repeated
}

/// A parked answer's key: the runner's target when it named one, then the
/// source and its arguments. The runner keeps one answer in flight per
/// target, so two targets asking one source with equal arguments are two
/// calls; a new call on the same key replaces the old one. Each send of a
/// parallel mutation is its own target (`Target::Sent`, LLP 1161.000 D5), so
/// two with equal arguments are two calls too.
type Key = (Option<Target>, String, Vec<u8>);

/// An answer that awaited a fetch: the prelude's call id, and the fetch
/// ticket the runner's request stands for.
struct Parked {
    call: u64,
    ticket: u64,
    more: Option<Request>,
    /// The fetch it awaits may change its server (`Request::has_effects`):
    /// let go, it runs to its end (LLP 1146 D2).
    effects: bool,
    work_taken: bool,
    ready: bool,
    due: Option<u64>,
    /// Asked again with nothing outstanding in the module: its last settle.
    last: bool,
}

/// An answer awaiting module work or a live-clock timer: a turn that changes
/// its wait can wake it before its deadline (LLP 1162 D3).
const WAITING: u64 = u64::MAX - 1;

/// The ticket of an answer that awaited `scheduler.yield()` (LLP 1141.001
/// D10): the host asks it again at once (`Dispatch::Again`), and its yields
/// resolve in that new turn, which has a budget of its own.
const YIELDED: u64 = u64::MAX - 2;

/// A timer at bake, on a worker, or already due (`Request::due_ms`). Future
/// timers on main use `WAITING` so another turn can change their wait (LLP 1162 D3).
const TIMER: u64 = u64::MAX - 3;

/// The prefix of a handler's name as the runner asks for it (LLP 1133.016 D9).
const TOOL: &str = "tool:";

/// Bake and worker timers stay with the runner until due. A live-clock wait
/// reaches the host now so another turn can wake it early (LLP 1162 D3).
fn timer_due(request: Request, ticket: u64, due: Option<u64>) -> Request {
    match due {
        Some(ms) if ticket == TIMER => request.due(ms),
        Some(ms) => request.wake(ms),
        None => request,
    }
}

/// The fetch an answer awaits, with the time its own timer comes due, if
/// it armed one: the runner resumes the answer then if no reply came first
/// (`Module::timer_first`).
fn fetch_timer(request: Request, due: Option<u64>) -> Request {
    match due {
        Some(ms) => request.wake(ms),
        None => request,
    }
}

/// A turn of an answer over its budget: the message names the repair.
fn over_budget(source: &str, why: &str) -> DataError {
    DataError::Overrun(format!(
        "`{source}` {why}; \
         a long loop can `await scheduler.yield()` every few thousand items to go on in a new turn; \
         a large download kept as a file, `fetch(url, {{exactBodyTo: \"app:/cache/…\"}})`, never enters the turn"
    ))
}

/// What the host door reaches during one call: the store the seam handed
/// `answer` or `parse` (none at bake — an empty store that refuses writes),
/// and the requests `fetch` recorded, by the prelude's ticket. Boxed for the
/// engine's lifetime; the shim holds a pointer to it.
#[derive(Default)]
struct HostState {
    secrets: secrets::Shared,
    store: Option<*mut Store>,
    requests: Vec<(u64, Request)>,
    /// The fetch tickets the app aborted after their requests were handed
    /// out (op 23), until `take_aborted` names their answers to the runner.
    aborted: Vec<u64>,
    /// Door ops answered as an older host's door answers them (tests).
    refused_ops: Vec<u32>,
    /// The socket operations the answer or mapper call being made handed
    /// over (op 14), by the prelude's ticket for each socket's stream (LLP 1016.000 D7).
    socket_ops: Vec<(u64, exact_runner::SocketOp)>,
    /// The socket operations a turn of an answer sent as it made them (op
    /// 17; the amendment of 2026-10-18), by the same ticket.
    answer_ops: Vec<(u64, exact_runner::SocketOp)>,
    /// Each open socket's buffered bytes at the start of this turn, by the
    /// prelude's ticket for its stream (op 15).
    socket_buffered: Vec<(u64, u64)>,
    /// The binary message being mapped (a socket's frame, a byte stream's
    /// bytes), which the prelude copies into an ArrayBuffer of its own
    /// through bytes door op 9 rather than decoding base64.
    binary: Vec<u8>,
    native: Option<Box<dyn NativeModule>>,
    /// The native module takes long calls off this thread (`native.later`).
    later: bool,
    /// The host's app module answers this source's long calls (LLP 1067.000
    /// Q6): `native` is available, and `later` goes to it.
    hosted: bool,
    /// The host's app module's `native.call`, when it answers one (D9).
    hosted_call: Option<exact_runner::NativeCall>,
    /// The canvas a draw in progress draws: its text engine and images
    /// (LLP 1056 D8, D9), for the recorder's `measureText` and `drawImage`.
    canvas: Option<exact_runner::exact_canvas::Env>,
    /// Under the agent, the repeatable stream `crypto` draws from instead
    /// of the OS (LLP 1069.005 D2b); this instance's, from its start.
    agent: Option<exact_data::crypto::AgentStream>,
    /// Its seed, until the journal has said the first draw is seeded.
    agent_unsaid: Option<u64>,
    /// `CryptoKey`s by handle, never on the heap (LLP 1069.005 D1b).
    keys: HashMap<u64, exact_data::crypto::EcKey>,
    next_key: u64,
    /// `DecompressionStream`s' and `CompressionStream`s' contexts by handle,
    /// from the keys' counter, and what the last write yielded
    /// (`compression.rs`).
    codecs: HashMap<u64, compression::Codec>,
    codec_out: Vec<u8>,
    /// `authCallback()`: this native build's, from the grants (LLP 1069.006).
    auth_callback: Option<String>,
    /// The engine has app storage: the host configured directories.
    storage: bool,
    /// The engine reaches the documents the person chose (`doc:`), with or
    /// without app storage: the grants name them (LLP 1069.010 D1).
    documents: bool,
    /// The document owner, when the grants keep documents (`doc.keep`, LLP
    /// 1069.010.000 D5): `storage.fs.recentDocuments()` reads its app's list.
    kept: Option<u64>,
    /// The bake's module ([`Module::inspect`]): storage refuses as `bake`.
    baking: bool,
    /// A replacement candidate must answer successfully before it can be installed.
    validating: bool,
    readonly_secrets: bool,
    /// The runtime's own journal lines since the last take (LLP 1097 D8).
    journal: Vec<String>,
    /// Delivering between answers (a background round, a let-go call's
    /// steps): there is no store, and storage is not refused as at bake.
    between_answers: bool,
    storage_changed: bool,
    /// How much of module work may yet settle a waiting answer: kept
    /// requests in flight and live answers' steps they left (op 21,
    /// `module_work.rs`, LLP 1146 D2).
    module_outstanding: usize,
}

/// A TypeScript data source: bytecode, its bake-time identity, and the
/// engine that runs it once loaded.
pub struct Module {
    bytecode: Bytecode,
    snapshot: bool,
    /// A snapshot's deadline, and the time it left when the snapshot was prepared.
    snapshot_deadline: Option<(Instant, std::time::Duration)>,
    app_id: String,
    grants: String,
    preloads: &'static [exact_runner::request::Preload],
    revision: std::sync::OnceLock<String>, // the bytecode's SHA-256, when first asked
    engine: Option<Watched>,
    retirement: retirement::Retirement,
    identity: exact_runner::ModuleIdentity,
    /// What an interrupt from another thread reaches; a built worker
    /// instance shares its template's.
    watch: Arc<Watch>,
    storage: Option<storage::Session>,
    directories: Option<storage::Directories>,
    host: Box<HostState>,
    native_factory: Option<NativeFactory>,
    /// Where the host sends `native.later` requests; shared with an instance
    /// built from this template, which fills it on its owner.
    native_slot: exact_runner::Native,
    /// The bound plan, kept so an owner thread can bind its own instance;
    /// none when placed on main (`keeps_plan`), whose engine is this one.
    plan: Option<Plan>,
    keeps_plan: bool,
    /// True before binding, then whether the entry reaches application code.
    reached: bool,
    sigs: HashMap<String, Sig>,
    parked: Vec<(Key, Parked)>,
    /// Module work's let-go calls (`module_work.rs`, LLP 1146 D2).
    work: module_work::Letting,
    /// Each dispatched waiter's flag, by its call: set when the call is let
    /// go, so a waiter whose outcome will be discarded takes no
    /// compression's right to write (LLP 1069.002 A1.5).
    retired: std::collections::HashMap<u64, std::sync::Arc<std::sync::atomic::AtomicBool>>,
    /// Stream answers (LLP 1016.000): each message is mapped by the call's
    /// `exactStream`, never resumed; forgetting the ticket ends the call.
    streams: Vec<(Key, Parked)>,
    /// Answers waiting on another answer's work whose dispatch was held.
    waiters: Vec<u64>,
    /// Overlays whose answer was encoded for the module's `exact.overlay`.
    overlays_encoded: u64,
    /// Whether the module had an `exact.overlay` when it loaded.
    has_overlay: bool,
    budget_ms: f64,
    turn_time: TurnTime,
    max_heap: u32,
    /// How long a storage step is waited for: a snapshot's deadline shortens it.
    storage_wait: std::time::Duration,
    /// Where a test holds compressions (`Module::hold_compressions`).
    compression_hold: Option<std::sync::Arc<storage::Gate>>,
    document_owner: u64,
    logs: Vec<String>,
    overruns: u32,
    /// The Canvas 2D roster the bake read (LLP 1056 D1), known before the
    /// engine loads.
    canvas_surfaces: Vec<(String, usize)>,
    /// The agent's launch seed, when the agent drives this process (LLP
    /// 1069.005 D2b); a built worker instance takes its template's.
    agent_seed: Option<u64>,
    /// Runs inline, on the runner's thread (LLP 1097 D6): storage an answer
    /// did not await moves to the background. An instance a worker owner
    /// builds runs one turn to its end, and keeps its answers waiting.
    main_thread: bool,
    worker_retired: Option<Arc<std::sync::atomic::AtomicBool>>,
    background: background::Background,
}

impl std::fmt::Debug for Module {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Module")
            .field("app_id", &self.app_id)
            .field("bytecode", &self.bytecode.len())
            .field("loaded", &self.engine.is_some())
            .field("sources", &self.sigs.len())
            .field("parked", &self.parked.len())
            .finish()
    }
}

/// One step of a call as the prelude reports it.
enum Step {
    Done(Result<Value, DataError>),
    /// `due`: when the timer a [`TIMER`] or [`WAITING`] wait is for is due, ms.
    Pending {
        call: u64,
        ticket: u64,
        due: Option<u64>,
    },
}

impl Module {
    /// A module, unloaded: `app_id` and `grants` are what the bake wrote
    /// beside the bytecode, cross-checked against the module's own exports
    /// at [`Module::load`]. Under the agent (`EXACT_AGENT=1`) its `crypto`
    /// draws the agent's repeatable stream (LLP 1069.005 D2b).
    pub fn new(
        bytecode: impl Into<Bytecode>,
        app_id: impl Into<String>,
        grants: impl Into<String>,
    ) -> Module {
        let grants = grants.into();
        let auth_callback = exact_runner::auth::carrier_callback(&grants, None);
        let module = Module {
            revision: std::sync::OnceLock::new(),
            bytecode: bytecode.into(),
            snapshot: false,
            snapshot_deadline: None,
            app_id: app_id.into(),
            grants,
            preloads: &[],
            engine: None,
            retirement: Default::default(),
            identity: Default::default(),
            watch: Arc::default(),
            storage: None,
            directories: None,
            host: Box::new(HostState {
                auth_callback,
                ..HostState::default()
            }),
            native_factory: None,
            native_slot: Default::default(),
            plan: None,
            keeps_plan: true,
            reached: true,
            sigs: HashMap::new(),
            parked: Vec::new(),
            work: Default::default(),
            retired: std::collections::HashMap::new(),
            streams: Vec::new(),
            waiters: Vec::new(),
            overlays_encoded: 0,
            has_overlay: true,
            budget_ms: DEFAULT_BUDGET_MS,
            turn_time: TurnTime::default(),
            max_heap: DEFAULT_MAX_HEAP,
            storage_wait: storage::WAIT,
            compression_hold: None,
            document_owner: 0,
            logs: Vec::new(),
            overruns: 0,
            canvas_surfaces: Vec::new(),
            agent_seed: None,
            main_thread: true,
            worker_retired: None,
            background: Default::default(),
        };
        module.with_agent_seed(exact_data::crypto::AgentStream::agent_seed())
    }

    /// Attach the binary's checked process-start requests (LLP 1162.000 D2).
    pub fn with_preloads(mut self, preloads: &'static [exact_runner::request::Preload]) -> Self {
        self.preloads = preloads;
        self
    }

    /// This module with `seed` as the agent's launch seed, or `None` for
    /// OS entropy whatever the environment says (LLP 1069.005 D2b): what
    /// [`Module::new`] reads from `EXACT_AGENT` and `EXACT_AGENT_SEED`,
    /// stated. The stream starts over.
    pub fn with_agent_seed(mut self, seed: Option<u64>) -> Module {
        self.agent_seed = seed;
        self.host.agent = seed.map(|s| exact_data::crypto::AgentStream::new(s, "typescript"));
        self.host.agent_unsaid = seed;
        self
    }

    /// This module's Canvas 2D roster, as the bake recorded it beside the
    /// bytecode (`module.rs`'s `CANVAS_SURFACES`, which a linked bake reads).
    pub fn with_canvas_surfaces<'a>(mut self, surfaces: impl AsRef<[(&'a str, usize)]>) -> Self {
        let surfaces = surfaces.as_ref();
        self.canvas_surfaces = surfaces.iter().map(|(n, a)| (n.to_string(), *a)).collect();
        self
    }

    /// The module's ABI, read by the bake for its receipt.
    pub fn abi(&mut self) -> Result<u32, String> {
        self.load()?;
        let engine = self.engine.as_ref().expect("loaded");
        engine
            .string("abi")?
            .parse()
            .map_err(|e| format!("exact-js: invalid ABI: {e}"))
    }

    /// The Canvas 2D roster: name and arity.
    pub fn canvas_roster(&self) -> &[(String, usize)] {
        &self.canvas_surfaces
    }

    /// Attach this app's separately linked native implementation. The factory
    /// runs only on activation with host-selected storage directories. A
    /// replacement receives a fresh instance; validation receives none.
    pub fn with_native(mut self, factory: fn(&str) -> Box<dyn NativeModule>) -> Self {
        self.native_factory = Some(factory);
        self
    }

    /// This module, placed (LLP 1027.002 D1): `Main` is this module as it
    /// is; `Worker` builds an instance of it on a host-owned thread at
    /// activation, from what this one knows, and this one stays as the
    /// template. The engine never crosses a thread.
    pub fn placed(mut self, placement: Placement) -> Placed<Module> {
        self.keeps_plan = placement == Placement::Worker;
        if !self.keeps_plan {
            self.plan = None;
        }
        Placed::built(self, placement, Module::build)
    }

    /// What an owner thread needs to build this module there (LLP 1027.002
    /// D2): its bytecode, identity, directories, plan and limits — all of it
    /// `Send`; the runtime is created, run and destroyed on the owner.
    fn build(
        template: &Module,
        retired: Arc<std::sync::atomic::AtomicBool>,
    ) -> exact_data::placed::Obtain<Module> {
        let bytecode = template.bytecode.clone();
        let app_id = template.app_id.clone();
        let grants = template.grants.clone();
        let preloads = template.preloads;
        let directories = template.directories.clone();
        let native_factory = template.native_factory;
        let plan = template.plan.as_ref().map(Plan::encode);
        let budget_ms = template.budget_ms;
        let turn_time = template.turn_time.fresh();
        let max_heap = template.max_heap;
        let document_owner = template.document_owner;
        let agent_seed = template.agent_seed;
        let identity = template.identity.clone();
        let watch = template.watch.clone();
        let native_slot = template.native_slot.clone();
        let secrets = template.host.secrets.clone();
        Box::new(move || {
            let mut module = Module::new(bytecode, app_id, grants)
                .with_agent_seed(agent_seed)
                .with_preloads(preloads);
            // Before `activate`, which loads: a worker's answers wait for
            // their storage (LLP 1097 D6).
            module.main_thread = false;
            module.worker_retired = Some(retired);
            // The template's interrupt reaches the instance on its owner, and
            // its native handle finds the instance's long-call handler.
            module.watch = watch;
            module.identity = identity;
            module.host.secrets = secrets;
            module.native_slot = native_slot;
            if let Some(factory) = native_factory {
                module = module.with_native(factory);
            }
            module.set_budget_ms(budget_ms);
            module.turn_time = turn_time;
            module.set_max_heap(max_heap);
            module.set_document_owner(document_owner);
            if let Some(paths) = directories {
                module.configure_storage(paths.data, paths.cache, paths.temporary)?;
            }
            if let Some(bytes) = plan {
                let plan = Plan::decode(&bytes).map_err(|e| {
                    DataError::Unavailable(format!("the plan did not cross to the owner: {e:?}"))
                })?;
                module.bind(&plan);
            }
            module.activate()?;
            Ok(module)
        })
    }

    /// A module loaded at once — the bake's and a test's shape; a host loads
    /// after its first pixel instead (LLP 1027 D4).
    pub fn loaded(
        bytecode: Vec<u8>,
        app_id: impl Into<String>,
        grants: impl Into<String>,
    ) -> Result<Module, String> {
        let mut m = Module::new(bytecode, app_id, grants);
        m.load()?;
        Ok(m)
    }

    /// Create the runtime, evaluate the prelude and the bytecode, and check
    /// that the module speaks this ABI and is the app the bake said it is. Idempotent.
    pub fn load(&mut self) -> Result<(), String> {
        if self.engine.is_some() {
            return Ok(());
        }
        let started = Instant::now();
        let engine = self.load_engine().map_err(|error| {
            if self.watch.take() {
                "exact-js: the module was interrupted while it loaded".to_string()
            } else {
                error
            }
        })?;
        let app_id = engine.string("appId")?;
        if app_id != self.app_id {
            return Err(format!(
                "exact-js: the module says it is `{app_id}`; the bake said `{}`",
                self.app_id
            ));
        }
        if engine.string("grants")?.trim() != self.grants.trim() {
            return Err("exact-js: the module's grants differ from what the bake recorded".into());
        }
        self.install(engine)?;
        let took_ms = started.elapsed().as_secs_f64() * 1e3;
        if took_ms > SLOW_LOAD_MS {
            self.logs.push(format!(
                "the module's top level took {took_ms:.0} ms to evaluate, and every answer waits for it after first pixel: compute what a source needs in that source, or once on its first call"
            ));
        }
        Ok(())
    }

    /// Build-time inspection: evaluate bytecode in a private engine and read
    /// its identity/grants. Hosts must use `new`/`loaded` with admitted metadata
    /// instead; discovering a grant does not authorize it on a device.
    pub fn inspect(bytecode: Vec<u8>) -> Result<Module, String> {
        // The bake's module never draws the agent's stream (LLP 1069.005 D2b).
        let mut module = Self::new(bytecode, "", "").with_agent_seed(None);
        module.host.baking = true;
        let engine = module.load_engine()?;
        module.app_id = engine.string("appId")?;
        module.grants = engine.string("grants")?;
        // The Canvas 2D roster (LLP 1056 D1): `{name: arity}`, read at build.
        let roster = engine.string("surfacesJson")?;
        if !roster.is_empty() {
            let json: Json = serde_json::from_str(&roster)
                .map_err(|e| format!("exact-js: app.ts `surfaces`: {e}"))?;
            let object = json
                .as_object()
                .ok_or("exact-js: app.ts `surfaces` must map names to arities")?;
            for (name, arity) in object {
                let arity = arity
                    .as_u64()
                    .ok_or_else(|| format!("exact-js: surface `{name}`'s arity is not a count"))?;
                module.canvas_surfaces.push((name.clone(), arity as usize));
            }
        }
        if module.app_id.is_empty() {
            return Err("exact-js: the module exports no appId".into());
        }
        // A grant a device would refuse refuses the build instead: a native
        // host that cannot parse the grants holds none of them.
        ibex2::grant::GrantSet::parse(&exact_runner::io_grants(&module.grants))
            .map_err(|e| format!("exact-js: app.ts `grants`: {e}"))?;
        module.install(engine)?;
        Ok(module)
    }

    /// `load`'s and `inspect`'s last step: the overlay is decided once, here, alike (a failed read keeps it asked).
    fn install(&mut self, mut engine: Watched) -> Result<(), String> {
        let overlay = engine.call("__exact_has_overlay", ["", "", ""]);
        if engine.out_of_memory() {
            return Err(overlay.expect_err("terminal OOM fails the call"));
        }
        self.has_overlay = overlay.map_or(true, |t| !t.is_empty());
        self.engine = Some(engine);
        Ok(())
    }

    fn load_engine(&mut self) -> Result<Watched, String> {
        self.load_engine_with_prelude(PRELUDE)
    }

    fn load_engine_with_prelude(&mut self, prelude: &'static [u8]) -> Result<Watched, String> {
        self.host.readonly_secrets = self.snapshot;
        let io = exact_runner::io_grants(&self.grants);
        // Stage 1 permits file reads, not file/SQLite writes or document handles.
        // Identity still checks the full baked grants; only endowment narrows.
        let io = if self.snapshot {
            io.lines()
                .filter(|line| line.starts_with("fs.read app:/"))
                .collect::<Vec<_>>()
                .join("\n")
        } else {
            io
        };
        self.host.documents = storage::reaches_documents(&io);
        let keeps = !self.snapshot && self.grants.lines().any(|line| line.trim() == "doc.keep");
        self.host.kept = keeps.then_some(self.document_owner);
        // The bindings Context must precede the engine and outlive its
        // adapter. Module declares `engine` before `storage`, and unload takes
        // the engine first, preserving that order on every path.
        self.storage = Some(storage::Session::open(
            &io,
            self.storage_wait,
            self.document_owner,
            self.compression_hold.clone(),
        )?);
        self.storage.as_mut().unwrap().worker_retired = self.worker_retired.clone();
        let ctx = &mut *self.host as *mut HostState as *mut c_void;
        let host: HostFn = host_door;
        let bytes: engine::BytesFn = crypto::bytes_door;
        let engine = Engine::new(
            self.max_heap,
            host,
            bytes,
            ctx,
            &self.storage.as_ref().expect("Context was created").context,
        )
        .map_err(|e| format!("exact-js: {e}"))?;
        // Reachable before anything runs in it: module initialization is
        // application code too.
        let mut engine = Watched::new(engine, self.watch.clone());
        engine
            .load_static(prelude)
            .map_err(|e| format!("exact-js: the prelude did not load: {e}"))?;
        if self.main_thread {
            engine
                .call("__exact_main_thread", ["", "", ""])
                .map_err(|e| format!("exact-js: the prelude did not load: {e}"))?;
        }
        self.host.storage = self.directories.is_some();
        if let Some(paths) = &self.directories {
            if let Some(factory) = self.native_factory {
                let mut native = factory(&self.grants);
                let roots = [&paths.data, &paths.cache, &paths.temporary].map(Clone::clone);
                native.configure_storage(NativeStorage::new(roots, &io)?)?;
                let slot = self.native_slot.clone();
                native.changes(Arc::new(move |topic: &str| slot.changed(topic)));
                let later = native.later();
                self.host.later = later.is_some();
                self.native_slot
                    .set(later.map(|handler| -> exact_runner::NativeHandler {
                        std::sync::Arc::new(move |body: Vec<u8>, reply| {
                            let reply = NativeReply::new(reply);
                            match exact_js_value::json::parse(&body) {
                                Ok(request) => handler(request.into(), reply),
                                Err(e) => reply.send(Err(format!("native.later: {e}"))),
                            }
                        })
                    }));
                self.host.native = Some(native);
            }
            self.storage
                .as_ref()
                .expect("Context was created")
                .configure(paths, self.snapshot)?;
        }
        if self.directories.is_some() || (self.host.documents && !self.host.baking) {
            engine.install_storage()?;
        }
        // The host's app module, when the app links no native module of its
        // own: present in agent mode too, where it substitutes its input
        // (LLP 1067.000 Q7), so it needs no storage directories.
        if self.host.native.is_none() && self.native_slot.hosted() {
            self.host.hosted = true;
            self.host.later = true;
            self.host.hosted_call = self.native_slot.hosted_call();
        }
        engine
            .harden()
            .map_err(|e| format!("exact-js: hardening failed: {e}"))?;
        match &self.bytecode {
            Bytecode::Linked(bytes) => engine.load_static(bytes),
            Bytecode::Owned(bytes) => engine.load_shared(bytes.clone()),
        }
        .map_err(|e| format!("exact-js: the module did not load: {e}"))?;
        let abi = engine.string("abi")?;
        if !abi_supported(&abi) {
            return Err(format!(
                "exact-js: the module speaks ABI {abi:?}; this executor speaks {ABI}"
            ));
        }
        Ok(engine)
    }

    /// Drop the runtime; answers are `Unavailable` until the next
    /// [`Module::load`], and every answer in flight is forgotten. Already-started
    /// external effects may finish; unloading waits only for the module's
    /// own storage, a second at most.
    pub fn unload(&mut self) {
        // A dev restart or a reload replaces the module: what it started
        // and did not await is finished first, within a second (LLP 1097
        // D10). A retired worker drops its queued work without delivering it.
        if !self.snapshot
            && !self
                .worker_retired
                .as_ref()
                .is_some_and(|r| r.load(std::sync::atomic::Ordering::Acquire))
        {
            self.finish_background(std::time::Duration::from_secs(1));
        }
        self.drop_runtime();
        self.retirement = Default::default();
    }

    fn drop_runtime(&mut self) {
        if let Some(mut engine) = self.engine.take() {
            self.logs.extend(engine.take_log());
        }
        self.host.keys.clear();
        self.host.codecs.clear();
        self.host.codec_out.clear();
        self.host.binary.clear();
        self.host.socket_ops.clear();
        self.host.answer_ops.clear();
        self.host.socket_buffered.clear();
        self.host.store = None;
        self.host.canvas = None;
        self.host.between_answers = false;
        self.streams.clear();
        self.waiters.clear();
        for (_, retired) in self.retired.drain() {
            retired.store(true, std::sync::atomic::Ordering::Release);
        }
        self.host.secrets.clear();
        self.storage = None;
        self.host.native = None;
        self.host.later = false;
        self.host.hosted = false;
        self.host.hosted_call = None;
        self.native_slot.set(None);
        self.parked.clear();
        self.work = Default::default();
        self.host.requests.clear();
        self.host.aborted.clear();
        self.background = Default::default();
        // Module work ends with the engine (LLP 1146 D2).
        self.host.module_outstanding = 0;
    }

    /// Whether an engine is up.
    pub fn is_loaded(&self) -> bool {
        self.engine.is_some()
    }

    /// How many overlays were encoded for the module's `exact.overlay` (none without one).
    pub fn overlays_encoded(&self) -> u64 {
        self.overlays_encoded
    }

    /// The per-turn budget in milliseconds (its own time, `turn.rs`); a turn
    /// over it is `DataError::Overrun`, and a resource keeps its last value
    /// while the runner asks it again (`runner/src/runner/overrun.rs`).
    pub fn set_budget_ms(&mut self, ms: f64) {
        self.budget_ms = ms;
    }

    /// Native storage operations started and not yet returned: a test of
    /// giving up waits for the work it gave up on to end before reading
    /// what that work did not write (LLP 1069.002 A1.5).
    #[doc(hidden)]
    pub fn storage_in_flight(&self) -> usize {
        self.storage.as_ref().map_or(0, |s| s.context.in_flight())
    }

    /// The heap ceiling for the next [`Module::load`].
    pub fn set_max_heap(&mut self, bytes: u32) {
        self.max_heap = bytes;
    }

    /// Calls that ran over the budget so far.
    pub fn overruns(&self) -> u32 {
        self.overruns
    }

    /// The module's `console` lines since the last take — for the runner's
    /// `logs` (LLP 1012).
    pub fn take_logs(&mut self) -> Vec<String> {
        if let Some(engine) = self.engine.as_mut() {
            self.logs.extend(engine.take_log());
        }
        std::mem::take(&mut self.logs)
    }

    /// The source names the bound plan declares, in no particular order.
    pub fn sources(&self) -> Vec<&str> {
        self.sigs.keys().map(String::as_str).collect()
    }

    /// Answers awaiting a fetch the host has yet to fulfil.
    pub fn in_flight(&self) -> usize {
        self.parked
            .iter()
            .chain(&self.streams)
            .map(|(_, p)| p.call)
            .collect::<std::collections::HashSet<_>>()
            .len()
    }

    /// How many calls the prelude still tracks, parked or not: a diagnostic
    /// (a call whose answer failed for good is unlinked from it, LLP 1041
    /// §8.4 Q5). `None` when the module is not loaded.
    pub fn calls_open(&mut self) -> Option<usize> {
        let engine = self.engine.as_mut()?;
        let count = engine.call("__exact_calls_open", ["", "", ""]);
        self.retire_oom();
        count.ok()?.parse().ok()
    }

    fn take_request(&mut self, ticket: u64) -> Option<Request> {
        let pos = self.host.requests.iter().position(|(t, _)| *t == ticket)?;
        Some(self.host.requests.remove(pos).1)
    }

    fn key(target: Option<Target>, source: &str, args: &[Value]) -> Key {
        let mut bytes = Vec::new();
        for a in args {
            bytes.extend(a.to_bytes());
        }
        (target, source.to_string(), bytes)
    }

    /// Begin an answer: marshal, call, drain, settle.
    fn begin(
        &mut self,
        store: Option<&mut Store>,
        target: Option<Target>,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        self.reload_after_oom()?;
        if self.engine.is_none() {
            return Err(DataError::Unavailable(
                "exact-js: the engine is not loaded".into(),
            ));
        }
        let refuses = self.host.baking || self.host.validating;
        let session = store.is_some() && !refuses;
        // No answer waits for another to begin (LLP 1097 D4.5): answers
        // interleave at their awaits, as two async calls do on the web, and
        // storage keeps the order it was issued in (the prelude's queue).
        let Some(sig) = self.sigs.get(source) else {
            return Err(DataError::UnknownSource(source.to_string()));
        };
        if args.len() != sig.params.len() {
            return Err(DataError::BadArguments(format!(
                "expected {} arguments, got {}",
                sig.params.len(),
                args.len()
            )));
        }
        let mut json_args = Vec::with_capacity(args.len());
        for (i, (arg, shape)) in args.iter().zip(&sig.params).enumerate() {
            json_args.push(
                to_json(arg, shape)
                    .map_err(|_| DataError::BadArguments(format!("argument {i}")))?,
            );
        }
        let args_text = Json::Array(json_args).to_string();
        let mut store = store;
        self.host.store = store.as_mut().map(|s| &mut **s as *mut Store);
        self.host.socket_ops.clear();
        self.host.answer_ops.clear();
        if let Some(store) = store.as_deref() {
            self.host.socket_buffered = socket::snapshot(&self.streams, store);
        }
        // A mutation's synchronous source may send on a socket (D7), and so
        // may a resource's answer and a mutation's after its source (the
        // amendment of 2026-10-18); a call for no target sends nothing.
        let mutation = match target {
            Some(Target::Mutation(_) | Target::Sent(..)) if session => "mutation",
            Some(Target::Resource(_)) if session => "resource",
            None if session && source.starts_with(TOOL) => "tool",
            _ => "",
        };
        let started = Instant::now();
        let budget_ms = self.turn_budget_ms(started);
        // (its own time on this thread's CPU clock, a wall backstop beside it: turn.rs)
        let clock = TurnClock::start(&self.turn_time);
        let mut begun = None;
        let result: Result<Step, DataError> = (|| {
            let engine = self.engine.as_mut().expect("checked above");
            let text = engine
                .call("__exact_call", [source, &args_text, mutation])
                .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
            let mut reply = Module::decode_reply(sig, engine, source, &text)?;
            if reply.fields.get("tag").and_then(Json::as_u64) == Some(3) {
                let call = reply
                    .fields
                    .get("call")
                    .and_then(Json::as_u64)
                    .ok_or_else(|| {
                        DataError::Unavailable(format!("`{source}`: a call with no id"))
                    })?;
                begun = Some(call);
                engine
                    .drain()
                    .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
                let text = Module::settle_reply(engine, &call.to_string(), "")
                    .map_err(|e| DataError::Unavailable(format!("`{source}` threw: {e}")))?;
                reply = Module::decode_reply(sig, engine, source, &text)?;
            }
            Ok(Module::step(sig, engine, source, reply, self.host.baking))
        })();
        self.host.store = None;
        if let Some(error) = self.retire_oom() {
            return Err(error);
        }
        // An interrupt may stop __exact_call before it returns its id.
        if result.is_err() && begun.is_none() {
            begun = self
                .engine
                .as_mut()
                .expect("checked above")
                .call("__exact_call_id", ["", "", ""])
                .ok()
                .and_then(|id| id.parse().ok())
                .filter(|id| *id != 0);
        }
        let deadline = self.snapshot_deadline.is_some();
        let over = clock.judge(
            budget_ms,
            deadline,
            self.main_thread,
            source,
            &mut self.logs,
        );
        if result.is_err() || over.is_some() {
            self.engine.as_mut().expect("checked above").clear_reply();
            self.forget_calls(begun.into_iter().collect());
            self.prune_secret_calls();
        }
        // What the turn sent as an answer goes whatever it answers (the
        // amendment of 2026-10-18); a mutation's staged socket operations
        // ride only an answer given now (D7).
        let answer_ops = std::mem::take(&mut self.host.answer_ops);
        if let (Some(store), false) = (store.as_deref_mut(), answer_ops.is_empty()) {
            self.stage_answer(store, source, answer_ops);
        }
        let socket_ops = std::mem::take(&mut self.host.socket_ops);
        let result = match (result, store) {
            (Ok(Step::Done(Ok(v))), Some(store)) if !socket_ops.is_empty() => {
                match self.stage_sockets(store, socket_ops) {
                    Ok(()) => Ok(Step::Done(Ok(v))),
                    Err(why) => Ok(Step::Done(Err(DataError::Failed(
                        FailureCode::Error,
                        why,
                        String::new(),
                    )))),
                }
            }
            (result, _) => result,
        };
        if result.is_err() && self.watch.take() {
            // After the store is cleared, so a let-go step is not this
            // answer's write (LLP 1097; a934686a0).
            self.finish_let_go();
            return Err(DataError::Unavailable(format!(
                "`{source}` was interrupted"
            )));
        }
        if let Some(why) = over {
            self.overruns += 1;
            self.finish_let_go();
            return Err(over_budget(source, &why));
        }
        let answer = match result {
            Err(e) => Err(e),
            // The web learns a rejection after the input commits. Native
            // answers fail the same target (LLP 1027.000 D3).
            Ok(Step::Done(r)) => r.map(Answer::Now).map_err(|e| match e {
                DataError::Unavailable(m) | DataError::BadArguments(m) if session => {
                    DataError::Failed(FailureCode::Error, m, String::new())
                }
                DataError::Failed(_, m, _) if refuses => DataError::Unavailable(m),
                e => e,
            }),
            Ok(Step::Pending { call, ticket, due }) => {
                let request = if matches!(ticket, 0 | WAITING | YIELDED | TIMER | secrets::TICKET) {
                    timer_due(Request::continuation(call), ticket, due)
                } else {
                    let Some(mut request) = self.take_request(ticket) else {
                        self.finish_let_go();
                        return Err(DataError::Unavailable(format!(
                            "`{source}` awaits a fetch it never made"
                        )));
                    };
                    // What an app's abort names it by (`take_aborted`).
                    request.fetch = Some(ticket);
                    fetch_timer(request, due)
                };
                let key = Module::key(target, source, args);
                // Tool invocations are independent even with equal arguments.
                // A targeted continuation also stays until `forgotten` or
                // `discard` identifies which call the runner kept after commit.
                let replaced = if !source.starts_with(TOOL)
                    && (target.is_none() || request.continuation.is_none())
                {
                    self.replace_on(&key)
                } else {
                    Vec::new()
                };
                self.park(key.clone(), call, ticket, &request)?;
                self.park_more(&key, call)?;
                self.forget_calls(replaced);
                Ok(Answer::Later(request))
            }
        };
        // A let-go call's operation may now be the one in flight. Deliver it
        // with no store, so its file writes land and its Store writes do not
        // (LLP 1097; a934686a0). Not while this answer's store is installed.
        self.finish_let_go();
        self.after_oom(answer)
    }

    /// The module's `overlay` for `source(args)`: `answer` with `writes`
    /// laid over it, called synchronously with no answer current and every effect
    /// refused. `None` when the module exports none or it returns `undefined`.
    fn overlay(
        &mut self,
        source: &str,
        args: &[Value],
        answer: &Value,
        writes: &[exact_runner::Write<'_>],
    ) -> Result<Option<exact_runner::Overlaid>, DataError> {
        if self.engine.is_none() {
            return Ok(None);
        }
        let Some(sig) = self.sigs.get(source) else {
            return Err(DataError::UnknownSource(source.to_string()));
        };
        // A module loaded with no `exact.overlay` shows the answer (`{tag:4}`): nothing is encoded for it.
        if !self.has_overlay {
            return Ok(None);
        }
        self.overlays_encoded += 1;
        let json = |value: &Value, shape: &Shape| {
            to_json(value, shape)
                .map_err(|_| DataError::BadArguments(format!("`{source}`'s overlay input")))
        };
        let args_json = Json::Array(
            args.iter()
                .zip(&sig.params)
                .map(|(a, shape)| json(a, shape))
                .collect::<Result<_, _>>()?,
        );
        let mut list = Vec::with_capacity(writes.len());
        for write in writes {
            let Some(wsig) = self.sigs.get(write.source) else {
                return Err(DataError::UnknownSource(write.source.to_string()));
            };
            let mut entry = serde_json::Map::new();
            entry.insert("id".into(), Json::from(write.id));
            entry.insert("mutation".into(), Json::from(write.mutation));
            entry.insert("source".into(), Json::from(write.source));
            entry.insert("answered".into(), Json::from(write.answered));
            entry.insert(
                "args".into(),
                Json::Array(
                    write
                        .args
                        .iter()
                        .zip(&wsig.params)
                        .map(|(a, shape)| json(a, shape))
                        .collect::<Result<_, _>>()?,
                ),
            );
            if let Some(reply) = write.reply {
                entry.insert("reply".into(), json(reply, &wsig.result)?);
            }
            list.push(Json::Object(entry));
        }
        let payload = serde_json::json!({ "answer": json(answer, &sig.result)?, "writes": list });
        let started = Instant::now();
        let budget_ms = self.turn_budget_ms(started);
        // (its own time on this thread's CPU clock, a wall backstop beside it: turn.rs)
        let clock = TurnClock::start(&self.turn_time);
        let engine = self.engine.as_mut().expect("checked above");
        let text = engine
            .call(
                "__exact_overlay",
                [source, &args_json.to_string(), &payload.to_string()],
            )
            .map_err(|e| DataError::Unavailable(format!("`{source}`'s overlay threw: {e}")))?;
        let deadline = self.snapshot_deadline.is_some();
        if let Some(why) = clock.judge(
            budget_ms,
            deadline,
            self.main_thread,
            source,
            &mut self.logs,
        ) {
            return Err(DataError::Overrun(format!("`{source}`'s overlay {why}")));
        }
        let reply = exact_js_value::json::parse(text.as_bytes())
            .map(Json::from)
            .map_err(|e| DataError::Unavailable(format!("`{source}`'s overlay: {e}")))?;
        let keep = reply
            .get("keep")
            .and_then(Json::as_array)
            .map(|ids| ids.iter().filter_map(Json::as_u64).collect())
            .unwrap_or_default();
        match reply.get("tag").and_then(Json::as_u64) {
            Some(0) => from_json(reply.get("value").unwrap_or(&Json::Null), &sig.result)
                .map(|value| Some(exact_runner::Overlaid { value, keep }))
                .map_err(|e| {
                    DataError::Failed(
                        FailureCode::Shape,
                        format!("`{source}`'s overlay is outside its shape: {e}"),
                        String::new(),
                    )
                }),
            Some(4) => Ok(None),
            _ => Err(DataError::Unavailable(format!(
                "`{source}`'s overlay failed: {}",
                reply.get("message").and_then(Json::as_str).unwrap_or("")
            ))),
        }
    }
}

impl Drop for Module {
    fn drop(&mut self) {
        self.unload();
    }
}

#[cfg(all(test, exact_js_engine))]
mod adopt_tests;
#[cfg(all(test, exact_js_engine))]
mod bootstrap_tests;
