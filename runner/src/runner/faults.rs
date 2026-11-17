//! Driver faults (LLP 1103): a fetch whose URL starts with an armed prefix
//! fails as a refused connection does, without going out. The table is the
//! session's: one per runner (a native reload's replacement takes over its
//! predecessor's), shared by every source, placement and lane, and consulted
//! where the host would hand a plain request to its transport
//! (`Runner::fault_dispatch`) and before each long-poll attempt
//! (`Runner::fetch_fault`), so each request is counted once. It is the driver's alone: armed by
//! `EXACT_AGENT_FAIL_FETCH` at launch under the agent, or by the agent's
//! `faults` operation.

use super::Runner;
use crate::{DataSource, Dispatch, FailureKind, Outcome, RequestOut, Response, Work};
use std::sync::{Arc, Mutex};

/// A long poll's check before each HTTP attempt: a matching fault's network error.
pub type FetchFault = Arc<dyn Fn(&str) -> Option<String> + Send + Sync>;

/// One armed prefix (D3): how many matching fetches it fails yet (`None`:
/// every one until `pass`), whether it still matches, and how many it has
/// failed since it was armed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Fault {
    /// Matches a URL that starts with it.
    pub prefix: String,
    /// The count given at arming, if any.
    pub times: Option<u32>,
    /// What is left of it.
    pub left: Option<u32>,
    /// Fetches it has failed or answered since it was armed; `pass` keeps it.
    pub hits: u32,
    /// Injected failures across replacements, independent of this entry’s response hits.
    pub failure_hits: u32,
    /// `pass` clears it.
    pub armed: bool,
    /// A final HTTP response instead of a connection failure.
    pub response: Option<Arc<Response>>,
}

fn response_json(response: &Response) -> serde_json::Value {
    serde_json::json!({"status": response.status,
        "headers": response.headers.iter().cloned().collect::<std::collections::BTreeMap<_, _>>(),
        "body": String::from_utf8_lossy(&response.body)})
}

fn parse_response(value: &serde_json::Value) -> Result<Response, String> {
    let bad = || {
        "answer fetch needs a status (200–599, excluding redirects), string headers and a text body of at most 64 KiB".to_string()
    };
    let status = value["status"]
        .as_u64()
        .filter(|n| (200..=599).contains(n) && ![301, 302, 303, 307, 308].contains(n))
        .ok_or_else(bad)? as u16;
    let body = value["body"].as_str().ok_or_else(bad)?;
    if body.len() > 64 * 1024 || ([204, 205, 304].contains(&status) && !body.is_empty()) {
        return Err(bad());
    }
    let headers = value["headers"].as_object().ok_or_else(bad)?;
    if headers.len() > 1024 {
        return Err("answer fetch: at most 1024 response headers".into());
    }
    let headers = headers
        .iter()
        .map(|(name, value)| {
            let value = value.as_str().ok_or_else(bad)?;
            if name.eq_ignore_ascii_case("set-cookie")
                || name.eq_ignore_ascii_case("set-cookie2")
                || name.is_empty()
                || !name
                    .bytes()
                    .all(|c| c.is_ascii_alphanumeric() || b"!#$%&'*+-.^_`|~".contains(&c))
                || value.chars().any(|c| {
                    c != '\t' && !((' '..='~').contains(&c) || ('\u{80}'..='\u{ff}').contains(&c))
                })
            {
                return Err(bad());
            }
            Ok((
                name.to_ascii_lowercase(),
                value.trim_matches([' ', '\t']).to_string(),
            ))
        })
        .collect::<Result<Vec<_>, String>>()?;
    Ok(Response {
        status,
        headers,
        body: body.as_bytes().to_vec(),
    })
}

fn table_spec(entries: &[Fault]) -> String {
    let n = |c: Option<u32>| c.map_or("-".to_string(), |n| n.to_string());
    entries
        .iter()
        .map(|f| {
            format!(
                "{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
                f.prefix,
                n(f.times),
                n(f.left),
                f.hits,
                u8::from(f.armed),
                f.response
                    .as_ref()
                    .map(|r| response_json(r).to_string())
                    .unwrap_or_default(),
                f.failure_hits
            )
        })
        .collect()
}

fn table_fits(entries: &[Fault]) -> Result<(), String> {
    // Both cumulative counts can grow while the table must remain reloadable.
    let growth: usize = entries
        .iter()
        .map(|f| 20 - f.hits.to_string().len() - f.failure_hits.to_string().len())
        .sum();
    if entries.iter().any(|f| f.response.is_some()) && table_spec(entries).len() + growth > 65536 {
        return Err("answer fetch: launch fixtures exceed 64 KiB".into());
    }
    Ok(())
}

impl Fault {
    fn live(&self) -> bool {
        self.armed && self.left != Some(0)
    }
}

/// The session's fault table.
#[derive(Debug, Default, Clone)]
pub struct Faults {
    entries: Arc<Mutex<Vec<Fault>>>,
    lines: Arc<Mutex<std::collections::VecDeque<String>>>,
}

impl Faults {
    /// The table a launch names: `EXACT_AGENT_FAIL_FETCH`, in
    /// [`Faults::parse`]'s lines, read only under the agent
    /// (`EXACT_AGENT=1`), as its other launch facts are.
    pub fn from_env() -> Result<Faults, String> {
        let agent = std::env::var("EXACT_AGENT").is_ok_and(|v| v == "1");
        let spec = std::env::var("EXACT_AGENT_FAIL_FETCH").ok();
        match spec.filter(|_| agent) {
            Some(spec) => Faults::parse(&spec),
            None => Ok(Faults::default()),
        }
    }

    /// A table from its launch lines, one prefix each: `<prefix>` fails
    /// every fetch, `<prefix>\t<times>` that many; a reload carries the rest
    /// as `<prefix>\t<times>\t<left>\t<hits>\t<armed>\t<response>\t<failureHits>`
    /// (`-` for no count, `armed` 1 or 0, empty response for a failure),
    /// which [`Faults::spec`] writes.
    pub fn parse(spec: &str) -> Result<Faults, String> {
        let count = |field: Option<&str>| -> Result<Option<u32>, String> {
            match field.unwrap_or("-") {
                "-" => Ok(None),
                n => n
                    .parse::<u32>()
                    .map(Some)
                    .map_err(|_| format!("fail fetch: `{n}` is not a count")),
            }
        };
        let mut faults: Vec<Fault> = Vec::new();
        for line in spec.lines().filter(|l| !l.trim().is_empty()) {
            let mut fields = line.split('\t');
            let prefix = fields.next().unwrap_or_default();
            if prefix.is_empty() {
                return Err("fail fetch: each line names a non-empty prefix".into());
            }
            let times = count(fields.next())?;
            if times == Some(0) {
                return Err("fail fetch: `times` is a positive integer".into());
            }
            let left = match fields.next() {
                Some(field) => count(Some(field))?,
                None => times,
            };
            let hits = count(fields.next())?.unwrap_or(0);
            let armed = fields.next() != Some("0");
            let response = fields
                .next()
                .filter(|s| !s.is_empty())
                .map(|s| {
                    serde_json::from_str(s)
                        .map_err(|e| e.to_string())
                        .and_then(|v| parse_response(&v))
                })
                .transpose()?;
            let failure_hits =
                count(fields.next())?.unwrap_or(if response.is_none() { hits } else { 0 });
            faults.retain(|f| f.prefix != prefix);
            faults.push(Fault {
                prefix: prefix.to_string(),
                times,
                left,
                hits,
                failure_hits,
                armed,
                response: response.map(Arc::new),
            });
        }
        table_fits(&faults)?;
        Ok(Faults {
            entries: Arc::new(Mutex::new(faults)),
            lines: Default::default(),
        })
    }

    /// The table as launch lines, for a reload to carry as it is.
    pub fn spec(&self) -> String {
        table_spec(&self.table())
    }

    fn table(&self) -> std::sync::MutexGuard<'_, Vec<Fault>> {
        self.entries.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Arms `prefix` for `times` fetches, or every one; arming it again
    /// replaces its entry.
    pub fn arm(&self, prefix: &str, times: Option<u32>) -> Result<(), String> {
        if prefix.is_empty() {
            return Err("fail fetch needs a URL prefix".into());
        }
        if times == Some(0) {
            return Err("fail fetch: `times` is a positive integer".into());
        }
        let mut table = self.table();
        let mut next = table.clone();
        next.retain(|f| f.prefix != prefix);
        next.push(Fault {
            prefix: prefix.to_string(),
            times,
            left: times,
            hits: 0,
            failure_hits: table
                .iter()
                .find(|f| f.prefix == prefix)
                .map_or(0, |f| f.failure_hits),
            armed: true,
            response: None,
        });
        table_fits(&next)?;
        *table = next;
        Ok(())
    }

    /// Replace a prefix with an immediate response. Validation precedes mutation.
    pub fn answer(&self, prefix: &str, value: &serde_json::Value) -> Result<(), String> {
        let response = parse_response(value)?;
        if prefix.is_empty() || prefix.contains(['\t', '\r', '\n']) {
            return Err("answer fetch needs a URL prefix without tabs or newlines".into());
        }
        let mut table = self.table();
        let mut next = table.clone();
        next.retain(|f| f.prefix != prefix);
        next.push(Fault {
            prefix: prefix.into(),
            times: None,
            left: None,
            hits: 0,
            failure_hits: table
                .iter()
                .find(|f| f.prefix == prefix)
                .map_or(0, |f| f.failure_hits),
            armed: true,
            response: Some(Arc::new(response)),
        });
        table_fits(&next)?;
        *table = next;
        Ok(())
    }

    /// Stops `prefix` failing; its hits stay for the test's check. Whether
    /// it was in the table.
    pub fn pass(&self, prefix: &str) -> bool {
        let mut table = self.table();
        let entry = table.iter_mut().find(|f| f.prefix == prefix);
        entry.map(|f| f.armed = false).is_some()
    }

    /// Whether a fetch of `url` fails, counting it: the longest live prefix
    /// it starts with decides.
    pub fn take(&self, url: &str) -> bool {
        self.take_response(url, false).is_some()
    }

    fn take_response(&self, url: &str, answers: bool) -> Option<Option<Response>> {
        let mut table = self.table();
        let hit = table
            .iter_mut()
            .filter(|f| f.live() && (answers || f.response.is_none()) && url.starts_with(&f.prefix))
            .max_by_key(|f| f.prefix.len());
        match hit {
            Some(f) => {
                f.hits += 1;
                if f.response.is_none() {
                    f.failure_hits += 1;
                }
                f.left = f.left.map(|n| n - 1);
                Some(f.response.as_deref().cloned())
            }
            _ => None,
        }
    }

    /// Whether a live fault is armed for exactly `point`, uncounted: it holds
    /// a presence move there until it is passed (LLP 1133.010 §9, drive 4).
    pub fn holds(&self, point: &str) -> bool {
        self.table()
            .iter()
            .any(|f| f.live() && f.response.is_none() && f.prefix == point)
    }

    /// Whether the table holds nothing.
    pub fn is_empty(&self) -> bool {
        self.table().is_empty()
    }

    /// The table as `state.faults` shows it: `prefix`, `times`, `left`
    /// (`null` without a count), `hits`, cumulative `failureHits`, `armed`
    /// and, for an answer, `response`.
    pub fn json(&self) -> String {
        let n = |c: Option<u32>| c.map_or("null".to_string(), |n| n.to_string());
        let mut out = String::from("[");
        for (i, f) in self.table().iter().enumerate() {
            if i > 0 {
                out.push(',');
            }
            out.push_str("{\"prefix\":");
            crate::agent::quote(&f.prefix, &mut out);
            out.push_str(&format!(
                ",\"times\":{},\"left\":{},\"hits\":{},\"armed\":{}",
                n(f.times),
                n(f.left),
                f.hits,
                f.armed
            ));
            out.push_str(&format!(",\"failureHits\":{}", f.failure_hits));
            if let Some(response) = &f.response {
                out.push_str(&format!(",\"response\":{}", response_json(response)));
            }
            out.push('}');
        }
        out.push(']');
        out
    }
}

impl<D: DataSource> Runner<D> {
    /// The session's driver faults (LLP 1103).
    pub fn faults(&self) -> &Faults {
        &self.faults
    }

    /// Share the live fault table with the executor's long-poll attempts.
    /// Their journal lines join the runner's at its next pump.
    pub fn fetch_fault(&self) -> FetchFault {
        let faults = self.faults.clone();
        Arc::new(move |url| {
            if !faults.take(url) {
                return None;
            }
            let message = format!("fetch failed (driver fault): {url}");
            let mut lines = faults.lines.lock().unwrap_or_else(|e| e.into_inner());
            lines.push_back(message.clone());
            if lines.len() > super::JOURNAL_RING {
                lines.pop_front();
            }
            Some(message)
        })
    }

    /// Take over the live table of the session this runner replaces in the
    /// same process. The old executor's long polls take from that table until
    /// they are let go, so a fault one takes after the reload carried the
    /// table stays spent here.
    pub fn share_faults(&mut self, live: &Faults) {
        self.faults = live.clone();
    }

    /// Drain executor fault messages before a pump or an agent inspection.
    pub fn take_fault_logs(&mut self) {
        let lines =
            std::mem::take(&mut *self.faults.lines.lock().unwrap_or_else(|e| e.into_inner()));
        for line in lines {
            self.log(line);
        }
    }

    /// What a host runs for a plain request instead of its transport, when
    /// a driver fault matches its URL (LLP 1103 D1, D2): the failure a
    /// refused connection is, `Failed { kind: Network }`, which a
    /// TypeScript source's `fetch` rejects with as `FetchError('Network')`.
    /// Long polls check each attempt through [`Runner::fetch_fault`]; other
    /// streams are not matched (D5). The journal says so.
    pub fn fault_dispatch(&mut self, r: &RequestOut) -> Option<Dispatch> {
        let request = &r.request;
        if request.stream || request.is_native() || request.is_auth() || request.surface.is_some() {
            return None;
        }
        if self.faults.is_empty() {
            return None;
        }
        // A fixture must not turn an ungranted request into a successful one.
        let answers =
            crate::grants::admitting(&self.grants(), request.grants.as_deref(), &request.url)
                .is_some();
        let (prefix, response) = self
            .faults
            .table()
            .iter()
            .filter(|f| {
                f.live() && (answers || f.response.is_none()) && request.url.starts_with(&f.prefix)
            })
            .max_by_key(|f| f.prefix.len())
            .map(|f| (f.prefix.clone(), f.response.clone()))?;
        if let Some(fixture) = response {
            let (faults, request) = (self.faults.clone(), request.clone());
            return Some(Dispatch::Run(Work::HttpResponse(Arc::new(move || {
                let mut response = (*fixture).clone();
                if request.method.eq_ignore_ascii_case("HEAD") {
                    response.body.clear();
                }
                let refusal = if request.body_to.is_some() {
                    Some("answer fetch does not support exactBodyTo")
                } else if response.body.len()
                    > match request.http {
                        crate::HttpScheduling::Independent { max_response_bytes } => {
                            max_response_bytes as usize
                        }
                        _ => 64 * 1024 * 1024,
                    }
                {
                    Some("answer fetch exceeds the request's maxResponseBytes")
                } else {
                    None
                };
                if let Some(message) = refusal {
                    return Outcome::Failed {
                        kind: FailureKind::Refused,
                        message: message.into(),
                    };
                }
                // A replacement entry owns fresh counters; an older in-flight answer cannot increment them.
                if let Some(entry) = faults.table().iter_mut().find(|f| {
                    f.prefix == prefix
                        && f.response
                            .as_ref()
                            .is_some_and(|r| Arc::ptr_eq(r, &fixture))
                }) {
                    entry.hits += 1;
                }
                let mut lines = faults.lines.lock().unwrap_or_else(|e| e.into_inner());
                lines.push_back(format!("fetch answered (driver fixture): {}", request.url));
                if lines.len() > super::JOURNAL_RING {
                    lines.pop_front();
                }
                Outcome::Response(response)
            }))));
        }
        self.faults.take_response(&request.url, answers)?;
        let message = format!("fetch failed (driver fault): {}", request.url);
        self.log(message.clone());
        Some(Dispatch::Run(Work::Now(Box::new(move || {
            Outcome::Failed {
                kind: FailureKind::Network,
                message,
            }
        }))))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_counts_survive_replacement_and_reload() {
        let f = Faults::default();
        let prefix = "https://api.test/";
        f.arm(prefix, None).unwrap();
        assert!(f.take(prefix));
        f.answer(
            prefix,
            &serde_json::json!({"status":200,"headers":{},"body":"ok"}),
        )
        .unwrap();
        assert!(f.take_response(prefix, true).is_some());
        let f = Faults::parse(&f.spec()).unwrap();
        {
            let table = f.table();
            assert_eq!((table[0].hits, table[0].failure_hits), (1, 1));
        }
        f.arm(prefix, None).unwrap();
        assert!(f.take(prefix));
        let f = Faults::parse(&f.spec()).unwrap();
        {
            let table = f.table();
            assert_eq!((table[0].hits, table[0].failure_hits), (1, 2));
        }
    }

    #[test]
    fn unreadable_launch_fixture_refuses_boot() {
        const CHILD: &str = "EXACT_FAULT_PARSE_CHILD";
        if std::env::var_os(CHILD).is_some() {
            let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
            builder.node(
                exact_kernel::NodeType::View as u8,
                None,
                None,
                0,
                &[],
                &[],
                None,
            );
            let result = Runner::boot(
                builder.finish().unwrap(),
                (),
                exact_kernel::Kernel::with_monospace(),
                Default::default(),
                "/",
            );
            assert!(result.is_err(), "invalid launch fixtures must refuse boot");
            return;
        }
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "runner::faults::tests::unreadable_launch_fixture_refuses_boot",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("EXACT_AGENT", "1")
            .env("EXACT_AGENT_FAIL_FETCH", "https://api.test/\t-\t-\t0\t1\t{")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stdout)
        );
    }

    #[test]
    fn unicode_launch_fixture_has_the_same_byte_budget_as_javascript() {
        let head = "https://api.test/\t-\t-\t0\t1\t";
        let mut body = "\u{2028}".to_string();
        let json = |body: &str| format!("{{\"status\":200,\"headers\":{{}},\"body\":\"{body}\"}}");
        body.push_str(&"a".repeat(65514 - head.len() - json(&body).len()));
        let spec = format!("{head}{}", json(&body));
        assert_eq!(spec.len(), 65514);
        let faults = Faults::parse(&spec).unwrap();
        assert_eq!(faults.spec().len(), 65517);
        assert!(Faults::parse(&faults.spec()).is_ok());
        body.push_str("aa");
        assert!(Faults::parse(&format!("{head}{}", json(&body))).is_err());
    }

    #[test]
    fn response_fixtures_refuse_an_unreloadable_table_without_replacing_it() {
        let faults = Faults::default();
        let response = serde_json::json!({"status":200,"headers":{},"body":"a".repeat(40000)});
        faults.answer("https://api.test/a", &response).unwrap();
        let before = faults.json();
        assert!(faults.answer("https://api.test/b", &response).is_err());
        assert_eq!(faults.json(), before);
        let escaped = serde_json::json!({"status":200,"headers":{},"body":"\n".repeat(40000)});
        assert!(faults.answer("https://api.test/a", &escaped).is_err());
        assert_eq!(faults.json(), before);
        assert!(Faults::parse(&faults.spec()).is_ok());
    }

    #[test]
    fn response_header_limit_is_checked_before_arming() {
        let faults = Faults::default();
        let prefix = "https://api.test/";
        let headers = (0..1024)
            .map(|i| (format!("x-{i}"), serde_json::json!("v")))
            .collect::<serde_json::Map<_, _>>();
        let mut response = serde_json::json!({"status":200,"headers":headers,"body":"ok"});
        faults.answer(prefix, &response).unwrap();
        let before = faults.json();
        response["headers"]["x-excess"] = serde_json::json!("v");
        assert!(faults.answer(prefix, &response).is_err());
        assert_eq!(faults.json(), before);
        assert!(Faults::parse(&format!("{prefix}\t-\t-\t0\t1\t{response}")).is_err());
        assert!(faults.lines.lock().unwrap().is_empty());
    }

    #[test]
    fn response_fixtures_refuse_oversized_bodies_and_cookie_headers() {
        let faults = Faults::default();
        for bad in [
            serde_json::json!({"status":200,"headers":{},"body":"a".repeat(70 * 1024)}),
            serde_json::json!({"status":200,"headers":{"Set-Cookie":"sid=1"},"body":""}),
            serde_json::json!({"status":200,"headers":{"SET-cookie2":"sid=1"},"body":""}),
        ] {
            assert!(
                faults.answer("https://api.test/", &bad).is_err(),
                "accepted invalid fixture"
            );
            assert!(Faults::parse(&format!("https://api.test/\t-\t-\t0\t1\t{bad}")).is_err());
            assert!(faults.is_empty());
        }
    }

    #[test]
    fn response_fixtures_validate_before_replacing_and_round_trip_at_launch() {
        let faults = Faults::default();
        let response = serde_json::json!({"status": 200, "headers": {"Content-Type": "application/json"}, "body": "{\"text\":\"Hello 🌍\\n\"}"});
        faults.answer("https://api.test/", &response).unwrap();
        assert_eq!(Faults::parse(&faults.spec()).unwrap().json(), faults.json());
        assert!(
            !faults.take("https://api.test/"),
            "long polls leave answers unused"
        );
        for bad in [
            serde_json::json!({"status": 302, "headers": {}, "body": ""}),
            serde_json::json!({"status": 204, "headers": {}, "body": "not empty"}),
            serde_json::json!({"status": 200, "headers": {"x": "one\r\ntwo"}, "body": ""}),
            serde_json::json!({"status": 200, "headers": {}, "body": "x".repeat(1024 * 1024 + 1)}),
        ] {
            assert!(faults.answer("https://api.test/", &bad).is_err());
        }
        let answer = faults
            .take_response("https://api.test/greeting", true)
            .unwrap()
            .unwrap();
        assert_eq!(answer.status, 200);
        assert_eq!(answer.headers[0].0, "content-type");
        faults.arm("https://api.test/", None).unwrap();
        assert!(faults
            .take_response("https://api.test/greeting", true)
            .unwrap()
            .is_none());
    }

    #[test]
    fn poll_attempts_share_live_faults_but_other_streams_do_not_take_them() {
        let mut builder = exact_plan::builder::PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        builder.node(
            exact_kernel::NodeType::View as u8,
            None,
            None,
            0,
            &[],
            &[],
            None,
        );
        let mut runner = Runner::boot(
            builder.finish().unwrap(),
            (),
            exact_kernel::Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let fault = runner.fetch_fault();
        let url = "https://api.test/changes?since=0";
        runner.faults().arm("https://api.test/", None).unwrap();
        runner.faults().arm(url, Some(1)).unwrap();
        for kind in 0..5 {
            let mut request = crate::Request::get(url);
            request.stream = true;
            match kind {
                1 => request.bytes = true,
                2 => request.socket = true,
                3 => request.upload_progress = true,
                4 => request = request.long_poll("since", "seq"),
                _ => {}
            }
            let out = RequestOut {
                completion: None,
                ticket: 1,
                target: "live".into(),
                request,
                class: Default::default(),
                forced: false,
            };
            assert!(runner.fault_dispatch(&out).is_none());
        }
        assert!(runner.faults().json().contains("\"left\":1,\"hits\":0"));
        assert!(fault(url).unwrap().contains(url));
        assert!(
            fault(url).is_some(),
            "the spent prefix lets the shorter one decide"
        );
        runner.faults().pass("https://api.test/");
        assert!(fault(url).is_none());
        assert!(fault("https://other.test/").is_none());
        runner.take_requests();
        assert_eq!(
            runner
                .journal()
                .filter(|l| l.contains("fetch failed (driver fault)"))
                .count(),
            2
        );
    }

    #[test]
    fn answers_do_not_hide_shorter_failures_from_polls_or_presence() {
        let faults = Faults::default();
        faults.arm("https://api.test/", Some(2)).unwrap();
        faults
            .answer(
                "https://api.test/items",
                &serde_json::json!({"status": 200, "headers": {}, "body": "fixture"}),
            )
            .unwrap();
        assert!(!faults.holds("https://api.test/items"));
        assert!(faults.take("https://api.test/items/stream"));
        assert!(faults.take("https://api.test/items/stream"));
        assert!(!faults.take("https://api.test/items/stream"));
        let table = faults.table();
        assert_eq!(table[0].hits, 2);
        assert_eq!(table[1].hits, 0);
    }

    #[test]
    fn the_longest_prefix_decides_counts_run_out_and_pass_keeps_hits() {
        let f = Faults::default();
        f.arm("https://api.test/", None).unwrap();
        f.arm("https://api.test/recipes", Some(1)).unwrap();
        assert!(f.take("https://api.test/recipes/3"));
        let json = f.json();
        assert!(
            json.contains("\"prefix\":\"https://api.test/recipes\""),
            "{json}"
        );
        assert!(json.contains("\"left\":0"), "{json}");
        // Its count spent, the shorter prefix decides.
        assert!(f.take("https://api.test/recipes/3"));
        assert!(!f.take("https://other.test/"));
        assert!(f.pass("https://api.test/"));
        assert!(!f.take("https://api.test/x"));
        assert!(
            f.json()
                .contains("\"left\":null,\"hits\":1,\"armed\":false"),
            "{}",
            f.json()
        );
        // Arming again replaces the entry.
        f.arm("https://api.test/", Some(2)).unwrap();
        assert!(f.json().contains("\"left\":2,\"hits\":0"), "{}", f.json());
        assert!(f.arm("", None).is_err());
        assert!(f.arm("x", Some(0)).is_err());
    }

    #[test]
    fn a_reload_carries_the_table_as_it_is() {
        let f = Faults::default();
        f.arm("https://a.test/", Some(1)).unwrap();
        assert!(f.take("https://a.test/1"));
        f.arm("https://b.test/", None).unwrap();
        f.pass("https://b.test/");
        let carried = Faults::parse(&f.spec()).unwrap();
        assert_eq!(carried.json(), f.json());
        assert!(
            !carried.take("https://a.test/2"),
            "a spent count stays spent"
        );
        assert!(
            !carried.take("https://b.test/"),
            "a passed prefix stays passed"
        );
        assert!(Faults::parse("\t2").is_err());
        assert!(Faults::parse("x\t0").is_err());
        assert!(Faults::parse("x\tmany").is_err());
        let launch = Faults::parse("https://c.test/\nhttps://d.test/\t2\n").unwrap();
        assert!(launch
            .json()
            .contains("\"prefix\":\"https://d.test/\",\"times\":2,\"left\":2"));
    }
}
