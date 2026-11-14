//! Host configuration and worker-side readiness for storage continuations.
//! @ref LLP 1027 D1a / D10 — effects leave the data seam as host work.
use exact_runner::{FailureKind, Outcome, Response};
use ibex2::{
    bindings::Context,
    grant::GrantSet,
    stdlib::{
        app_fs::AppDirectories,
        fs::{DocumentError, DocumentLease},
        fs_image::{self, CancelResult, WorkHandle},
    },
};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Condvar, Mutex,
    },
    time::{Duration, Instant},
};

#[derive(Clone)]
pub(crate) struct Directories {
    pub data: PathBuf,
    pub cache: PathBuf,
    pub temporary: PathBuf,
}

/// What a waiter that took a compression's right to write away fails with
/// (LLP 1069.002 A1.5); the prelude settles that operation on it.
pub(crate) const IMAGE_ABANDONED: &str =
    "compressImage: timeout: the storage wait ran out; nothing was written";

/// How long a storage step is waited for before its answer fails.
pub(crate) const WAIT: Duration = Duration::from_secs(30);

pub(crate) struct Session {
    pub context: Arc<Context>,
    alive: Arc<AtomicBool>,
    pub worker_retired: Option<Arc<AtomicBool>>,
    wait: Duration,
    /// A test's hold on compressions (`Module::hold_compressions`).
    hold: Option<SessionHold>,
    #[cfg(test)]
    extended: Option<std::sync::mpsc::Sender<Arc<Context>>>,
}

impl Session {
    /// A storage session under `grants`: the app's directories where the
    /// host configured them, and always the documents the person chose
    /// (`doc:`, LLP 1069.010 D1), which need none, as a Rust source's do.
    pub fn open(
        grants: &str,
        wait: Duration,
        owner: u64,
        hold: Option<Arc<Gate>>,
    ) -> Result<Self, String> {
        let hold = hold.map(SessionHold::new);
        let grants = GrantSet::parse(grants).map_err(|e| e.to_string())?;
        let context = Context::new(grants);
        context
            .set_storage_limits(ibex2::stdlib::fs::ResultLimits {
                bytes: exact_data::storage::MAX_BYTES,
            })
            .map_err(|e| e.to_string())?;
        if let Some(provider) = ibex2::stdlib::sqlite::linked() {
            context
                .set_sqlite_provider(provider)
                .map_err(|e| e.to_string())?;
        }
        context
            .set_documents(Arc::new(move |path, _access| documents(path, owner)))
            .map_err(|e| e.to_string())?;
        // Only where there is a codec: elsewhere ibex2 refuses the call as
        // unsupported before reading the source (LLP 1069.002 A1.3).
        if exact_data::image::available() {
            context
                .set_image_codec(Arc::new(ImageCodec { hold: hold.clone() }), image_limits())
                .map_err(|e| e.to_string())?;
        }
        Ok(Self {
            context: Arc::new(context),
            alive: Arc::new(AtomicBool::new(true)),
            worker_retired: None,
            wait,
            hold,
            #[cfg(test)]
            extended: None,
        })
    }

    /// Activate app directories after the native module has received their
    /// paths, but before storage is materialized in JavaScript.
    pub fn configure(&self, paths: &Directories, read_only: bool) -> Result<(), String> {
        for path in [&paths.data, &paths.cache, &paths.temporary] {
            if !path.is_absolute() {
                return Err("app storage roots must be absolute".into());
            }
            if !read_only {
                std::fs::create_dir_all(path).map_err(|e| format!("app storage: {e}"))?;
            }
        }
        #[cfg(unix)]
        let directories = if read_only {
            AppDirectories::snapshot(&paths.data, &paths.cache, &paths.temporary)
        } else {
            AppDirectories::new(&paths.data, &paths.cache, &paths.temporary)
        };
        #[cfg(not(unix))]
        let directories = AppDirectories::new(&paths.data, &paths.cache, &paths.temporary);
        self.context
            .set_app_directories(directories.map_err(|e| e.to_string())?)
            .map_err(|e| e.to_string())
    }

    /// Wait on this thread, until `deadline`, for a completion to deliver
    /// (or nothing left in flight): teardown's bounded wait (LLP 1097 D10).
    pub fn wait_until(&self, deadline: Instant) -> bool {
        loop {
            if self.context.is_idle() || self.context.wait(Duration::from_millis(25)) {
                return true;
            }
            if Instant::now() >= deadline {
                return false;
            }
        }
    }

    pub fn continuation(&self, engine: &mut crate::Engine) -> Box<dyn FnOnce() -> Outcome + Send> {
        self.continuation_for(None, true, engine)
    }

    /// The module owns work left in flight after an answer fails. Its
    /// uncancellable writes keep their queue slot until completion or unload.
    pub fn background_continuation(
        &self,
        engine: &mut crate::Engine,
    ) -> Box<dyn FnOnce() -> Outcome + Send> {
        self.continuation_for(None, false, engine)
    }

    /// A waiter for an answer's step, which gives up without taking the
    /// current image's right once `retired` is set (its call was let go, and
    /// another waiter now watches the work: LLP 1069.002 A1.5).
    /// Without background delivery, an uncancellable step keeps waiting.
    pub fn continuation_for(
        &self,
        retired: Option<Arc<AtomicBool>>,
        fail_wait: bool,
        engine: &mut crate::Engine,
    ) -> Box<dyn FnOnce() -> Outcome + Send> {
        // Without a native identity, no cancellation can release the head.
        let image = engine.storage_image_work(&self.context).ok().flatten();
        self.wait_for(retired, fail_wait, image)
    }

    fn wait_for(
        &self,
        retired: Option<Arc<AtomicBool>>,
        fail_wait: bool,
        image: Option<WorkHandle>,
    ) -> Box<dyn FnOnce() -> Outcome + Send> {
        let context = self.context.clone();
        let alive = self.alive.clone();
        let worker_retired = self.worker_retired.clone();
        let wait = self.wait;
        let hold = self.hold.clone();
        #[cfg(test)]
        let extended = self.extended.clone();
        // Retain only the current head. An abandoned image can still be
        // running after its queue slot was released, under a later waiter.
        Box::new(move || {
            let mut deadline = Instant::now() + wait;
            loop {
                if !alive.load(Ordering::Acquire)
                    || worker_retired
                        .as_ref()
                        .is_some_and(|r| r.load(Ordering::Acquire))
                {
                    return Outcome::Failed {
                        kind: FailureKind::Aborted,
                        message: "storage runtime was unloaded".into(),
                    };
                }
                // Another ready call may already have completed this answer;
                // the owner thread still has to inspect its JS continuation.
                if context.is_idle() || context.wait(Duration::from_millis(25)) {
                    return Outcome::Response(Response {
                        status: 204,
                        headers: vec![],
                        body: vec![],
                    });
                }
                let live = || retired.as_ref().is_none_or(|r| !r.load(Ordering::Acquire));
                let retired_outcome = || Outcome::Failed {
                    kind: FailureKind::Aborted,
                    message: "storage continuation retired: its call was let go".into(),
                };
                if !live() {
                    return retired_outcome();
                }
                // Held compressions outlast any wait: with nothing else in
                // flight, and no completion ready (one is queued before its
                // work leaves the count), the wait has run out.
                let held = hold.as_ref().map_or(0, SessionHold::held);
                let only_held =
                    held > 0 && context.in_flight() <= held && !context.wait(Duration::ZERO);
                if only_held || Instant::now() >= deadline {
                    let mut abandoned = false;
                    let mut committed = false;
                    if let Some(handle) = &image {
                        if !live() {
                            return retired_outcome();
                        }
                        match handle.cancel() {
                            CancelResult::Abandoned => abandoned = true,
                            CancelResult::CommitStarted | CancelResult::Settled(Ok(_)) => {
                                committed = true
                            }
                            CancelResult::Settled(Err(_)) => {}
                        }
                    }
                    if !live() {
                        return retired_outcome();
                    }
                    // Cancellation that loses publication returns immediately.
                    // This embedder waits outside Ibex locks and delivers the
                    // actual result instead of claiming nothing was written.
                    if committed || (!abandoned && (!fail_wait || context.wait(Duration::ZERO))) {
                        deadline = Instant::now() + Duration::from_secs(5);
                        #[cfg(test)]
                        if let Some(extended) = &extended {
                            let _ = extended.send(context.clone());
                        }
                        continue;
                    }
                    let message = if abandoned {
                        IMAGE_ABANDONED
                    } else {
                        "storage continuation timed out"
                    };
                    return Outcome::Failed {
                        kind: FailureKind::Aborted,
                        message: message.into(),
                    };
                }
            }
        })
    }
}

/// Storage's availability as the prelude's refusal code (kanban F28): `bake`
/// where storage is not live; none where the module reaches it; else `agent`
/// for a drive that names no scratch store, or `unsupported`.
pub(crate) fn refusal(live: bool, reaches: bool, agent: bool) -> Result<Option<String>, String> {
    if !live {
        return Err("bake".into());
    }
    Ok((!reaches).then(|| if agent { "agent" } else { "unsupported" }.into()))
}

/// The kept files an atomic write from TypeScript is replacing, by the
/// token the prelude holds until the write settles.
#[allow(clippy::type_complexity)]
static REPLACING: std::sync::Mutex<std::collections::BTreeMap<u64, (u64, String, ReplacedFiles)>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());
static NEXT_REPLACING: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);
type ReplacedFiles = exact_data::documents::kept::Replaced;

/// `storage.fs.recentDocuments()` (`ask` empty) and `forgetDocument(path)`
/// (`ask` is `forget <path>`), for the session `owner` names when the grants
/// keep documents (LLP 1069.010.000 D5): the list as JSON, or nothing.
///
/// And the app's own `atomicWriteFile` of a kept file (D4), around the
/// write: `replacing <path>` reads what it replaces and answers a token, or
/// nothing when it replaces no kept file; `replaced <token>` carries the
/// entry to the file written; `dropped <token>` lets it go (the write failed).
pub(crate) fn kept(owner: Option<u64>, ask: &str) -> Result<Option<String>, String> {
    use exact_data::documents::kept;
    let owner =
        owner.ok_or("storage: recentDocuments and forgetDocument need the doc.keep grant")?;
    let replacing = || REPLACING.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(path) = ask.strip_prefix("replacing ") {
        // A save issued while an earlier one of the same file is in flight
        // replaces what that one writes: the earlier save may have renamed
        // its file and not yet carried the entry, so the identity at the path
        // is not yet the kept one. The queue runs the two in order.
        let earlier = || {
            (replacing().values())
                .find(|(o, p, _)| *o == owner && p == path)
                .map(|(.., r)| r.clone())
        };
        let Some(replaced) = earlier().or_else(|| kept::before_write(owner, path)) else {
            return Ok(None);
        };
        let token = NEXT_REPLACING.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        replacing().insert(token, (owner, path.to_owned(), replaced));
        return Ok(Some(token.to_string()));
    }
    for (how, carry) in [("replaced ", true), ("dropped ", false)] {
        if let Some(token) = ask.strip_prefix(how) {
            let held = token.parse().ok().and_then(|t: u64| replacing().remove(&t));
            if let Some((owner, _, replaced)) = held.filter(|_| carry) {
                kept::carry(owner, replaced)?;
            }
            return Ok(None);
        }
    }
    match ask.strip_prefix("forget ") {
        Some(path) => kept::forget(owner, path)
            .map(|()| None)
            .map_err(|_| format!("document ENOENT: {path} is not a document this app keeps")),
        // A session that keeps nothing (no store) has kept nothing.
        None => Ok(Some(
            kept::list(owner).map_or_else(|| "[]".into(), |list| list.to_string()),
        )),
    }
}

/// The documents a host minted ([`exact_data::documents`]), as ibex2's
/// table: the one a Rust source's storage requests resolve through.
fn documents(path: &str, owner: u64) -> Result<DocumentLease, DocumentError> {
    use exact_data::documents::{resolve, Resolved};
    let resolved = resolve(path, owner).map_err(DocumentError::from)?;
    let guard = std::sync::Arc::new(());
    match resolved {
        Resolved::Root(name) => DocumentLease::root(name, guard),
        Resolved::Real {
            directory,
            path,
            selected_file: Some(name),
        } => DocumentLease::selected_file_named(directory, path, name, guard),
        Resolved::Real {
            directory,
            path,
            selected_file: None,
        } => DocumentLease::directory(directory, path, guard),
    }
}

/// The platform's image codec ([`exact_data::image`]), as ibex2's
/// (`fs.compressImage`, LLP 1069.002 A1; `fs.transformImage`, LLP 1133.019
/// D14): the one a Rust source's storage requests run too.
pub(crate) struct ImageCodec {
    hold: Option<SessionHold>,
}
/// A transform's output side bounds the service; `compressImage`'s own
/// `maxDimension` is the codec's to check (8192).
fn image_limits() -> fs_image::WorkLimits {
    fs_image::WorkLimits {
        max_job_bytes: 640 << 20,
        max_dimension: exact_data::image::edit::MAX_SIDE,
        ..fs_image::WorkLimits::default()
    }
}
impl fs_image::ImageCodec for ImageCodec {
    fn limits(&self) -> fs_image::CodecLimits {
        fs_image::CodecLimits {
            max_pixels: exact_data::image::MAX_SOURCE_PIXELS,
            max_scratch_bytes: 512 << 20,
        }
    }
    fn encode(
        &self,
        bytes: &[u8],
        request: fs_image::ImageRequest,
        control: &fs_image::WorkControl,
    ) -> Result<fs_image::CompressedImage, ibex2::boundary::HostError> {
        if let Some(hold) = &self.hold {
            hold.enter();
        }
        let call = request.call();
        exact_data::image::encode_request(
            bytes,
            request.transform,
            request.options.as_deref(),
            match request.format {
                fs_image::ImageFormat::Jpeg => exact_data::image::Format::Jpeg,
                fs_image::ImageFormat::Png => exact_data::image::Format::Png,
            },
            (request.max_dimension, request.max_bytes),
            exact_data::image::Deadline {
                at: control.deadline(),
                check: &|| {
                    control
                        .check()
                        .map_err(|e| exact_data::image::edit::failure_named(call, "timeout", e))
                },
                max_pixels: control
                    .limits()
                    .max_pixels
                    .min(exact_data::image::MAX_SOURCE_PIXELS),
                max_scratch_bytes: control.limits().max_scratch_bytes,
            },
        )
        .map(|c| fs_image::CompressedImage {
            bytes: c.bytes,
            width: c.width,
            height: c.height,
            report: c.report,
            extension: c.extension,
        })
        .map_err(ibex2::boundary::HostError::Failed)
    }
}

/// Whether `grants` reach documents (`fs.read doc:/`, `fs.write doc:/`):
/// such a module gets a storage session even with no app directories.
pub(crate) fn reaches_documents(grants: &str) -> bool {
    grants.lines().any(|line| {
        let mut words = line.split_whitespace();
        matches!(words.next(), Some("fs.read" | "fs.write"))
            && words.next().is_some_and(|p| p.starts_with("doc:"))
    })
}

impl crate::Module {
    /// Holds each image compression or transform at its start, from the
    /// next [`crate::Module::load`], until the hold is released or dropped.
    /// Meanwhile a storage wait with only held work in flight runs out at
    /// once, as it would were the work longer than the wait: a test of
    /// giving up gives up while the work runs, however fast the codec and
    /// the machine (docs/testing.md).
    #[doc(hidden)]
    pub fn hold_compressions(&mut self) -> CompressionHold {
        let gate = Arc::new(Gate::default());
        self.compression_hold = Some(gate.clone());
        CompressionHold(gate)
    }
}

/// A test's hold on the module's compressions ([`crate::Module::hold_compressions`]).
#[doc(hidden)]
pub struct CompressionHold(Arc<Gate>);

impl CompressionHold {
    /// Lets every held compression, and each that starts later, run.
    pub fn release(&self) {
        let mut state = self.0.state.lock().unwrap_or_else(|e| e.into_inner());
        state.released = true;
        state.held.clear();
        self.0.changed.notify_all();
    }
}

impl Drop for CompressionHold {
    fn drop(&mut self) {
        self.release();
    }
}

#[derive(Default)]
pub(crate) struct Gate {
    state: Mutex<Held>,
    changed: Condvar,
}

#[derive(Default)]
struct Held {
    /// The compressions held now, by session.
    held: BTreeMap<u64, usize>,
    sessions: u64,
    released: bool,
}

/// One storage session's part of a hold: its compressions are counted apart
/// from those an earlier session of the module left held.
#[derive(Clone)]
struct SessionHold {
    gate: Arc<Gate>,
    session: u64,
}

impl SessionHold {
    fn new(gate: Arc<Gate>) -> Self {
        let session = {
            let mut state = gate.state.lock().unwrap_or_else(|e| e.into_inner());
            state.sessions += 1;
            state.sessions
        };
        Self { gate, session }
    }

    /// Waits until the hold is released.
    fn enter(&self) {
        let mut state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        if !state.released {
            *state.held.entry(self.session).or_default() += 1;
        }
        while !state.released {
            state = self
                .gate
                .changed
                .wait(state)
                .unwrap_or_else(|e| e.into_inner());
        }
    }

    /// This session's compressions held now.
    fn held(&self) -> usize {
        let state = self.gate.state.lock().unwrap_or_else(|e| e.into_inner());
        state.held.get(&self.session).copied().unwrap_or(0)
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        self.alive.store(false, Ordering::Release);
        // Close admission and abandon cancellable work without waiting under
        // a shared lock. An already-started publication retains its real result.
        self.context.image_work().close();
    }
}

#[cfg(all(test, unix))]
mod tests {
    /// LLP 1069.010.000 D4: a save issued after an earlier one of the same
    /// kept file renamed its file but before it carried the entry still
    /// carries it, to the file the later one writes.
    #[test]
    fn a_save_issued_while_another_is_in_flight_carries_the_entry() {
        use exact_data::documents::kept;
        use std::os::unix::fs::MetadataExt;
        let dir = std::env::temp_dir().join(format!("exact-js-overlap-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("drawing.md");
        std::fs::write(&file, "first").unwrap();
        let set = dir.join("kept.json");
        kept::open(9140, "test.js.overlap", &set);
        let doc = kept::keep(9140, &file, "", "readwrite").unwrap();
        let replace = |text: &str| {
            std::fs::write(dir.join("next"), text).unwrap();
            std::fs::rename(dir.join("next"), &file).unwrap();
        };
        let ask = |ask: String| super::kept(Some(9140), &ask).unwrap();
        let a = ask(format!("replacing {doc}")).unwrap();
        replace("a");
        let b = ask(format!("replacing {doc}")).expect("the later save carries too");
        ask(format!("replaced {a}"));
        replace("b");
        ask(format!("replaced {b}"));
        let written: serde_json::Value =
            serde_json::from_slice(&std::fs::read(&set).unwrap()).unwrap();
        assert_eq!(
            written["documents"][0]["inode"],
            std::fs::metadata(&file).unwrap().ino()
        );
        kept::end(9140);
        exact_data::documents::forget(9140);
        std::fs::remove_dir_all(dir).unwrap();
    }
}

#[cfg(test)]
mod hold_tests {
    use super::{CompressionHold, Gate, SessionHold};
    use std::{
        sync::Arc,
        time::{Duration, Instant},
    };

    #[cfg(all(exact_js_engine, target_vendor = "apple"))]
    #[test]
    fn dropping_a_placed_worker_ends_its_storage_wait_without_starting_queued_work() {
        use crate::Module;
        use exact_plan::Value;
        use exact_runner::{Answer, DataSource, Dispatch, Placement, Reply, Store, Work};

        fn build(
            template: &Module,
            retired: Arc<std::sync::atomic::AtomicBool>,
        ) -> exact_data::placed::Obtain<Module> {
            let obtain = Module::build(template, retired);
            let extended = template.storage.as_ref().unwrap().extended.clone();
            Box::new(move || {
                let mut module = obtain()?;
                let session = module.storage.as_mut().unwrap();
                session.wait = Duration::ZERO;
                session.extended = extended;
                Ok(module)
            })
        }
        ibex2_sqlite::link();
        let grants: String = serde_json::from_str(
            include_str!("../tests/fixtures/storage.ts")
                .lines()
                .find_map(|line| line.strip_prefix("const grants = "))
                .unwrap()
                .trim_end_matches(';'),
        )
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("exact-worker-retirement-{}", std::process::id()));
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/storage.hbc")).to_vec(),
            "dev.exact.storage-test",
            &grants,
        );
        module.set_budget_ms(f64::INFINITY);
        module
            .configure_storage(root.join("data"), root.join("cache"), root.join("tmp"))
            .unwrap();
        module.bind(
            &contract::compile(
                r#"
shape Row
  text: string
component App
  mutation output as shape Row
  action invoke(op: string, value: string)
    send output = work(op, value)
  view
    text "storage"
"#,
            )
            .unwrap(),
        );
        module.load().unwrap();
        let (extended, observed) = std::sync::mpsc::channel();
        module.storage.as_mut().unwrap().extended = Some(extended);
        let database = rusqlite::Connection::open(root.join("data/notes.db")).unwrap();
        database
            .execute_batch("CREATE TABLE notes(body TEXT); BEGIN IMMEDIATE")
            .unwrap();
        let mut placed = exact_data::Placed::built(module, Placement::Worker, build);
        placed.activate().unwrap();
        let args = vec![Value::str("worker-retirement"), Value::str("orphan")];
        let mut store = Store::new(&grants, []);
        let Answer::Later(request) = placed.answer(&mut store, "work", &args).unwrap() else {
            panic!("a worker answers on its owner")
        };
        let Dispatch::Run(Work::Later(work)) =
            placed.dispatch(request.continuation.unwrap(), &store)
        else {
            panic!("a worker owns its storage wait")
        };
        let (sent, reply) = std::sync::mpsc::channel();
        work(Reply::new(move |outcome| {
            let _ = sent.send(outcome);
        }));
        let context = observed
            .recv_timeout(Duration::from_secs(30))
            .expect("the write waits on the held database");
        let next = vec![Value::str("timeout-queue"), Value::str("unqueued")];
        let Answer::Later(request) = placed.answer(&mut store, "work", &next).unwrap() else {
            panic!("a second turn queues behind the held write")
        };
        let Dispatch::Run(Work::Later(work)) =
            placed.dispatch(request.continuation.unwrap(), &store)
        else {
            panic!("a second turn runs on the owner")
        };
        let (sent, queued_reply) = std::sync::mpsc::channel();
        work(Reply::new(move |outcome| {
            let _ = sent.send(outcome);
        }));
        drop(placed);
        // Disconnection observes destruction of both the waiter and its session,
        // while the native write remains blocked by the database transaction.
        let retired = matches!(
            observed.recv_timeout(Duration::from_secs(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ) && !context.wait(Duration::ZERO);
        database.execute_batch("COMMIT").unwrap();
        reply
            .recv_timeout(Duration::from_secs(30))
            .expect("the retired turn releases its reply");
        let queued = queued_reply
            .recv_timeout(Duration::from_secs(30))
            .expect("the unstarted turn releases its reply");
        assert!(matches!(
            observed.recv_timeout(Duration::from_secs(30)),
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected)
        ));
        let orphan = root.join("data/song-orphan").exists();
        let reaction = root.join("data/song-reaction").exists();
        let unqueued = root.join("data/song").exists();
        assert!(
            context.wait(Duration::from_secs(30)),
            "the submitted write finishes without a VM"
        );
        let deadline = Instant::now() + Duration::from_secs(30);
        while context.in_flight() > 0 {
            assert!(
                Instant::now() < deadline,
                "the submitted write never finished"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let submitted: i64 = database
            .query_row(
                "SELECT COUNT(*) FROM notes WHERE body = 'submitted'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        drop(context);
        drop(database);
        std::fs::remove_dir_all(root).unwrap();
        assert!(retired, "the retired worker kept its waiter and VM alive");
        assert_eq!(submitted, 1, "the already-submitted write may still commit");
        assert!(
            !orphan && !reaction && !unqueued,
            "a retired worker issued queued storage"
        );
        assert!(matches!(
            queued,
            exact_runner::Outcome::Failed {
                kind: exact_runner::FailureKind::Aborted,
                ..
            }
        ));
    }

    #[cfg(all(exact_js_engine, target_vendor = "apple"))]
    #[test]
    fn a_worker_write_outlives_its_wait_after_an_image_was_revoked() {
        use crate::Module;
        use exact_plan::Value;
        use exact_runner::{Answer, DataSource, Dispatch, Outcome, Store, Work};

        let grants: String = serde_json::from_str(
            include_str!("../tests/fixtures/storage.ts")
                .lines()
                .find_map(|line| line.strip_prefix("const grants = "))
                .unwrap()
                .trim_end_matches(';'),
        )
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("exact-worker-storage-wait-{}", std::process::id()));
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/storage.hbc")).to_vec(),
            "dev.exact.storage-test",
            &grants,
        );
        module.main_thread = false;
        module.storage_wait = Duration::ZERO;
        module.set_budget_ms(f64::INFINITY);
        module
            .configure_storage(root.join("data"), root.join("cache"), root.join("tmp"))
            .unwrap();
        module.bind(
            &contract::compile(
                r#"
shape Row
  text: string
component App
  mutation output as shape Row
  action invoke(op: string, value: string)
    send output = work(op, value)
  view
    text "storage"
"#,
            )
            .unwrap(),
        );
        let hold = module.hold_compressions();
        module.activate().unwrap();
        std::fs::write(
            root.join("data/noise.bmp"),
            include_bytes!("../../scripts/fixtures/picker/oriented-gps.jpg"),
        )
        .unwrap();
        let mut store = Store::new(&grants, []);
        let args = |op: &str, value: &str| vec![Value::str(op), Value::str(value)];
        let run = |module: &mut Module, request: exact_runner::Request, store: &Store| {
            let Dispatch::Run(Work::Now(work)) =
                module.dispatch(request.continuation.unwrap(), store)
            else {
                panic!("the worker owns its storage waiter")
            };
            work
        };
        let image = args("compress-abandoned", "");
        let Answer::Later(request) = module.answer(&mut store, "work", &image).unwrap() else {
            panic!("the held image must await storage")
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        let held = module.storage.as_ref().unwrap().hold.as_ref().unwrap();
        while held.held() == 0 {
            assert!(
                Instant::now() < deadline,
                "the image never entered its hold"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let outcome = run(&mut module, request, &store)();
        assert!(format!("{outcome:?}").contains(super::IMAGE_ABANDONED));
        assert!(module.parse(&mut store, "work", &image, outcome).is_err());

        let write = args("timeout-queue", "first");
        let Answer::Later(request) = module.answer(&mut store, "work", &write).unwrap() else {
            panic!("the atomic write must await storage")
        };
        let context = module.storage.as_ref().unwrap().context.clone();
        // SAFETY: Context retains its state until the held completion is returned.
        let state = unsafe { &*context.state_ptr().cast::<ibex2::task::RuntimeState>() };
        assert!(context.wait(Duration::from_secs(30)));
        // Hold the write's real completion without delivering it to JavaScript.
        let completion = state.queue.take().unwrap();
        let (extended, observed) = std::sync::mpsc::channel();
        module.storage.as_mut().unwrap().extended = Some(extended);
        let waiter = std::thread::spawn(run(&mut module, request, &store));
        let kept = observed.recv_timeout(Duration::from_secs(30)).is_ok();
        state.queue.admit(completion);
        let outcome = waiter.join().unwrap();
        assert!(kept, "the write lost its waiter: {outcome:?}");
        assert!(matches!(outcome, Outcome::Response(_)), "{outcome:?}");
        let mut answer = module.parse(&mut store, "work", &write, outcome).unwrap();
        for _ in 0..8 {
            let Answer::Later(request) = answer else {
                break;
            };
            let outcome = run(&mut module, request, &store)();
            answer = module.parse(&mut store, "work", &write, outcome).unwrap();
        }
        assert!(matches!(answer, Answer::Now(_)), "{answer:?}");
        let next = args("timeout-queue", "successor");
        let mut answer = module.answer(&mut store, "work", &next).unwrap();
        for _ in 0..8 {
            let Answer::Later(request) = answer else {
                break;
            };
            let outcome = run(&mut module, request, &store)();
            answer = module.parse(&mut store, "work", &next, outcome).unwrap();
        }
        assert!(matches!(answer, Answer::Now(_)), "{answer:?}");
        assert_eq!(std::fs::read(root.join("data/song")).unwrap(), b"successor");
        assert_eq!(
            std::fs::read(root.join("data/song-orphan")).unwrap(),
            b"successor"
        );
        assert!(!root.join("data/song-reaction").exists());
        assert_eq!(module.background_state().unwrap().in_flight, 0);
        hold.release();
        let deadline = Instant::now() + Duration::from_secs(30);
        while context.in_flight() > 0 {
            assert!(
                Instant::now() < deadline,
                "the revoked image never returned"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        assert!(!root.join("data/slow.jpg").exists());
        drop(module);
        std::fs::remove_dir_all(root).unwrap();
    }

    /// A let-go chain whose wait runs out fails only the answer whose write is
    /// in flight. Another let-go answer's write queued behind it lands once
    /// that write completes, delivered by a background round.
    #[cfg(exact_js_engine)]
    #[test]
    fn a_let_go_timeout_keeps_another_let_go_answers_queued_write() {
        use crate::Module;
        use exact_plan::Value;
        use exact_runner::{Answer, DataSource, Dispatch, Store, Work, BACKGROUND};

        let grants: String = serde_json::from_str(
            include_str!("../tests/fixtures/storage.ts")
                .lines()
                .find_map(|line| line.strip_prefix("const grants = "))
                .unwrap()
                .trim_end_matches(';'),
        )
        .unwrap();
        let root =
            std::env::temp_dir().join(format!("exact-let-go-timeout-{}", std::process::id()));
        let mut module = Module::new(
            include_bytes!(concat!(env!("OUT_DIR"), "/storage.hbc")).to_vec(),
            "dev.exact.storage-test",
            &grants,
        );
        module.storage_wait = Duration::ZERO;
        module.set_budget_ms(f64::INFINITY);
        module
            .configure_storage(root.join("data"), root.join("cache"), root.join("tmp"))
            .unwrap();
        module.bind(
            &contract::compile(
                r#"
shape Row
  text: string
component App
  mutation output as shape Row
  action invoke(op: string, value: string)
    send output = work(op, value)
  view
    text "storage"
"#,
            )
            .unwrap(),
        );
        module.activate().unwrap();
        let mut store = Store::new(&grants, []);
        let append = |value: &str| vec![Value::str("append"), Value::str(value)];
        let later = |answer| matches!(answer, Ok(Answer::Later(_)));
        assert!(later(module.answer(&mut store, "work", &append("a"))));
        let context = module.storage.as_ref().unwrap().context.clone();
        // SAFETY: Context retains its state until the held completion is returned.
        let state = unsafe { &*context.state_ptr().cast::<ibex2::task::RuntimeState>() };
        assert!(context.wait(Duration::from_secs(30)));
        // `a`'s append has run; its completion is held from JavaScript, and
        // the work counts as in flight, as a write stalled in `sync` would.
        let completion = state.queue.take().unwrap();
        state.task_started();
        assert!(later(module.answer(&mut store, "work", &append("b"))));
        // The same calls again let both go: `b` while `a` is live, then `a`,
        // whose let-go chain's wait runs out on the held completion.
        assert!(later(module.answer(&mut store, "work", &append("b"))));
        assert!(later(module.answer(&mut store, "work", &append("a"))));
        let log = || std::fs::read_to_string(root.join("data/log")).unwrap_or_default();
        assert_eq!(log(), "a;");
        state.queue.admit(completion);
        state.task_finished();
        let mut next = DataSource::background(&mut module, &store);
        for _ in 0..8 {
            let Some(_) = next else { break };
            let Dispatch::Run(Work::Now(work)) = module.dispatch(BACKGROUND, &store) else {
                panic!("a background round runs on the host's worker")
            };
            let outcome = std::thread::spawn(work).join().unwrap();
            next = module
                .background_landed(&store, outcome)
                .unwrap()
                .or_else(|| DataSource::background(&mut module, &store));
        }
        // The live `b`'s append is not a background round; wait for its write.
        assert!(context.wait(Duration::from_secs(30)));
        assert_eq!(log(), "a;b;b;", "the let-go `b`'s append was dropped");
        drop(module);
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn uncancellable_work_keeps_a_waiter_until_completion_or_unload() {
        use exact_runner::{FailureKind, Outcome};
        for unload in [false, true] {
            let mut session = super::Session::open("", Duration::ZERO, 0, None).unwrap();
            let context = session.context.clone();
            // SAFETY: Context owns this state, retained here until the task ends.
            let state = unsafe { &*context.state_ptr().cast::<ibex2::task::RuntimeState>() };
            state.task_started();
            let outcome = session.wait_for(None, true, None)();
            assert!(matches!(outcome, Outcome::Failed { message, .. }
                if message == "storage continuation timed out"));

            let (tx, rx) = std::sync::mpsc::channel();
            session.extended = Some(tx);
            let work = session.wait_for(None, false, None);
            session.extended = None;
            let waiter = std::thread::spawn(work);
            rx.recv_timeout(Duration::from_secs(30))
                .expect("the module must keep waiting past the answer's deadline");
            if unload {
                drop(session);
            } else {
                state
                    .queue
                    .complete(1, Ok(ibex2::boundary::HostValue::Undefined));
                state.task_finished();
            }
            let outcome = waiter.join().unwrap();
            if unload {
                state.task_finished();
                assert!(
                    matches!(outcome, Outcome::Failed { kind: FailureKind::Aborted, message }
                    if message == "storage runtime was unloaded")
                );
            } else {
                assert!(matches!(outcome, Outcome::Response(_)), "{outcome:?}");
            }
        }
    }

    /// A compression an earlier session left held counts for no later one,
    /// whose wait it would otherwise end (a module reloaded under a hold).
    #[test]
    fn a_hold_counts_each_session_s_compressions_apart() {
        let gate = Arc::new(Gate::default());
        let (old, new) = (
            SessionHold::new(gate.clone()),
            SessionHold::new(gate.clone()),
        );
        let held = std::thread::spawn({
            let old = old.clone();
            move || old.enter()
        });
        let hang = Instant::now() + Duration::from_secs(30);
        while old.held() == 0 {
            assert!(
                Instant::now() < hang,
                "the compression never entered the hold"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        assert_eq!(new.held(), 0);
        CompressionHold(gate).release();
        held.join().unwrap();
        assert_eq!(old.held(), 0);
    }
}
