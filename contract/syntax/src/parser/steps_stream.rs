//! `answer stream` step parser (LLP 1103).

use super::steps::json_string;
use super::*;

enum StreamRow {
    Event {
        data: String,
        name: Option<String>,
        id: Option<String>,
        after_ms: u32,
        span: Span,
    },
    Bytes {
        base64: String,
        after_ms: u32,
        span: Span,
    },
    End {
        after_ms: u32,
        span: Span,
    },
    Fail {
        message: String,
        after_ms: u32,
        span: Span,
    },
}

impl StreamRow {
    fn after_ms(&self) -> u32 {
        match self {
            Self::Event { after_ms, .. }
            | Self::Bytes { after_ms, .. }
            | Self::End { after_ms, .. }
            | Self::Fail { after_ms, .. } => *after_ms,
        }
    }

    fn span(&self) -> Span {
        match self {
            Self::Event { span, .. }
            | Self::Bytes { span, .. }
            | Self::End { span, .. }
            | Self::Fail { span, .. } => *span,
        }
    }
}

impl Parser {
    /// Parse `answer stream "<prefix>" [method <METHOD>] with <status> [header "<k>" "<v>"]…`
    /// with either an optional indented schedule block (streaming 2xx) or an inline buffered body.
    pub(super) fn answer_stream(&mut self, span: Span) -> R<Step> {
        self.expect_word("stream")?;
        let prefix = self.launch_str("answer stream", "a URL prefix")?;
        if prefix.contains(['\t', '\r', '\n']) {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: "answer stream: URL prefix cannot contain tabs or newlines".into(),
                span,
            });
        }
        let method = if self.at_ident("method") {
            self.next();
            let (name, _) = self.ident()?;
            Some(name.to_ascii_uppercase())
        } else {
            None
        };
        self.expect_word("with")?;
        let status_num = self.step_number("an HTTP status from 200 through 599")?;
        if status_num.fract() != 0.0 || !(200.0..=599.0).contains(&status_num) {
            return self.err(
                "syntax-expected-step",
                "answer stream takes a streaming 2xx status (200-299 except 204, 205) or a buffered status (204, 205, 304, 400-599)",
            );
        }
        let status = status_num as u16;
        let streaming = (200..=299).contains(&status) && status != 204 && status != 205;
        let buffered = matches!(status, 204 | 205 | 304) || (400..=599).contains(&status);
        if !streaming && !buffered {
            return self.err(
                "syntax-expected-step",
                "answer stream refuses redirect and other 3xx statuses except 304; use 200-299 (except 204, 205) for a stream or 204, 205, 304, 400-599 for a buffered reply",
            );
        }

        let mut headers = Vec::new();
        let mut explicit_content_type: Option<String> = None;
        while self.at_ident("header") {
            self.next();
            let name = self.str_lit("a header name")?.to_ascii_lowercase();
            let value = self.str_lit("a header value")?;
            if name.is_empty()
                || !name
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"!#$%&'*+.^_`|~-".contains(&b))
            {
                return self.err(
                    "syntax-expected-step",
                    "answer stream: header name must be a non-empty HTTP token",
                );
            }
            if value
                .bytes()
                .any(|b| b != b'\t' && !(0x20..=0x7e).contains(&b) && b < 0x80)
            {
                return self.err(
                    "syntax-expected-step",
                    "answer stream: header value contains invalid control characters",
                );
            }
            if matches!(name.as_str(), "set-cookie" | "set-cookie2") {
                return self.err(
                    "syntax-expected-step",
                    "answer stream cannot expose set-cookie or set-cookie2 headers on every host",
                );
            }
            if name == "content-type" {
                explicit_content_type = Some(media_type(&value));
            }
            headers.push(format!("{}:{}", json_string(&name), json_string(&value)));
        }

        let stream = if streaming {
            if method.as_deref() == Some("HEAD") {
                return Err(SyntaxError {
                    id: "syntax-expected-step",
                    message: "answer stream: explicit `method HEAD` cannot declare a streaming 2xx schedule".into(),
                    span,
                });
            }
            if !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof) {
                return self.err(
                    "syntax-expected-step",
                    "answer stream with a streaming 2xx status takes no inline body or inline `after`; write `event`, `bytes`, `end`, or `fail` rows in an indented block",
                );
            }
            self.newline()?;
            let rows = self.block(|p| p.stream_row())?;
            encode_stream_schedule(status, headers, explicit_content_type.as_deref(), &rows)?
        } else {
            let no_body = matches!(status, 204 | 205 | 304);
            let (body, default_ct) = if no_body {
                if !self.at_ident("after")
                    && !matches!(self.peek_kind(), TokenKind::Newline | TokenKind::Eof)
                {
                    return self.err(
                        "syntax-expected-step",
                        format!("answer stream: status {status} cannot carry a response body"),
                    );
                }
                (String::new(), "text/plain; charset=utf-8")
            } else {
                match self.peek_kind() {
                    _ if self.at_ident("after") => (String::new(), "text/plain; charset=utf-8"),
                    TokenKind::Newline | TokenKind::Eof => {
                        (String::new(), "text/plain; charset=utf-8")
                    }
                    TokenKind::Str(_) => {
                        (self.str_lit("a text body")?, "text/plain; charset=utf-8")
                    }
                    _ => (self.json_value()?, "application/json"),
                }
            };
            if explicit_content_type.is_none() {
                headers.push(format!("\"content-type\":{}", json_string(default_ct)));
            }
            let delay = if self.at_ident("after") {
                let ms = self.parse_after_ms("answer stream")?;
                format!(",\"afterMs\":{ms}")
            } else {
                String::new()
            };
            self.newline()?;
            if matches!(self.peek_kind(), TokenKind::Indent) {
                return self.err(
                    "syntax-expected-step",
                    format!(
                        "answer stream: buffered status {status} takes no indented schedule block"
                    ),
                );
            }
            if body.len() > 65536 {
                return Err(SyntaxError {
                    id: "syntax-expected-step",
                    message: "answer stream: encoded fixture exceeds 64 KiB".into(),
                    span,
                });
            }
            format!(
                "{{\"status\":{status},\"headers\":{{{}}},\"body\":{}{delay}}}",
                headers.join(","),
                json_string(&body)
            )
        };

        if prefix.len() + stream.len() + 20 > 65536 {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: "answer stream: encoded fixture exceeds 64 KiB".into(),
                span,
            });
        }

        Ok(Step::AnswerStream {
            prefix,
            method,
            stream,
            span,
        })
    }

    fn stream_row(&mut self) -> R<StreamRow> {
        let (word, span) = match self.peek_kind().clone() {
            TokenKind::Ident(w) => (w, self.peek().span),
            other => {
                return self.err(
                    "syntax-expected-step",
                    format!(
                        "expected `event`, `bytes`, `end`, or `fail` in an `answer stream` schedule, found {}",
                        describe(&other)
                    ),
                )
            }
        };
        self.next();
        let row = match word.as_str() {
            "event" => {
                let data = match self.peek_kind() {
                    TokenKind::Str(_) => self.str_lit("event data")?,
                    _ => self.json_value()?,
                };
                let mut name = None;
                let mut id = None;
                loop {
                    if self.at_ident("name") {
                        if name.is_some() {
                            return self.err(
                                "syntax-expected-step",
                                "answer stream: `name` may appear at most once on an `event` row",
                            );
                        }
                        self.next();
                        name = Some(self.str_lit("an event name")?);
                    } else if self.at_ident("id") {
                        if id.is_some() {
                            return self.err(
                                "syntax-expected-step",
                                "answer stream: `id` may appear at most once on an `event` row",
                            );
                        }
                        self.next();
                        id = Some(self.str_lit("an event id")?);
                    } else {
                        break;
                    }
                }
                let after_ms = if self.at_ident("after") {
                    self.parse_after_ms("answer stream")?
                } else {
                    0
                };
                StreamRow::Event {
                    data,
                    name,
                    id,
                    after_ms,
                    span,
                }
            }
            "bytes" => {
                let is_base64 = if self.at_ident("base64") {
                    self.next();
                    true
                } else {
                    false
                };
                let payload_span = self.peek().span;
                let payload = self.str_lit("byte chunk payload")?;
                let base64 = if is_base64 {
                    let bytes = decode_base64(&payload).map_err(|msg| SyntaxError {
                        id: "syntax-expected-step",
                        message: format!("answer stream: invalid base64 payload: {msg}"),
                        span: payload_span,
                    })?;
                    encode_base64(&bytes)
                } else {
                    encode_base64(payload.as_bytes())
                };
                let after_ms = if self.at_ident("after") {
                    self.parse_after_ms("answer stream")?
                } else {
                    0
                };
                StreamRow::Bytes {
                    base64,
                    after_ms,
                    span,
                }
            }
            "end" => {
                let after_ms = if self.at_ident("after") {
                    self.parse_after_ms("answer stream")?
                } else {
                    0
                };
                StreamRow::End { after_ms, span }
            }
            "fail" => {
                let message = self.str_lit("the stream failure message")?;
                let after_ms = if self.at_ident("after") {
                    self.parse_after_ms("answer stream")?
                } else {
                    0
                };
                StreamRow::Fail {
                    message,
                    after_ms,
                    span,
                }
            }
            other => {
                return Err(SyntaxError {
                    id: "syntax-expected-step",
                    message: format!(
                        "expected `event`, `bytes`, `end`, or `fail` in an `answer stream` schedule, found `{other}`"
                    ),
                    span,
                })
            }
        };
        self.newline()?;
        Ok(row)
    }

    fn parse_after_ms(&mut self, context: &str) -> R<u32> {
        self.next(); // consume `after`
        let ms = self.step_number("a delay in milliseconds")?;
        if ms.fract() != 0.0 || !(0.0..=u32::MAX as f64).contains(&ms) {
            return self.err(
                "syntax-expected-step",
                format!(
                    "{context}: after takes a nonnegative whole number of milliseconds up to 4294967295"
                ),
            );
        }
        if self.at_ident("ms") {
            self.next();
        }
        Ok(ms as u32)
    }
}

fn encode_stream_schedule(
    status: u16,
    mut headers: Vec<String>,
    explicit_content_type: Option<&str>,
    rows: &[StreamRow],
) -> R<String> {
    let mut saw_event = false;
    let mut saw_bytes = false;
    let mut saw_terminal = false;
    let mut prev_after: u32 = 0;

    for row in rows {
        if saw_terminal {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: "answer stream: no row may follow `end` or `fail`".into(),
                span: row.span(),
            });
        }
        let after = row.after_ms();
        if after < prev_after {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: format!(
                    "answer stream: `after {after}` is earlier than the previous row's `after {prev_after}`; offsets are cumulative from stream open and must be non-decreasing"
                ),
                span: row.span(),
            });
        }
        prev_after = after;
        match row {
            StreamRow::Event { .. } => saw_event = true,
            StreamRow::Bytes { .. } => saw_bytes = true,
            StreamRow::End { .. } | StreamRow::Fail { .. } => saw_terminal = true,
        }
        if saw_event && saw_bytes {
            return Err(SyntaxError {
                id: "syntax-expected-step",
                message: "answer stream: cannot mix `event` and `bytes` rows in one schedule"
                    .into(),
                span: row.span(),
            });
        }
    }

    if saw_event {
        if let Some(ct) = explicit_content_type {
            if ct != "text/event-stream" {
                return Err(SyntaxError {
                    id: "syntax-expected-step",
                    message: format!(
                        "answer stream: `event` rows require content-type `text/event-stream`, found `{ct}`"
                    ),
                    span: rows
                        .iter()
                        .find(|r| matches!(r, StreamRow::Event { .. }))
                        .unwrap()
                        .span(),
                });
            }
        }
    }
    if saw_bytes && explicit_content_type == Some("text/event-stream") {
        return Err(SyntaxError {
            id: "syntax-expected-step",
            message: "answer stream: `bytes` rows cannot use content-type `text/event-stream`"
                .into(),
            span: rows
                .iter()
                .find(|r| matches!(r, StreamRow::Bytes { .. }))
                .unwrap()
                .span(),
        });
    }

    let mode = if saw_bytes {
        "bytes"
    } else if saw_event {
        "events"
    } else if explicit_content_type.is_some_and(|ct| ct != "text/event-stream") {
        "bytes"
    } else {
        "events"
    };

    if explicit_content_type.is_none() {
        let default_ct = if mode == "bytes" {
            "application/octet-stream"
        } else {
            "text/event-stream"
        };
        headers.push(format!("\"content-type\":{}", json_string(default_ct)));
    }

    let mut steps = Vec::with_capacity(rows.len());
    for row in rows {
        match row {
            StreamRow::Event {
                data,
                name,
                id,
                after_ms,
                ..
            } => {
                let mut item = format!("{{\"kind\":\"event\",\"data\":{}", json_string(data));
                if let Some(name) = name {
                    item.push_str(",\"name\":");
                    item.push_str(&json_string(name));
                }
                if let Some(id) = id {
                    item.push_str(",\"id\":");
                    item.push_str(&json_string(id));
                }
                item.push_str(&format!(",\"afterMs\":{after_ms}}}"));
                steps.push(item);
            }
            StreamRow::Bytes {
                base64, after_ms, ..
            } => {
                steps.push(format!(
                    "{{\"kind\":\"bytes\",\"base64\":{},\"afterMs\":{after_ms}}}",
                    json_string(base64)
                ));
            }
            StreamRow::End { after_ms, .. } => {
                steps.push(format!("{{\"kind\":\"end\",\"afterMs\":{after_ms}}}"));
            }
            StreamRow::Fail {
                message, after_ms, ..
            } => {
                steps.push(format!(
                    "{{\"kind\":\"fail\",\"message\":{},\"afterMs\":{after_ms}}}",
                    json_string(message)
                ));
            }
        }
    }

    Ok(format!(
        "{{\"status\":{status},\"headers\":{{{}}},\"mode\":\"{mode}\",\"steps\":[{}]}}",
        headers.join(","),
        steps.join(",")
    ))
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
