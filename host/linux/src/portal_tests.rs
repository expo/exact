//! The portal's requests and answers as data, and whole picks against a
//! stand-in session bus on a socket of the test's own: no desktop runs
//! here, so what a real portal draws is not tested.
use super::wire::{self, Message, Value};
use super::*;
use std::io::{Read, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::time::Duration;

fn picker(kind: Kind, multiple: bool, suggested_name: &str) -> Picker {
    Picker {
        kind,
        id: "opened".into(),
        multiple,
        suggested_name: suggested_name.into(),
    }
}

#[test]
fn a_message_reads_back_as_it_was_written() {
    let filters = Value::Array(
        "(sa(us))".into(),
        vec![Value::Struct(vec![
            Value::str("Markdown"),
            Value::Array(
                "(us)".into(),
                vec![Value::Struct(vec![Value::U32(0), Value::str("*.md")])],
            ),
        ])],
    );
    let body = [
        Value::str(""),
        Value::str("Open File"),
        Value::dict(vec![
            ("handle_token", Value::str("t1")),
            ("multiple", Value::Bool(true)),
            ("filters", filters.clone()),
            (
                "current_folder",
                Value::Array(
                    "y".into(),
                    b"/tmp\0".iter().map(|b| Value::Byte(*b)).collect(),
                ),
            ),
            ("none", Value::Array("s".into(), vec![])),
        ]),
    ];
    let m = wire::message(
        wire::METHOD_CALL,
        9,
        None,
        "org.freedesktop.portal.Desktop",
        "/org/freedesktop/portal/desktop",
        "org.freedesktop.portal.FileChooser",
        "OpenFile",
        &body,
    );
    let head: [u8; 16] = m[..16].try_into().unwrap();
    assert_eq!(wire::message_len(&head), Some(m.len()));
    let read = wire::parse(&m).unwrap();
    assert_eq!(
        (read.kind, read.serial, read.member.as_str()),
        (wire::METHOD_CALL, 9, "OpenFile")
    );
    assert_eq!(read.signature, "ssa{sv}");
    assert_eq!(read.body, body);
    assert_eq!(read.body[2].get("filters"), Some(&filters));
    assert_eq!(read.body[2].get("multiple"), Some(&Value::Bool(true)));
    // Cut short, or claiming more than it holds, it does not read.
    assert_eq!(wire::parse(&m[..m.len() - 1]), None);
    let mut long = m.clone();
    long[4..8].copy_from_slice(&(64u32 << 20).to_le_bytes());
    assert_eq!(wire::message_len(&long[..16].try_into().unwrap()), None);
}

#[test]
fn each_picker_asks_its_own_method_and_options() {
    let (member, options) = request(&picker(Kind::Open, true, ""), "tok");
    assert_eq!(member, "OpenFile");
    assert_eq!(options.signature(), "a{sv}");
    assert_eq!(options.get("handle_token"), Some(&Value::str("tok")));
    assert_eq!(options.get("multiple"), Some(&Value::Bool(true)));
    assert_eq!(options.get("directory"), None);
    let (member, options) = request(&picker(Kind::Directory, false, ""), "tok");
    assert_eq!(
        (member, options.get("directory")),
        ("OpenFile", Some(&Value::Bool(true)))
    );
    let (member, options) = request(&picker(Kind::Save, false, "notes.md"), "tok");
    assert_eq!(
        (member, options.get("current_name")),
        ("SaveFile", Some(&Value::str("notes.md")))
    );
    assert_eq!(
        options.get("filters"),
        None,
        "a Linux binary has no file_handlers"
    );
    assert_eq!(
        request_path(":1.42", "tok"),
        "/org/freedesktop/portal/desktop/request/1_42/tok"
    );
}

#[test]
fn a_response_is_the_local_files_chosen_a_cancel_or_a_refusal() {
    let uris = |list: &[&str]| {
        Value::dict(vec![(
            "uris",
            Value::Array("s".into(), list.iter().map(|u| Value::str(u)).collect()),
        )])
    };
    let chosen = outcome(&[
        Value::U32(0),
        uris(&[
            "file:///home/a/My%20Notes/x.md",
            "sftp://host/y.md",
            "file://localhost/tmp/z",
            "file:///bad%2",
        ]),
    ]);
    assert_eq!(
        chosen,
        Outcome::Chosen(vec![
            PathBuf::from("/home/a/My Notes/x.md"),
            PathBuf::from("/tmp/z")
        ])
    );
    assert_eq!(
        outcome(&[Value::U32(0), Value::dict(vec![])]),
        Outcome::Chosen(vec![])
    );
    assert_eq!(outcome(&[Value::U32(1), uris(&[])]), Outcome::Cancelled);
    assert!(
        matches!(outcome(&[Value::U32(2), uris(&[])]), Outcome::Refused(r) if r.contains("ended"))
    );
    assert!(matches!(outcome(&[Value::str("?")]), Outcome::Refused(_)));
    assert_eq!(path_of("file://other-host/x"), None);
    assert_eq!(path_of("file:///caf%C3%A9"), Some(PathBuf::from("/café")));
}

#[test]
fn a_bus_address_names_its_unix_socket() {
    assert_eq!(
        socket_of("unix:path=/run/user/1000/bus"),
        Some(Socket::Path("/run/user/1000/bus".into()))
    );
    assert_eq!(
        socket_of("unix:abstract=/tmp/dbus-AbC,guid=0123"),
        Some(Socket::Abstract(b"/tmp/dbus-AbC".to_vec()))
    );
    assert_eq!(
        socket_of("tcp:host=localhost,port=1;unix:guid=1,path=/tmp/a%20b"),
        Some(Socket::Path("/tmp/a b".into()))
    );
    assert_eq!(socket_of("tcp:host=localhost,port=1"), None);
    assert_eq!(socket_of(""), None);
}

#[test]
fn with_no_session_bus_a_pick_is_refused_at_once() {
    let ask = picker(Kind::Open, false, "");
    let none = choose(&ask, None, Duration::from_secs(1));
    assert!(
        matches!(&none, Outcome::Refused(r) if r.contains("no session bus")),
        "{none:?}"
    );
    assert!(matches!(
        choose(&ask, Some(""), Duration::from_secs(1)),
        Outcome::Refused(_)
    ));
    let gone = choose(
        &ask,
        Some("unix:path=/nonexistent/exact-bus"),
        Duration::from_secs(1),
    );
    assert!(
        matches!(&gone, Outcome::Refused(r) if r.contains("did not answer")),
        "{gone:?}"
    );
}

/// How the stand-in portal behaves.
#[derive(Clone, Copy)]
enum Portal {
    /// Its chooser answers `Response` with this code and these URIs.
    Answers(u32, &'static [&'static str]),
    /// It has no `FileChooser`.
    Absent,
    /// Its `FileChooser` is version 2.
    Old,
    /// It quits with the chooser up.
    Quits,
    /// The bus never says `Hello`.
    Silent,
}

fn read_message(s: &mut UnixStream) -> Option<Message> {
    let mut head = [0u8; 16];
    s.read_exact(&mut head).ok()?;
    let mut buf = head.to_vec();
    buf.resize(wire::message_len(&head)?, 0);
    s.read_exact(&mut buf[16..]).ok()?;
    wire::parse(&buf)
}

fn line(s: &mut UnixStream) -> Vec<u8> {
    let mut out = Vec::new();
    let mut b = [0u8; 1];
    while !out.ends_with(b"\r\n") && s.read_exact(&mut b).is_ok() {
        out.push(b[0]);
    }
    out
}

/// An error reply to `serial`: the writer names no error, so the header
/// is written here.
fn error(serial: u32, name: &str, text: &str) -> Vec<u8> {
    let field =
        |code: u8, v: Value| Value::Struct(vec![Value::Byte(code), Value::Variant(Box::new(v))]);
    let fields = vec![
        field(4, Value::str(name)),
        field(5, Value::U32(serial)),
        field(8, Value::Sig("s".into())),
    ];
    let mut body = Vec::new();
    wire::write(&mut body, &Value::str(text));
    let mut m = Vec::new();
    for v in [
        Value::Byte(b'l'),
        Value::Byte(wire::ERROR),
        Value::Byte(0),
        Value::Byte(1),
        Value::U32(body.len() as u32),
        Value::U32(2),
        Value::Array("(yv)".into(), fields),
    ] {
        wire::write(&mut m, &v);
    }
    while !m.len().is_multiple_of(8) {
        m.push(0);
    }
    m.extend_from_slice(&body);
    m
}

/// A session bus with one portal behind it, on a socket of its own; its
/// address, and what the portal was asked.
fn bus(name: &str, portal: Portal) -> (String, std::thread::JoinHandle<Vec<Message>>) {
    let path = std::env::temp_dir().join(format!("exact-bus-{name}-{}", std::process::id()));
    let _ = std::fs::remove_file(&path);
    let listener = UnixListener::bind(&path).unwrap();
    let address = format!("unix:path={}", path.display());
    let served = std::thread::spawn(move || {
        let (mut s, _) = listener.accept().unwrap();
        let _ = std::fs::remove_file(&path);
        let mut asked = Vec::new();
        assert!(line(&mut s).starts_with(b"\0AUTH EXTERNAL "));
        s.write_all(b"OK 0123456789abcdef\r\n").unwrap();
        assert_eq!(line(&mut s), b"BEGIN\r\n");
        let mut serial = 100;
        let mut send = |s: &mut UnixStream,
                        kind,
                        reply: Option<u32>,
                        path: &str,
                        iface: &str,
                        member: &str,
                        body: &[Value]| {
            serial += 1;
            let _ = s.write_all(&wire::message(
                kind, serial, reply, "", path, iface, member, body,
            ));
        };
        while let Some(m) = read_message(&mut s) {
            asked.push(m.clone());
            let reply = Some(m.serial);
            match m.member.as_str() {
                "Hello" if matches!(portal, Portal::Silent) => {}
                "Hello" => send(
                    &mut s,
                    wire::METHOD_RETURN,
                    reply,
                    "",
                    "",
                    "",
                    &[Value::str(":1.7")],
                ),
                "Get" if matches!(portal, Portal::Absent) => {
                    let unknown = "org.freedesktop.DBus.Error.UnknownInterface";
                    s.write_all(&error(m.serial, unknown, "No such interface"))
                        .unwrap();
                }
                "Get" => {
                    let version = if matches!(portal, Portal::Old) { 2 } else { 4 };
                    send(
                        &mut s,
                        wire::METHOD_RETURN,
                        reply,
                        "",
                        "",
                        "",
                        &[Value::Variant(Box::new(Value::U32(version)))],
                    );
                }
                "AddMatch" => send(&mut s, wire::METHOD_RETURN, reply, "", "", "", &[]),
                "OpenFile" | "SaveFile" => {
                    let token = match m.body[2].get("handle_token") {
                        Some(Value::Str(t)) => t.clone(),
                        _ => panic!("no handle_token"),
                    };
                    let handle = request_path(":1.7", &token);
                    send(
                        &mut s,
                        wire::METHOD_RETURN,
                        reply,
                        "",
                        "",
                        "",
                        &[Value::Path(handle.clone())],
                    );
                    // Another request's answer is not this one's.
                    let other = request_path(":1.7", "other");
                    let empty = Value::dict(vec![]);
                    send(
                        &mut s,
                        wire::SIGNAL,
                        None,
                        &other,
                        "org.freedesktop.portal.Request",
                        "Response",
                        &[Value::U32(1), empty],
                    );
                    match portal {
                        Portal::Answers(code, uris) => {
                            let uris = Value::Array(
                                "s".into(),
                                uris.iter().map(|u| Value::str(u)).collect(),
                            );
                            let results = Value::dict(vec![("uris", uris)]);
                            send(
                                &mut s,
                                wire::SIGNAL,
                                None,
                                &handle,
                                "org.freedesktop.portal.Request",
                                "Response",
                                &[Value::U32(code), results],
                            );
                        }
                        Portal::Quits => {
                            let names = [
                                Value::str("org.freedesktop.portal.Desktop"),
                                Value::str(":1.3"),
                                Value::str(""),
                            ];
                            send(
                                &mut s,
                                wire::SIGNAL,
                                None,
                                "/org/freedesktop/DBus",
                                "org.freedesktop.DBus",
                                "NameOwnerChanged",
                                &names,
                            );
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
        asked
    });
    (address, served)
}

#[test]
fn a_pick_is_asked_of_the_portal_and_its_answer_heard() {
    let (address, served) = bus(
        "open",
        Portal::Answers(0, &["file:///home/a/one.md", "file:///home/a/two%23.md"]),
    );
    let got = choose(
        &picker(Kind::Open, true, ""),
        Some(&address),
        Duration::from_secs(5),
    );
    assert_eq!(
        got,
        Outcome::Chosen(vec!["/home/a/one.md".into(), "/home/a/two#.md".into()])
    );
    let asked = served.join().unwrap();
    let members: Vec<&str> = asked.iter().map(|m| m.member.as_str()).collect();
    assert_eq!(
        members,
        ["Hello", "Get", "AddMatch", "AddMatch", "OpenFile"],
        "it listens before it asks"
    );
    let open = &asked[4];
    assert_eq!(
        (open.destination.as_str(), open.interface.as_str()),
        (
            "org.freedesktop.portal.Desktop",
            "org.freedesktop.portal.FileChooser"
        )
    );
    assert_eq!(open.body[..2], [Value::str(""), Value::str("Open File")]);
    assert_eq!(open.body[2].get("multiple"), Some(&Value::Bool(true)));
    let Value::Str(rule) = &asked[2].body[0] else {
        panic!()
    };
    assert!(
        rule.contains("member='Response',path='/org/freedesktop/portal/desktop/request/1_7/exact"),
        "{rule}"
    );

    let (address, _) = bus("save", Portal::Answers(1, &[]));
    assert_eq!(
        choose(
            &picker(Kind::Save, false, "a.md"),
            Some(&address),
            Duration::from_secs(5)
        ),
        Outcome::Cancelled
    );
}

#[test]
fn a_missing_old_quitting_or_silent_portal_refuses_and_never_waits_on() {
    let refusal = |name: &str, portal: Portal, kind: Kind| {
        let (address, _) = bus(name, portal);
        match choose(
            &picker(kind, false, "a.md"),
            Some(&address),
            Duration::from_millis(300),
        ) {
            Outcome::Refused(r) => r,
            other => panic!("{name}: {other:?}"),
        }
    };
    let absent = refusal("absent", Portal::Absent, Kind::Open);
    assert!(
        absent.contains("no desktop portal file chooser") && absent.contains("UnknownInterface"),
        "{absent}"
    );
    let old = refusal("old", Portal::Old, Kind::Directory);
    assert!(old.contains("version 2"), "{old}");
    assert!(refusal("quits", Portal::Quits, Kind::Save).contains("quit"));
    let silent = refusal("silent", Portal::Silent, Kind::Open);
    assert!(silent.contains("did not answer"), "{silent}");
    // Version 2 still opens files.
    let (address, _) = bus("old-file", Portal::Old);
    let open = std::thread::spawn(move || {
        choose(
            &picker(Kind::Open, false, ""),
            Some(&address),
            Duration::from_millis(300),
        )
    });
    std::thread::sleep(Duration::from_millis(500));
    assert!(
        !open.is_finished(),
        "the chooser is up, and the person takes their time"
    );
}

/// A pick through the presenter: the element hears `change` with the
/// chosen file's `doc:` path, or `cancel` with the refusal journalled.
#[test]
fn the_element_hears_what_the_portal_answered() {
    use crate::presenter::{PainterChoice, Presenter};
    const APP: &str = r#"component App
  state opened = "none"
  state cancels = 0
  action got(value)
    opened = value
  action cancelled
    cancels = cancels + 1
  view
    column
      input id="opened" display="none" change=got cancel=cancelled
      text opened testId="opened-value"
      text `${cancels}` testId="cancels"
"#;
    #[derive(Default)]
    struct Nothing;
    impl exact_runner::DataSource for Nothing {
        fn query(
            &mut self,
            n: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Err(exact_runner::DataError::UnknownSource(n.into()))
        }
    }
    let (mut p, _) = Presenter::boot_with(
        &crate::test_plan(APP),
        Nothing,
        (400., 400.),
        1.,
        PathBuf::new(),
        PainterChoice::Cpu,
    )
    .unwrap();
    let k = p.host().kernel();
    let view = k.node_by_key(k.find_by_id("opened")[0]).unwrap().id;
    let text = |p: &Presenter<Nothing>, id: &str| {
        let k = p.host().kernel();
        let node = k.node_by_key(k.find_by_test_id(id)[0]).unwrap();
        node.props
            .str(exact_kernel::PropId::Text)
            .unwrap()
            .to_owned()
    };
    let until = |p: &mut Presenter<Nothing>, id: &str, not: &str| {
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while text(p, id) == not && std::time::Instant::now() < deadline {
            p.pump(p.host().now());
            std::thread::sleep(Duration::from_millis(5));
        }
        text(p, id)
    };
    p.ask_portal(
        picker(Kind::Open, false, ""),
        view,
        None,
        Duration::from_secs(1),
    );
    assert_eq!(until(&mut p, "cancels", "0"), "1");
    let journal = |p: &Presenter<Nothing>| {
        p.host()
            .runner()
            .journal()
            .map(str::to_owned)
            .collect::<Vec<_>>()
    };
    let refused = "showOpenFilePicker: refused: no desktop portal: there is no session bus";
    assert!(
        journal(&p).iter().any(|l| l.ends_with(refused)),
        "{:?}",
        journal(&p)
    );

    let file = std::env::temp_dir().join(format!("exact-portal-chosen-{}.md", std::process::id()));
    std::fs::write(&file, "# chosen").unwrap();
    let uri: &'static str = Box::leak(format!("file://{}", file.display()).into_boxed_str());
    let uris: &'static [&'static str] = Box::leak(vec![uri].into_boxed_slice());
    let (address, _) = bus("presenter", Portal::Answers(0, uris));
    p.ask_portal(
        picker(Kind::Open, false, ""),
        view,
        Some(address),
        Duration::from_secs(5),
    );
    let opened = until(&mut p, "opened-value", "none");
    let name = file.file_name().unwrap().to_str().unwrap();
    assert!(
        opened.starts_with("doc:/") && opened.ends_with(name),
        "{opened}"
    );
    assert!(
        journal(&p)
            .iter()
            .any(|l| l.ends_with("showOpenFilePicker: chosen")),
        "{:?}",
        journal(&p)
    );
    let _ = std::fs::remove_file(file);
}
