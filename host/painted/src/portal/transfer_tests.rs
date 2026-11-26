#![allow(unsafe_code)]

use super::super::wire::{self, Message};
use super::*;
use std::io::{Read, Write};
use std::os::fd::{AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

/// One line of the authentication exchange, its CRLF included.
fn line(s: &mut UnixStream) -> Vec<u8> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    while !line.ends_with(b"\r\n") {
        s.read_exact(&mut byte).unwrap();
        line.push(byte[0]);
    }
    line
}

/// The next message on `s` and the descriptors sent beside it
/// (`SCM_RIGHTS`), its header read however many reads it takes.
fn receive(s: &mut UnixStream) -> Option<(Message, Vec<OwnedFd>)> {
    let mut head = [0u8; 16];
    let mut fds = Vec::new();
    let mut got = 0;
    while got < head.len() {
        match receive_some(s, &mut head[got..], &mut fds) {
            0 => return None,
            n => got += n,
        }
    }
    let mut buf = head.to_vec();
    buf.resize(wire::message_len(&head)?, 0);
    s.read_exact(&mut buf[16..]).ok()?;
    Some((wire::parse(&buf)?, fds))
}

/// One `recvmsg` into `into`, keeping the descriptors it brings: how many
/// bytes it read, none at the end or on an error.
fn receive_some(s: &UnixStream, into: &mut [u8], fds: &mut Vec<OwnedFd>) -> usize {
    let mut iov = libc::iovec {
        iov_base: into.as_mut_ptr().cast(),
        iov_len: into.len(),
    };
    let mut control = [0u64; 64];
    // SAFETY: an all-zero msghdr is a valid empty one; its buffers outlive the call.
    let mut msg: libc::msghdr = unsafe { std::mem::zeroed() };
    msg.msg_iov = &mut iov;
    msg.msg_iovlen = 1;
    msg.msg_control = control.as_mut_ptr().cast();
    msg.msg_controllen = std::mem::size_of_val(&control) as _;
    // SAFETY: msg points at live buffers for the call.
    let n = unsafe { libc::recvmsg(s.as_raw_fd(), &mut msg, libc::MSG_CMSG_CLOEXEC) };
    // SAFETY: the control buffer holds what recvmsg wrote; each header is read within it.
    unsafe {
        let mut cmsg = libc::CMSG_FIRSTHDR(&msg);
        while !cmsg.is_null() {
            if (*cmsg).cmsg_level == libc::SOL_SOCKET && (*cmsg).cmsg_type == libc::SCM_RIGHTS {
                let count = ((*cmsg).cmsg_len as usize - libc::CMSG_LEN(0) as usize) / 4;
                let data = libc::CMSG_DATA(cmsg).cast::<i32>();
                fds.extend((0..count).map(|k| OwnedFd::from_raw_fd(data.add(k).read_unaligned())));
            }
            cmsg = libc::CMSG_NXTHDR(&msg, cmsg);
        }
    }
    n.max(0) as usize
}

/// What the fake bus does: how many connections it serves, which
/// `AddFiles` call (counting from 1) it refuses, which call of a member it
/// answers only after [`LATE`], after how many calls it closes each
/// connection but the last, which call it answers with bytes that are no
/// message, after how many calls it stops reading for a while, from which
/// start it holds the replies to two until told to send them, when it sends
/// an unasked message, and whom it tells of each call and of each
/// connection's end.
#[derive(Default)]
struct Serving {
    connections: usize,
    refuse: Option<usize>,
    slow: Option<(&'static str, usize)>,
    ends: Option<usize>,
    garbage: Option<usize>,
    stall: Option<(usize, std::sync::mpsc::Receiver<()>)>,
    defer: Option<(usize, std::sync::mpsc::Receiver<()>)>,
    push: Option<std::sync::mpsc::Receiver<()>>,
    tell: Option<std::sync::mpsc::Sender<String>>,
}

/// How long the fake bus takes to answer its slow call.
const LATE: Duration = Duration::from_millis(600);

/// A call the bus heard: the connection it came on, by the order they were
/// accepted, its member, the files its descriptors name, and its first text
/// argument (a transfer's key), if any.
#[derive(Debug)]
struct Call {
    connection: usize,
    member: String,
    named: Vec<PathBuf>,
    key: Option<String>,
    wants_reply: bool,
}

/// Each call the bus heard, in order.
type Heard = Vec<Call>;

/// A session bus that agrees to pass descriptors and answers the file
/// transfer calls as `serving` says, each connection at once: its address,
/// and the calls it heard.
fn bus(name: &str, serving: Serving) -> (String, std::thread::JoinHandle<Heard>) {
    let path = std::env::temp_dir().join(format!("exact-transfer-{name}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let address = format!("unix:path={}", path.display());
    let served = std::thread::spawn(move || {
        let connections = serving.connections;
        let shared = Arc::new(Mutex::new((serving, Heard::new(), 0)));
        let serves: Vec<_> = (0..connections)
            .map(|n| {
                let (s, _) = listener.accept().unwrap();
                let shared = shared.clone();
                let ends = lock(&shared).0.ends.filter(|_| n + 1 < connections);
                std::thread::spawn(move || serve(s, n, &shared, ends))
            })
            .collect();
        for serve in serves {
            serve.join().unwrap();
        }
        let _ = std::fs::remove_file(&path);
        let heard = std::mem::take(&mut lock(&shared).1);
        heard
    });
    (address, served)
}

/// Answer one connection until it closes, or until it has answered `ends`
/// calls, recording its calls.
fn serve(
    s: UnixStream,
    connection: usize,
    shared: &Mutex<(Serving, Heard, usize)>,
    ends: Option<usize>,
) {
    serve_calls(s, connection, shared, ends);
    if let Some(tell) = &lock(shared).0.tell {
        let _ = tell.send(format!("end {connection}"));
    }
}

fn serve_calls(
    mut s: UnixStream,
    connection: usize,
    shared: &Mutex<(Serving, Heard, usize)>,
    ends: Option<usize>,
) {
    let mut answered = 0;
    let mut deferred: Vec<Vec<u8>> = Vec::new();
    if let Some(push) = lock(shared).0.push.take() {
        let mut out = s.try_clone().unwrap();
        std::thread::spawn(move || {
            for () in push {
                let signal = wire::message(wire::SIGNAL, 99, None, "", "/x", "x.Y", "Z", &[], 0);
                if out.write_all(&signal).is_err() {
                    return;
                }
            }
        });
    }
    assert!(line(&mut s).starts_with(b"\0AUTH EXTERNAL "));
    s.write_all(b"OK 0123456789abcdef\r\n").unwrap();
    assert_eq!(line(&mut s), b"NEGOTIATE_UNIX_FD\r\n");
    s.write_all(b"AGREE_UNIX_FD\r\n").unwrap();
    assert_eq!(line(&mut s), b"BEGIN\r\n");
    while let Some((m, fds)) = receive(&mut s) {
        let named = fds
            .iter()
            .map(|fd| std::fs::read_link(format!("/proc/self/fd/{}", fd.as_raw_fd())).unwrap())
            .collect();
        let key = match m.body.first() {
            Some(Value::Str(key)) => Some(key.clone()),
            _ => None,
        };
        // This call's place among the bus's calls of its member, counting from 1
        let (nth, slow) = {
            let mut shared = lock(shared);
            shared.1.push(Call {
                connection,
                member: m.member.clone(),
                named,
                key,
                wants_reply: m.flags & wire::NO_REPLY_EXPECTED == 0,
            });
            if let Some(tell) = &shared.0.tell {
                let _ = tell.send(m.member.clone());
            }
            let nth = shared.1.iter().filter(|c| c.member == m.member).count();
            (
                nth,
                shared
                    .0
                    .slow
                    .is_some_and(|(member, n)| member == m.member && n == nth),
            )
        };
        if slow {
            std::thread::sleep(LATE);
        }
        let reply = |body: &[Value]| {
            wire::message(
                wire::METHOD_RETURN,
                900,
                Some(m.serial),
                "",
                "",
                "",
                "",
                body,
                0,
            )
        };
        let answer = match m.member.as_str() {
            "Hello" => reply(&[Value::str(":1.9")]),
            "StartTransfer" => {
                let options = &m.body[0];
                assert_eq!(options.get("writable"), Some(&Value::Bool(true)));
                assert_eq!(options.get("autostop"), Some(&Value::Bool(true)));
                reply(&[Value::str(&format!("key{nth}"))])
            }
            "AddFiles" => {
                assert_eq!(
                    m.unix_fds as usize,
                    fds.len(),
                    "its header counts the descriptors"
                );
                for fd in &fds {
                    // SAFETY: fcntl(2) on a descriptor this test holds takes no pointers.
                    let flags = unsafe { libc::fcntl(fd.as_raw_fd(), libc::F_GETFL) };
                    assert_eq!(flags & libc::O_PATH, libc::O_PATH, "a path alone");
                    assert_eq!(
                        flags & libc::O_NOFOLLOW,
                        0,
                        "the document portal refuses `O_NOFOLLOW`"
                    );
                }
                let indices: Vec<Value> = (0..fds.len() as i64).map(Value::Num).collect();
                assert_eq!(
                    m.body[1],
                    Value::Array("h".into(), indices),
                    "its body indexes them"
                );
                let refused = {
                    let mut shared = lock(shared);
                    shared.2 += 1;
                    shared.0.refuse == Some(shared.2)
                };
                if refused {
                    let refusal = "org.freedesktop.portal.Error.NotAllowed";
                    super::super::tests::error(m.serial, refusal, "Can't export file")
                } else {
                    reply(&[])
                }
            }
            "RetrieveFiles" => reply(&[Value::Array(
                "s".into(),
                vec![Value::str("/run/user/1000/doc/1a2b/notes.txt")],
            )]),
            _ => reply(&[]),
        };
        answered += 1;
        let (garbage, stall) = {
            let mut shared = lock(shared);
            let stall = shared
                .0
                .stall
                .take_if(|(after, _)| *after == answered)
                .map(|(_, release)| release);
            (shared.0.garbage == Some(answered), stall)
        };
        if garbage {
            s.write_all(&[b'X'; 16]).unwrap();
            continue;
        }
        // Two starts' replies held, then sent together once the test says
        let held = lock(shared).0.defer.as_ref().is_some_and(|(from, _)| {
            m.member == "StartTransfer" && (*from..*from + 2).contains(&nth)
        });
        if held {
            deferred.push(answer);
            if deferred.len() == 2 {
                let (_, release) = lock(shared).0.defer.take().unwrap();
                release.recv().unwrap();
                for answer in deferred.drain(..) {
                    s.write_all(&answer).unwrap();
                }
            }
            continue;
        }
        // A call that wants no reply gets none, as a service sends it none
        if m.flags & wire::NO_REPLY_EXPECTED == 0 {
            s.write_all(&answer).unwrap();
        }
        if let Some(release) = stall {
            // Read no more, the connection kept open, until the test says, so what the client writes fills the socket
            let _ = release.recv();
            return;
        }
        if ends == Some(answered) {
            return;
        }
    }
}

/// Seventeen files in a folder of their own.
fn files(name: &str) -> (PathBuf, Vec<PathBuf>) {
    let folder = std::env::temp_dir().join(format!(
        "exact-transfer-{name}-files-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&folder).unwrap();
    let paths = (1..=17)
        .map(|n| {
            let path = folder.join(format!("{n}.txt"));
            std::fs::write(&path, n.to_string()).unwrap();
            path
        })
        .collect();
    (folder, paths)
}

fn soon() -> Instant {
    Instant::now() + Duration::from_secs(10)
}

fn members(heard: &Heard) -> Vec<&str> {
    heard.iter().map(|c| c.member.as_str()).collect()
}

/// Wait up to 5 seconds for the bus to tell of a `member` call, which a
/// stop's own thread writes in its time.
fn until_heard(heard: &std::sync::mpsc::Receiver<String>, member: &str) {
    let until = Instant::now() + Duration::from_secs(5);
    while let Ok(told) = heard.recv_timeout(until.saturating_duration_since(Instant::now())) {
        if told == member {
            return;
        }
    }
    panic!("the bus heard no {member}");
}

/// The calls connection `n` heard.
fn members_on(heard: &Heard, n: usize) -> Vec<&str> {
    heard
        .iter()
        .filter(|c| c.connection == n)
        .map(|c| c.member.as_str())
        .collect()
}

/// The keys of the transfers the bus heard stopped.
fn stopped(heard: &Heard) -> Vec<&str> {
    heard
        .iter()
        .filter(|c| c.member == "StopTransfer")
        .filter_map(|c| c.key.as_deref())
        .collect()
}

/// `paths` registered with `portal` by `deadline`, each opened as the
/// drag's copies are, as a path alone.
fn send(portal: &Portal, paths: &[PathBuf], deadline: Instant) -> Result<String, String> {
    portal
        .send(paths.len(), |n| path_only(&paths[n]), deadline)
        .map(|transfer| transfer.key)
}

/// `path` opened as a path alone (`O_PATH`), as `AddFiles` takes it.
fn path_only(path: &std::path::Path) -> Result<std::fs::File, String> {
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::File::options()
        .read(true)
        .custom_flags(libc::O_PATH)
        .open(path)
        .map_err(|e| format!("{}: {e}", path.display()))
}

#[test]
fn a_transfer_sends_its_files_beside_add_files_sixteen_at_a_time() {
    let serving = Serving {
        connections: 2,
        ..Serving::default()
    };
    let (address, served) = bus("sends", serving);
    let (folder, paths) = files("sends");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths, soon()), Ok("key1".to_owned()));
    assert_eq!(
        portal.retrieve("key1"),
        Ok(vec![PathBuf::from("/run/user/1000/doc/1a2b/notes.txt")]),
        "a retrieval goes on a connection of its own"
    );
    drop(portal);
    let calls = served.join().unwrap();
    let adds: Vec<&Vec<PathBuf>> = calls
        .iter()
        .filter(|c| c.member == "AddFiles")
        .map(|c| &c.named)
        .collect();
    assert_eq!(
        adds.iter().map(|named| named.len()).collect::<Vec<_>>(),
        [16, 1]
    );
    let named: Vec<PathBuf> = adds.into_iter().flatten().cloned().collect();
    assert_eq!(named, paths, "each descriptor names its file, in order");
    assert_eq!(
        members(&calls),
        [
            "Hello",
            "StartTransfer",
            "AddFiles",
            "AddFiles",
            "Hello",
            "RetrieveFiles"
        ]
    );
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_transfer_whose_files_are_refused_is_stopped_on_its_own_connection() {
    for (refuse, expected) in [
        (
            1,
            &["Hello", "StartTransfer", "AddFiles", "StopTransfer"][..],
        ),
        (
            2,
            &[
                "Hello",
                "StartTransfer",
                "AddFiles",
                "AddFiles",
                "StopTransfer",
            ][..],
        ),
    ] {
        let (tell, heard) = std::sync::mpsc::channel();
        let serving = Serving {
            connections: 1,
            refuse: Some(refuse),
            tell: Some(tell),
            ..Serving::default()
        };
        let (address, served) = bus(&format!("refused-{refuse}"), serving);
        let (folder, paths) = files(&format!("refused-{refuse}"));
        let portal = Portal::at(Some(address));
        assert!(send(&portal, &paths, soon()).is_err());
        until_heard(&heard, "StopTransfer");
        drop(portal);
        assert_eq!(
            members(&served.join().unwrap()),
            expected,
            "batch {refuse} refused"
        );
        let _ = std::fs::remove_dir_all(folder);
    }
}

#[test]
fn a_file_that_cannot_be_opened_stops_its_transfer_alone() {
    let (tell, heard) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("missing", serving);
    let (folder, paths) = files("missing");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
    let missing = [paths[1].clone(), folder.join("missing.txt")];
    assert!(send(&portal, &missing, soon()).is_err());
    until_heard(&heard, "StopTransfer");
    drop(portal);
    let calls = served.join().unwrap();
    assert_eq!(
        members(&calls),
        [
            "Hello",
            "StartTransfer",
            "AddFiles",
            "StartTransfer",
            "StopTransfer"
        ],
        "no batch holding it is added, and the connection that gave the first key stays"
    );
    assert_eq!(stopped(&calls), ["key2"]);
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_registration_out_of_time_is_stopped_and_its_connection_kept() {
    let (tell, heard) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        slow: Some(("AddFiles", 2)),
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("slow", serving);
    let (folder, paths) = files("slow");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
    let started = Instant::now();
    assert!(send(&portal, &paths[..1], started + LATE / 2).is_err());
    assert!(
        started.elapsed() < LATE,
        "the registration ends by its deadline"
    );
    until_heard(&heard, "StopTransfer");
    assert_eq!(
        send(&portal, &paths[..1], soon()),
        Ok("key3".to_owned()),
        "the late reply is read and the connection goes on"
    );
    // A transfer whose time ran out before it could start writes nothing
    assert!(send(&portal, &paths[..1], Instant::now()).is_err());
    drop(portal);
    let calls = served.join().unwrap();
    assert_eq!(
        members(&calls),
        [
            "Hello",
            "StartTransfer",
            "AddFiles",
            "StartTransfer",
            "AddFiles",
            "StopTransfer",
            "StartTransfer",
            "AddFiles"
        ]
    );
    assert_eq!(stopped(&calls), ["key2"], "the first key stays");
    let _ = std::fs::remove_dir_all(folder);
}

#[test]
fn a_transfer_out_of_time_is_not_started() {
    let (address, served) = bus("late", Serving::default());
    let (folder, paths) = files("late");
    let portal = Portal::at(Some(address));
    assert!(send(&portal, &paths, Instant::now()).is_err());
    drop(portal);
    assert!(served.join().unwrap().is_empty());
    let _ = std::fs::remove_dir_all(folder);
}

/// A connection the test answers itself, as the bus would: accepted on
/// `listener`, authenticated, its unix descriptors agreed to when asked.
fn accept(listener: &UnixListener) -> UnixStream {
    let (mut s, _) = listener.accept().unwrap();
    assert!(line(&mut s).starts_with(b"\0AUTH EXTERNAL "));
    s.write_all(b"OK 0123456789abcdef\r\n").unwrap();
    let mut next = line(&mut s);
    if next == b"NEGOTIATE_UNIX_FD\r\n" {
        s.write_all(b"AGREE_UNIX_FD\r\n").unwrap();
        next = line(&mut s);
    }
    assert_eq!(next, b"BEGIN\r\n");
    s
}

/// The next call on `s`, which must be `member`.
fn next(s: &mut UnixStream, member: &str) -> Message {
    let (m, _) = receive(s).unwrap();
    assert_eq!(m.member, member);
    m
}

fn answer(s: &mut UnixStream, call: &Message, body: &[Value]) {
    let reply = wire::message(
        wire::METHOD_RETURN,
        900,
        Some(call.serial),
        "",
        "",
        "",
        "",
        body,
        0,
    );
    s.write_all(&reply).unwrap();
}

/// Answer `Hello` and `AddMatch` on a new watch connection.
fn watched(listener: &UnixListener) -> UnixStream {
    let mut watch = accept(listener);
    let hello = next(&mut watch, "Hello");
    answer(&mut watch, &hello, &[Value::str(":1.1")]);
    let rule = next(&mut watch, "AddMatch");
    answer(&mut watch, &rule, &[]);
    watch
}

/// Wait up to 5 seconds for `holds`.
fn wait_for(what: &str, holds: impl Fn() -> bool) {
    let until = Instant::now() + Duration::from_secs(5);
    while !holds() {
        assert!(Instant::now() < until, "{what}");
        std::thread::sleep(Duration::from_millis(10));
    }
}

#[test]
fn a_probe_answered_after_the_owner_changed_is_ignored_and_a_lost_watch_starts_again() {
    let path = std::env::temp_dir().join(format!("exact-transfer-watch-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let portal = Portal::at(Some(format!("unix:path={}", path.display())));
    portal.watch();
    let mut watch = watched(&listener);
    let mut asker = accept(&listener);
    let hello = next(&mut asker, "Hello");
    answer(&mut asker, &hello, &[Value::str(":1.2")]);
    let first = next(&mut asker, "Get");
    let version = [Value::Variant(Box::new(Value::U32(1)))];
    let probes = portal.state().generation;
    // The portal's owner changes while the first probe waits for its answer
    let changed = wire::message(
        wire::SIGNAL,
        7,
        None,
        "",
        DBUS_PATH,
        DBUS,
        "NameOwnerChanged",
        &[
            Value::str(DOCUMENTS),
            Value::str(":1.5"),
            Value::str(":1.6"),
        ],
        0,
    );
    watch.write_all(&changed).unwrap();
    wait_for("the owner change starts another probe", || {
        portal.state().generation >= probes + 2
    });
    answer(&mut asker, &first, &version);
    let second = next(&mut asker, "Get");
    assert!(
        !portal.available(),
        "the answer from before the owner changed is ignored"
    );
    answer(&mut asker, &second, &version);
    wait_for("the new owner's answer is kept", || portal.available());
    drop(watch);
    wait_for("a lost watch leaves nothing available", || {
        !portal.available() && !portal.state().watching
    });
    portal.reprobe();
    let _watch = watched(&listener);
    let third = next(&mut asker, "Get");
    answer(&mut asker, &third, &version);
    wait_for("a lift watches and probes again", || portal.available());
    let _ = std::fs::remove_file(&path);
}

/// A start whose wait ran out may yet make a transfer: its late key is
/// stopped once it comes, whether or not another transfer follows, on the
/// connection that keeps the keys given before.
#[test]
fn a_start_out_of_time_has_its_late_key_stopped_and_the_others_kept() {
    for follows in [true, false] {
        let (tell, heard) = std::sync::mpsc::channel();
        let serving = Serving {
            connections: 1,
            slow: Some(("StartTransfer", 2)),
            tell: Some(tell),
            ..Serving::default()
        };
        let name = format!("late-start-{follows}");
        let (address, served) = bus(&name, serving);
        let (folder, paths) = files(&name);
        let portal = Portal::at(Some(address));
        assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
        assert!(send(&portal, &paths[..1], Instant::now() + LATE / 2).is_err());
        if follows {
            assert_eq!(send(&portal, &paths[..1], soon()), Ok("key3".to_owned()));
        }
        // With nothing else asked, the reader alone hears the late key, and its stop goes
        until_heard(&heard, "StopTransfer");
        drop(portal);
        let calls = served.join().unwrap();
        let mut heard = members(&calls);
        // The stop's own thread writes it beside the next transfer's calls, in either order
        heard[5..].sort();
        let expected: &[&str] = match follows {
            true => &[
                "Hello",
                "StartTransfer",
                "AddFiles",
                "StartTransfer",
                "StartTransfer",
                "AddFiles",
                "StopTransfer",
            ],
            false => &[
                "Hello",
                "StartTransfer",
                "AddFiles",
                "StartTransfer",
                "StopTransfer",
            ],
        };
        assert_eq!(heard, expected, "another transfer following: {follows}");
        assert_eq!(stopped(&calls), ["key2"], "the late key alone");
        let _ = std::fs::remove_dir_all(folder);
    }
}

/// A stop wants no reply, so nothing waits on it: the next transfer goes on
/// at once.
#[test]
fn a_stop_wants_no_reply_and_holds_up_nothing() {
    let (tell, heard) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        refuse: Some(1),
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("no-reply", serving);
    let (folder, paths) = files("no-reply");
    let portal = Portal::at(Some(address));
    assert!(send(&portal, &paths[..1], soon()).is_err());
    until_heard(&heard, "StopTransfer");
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key2".to_owned()));
    drop(portal);
    let calls = served.join().unwrap();
    assert_eq!(
        members(&calls),
        [
            "Hello",
            "StartTransfer",
            "AddFiles",
            "StopTransfer",
            "StartTransfer",
            "AddFiles"
        ]
    );
    let asks: Vec<bool> = calls.iter().map(|c| c.wants_reply).collect();
    assert_eq!(asks, [true, true, true, false, true, true]);
    let _ = std::fs::remove_dir_all(folder);
}

/// Each sender connection the bus ends is replaced, the registration it
/// could not start going on the new one; one connection at a time is made.
#[test]
fn a_sender_connection_the_bus_ended_is_replaced_each_time() {
    let serving = Serving {
        connections: 3,
        ends: Some(3),
        ..Serving::default()
    };
    let (address, served) = bus("ended", serving);
    let (folder, paths) = files("ended");
    let portal = Portal::at(Some(address));
    for key in ["key1", "key2", "key3"] {
        assert_eq!(send(&portal, &paths[..1], soon()), Ok(key.to_owned()));
    }
    assert_eq!(portal.inner.senders.load(Ordering::Relaxed), 3);
    drop(portal);
    let calls = served.join().unwrap();
    for n in 0..3 {
        assert_eq!(
            members_on(&calls, n),
            ["Hello", "StartTransfer", "AddFiles"]
        );
    }
    let _ = std::fs::remove_dir_all(folder);
}

/// The reader hands a reply to its call while another holds the writer, and
/// queues the stop of a start answered after its caller gave up, which goes
/// once the writer is free: neither waits on a write.
#[test]
fn the_reader_waits_on_no_write() {
    let (tell, heard) = std::sync::mpsc::channel();
    let (release, released) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        defer: Some((2, released)),
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("held-writer", serving);
    let (folder, paths) = files("held-writer");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
    assert_eq!(
        heard.iter().take(3).collect::<Vec<_>>(),
        ["Hello", "StartTransfer", "AddFiles"]
    );
    let sender = lock(&portal.inner.sender).clone().unwrap();
    let options = || {
        Value::dict(vec![
            ("writable", Value::Bool(true)),
            ("autostop", Value::Bool(true)),
        ])
    };
    // The late start reaches the bus first, so its reply comes before the other's
    let late = {
        let sender = sender.clone();
        std::thread::spawn(move || {
            sender.call(
                "StartTransfer",
                &[options()],
                &[],
                Instant::now() + LATE / 2,
                true,
            )
        })
    };
    assert_eq!(heard.recv().unwrap(), "StartTransfer");
    let answered = {
        let sender = sender.clone();
        std::thread::spawn(move || sender.call("StartTransfer", &[options()], &[], soon(), true))
    };
    assert_eq!(heard.recv().unwrap(), "StartTransfer");
    assert!(late.join().unwrap().is_err(), "its caller stopped waiting");
    let held = lock(&sender.out);
    release.send(()).unwrap();
    let reply = answered.join().unwrap();
    assert!(
        matches!(reply.as_deref(), Ok([Value::Str(_)])),
        "a reply comes while the writer is held: {reply:?}"
    );
    assert!(
        heard.recv_timeout(LATE).is_err(),
        "the late key's stop waits for the writer"
    );
    drop(held);
    until_heard(&heard, "StopTransfer");
    drop((sender, portal));
    let calls = served.join().unwrap();
    assert_eq!(stopped(&calls), ["key2"], "the late key alone");
    let _ = std::fs::remove_dir_all(folder);
}

/// A stop whose write runs out of time before it begins is tried again on
/// the same connection, and goes once the writer is free.
#[test]
fn a_stop_that_could_not_be_written_is_tried_again() {
    let (tell, heard) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("stop-again", serving);
    let (folder, paths) = files("stop-again");
    let portal = Portal::at(Some(address));
    let transfer = portal.send(1, |n| path_only(&paths[n]), soon()).unwrap();
    assert_eq!(
        heard.iter().take(3).collect::<Vec<_>>(),
        ["Hello", "StartTransfer", "AddFiles"]
    );
    let sender = lock(&portal.inner.sender).clone().unwrap();
    let held = lock(&sender.out);
    let before = sender.serial.load(Ordering::Relaxed);
    portal.stop(&transfer);
    // Each attempt takes a serial: two while the writer is held, the first having run out
    let until = Instant::now() + 2 * (STOP_WRITE + STOP_PACE);
    while sender.serial.load(Ordering::Relaxed) < before + 2 {
        assert!(Instant::now() < until, "the stop was not tried again");
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        heard.try_recv().is_err(),
        "nothing was written while the writer was held"
    );
    drop(held);
    until_heard(&heard, "StopTransfer");
    drop((sender, portal));
    let calls = served.join().unwrap();
    assert_eq!(
        members(&calls),
        ["Hello", "StartTransfer", "AddFiles", "StopTransfer"]
    );
    assert_eq!(stopped(&calls), ["key1"]);
    let _ = std::fs::remove_dir_all(folder);
}

/// The reader sets only its own way's timeout on the socket it shares with
/// the writers: a message it reads leaves the send timeout a writer set.
#[test]
fn the_reader_leaves_the_writers_timeout() {
    let (push, pushed) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        push: Some(pushed),
        ..Serving::default()
    };
    let (address, served) = bus("ways", serving);
    let mut reader =
        super::super::bus::Bus::connect_passing(&address, super::super::TIMEOUT, None).unwrap();
    let writer = reader.writer().unwrap();
    let set = Duration::from_secs(7);
    writer.set_write_timeout(Some(set)).unwrap();
    writer
        .set_read_timeout(Some(Duration::from_secs(9)))
        .unwrap();
    // A message no call asked for, read here
    push.send(()).unwrap();
    assert_eq!(reader.next().unwrap().member, "Z");
    assert_eq!(
        writer.write_timeout().unwrap(),
        Some(set),
        "the read left the send timeout"
    );
    assert_eq!(
        writer.read_timeout().unwrap(),
        Some(super::super::TIMEOUT),
        "the read set its own"
    );
    drop((reader, writer, push));
    let _ = served.join();
}

/// Bytes that are no message end the reader, which wakes the calls waiting
/// and ends the connection before another is made; the next transfer goes
/// on a new one.
#[test]
fn a_reader_that_meets_no_message_ends_its_connection() {
    let (tell, heard) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 2,
        garbage: Some(5),
        tell: Some(tell),
        ..Serving::default()
    };
    let (address, served) = bus("garbage", serving);
    let (folder, paths) = files("garbage");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
    let started = Instant::now();
    assert!(
        send(&portal, &paths[..1], soon()).is_err(),
        "its AddFiles is answered by no message"
    );
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "the waiting call wakes at once"
    );
    // The client ended the connection itself, before any other is made
    let mut ended = false;
    while let Ok(told) = heard.recv_timeout(Duration::from_secs(5)) {
        ended |= told == "end 0";
        if ended {
            break;
        }
    }
    assert!(ended, "the bus sees the connection's end");
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key3".to_owned()));
    drop(portal);
    let calls = served.join().unwrap();
    assert_eq!(
        members_on(&calls, 1),
        ["Hello", "StartTransfer", "AddFiles"]
    );
    let _ = std::fs::remove_dir_all(folder);
}

/// With the bus reading nothing, writes fill the socket and each ends by
/// its call's deadline.
#[test]
fn a_full_socket_holds_a_write_no_longer_than_its_deadline() {
    let (release, released) = std::sync::mpsc::channel();
    let serving = Serving {
        connections: 1,
        stall: Some((3, released)),
        ..Serving::default()
    };
    let (address, served) = bus("full", serving);
    let (folder, paths) = files("full");
    let portal = Portal::at(Some(address));
    assert_eq!(send(&portal, &paths[..1], soon()), Ok("key1".to_owned()));
    let sender = lock(&portal.inner.sender).clone().unwrap();
    // Bound after the portal, so it drops first on every exit: a failed assertion releases the fake's socket before
    // the portal waits for its writer
    let release = release;
    let big = Value::str(&"x".repeat(64 << 10));
    // The calls run on a thread of their own, so a write that never ends fails the test by its own deadline
    let (done, calls) = std::sync::mpsc::channel();
    let caller = {
        let sender = sender.clone();
        std::thread::spawn(move || {
            for _ in 0..8 {
                let started = Instant::now();
                let deadline = started + Duration::from_millis(200);
                let failed = sender
                    .call("StopTransfer", &[big.clone()], &[], deadline, false)
                    .unwrap_err();
                let _ = done.send((failed, started.elapsed()));
            }
        })
    };
    let mut failures = Vec::new();
    for _ in 0..8 {
        match calls.recv_timeout(Duration::from_secs(5)) {
            Ok((failed, took)) => {
                assert!(took < Duration::from_millis(700), "{took:?}");
                failures.push(failed);
            }
            Err(_) => {
                drop(release);
                panic!(
                    "a call was still writing 5 seconds after its 200 ms deadline: {failures:?}"
                );
            }
        }
    }
    caller.join().unwrap();
    // The bus kept the connection open, so the first write that failed ran out of time
    let written = |e: &String| {
        e.contains("did not take the call in time")
            || e.contains("took only part of the call in time")
    };
    let first = failures
        .iter()
        .find(|e| !e.contains("did not answer in time"));
    assert!(first.is_some_and(written), "{failures:?}");
    drop((sender, portal, release));
    let _ = served.join();
    let _ = std::fs::remove_dir_all(folder);
}
