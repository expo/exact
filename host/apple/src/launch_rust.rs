//! Rust data capabilities before the engine exists (LLP 1162.000 D2, D3).
use super::*;
use crate::executor::PreloadEvent;
use exact_data::secrets::Backend;
use exact_runner::request::{LaunchContext, LaunchRequest};
use exact_runner::{Request, Response};
use std::sync::Condvar;
use std::time::{Duration, Instant};

// Bound token refreshes and reads naming unknown preloads (LLP 1162.000 D2).
pub(super) const LAUNCH_DEADLINE: Duration = Duration::from_secs(5);

pub(super) struct Running {
    started: Instant,
    done: Mutex<bool>,
    changed: Condvar,
    fetch_abort: ibex2::stdlib::abort::AbortController,
    name: &'static str,
    launch: std::sync::Weak<Launch>,
    #[cfg(test)]
    returned: (Mutex<bool>, Condvar),
    #[cfg(test)]
    clock: Mutex<Option<Arc<dyn Fn() -> Instant + Send + Sync>>>,
}
impl Running {
    fn now(&self) -> Instant {
        #[cfg(test)]
        if let Some(clock) = self.clock.lock().unwrap().as_ref() {
            return clock();
        }
        Instant::now()
    }
    #[cfg(test)]
    pub(super) fn test_clock(&self, clock: Arc<dyn Fn() -> Instant + Send + Sync>) {
        *self.clock.lock().unwrap() = Some(clock);
    }
    #[cfg(test)]
    pub(super) fn expire_now(&self) {
        self.expire(self.started + LAUNCH_DEADLINE);
    }
    #[cfg(test)]
    pub(super) fn wait_returned(&self) {
        let (returned, timeout) = self
            .returned
            .1
            .wait_timeout_while(
                self.returned.0.lock().unwrap(),
                Duration::from_secs(10),
                |returned| !*returned,
            )
            .unwrap();
        assert!(!timeout.timed_out() && *returned, "launch did not return");
    }

    fn record(&self, launch: &LaunchPreloads, outcome: PreloadEvent) {
        launch.0.journal.lock().unwrap().push(PreloadFact {
            name: self.name.into(),
            origin: String::new(),
            outcome,
        });
    }
    pub(super) fn expire(&self, now: Instant) {
        let mut done = self.done.lock().unwrap();
        if !*done && now.saturating_duration_since(self.started) >= LAUNCH_DEADLINE {
            *done = true;
            self.fetch_abort.abort();
            if let Some(launch) = self.launch.upgrade().map(LaunchPreloads) {
                self.record(
                    &launch,
                    PreloadEvent::LaunchDeadline {
                        elapsed_ms: now.saturating_duration_since(self.started).as_millis() as u64,
                    },
                );
                launch.0.set.get().unwrap().control.launch_pending(None);
            }
            self.changed.notify_all();
        }
    }
}

pub(super) struct Prepared {
    pub preload: Preload,
    pub grants: String,
    pub versions: BTreeMap<String, u64>,
}

pub(super) fn origin(request: &Request) -> String {
    url::Url::parse(&request.url)
        .map(|u| u.origin().ascii_serialization())
        .unwrap_or_default()
}

pub(super) fn run(
    launch: &LaunchPreloads,
    entry: exact_runner::request::Launch,
    app: &str,
    grants: &str,
    definitions: &[exact_runner::request::Preload],
) {
    let running = Arc::new(Running {
        started: Instant::now(),
        done: Mutex::new(false),
        changed: Condvar::new(),
        fetch_abort: ibex2::stdlib::abort::AbortController::new(),
        name: entry.name,
        launch: Arc::downgrade(&launch.0),
        #[cfg(test)]
        returned: Default::default(),
        #[cfg(test)]
        clock: Default::default(),
    });
    #[cfg(test)]
    struct Returned(Arc<Running>);
    #[cfg(test)]
    impl Drop for Returned {
        fn drop(&mut self) {
            *self.0.returned.0.lock().unwrap() = true;
            self.0.returned.1.notify_all();
        }
    }
    #[cfg(test)]
    let _returned = Returned(running.clone());
    let _ = launch.0.rust.set(running.clone());
    launch
        .0
        .set
        .get()
        .unwrap()
        .control
        .launch_pending(Some(LAUNCH_DEADLINE));
    let watch = running.clone();
    if std::thread::Builder::new()
        .name("exact-launch-deadline".into())
        .spawn(move || {
            let done = watch.done.lock().unwrap();
            let (done, _) = watch
                .changed
                .wait_timeout_while(
                    done,
                    (watch.started + LAUNCH_DEADLINE).saturating_duration_since(Instant::now()),
                    |done| !*done,
                )
                .unwrap();
            let expired = !*done;
            drop(done);
            if expired {
                watch.expire(watch.now());
            }
        })
        .is_err()
    {
        running.expire(running.started + LAUNCH_DEADLINE);
        return;
    }
    let mut context = Context::new(launch, app, grants, running.clone(), definitions);
    let result =
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| (entry.run)(&mut context)));
    running.expire(running.now());
    let mut done = running.done.lock().unwrap();
    if result.is_err() {
        launch
            .0
            .foreground
            .lock()
            .unwrap()
            .retain(|d| !context.accepted.contains(&d.preload.name));
        launch
            .0
            .set
            .get()
            .unwrap()
            .control
            .cancel_names(&context.accepted);
    }
    if *done {
        return;
    }
    *done = true;
    running.changed.notify_all();
    let elapsed_ms = running.started.elapsed().as_millis() as u64;
    running.record(
        launch,
        match result {
            Ok(()) => PreloadEvent::LaunchRan {
                elapsed_ms,
                requests: context.requests,
            },
            Err(_) => PreloadEvent::LaunchPanicked { elapsed_ms },
        },
    );
}

struct Context<'a> {
    launch: &'a LaunchPreloads,
    grants: &'a str,
    backend: Option<Arc<dyn Backend>>,
    snapshot: BTreeMap<String, String>,
    versions: BTreeMap<String, u64>,
    allowed: std::collections::BTreeSet<String>,
    guarded: std::collections::BTreeSet<String>,
    used: BTreeMap<String, u64>,
    failed_writes: std::collections::BTreeSet<String>,
    running: Arc<Running>,
    names: std::collections::BTreeSet<String>,
    accepted: Vec<String>,
    requests: usize,
    manifest_count: usize,
}
impl<'a> Context<'a> {
    fn new(
        launch: &'a LaunchPreloads,
        app: &str,
        grants: &'a str,
        running: Arc<Running>,
        definitions: &[exact_runner::request::Preload],
    ) -> Self {
        let backend = || {
            crate::store::launch_backend(grants, app)
                .ok()
                .map(|b| Arc::new(b) as Arc<dyn Backend>)
        };
        #[cfg(test)]
        let backend = launch.0.backend.lock().unwrap().clone().or_else(backend);
        #[cfg(not(test))]
        let backend = backend();
        let policies = crate::presence_secrets::lines(grants);
        let allowed: std::collections::BTreeSet<_> =
            policies.iter().map(|(name, _)| name.clone()).collect();
        let guarded: std::collections::BTreeSet<_> = policies
            .into_iter()
            .filter(|(_, policy)| policy.is_some())
            .map(|(name, _)| name)
            .collect();
        // Each value must match its version for compare-and-set (LLP 1162.000 D2).
        let (versions, snapshot) = {
            let versions = launch.0.invalidated.lock().unwrap();
            let snapshot = allowed
                .iter()
                .filter(|name| !guarded.contains(*name))
                .filter_map(|name| {
                    backend
                        .as_ref()?
                        .read(name)
                        .ok()
                        .flatten()
                        .map(|value| (name.clone(), value))
                })
                .collect();
            (versions.clone(), snapshot)
        };
        Self {
            launch,
            grants,
            backend,
            snapshot,
            versions,
            allowed,
            guarded,
            used: BTreeMap::new(),
            failed_writes: Default::default(),
            running,
            names: definitions.iter().map(|d| d.name.into()).collect(),
            accepted: Vec::new(),
            requests: 0,
            manifest_count: definitions.len(),
        }
    }
    fn write(&mut self, name: &str, value: Option<&str>) -> Result<(), String> {
        if !self.allowed.contains(name) || self.guarded.contains(name) {
            return Err("launch secret refused".into());
        }
        if *self.running.done.lock().unwrap() {
            return Err("launch deadline".into());
        }
        if self.failed_writes.contains(name) {
            return Err("launch secret write failed".into());
        }
        let backend = self.backend.clone().ok_or("launch secrets unavailable")?;
        let expected = self.versions.get(name).copied().unwrap_or(0);
        self.launch.with_secret_version(name, expected, |version| {
            if let Err(error) = backend.write(name, value) {
                // Persistence can fail after changing storage, and the function holds the
                // value it tried to write: what it hands over next depends on it (LLP 1162.000 D3).
                self.failed_writes.insert(name.into());
                self.used.insert(name.into(), expected);
                return Err(error);
            }
            self.launch.store_written();
            self.versions.insert(name.into(), version);
            self.used.insert(name.into(), version);
            match value {
                Some(value) => {
                    self.snapshot.insert(name.into(), value.into());
                }
                None => {
                    self.snapshot.remove(name);
                }
            }
            Ok(())
        })
    }
}
impl LaunchContext for Context<'_> {
    fn secret(&mut self, name: &str) -> Option<String> {
        if !self.allowed.contains(name) {
            return None;
        }
        self.used
            .insert(name.into(), self.versions.get(name).copied().unwrap_or(0));
        self.snapshot.get(name).cloned()
    }
    fn set_secret(&mut self, name: &str, value: &str) -> Result<(), String> {
        self.write(name, Some(value))
    }
    fn forget_secret(&mut self, name: &str) -> Result<(), String> {
        self.write(name, None)
    }
    fn preload(
        &mut self,
        LaunchRequest {
            name,
            request,
            when,
        }: LaunchRequest,
    ) -> Result<(), String> {
        self.requests += 1;
        self.running.expire(self.running.now());
        let done = self.running.done.lock().unwrap();
        let versions = self.launch.0.invalidated.lock().unwrap();
        let valid_name = name.as_bytes().first().is_some_and(u8::is_ascii_lowercase)
            && name
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'-');
        let refusal = if !valid_name {
            Some((PreloadEvent::InvalidName, "invalid launch preload name"))
        } else if *done
            || self
                .running
                .now()
                .saturating_duration_since(self.running.started)
                >= LAUNCH_DEADLINE
        {
            Some((PreloadEvent::Refused, "launch deadline"))
        } else if !self.names.insert(name.clone()) || self.manifest_count + self.accepted.len() >= 8
        {
            Some((PreloadEvent::Refused, "launch preload name or limit"))
        } else if url::Url::parse(&request.url).is_err() {
            Some((PreloadEvent::InvalidUrl, "invalid launch preload URL"))
        } else if request.headers.iter().any(|(_, value)| {
            !ibex2::stdlib::fetch::is_valid_value(value)
                || ibex2::stdlib::fetch::normalize_value(value) != value
        }) {
            Some((PreloadEvent::InvalidHeader, "invalid launch preload header"))
        } else if !crate::executor::valid_request(&request) {
            Some((PreloadEvent::Refused, "invalid launch preload request"))
        } else if exact_runner::grants::admitting_request(self.grants, &request).is_none() {
            Some((PreloadEvent::RefusedGrant, "launch preload grant refused"))
        } else if self
            .used
            .keys()
            .any(|name| self.failed_writes.contains(name))
        {
            Some((PreloadEvent::DroppedBySecret, "launch secret write failed"))
        } else if self
            .used
            .iter()
            .any(|(name, version)| versions.get(name).copied().unwrap_or(0) != *version)
        {
            Some((
                PreloadEvent::DroppedBySecret,
                "launch preload secret changed",
            ))
        } else {
            None
        };
        if let Some((outcome, reason)) = refusal {
            if valid_name {
                self.launch.0.set.get().unwrap().control.defined(&name);
                self.launch.0.journal.lock().unwrap().push(PreloadFact {
                    name,
                    origin: origin(&request),
                    outcome,
                });
            } else {
                self.running.record(self.launch, outcome);
            }
            return Err(reason.into());
        }
        self.launch
            .0
            .marks
            .lock()
            .unwrap()
            .entries
            .push((name.clone(), LaunchMark::default()));
        self.accepted.push(name.clone());
        let preload = Preload {
            name,
            deadline_ms: request.timeout_ms,
            request,
            secrets: self.used.keys().cloned().collect(),
        };
        if when == PreloadWhen::Foreground
            && self.launch.0.phase.load(Ordering::Acquire) & OPENED == 0
        {
            self.launch
                .0
                .set
                .get()
                .unwrap()
                .control
                .defined(&preload.name);
            self.launch.0.foreground.lock().unwrap().push(Prepared {
                preload,
                grants: self.grants.into(),
                versions: self.used.clone(),
            });
        } else {
            self.launch.extend_rust(preload, self.grants);
        }
        Ok(())
    }
    fn fetch(&mut self, request: Request) -> Result<Response, String> {
        self.running.expire(self.running.now());
        if *self.running.done.lock().unwrap() {
            return Err("launch deadline".into());
        }
        let result = crate::executor::launch_fetch(
            request,
            self.grants,
            ibex2::host::Host::new(),
            &self.running.fetch_abort.signal(),
        );
        if *self.running.done.lock().unwrap() {
            Err("launch deadline".into())
        } else {
            result
        }
    }
}
