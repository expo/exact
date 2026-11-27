//! Upload admission with transport opening and outcome delivery controlled separately.
use super::*;
use std::collections::BTreeSet;

fn reply(replies: &mut BTreeSet<u64>, ticket: u64, response: exact_runner::Response) {
    assert!((1..=15).contains(&ticket), "unexpected reply {ticket}");
    assert_eq!(response.status, 201);
    assert_eq!(response.body, br#"{"id":"a1"}"#);
    assert!(replies.insert(ticket), "duplicate reply {ticket}");
}

/// LLP 1016.000 D10: fifteen 2 MB photo uploads, in the budget a device
/// has, go out three at a time, as upstream T3 Code sends them, and leave
/// the app's other streams their places. An event stream opened before the
/// burst and one opened after it both open at once, and every upload ends
/// with its reply. Without the cap all fifteen would go out at once, since
/// the budget and the lane have room for them, and with the first stream
/// they would take all 16 places, so the second stream would be refused.
/// (Fifteen, because the independent lane has room for no more of them at
/// once on the asynchronous core, which counts a second copy of each body.)
#[test]
fn a_burst_of_photo_uploads_goes_three_at_a_time_and_leaves_other_streams_their_places() {
    let (core, script, woke) = setup();
    core.run_owned(stream(100, "/events", 1024), None).unwrap();
    let before = feed(&script, 0);
    for ticket in 1..=15 {
        core.run_owned(upload(ticket, "/upload", 2 << 20), None)
            .unwrap();
    }
    let going: Vec<Feed> = (1..=3).map(|at| feed(&script, at)).collect();
    core.run_owned(stream(101, "/events", 1024), None)
        .expect("a stream after the burst has its place");
    let after = feed(&script, 4);
    assert_eq!(
        core.shared.uploads.waiting(),
        (12, 3 * (2 << 20) * super::super::COPIES)
    );
    for (fed, ticket) in [(&before, 100), (&after, 101)] {
        fed.send(format!("id: {ticket}\ndata: {ticket}\n\n").into_bytes())
            .unwrap();
        while !matches!(next(&core, &woke), (t, Outcome::Message(_)) if t == ticket) {}
    }
    let (opening, openings) = channel();
    *script.opening.lock().unwrap() = Some(opening);
    for step in going {
        for _ in 0..3 {
            step.send(Vec::new()).unwrap();
        }
    }
    let releases: Vec<_> = (0..3)
        .map(|_| openings.recv_timeout(Duration::from_secs(5)).unwrap())
        .collect();
    script.opening.lock().unwrap().take();
    // Consume the replacement uploads' starts before their feeds exist.
    // Registering those feeds produces no executor outcome or wake.
    let (mut replies, mut starts) = (BTreeSet::new(), BTreeSet::new());
    while replies.len() < 3 || starts.len() < 3 {
        match next(&core, &woke) {
            (ticket, Outcome::Response(r)) => reply(&mut replies, ticket, r),
            (ticket, Outcome::Message(m)) if ticket > 3 && m.upload_started() => {
                assert!(starts.insert(ticket), "duplicate start {ticket}");
            }
            (_, Outcome::Message(_)) => {}
            other => panic!("expected upload progress or a reply, got {other:?}"),
        }
    }
    assert_eq!(replies, BTreeSet::from([1, 2, 3]));
    assert_eq!(starts, BTreeSet::from([4, 5, 6]));
    assert!(
        core.drain().is_empty(),
        "all outcomes preceded the new feeds"
    );
    while woke.try_recv().is_ok() {}
    core.begin_pump();
    assert!(
        woke.try_recv().is_err(),
        "no outcome remains to wake the host"
    );
    for release in releases {
        release.send(()).unwrap();
    }

    for opened in (5..17).step_by(3) {
        // A start may already be drained: wait for feeds, not another outcome.
        let batch: Vec<_> = (opened..opened + 3).map(|at| feed(&script, at)).collect();
        for step in batch {
            for _ in 0..3 {
                step.send(Vec::new()).unwrap();
            }
        }
        for _ in 0..3 {
            loop {
                match next(&core, &woke) {
                    (ticket, Outcome::Response(r)) => {
                        reply(&mut replies, ticket, r);
                        break;
                    }
                    (_, Outcome::Message(_)) => {}
                    other => panic!("expected upload progress or a reply, got {other:?}"),
                }
            }
        }
    }
    assert_eq!(replies, (1..=15).collect());
    assert_eq!(
        script.going.lock().unwrap().1,
        3,
        "never more than three at once"
    );
    assert_eq!(core.shared.uploads.waiting(), (0, 0));
    for (fed, ticket) in [(&before, 100), (&after, 101)] {
        fed.send(format!("data: {ticket}\n\n").into_bytes())
            .unwrap();
        let (got, outcome) = next(&core, &woke);
        assert_eq!(
            (got, data(&outcome)),
            (ticket, (ticket.to_string().as_str(), 0))
        );
    }
}

/// An upload whose file body finds every reader place taken (reads stuck
/// on a stalled disk, in this executor or another) waits in its queue
/// unopened, and starts when a place is given back (LLP 1133.008.000 D4).
#[test]
fn an_upload_waits_for_a_reader_place_and_starts_when_one_is_given_back() {
    let _turn = crate::tests::one_at_a_time();
    crate::body::settle_readers();
    let (core, script, woke) = setup_with("net.fetch https://example.test\nfs.read app:/tmp");
    let files = AppFiles::new("a.bin", &[7; 4096]);
    core.set_app_roots(files.roots());
    // The roots open first, so the stuck reads below hold every place.
    core.run_owned(file_upload(1, "/upload", "a.bin"), None)
        .unwrap();
    let first = feed(&script, 0);
    for _ in 0..3 {
        first.send(Vec::new()).unwrap();
    }
    while !matches!(next(&core, &woke), (1, Outcome::Response(_))) {}
    crate::body::settle_readers();
    let release = Arc::new(std::sync::atomic::AtomicBool::new(false));
    let stuck: Vec<_> = (0..crate::body::MAX_READERS)
        .map(|_| {
            let release = release.clone();
            crate::body::reader(move || {
                while !release.load(std::sync::atomic::Ordering::Acquire) {
                    std::thread::sleep(Duration::from_millis(2));
                }
            })
            .expect("a free place")
        })
        .collect();
    core.run_owned(file_upload(2, "/upload", "a.bin"), None)
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while core.shared.uploads.waiting().0 == 0 {
        assert!(Instant::now() < deadline, "the upload never waited");
        std::thread::sleep(Duration::from_millis(2));
    }
    std::thread::sleep(Duration::from_millis(50));
    assert_eq!(
        script.feeds.lock().unwrap().len(),
        1,
        "it opened without a place"
    );
    release.store(true, std::sync::atomic::Ordering::Release);
    for reader in stuck {
        reader.join().unwrap();
    }
    let second = feed(&script, 1);
    for _ in 0..3 {
        second.send(Vec::new()).unwrap();
    }
    while !matches!(next(&core, &woke), (2, Outcome::Response(_))) {}
    assert_eq!(core.shared.uploads.waiting(), (0, 0));
}

#[test]
fn take_place_racing_reader_drops_never_stalls_an_upload() {
    let _turn = crate::tests::one_at_a_time();
    crate::body::settle_readers();
    let (core, script, woke) = setup_with("net.fetch https://example.test\nfs.read app:/tmp");
    let files = AppFiles::new("a.bin", &[7; 4096]);
    core.set_app_roots(files.roots());
    // Open app roots once so each round contends only on reader places.
    core.run_owned(file_upload(1, "/upload", "a.bin"), None)
        .unwrap();
    let first = feed(&script, 0);
    for _ in 0..3 {
        first.send(Vec::new()).unwrap();
    }
    while !matches!(next(&core, &woke), (1, Outcome::Response(_))) {}
    crate::body::settle_readers();

    for round in 0..40u64 {
        let ticket = round + 2;
        let ready = Arc::new(std::sync::Barrier::new(crate::body::MAX_READERS + 1));
        let go = Arc::new(std::sync::Barrier::new(crate::body::MAX_READERS + 1));
        let holders: Vec<_> = (0..crate::body::MAX_READERS)
            .map(|_| {
                let ready = ready.clone();
                let go = go.clone();
                crate::body::reader(move || {
                    ready.wait();
                    go.wait();
                })
                .expect("a free place")
            })
            .collect();
        ready.wait();
        // Release all reader places as the upload enters `take_place`.
        go.wait();
        core.run_owned(file_upload(ticket, "/upload", "a.bin"), None)
            .unwrap();
        let next_feed = feed(&script, (round + 1) as usize);
        for _ in 0..3 {
            next_feed.send(Vec::new()).unwrap();
        }
        while !matches!(next(&core, &woke), (t, Outcome::Response(_)) if t == ticket) {}
        for holder in holders {
            holder.join().unwrap();
        }
        crate::body::settle_readers();
    }
    assert_eq!(core.shared.uploads.waiting(), (0, 0));
}
