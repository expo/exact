//! The snapshot entry's refresh (LLP 1126 D3); its slots are the runner's
//! (`exact_runner::entry::slots`).
pub(crate) use exact_runner::entry::slots;

/// How long a refresh waits for a watch import that overlaps its store read:
/// `IMPORT_WAITS` reads, `IMPORT_WAIT` apart.
const IMPORT_WAIT: std::time::Duration = std::time::Duration::from_millis(20);
const IMPORT_WAITS: u32 = 5;

impl<D: exact_runner::DataSource> crate::Host<D> {
    /// `configuration` is the placement's chosen values, JSON, which a
    /// configured widget's refresh receives (`exact_runner::entry::refresh_args`).
    pub(crate) fn entry_timeline(
        &mut self,
        now: f64,
        configuration: Option<&[u8]>,
        io: Option<crate::link::IoLinks>,
        module: Option<(&str, &[u8])>,
        deadline: std::time::Instant,
    ) -> Result<String, String> {
        use std::time::Instant;
        if self.runner.plan().snapshot_entry.is_empty() {
            return Err("not a snapshot entry".into());
        }
        if Instant::now() >= deadline {
            return Err("widget refresh timed out".into());
        }
        let plan = self.runner.plan().encode();
        let kind = self.runner.plan().snapshot_entry.clone();
        let data = self.runner.data();
        data.prepare_snapshot(
            module.map(|(receipt, bytecode)| exact_runner::SnapshotModule {
                receipt,
                bytecode,
                plan: &plan,
                kind: &kind,
            }),
            deadline,
        )
        .map_err(|e| format!("snapshot module: {e:?}"))?;
        let app = data.app_id().to_owned();
        let grants = data.grants().to_owned();
        let roots = crate::picker::app_dirs(&app)
            .map_err(|e| format!("snapshot storage: {e:?}"))?
            .map(|(roots, _)| roots);
        if let Some([data_root, cache, temporary]) = roots.clone() {
            data.configure_storage(data_root, cache, temporary)
                .map_err(|e| format!("snapshot storage: {e:?}"))?;
        }
        let interrupt = data.interrupt();
        let (cancel, wait) = std::sync::mpsc::channel();
        let watchdog = interrupt.map(|interrupt| {
            std::thread::spawn(move || {
                if wait
                    .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                    .is_err()
                {
                    interrupt.trigger();
                }
            })
        });
        let result = (|| {
            let endowed = io
                .map(|io| (io.endow)(&grants, &app, false))
                .unwrap_or_else(crate::store::Endowed::none);
            if let Some(error) = endowed.unbound {
                return Err(format!("{error}; every request is refused"));
            }
            // The watch app's import of a phone snapshot can overlap this read. It
            // takes milliseconds, so wait for it briefly; a refresh refused here
            // shows no account, and the import's end reloads the timeline.
            let mut waits = 0;
            let snapshot = loop {
                let read = io.map(|io| (io.snapshot)(endowed.bindings.as_ref(), &app));
                match read.unwrap_or_else(|| Ok(Vec::new())) {
                    Ok(snapshot) => break snapshot,
                    Err(_) if waits < IMPORT_WAITS && Instant::now() + IMPORT_WAIT < deadline => {
                        waits += 1;
                        std::thread::sleep(IMPORT_WAIT);
                    }
                    Err(error) => return Err(error),
                }
            };
            let wake = Box::new(std::thread::current());
            #[allow(unsafe_code)]
            extern "C" fn notify(ctx: *mut std::ffi::c_void) {
                // Executor retirement fences callbacks before the box is freed.
                unsafe { &*(ctx as *const std::thread::Thread) }.unpark();
            }
            let mut executor = None;
            let mut bindings = endowed.bindings;
            let result = self.runner.snapshot_timeline(
                now,
                configuration,
                snapshot,
                deadline,
                |mut request, work| {
                    let remaining = deadline.saturating_duration_since(Instant::now());
                    if remaining.is_zero() {
                        return Err("widget refresh timed out".into());
                    }
                    let io = io.ok_or("this build links no I/O; every request is refused")?;
                    let executor = executor.get_or_insert_with(|| {
                        let executor = (io.start)(
                            bindings.take(),
                            &grants,
                            Some((
                                notify,
                                (&*wake as *const std::thread::Thread).cast_mut().cast(),
                            )),
                        );
                        if let Some(roots) = roots.clone() {
                            executor.set_app_roots(roots);
                        }
                        executor
                    });
                    if request.continuation.is_none() {
                        let ms = remaining.as_millis().max(1).min(u32::MAX as u128) as u32;
                        request.timeout_ms = Some(request.timeout_ms.map_or(ms, |own| own.min(ms)));
                    }
                    let class = if work.is_none() && request.is_plain_http() {
                        exact_runner::RequestClass::Read
                    } else {
                        exact_runner::RequestClass::Ordered
                    };
                    executor.run(
                        exact_runner::RequestOut {
                            completion: None,
                            class,
                            ticket: 1,
                            target: "widget refresh".into(),
                            request,
                            forced: false,
                        },
                        work,
                    )?;
                    loop {
                        executor.begin_pump();
                        if let Some((_, outcome, _)) = executor.drain().into_iter().next() {
                            return Ok(outcome);
                        }
                        let remaining = deadline.saturating_duration_since(Instant::now());
                        if remaining.is_zero() {
                            executor.forget(&|_| false, false);
                            return Err("widget refresh timed out".into());
                        }
                        std::thread::park_timeout(remaining);
                    }
                },
            );
            drop(executor);
            drop(wake);
            result
        })();
        let _ = cancel.send(());
        if let Some(watchdog) = watchdog {
            let _ = watchdog.join();
        }
        result
    }
}
