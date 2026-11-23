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
        start_in: None,
    }
}

fn declared_filters() -> Vec<Filter> {
    filters(
        r#"{"inputs":{"fileHandlers":[
          {"action":"/","name":"Markdown","accept":{"text/markdown":[".md",".markdown"]}},
          {"action":"/","name":"Board","accept":{"application/vnd.studio.board+json":[".board"]}},
          {"action":"/","name":"Archive","accept":{"application/octet-stream":[".tar.gz",".c++"]}},
          {"action":"/","name":"Text","accept":{"text/plain":[".txt"]}},
          {"action":"/","name":"JSON","accept":{"application/json":[]}},
          {"action":"/","name":"Folder","accept":{"inode/directory":[]}}
        ]}}"#,
    )
    .unwrap()
}

fn filter_rule(kind: u32, value: &str) -> Value {
    Value::Struct(vec![Value::U32(kind), Value::str(value)])
}

fn markdown_filter_value() -> Value {
    Value::Struct(vec![
        Value::str("Markdown"),
        Value::Array(
            "(us)".into(),
            vec![
                filter_rule(0, "*.[mM][dD]"),
                filter_rule(0, "*.[mM][aA][rR][kK][dD][oO][wW][nN]"),
            ],
        ),
    ])
}

fn offered_filters() -> Value {
    Value::Array(
        "(sa(us))".into(),
        vec![
            markdown_filter_value(),
            Value::Struct(vec![
                Value::str("Board"),
                Value::Array(
                    "(us)".into(),
                    vec![filter_rule(0, "*.[bB][oO][aA][rR][dD]")],
                ),
            ]),
            Value::Struct(vec![
                Value::str("Archive"),
                Value::Array(
                    "(us)".into(),
                    vec![
                        filter_rule(0, "*.[tT][aA][rR].[gG][zZ]"),
                        filter_rule(0, "*.[cC]++"),
                    ],
                ),
            ]),
            Value::Struct(vec![
                Value::str("Text"),
                Value::Array("(us)".into(), vec![filter_rule(0, "*.[tT][xX][tT]")]),
            ]),
            Value::Struct(vec![
                Value::str("JSON"),
                Value::Array("(us)".into(), vec![filter_rule(1, "application/json")]),
            ]),
        ],
    )
}

#[test]
fn declared_extensions_are_literal_portal_rules() {
    assert_eq!(
        declared_filters(),
        vec![
            Filter {
                name: "Markdown".into(),
                rules: vec![
                    (0, "*.[mM][dD]".into()),
                    (0, "*.[mM][aA][rR][kK][dD][oO][wW][nN]".into()),
                ],
            },
            Filter {
                name: "Board".into(),
                rules: vec![(0, "*.[bB][oO][aA][rR][dD]".into())],
            },
            Filter {
                name: "Archive".into(),
                rules: vec![
                    (0, "*.[tT][aA][rR].[gG][zZ]".into()),
                    (0, "*.[cC]++".into()),
                ],
            },
            Filter {
                name: "Text".into(),
                rules: vec![(0, "*.[tT][xX][tT]".into())],
            },
            Filter {
                name: "JSON".into(),
                rules: vec![(1, "application/json".into())],
            },
        ],
        "a directory handler is not a file filter"
    );

    for extension in ["", ".", ".MD", "board", ".md.", ".m_d", ".m-d", ".mé"] {
        let compat = serde_json::json!({"inputs":{"fileHandlers":[{
            "action":"/", "accept":{"text/markdown":[extension]}
        }]}})
        .to_string();
        assert!(filters(&compat).is_err(), "{extension:?}");
    }
    for name in ["", "Markdown\0files", "Markdown\nfiles"] {
        let compat = serde_json::json!({"inputs":{"fileHandlers":[{
            "action":"/", "name":name, "accept":{"text/markdown":[".md"]}
        }]}})
        .to_string();
        assert_eq!(
            filters(&compat).unwrap_err(),
            "file handler 0's name is not a valid label",
            "{name:?}"
        );
    }
    let error = filters("not json").unwrap_err();
    assert!(
        error.contains("not JSON: expected ident at line 1 column 2"),
        "{error}"
    );
    assert_eq!(filters(r#"{"inputs":{}}"#).unwrap(), []);
}

#[test]
fn manifest_compatibility_metadata_becomes_portal_filters() {
    let dir = std::env::temp_dir().join(format!(
        "exact-portal-manifest-compatibility-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let result = std::panic::catch_unwind(|| {
        std::fs::write(
            dir.join("app.json"),
            r#"{"name":"Documents","app":{"id":"test.documents","name":"Documents"},"file_handlers":[{"action":"/","name":"Board","accept":{"application/x-studio-board+json":["board"]}},{"action":"/","name":"Markdown","accept":{"text/markdown":[".MD"]}}]}"#,
        )
        .unwrap();
        std::fs::write(
            dir.join("app.contract"),
            "component App\n  view\n    text \"Documents\"\n",
        )
        .unwrap();
        let manifest = contract::Manifest::read(&dir).unwrap();
        let compat = exact_bake::compatibility_id(
            &dir,
            "linux",
            "x86_64-unknown-linux-gnu",
            &manifest,
            Some(""),
            false,
        )
        .unwrap();
        assert_eq!(
            filters(&compat.to_json()).unwrap(),
            vec![
                Filter {
                    name: "Board".into(),
                    rules: vec![(0, "*.[bB][oO][aA][rR][dD]".into())],
                },
                Filter {
                    name: "Markdown".into(),
                    rules: vec![(0, "*.[mM][dD]".into())],
                },
            ]
        );
    });
    std::fs::remove_dir_all(&dir).unwrap();
    if let Err(error) = result {
        std::panic::resume_unwind(error);
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
    let filters = declared_filters();
    let offered = offered_filters();
    let (member, options) = request(&picker(Kind::Open, true, ""), "tok", &filters, None);
    assert_eq!(member, "OpenFile");
    assert_eq!(options.signature(), "a{sv}");
    assert_eq!(options.get("handle_token"), Some(&Value::str("tok")));
    assert_eq!(options.get("multiple"), Some(&Value::Bool(true)));
    assert_eq!(options.get("directory"), None);
    assert_eq!(options.get("filters"), Some(&offered));
    assert_eq!(
        options.get("current_filter"),
        Some(&markdown_filter_value())
    );
    let (member, options) = request(&picker(Kind::Directory, false, ""), "tok", &filters, None);
    assert_eq!(
        (member, options.get("directory")),
        ("OpenFile", Some(&Value::Bool(true)))
    );
    assert_eq!(options.get("filters"), None);
    assert_eq!(options.get("current_filter"), None);
    let (member, options) = request(
        &picker(Kind::Save, false, "notes.md"),
        "tok",
        &filters,
        None,
    );
    assert_eq!(
        (member, options.get("current_name")),
        ("SaveFile", Some(&Value::str("notes.md")))
    );
    assert_eq!(options.get("filters"), Some(&offered));
    assert_eq!(
        options.get("current_filter"),
        Some(&markdown_filter_value())
    );

    let (_, options) = request(&picker(Kind::Open, false, ""), "tok", &[], None);
    assert_eq!(
        options.get("filters"),
        None,
        "an app that declares no file_handlers offers every file"
    );
    assert_eq!(
        request_path(":1.42", "tok"),
        "/org/freedesktop/portal/desktop/request/1_42/tok"
    );
}

/// LLP 1173 D2: `startIn`'s folder is `current_folder`, a NUL-terminated
/// byte string, on `OpenFile` and `SaveFile` alike.
#[test]
fn a_start_folder_is_the_portals_current_folder() {
    let bytes = |s: &str| {
        let b = s.bytes().chain([0]).map(Value::Byte).collect();
        Value::Array("y".into(), b)
    };
    let folder = std::path::Path::new("/home/ana/Bilder/Bildschirmfotos");
    for (kind, member) in [
        (Kind::Open, "OpenFile"),
        (Kind::Directory, "OpenFile"),
        (Kind::Save, "SaveFile"),
    ] {
        let (asked, options) = request(&picker(kind, false, "a.png"), "tok", &[], Some(folder));
        assert_eq!(asked, member);
        assert_eq!(
            options.get("current_folder"),
            Some(&bytes("/home/ana/Bilder/Bildschirmfotos"))
        );
        let (_, options) = request(&picker(kind, false, "a.png"), "tok", &[], None);
        assert_eq!(options.get("current_folder"), None);
    }
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
    let none = choose(&ask, &[], None, None, Duration::from_secs(1));
    assert!(
        matches!(&none, Outcome::Refused(r) if r.contains("no session bus")),
        "{none:?}"
    );
    assert!(matches!(
        choose(&ask, &[], None, Some(""), Duration::from_secs(1)),
        Outcome::Refused(_)
    ));
    let gone = choose(
        &ask,
        &[],
        None,
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
    #[cfg(target_os = "linux")]
    Settings(&'static str),
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
                #[cfg(target_os = "linux")]
                "ReadAll" | "ReadOne" if matches!(portal, Portal::Settings(method) if method != m.member) =>
                {
                    s.write_all(&error(
                        m.serial,
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        &m.member,
                    ))
                    .unwrap();
                }
                #[cfg(target_os = "linux")]
                "ReadAll" | "Read" | "ReadOne" => {
                    let namespace = "org.freedesktop.appearance";
                    let key = "color-scheme";
                    let variant = |v| Value::Variant(Box::new(v));
                    let path = "/org/freedesktop/portal/desktop";
                    let iface = "org.freedesktop.portal.Settings";
                    let setting = |ns, n| [Value::str(ns), Value::str(key), variant(Value::U32(n))];

                    send(
                        &mut s,
                        wire::SIGNAL,
                        None,
                        path,
                        iface,
                        "SettingChanged",
                        &setting(namespace, 2),
                    );
                    let gnome = "org.gnome.desktop.interface";
                    let result = if m.member == "ReadAll" {
                        assert_eq!(
                            m.body,
                            [Value::Array(
                                "s".into(),
                                vec![Value::str(namespace), Value::str(gnome)]
                            )]
                        );
                        let group = |ns, entries| {
                            Value::Entry(Box::new(Value::str(ns)), Box::new(Value::dict(entries)))
                        };
                        Value::Array(
                            "{sa{sv}}".into(),
                            vec![
                                group(namespace, vec![(key, Value::U32(1))]),
                                group(
                                    gnome,
                                    vec![
                                        ("cursor-blink", Value::Bool(false)),
                                        ("cursor-blink-time", Value::U32(640)),
                                        ("cursor-blink-timeout", Value::U32(3)),
                                    ],
                                ),
                            ],
                        )
                    } else {
                        assert_eq!(m.body, [Value::str(namespace), Value::str(key)]);
                        let value = variant(Value::U32(1));
                        if m.member == "Read" {
                            variant(value)
                        } else {
                            value
                        }
                    };
                    send(&mut s, wire::METHOD_RETURN, reply, "", "", "", &[result]);
                    for (ns, n) in [("other", 0), (namespace, 2), (namespace, 2), (namespace, 0)] {
                        send(
                            &mut s,
                            wire::SIGNAL,
                            None,
                            path,
                            iface,
                            "SettingChanged",
                            &setting(ns, n),
                        );
                    }
                    if m.member == "ReadAll" {
                        send(
                            &mut s,
                            wire::SIGNAL,
                            None,
                            path,
                            iface,
                            "SettingChanged",
                            &[
                                Value::str(gnome),
                                Value::str("cursor-blink"),
                                variant(Value::Bool(true)),
                            ],
                        );
                    }
                }
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

#[cfg(target_os = "linux")]
#[test]
fn desktop_appearance_follows_the_settings_portal() {
    use crate::desktop::{CursorSettings, WindowSettings};
    for method in ["ReadAll", "ReadOne", "Read"] {
        let (address, served) = bus(method, Portal::Settings(method));
        let (send, updates) = std::sync::mpsc::channel();
        let watch = watch_settings(
            &address,
            |s| s,
            move |s| {
                let _ = send.send(s);
            },
        )
        .unwrap();
        let mut cursor = if method == "ReadAll" {
            CursorSettings {
                blink: false,
                cycle: Duration::from_millis(640),
                timeout: Duration::from_secs(3),
            }
        } else {
            CursorSettings::default()
        };
        for dark in [Some(true), Some(false), None] {
            assert_eq!(
                updates.recv_timeout(Duration::from_secs(3)).unwrap(),
                WindowSettings { dark, cursor }
            );
        }
        if method == "ReadAll" {
            cursor.blink = true;
            assert_eq!(
                updates.recv_timeout(Duration::from_secs(3)).unwrap(),
                WindowSettings { dark: None, cursor }
            );
        }
        drop(watch);
        let asked = served.join().unwrap();
        let methods: Vec<_> = asked.iter().map(|m| m.member.as_str()).collect();
        let attempts = ["Hello", "AddMatch", "ReadAll", "ReadOne", "Read"];
        let last = attempts.iter().position(|m| *m == method).unwrap();
        assert_eq!(methods, attempts[..=last]);
        assert!(updates.into_iter().next().is_none());
    }
}

#[test]
fn a_pick_is_asked_of_the_portal_and_its_answer_heard() {
    let filters = declared_filters();
    let (address, served) = bus(
        "open",
        Portal::Answers(0, &["file:///home/a/one.md", "file:///home/a/two%23.md"]),
    );
    let got = choose(
        &picker(Kind::Open, true, ""),
        &filters,
        None,
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
    assert_eq!(open.body[2].get("filters"), Some(&offered_filters()));
    assert_eq!(
        open.body[2].get("current_filter"),
        Some(&markdown_filter_value())
    );
    let Value::Str(rule) = &asked[2].body[0] else {
        panic!()
    };
    assert!(
        rule.contains("member='Response',path='/org/freedesktop/portal/desktop/request/1_7/exact"),
        "{rule}"
    );

    let (address, served) = bus("save", Portal::Answers(1, &[]));
    assert_eq!(
        choose(
            &picker(Kind::Save, false, "a.md"),
            &filters,
            Some(std::path::Path::new("/home/a/Pictures")),
            Some(&address),
            Duration::from_secs(5)
        ),
        Outcome::Cancelled
    );
    let asked = served.join().unwrap();
    let save = asked
        .iter()
        .find(|message| message.member == "SaveFile")
        .unwrap();
    assert_eq!(save.body[2].get("filters"), Some(&offered_filters()));
    assert_eq!(
        save.body[2].get("current_folder"),
        Some(&Value::Array(
            "y".into(),
            b"/home/a/Pictures\0"
                .iter()
                .map(|b| Value::Byte(*b))
                .collect()
        ))
    );
    assert_eq!(
        save.body[2].get("current_filter"),
        Some(&markdown_filter_value())
    );

    let (address, served) = bus("unfiltered", Portal::Answers(1, &[]));
    assert_eq!(
        choose(
            &picker(Kind::Open, false, ""),
            &[],
            None,
            Some(&address),
            Duration::from_secs(5)
        ),
        Outcome::Cancelled
    );
    let asked = served.join().unwrap();
    let open = asked
        .iter()
        .find(|message| message.member == "OpenFile")
        .unwrap();
    assert_eq!(open.body[2].get("filters"), None);
    assert_eq!(open.body[2].get("current_filter"), None);
}

#[test]
fn a_missing_old_quitting_or_silent_portal_refuses_and_never_waits_on() {
    let refusal = |name: &str, portal: Portal, kind: Kind| {
        let (address, _) = bus(name, portal);
        match choose(
            &picker(kind, false, "a.md"),
            &[],
            None,
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
            &[],
            None,
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
    p.compat = "{}".into();
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

    p.compat = "not json".into();
    p.ask_portal(
        picker(Kind::Open, false, ""),
        view,
        None,
        None,
        Duration::from_secs(1),
    );
    assert_eq!(until(&mut p, "cancels", "1"), "2");
    assert!(
        journal(&p)
            .iter()
            .any(|line| line
                .contains("showOpenFilePicker: refused: compatibility metadata is not JSON: expected ident at line 1 column 2")),
        "{:?}",
        journal(&p)
    );
    let before = text(&p, "opened-value");
    let folder =
        std::env::temp_dir().join(format!("exact-portal-directory-{}", std::process::id()));
    std::fs::create_dir_all(&folder).unwrap();
    let uri: &'static str = Box::leak(format!("file://{}", folder.display()).into_boxed_str());
    let uris: &'static [&'static str] = Box::leak(vec![uri].into_boxed_slice());
    let (address, _) = bus("directory-invalid-handlers", Portal::Answers(0, uris));
    p.ask_portal(
        picker(Kind::Directory, false, ""),
        view,
        None,
        Some(address),
        Duration::from_secs(5),
    );
    let opened = until(&mut p, "opened-value", &before);
    assert!(
        opened.starts_with("doc:/")
            && opened.ends_with(folder.file_name().unwrap().to_str().unwrap()),
        "{opened}"
    );
    assert!(
        journal(&p)
            .iter()
            .any(|line| line.ends_with("showDirectoryPicker: chosen")),
        "{:?}",
        journal(&p)
    );
    // The handle is where a later chooser can open (LLP 1173 D2); one the
    // session never held opens where the portal would, journalled.
    let real = std::fs::canonicalize(&folder).unwrap();
    assert_eq!(
        p.start_folder("showSaveFilePicker", Some(&opened)),
        Some(real)
    );
    assert_eq!(
        p.start_folder("showSaveFilePicker", Some("doc:/999999/x")),
        None
    );
    let refused =
        "showSaveFilePicker: startIn: refused: doc:/999999/x is not a document this session holds";
    assert!(
        journal(&p).iter().any(|line| line.ends_with(refused)),
        "{:?}",
        journal(&p)
    );
    std::fs::remove_dir_all(folder).unwrap();
    p.compat = "{}".into();

    let file = std::env::temp_dir().join(format!("exact-portal-chosen-{}.md", std::process::id()));
    std::fs::write(&file, "# chosen").unwrap();
    let uri: &'static str = Box::leak(format!("file://{}", file.display()).into_boxed_str());
    let uris: &'static [&'static str] = Box::leak(vec![uri].into_boxed_slice());
    let (address, _) = bus("presenter", Portal::Answers(0, uris));
    let before = text(&p, "opened-value");
    p.ask_portal(
        picker(Kind::Open, false, ""),
        view,
        None,
        Some(address),
        Duration::from_secs(5),
    );
    let opened = until(&mut p, "opened-value", &before);
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

    // A pick whose element is made again (a list row given another item)
    // answers nothing, whether its answer came before the renewal or after.
    for renew_first in [true, false] {
        p.ask_portal(
            picker(Kind::Open, false, ""),
            view,
            None,
            None,
            Duration::from_secs(1),
        );
        if !renew_first {
            let answered = |p: &Presenter<Nothing>| !p.portal.lock().unwrap().answered.is_empty();
            let deadline = std::time::Instant::now() + Duration::from_secs(5);
            while !answered(&p) && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(5));
            }
            assert!(answered(&p), "the portal answered");
        }
        p.picks_renewed(&[view]);
        for _ in 0..20 {
            p.pump(p.host().now());
            std::thread::sleep(Duration::from_millis(5));
        }
        assert_eq!(text(&p, "cancels"), "2", "renewed first: {renew_first}");
    }
    assert!(
        journal(&p)
            .iter()
            .any(|l| l.ends_with("was made again; its answer is dropped")),
        "{:?}",
        journal(&p)
    );
}
