//! File destinations over the portable adapter, including physical release.
use super::super::super::{
    tests::body_to::{download, physically_ended, serving_on, Files, GRANTS},
    tests::{collect, job},
    Core, State,
};
use super::*;
use std::sync::{
    mpsc::{channel, Receiver},
    Condvar,
};

const HANG: Duration = Duration::from_secs(30);

struct Gate {
    open: Arc<(Mutex<bool>, Condvar)>,
    entered: Receiver<Run>,
}
impl Gate {
    fn at(phase: Phase, ticket: u64) -> (Self, Hook) {
        let open = Arc::new((Mutex::new(false), Condvar::new()));
        let (send, entered) = channel();
        let waiting = open.clone();
        let once = AtomicBool::new(false);
        let hook = Arc::new(move |at, run: Run| {
            if at == phase && run.0 == ticket && !once.swap(true, Ordering::AcqRel) {
                send.send(run).unwrap();
                let (lock, changed) = &*waiting;
                let guard = lock.lock().unwrap();
                let (guard, _) = changed
                    .wait_timeout_while(guard, HANG, |open| !*open)
                    .unwrap();
                assert!(*guard, "held file operation was not released");
            }
        });
        (Self { open, entered }, hook)
    }
    fn wait(&self) -> Run {
        self.entered
            .recv_timeout(HANG)
            .expect("file operation did not start")
    }
    fn release(&self) {
        *self.open.0.lock().unwrap() = true;
        self.open.1.notify_all();
    }
}
impl Drop for Gate {
    fn drop(&mut self) {
        self.release();
    }
}

fn wait(core: &Core, until: impl Fn(&State) -> bool) {
    let state = core.shared.state.lock().unwrap();
    let (state, _) = core
        .shared
        .ready
        .wait_timeout_while(state, HANG, |s| !until(s))
        .unwrap();
    assert!(until(&state), "machine condition did not hold");
}

fn unchanged(files: &Files) {
    assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
    assert_eq!(files.cache(), ["a"]);
}

#[test]
fn a_stalled_write_and_cleanup_keep_the_charge_origin_and_lane_place() {
    for independent in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (core, _, woke) = serving_on(GRANTS, true);
        core.set_app_roots(files.roots());
        let (writing, write_hook) = Gate::at(Phase::Write, 1);
        let (cleanup, clean_hook) = Gate::at(Phase::Cleaned, 1);
        *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() =
            Some(Arc::new(move |p, r| {
                write_hook(p, r);
                clean_hook(p, r);
            }));
        let request = download("/bytes/100", "app:/cache/a");
        let request = if independent {
            request.independent_http(1024)
        } else {
            request
        };
        core.run(job(1, request), None).unwrap();
        let run = writing.wait();
        let lane = usize::from(independent);
        let charge = core.shared.state.lock().unwrap().bytes[lane];
        core.run(job(2, Request::get("https://example.test/bytes/1")), None)
            .unwrap();
        core.cancel(&[1]);
        wait(&core, |s| {
            s.running
                .iter()
                .any(|r| r.ticket == 1 && r.terminal && r.drained)
        });
        let held = || {
            let state = core.shared.state.lock().unwrap();
            assert!(state.bytes[lane] >= charge);
            assert!(
                state.counts[lane] >= 1,
                "file admission place released before cleanup"
            );
            assert_eq!(state.book.origins[lane].values().sum::<usize>(), 1);
            assert_eq!(state.book.in_flight, usize::from(independent));
            assert!(state
                .running
                .iter()
                .any(|r| (r.ticket, r.generation) == run));
            if !independent {
                assert!(state.book.blocking);
                assert_eq!(state.jobs[0].front().unwrap().ticket, 2);
            }
        };
        held();
        writing.release();
        cleanup.wait();
        held();
        unchanged(&files);
        cleanup.release();
        assert_eq!(collect(&core, &woke, 1)[0].0, 2);
        physically_ended(&core);
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.bytes, [0, 0]);
        assert_eq!(state.counts, [0, 0]);
        assert_eq!(state.book.in_flight, 0);
        assert!(state.book.origins.iter().all(|o| o.is_empty()));
        assert!(state.book.downloads.is_empty());
        assert!(state.book.download_ends.is_empty());
    }
}

#[test]
fn an_abort_or_deadline_before_publish_leaves_no_file_or_contact_after_preparation() {
    for phase in [Phase::Prepare, Phase::Publish] {
        for deadline in [false, true] {
            let files = Files::new();
            std::fs::write(files.0.join("cache/a"), b"old").unwrap();
            let (core, opened, woke) = serving_on(GRANTS, true);
            core.set_app_roots(files.roots());
            let (held, hook) = Gate::at(phase, 1);
            *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() = Some(hook);
            core.run(
                job(1, download("/bytes/100", "app:/cache/a").timeout(60_000)),
                None,
            )
            .unwrap();
            let run = held.wait();
            if deadline {
                core.machine.as_ref().unwrap().timer.expire(run);
                let done = collect(&core, &woke, 1);
                assert!(
                    matches!(
                        &done[0].1,
                        Outcome::Failed {
                            kind: FailureKind::Timeout,
                            ..
                        }
                    ),
                    "{done:?}"
                );
            } else {
                core.cancel(&[1]);
                wait(&core, |s| {
                    s.running.iter().any(|r| r.ticket == 1 && r.terminal)
                });
            }
            assert_eq!(
                core.shared.state.lock().unwrap().counts[0],
                1,
                "terminal file request still owns its admission place"
            );
            held.release();
            physically_ended(&core);
            unchanged(&files);
            assert_eq!(
                opened.lock().unwrap().len(),
                usize::from(phase == Phase::Publish)
            );
            assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
        }
    }
}

#[test]
fn a_rename_wins_over_an_abort_or_deadline_that_arrives_after_it() {
    let files = Files::new();
    let (core, _, woke) = serving_on(GRANTS, true);
    core.set_app_roots(files.roots());
    let (renamed, hook) = Gate::at(Phase::Renamed, 1);
    *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() = Some(hook);
    core.run(
        job(1, download("/bytes/100", "app:/cache/a").timeout(60_000)),
        None,
    )
    .unwrap();
    let run = renamed.wait();
    core.machine.as_ref().unwrap().timer.expire(run);
    let abort = core.shared.state.lock().unwrap().running[0]
        .exchange
        .clone()
        .unwrap();
    abort.abort();
    renamed.release();
    let done = collect(&core, &woke, 1);
    assert!(
        matches!(&done[0].1, Outcome::Response(r) if r.body_written() == Some(100)),
        "{done:?}"
    );
    physically_ended(&core);
    assert_eq!(files.cache(), ["a"]);
}

#[test]
fn a_forgotten_safe_request_or_retirement_cleans_up_a_stalled_file_body() {
    for retire in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (mut core, _, woke) = serving_on(GRANTS, true);
        core.set_app_roots(files.roots());
        let (held, hook) = Gate::at(Phase::Write, 1);
        let (cleaned, clean_hook) = Gate::at(Phase::Cleaned, 1);
        cleaned.release();
        *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() =
            Some(Arc::new(move |p, r| {
                hook(p, r);
                clean_hook(p, r);
            }));
        core.run(job(1, download("/bytes/100", "app:/cache/a")), None)
            .unwrap();
        held.wait();
        if retire {
            core.retire();
            assert_ne!(core.shared.state.lock().unwrap().bytes, [0, 0]);
            held.release();
            cleaned.wait();
            physically_ended(&core);
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.bytes, [0, 0]);
            assert_eq!(state.counts, [0, 0]);
            assert!(state.book.origins.iter().all(HashMap::is_empty));
            assert!(!state.book.blocking);
            assert!(woke.try_recv().is_err());
        } else {
            core.forget(|_| false);
            held.release();
            physically_ended(&core);
            assert_eq!(core.shared.state.lock().unwrap().bytes, [0, 0]);
        }
        assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
        let mut names = files.cache();
        // Retirement also prunes the process registry, which keeps its lock file.
        names.retain(|name| name != "exact-children.lock");
        assert_eq!(names, ["a"]);
    }
}

#[test]
fn a_redirect_writes_only_the_last_hop() {
    let files = Files::new();
    let (core, opened, woke) = serving_on(GRANTS, true);
    core.set_app_roots(files.roots());
    core.run(job(1, download("/redirect", "app:/cache/a")), None)
        .unwrap();
    let done = collect(&core, &woke, 1);
    assert!(
        matches!(&done[0].1, Outcome::Response(r) if r.body_written() == Some(17)),
        "{done:?}"
    );
    physically_ended(&core);
    assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap().len(), 17);
    assert_eq!(files.cache(), ["a"]);
    assert_eq!(
        *opened.lock().unwrap(),
        [
            "https://example.test/redirect",
            "https://example.test/bytes/17"
        ]
    );
}

#[test]
fn two_independent_file_bodies_write_their_own_files_while_both_are_in_flight() {
    let files = Files::new();
    let (core, _, woke) = serving_on(GRANTS, true);
    core.set_app_roots(files.roots());
    let (first, first_hook) = Gate::at(Phase::Write, 1);
    let (second, second_hook) = Gate::at(Phase::Write, 2);
    *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() = Some(Arc::new(move |p, r| {
        first_hook(p, r);
        second_hook(p, r);
    }));
    for (ticket, path) in [(1, "app:/cache/a"), (2, "app:/cache/b")] {
        core.run(
            job(
                ticket,
                download(&format!("/bytes/{}", ticket * 10), path).independent_http(1024),
            ),
            None,
        )
        .unwrap();
    }
    first.wait();
    second.wait();
    assert_eq!(core.shared.state.lock().unwrap().book.in_flight, 2);
    first.release();
    second.release();
    for (ticket, outcome) in collect(&core, &woke, 2) {
        assert!(matches!(outcome, Outcome::Response(r) if r.body_written() == Some(ticket * 10)));
    }
    physically_ended(&core);
    assert_eq!(files.cache(), ["a", "b"]);
    for (name, size) in [("a", 10), ("b", 20)] {
        let bytes = std::fs::read(files.0.join("cache").join(name)).unwrap();
        assert_eq!(bytes.len(), size);
        assert!(bytes.iter().enumerate().all(|(i, b)| *b == (i * 31) as u8));
    }
}

#[test]
fn a_grant_refusal_on_the_first_or_redirected_hop_cleans_up_the_destination() {
    for redirected in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (core, opened, woke) = serving_on(GRANTS, true);
        core.set_app_roots(files.roots());
        let request = if redirected {
            download("/redirect-denied", "app:/cache/a")
        } else {
            Request::get("https://denied.test/").body_to("app:/cache/a")
        };
        core.run(job(1, request), None).unwrap();
        assert!(matches!(
            &collect(&core, &woke, 1)[0].1,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        physically_ended(&core);
        unchanged(&files);
        assert_eq!(opened.lock().unwrap().len(), usize::from(redirected));
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.bytes, [0, 0]);
        assert!(state.book.origins.iter().all(|o| o.is_empty()));
    }
}

#[test]
fn a_completed_reply_forgotten_during_cleanup_keeps_its_admission_place() {
    let files = Files::new();
    let (core, _, _) = serving_on(GRANTS, true);
    core.set_app_roots(files.roots());
    let (cleanup, hook) = Gate::at(Phase::Cleaned, 1);
    *core.machine.as_ref().unwrap().body_to_hook.lock().unwrap() = Some(hook);
    core.run(
        job(
            1,
            download("/bytes/100", "app:/cache/a").independent_http(1024),
        ),
        None,
    )
    .unwrap();
    cleanup.wait();
    wait(&core, |s| !s.completed[1].is_empty());
    core.forget(|_| false);
    {
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.counts[1], 1);
        assert!(state.bytes[1] > 0);
        assert_eq!(state.book.in_flight, 1);
    }
    cleanup.release();
    physically_ended(&core);
    let state = core.shared.state.lock().unwrap();
    assert_eq!(state.counts, [0, 0]);
    assert_eq!(state.bytes, [0, 0]);
    assert_eq!(state.book.in_flight, 0);
    assert_eq!(files.cache(), ["a"]);
    assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap().len(), 100);
}
