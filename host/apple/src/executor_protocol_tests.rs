//! Protocol negotiation over the platform transport (LLP 1016.000).
use super::*;
use ibex2::grant::GrantSet;
use std::io::{Read, Write};

#[test]
#[ignore = "async lane: real WebSocket handshake"]
fn socket_protocols_reach_the_peer_and_report_the_selection() {
    protocols(ibex2::host::Host::new);
}

pub(crate) fn protocols(host: fn() -> ibex2::host::Host) {
    for (offer, selected, accepted) in [
        (vec!["chat.v2", "Chat.v2", "chat.v1"], Some("chat.v1"), true),
        (vec![], None, true),
        (vec!["chat.v2"], None, false),
        (vec!["chat.v2"], Some("unknown"), false),
        (vec![], Some("unknown"), false),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("ws://{}", listener.local_addr().unwrap());
        let expected = offer.join(",");
        let peer = std::thread::spawn(move || {
            let (mut wire, _) = listener.accept().unwrap();
            wire.set_read_timeout(Some(Duration::from_secs(10)))
                .unwrap();
            let mut head = Vec::new();
            let mut byte = [0];
            while !head.ends_with(b"\r\n\r\n") {
                assert_eq!(wire.read(&mut byte).unwrap(), 1);
                head.push(byte[0]);
            }
            let head = String::from_utf8(head).unwrap();
            let header = |name: &str| {
                head.lines()
                    .find_map(|line| {
                        let (key, value) = line.split_once(':')?;
                        key.eq_ignore_ascii_case(name).then(|| value.trim())
                    })
                    .unwrap_or("")
            };
            assert_eq!(header("sec-websocket-protocol").replace(" ", ""), expected);
            let accept = ibex2::stdlib::websocket::accept_key(header("sec-websocket-key"));
            let selected = selected
                .map(|s| format!("Sec-WebSocket-Protocol: {s}\r\n"))
                .unwrap_or_default();
            write!(wire, "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n{selected}\r\n").unwrap();
            let _ = wire.write_all(b"\x81\x05hello");
            let _ = wire.read(&mut byte);
        });
        let grants = GrantSet::parse(&format!("net.websocket {url}")).unwrap();
        let bindings = host().endow(grants);
        let abort = AbortController::new();
        let mut request = Request::get(&url).protocols(offer).unwrap();
        request.stream = true;
        let opened = open_socket(&bindings, request, &abort);
        assert_eq!(opened.is_ok(), accepted, "selection {selected:?}");
        if let Ok(Opened::Socket(mut socket, _)) = opened {
            assert_eq!(socket.protocol(), selected.unwrap_or(""));
            assert!(matches!(socket.next(), Ok(Incoming::Text(s)) if s == "hello"));
            abort.abort();
        }
        peer.join().unwrap();
    }
    let request = Request::get("ws://127.0.0.1:1").protocols(["bad/name"]);
    assert!(request.unwrap_err().contains("bad/name"));
}
