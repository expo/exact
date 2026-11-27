use exact_runner::{FailureKind, Outcome, Response, SurfaceOutcome};

/// A reply as the ABI carries it, as the runner's `Outcome`. Kind 8 is a
/// stream's messages that arrived together (LLP 1016.000 D4): a block of
/// header lines each (`event`, `id`, `coalesced`, `protocol`, and with more
/// than one, `length`, its data's bytes), blocks apart by an empty line, and
/// their data one after another as the body. One block is one message.
pub fn outcome_from(kind: u32, status: u32, headers: &str, body: Vec<u8>) -> Outcome {
    match kind {
        8 => {
            let mut at = 0usize;
            let mut messages: Vec<_> = (headers.split("\n\n"))
                .map(|block| {
                    let field = |name: &str| {
                        (block.lines())
                            .find_map(|l| l.strip_prefix(name)?.strip_prefix(": "))
                            .unwrap_or("")
                            .to_string()
                    };
                    let end = field("length")
                        .parse()
                        .map_or(body.len(), |n: usize| at.saturating_add(n).min(body.len()));
                    let data = String::from_utf8_lossy(&body[at.min(end)..end]).into_owned();
                    at = end;
                    exact_runner::Message {
                        event: field("event"),
                        protocol: field("protocol").into(),
                        id: field("id"),
                        coalesced: field("coalesced").parse().unwrap_or(0),
                        data,
                        binary: None,
                        head: None,
                    }
                })
                .collect();
            match messages.len() {
                1 => Outcome::Message(messages.remove(0)),
                _ => Outcome::Messages(messages),
            }
        }
        11 => match serde_json::from_slice::<Vec<(String, String)>>(&body) {
            Ok(headers) => {
                Outcome::Message(exact_runner::Message::http_open(status as u16, headers))
            }
            Err(e) => Outcome::Failed {
                kind: FailureKind::Refused,
                message: format!("malformed stream open headers: {e}"),
            },
        },
        5 => Outcome::Storage(body),
        6 if body.len() <= exact_runner::MAX_HOST_WORK_BYTES => {
            Outcome::Surface(SurfaceOutcome::Captured(body))
        }
        6 => Outcome::Failed {
            kind: FailureKind::Refused,
            message: "surface capture exceeds 16 MiB".into(),
        },
        7 => Outcome::Surface(SurfaceOutcome::Restored),
        0 => Outcome::Response(Response {
            status: status as u16,
            // A line that starts with a colon is a pseudo-header the page
            // names (`:exact-body-to`, exactBodyTo's size): no browser header
            // name has a colon, so it keeps its leading one.
            headers: headers
                .lines()
                .filter_map(|l| {
                    let (pseudo, l) = l.strip_prefix(':').map_or((false, l), |l| (true, l));
                    l.split_once(':').map(|(k, v)| {
                        let k = k.trim();
                        let k = if pseudo { format!(":{k}") } else { k.into() };
                        (k, v.trim().to_string())
                    })
                })
                .collect(),
            body,
        }),
        k => Outcome::Failed {
            kind: match k {
                2 => FailureKind::Refused,
                3 => FailureKind::Unsupported,
                4 => FailureKind::Aborted,
                10 => FailureKind::Timeout,
                _ => FailureKind::Network,
            },
            message: String::from_utf8_lossy(&body).into_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kind_11_decodes_http_open_head_with_json_header_pairs() {
        let pairs = vec![
            ("content-type".to_string(), "text/event-stream".to_string()),
            ("mcp-session-id".to_string(), "abc:123".to_string()),
            ("x-dup".to_string(), "one".to_string()),
            ("x-dup".to_string(), "two".to_string()),
        ];
        let body = serde_json::to_vec(&pairs).unwrap();
        let outcome = outcome_from(11, 200, "", body);
        assert_eq!(
            outcome,
            Outcome::Message(exact_runner::Message::http_open(200, pairs))
        );
        assert!(matches!(
            outcome_from(11, 200, "", b"not-json".to_vec()),
            Outcome::Failed {
                kind: FailureKind::Refused,
                ..
            }
        ));
    }
}
