//! The render server's shared HTTP cache (LLP 1153.001 D7; RFC 9111).
use exact_runner::CacheMode;
use ibex2::boundary::HostError;
use ibex2::host::Fetch;
use ibex2::stdlib::abort::{AbortController, AbortSignal};
use ibex2::stdlib::fetch::{Body, BodySource, Headers, Request, Response, StreamingResponse};
use std::collections::{BTreeMap, VecDeque};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

type Directives = BTreeMap<String, String>;
fn directives(headers: &Headers) -> Directives {
    headers
        .get("cache-control")
        .unwrap_or("")
        .split(',')
        .filter_map(|part| {
            let (name, value) = part.trim().split_once('=').unwrap_or((part.trim(), ""));
            (!name.is_empty()).then(|| {
                (
                    name.trim().to_ascii_lowercase(),
                    value.trim().trim_matches('"').to_ascii_lowercase(),
                )
            })
        })
        .collect()
}
fn seconds(value: Option<&String>) -> Option<f64> {
    value
        .filter(|v| !v.is_empty())?
        .parse::<f64>()
        .ok()
        .filter(|n| n.is_finite() && *n >= 0.0)
}
fn date(headers: &Headers, name: &str) -> Option<f64> {
    super::cache::http_date(headers.get(name)?).map(|s| s as f64)
}

/// A server-owned shared cache, separate from each render's runner and module.
/// Clones share at most 256 answers, 64 MiB of body storage, and 8 MiB per answer.
/// Dropping the last owner aborts its background revalidations.
#[derive(Clone)]
pub struct SharedHttpCache {
    owner: Arc<Owner>,
}
struct Owner {
    inner: Arc<Inner>,
}
struct Inner {
    state: Mutex<State>,
    bytes: Arc<AtomicUsize>,
    storing: AtomicUsize,
    now: Arc<dyn Fn() -> f64 + Send + Sync>,
    limits: Limits,
    #[cfg(test)]
    ready: std::sync::Condvar,
}
#[derive(Clone, Copy)]
struct Limits {
    entries: usize,
    bytes: usize,
    entry: usize,
}
#[derive(Default)]
struct State {
    entries: VecDeque<(String, Entry)>,
    refreshing: BTreeMap<String, AbortController>,
}
#[derive(Clone)]
struct Entry {
    head: Candidate,
    body: Arc<Payload>,
}
#[derive(Clone)]
struct Candidate {
    status: u16,
    status_text: String,
    headers: Headers,
    stored: f64,
    age: f64,
    lifetime: f64,
    stale: f64,
    revalidate: bool,
    shareable: bool,
    vary: Vec<(String, Option<String>)>,
}
impl Candidate {
    fn matches(&self, request: &Request) -> bool {
        (!request.headers.has("authorization") || self.shareable)
            && self
                .vary
                .iter()
                .all(|(name, value)| request.headers.get(name) == value.as_deref())
    }
}
struct Payload {
    bytes: Vec<u8>,
    budget: Arc<AtomicUsize>,
}
impl Drop for Payload {
    fn drop(&mut self) {
        let bytes = std::mem::take(&mut self.bytes);
        let size = bytes.len();
        drop(bytes);
        self.budget.fetch_sub(size, Ordering::Relaxed);
    }
}
impl Drop for Owner {
    fn drop(&mut self) {
        // Aborting can synchronously retire a transport; do it outside the cache lock.
        let aborts = std::mem::take(&mut self.inner.state.lock().unwrap().refreshing);
        for (_, abort) in aborts {
            abort.abort();
        }
    }
}
impl Default for SharedHttpCache {
    fn default() -> Self {
        Self::new()
    }
}
impl SharedHttpCache {
    /// An empty cache whose freshness clock is wall time.
    pub fn new() -> Self {
        Self::with_clock(
            Limits {
                entries: 256,
                bytes: 64 << 20,
                entry: 8 << 20,
            },
            Arc::new(|| {
                SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs_f64()
            }),
        )
    }
    fn with_clock(limits: Limits, now: Arc<dyn Fn() -> f64 + Send + Sync>) -> Self {
        Self {
            owner: Arc::new(Owner {
                inner: Arc::new(Inner {
                    state: Mutex::default(),
                    bytes: Arc::default(),
                    storing: AtomicUsize::new(0),
                    now,
                    limits,
                    #[cfg(test)]
                    ready: std::sync::Condvar::new(),
                }),
            }),
        }
    }
    pub(super) fn stream(
        &self,
        fetch: &Fetch,
        request: Request,
        mode: CacheMode,
        signal: &AbortSignal,
    ) -> Result<StreamingResponse, HostError> {
        signal.check()?;
        let inner = &self.owner.inner;
        let asked = directives(&request.headers);
        let store = request.method.eq_ignore_ascii_case("GET")
            && request.body.is_none()
            && request.form_data.is_none()
            && mode != CacheMode::NoStore
            && !asked.contains_key("no-store");
        let past = matches!(mode, CacheMode::Reload | CacheMode::NoCache)
            || asked.contains_key("no-cache")
            || seconds(asked.get("max-age")) == Some(0.0);
        // A caller's conditional is an origin question, including on a fresh hit.
        let conditional = [
            "range",
            "if-range",
            "if-match",
            "if-none-match",
            "if-modified-since",
            "if-unmodified-since",
        ]
        .iter()
        .any(|name| request.headers.has(name));
        if store && !past && !conditional {
            let now = (inner.now)();
            // The transport sends this spelling, including dot segments.
            let url = request.url.clone();
            let hit = {
                let mut state = inner.state.lock().unwrap();
                let index = state.entries.iter().position(|(u, _)| *u == url);
                index.and_then(|index| {
                    let (_, entry) = &state.entries[index];
                    let e = &entry.head;
                    let age = e.age + (now - e.stored).max(0.0);
                    let stale = if e.revalidate {
                        0.0
                    } else {
                        e.stale
                            .max(if asked.get("max-stale").is_some_and(String::is_empty) {
                                f64::INFINITY
                            } else {
                                seconds(asked.get("max-stale")).unwrap_or(0.0)
                            })
                    };
                    if e.matches(&request)
                        && age < e.lifetime + stale
                        && age <= seconds(asked.get("max-age")).unwrap_or(f64::INFINITY)
                        && e.lifetime - age
                            >= seconds(asked.get("min-fresh")).unwrap_or(f64::NEG_INFINITY)
                    {
                        let pair = state.entries.remove(index).unwrap();
                        let hit = (pair.1.clone(), age);
                        state.entries.push_back(pair);
                        Some(hit)
                    } else {
                        None
                    }
                })
            };
            if let Some((entry, age)) = hit {
                if age >= entry.head.lifetime && mode != CacheMode::OnlyIfCached {
                    refresh(inner, fetch, request.clone(), url.clone());
                }
                signal.check()?;
                let mut headers = entry.head.headers.clone();
                headers.set("age", &format!("{:.0}", age.floor()));
                return Ok(StreamingResponse {
                    status: entry.head.status,
                    status_text: entry.head.status_text.clone(),
                    headers,
                    body: Body::new(
                        Box::new(Kept {
                            body: entry.body,
                            at: 0,
                        }),
                        request.body_limit(),
                        signal.clone(),
                    ),
                    url,
                    redirected: false,
                });
            }
        }
        if mode == CacheMode::OnlyIfCached {
            return Err(HostError::Failed(
                "only-if-cached: no reply is stored for this request".into(),
            ));
        }
        let partial = request.headers.has("range") || request.headers.has("if-range");
        network(inner, fetch, request, signal, store && !partial)
    }
}
impl super::Core {
    /// Share HTTP answers with other blocking executors belonging to this server.
    /// Install before admitting work. Grant and reach checks still precede cache hits.
    pub fn set_shared_http_cache(&self, cache: SharedHttpCache) {
        assert!(
            self.machine.is_none(),
            "shared HTTP cache requires blocking transports"
        );
        assert!(
            self.shared.http_cache.set(cache).is_ok(),
            "HTTP cache already installed"
        );
    }
}
fn candidate(
    request: &Request,
    response: &StreamingResponse,
    sent: f64,
    now: f64,
) -> Option<Candidate> {
    if response.redirected
        || !matches!(
            response.status,
            200 | 203 | 204 | 404 | 405 | 410 | 414 | 501
        )
    {
        return None;
    }
    let cc = directives(&response.headers);
    if ["no-store", "private", "no-cache"]
        .iter()
        .any(|name| cc.contains_key(*name))
    {
        return None;
    }
    let shareable = ["public", "s-maxage", "must-revalidate"]
        .iter()
        .any(|name| cc.contains_key(*name));
    if request.headers.has("authorization") && !shareable {
        return None;
    }
    let vary: Vec<_> = response
        .headers
        .get("vary")
        .unwrap_or("")
        .split(',')
        .map(|name| name.trim().to_ascii_lowercase())
        .filter(|name| !name.is_empty())
        .collect();
    if vary.iter().any(|name| name == "*") {
        return None;
    }
    let date = date(&response.headers, "date").unwrap_or(now);
    let lifetime = seconds(cc.get("s-maxage"))
        .or_else(|| seconds(cc.get("max-age")))
        .or_else(|| {
            self::date(&response.headers, "expires").map(|expires| (expires - date).max(0.0))
        })?;
    if lifetime <= 0.0 {
        return None;
    }
    let age = response
        .headers
        .get("age")
        .and_then(|v| v.parse::<f64>().ok())
        .filter(|v| v.is_finite() && *v >= 0.0)
        .unwrap_or(0.0);
    Some(Candidate {
        status: response.status,
        status_text: response.status_text.clone(),
        headers: response.headers.clone(),
        stored: now,
        age: (now - date).max(0.0).max(age + (now - sent).max(0.0)),
        lifetime,
        stale: seconds(cc.get("stale-while-revalidate")).unwrap_or(0.0),
        revalidate: cc.contains_key("must-revalidate") || cc.contains_key("proxy-revalidate"),
        shareable,
        vary: vary
            .into_iter()
            .map(|name| {
                let value = request.headers.get(&name).map(str::to_owned);
                (name, value)
            })
            .collect(),
    })
}
fn network(
    inner: &Arc<Inner>,
    fetch: &Fetch,
    request: Request,
    signal: &AbortSignal,
    store: bool,
) -> Result<StreamingResponse, HostError> {
    let sent = (inner.now)();
    let mut response = super::check_headers(fetch.stream(request.clone(), signal)?)?;
    signal.check()?;
    if store {
        if response.status == 304 {
            renew(inner, &request, &response, sent, signal);
            return Ok(response);
        }
        // A full answer supersedes the old representation even when it cannot be
        // kept. One being collected replaces it once its body is complete, so a
        // stale answer still serves while a refetch reads, and survives its failure.
        let full = ((200..500).contains(&response.status) && response.status != 206)
            || directives(&response.headers).contains_key("no-store");
        let mut collecting = false;
        if let Some(head) = candidate(&request, &response, sent, (inner.now)()) {
            let length = response
                .headers
                .get("content-length")
                .and_then(|s| s.parse::<usize>().ok())
                .unwrap_or(0);
            if length <= inner.limits.entry
                && inner
                    .storing
                    .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |n| {
                        (n < inner.limits.entries).then_some(n + 1)
                    })
                    .is_ok()
            {
                let collection = Collection {
                    inner: inner.clone(),
                    bytes: Vec::new(),
                    reserved: 0,
                };
                let body = std::mem::replace(
                    &mut response.body,
                    Body::new(
                        Box::new(std::io::Cursor::new(Vec::new())),
                        0,
                        signal.clone(),
                    ),
                );
                response.body = Body::new(
                    Box::new(Collecting {
                        body,
                        collection: Some(collection),
                        head,
                        request: request.clone(),
                        signal: signal.clone(),
                    }),
                    request.body_limit(),
                    signal.clone(),
                );
                collecting = true;
            }
        }
        if full && !collecting {
            supersede(inner, &request);
        }
    }
    Ok(response)
}
fn supersede(inner: &Inner, request: &Request) {
    inner
        .state
        .lock()
        .unwrap()
        .entries
        .retain(|(url, entry)| *url != request.url || !entry.head.matches(request));
}
fn renew(
    inner: &Arc<Inner>,
    request: &Request,
    response: &StreamingResponse,
    sent: f64,
    signal: &AbortSignal,
) {
    if response.redirected {
        return;
    }
    let mut state = inner.state.lock().unwrap();
    let Some(index) = state
        .entries
        .iter()
        .position(|(url, entry)| *url == request.url && entry.head.matches(request))
    else {
        return;
    };
    let old = &state.entries[index].1.head;
    let tag = old.headers.get("etag");
    let validates = if let Some(tag) = response.headers.get("etag") {
        old.headers.get("etag").is_some_and(|kept| {
            if let Some(weak) = tag.strip_prefix("W/") {
                kept.strip_prefix("W/").unwrap_or(kept) == weak
            } else {
                kept == tag
            }
        })
    } else if let Some(asked) = request.headers.get("if-none-match") {
        // Without a response validator, a list or wildcard cannot identify the bytes.
        tag.is_some_and(|tag| asked.trim().trim_start_matches("W/") == tag.trim_start_matches("W/"))
    } else {
        old.headers.get("last-modified").is_some_and(|modified| {
            request.headers.get("if-modified-since") == Some(modified)
                && response
                    .headers
                    .get("last-modified")
                    .is_none_or(|v| v == modified)
        })
    };
    // An unmatched 304 says the kept bytes are not the origin's current ones.
    if !validates {
        state.entries.remove(index);
        return;
    }
    let now = (inner.now)();
    let mut headers = old.headers.clone();
    for (name, value) in super::cache::end_to_end(&response.headers.entries().to_vec()) {
        if !matches!(
            name.as_str(),
            "content-length" | "content-encoding" | "content-range" | "etag" | "last-modified"
        ) {
            headers.set(&name, &value);
        }
    }
    if !response.headers.has("date") {
        headers.set("date", &super::cache::format_date(now as u64));
    }
    if !response.headers.has("age") {
        headers.delete("age");
    }
    let renewed = Response {
        status: old.status,
        status_text: old.status_text.clone(),
        headers,
        body: Vec::new(),
        url: request.url.clone(),
        redirected: false,
    }
    .into_stream(0, signal.clone());
    if let Some(head) = candidate(request, &renewed, sent, now) {
        state.entries[index].1.head = head;
    } else {
        state.entries.remove(index);
    }
}
struct Kept {
    body: Arc<Payload>,
    at: usize,
}
impl BodySource for Kept {
    fn read(&mut self, out: &mut [u8]) -> Result<usize, HostError> {
        let n = out.len().min(self.body.bytes.len() - self.at);
        out[..n].copy_from_slice(&self.body.bytes[self.at..self.at + n]);
        self.at += n;
        Ok(n)
    }
}
struct Collection {
    inner: Arc<Inner>,
    bytes: Vec<u8>,
    reserved: usize,
}
impl Drop for Collection {
    fn drop(&mut self) {
        drop(std::mem::take(&mut self.bytes));
        self.inner.bytes.fetch_sub(self.reserved, Ordering::Relaxed);
        self.inner.storing.fetch_sub(1, Ordering::Relaxed);
    }
}
impl Collection {
    fn reserve(&mut self, bytes: usize) -> bool {
        let mut state = self.inner.state.lock().unwrap();
        while self
            .inner
            .bytes
            .load(Ordering::Relaxed)
            .saturating_add(bytes)
            > self.inner.limits.bytes
        {
            if state.entries.pop_front().is_none() {
                return false;
            }
        }
        self.inner.bytes.fetch_add(bytes, Ordering::Relaxed);
        self.reserved += bytes;
        true
    }
    fn append(&mut self, bytes: &[u8]) -> bool {
        let size = self.bytes.len() + bytes.len();
        if size > self.inner.limits.entry {
            return false;
        }
        if size > self.bytes.capacity() {
            let capacity = size
                .max(self.bytes.capacity().saturating_mul(2))
                .min(self.inner.limits.entry);
            // Charge both allocations until the old buffer has been freed.
            if !self.reserve(capacity) {
                return false;
            }
            let mut grown = Vec::with_capacity(capacity);
            grown.extend_from_slice(&self.bytes);
            let old = std::mem::replace(&mut self.bytes, grown);
            let capacity = old.capacity();
            drop(old);
            self.inner.bytes.fetch_sub(capacity, Ordering::Relaxed);
            self.reserved -= capacity;
        }
        self.bytes.extend_from_slice(bytes);
        true
    }
    fn store(mut self, request: &Request, head: Candidate) {
        // The collected buffer and exact-size stored copy coexist, as in Bun's cache.
        if !self.reserve(self.bytes.len()) {
            supersede(&self.inner, request);
            return;
        }
        let bytes = self.bytes.clone();
        let old = std::mem::take(&mut self.bytes);
        let capacity = old.capacity();
        drop(old);
        self.inner.bytes.fetch_sub(capacity, Ordering::Relaxed);
        self.reserved -= capacity;
        let body = Arc::new(Payload {
            bytes,
            budget: self.inner.bytes.clone(),
        });
        self.reserved = 0;
        let url = request.url.clone();
        let mut state = self.inner.state.lock().unwrap();
        state.entries.retain(|(u, _)| *u != url);
        state.entries.push_back((url, Entry { head, body }));
        while state.entries.len() > self.inner.limits.entries {
            state.entries.pop_front();
        }
    }
}
struct Collecting {
    body: Body,
    collection: Option<Collection>,
    head: Candidate,
    request: Request,
    signal: AbortSignal,
}
impl BodySource for Collecting {
    // A body that fails or is let go leaves the old answer in place; one too
    // large to keep supersedes it.
    fn read(&mut self, out: &mut [u8]) -> Result<usize, HostError> {
        match self.body.read(out) {
            Ok(0) => {
                if let Some(collection) = self.collection.take() {
                    if !self.signal.aborted() {
                        collection.store(&self.request, self.head.clone());
                    }
                }
                Ok(0)
            }
            Ok(n) => {
                if self
                    .collection
                    .as_mut()
                    .is_some_and(|c| !c.append(&out[..n]))
                {
                    let inner = self.collection.take().unwrap().inner.clone();
                    supersede(&inner, &self.request);
                }
                Ok(n)
            }
            Err(error) => {
                self.collection.take();
                Err(error)
            }
        }
    }
}
fn refresh(inner: &Arc<Inner>, fetch: &Fetch, mut request: Request, url: String) {
    let abort = AbortController::new();
    {
        let mut state = inner.state.lock().unwrap();
        if state.refreshing.contains_key(&url) || state.refreshing.len() >= 6 {
            return;
        }
        state.refreshing.insert(url.clone(), abort.clone());
    }
    // The server owns this refetch; a render's deadline cancels only that render.
    request.timeout = request.timeout.or(Some(std::time::Duration::from_secs(2)));
    let inner = inner.clone();
    let fetch = fetch.clone();
    let cleanup = Refresh {
        inner: inner.clone(),
        url,
    };
    let _ = std::thread::Builder::new()
        .name("render-http-cache".into())
        .spawn(move || {
            let _cleanup = cleanup;
            if let Ok(mut response) = network(&inner, &fetch, request, &abort.signal(), true) {
                let mut bytes = [0; 16384];
                while matches!(response.body.read(&mut bytes), Ok(n) if n != 0) {}
            }
        });
}
struct Refresh {
    inner: Arc<Inner>,
    url: String,
}
impl Drop for Refresh {
    fn drop(&mut self) {
        self.inner
            .state
            .lock()
            .unwrap()
            .refreshing
            .remove(&self.url);
        #[cfg(test)]
        self.inner.ready.notify_all();
    }
}

#[cfg(test)]
#[path = "shared_cache_tests.rs"]
mod tests;
