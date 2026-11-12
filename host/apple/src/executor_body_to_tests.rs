//! A reply's body written to an app file (`Request::body_to`, TypeScript's
//! `exactBodyTo`; LLP 1108 D6 R2): both cores stage the file
//! under `fs.write` before sending, writes a 2xx body as it arrives, and
//! replaces the target only when the body is complete.
use super::*;
use exact_runner::Answer;
use ibex2::stdlib::fetch::{Body, BodySource};

/// The requests that reached the transport, by URL.
type Opened = Arc<Mutex<Vec<String>>>;

/// A deterministic byte at `offset` of a served body.
fn byte(offset: usize) -> u8 {
    (offset.wrapping_mul(31) ^ (offset >> 9)) as u8
}

/// A body of `size` bytes made as it is read, at most 7919 bytes a read
/// (chunked); `interrupted` fails after the first 256 KiB.
struct Made {
    at: usize,
    size: usize,
    interrupted: bool,
}
impl BodySource for Made {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, HostError> {
        if self.interrupted && self.at >= 256 << 10 {
            return Err(HostError::Failed("interrupted body".into()));
        }
        let n = out.len().min(7919).min(self.size - self.at);
        // The pattern repeats every 128 KiB; bulk copies keep large scripted
        // replies from spending their completion wait generating bytes in debug.
        static PATTERN: std::sync::OnceLock<Vec<u8>> = std::sync::OnceLock::new();
        let pattern = PATTERN.get_or_init(|| (0..(1 << 17)).map(byte).collect());
        let at = self.at % pattern.len();
        let first = n.min(pattern.len() - at);
        out[..first].copy_from_slice(&pattern[at..at + first]);
        out[first..n].copy_from_slice(&pattern[..n - first]);
        self.at += n;
        Ok(n)
    }
}

/// Serves generated bodies, a declared length, an interrupted body and a 404.
struct Server(Opened);
impl Transport for Server {
    fn open(
        &self,
        request: &ibex2::stdlib::fetch::Request,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        self.0.lock().unwrap().push(request.url.clone());
        let path = request.url.trim_start_matches("https://example.test");
        let (status, source): (u16, Box<dyn BodySource>) = match path
            .strip_prefix("/bytes/")
            .or_else(|| path.strip_prefix("/declared/"))
            .or_else(|| path.strip_prefix("/error/"))
            .map(str::parse)
        {
            Some(Ok(size)) => (
                200,
                Box::new(Made {
                    at: 0,
                    size,
                    interrupted: false,
                }),
            ),
            _ if path == "/interrupted" => (
                200,
                Box::new(Made {
                    at: 0,
                    size: usize::MAX,
                    interrupted: true,
                }),
            ),
            _ => (
                404,
                Box::new(std::io::Cursor::new(b"no such thing".to_vec())),
            ),
        };
        let mut headers = Headers::default();
        headers.set_response("content-type", "application/octet-stream");
        if let Some(size) = path.strip_prefix("/declared/") {
            headers.set_response("content-length", size);
            if request.redirect != ibex2::stdlib::fetch::RedirectMode::Manual
                && size.parse::<u64>().unwrap() > request.body_limit() as u64
            {
                return Err(ibex2::stdlib::fetch::over_limit(request.body_limit()));
            }
        }
        if path == "/redirect" {
            headers.set_response("location", "https://example.test/bytes/17");
        } else if path == "/redirect-denied" {
            headers.set_response("location", "https://denied.test/");
        }
        Ok(StreamingResponse {
            status: if path.starts_with("/redirect") {
                302
            } else if path.starts_with("/error/") {
                404
            } else {
                status
            },
            status_text: String::new(),
            headers,
            body: Body::new(source, request.body_limit(), signal.clone()),
            url: request.url.clone(),
            redirected: false,
        })
    }
}

pub(in super::super) struct Files(pub(in super::super) std::path::PathBuf);
impl Files {
    pub(in super::super) fn new() -> Files {
        let root = std::env::temp_dir().join(format!(
            "exact-body-to-{}-{}",
            std::process::id(),
            ibex2::stdlib::crypto::random_uuid().unwrap()
        ));
        for dir in ["data", "cache", "tmp"] {
            std::fs::create_dir_all(root.join(dir)).unwrap();
        }
        Files(root)
    }
    pub(in super::super) fn roots(&self) -> [std::path::PathBuf; 3] {
        ["data", "cache", "tmp"].map(|dir| self.0.join(dir))
    }
    /// The names in `app:/cache`: no staging file is left behind.
    pub(in super::super) fn cache(&self) -> Vec<String> {
        let mut names: Vec<_> = std::fs::read_dir(self.0.join("cache"))
            .unwrap()
            .map(|e| e.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
}
impl Drop for Files {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

pub(in super::super) const GRANTS: &str = "net.fetch https://example.test\nfs.write app:/cache";

/// The workers, regardless of the process switch.
fn serving(grants: &str) -> (Core, Opened, Receiver<()>) {
    serving_on(grants, false)
}

pub(in super::super) fn serving_on(grants: &str, switched: bool) -> (Core, Opened, Receiver<()>) {
    let opened = Opened::default();
    let owners = (0..WORKERS)
        .map(|_| {
            Some(
                ibex2::host::Host::with_transport(Box::new(Server(opened.clone()))).endow(
                    ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants)).unwrap(),
                ),
            )
        })
        .collect();
    let (wake, woke) = channel();
    let wake: Wake = Box::new(move || {
        let _ = wake.send(());
    });
    let core = Core::with_owners_lanes(
        owners,
        grants,
        wake,
        Arc::new(crate::executor::core::tests::lane_host),
        switched.then(asynch::asynch_lanes),
    );
    (core, opened, woke)
}

pub(in super::super) fn download(path: &str, to: &str) -> Request {
    Request::get(&format!("https://example.test{path}")).body_to(to)
}

pub(in super::super) fn physically_ended(core: &Core) {
    if core.machine.is_some() {
        let state = core.shared.state.lock().unwrap();
        let (state, _) = core
            .shared
            .ready
            .wait_timeout_while(state, Duration::from_secs(30), |s| !s.running.is_empty())
            .unwrap();
        assert!(state.running.is_empty(), "file requests still live");
    }
}

#[test]
fn a_2xx_body_is_written_whole_to_the_file_and_the_response_names_its_size() {
    for switched in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/big"), b"old").unwrap();
        let (core, _opened, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        // Over 16 MiB on the ordered lane (no ceiling named: 1 GiB), and a
        // smaller one on the independent lane, a chunk at a time.
        let big = (20 << 20) + 3;
        core.run(
            job(1, download(&format!("/bytes/{big}"), "app:/cache/big")),
            None,
        )
        .unwrap();
        core.run(
            job(
                2,
                download("/bytes/100000", "app:/cache/small").independent_http(100_000),
            ),
            None,
        )
        .unwrap();
        for (ticket, outcome) in collect(&core, &woke, 2) {
            let Outcome::Response(r) = outcome else {
                panic!("{ticket}: {outcome:?}")
            };
            assert_eq!(r.status, 200);
            assert!(r.body.is_empty(), "{ticket}: the body entered the response");
            let (name, size) = if ticket == 1 {
                ("big", big)
            } else {
                ("small", 100_000)
            };
            assert_eq!(r.body_written(), Some(size as u64));
            let written = std::fs::read(files.0.join("cache").join(name)).unwrap();
            assert_eq!(written.len(), size);
            assert!(
                written.iter().enumerate().all(|(i, b)| *b == byte(i)),
                "{name} differs"
            );
        }
        physically_ended(&core);
        assert_eq!(files.cache(), ["big", "small"]);
    }
}

#[test]
fn a_reply_that_is_not_2xx_is_the_response_and_leaves_the_file() {
    for switched in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (core, _opened, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        core.run(job(1, download("/missing", "app:/cache/a")), None)
            .unwrap();
        let outcomes = collect(&core, &woke, 1);
        let Outcome::Response(r) = &outcomes[0].1 else {
            panic!("{outcomes:?}")
        };
        assert_eq!(
            (r.status, &r.body[..], r.body_written()),
            (404, &b"no such thing"[..], None)
        );
        assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
        physically_ended(&core);
        assert_eq!(files.cache(), ["a"]);
    }
}

#[test]
fn a_body_to_a_file_is_refused_unsent_without_its_grant_parent_or_app_files() {
    for switched in [false, true] {
        let files = Files::new();
        let (core, opened, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        // No `fs.write` over app:/data; a source scope without it; no parent.
        core.run(job(1, download("/bytes/10", "app:/data/a")), None)
            .unwrap();
        let mut scoped = download("/bytes/10", "app:/cache/a");
        scoped.grants = Some("net.fetch https://example.test".into());
        core.run(job(2, scoped), None).unwrap();
        core.run(job(3, download("/bytes/10", "app:/cache/none/a")), None)
            .unwrap();
        for (ticket, outcome) in collect(&core, &woke, 3) {
            let Outcome::Failed { kind, message } = outcome else {
                panic!("{ticket}: {outcome:?}")
            };
            assert_eq!(kind, FailureKind::Refused, "{ticket}: {message}");
            assert!(
                message.starts_with("exactBodyTo app:/"),
                "{ticket}: {message}"
            );
            if ticket < 3 {
                assert!(
                    message.contains(": denied: fs.write "),
                    "{ticket}: {message}"
                );
            }
        }
        std::fs::create_dir(files.0.join("cache/folder")).unwrap();
        core.run(job(7, download("/bytes/10", "app:/cache/folder")), None)
            .unwrap();
        assert!(matches!(
            &collect(&core, &woke, 1)[0].1,
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
        std::fs::remove_dir(files.0.join("cache/folder")).unwrap();
        #[cfg(unix)]
        {
            let linked = Files::new();
            std::fs::remove_dir(linked.0.join("cache")).unwrap();
            std::os::unix::fs::symlink(linked.0.join("tmp"), linked.0.join("cache")).unwrap();
            let (bad_root, bad_opened, bad_woke) = serving_on(GRANTS, switched);
            bad_root.set_app_roots(linked.roots());
            bad_root
                .run(job(8, download("/bytes/10", "app:/cache/a")), None)
                .unwrap();
            assert!(matches!(&collect(&bad_root, &bad_woke, 1)[0].1,
            Outcome::Failed { kind: FailureKind::Refused, message } if message.contains("symbolic link")));
            physically_ended(&bad_root);
            assert!(bad_opened.lock().unwrap().is_empty());
        }
        // A host that names no app files.
        let (bare, _, bare_woke) = serving_on(GRANTS, switched);
        bare.run(job(4, download("/bytes/10", "app:/cache/a")), None)
            .unwrap();
        let outcomes = collect(&bare, &bare_woke, 1);
        assert!(
            matches!(
                &outcomes[0].1,
                Outcome::Failed {
                    kind: FailureKind::Unsupported,
                    ..
                }
            ),
            "{outcomes:?}"
        );
        // A path with a `..` segment, and a stream, are refused at admission.
        assert!(core
            .run(job(5, download("/bytes/10", "app:/cache/../data/a")), None)
            .unwrap_err()
            .contains(". or .."));
        let Answer::Later(stream) = Answer::stream(download("/bytes/10", "app:/cache/a")) else {
            unreachable!()
        };
        assert!(core
            .run(job(6, stream), None)
            .unwrap_err()
            .contains("stream"));
        assert!(
            opened.lock().unwrap().is_empty(),
            "a refused request was sent"
        );
        physically_ended(&core);
        assert!(
            files.cache().is_empty(),
            "core async={switched}: refused request made {:?}",
            files.cache()
        );
    }
}

#[test]
fn a_body_over_its_ceiling_leaves_the_target() {
    for switched in [false, true] {
        for path in [
            "/bytes/2000000",
            "/declared/2000000",
            "/declared/1073741825",
        ] {
            let files = Files::new();
            std::fs::write(files.0.join("cache/a"), b"old").unwrap();
            let (core, _opened, woke) = serving_on(GRANTS, switched);
            core.set_app_roots(files.roots());
            core.run(
                job(
                    1,
                    if path.ends_with("1073741825") {
                        download(path, "app:/cache/a")
                    } else {
                        download(path, "app:/cache/a").independent_http(1 << 20)
                    },
                ),
                None,
            )
            .unwrap();
            let outcomes = collect(&core, &woke, 1);
            assert!(
                matches!(&outcomes[0].1, Outcome::Failed { kind: FailureKind::Refused, message }
            if message == "HTTP response exceeds limit"),
                "{outcomes:?}"
            );
            assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
            physically_ended(&core);
            assert_eq!(files.cache(), ["a"], "a staging file was left");
        }
    }
}

#[test]
fn a_body_ended_mid_way_leaves_the_target() {
    for switched in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (core, _opened, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        core.run(job(1, download("/interrupted", "app:/cache/a")), None)
            .unwrap();
        let outcomes = collect(&core, &woke, 1);
        assert!(
            matches!(
                &outcomes[0].1,
                Outcome::Failed {
                    kind: FailureKind::Network,
                    ..
                }
            ),
            "{outcomes:?}"
        );
        assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
        physically_ended(&core);
        assert_eq!(files.cache(), ["a"], "a staging file was left");
    }
}

/// The lane is charged what the request can hold in memory, not the 1 GiB it
/// may write, so a large download fits beside open sockets
/// (issues/20261026-independent-http-refuses-ceilings-over-32-mib-on-apple.md).
#[test]
fn a_body_to_a_file_is_charged_a_bounded_allowance() {
    let request = download("/bytes/1", "app:/cache/a").independent_http(1 << 30);
    let (lane, _, limit) = reservation(&request).unwrap();
    assert_eq!(lane, 1);
    assert!(limit < 3 << 20, "charged {limit}");
    let plain = Request::get("https://example.test/bytes/1").independent_http(1 << 30);
    assert!(reservation(&plain).is_err());
    for switched in [false, true] {
        let files = Files::new();
        let (core, _, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        core.run(job(1, request.clone()), None).unwrap();
        assert!(matches!(&collect(&core, &woke, 1)[0].1,
            Outcome::Response(r) if r.body_written() == Some(1)));
        physically_ended(&core);
        assert_eq!(files.cache(), ["a"]);
    }
}

/// A native delegate must return promptly, so its disk destination is deferred
/// to the Apple slice (LLP 1162 D8).
#[test]
fn the_native_darwin_lane_refuses_a_body_to_a_file_before_contact() {
    let files = Files::new();
    let script = Arc::new(super::async_tests::Script::default());
    let (core, woke) = super::async_tests::core(&script, GRANTS);
    core.set_app_roots(files.roots());
    core.run(job(1, download("/bytes/10", "app:/cache/a")), None)
        .unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(
        matches!(&outcomes[0].1,
        Outcome::Failed { kind: FailureKind::Unsupported, message }
        if message.contains("DarwinLane") && message.contains("URLSession")),
        "{outcomes:?}"
    );
    physically_ended(&core);
    assert_eq!(script.wait(0), 0);
    assert!(files.cache().is_empty());
}

/// A plain reply on the independent lane is read into one buffer of its
/// ceiling, so the lane charges the ceiling once and holds the documented
/// 64 MiB beside the most request buffers it admits. A stream's messages
/// still grow, so a stream is charged twice its ceiling
/// (issues/closed/20261026-independent-http-refuses-ceilings-over-32-mib-on-apple.md).
#[test]
fn a_64_mib_ceiling_is_charged_once_and_fits_the_lane() {
    let read = Request::get("https://example.test/read").independent_http(64 << 20);
    let (lane, charge, limit) = reservation(&read).unwrap();
    assert_eq!((lane, limit - charge), (1, (64 << 20) + MAX_HEADERS * 2));
    assert!(MAX_REQUEST + limit - charge <= BYTES[1]);
    let Answer::Later(stream) = Answer::stream(read.independent_http(8 << 20)) else {
        unreachable!()
    };
    let (_, charge, limit) = reservation(&stream).unwrap();
    assert_eq!(limit - charge, (16 << 20) + MAX_HEADERS * 2);
}

/// The reply is read into a buffer of its ceiling, never grown past it, and
/// trimmed to its length; one byte over the ceiling is refused.
#[test]
fn a_reply_is_read_into_one_buffer_within_its_ceiling() {
    let (core, _opened, woke) = serving(GRANTS);
    let get = |size: usize, ceiling: u32| {
        Request::get(&format!("https://example.test/bytes/{size}")).independent_http(ceiling)
    };
    let big = (40 << 20) + 3;
    core.run(job(1, get(big, 64 << 20)), None).unwrap();
    core.run(job(2, get(100_000, 100_000)), None).unwrap();
    core.run(job(3, get(100_001, 100_000)), None).unwrap();
    let mut outcomes = collect(&core, &woke, 3);
    outcomes.sort_by_key(|(ticket, _)| *ticket);
    for (at, size) in [(0, big), (1, 100_000)] {
        let Outcome::Response(r) = &outcomes[at].1 else {
            panic!("{:?}", outcomes[at])
        };
        assert_eq!((r.body.len(), r.body.capacity()), (size, size));
        let mut sampled = (0..size).step_by(4093).chain(
            [size - 1, (1 << 17) - 1, 1 << 17, (1 << 17) + 1]
                .into_iter()
                .filter(|i| *i < size),
        );
        assert!(sampled.all(|i| r.body[i] == byte(i)));
    }
    assert!(
        matches!(&outcomes[2].1, Outcome::Failed { kind: FailureKind::Refused, message }
            if message == "HTTP response exceeds limit"),
        "{:?}",
        outcomes[2]
    );
}

/// A read whose ceiling the lane's open streams leave no room for is refused
/// when it is made, not left waiting for as long as they stay open; one that
/// fits beside them runs.
#[test]
fn a_ceiling_open_streams_leave_no_room_for_is_refused_when_made() {
    let (core, fixture, woke) = setup();
    let held = Request::get("https://example.test/hold").independent_http(8 << 20);
    let Answer::Later(stream) = Answer::stream(held) else {
        unreachable!()
    };
    core.run(job(1, stream), None).unwrap();
    fixture.wait_held(1);
    let read = |ceiling| Request::get("https://example.test/read").independent_http(ceiling);
    let refused = core.run(job(2, read(64 << 20)), None).unwrap_err();
    assert!(refused.contains("held by open streams"), "{refused}");
    core.run(job(3, read(48 << 20)), None).unwrap();
    let outcomes = collect(&core, &woke, 1);
    assert!(
        matches!(&outcomes[0], (3, Outcome::Response(r)) if r.body == b"done"),
        "{outcomes:?}"
    );
    fixture.release();
}

#[test]
fn a_non_2xx_body_is_held_only_within_its_allowance() {
    for switched in [false, true] {
        let files = Files::new();
        std::fs::write(files.0.join("cache/a"), b"old").unwrap();
        let (core, _, woke) = serving_on(GRANTS, switched);
        core.set_app_roots(files.roots());
        let size = exact_runner::BODY_TO_HELD_BYTES + 1;
        core.run(
            job(
                1,
                download(&format!("/error/{size}"), "app:/cache/a").independent_http(1 << 30),
            ),
            None,
        )
        .unwrap();
        let done = collect(&core, &woke, 1);
        assert!(
            matches!(&done[0].1, Outcome::Failed { kind: FailureKind::Refused, message }
            if message == "HTTP response exceeds limit"),
            "{done:?}"
        );
        physically_ended(&core);
        assert_eq!(std::fs::read(files.0.join("cache/a")).unwrap(), b"old");
        assert_eq!(files.cache(), ["a"]);
    }
}

#[test]
fn a_queued_read_crowded_out_by_a_later_stream_is_refused() {
    for class in [
        RequestClass::Ordered,
        RequestClass::Read,
        RequestClass::Parallel(0),
    ] {
        let (core, fixture, woke) = setup();
        core.run(
            job(
                1,
                Request::get("https://example.test/hold").independent_http(40 << 20),
            ),
            None,
        )
        .unwrap();
        fixture.wait_held(1);

        let mut read = Request::get("https://example.test/read").independent_http(64 << 20);
        read.timeout_ms = Some(30_000);
        let mut request = job(2, read);
        request.class = class;
        core.run(request, None).unwrap();
        assert_eq!(core.shared.state.lock().unwrap().jobs[1][0].ticket, 2);

        let Answer::Later(stream) =
            Answer::stream(Request::get("https://example.test/hold").independent_http(8 << 20))
        else {
            unreachable!()
        };
        core.run(job(3, stream), None).unwrap();
        fixture.wait_held(2);

        let outcomes = collect(&core, &woke, 1);
        assert!(
            matches!(&outcomes[0], (2, Outcome::Failed { kind: FailureKind::Refused, message })
                if message == "native response ceiling exceeds the lane budget held by open streams"),
            "{outcomes:?}"
        );

        // A smaller successor can run while both earlier transports stay held.
        core.run(
            job(
                4,
                Request::get("https://example.test/after").independent_http(1 << 20),
            ),
            None,
        )
        .unwrap();
        let outcomes = collect(&core, &woke, 1);
        assert!(matches!(&outcomes[0], (4, Outcome::Response(r)) if r.body == b"done"));
        {
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts, [0, 2]);
            assert_eq!(state.streams, 1);
            assert_eq!(
                state.bytes[1],
                state.running.iter().map(|r| r.charge).sum::<usize>()
            );
            assert!(state.jobs[1].is_empty());
            assert!(state.completed[1].is_empty());
        }
        assert_eq!(
            fixture.state.lock().unwrap().2,
            [
                "https://example.test/hold",
                "https://example.test/hold",
                "https://example.test/after"
            ]
        );

        fixture.release();
        let mut tickets: Vec<_> = collect(&core, &woke, 2)
            .into_iter()
            .map(|(t, _)| t)
            .collect();
        tickets.sort();
        assert_eq!(tickets, [1, 3]);
        until_settled(&core);
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.counts, [0, 0]);
        assert_eq!(state.bytes, [0, 0]);
        assert_eq!(state.streams, 0);
    }
}

#[test]
fn an_upload_reserving_room_wakes_queued_reads_before_its_first_event() {
    for ceiling in [32 << 20, 64 << 20] {
        let fixture = Arc::new(Fixture::default());
        let grants = "net.fetch https://example.test";
        let owners = (0..WORKERS)
            .map(|_| {
                Some(
                    ibex2::host::Host::with_transport(Box::new(Fake(fixture.clone())))
                        .endow(ibex2::grant::GrantSet::parse(grants).unwrap()),
                )
            })
            .collect();
        let (wake, woke) = channel();
        // Count notifications on workers, where no async finisher can
        // contribute one; exercise refusal on the selected execution model.
        let lanes = (ceiling == 64 << 20 && asynchronous()).then(asynch_lanes);
        let core = Core::with_owners_lanes(
            owners,
            grants,
            Box::new(move || {
                let _ = wake.send(());
            }),
            Arc::new(crate::executor::core::tests::lane_host),
            lanes,
        );
        let (opening, opened) = channel();
        let (release, waiting) = channel::<()>();
        let waiting = Mutex::new(waiting);
        let stream_fixture = fixture.clone();
        let core = core.streams_on(move || {
            opening.send(()).unwrap();
            let _ = waiting.lock().unwrap().recv();
            ibex2::host::Host::with_transport(Box::new(Fake(stream_fixture.clone())))
        });
        core.run(
            job(
                1,
                Request::get("https://example.test/hold").independent_http(40 << 20),
            ),
            None,
        )
        .unwrap();
        fixture.wait_held(1);
        let mut read = Request::get("https://example.test/read").independent_http(ceiling);
        read.timeout_ms = Some(30_000);
        core.run(job(2, read), None).unwrap();
        assert_eq!(core.shared.state.lock().unwrap().jobs[1][0].ticket, 2);

        let before = core.shared.worker_notifications.load(Ordering::Relaxed);
        let mut upload = Request::get("https://example.test/hold")
            .independent_http(8 << 20)
            .upload_progress();
        upload.method = "POST".into();
        upload.body = vec![7];
        let Answer::Later(upload) = Answer::stream(upload) else {
            unreachable!()
        };
        core.run(job(3, upload), None).unwrap();
        opened.recv_timeout(Duration::from_secs(5)).unwrap();
        if ceiling == 32 << 20 {
            // This read still fits beside the stream, so its settlement
            // cannot mask a missing notification at upload promotion.
            assert_eq!(
                core.shared.worker_notifications.load(Ordering::Relaxed) - before,
                2,
                "upload admission and promotion must each notify the schedulers"
            );
            assert_eq!(core.shared.state.lock().unwrap().jobs[1][0].ticket, 2);
        } else {
            let outcomes = collect(&core, &woke, 1);
            assert!(
                matches!(&outcomes[0], (2, Outcome::Failed { kind: FailureKind::Refused, message })
                    if message == "native response ceiling exceeds the lane budget held by open streams"),
                "{outcomes:?}"
            );
            let state = core.shared.state.lock().unwrap();
            assert_eq!(state.counts, [0, 2]);
            assert_eq!(state.streams, 1);
            assert_eq!(
                state.bytes[1],
                state.running.iter().map(|r| r.charge).sum::<usize>()
            );
            assert!(state.jobs[1].is_empty());
        }
        assert_eq!(
            fixture.state.lock().unwrap().2,
            ["https://example.test/hold"]
        );
        fixture.release();
        release.send(()).unwrap();
        let mut ended = Vec::new();
        let expected = if ceiling == 32 << 20 { 3 } else { 2 };
        while ended.len() < expected {
            ended.extend(
                collect(&core, &woke, 1)
                    .into_iter()
                    .filter(|(_, outcome)| !matches!(outcome, Outcome::Message(_)))
                    .map(|(ticket, _)| ticket),
            );
        }
        ended.sort();
        assert_eq!(
            ended,
            if ceiling == 32 << 20 {
                vec![1, 2, 3]
            } else {
                vec![1, 3]
            }
        );
        until_settled(&core);
        let state = core.shared.state.lock().unwrap();
        assert_eq!(state.counts, [0, 0]);
        assert_eq!(state.bytes, [0, 0]);
        assert_eq!(state.streams, 0);
    }
}
