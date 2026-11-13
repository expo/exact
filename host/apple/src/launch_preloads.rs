//! Process-start requests and their boot handoff (LLP 1162.000 D4).
use crate::executor::{Io, Preload, PreloadControl, PreloadFact, PreloadObservation, Preloads};
use exact_runner::{request::PreloadWhen, DataSource, Runner};
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU8, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
#[path = "launch_rust.rs"]
mod rust;
#[path = "launch_store.rs"]
mod store;
pub use store::StoreWatch;

const BEGUN: u8 = 1;
const PREWARM: u8 = 2;
const LAUNCHED: u8 = 4;
const OPENED: u8 = 8;
static PROCESS: OnceLock<LaunchPreloads> = OnceLock::new();

/// Environment flags that give a driver ownership of requests or select another plan.
pub const EXCLUSIONS: &[&str] = &[
    "EXACT_AGENT",
    "EXACT_PLAN",
    "EXACT_DEV_PLAN",
    "EXACT_TESTS",
    "XCTestConfigurationFilePath",
    "XCTestBundlePath",
];

/// A launch's work, shared by entry points, the store and its boot.
#[derive(Clone, Default)]
pub struct LaunchPreloads(Arc<Launch>);
#[derive(Default)]
struct Launch {
    phase: AtomicU8,
    selected_update: AtomicBool,
    set: OnceLock<Set>,
    invalidated: Mutex<BTreeMap<String, u64>>,
    rust: OnceLock<Arc<rust::Running>>,
    hosts: Mutex<Vec<std::sync::Weak<StoreWatch>>>,
    foreground: Mutex<Vec<rust::Prepared>>,
    #[cfg(test)]
    backend: Mutex<Option<Arc<dyn exact_data::secrets::Backend>>>,
    journal: Mutex<Vec<PreloadFact>>,
    marks: Mutex<Marks>,
    #[cfg(test)]
    reader: Mutex<Option<Arc<SecretReader>>>,
}
#[cfg(test)]
type SecretReader =
    dyn Fn(&str, &str, &[exact_runner::request::Preload]) -> BTreeMap<String, String> + Send + Sync;

struct DefinitionsPending(PreloadControl);
impl Drop for DefinitionsPending {
    fn drop(&mut self) {
        self.0.definitions_pending(false);
    }
}

struct Set {
    owner: Mutex<Option<Preloads>>,
    control: PreloadControl,
}
#[derive(Default)]
struct Marks {
    entries: Vec<(String, LaunchMark)>,
    tickets: BTreeMap<u64, String>,
}
/// Seconds on CACurrentMediaTime's mach_absolute_time clock; zero means unobserved.
#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct LaunchMark {
    /// Handed to the fetch client.
    pub handed: f64,
    /// Complete reply received.
    pub completed: f64,
    /// The batch carrying the successful fulfillment was built.
    pub committed: f64,
    /// Last runner commit sequence in that batch.
    pub sequence: u64,
}

/// The process's handoff, if a launch entry point has run.
pub fn current() -> Option<LaunchPreloads> {
    PROCESS.get().cloned()
}

/// [`current`] as a session holds it: [`crate::link::IoLinks`]'s `preloads`.
pub fn linked() -> Option<Box<dyn LaunchWork>> {
    current().map(|launch| Box::new(launch) as Box<dyn LaunchWork>)
}

/// What a session asks of the launch's work. A session reaches it only
/// through [`crate::link::IoLinks`], so an archive that links no I/O
/// carries none of it, nor the executor behind it (LLP 1047.001).
pub trait LaunchWork: Send + Sync {
    /// Whether a launch entry point started the work.
    fn started(&self) -> bool;
    /// The launch's answer to one of the boot's requests, if it has one.
    fn answer(
        &self,
        request: &exact_runner::RequestOut,
    ) -> Option<(exact_runner::Outcome, exact_runner::CompletionTiming)>;
    /// The app selected an update: the baked plan's definitions are stale.
    fn select_update(&self);
    /// Hand what is still in flight to the session's executor.
    fn adopt(&self, executor: &dyn Io);
    /// A write changed the secret `name`.
    fn drop_secret(&self, name: &str);
    /// What happened to each definition since the last call.
    fn journal(&self) -> Vec<PreloadFact>;
    /// The definition the ticket's request came from, once.
    fn fulfilled(&self, ticket: u64) -> Option<String>;
    /// The named definitions' answers reached a batch at `sequence`.
    fn committed(&self, names: Vec<String>, sequence: u64);
    /// Watch the launch's store writes, `pending` read under the writers' lock.
    fn watch_store(&self, pending: &dyn Fn() -> bool) -> Arc<StoreWatch>;
}

impl LaunchWork for LaunchPreloads {
    fn started(&self) -> bool {
        self.0.set.get().is_some()
    }
    fn answer(
        &self,
        request: &exact_runner::RequestOut,
    ) -> Option<(exact_runner::Outcome, exact_runner::CompletionTiming)> {
        self.0.set.get()?.control.settle(request)
    }
    fn select_update(&self) {
        self.0.selected_update.store(true, Ordering::Release);
        if let Some(set) = self.0.set.get() {
            set.control.select_update();
        }
    }
    fn adopt(&self, executor: &dyn Io) {
        if let Some(set) = self.0.set.get() {
            let owner = set.owner.lock().unwrap().take();
            if let Some(owner) = owner {
                if let Err(owner) = executor.adopt_preloads(owner) {
                    *set.owner.lock().unwrap() = Some(owner);
                }
            }
        }
    }
    fn drop_secret(&self, name: &str) {
        self.invalidate(name);
    }
    fn journal(&self) -> Vec<PreloadFact> {
        LaunchPreloads::journal(self)
    }
    fn fulfilled(&self, ticket: u64) -> Option<String> {
        LaunchPreloads::fulfilled(self, ticket)
    }
    fn committed(&self, names: Vec<String>, sequence: u64) {
        LaunchPreloads::committed(self, names, sequence)
    }
    fn watch_store(&self, pending: &dyn Fn() -> bool) -> Arc<StoreWatch> {
        LaunchPreloads::watch_store(self, pending)
    }
}

impl dyn LaunchWork {
    /// Answer the boot's requests the launch already holds, in ticket order,
    /// up to the first it does not; the rest are the boot's to send.
    pub(crate) fn settle<D: DataSource>(
        &self,
        runner: &mut Runner<D>,
    ) -> Result<(Vec<exact_runner::RequestOut>, Vec<String>), crate::host::HostError> {
        if !self.started() {
            return Ok((Vec::new(), Vec::new()));
        }
        let mut pending = Vec::new();
        let mut fulfilled = Vec::new();
        let mut requests = runner.take_requests();
        requests.sort_by_key(|request| request.ticket);
        let mut requests = requests.into_iter();
        while let Some(request) = requests.next() {
            if !runner.holds(request.ticket) {
                continue;
            }
            if let Some((outcome, timing)) = request
                .request
                .continuation
                .is_none()
                .then(|| self.answer(&request))
                .flatten()
            {
                let name = self.fulfilled(request.ticket);
                if runner
                    .fulfill_measured(request.ticket, outcome, Some(timing))
                    .map_err(crate::host::HostError::Runner)?
                    .is_some()
                {
                    fulfilled.extend(name);
                }
            } else {
                pending.push(request);
                pending.extend(requests);
                break;
            }
        }
        pending.extend(runner.take_requests());
        pending.sort_by_key(|request| request.ticket);
        pending.retain(|request| runner.holds(request.ticket));
        Ok((pending, fulfilled))
    }
}

/// Begin at the top of main; the factory is evaluated only on the launch thread.
pub fn begin<D: DataSource>(prewarm: bool, factory: impl FnOnce() -> D + Send + 'static) {
    PROCESS
        .get_or_init(LaunchPreloads::default)
        .begin(prewarm, factory);
}
/// The delegate's initializer marks the real launch after prewarming.
pub fn launched<D: DataSource>(factory: impl FnOnce() -> D + Send + 'static) {
    PROCESS
        .get_or_init(LaunchPreloads::default)
        .signal(false, factory);
}
/// A person opened this launch; foreground definitions can now leave.
pub fn opened<D: DataSource>(factory: impl FnOnce() -> D + Send + 'static) {
    PROCESS
        .get_or_init(LaunchPreloads::default)
        .signal(true, factory);
}

impl LaunchPreloads {
    fn begin<D: DataSource>(&self, prewarm: bool, factory: impl FnOnce() -> D + Send + 'static) {
        let previous = self
            .0
            .phase
            .try_update(Ordering::AcqRel, Ordering::Acquire, |phase| {
                (phase & BEGUN == 0)
                    .then_some(phase | BEGUN | if prewarm { PREWARM } else { LAUNCHED })
            });
        if previous.is_ok() && (!prewarm || self.0.phase.load(Ordering::Acquire) & LAUNCHED != 0) {
            self.start(factory);
        }
    }
    fn signal<D: DataSource>(&self, opened: bool, factory: impl FnOnce() -> D + Send + 'static) {
        let previous = self
            .0
            .phase
            .fetch_or(LAUNCHED | if opened { OPENED } else { 0 }, Ordering::AcqRel);
        self.start(factory);
        if opened && previous & OPENED == 0 && self.0.set.get().is_some() {
            self.start_foreground();
        }
    }
    fn start<D: DataSource>(&self, factory: impl FnOnce() -> D + Send + 'static) {
        if EXCLUSIONS
            .iter()
            .any(|name| std::env::var_os(name).is_some())
        {
            return;
        }
        let mut started = false;
        self.0.set.get_or_init(|| {
            let owner = Preloads::start(Vec::new(), "", ibex2::host::Host::new);
            let control = owner.control();
            control.definitions_pending(true);
            started = true;
            Set {
                owner: Mutex::new(Some(owner)),
                control,
            }
        });
        if !started {
            return;
        }
        if self.0.selected_update.load(Ordering::Acquire) {
            self.0.set.get().unwrap().control.select_update();
        }
        let launch = self.clone();
        let spawned = std::thread::Builder::new()
            .name("exact-launch".into())
            .spawn(move || {
                let pending = DefinitionsPending(launch.0.set.get().unwrap().control.clone());
                let data = factory();
                let definitions: Vec<_> = data.preloads().iter().copied().take(8).collect();
                let grants = data.grants().to_owned();
                let entry = data.launch();
                let app = data.app_id().to_owned();
                #[cfg(test)]
                let custom = launch.0.reader.lock().unwrap().clone();
                #[cfg(test)]
                let secrets = match custom {
                    Some(read) => read(&grants, data.app_id(), &definitions),
                    None => crate::store::launch_secrets(&grants, data.app_id(), &definitions),
                };
                #[cfg(not(test))]
                let secrets = crate::store::launch_secrets(&grants, data.app_id(), &definitions);
                drop(data);
                let mut marks = launch.0.marks.lock().unwrap();
                marks.entries.extend(
                    definitions
                        .iter()
                        .map(|d| (d.name.into(), LaunchMark::default())),
                );
                drop(marks);
                for definition in &definitions {
                    launch.0.set.get().unwrap().control.defined(definition.name);
                }
                launch.start_definitions(&definitions, &secrets, &grants, PreloadWhen::Start);
                launch.start_definitions(&definitions, &secrets, &grants, PreloadWhen::Foreground);
                if let Some(entry) = entry {
                    launch.0.set.get().unwrap().control.handed();
                    rust::run(&launch, entry, &app, &grants, &definitions);
                }
                launch.0.set.get().unwrap().control.launch_pending(None);
                drop(pending);
            });
        if spawned.is_err() {
            self.0.set.get().unwrap().control.definitions_pending(false);
        }
    }
    fn start_definitions(
        &self,
        definitions: &[exact_runner::request::Preload],
        secrets: &BTreeMap<String, String>,
        grants: &str,
        when: PreloadWhen,
    ) {
        let weak = Arc::downgrade(&self.0);
        let observer = Arc::new(move |name: &str, observation| {
            if let Some(launch) = weak.upgrade().map(LaunchPreloads) {
                launch.observe(name, observation);
            }
        });
        let invalidated = self.0.invalidated.lock().unwrap();
        let mut ready = Vec::new();
        for definition in definitions.iter().filter(|d| d.when == when) {
            if definition
                .secrets
                .iter()
                .any(|name| invalidated.contains_key(*name))
            {
                self.record_definition(definition, crate::executor::PreloadEvent::DroppedBySecret);
                continue;
            }
            match definition.fill(|name| secrets.get(name).cloned()) {
                Ok(request) => {
                    use crate::executor::PreloadEvent;
                    use ibex2::stdlib::fetch;
                    let invalid = if url::Url::parse(&request.url).is_err() {
                        Some(PreloadEvent::InvalidUrl)
                    } else if request.headers.iter().any(|(_, value)| {
                        !fetch::is_valid_value(value) || fetch::normalize_value(value) != value
                    }) {
                        Some(PreloadEvent::InvalidHeader)
                    } else {
                        None
                    };
                    if let Some(outcome) = invalid {
                        self.record_definition(definition, outcome);
                        continue;
                    }
                    let preload = Preload {
                        name: definition.name.into(),
                        request,
                        deadline_ms: definition.timeout_ms,
                        secrets: definition.secrets.iter().map(|s| (*s).into()).collect(),
                    };
                    if when == PreloadWhen::Foreground
                        && self.0.phase.load(Ordering::Acquire) & OPENED == 0
                    {
                        self.0.foreground.lock().unwrap().push(rust::Prepared {
                            preload,
                            grants: grants.into(),
                            versions: definition
                                .secrets
                                .iter()
                                .map(|s| ((*s).into(), 0))
                                .collect(),
                        });
                    } else {
                        ready.push(preload);
                    }
                }
                Err(_) => {
                    self.record_definition(definition, crate::executor::PreloadEvent::MissingSecret)
                }
            }
        }
        if let Some(set) = self.0.set.get() {
            set.control.extend(ready, grants, observer);
        }
    }
    fn record_definition(
        &self,
        definition: &exact_runner::request::Preload,
        outcome: crate::executor::PreloadEvent,
    ) {
        let origin = definition
            .fill(|_| Some("redacted".into()))
            .ok()
            .and_then(|r| url::Url::parse(&r.url).ok())
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default();
        self.0.journal.lock().unwrap().push(PreloadFact {
            name: definition.name.into(),
            origin,
            outcome,
        });
    }
    fn observe(&self, name: &str, observation: PreloadObservation) {
        let at = clock();
        let mut marks = self.0.marks.lock().unwrap();
        if let PreloadObservation::Taken(ticket) = observation {
            marks.tickets.insert(ticket, name.into());
            return;
        }
        if let Some((_, mark)) = marks.entries.iter_mut().find(|(n, _)| *n == name) {
            match observation {
                PreloadObservation::Handed => mark.handed = at,
                PreloadObservation::Complete => mark.completed = at,
                PreloadObservation::Taken(_) => unreachable!(),
            }
        }
    }
    fn invalidate(&self, name: &str) -> u64 {
        let mut versions = self.0.invalidated.lock().unwrap();
        let version = versions.entry(name.into()).or_default();
        *version += 1;
        if let Some(set) = self.0.set.get() {
            set.control.drop_secret(name);
        }
        *version
    }
    fn with_secret_version<T>(
        &self,
        name: &str,
        expected: u64,
        write: impl FnOnce(u64) -> Result<T, String>,
    ) -> Result<T, String> {
        // Compare, persist and publish in the app writers' order (LLP 1162.000 D3).
        let mut versions = self.0.invalidated.lock().unwrap();
        let version = versions.entry(name.into()).or_default();
        if *version != expected {
            return Err("launch secret changed".into());
        }
        *version += 1;
        if let Some(set) = self.0.set.get() {
            set.control.drop_secret(name);
        }
        write(*version)
    }
    fn with_secret_write<T>(&self, name: &str, write: impl FnOnce() -> T) -> T {
        let mut versions = self.0.invalidated.lock().unwrap();
        *versions.entry(name.into()).or_default() += 1;
        if let Some(set) = self.0.set.get() {
            set.control.drop_secret(name);
        }
        write()
    }
    fn start_foreground(&self) {
        let versions = self.0.invalidated.lock().unwrap();
        let mut foreground = self.0.foreground.lock().unwrap();
        for definition in foreground.drain(..) {
            if definition
                .versions
                .iter()
                .any(|(name, version)| versions.get(name).copied().unwrap_or(0) != *version)
            {
                self.0.journal.lock().unwrap().push(PreloadFact {
                    name: definition.preload.name.clone(),
                    origin: rust::origin(&definition.preload.request),
                    outcome: crate::executor::PreloadEvent::DroppedBySecret,
                });
            } else {
                self.extend_rust(definition.preload, &definition.grants);
            }
        }
    }
    fn extend_rust(&self, preload: Preload, grants: &str) {
        let weak = Arc::downgrade(&self.0);
        let observer = Arc::new(move |name: &str, observation| {
            if let Some(launch) = weak.upgrade().map(LaunchPreloads) {
                launch.observe(name, observation);
            }
        });
        self.0
            .set
            .get()
            .unwrap()
            .control
            .extend(vec![preload], grants, observer);
    }
    pub(crate) fn journal(&self) -> Vec<PreloadFact> {
        let mut facts = std::mem::take(&mut *self.0.journal.lock().unwrap());
        if let Some(set) = self.0.set.get() {
            facts.extend(set.control.take_journal());
        }
        facts
    }
    pub(crate) fn fulfilled(&self, ticket: u64) -> Option<String> {
        self.0.marks.lock().unwrap().tickets.remove(&ticket)
    }
    pub(crate) fn committed(&self, names: Vec<String>, sequence: u64) {
        let mut marks = self.0.marks.lock().unwrap();
        for (name, mark) in &mut marks.entries {
            if mark.committed == 0. && names.iter().any(|n| n.as_str() == name) {
                mark.committed = clock();
                mark.sequence = sequence;
            }
        }
    }
    /// Definition order; no request bytes cross the measurement ABI.
    pub fn mark(&self, index: usize) -> Option<(String, LaunchMark)> {
        self.0.marks.lock().unwrap().entries.get(index).cloned()
    }
    /// Number of launch definitions whose marks can be read.
    pub fn count(&self) -> usize {
        self.0.marks.lock().unwrap().entries.len()
    }
}

pub(crate) fn with_secret_write<T>(name: &str, write: impl FnOnce() -> T) -> T {
    match current() {
        Some(launch) => launch.with_secret_write(name, write),
        None => write(),
    }
}

#[cfg(target_vendor = "apple")]
#[allow(unsafe_code)]
fn clock() -> f64 {
    #[repr(C)]
    struct Timebase {
        numer: u32,
        denom: u32,
    }
    extern "C" {
        fn mach_absolute_time() -> u64;
        fn mach_timebase_info(info: *mut Timebase) -> i32;
    }
    let mut base = Timebase { numer: 0, denom: 0 };
    // SAFETY: the system writes exactly one timebase into this local value.
    unsafe {
        if mach_timebase_info(&mut base) != 0 {
            return 0.;
        }
        mach_absolute_time() as f64 * base.numer as f64 / base.denom as f64 / 1e9
    }
}
#[cfg(not(target_vendor = "apple"))]
fn clock() -> f64 {
    static ZERO: OnceLock<std::time::Instant> = OnceLock::new();
    ZERO.get_or_init(std::time::Instant::now)
        .elapsed()
        .as_secs_f64()
        + 1.
}

/// Copy the name into a caller-owned buffer; null reads only its size.
#[allow(unsafe_code, clippy::not_unsafe_ptr_arg_deref)]
pub fn copy_name(index: usize, out: *mut u8, capacity: usize) -> usize {
    let Some((name, _)) = current().and_then(|p| p.mark(index)) else {
        return 0;
    };
    if !out.is_null() {
        // SAFETY: the ABI caller supplies capacity writable bytes, without aliasing the launch name.
        unsafe {
            std::ptr::copy_nonoverlapping(name.as_ptr(), out, capacity.min(name.len()));
        }
    }
    name.len()
}

#[cfg(test)]
#[path = "launch_preloads_tests.rs"]
mod tests;
