//! Virtual-clock delivery for scripted HTTP stream fixtures (`answer stream`, LLP 1103).
//!
//! Parked rows (`after > 0`) and virtual `exactIdleTimeout` deadlines live on this
//! lane rather than `data_timers`, so `clock settle` never advances past a parked
//! stream event while explicit `clock +<ms>` walks to each due in chronological order.

use super::super::stream_fixture_spec::{StreamFixture, StreamMode, StreamStep};
use super::super::*;
use crate::request::StreamHead;
use crate::{Dispatch, FailureKind, Message, Outcome, RequestOut, Response, Work};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

#[derive(Clone)]
#[cfg_attr(test, derive(Debug))]
pub(in crate::runner) struct StreamReqMeta {
    pub(in crate::runner) url: String,
    pub(in crate::runner) is_head: bool,
    pub(in crate::runner) stream_open: bool,
    pub(in crate::runner) bytes: bool,
    pub(in crate::runner) idle_timeout_ms: Option<u32>,
    pub(in crate::runner) max_response_bytes: usize,
    /// Set once `stream_fault_dispatch` or Wasm handoff binds an `answer stream` fixture.
    pub(in crate::runner) fixture: bool,
}

pub(in crate::runner) struct ActiveStreamFixture {
    pub(in crate::runner) ticket: u64,
    pub(in crate::runner) prefix: String,
    pub(in crate::runner) method: Option<String>,
    pub(in crate::runner) url: String,
    pub(in crate::runner) fixture: Arc<StreamFixture>,
    pub(in crate::runner) opened_at: f64,
    pub(in crate::runner) next_step: usize,
    pub(in crate::runner) max_response_bytes: usize,
    pub(in crate::runner) idle_timeout_ms: Option<u32>,
    pub(in crate::runner) last_heard_ms: f64,
    /// Only becomes true after executor admission succeeds (or on web handoff).
    pub(in crate::runner) admitted: Arc<AtomicBool>,
    /// Present when a `bytes` schedule serves a non-`exactStreamBytes` request:
    /// chunks accumulate on the virtual clock and resolve as one HTTP `Response` on `end`.
    pub(in crate::runner) buffered_body: Option<Vec<u8>>,
    /// True only when handed off from Web Wasm (`kind == 12`) so native sessions never
    /// accumulate page-mirror notifications.
    pub(in crate::runner) wasm_handoff: bool,
}

impl ActiveStreamFixture {
    fn steps(&self) -> &[StreamStep] {
        match self.fixture.as_ref() {
            StreamFixture::Schedule { steps, .. } => steps,
            StreamFixture::Buffered { .. } => &[],
        }
    }

    fn mode(&self) -> StreamMode {
        match self.fixture.as_ref() {
            StreamFixture::Schedule { mode, .. } => *mode,
            StreamFixture::Buffered { .. } => StreamMode::Events,
        }
    }

    fn head(&self) -> (u16, Vec<(String, String)>) {
        match self.fixture.as_ref() {
            StreamFixture::Schedule {
                status, headers, ..
            } => (*status, headers.clone()),
            StreamFixture::Buffered { response, .. } => (response.status, response.headers.clone()),
        }
    }

    fn step_due(&self) -> Option<f64> {
        self.steps()
            .get(self.next_step)
            .map(|step| self.opened_at + f64::from(step.after_ms()))
    }

    fn idle_due(&self) -> Option<f64> {
        self.idle_timeout_ms
            .map(|ms| self.last_heard_ms + f64::from(ms))
    }

    fn next_due(&self) -> Option<f64> {
        match (self.step_due(), self.idle_due()) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (a, b) => a.or(b),
        }
    }

    fn has_more_work(&self) -> bool {
        self.next_step < self.steps().len() || self.idle_timeout_ms.is_some()
    }
}

pub(in crate::runner) fn step_message(
    step: &StreamStep,
    max_response_bytes: usize,
) -> Result<Option<Message>, String> {
    match step {
        StreamStep::Event { data, name, id, .. } => {
            if data.len() > max_response_bytes {
                return Err("stream message exceeds limit".into());
            }
            Ok(Some(Message {
                event: name.clone().unwrap_or_default(),
                id: id.clone().unwrap_or_default(),
                data: data.clone(),
                ..Default::default()
            }))
        }
        StreamStep::Bytes { bytes, .. } => {
            if bytes.len() > max_response_bytes {
                return Err("stream message exceeds limit".into());
            }
            Ok(Some(Message {
                binary: Some(bytes.clone()),
                ..Default::default()
            }))
        }
        StreamStep::End { .. } | StreamStep::Fail { .. } => Ok(None),
    }
}

impl<D: DataSource> Runner<D> {
    /// Drain `(prefix, method, url, count_hit, count_failure)` credits queued by Web Wasm
    /// handoffs since the last batch, so the page's fault table mirrors `hits` and `failureHits`.
    pub fn take_stream_fixture_failures(
        &mut self,
    ) -> Vec<(String, Option<String>, String, bool, bool)> {
        std::mem::take(&mut self.clock.stream_fixture_failures)
    }

    pub(in crate::runner) fn record_stream_credit(
        &mut self,
        prefix: &str,
        method: Option<&str>,
        url: &str,
        count_hit: bool,
        count_failure: bool,
        wasm_handoff: bool,
    ) {
        if count_hit {
            self.faults.count_stream_hit(prefix, method, count_failure);
            self.faults
                .push_log(format!("stream answered (driver fixture): {url}"));
        } else if count_failure {
            self.faults.count_stream_failure(prefix, method);
        }
        if wasm_handoff {
            self.clock.stream_fixture_failures.push((
                prefix.to_string(),
                method.map(ToOwned::to_owned),
                url.to_string(),
                count_hit,
                count_failure,
            ));
        }
    }

    /// Open a scripted streaming 2xx fixture handed off from the web Wasm page (`kind == 12`).
    /// Authoritative request parameters (`stream_open`, `bytes`, `idle_timeout_ms`,
    /// `max_response_bytes`, `url`) come from the runner's pending stream request, not the payload.
    pub(crate) fn open_web_stream_fixture_inner(
        &mut self,
        ticket: u64,
        payload: &[u8],
    ) -> Result<Vec<Timed>, RunnerError> {
        let Some(meta) = self
            .pending
            .iter()
            .find(|p| p.ticket == ticket)
            .and_then(|p| p.stream_meta.as_deref().cloned())
        else {
            if !self.holds(ticket) {
                return Ok(Vec::new());
            }
            let outcome = Outcome::Failed {
                kind: FailureKind::Refused,
                message: "stream fixture handoff requires a pending streaming request".into(),
            };
            return Ok(self
                .fulfill_measured(ticket, outcome, None)?
                .map(|receipt| Timed {
                    at_ms: self.now_ms,
                    receipt,
                })
                .into_iter()
                .collect());
        };
        let value: serde_json::Value = match serde_json::from_slice(payload) {
            Ok(v) => v,
            Err(e) => {
                let outcome = Outcome::Failed {
                    kind: FailureKind::Refused,
                    message: format!("malformed stream fixture handoff: {e}"),
                };
                return Ok(self
                    .fulfill_measured(ticket, outcome, None)?
                    .map(|receipt| Timed {
                        at_ms: self.now_ms,
                        receipt,
                    })
                    .into_iter()
                    .collect());
            }
        };
        let prefix = value
            .get("prefix")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let method = value
            .get("method")
            .and_then(|v| v.as_str())
            .map(|m| m.to_ascii_uppercase());
        let parsed = match value
            .get("stream")
            .ok_or_else(|| "missing stream fixture".to_string())
            .and_then(super::super::stream_fixture_spec::parse_stream_fixture)
        {
            Ok(f) => Arc::new(f),
            Err(message) => {
                let outcome = Outcome::Failed {
                    kind: FailureKind::Refused,
                    message,
                };
                return Ok(self
                    .fulfill_measured(ticket, outcome, None)?
                    .map(|receipt| Timed {
                        at_ms: self.now_ms,
                        receipt,
                    })
                    .into_iter()
                    .collect());
            }
        };
        let (status, headers, mode, steps) = match parsed.as_ref() {
            StreamFixture::Buffered { response, after_ms } => {
                if !self.holds(ticket) {
                    return Ok(Vec::new());
                }
                let mut resp = response.clone();
                if meta.is_head {
                    resp.body.clear();
                }
                if resp.body.len() > meta.max_response_bytes {
                    let outcome = Outcome::Failed {
                        kind: FailureKind::Refused,
                        message: "answer stream exceeds the request's maxResponseBytes".into(),
                    };
                    return Ok(self
                        .fulfill_measured(ticket, outcome, None)?
                        .map(|receipt| Timed {
                            at_ms: self.now_ms,
                            receipt,
                        })
                        .into_iter()
                        .collect());
                }
                if *after_ms > 0 {
                    self.delay_stream_fixture_reply(
                        ticket,
                        self.now_ms,
                        *after_ms,
                        prefix,
                        method,
                        meta.url,
                        resp.status >= 400,
                        true,
                    );
                } else {
                    self.record_stream_credit(
                        &prefix,
                        method.as_deref(),
                        &meta.url,
                        true,
                        resp.status >= 400,
                        true,
                    );
                }
                return Ok(self
                    .fulfill_measured(ticket, Outcome::Response(resp), None)?
                    .map(|receipt| Timed {
                        at_ms: self.now_ms,
                        receipt,
                    })
                    .into_iter()
                    .collect());
            }
            StreamFixture::Schedule {
                status,
                headers,
                mode,
                steps,
            } => (status, headers, mode, steps),
        };
        if !self.holds(ticket) {
            return Ok(Vec::new());
        }
        let buffered_mode = *mode == StreamMode::Bytes && !meta.bytes;
        if let Some(pending) = self.pending.iter_mut().find(|p| p.ticket == ticket) {
            if let Some(m) = pending.stream_meta.as_deref_mut() {
                m.fixture = true;
            }
            pending.fixture_opened = true;
        }
        let mut receipts = Vec::new();
        if meta.stream_open && !buffered_mode {
            if let Some(receipt) = self.fulfill_measured(
                ticket,
                Outcome::Message(Message::http_open(*status, headers.clone())),
                None,
            )? {
                receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                });
            }
            if !self.holds(ticket) {
                return Ok(receipts);
            }
        }
        let mut leading = Vec::new();
        let mut buffered_body = buffered_mode.then(Vec::new);
        let mut next_step = 0usize;
        let mut terminal: Option<Outcome> = None;
        while let Some(step) = steps.get(next_step) {
            if step.after_ms() != 0 {
                break;
            }
            match step {
                StreamStep::End { .. } => {
                    next_step += 1;
                    terminal = Some(if let Some(body) = buffered_body.take() {
                        Outcome::Response(Response {
                            status: *status,
                            headers: headers.clone(),
                            body,
                        })
                    } else {
                        Outcome::Failed {
                            kind: FailureKind::Network,
                            message: mode.eof_message().into(),
                        }
                    });
                    break;
                }
                StreamStep::Fail { message, .. } => {
                    next_step += 1;
                    self.record_stream_credit(
                        &prefix,
                        method.as_deref(),
                        &meta.url,
                        false,
                        true,
                        true,
                    );
                    terminal = Some(Outcome::Failed {
                        kind: FailureKind::Network,
                        message: message.clone(),
                    });
                    break;
                }
                StreamStep::Bytes { bytes, .. } if buffered_body.is_some() => {
                    let buf = buffered_body.as_mut().unwrap();
                    if buf.len().saturating_add(bytes.len()) > meta.max_response_bytes {
                        terminal = Some(Outcome::Failed {
                            kind: FailureKind::Refused,
                            message: "answer stream exceeds the request's maxResponseBytes".into(),
                        });
                        break;
                    }
                    buf.extend_from_slice(bytes);
                    next_step += 1;
                }
                StreamStep::Event { .. } | StreamStep::Bytes { .. } => {
                    match step_message(step, meta.max_response_bytes) {
                        Ok(Some(msg)) => {
                            leading.push(msg);
                            next_step += 1;
                        }
                        Ok(None) => break,
                        Err(message) => {
                            terminal = Some(Outcome::Failed {
                                kind: FailureKind::Refused,
                                message,
                            });
                            break;
                        }
                    }
                }
            }
        }
        if !leading.is_empty() && self.holds(ticket) {
            let outcome = if leading.len() == 1 {
                Outcome::Message(leading.pop().unwrap())
            } else {
                Outcome::Messages(leading)
            };
            if let Some(receipt) = self.fulfill_measured(ticket, outcome, None)? {
                receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                });
            }
        }
        if let Some(end_outcome) = terminal {
            if self.holds(ticket) {
                if let Some(receipt) = self.fulfill_measured(ticket, end_outcome, None)? {
                    receipts.push(Timed {
                        at_ms: self.now_ms,
                        receipt,
                    });
                }
            }
            return Ok(receipts);
        }
        if self.holds(ticket) {
            self.install_stream_fixture(ActiveStreamFixture {
                ticket,
                prefix,
                method,
                url: meta.url,
                fixture: parsed,
                opened_at: self.now_ms,
                next_step,
                max_response_bytes: meta.max_response_bytes,
                idle_timeout_ms: meta.idle_timeout_ms,
                last_heard_ms: self.now_ms,
                admitted: Arc::new(AtomicBool::new(true)),
                buffered_body,
                wasm_handoff: true,
            });
        }
        Ok(receipts)
    }

    pub(in crate::runner) fn install_stream_fixture(&mut self, active: ActiveStreamFixture) {
        self.clock
            .stream_fixtures
            .retain(|existing| existing.ticket != active.ticket);
        if active.has_more_work() {
            self.clock.stream_fixtures.push(active);
        }
    }

    pub(in crate::runner) fn prune_stream_fixtures(&mut self) {
        if !self.clock.stream_fixtures.is_empty() {
            let mut items = std::mem::take(&mut self.clock.stream_fixtures);
            items.retain(|f| self.holds(f.ticket));
            self.clock.stream_fixtures = items;
        }
    }

    pub(in crate::runner) fn cancel_stream_fixture(&mut self, ticket: u64) {
        self.clock.stream_fixtures.retain(|f| f.ticket != ticket);
    }

    /// Earliest due time among admitted parked stream fixture steps and virtual idle deadlines.
    /// Deliberately excluded from `data_timer_due_ms()` so `clock settle` does not advance
    /// past parked stream events.
    pub(super) fn stream_fixture_due_ms(&self) -> Option<f64> {
        self.clock
            .stream_fixtures
            .iter()
            .filter(|f| f.admitted.load(Ordering::Acquire) && self.holds(f.ticket))
            .filter_map(ActiveStreamFixture::next_due)
            .reduce(f64::min)
    }

    /// Deliver one due stream fixture event batch, terminal step, or idle timeout at `<= at`.
    pub(super) fn release_stream_fixtures(
        &mut self,
        at: f64,
        receipts: &mut Vec<Timed>,
    ) -> Result<bool, RunnerError> {
        self.prune_stream_fixtures();
        let Some((idx, _)) = self
            .clock
            .stream_fixtures
            .iter()
            .enumerate()
            .filter(|(_, f)| f.admitted.load(Ordering::Acquire))
            .filter_map(|(i, f)| f.next_due().filter(|due| *due <= at).map(|due| (i, due)))
            .min_by(|(_, a), (_, b)| a.total_cmp(b))
        else {
            return Ok(false);
        };

        let idle_fires = {
            let entry = &self.clock.stream_fixtures[idx];
            match (entry.idle_due(), entry.step_due()) {
                (Some(idle), Some(step)) => idle <= step && idle <= at,
                (Some(idle), None) => idle <= at,
                _ => false,
            }
        };

        if idle_fires {
            let entry = self.clock.stream_fixtures.remove(idx);
            let idle_ms = entry.idle_timeout_ms.unwrap_or(0);
            self.record_stream_credit(
                &entry.prefix,
                entry.method.as_deref(),
                &entry.url,
                false,
                true,
                entry.wasm_handoff,
            );
            let outcome = Outcome::Failed {
                kind: FailureKind::Timeout,
                message: format!("nothing arrived for {idle_ms} ms"),
            };
            if let Some(receipt) = self.fulfill_measured(entry.ticket, outcome, None)? {
                receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                });
            }
            return Ok(true);
        }

        let delivered: Option<(u64, Outcome)> = {
            let entry = &mut self.clock.stream_fixtures[idx];
            let fixture = entry.fixture.clone();
            let steps = match fixture.as_ref() {
                StreamFixture::Schedule { steps, .. } => steps.as_slice(),
                StreamFixture::Buffered { .. } => &[],
            };
            let first = &steps[entry.next_step];
            let due_after_ms = first.after_ms();
            match first {
                StreamStep::End { .. } => {
                    let (status, headers) = entry.head();
                    let message = entry.mode().eof_message().to_string();
                    let mut entry = self.clock.stream_fixtures.remove(idx);
                    let outcome = if let Some(body) = entry.buffered_body.take() {
                        Outcome::Response(Response {
                            status,
                            headers,
                            body,
                        })
                    } else {
                        Outcome::Failed {
                            kind: FailureKind::Network,
                            message,
                        }
                    };
                    Some((entry.ticket, outcome))
                }
                StreamStep::Fail { message, .. } => {
                    let message = message.clone();
                    let entry = self.clock.stream_fixtures.remove(idx);
                    self.record_stream_credit(
                        &entry.prefix,
                        entry.method.as_deref(),
                        &entry.url,
                        false,
                        true,
                        entry.wasm_handoff,
                    );
                    Some((
                        entry.ticket,
                        Outcome::Failed {
                            kind: FailureKind::Network,
                            message,
                        },
                    ))
                }
                StreamStep::Bytes { .. } if entry.buffered_body.is_some() => {
                    let ticket = entry.ticket;
                    let limit = entry.max_response_bytes;
                    let mut refused = None;
                    let mut cursor = entry.next_step;
                    let mut buf = entry.buffered_body.take().unwrap();
                    while let Some(step) = steps.get(cursor) {
                        if step.after_ms() != due_after_ms {
                            break;
                        }
                        if let StreamStep::Bytes { bytes, .. } = step {
                            if buf.len().saturating_add(bytes.len()) > limit {
                                refused = Some(
                                    "answer stream exceeds the request's maxResponseBytes".into(),
                                );
                                break;
                            }
                            buf.extend_from_slice(bytes);
                            cursor += 1;
                        } else {
                            break;
                        }
                    }
                    entry.next_step = cursor;
                    if let Some(message) = refused {
                        self.clock.stream_fixtures.remove(idx);
                        Some((
                            ticket,
                            Outcome::Failed {
                                kind: FailureKind::Refused,
                                message,
                            },
                        ))
                    } else {
                        entry.buffered_body = Some(buf);
                        entry.last_heard_ms = self.now_ms;
                        if !entry.has_more_work() {
                            self.clock.stream_fixtures.remove(idx);
                        }
                        None
                    }
                }
                StreamStep::Event { .. } | StreamStep::Bytes { .. } => {
                    let ticket = entry.ticket;
                    let limit = entry.max_response_bytes;
                    let mut batch = Vec::new();
                    let mut refused = None;
                    let mut cursor = entry.next_step;
                    while let Some(step) = steps.get(cursor) {
                        if step.after_ms() != due_after_ms {
                            break;
                        }
                        match step_message(step, limit) {
                            Ok(Some(msg)) => {
                                batch.push(msg);
                                cursor += 1;
                            }
                            Ok(None) => break,
                            Err(msg) => {
                                refused = Some(msg);
                                break;
                            }
                        }
                    }
                    entry.next_step = cursor;
                    if let Some(message) = refused {
                        self.clock.stream_fixtures.remove(idx);
                        Some((
                            ticket,
                            Outcome::Failed {
                                kind: FailureKind::Refused,
                                message,
                            },
                        ))
                    } else {
                        entry.last_heard_ms = self.now_ms;
                        if !entry.has_more_work() {
                            self.clock.stream_fixtures.remove(idx);
                        }
                        let outcome = if batch.len() == 1 {
                            Outcome::Message(batch.pop().unwrap())
                        } else {
                            Outcome::Messages(batch)
                        };
                        Some((ticket, outcome))
                    }
                }
            }
        };

        if let Some((ticket, outcome)) = delivered {
            if let Some(receipt) = self.fulfill_measured(ticket, outcome, None)? {
                receipts.push(Timed {
                    at_ms: self.now_ms,
                    receipt,
                });
            }
        }
        Ok(true)
    }

    pub(in crate::runner) fn stream_fault_dispatch(&mut self, r: &RequestOut) -> Option<Dispatch> {
        let request = &r.request;
        let is_http = request.url.split_once("://").is_some_and(|(scheme, _)| {
            scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https")
        });
        if !is_http
            || request.long_poll.is_some()
            || request.upload_progress
            || request.listen.is_some()
        {
            return None;
        }
        // A fixture must not turn an ungranted stream request into a successful one.
        let answers = self.links.grants.is_some_and(|link| {
            (link.admitting)(&self.grants(), request.grants.as_deref(), &request.url).is_some()
        });
        if !answers {
            return None;
        }
        let (prefix, method, fixture) = self
            .faults
            .table()
            .iter()
            .filter(|f| f.matches_stream(&request.url, &request.method))
            .max_by_key(|f| f.priority())
            .and_then(|f| Some((f.prefix.clone(), f.method.clone(), f.stream.clone()?)))?;

        if let Some(why) = request
            .body_from
            .is_some()
            .then_some("answer stream does not support exactBodyFrom")
            .or_else(|| {
                request
                    .body_to
                    .is_some()
                    .then_some("answer stream does not support exactBodyTo")
            })
            .or_else(|| request.ceiling_refusal())
            .or_else(|| request.timeout_refusal())
            .or_else(|| request.stream_open_refusal())
        {
            let message = why.to_string();
            return Some(Dispatch::Run(Work::Now(Box::new(move || {
                Outcome::Failed {
                    kind: FailureKind::Refused,
                    message,
                }
            }))));
        }

        let max_response_bytes = match request.http {
            crate::HttpScheduling::Independent { max_response_bytes } => {
                max_response_bytes as usize
            }
            _ => 64 * 1024 * 1024,
        };

        // Buffered replies (4xx/5xx, 204, 205, 304) and HEAD against a wildcard schedule
        // deliver a terminal Response without exactStreamOpen.
        let buffered = match fixture.as_ref() {
            StreamFixture::Buffered { response, after_ms } => Some((response.clone(), *after_ms)),
            StreamFixture::Schedule {
                status, headers, ..
            } if request.method.eq_ignore_ascii_case("HEAD") => Some((
                Response {
                    status: *status,
                    headers: headers.clone(),
                    body: Vec::new(),
                },
                0,
            )),
            StreamFixture::Schedule { .. } => None,
        };

        if let Some((mut response, after_ms)) = buffered {
            if request.method.eq_ignore_ascii_case("HEAD") {
                response.body.clear();
            }
            if response.body.len() > max_response_bytes {
                return Some(Dispatch::Run(Work::Now(Box::new(|| Outcome::Failed {
                    kind: FailureKind::Refused,
                    message: "answer stream exceeds the request's maxResponseBytes".into(),
                }))));
            }
            let faults = self.faults.clone();
            let url = request.url.clone();
            if after_ms > 0 {
                // Defer hit/failure accounting until the delayed reply actually lands on the clock.
                self.delay_stream_fixture_reply(
                    r.ticket,
                    self.now_ms,
                    after_ms,
                    prefix,
                    method,
                    url,
                    response.status >= 400,
                    false,
                );
                return Some(Dispatch::Run(Work::HttpResponse(Arc::new(move || {
                    Outcome::Response(response.clone())
                }))));
            }
            return Some(Dispatch::Run(Work::HttpResponse(Arc::new(move || {
                faults.count_stream_hit(&prefix, method.as_deref(), response.status >= 400);
                faults.push_log(format!("stream answered (driver fixture): {url}"));
                Outcome::Response(response.clone())
            }))));
        }

        let StreamFixture::Schedule {
            status,
            headers,
            mode,
            steps,
        } = fixture.as_ref()
        else {
            unreachable!("buffered handled above");
        };

        let buffered_mode = *mode == StreamMode::Bytes && !request.bytes;
        let mut leading: Vec<Message> = Vec::new();
        if request.stream_open && !buffered_mode {
            leading.push(Message {
                event: "open".into(),
                head: Some(Box::new(StreamHead {
                    status: *status,
                    headers: headers.clone(),
                })),
                ..Default::default()
            });
        }

        let mut buffered_body = buffered_mode.then(Vec::new);
        let mut next_step = 0usize;
        let mut terminal: Option<Outcome> = None;
        let mut leading_failed = false;
        while let Some(step) = steps.get(next_step) {
            if step.after_ms() != 0 {
                break;
            }
            match step {
                StreamStep::End { .. } => {
                    next_step += 1;
                    terminal = Some(if let Some(body) = buffered_body.take() {
                        Outcome::Response(Response {
                            status: *status,
                            headers: headers.clone(),
                            body,
                        })
                    } else {
                        Outcome::Failed {
                            kind: FailureKind::Network,
                            message: mode.eof_message().into(),
                        }
                    });
                    break;
                }
                StreamStep::Fail { message, .. } => {
                    next_step += 1;
                    leading_failed = true;
                    terminal = Some(Outcome::Failed {
                        kind: FailureKind::Network,
                        message: message.clone(),
                    });
                    break;
                }
                StreamStep::Bytes { bytes, .. } if buffered_body.is_some() => {
                    let buf = buffered_body.as_mut().unwrap();
                    if buf.len().saturating_add(bytes.len()) > max_response_bytes {
                        terminal = Some(Outcome::Failed {
                            kind: FailureKind::Refused,
                            message: "answer stream exceeds the request's maxResponseBytes".into(),
                        });
                        break;
                    }
                    buf.extend_from_slice(bytes);
                    next_step += 1;
                }
                StreamStep::Event { .. } | StreamStep::Bytes { .. } => {
                    match step_message(step, max_response_bytes) {
                        Ok(Some(msg)) => {
                            leading.push(msg);
                            next_step += 1;
                        }
                        Ok(None) => break,
                        Err(message) => {
                            terminal = Some(Outcome::Failed {
                                kind: FailureKind::Refused,
                                message,
                            });
                            break;
                        }
                    }
                }
            }
        }

        if let Some(pending) = self.pending.iter_mut().find(|p| p.ticket == r.ticket) {
            if let Some(m) = pending.stream_meta.as_deref_mut() {
                m.fixture = true;
            }
        }
        // Defer hit accounting, failure accounting, and parked-work activation until the
        // executor admits the request and runs `Work::Later` (never count refused requests).
        let admitted = Arc::new(AtomicBool::new(false));
        if terminal.is_none() {
            self.install_stream_fixture(ActiveStreamFixture {
                ticket: r.ticket,
                prefix: prefix.clone(),
                method: method.clone(),
                url: request.url.clone(),
                fixture,
                opened_at: self.now_ms,
                next_step,
                max_response_bytes,
                idle_timeout_ms: request.idle_timeout_ms,
                last_heard_ms: self.now_ms,
                admitted: admitted.clone(),
                buffered_body,
                wasm_handoff: false,
            });
        }

        let faults = self.faults.clone();
        let url = request.url.clone();
        Some(Dispatch::Run(Work::Later(Box::new(move |mut reply| {
            admitted.store(true, Ordering::Release);
            faults.count_stream_hit(&prefix, method.as_deref(), leading_failed);
            faults.push_log(format!("stream answered (driver fixture): {url}"));
            if let Some(end_outcome) = terminal {
                for msg in leading {
                    reply.message(msg);
                }
                reply.send(end_outcome);
                return;
            }
            for msg in leading {
                reply.message(msg);
            }
            // Empty batch marks `fixture_opened = true` in `Runner::fulfill` without closing the stream
            // (including buffered byte schedules, so `Reply::drop` never sends `Aborted`).
            reply.send(Outcome::Messages(Vec::new()));
        }))))
    }
}
