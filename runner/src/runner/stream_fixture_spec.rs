//! Scripted HTTP stream fixture specification (`answer stream`, LLP 1103).

use crate::Response;
use std::collections::BTreeMap;

/// Stream schedule delivery mode: server-sent events or raw byte chunks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StreamMode {
    /// `text/event-stream` SSE events.
    Events,
    /// Non-SSE byte chunks.
    Bytes,
}

impl StreamMode {
    /// Wire name in JSON.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Events => "events",
            Self::Bytes => "bytes",
        }
    }

    /// Real HTTP EOF message delivered when a schedule finishes with `end`.
    pub fn eof_message(self) -> &'static str {
        match self {
            Self::Events => "the event stream ended",
            Self::Bytes => "the byte stream ended",
        }
    }
}

/// One scheduled step in a streaming 2xx fixture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamStep {
    /// Deliver one SSE event.
    Event {
        /// Event data string.
        data: String,
        /// Optional SSE `event:` type.
        name: Option<String>,
        /// Optional SSE `id:`.
        id: Option<String>,
        /// Cumulative milliseconds from stream open.
        after_ms: u32,
    },
    /// Deliver one byte chunk.
    Bytes {
        /// Decoded payload bytes.
        bytes: Vec<u8>,
        /// Canonical RFC 4648 base64 representation for wire round-trip.
        base64: String,
        /// Cumulative milliseconds from stream open.
        after_ms: u32,
    },
    /// End the stream cleanly (HTTP EOF).
    End {
        /// Cumulative milliseconds from stream open.
        after_ms: u32,
    },
    /// Fail the stream mid-flight with a network error.
    Fail {
        /// Error message.
        message: String,
        /// Cumulative milliseconds from stream open.
        after_ms: u32,
    },
}

impl StreamStep {
    /// Cumulative offset from stream open in milliseconds.
    pub fn after_ms(&self) -> u32 {
        match self {
            Self::Event { after_ms, .. }
            | Self::Bytes { after_ms, .. }
            | Self::End { after_ms }
            | Self::Fail { after_ms, .. } => *after_ms,
        }
    }
}

/// Parsed `answer stream` fixture: either a streaming 2xx schedule or a buffered reply.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StreamFixture {
    /// Non-2xx, 204, 205, or 304 terminal buffered response.
    Buffered {
        /// Final HTTP response.
        response: Response,
        /// Optional delay before delivery in milliseconds.
        after_ms: u32,
    },
    /// Streaming 2xx schedule (200–299 except 204 and 205).
    Schedule {
        /// HTTP status code.
        status: u16,
        /// Lowercase response headers.
        headers: Vec<(String, String)>,
        /// Event stream or byte stream.
        mode: StreamMode,
        /// Monotonic schedule steps.
        steps: Vec<StreamStep>,
    },
}

impl StreamFixture {
    /// Whether this fixture is a streaming 2xx schedule.
    pub fn is_streaming(&self) -> bool {
        matches!(self, Self::Schedule { .. })
    }

    /// Whether opening this fixture immediately counts as an arranged failure (status >= 400).
    pub fn is_buffered_failure(&self) -> bool {
        matches!(self, Self::Buffered { response, .. } if response.status >= 400)
    }
}

/// Parse and validate a stream fixture from JSON (`answer stream` or `prefer.faults`).
pub fn parse_stream_fixture(value: &serde_json::Value) -> Result<StreamFixture, String> {
    let bad = |msg: &str| format!("answer stream: {msg}");
    let status = value
        .get("status")
        .and_then(|v| v.as_u64())
        .filter(|n| (200..=599).contains(n))
        .ok_or_else(|| bad("status must be 200-299 (except 204, 205) or 204, 205, 304, 400-599"))?
        as u16;
    let streaming = (200..=299).contains(&status) && status != 204 && status != 205;
    let buffered = matches!(status, 204 | 205 | 304) || (400..=599).contains(&status);
    if !streaming && !buffered {
        return Err(bad("redirect and 3xx statuses other than 304 are refused"));
    }

    let raw_headers = value
        .get("headers")
        .and_then(|v| v.as_object())
        .ok_or_else(|| bad("headers must be an object of strings"))?;
    if raw_headers.len() > 1024 {
        return Err(bad("at most 1024 response headers"));
    }
    let mut headers = Vec::with_capacity(raw_headers.len());
    let mut explicit_ct: Option<String> = None;
    for (name, val) in raw_headers {
        let val = val
            .as_str()
            .ok_or_else(|| bad("header values must be strings"))?;
        if name.eq_ignore_ascii_case("set-cookie")
            || name.eq_ignore_ascii_case("set-cookie2")
            || name.is_empty()
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
            || val.chars().any(|c| {
                c != '\t' && !((' '..='~').contains(&c) || ('\u{80}'..='\u{ff}').contains(&c))
            })
        {
            return Err(bad("invalid header name or value"));
        }
        let lower = name.to_ascii_lowercase();
        let trimmed = val.trim_matches([' ', '\t']).to_string();
        if lower == "content-type" {
            explicit_ct = Some(media_type(&trimmed));
        }
        headers.push((lower, trimmed));
    }

    if buffered {
        if value.get("steps").is_some() || value.get("mode").is_some() {
            return Err(bad("buffered status cannot declare a streaming schedule"));
        }
        let body = value
            .get("body")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad("buffered reply requires a string body"))?;
        if body.len() > 64 * 1024 || (matches!(status, 204 | 205 | 304) && !body.is_empty()) {
            return Err(bad("invalid buffered body for status"));
        }
        let after_ms = parse_after(value.get("afterMs"))?;
        return Ok(StreamFixture::Buffered {
            response: Response {
                status,
                headers,
                body: body.as_bytes().to_vec(),
            },
            after_ms,
        });
    }

    if value.get("body").is_some() || value.get("afterMs").is_some() {
        return Err(bad(
            "streaming 2xx schedule takes steps, not inline body or afterMs",
        ));
    }
    let mode = match value.get("mode").and_then(|v| v.as_str()) {
        Some("events") => StreamMode::Events,
        Some("bytes") => StreamMode::Bytes,
        _ => return Err(bad("mode must be \"events\" or \"bytes\"")),
    };
    let raw_steps = value
        .get("steps")
        .and_then(|v| v.as_array())
        .ok_or_else(|| bad("streaming 2xx fixture requires a steps array"))?;

    let mut steps = Vec::with_capacity(raw_steps.len());
    let mut prev_after: u32 = 0;
    let mut saw_terminal = false;
    let mut saw_event = false;
    let mut saw_bytes = false;

    for raw in raw_steps {
        if saw_terminal {
            return Err(bad("no step may follow end or fail"));
        }
        let kind = raw
            .get("kind")
            .and_then(|v| v.as_str())
            .ok_or_else(|| bad("each step requires a kind"))?;
        let after_ms = parse_after(raw.get("afterMs"))?;
        if after_ms < prev_after {
            return Err(bad(
                "step afterMs values must be monotonically non-decreasing",
            ));
        }
        prev_after = after_ms;
        match kind {
            "event" => {
                if mode != StreamMode::Events || saw_bytes {
                    return Err(bad(
                        "event steps require mode \"events\" and cannot mix with bytes",
                    ));
                }
                saw_event = true;
                let data = raw
                    .get("data")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| bad("event step requires string data"))?
                    .to_string();
                let name = match raw.get("name") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(_) => return Err(bad("event name must be a string")),
                };
                let id = match raw.get("id") {
                    None | Some(serde_json::Value::Null) => None,
                    Some(serde_json::Value::String(s)) => Some(s.clone()),
                    Some(_) => return Err(bad("event id must be a string")),
                };
                steps.push(StreamStep::Event {
                    data,
                    name,
                    id,
                    after_ms,
                });
            }
            "bytes" => {
                if mode != StreamMode::Bytes || saw_event {
                    return Err(bad(
                        "bytes steps require mode \"bytes\" and cannot mix with events",
                    ));
                }
                saw_bytes = true;
                let b64 = raw
                    .get("base64")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| bad("bytes step requires base64 payload"))?;
                let bytes = decode_base64(b64).map_err(|m| bad(&format!("invalid base64: {m}")))?;
                let base64 = encode_base64(&bytes);
                steps.push(StreamStep::Bytes {
                    bytes,
                    base64,
                    after_ms,
                });
            }
            "end" => {
                saw_terminal = true;
                steps.push(StreamStep::End { after_ms });
            }
            "fail" => {
                saw_terminal = true;
                let message = raw
                    .get("message")
                    .and_then(|v| v.as_str())
                    .ok_or_else(|| bad("fail step requires a string message"))?
                    .to_string();
                steps.push(StreamStep::Fail { message, after_ms });
            }
            _ => return Err(bad("step kind must be event, bytes, end, or fail")),
        }
    }

    match (mode, explicit_ct.as_deref()) {
        (StreamMode::Events, Some(ct)) if ct != "text/event-stream" => {
            return Err(bad(
                "event schedule requires content-type text/event-stream",
            ));
        }
        (StreamMode::Bytes, Some("text/event-stream")) => {
            return Err(bad(
                "bytes schedule cannot use content-type text/event-stream",
            ));
        }
        _ => {}
    }
    if explicit_ct.is_none() {
        let default_ct = match mode {
            StreamMode::Events => "text/event-stream",
            StreamMode::Bytes => "application/octet-stream",
        };
        headers.push(("content-type".into(), default_ct.into()));
    }

    Ok(StreamFixture::Schedule {
        status,
        headers,
        mode,
        steps,
    })
}

/// Encode a stream fixture to canonical JSON for `state.faults` and TSV wire round-trips.
pub fn stream_fixture_json(fixture: &StreamFixture) -> serde_json::Value {
    match fixture {
        StreamFixture::Buffered { response, after_ms } => {
            let mut value = serde_json::json!({
                "status": response.status,
                "headers": response.headers.iter().cloned().collect::<BTreeMap<_, _>>(),
                "body": String::from_utf8_lossy(&response.body),
            });
            if *after_ms != 0 {
                value["afterMs"] = (*after_ms).into();
            }
            value
        }
        StreamFixture::Schedule {
            status,
            headers,
            mode,
            steps,
        } => {
            let encoded_steps: Vec<serde_json::Value> = steps
                .iter()
                .map(|step| match step {
                    StreamStep::Event {
                        data,
                        name,
                        id,
                        after_ms,
                    } => {
                        let mut obj = serde_json::json!({
                            "kind": "event",
                            "data": data,
                            "afterMs": after_ms,
                        });
                        if let Some(name) = name {
                            obj["name"] = name.clone().into();
                        }
                        if let Some(id) = id {
                            obj["id"] = id.clone().into();
                        }
                        obj
                    }
                    StreamStep::Bytes {
                        base64, after_ms, ..
                    } => serde_json::json!({
                        "kind": "bytes",
                        "base64": base64,
                        "afterMs": after_ms,
                    }),
                    StreamStep::End { after_ms } => serde_json::json!({
                        "kind": "end",
                        "afterMs": after_ms,
                    }),
                    StreamStep::Fail { message, after_ms } => serde_json::json!({
                        "kind": "fail",
                        "message": message,
                        "afterMs": after_ms,
                    }),
                })
                .collect();
            serde_json::json!({
                "status": status,
                "headers": headers.iter().cloned().collect::<BTreeMap<_, _>>(),
                "mode": mode.as_str(),
                "steps": encoded_steps,
            })
        }
    }
}

fn parse_after(value: Option<&serde_json::Value>) -> Result<u32, String> {
    match value {
        None | Some(serde_json::Value::Null) => Ok(0),
        Some(v) => v
            .as_u64()
            .and_then(|n| u32::try_from(n).ok())
            .ok_or_else(|| "answer stream: invalid delay in milliseconds".into()),
    }
}

fn media_type(value: &str) -> String {
    value
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase()
}

fn encode_base64(bytes: &[u8]) -> String {
    const TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b0 = chunk[0] as u32;
        let b1 = chunk.get(1).copied().unwrap_or(0) as u32;
        let b2 = chunk.get(2).copied().unwrap_or(0) as u32;
        let triple = (b0 << 16) | (b1 << 8) | b2;
        out.push(TABLE[((triple >> 18) & 0x3f) as usize] as char);
        out.push(TABLE[((triple >> 12) & 0x3f) as usize] as char);
        out.push(if chunk.len() > 1 {
            TABLE[((triple >> 6) & 0x3f) as usize] as char
        } else {
            '='
        });
        out.push(if chunk.len() > 2 {
            TABLE[(triple & 0x3f) as usize] as char
        } else {
            '='
        });
    }
    out
}

fn decode_base64(input: &str) -> Result<Vec<u8>, &'static str> {
    let bytes = input.as_bytes();
    if !bytes.len().is_multiple_of(4) {
        return Err("length must be a multiple of 4");
    }
    let val = |b: u8| -> Option<u8> {
        match b {
            b'A'..=b'Z' => Some(b - b'A'),
            b'a'..=b'z' => Some(b - b'a' + 26),
            b'0'..=b'9' => Some(b - b'0' + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let mut out = Vec::with_capacity((bytes.len() / 4) * 3);
    for (i, chunk) in bytes.chunks_exact(4).enumerate() {
        let last = (i + 1) * 4 == bytes.len();
        let a = val(chunk[0]).ok_or("unexpected character")?;
        let b = val(chunk[1]).ok_or("unexpected character")?;
        if chunk[2] == b'=' {
            if !last || chunk[3] != b'=' {
                return Err("misplaced padding");
            }
            if (b & 0x0f) != 0 {
                return Err("non-zero trailing bits");
            }
            out.push((a << 2) | (b >> 4));
        } else if chunk[3] == b'=' {
            if !last {
                return Err("misplaced padding");
            }
            let c = val(chunk[2]).ok_or("unexpected character")?;
            if (c & 0x03) != 0 {
                return Err("non-zero trailing bits");
            }
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
        } else {
            let c = val(chunk[2]).ok_or("unexpected character")?;
            let d = val(chunk[3]).ok_or("unexpected character")?;
            out.push((a << 2) | (b >> 4));
            out.push((b << 4) | (c >> 2));
            out.push((c << 6) | d);
        }
    }
    Ok(out)
}
