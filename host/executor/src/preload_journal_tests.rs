use super::*;

#[test]
fn late_launch_facts_drain_from_adopted_and_prefetch_created_controls() {
    for asynchronous in [false, true] {
        for adopted in [false, true] {
            let server = Server::new();
            let (core, woke) = server.core(asynchronous, true);
            if adopted {
                adopt(&core, launch(&server, Vec::new()));
            }
            let release = held_plan(&server, "/ahead", b"prefetched");
            core.run(job(1, server.request("/ahead").prefetch()), None)
                .unwrap();
            server.seen(1);
            let control = core.preloads.lock().unwrap().as_ref().unwrap().control();
            assert!(control.take_journal().is_empty());
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"prefetched"));
            assert!(control.take_journal().is_empty());

            let release = held_plan(&server, "/late", b"launched");
            control.extend(
                vec![definition(&server, "/late")],
                &core.grants,
                Arc::new(|_, _| {}),
            );
            server.seen(2);
            let facts = control.take_journal();
            assert_eq!(facts.len(), 1);
            assert_eq!((&*facts[0].name, facts[0].outcome), ("late", Event::Sent));
            core.run(job(2, server.request("/late").prefetch()), None)
                .unwrap();
            release.send(()).unwrap();
            let (_, outcome, timing) = collect(&core, &woke);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"launched"));
            let facts = control.take_journal();
            assert_eq!(facts.len(), 1);
            assert_eq!((&*facts[0].name, facts[0].outcome), ("late", Event::Taken));
            assert!(control.take_journal().is_empty());

            update(&control.destination, close);
            let facts = control.take_journal();
            assert_eq!(facts.len(), 1);
            assert_eq!(
                (&*facts[0].name, facts[0].outcome),
                ("late", Event::Cancelled)
            );
            assert!(control.take_journal().is_empty());
            assert_eq!(server.count(), 2);
        }
    }
}

#[test]
fn launch_reaches_the_server_before_an_executor_and_both_cores_take_it() {
    for asynchronous in [false, true] {
        let server = Server::new();
        let release = held_plan(&server, "/early", b"early");
        let set = launch(&server, vec![definition(&server, "/early")]);
        server.seen(1);
        let control = set.control();
        assert_eq!(control.take_journal()[0].outcome, Event::Sent);
        let (core, woke) = server.core(asynchronous, true);
        adopt(&core, set);
        for ticket in 1..=2 {
            core.run(job(ticket, server.request("/early")), None)
                .unwrap();
        }
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(effect_count(&state), 0);
            assert!(state.running.is_empty());
            assert!(state.completed[0].is_empty());
        }
        release.send(()).unwrap();
        for expected in 1..=2 {
            let (ticket, outcome, timing) = collect(&core, &woke);
            assert_eq!(ticket, expected);
            assert!(timing.cached);
            assert!(matches!(outcome, Outcome::Response(r) if r.body == b"early"));
        }
        assert_eq!(server.count(), 1);
        assert_eq!(control.take_journal()[0].outcome, Event::Taken);
        assert!(control.take_journal().is_empty());
    }
}

#[test]
fn replacing_the_http_cache_clears_the_pending_bit_a_control_holds() {
    let first = Server::new();
    let denied = Server::new();
    let set = launch(&first, vec![definition(&denied, "/private")]);
    complete_set(&set);
    let control = set.control();
    let location = set.destination.clone();
    let (core, _) = first.core(false, true);
    adopt(&core, set);
    assert!(location.journal_pending.load(Ordering::Acquire));
    core.enable_http_cache();
    assert!(control.take_journal().is_empty());
    assert!(!location.journal_pending.load(Ordering::Acquire));
}

#[test]
fn an_empty_journal_drain_takes_neither_lock_before_or_after_adoption() {
    for adopted in [false, true] {
        let set = Preloads::start(Vec::new(), "", ibex2::host::Host::new);
        let control = set.control();
        let location = control.destination.clone();
        let core = Core::with_owners_on(
            (0..WORKERS).map(|_| None).collect(),
            "",
            Box::new(|| {}),
            Arc::new(ibex2::host::Host::new),
        );
        let _staging = if adopted {
            assert!(core.adopt_preloads(set).is_ok());
            None
        } else {
            Some(set)
        };
        let state = core.shared.state.lock().unwrap();
        let destination = location.state.lock().unwrap();
        let (done, drained) = mpsc::channel();
        let reader = std::thread::spawn(move || {
            for _ in 0..100 {
                assert!(control.take_journal().is_empty());
            }
            done.send(()).unwrap();
        });
        let result = drained.recv_timeout(Duration::from_secs(10));
        drop(destination);
        drop(state);
        reader.join().unwrap();
        result.expect("an empty drain waited for a cache lock");
    }
}
