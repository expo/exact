//! The app executor's private, bounded HTTP cache (LLP 1162 D4; RFC 9111).
use super::*;
use exact_runner::CacheMode;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub(super) const CAPACITY: usize = 16 << 20;
pub(super) const MAX_ENTRY: usize = CAPACITY / 4;
type Headers = Vec<(String, String)>;

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    method: String,
    url: String,
    headers: Headers,
}
impl Key {
    fn of(request: &Request) -> Self {
        let mut headers: Headers = request
            .headers
            .iter()
            .map(|(k, v)| (k.to_ascii_lowercase(), v.clone()))
            .collect();
        headers.sort_by(|a, b| a.0.cmp(&b.0));
        Self {
            method: request.method.clone(),
            url: identity(&request.url),
            headers,
        }
    }
}

struct Entry {
    key: Key,
    // Secret writes reach HTTP copies after the holds release their keys (LLP 1162.000 D3).
    preloads: u8,
    response: Response,
    stored: Instant,
    age: Duration,
    lifetime: Duration,
    validate: bool,
    bytes: usize,
    used: AtomicUsize,
}
impl Entry {
    fn age(&self, now: Instant) -> Duration {
        self.age
            .saturating_add(now.saturating_duration_since(self.stored))
    }
    fn fresh(&self, request: &Request, now: Instant) -> bool {
        let directives = Directives::of(&request.headers);
        let age = self.age(now);
        !self.validate
            && !directives.no_cache
            && !values(&request.headers, "pragma").any(|v| v.eq_ignore_ascii_case("no-cache"))
            && age.saturating_add(Duration::from_secs(directives.min_fresh.unwrap_or(0)))
                < self.lifetime
            && directives
                .max_age
                .is_none_or(|n| age <= Duration::from_secs(n))
    }
    fn response_head(&self, now: Instant) -> Response {
        let mut response = Response {
            status: self.response.status,
            headers: self.response.headers.clone(),
            body: Vec::new(),
        };
        set(
            &mut response.headers,
            "age",
            &self.age(now).as_secs().to_string(),
        );
        response
    }
    fn response(&self, now: Instant) -> Response {
        let mut response = self.response_head(now);
        response.body = self.response.body.clone();
        response
    }
}

pub(super) struct Exchange {
    key: Key,
    previous: Option<Arc<Entry>>,
    started: Instant,
    store: bool,
}

pub(super) struct Cache {
    pub(super) preload_names: bool,
    pub(super) preload_definitions_due: Option<Instant>,
    pub(super) preload_launch_due: Option<Instant>,
    pub(super) preload_defined: std::collections::BTreeSet<String>,
    pub(super) grants: String,
    entries: VecDeque<Arc<Entry>>,
    // In-flight validations still own these bytes (LLP 1162 D4's bound).
    retired: Vec<Arc<Entry>>,
    pub(super) bytes: usize,
    pub(super) preloads: Vec<super::preload::Held>,
    pub(super) journal: Vec<super::preload::Fact>,
    pub(super) clock: Arc<dyn Fn() -> Instant + Send + Sync>,
    tick: usize,
}
impl Cache {
    pub(super) fn new(grants: String) -> Self {
        Self {
            preload_names: true,
            preload_definitions_due: None,
            preload_launch_due: None,
            preload_defined: Default::default(),
            grants,
            entries: VecDeque::new(),
            retired: Vec::new(),
            bytes: 0,
            preloads: Vec::new(),
            journal: Vec::new(),
            clock: Arc::new(Instant::now),
            tick: 0,
        }
    }
    fn collect(&mut self) {
        super::preload::expire(self);
        self.retired.retain(|e| {
            if Arc::strong_count(e) == 1 {
                self.bytes -= e.bytes;
                false
            } else {
                true
            }
        });
    }
    fn remove(&mut self, index: usize) {
        let entry = self.entries.remove(index).expect("cache entry");
        if Arc::strong_count(&entry) == 1 {
            self.bytes -= entry.bytes;
        } else {
            self.retired.push(entry);
        }
    }
    fn drop_key(&mut self, key: &Key) {
        if let Some(index) = self.entries.iter().position(|e| e.key == *key) {
            self.remove(index);
        }
    }
    fn find(&mut self, key: &Key) -> Option<Arc<Entry>> {
        let index = self.entries.iter().position(|e| e.key == *key)?;
        let entry = self.entries.remove(index).expect("cache entry");
        entry.used.store(self.touch(), Ordering::Relaxed);
        self.entries.push_back(entry.clone());
        Some(entry)
    }
    fn insert(&mut self, entry: Entry) {
        self.collect();
        if entry.bytes > MAX_ENTRY {
            return;
        }
        if self.room(entry.bytes) {
            entry.used.store(self.touch(), Ordering::Relaxed);
            self.bytes += entry.bytes;
            self.entries.push_back(Arc::new(entry));
        }
    }
    pub(super) fn touch(&mut self) -> usize {
        self.tick += 1;
        self.tick
    }
    pub(super) fn room(&mut self, bytes: usize) -> bool {
        self.collect();
        while bytes > CAPACITY.saturating_sub(self.bytes) {
            let held = self
                .preloads
                .iter()
                .enumerate()
                .filter_map(|(i, p)| p.used().map(|used| (i, used)))
                .min_by_key(|(_, used)| *used);
            let entry = self.entries.front().map(|e| e.used.load(Ordering::Relaxed));
            match (held, entry) {
                (Some((i, used)), entry) if entry.is_none_or(|e| used < e) => {
                    super::preload::drop_held(self, i, super::preload::Event::NeverTaken);
                }
                (_, Some(_)) => self.remove(0),
                _ => return false,
            }
        }
        true
    }
    #[allow(dead_code)] // Some shells expose no launch entry (LLP 1162.000 D4).
    pub(super) fn charge(&mut self, bytes: usize) {
        self.bytes += bytes;
    }
    pub(super) fn release(&mut self, bytes: usize) {
        self.bytes -= bytes;
    }
}

impl Core {
    /// Keep a private HTTP cache for the app's reads (LLP 1162 D4).
    pub fn enable_http_cache(&self) {
        self.shared.state.lock().unwrap().cache = Some(Cache::new(self.grants.clone()));
    }
}

fn eligible(request: &Request, class: RequestClass) -> bool {
    class == RequestClass::Read && request.method.eq_ignore_ascii_case("GET")
        && request.http == HttpScheduling::Ordered && request.is_plain_http()
        // Partial and caller-conditional requests are forwarded (RFC 9111 §4).
        && !request.headers.iter().any(|(k, _)| matches!(k.to_ascii_lowercase().as_str(),
            "range" | "if-range" | "if-match" | "if-unmodified-since" | "if-none-match" | "if-modified-since"))
}

pub(super) fn barrier(request: &Request, class: RequestClass, work: bool) -> bool {
    class != RequestClass::Read || !request.is_plain_http() || work
}

pub(super) fn replacing(
    request: &Request,
    class: RequestClass,
    forced: bool,
    work: bool,
) -> Option<Key> {
    (!work
        && eligible(request, class)
        && request.cache != CacheMode::NoStore
        && !Directives::of(&request.headers).no_store
        && (forced || matches!(request.cache, CacheMode::Reload | CacheMode::NoCache)))
    .then(|| Key::of(request))
}

fn replacing_before(state: &State, ticket: u64, key: &Key) -> bool {
    state.ordered.iter().take_while(|t| **t != ticket).any(|t| {
        state
            .running
            .iter()
            .any(|r| r.ticket == *t && !r.terminal && r.cache_replacing.as_ref() == Some(key))
            || state.jobs[0].iter().any(|j| {
                j.ticket == *t
                    && replacing(&j.request, j.class, j.forced, j.work.is_some()).as_ref()
                        == Some(key)
            })
    })
}

pub(super) fn granted(grants: &str, request: &Request) -> Result<(), Outcome> {
    granted_url(grants, request, &request.url)
}

pub(super) fn granted_url(grants: &str, request: &Request, url: &str) -> Result<(), Outcome> {
    let grants = exact_data::storage::scope(grants, request.grants.as_deref())
        .and_then(|s| ibex2::grant::GrantSet::parse(&exact_runner::io_grants(s)))
        .map_err(|message| failed(FailureKind::Refused, message))?;
    let checked = ibex2::stdlib::fetch::origin_of(url).and_then(|origin| {
        ibex2::boundary::admit(&grants, &ibex2::grant::Operation::Fetch { origin })
    });
    checked.map_err(|e| fetch_failure(e, &AbortController::new()))
}

/// Hits settle without a worker, behind earlier non-reads (LLP 1162 D1, D4).
pub(super) fn serve_ready(state: &mut State) {
    super::preload::serve_ready(state);
    if !super::preload::reserve_front(state) {
        return;
    }
    if state.retired
        || state.cache.is_none()
        || state
            .running
            .iter()
            .any(|r| r.lane == 0 && r.blocking && r.cache_barrier)
    {
        return;
    }
    let mut index = 0;
    while index < state.jobs[0].len() {
        if !super::preload::reserve_front(state) {
            break;
        }
        let job = &state.jobs[0][index];
        if job.class != RequestClass::Read || !job.request.is_plain_http() || job.work.is_some() {
            break;
        }
        if super::preload::waiting(state, job)
            || state
                .cache
                .as_ref()
                .is_some_and(|c| super::preload::different_body(c, &job.request))
            || job.request.reach.origin != exact_runner::CallOrigin::App
            || job.forced
            || matches!(
                job.request.cache,
                CacheMode::NoStore | CacheMode::Reload | CacheMode::NoCache
            )
            || !eligible(&job.request, job.class)
            || Directives::of(&job.request.headers).no_store
        {
            index += 1;
            continue;
        }
        let key = Key::of(&job.request);
        if replacing_before(state, job.ticket, &key) {
            index += 1;
            continue;
        }
        let cache = state.cache.as_mut().expect("enabled");
        let now = Instant::now();
        // Fetch §4.6 allows force-cache and only-if-cached to use stored replies
        // under no-cache or must-revalidate, overriding RFC 9111 §4.2.4 by
        // author precedence (LLP 1162 D4; LLP 1115). Native apps have no page
        // origin to restrict the hit.
        let Some(entry) = cache.entries.iter().find(|e| {
            e.key == key
                && (matches!(
                    job.request.cache,
                    CacheMode::ForceCache | CacheMode::OnlyIfCached
                ) || e.fresh(&job.request, now))
        }) else {
            index += 1;
            continue;
        };
        let checked = granted(&cache.grants, &job.request);
        let (outcome, cached) = match checked {
            Ok(()) => {
                let mut response = entry.response_head(now);
                let bytes = response.body.capacity()
                    + header_bytes(&response.headers)
                    + entry.response.body.len()
                    + std::mem::size_of::<Completed>();
                if bytes > BYTES[0].saturating_sub(state.bytes[0] - job.charge) {
                    break;
                }
                response.body = entry.response.body.clone();
                (Outcome::Response(response), true)
            }
            Err(outcome) => (outcome, false),
        };
        let bytes = retained(&outcome) + std::mem::size_of::<Completed>();
        if bytes > BYTES[0].saturating_sub(state.bytes[0] - job.charge) {
            break;
        }
        if cached {
            cache.find(&key);
        }
        let job = state.jobs[0].remove(index).expect("cache job");
        state.bytes[0] = state.bytes[0] - job.charge + bytes;
        state.light += 1;
        state.completed[0].push_back(Completed {
            elapsed_ms: 0,
            finished_at: now,
            ticket: job.ticket,
            outcome,
            bytes,
            message: false,
            stream: false,
            light: true,
            again: false,
            reply: false,
            cached,
        });
    }
    if has_ready(state) {
        wake(state);
    }
}

pub(super) fn missing(request: &Request, forced: bool) -> Option<Outcome> {
    (request.cache == CacheMode::OnlyIfCached && !forced).then(|| {
        failed(
            FailureKind::Network,
            "only-if-cached: no reply is stored for this request",
        )
    })
}

pub(super) fn prepare(
    shared: &Shared,
    ticket: u64,
    request: &mut Request,
    class: RequestClass,
    forced: bool,
) -> Option<Outcome> {
    if !eligible(request, class) || request.cache == CacheMode::NoStore {
        return None;
    }
    // A read can carry a tool's reach (LLP 1133.036 D6). Its hit check can
    // resolve a name, so it runs on the worker or starter, outside the state lock.
    let checked = reach::admit_cached(request);
    let mut state = shared.state.lock().unwrap();
    state.cache.as_ref()?;
    let key = Key::of(request);
    let replacing = replacing_before(&state, ticket, &key)
        || state
            .cache
            .as_ref()
            .is_some_and(|c| super::preload::different_body(c, request));
    let cache = state.cache.as_mut().expect("enabled");
    {
        let now = Instant::now();
        if !forced
            && !replacing
            && !matches!(request.cache, CacheMode::Reload | CacheMode::NoCache)
            && !Directives::of(&request.headers).no_store
        {
            if let Some(entry) = cache.entries.iter().find(|e| {
                e.key == key
                    && (matches!(
                        request.cache,
                        CacheMode::ForceCache | CacheMode::OnlyIfCached
                    ) || e.fresh(request, now))
            }) {
                let outcome = match checked {
                    Ok(()) => granted(&cache.grants, request).map(|()| entry.response(now)),
                    Err(e) => Err(fetch_failure(e, &AbortController::new())),
                };
                let cached = outcome.is_ok();
                if cached {
                    cache.find(&key);
                }
                if let Some(run) = state.running.iter_mut().find(|r| r.ticket == ticket) {
                    run.cached = cached;
                }
                return Some(outcome.map_or_else(|outcome| outcome, Outcome::Response));
            }
        }
    }
    let store = !Directives::of(&request.headers).no_store;
    let previous = if !forced && !replacing && request.cache != CacheMode::Reload && store {
        cache.find(&key)
    } else {
        None
    };
    let previous = previous.filter(|e| {
        first(&e.response.headers, "etag").is_some()
            || first(&e.response.headers, "last-modified").is_some()
    });
    if let Some(entry) = &previous {
        for (validator, condition) in [
            ("etag", "If-None-Match"),
            ("last-modified", "If-Modified-Since"),
        ] {
            if let Some(value) = first(&entry.response.headers, validator) {
                set(&mut request.headers, condition, value);
            }
        }
    }
    if let Some(run) = state.running.iter_mut().find(|r| r.ticket == ticket) {
        run.cache_exchange = Some(Exchange {
            key,
            previous,
            started: Instant::now(),
            store,
        });
    }
    None
}

pub(super) fn retry_request(
    shared: &Shared,
    ticket: u64,
    request: &ibex2::stdlib::fetch::Request,
) -> Option<ibex2::stdlib::fetch::Request> {
    let state = shared.state.lock().unwrap();
    state
        .running
        .iter()
        .find(|r| r.ticket == ticket)?
        .cache_exchange
        .as_ref()?
        .previous
        .as_ref()?;
    let mut request = request.clone();
    request.headers.delete("if-none-match");
    request.headers.delete("if-modified-since");
    Some(request)
}

/// A redirected 304 answers our conditional, never the caller (LLP 1162 D4).
pub(super) fn retry(state: &mut State, ticket: u64, status: u16, redirected: bool) -> bool {
    if status != 304 || !redirected {
        return false;
    }
    let Some(exchange) = state
        .running
        .iter_mut()
        .find(|r| r.ticket == ticket)
        .and_then(|r| r.cache_exchange.as_mut())
    else {
        return false;
    };
    if exchange.previous.take().is_none() {
        return false;
    }
    exchange.started = Instant::now();
    if let Some(cache) = state.cache.as_mut() {
        cache.drop_key(&exchange.key);
        cache.collect();
    }
    true
}

pub(super) fn response(state: &mut State, ticket: u64, response: &mut Response, redirected: bool) {
    let Some(run) = state.running.iter_mut().find(|r| r.ticket == ticket) else {
        return;
    };
    let Some(exchange) = run.cache_exchange.take() else {
        return;
    };
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    if state.retired {
        return;
    }
    let now = Instant::now();
    let wall = SystemTime::now();
    let Exchange {
        key,
        previous,
        started,
        store,
    } = exchange;
    if redirected {
        if store {
            cache.drop_key(&key);
        }
        return;
    }
    if response.status == 304 {
        if let Some(old) = &previous {
            let mut renewed = old.response.clone();
            let mut headers = end_to_end(&response.headers);
            // A 304 cannot relabel the stored bytes with new validators or a
            // new byte description (RFC 9111 §3.2, §4.3.4; LLP 1162 D4).
            headers.retain(|(name, _)| {
                !matches!(
                    name.to_ascii_lowercase().as_str(),
                    "content-length"
                        | "content-encoding"
                        | "content-range"
                        | "etag"
                        | "last-modified"
                )
            });
            if first(&headers, "date").is_none() {
                set(&mut headers, "date", &format_date(epoch(wall)));
            }
            for (name, _) in &headers {
                renewed
                    .headers
                    .retain(|(k, _)| !k.eq_ignore_ascii_case(name));
            }
            renewed.headers.extend(headers);
            // A 304's missing Age is zero for this exchange, not the old Age (§4.2.3).
            if first(&response.headers, "age").is_none() {
                renewed
                    .headers
                    .retain(|(k, _)| !k.eq_ignore_ascii_case("age"));
            }
            *response = renewed;
            run.cached = true;
        }
    }
    let preloads = previous.as_ref().map_or(0, |entry| entry.preloads);
    drop(previous);
    store_response(
        cache, key, started, store, response, run.cached, preloads, now, wall,
    );
}

#[allow(dead_code)] // Some shells expose no launch entry (LLP 1162.000 D4).
pub(super) fn store_preload(
    cache: &mut Cache,
    index: usize,
    request: &Request,
    response: &mut Response,
    started: Instant,
    redirected: bool,
) {
    if !redirected && eligible(request, RequestClass::Read) && request.cache != CacheMode::NoStore {
        store_response(
            cache,
            Key::of(request),
            started,
            !Directives::of(&request.headers).no_store,
            response,
            false,
            1 << index,
            Instant::now(),
            SystemTime::now(),
        );
    }
}

#[allow(clippy::too_many_arguments)]
fn store_response(
    cache: &mut Cache,
    key: Key,
    started: Instant,
    store: bool,
    response: &mut Response,
    cached: bool,
    preloads: u8,
    now: Instant,
    wall: SystemTime,
) {
    if !store {
        return;
    }
    if response.status != 200 && !Directives::of(&response.headers).no_store {
        return;
    }
    let preloads = preloads
        | cache
            .entries
            .iter()
            .find(|e| e.key == key)
            .map_or(0, |e| e.preloads);
    cache.drop_key(&key);
    cache.collect();
    if response.status != 200 {
        return;
    }
    let mut headers = end_to_end(&response.headers);
    if first(&headers, "date").is_none() {
        set(&mut headers, "date", &format_date(epoch(wall)));
    }
    let received = wall.duration_since(UNIX_EPOCH).unwrap_or_default();
    let delay = now.saturating_duration_since(started);
    if cached {
        set(
            &mut response.headers,
            "age",
            &current_age(&headers, delay, received).as_secs().to_string(),
        );
    }
    let Some((age, lifetime, validate)) = freshness(&headers, delay, received) else {
        return;
    };
    let bytes = response.body.len()
        + header_bytes(&headers)
        + std::mem::size_of::<Entry>()
        + key.method.capacity()
        + key.url.capacity()
        + header_bytes(&key.headers);
    if bytes > MAX_ENTRY {
        return;
    }
    let response = Response {
        status: 200,
        headers,
        body: response.body.clone(),
    };
    cache.insert(Entry {
        key,
        preloads,
        response,
        stored: now,
        age,
        lifetime,
        validate,
        bytes,
        used: AtomicUsize::new(0),
    });
}

fn freshness(
    headers: &Headers,
    delay: Duration,
    received: Duration,
) -> Option<(Duration, Duration, bool)> {
    let directives = Directives::of(headers);
    if directives.no_store || values(headers, "vary").any(|v| list(v).any(|n| n.trim() == "*")) {
        return None;
    }
    let date = first(headers, "date")
        .and_then(http_date)
        .unwrap_or(received.as_secs());
    let expires = first(headers, "expires").and_then(http_date);
    let lifetime = directives
        .max_age
        .or_else(|| expires.map(|e| e.saturating_sub(date)));
    if lifetime.is_none()
        && first(headers, "etag").is_none()
        && first(headers, "last-modified").is_none()
    {
        return None;
    }
    Some((
        current_age(headers, delay, received),
        Duration::from_secs(lifetime.unwrap_or(0)),
        directives.no_cache
            || directives.invalid_age
            || first(headers, "date").is_some_and(|date| http_date(date).is_none()),
    ))
}

fn current_age(headers: &Headers, delay: Duration, received: Duration) -> Duration {
    let date = first(headers, "date")
        .and_then(http_date)
        .map(Duration::from_secs)
        .unwrap_or(received);
    let age = first(headers, "age")
        .map_or(Some(0), seconds)
        .unwrap_or(u64::MAX);
    received
        .saturating_sub(date)
        .max(Duration::from_secs(age).saturating_add(delay))
}

pub(super) fn identity(value: &str) -> String {
    let Ok(url) = url::Url::parse(value) else {
        return value.to_owned();
    };
    let Some((_, authority)) = value.split_once("://") else {
        return value.to_owned();
    };
    let at = authority.find(['/', '?', '#']).unwrap_or(authority.len());
    let target = authority[at..].split('#').next().unwrap_or("");
    // The portable transport sends the path and query verbatim. In particular,
    // dot segments and percent encodings must not collide (LLP 1162 D4).
    format!(
        "{}{}{}",
        &url[..url::Position::BeforePath],
        if target.starts_with('/') { "" } else { "/" },
        target
    )
}

pub(super) fn invalidates(request: &Request, synthetic: bool) -> Option<(String, bool)> {
    unsafe_method(&request.method).then(|| (identity(&request.url), synthetic))
}
fn unsafe_method(method: &str) -> bool {
    !["GET", "HEAD", "OPTIONS", "TRACE"]
        .iter()
        .any(|m| method.eq_ignore_ascii_case(m))
}
/// What a write's outcome drops. A fetch's answer has dropped it at each hop's
/// head (`observe`), and a driver's answer has no hop. A write that failed may
/// have been applied: a connection lost before the transport tells a head
/// leaves nothing to observe, and URLSession tells a head only once some of
/// its body has come (LLP 1162 D4).
pub(super) fn invalidate(
    state: &mut State,
    invalidation: Option<&(String, bool)>,
    outcome: &Outcome,
) {
    match (invalidation, outcome) {
        (Some((url, true)), Outcome::Response(response)) => {
            invalidate_head(state, url, response.status, &response.headers)
        }
        (Some((url, _)), Outcome::Failed { .. }) => drop_urls(state, &[identity(url)]),
        _ => {}
    }
}
fn invalidate_head(state: &mut State, url: &str, status: u16, headers: &[(String, String)]) {
    if !(200..400).contains(&status) {
        return;
    }
    let Ok(base) = url::Url::parse(url) else {
        return;
    };
    let mut urls = vec![identity(url)];
    for name in ["location", "content-location"] {
        for value in values(headers, name) {
            if let Ok(target) = base.join(value) {
                if target.origin() == base.origin() {
                    urls.push(identity(target.as_str()));
                }
            }
        }
    }
    drop_urls(state, &urls);
}
fn drop_urls(state: &mut State, urls: &[String]) {
    let Some(cache) = state.cache.as_mut() else {
        return;
    };
    super::preload::drop_urls(cache, urls);
    for index in (0..cache.entries.len()).rev() {
        if urls.contains(&cache.entries[index].key.url) {
            cache.remove(index);
        }
    }
    cache.collect();
}
#[allow(dead_code)] // Some shells expose no launch entry (LLP 1162.000 D4).
pub(super) fn drop_preload_copy(cache: &mut Cache, index: usize) {
    for i in (0..cache.entries.len()).rev() {
        if cache.entries[i].preloads & (1 << index) != 0 {
            cache.remove(i);
        }
    }
    cache.collect();
}

pub(super) fn observe(shared: &Arc<Shared>, request: &mut ibex2::stdlib::fetch::Request) {
    if !unsafe_method(&request.method) {
        return;
    }
    let shared = Arc::downgrade(shared);
    request.received = Some(ibex2::stdlib::fetch::Received(Arc::new(
        move |method, url, status, headers| {
            if unsafe_method(method) {
                if let Some(shared) = shared.upgrade() {
                    invalidate_head(
                        &mut shared.state.lock().unwrap(),
                        url,
                        status,
                        headers.entries(),
                    );
                    #[cfg(test)]
                    shared.ready.notify_all();
                }
            }
        },
    )));
}

fn values<'a>(headers: &'a [(String, String)], name: &'a str) -> impl Iterator<Item = &'a str> {
    headers
        .iter()
        .filter(move |(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}
fn first<'a>(headers: &'a [(String, String)], name: &str) -> Option<&'a str> {
    headers
        .iter()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.trim())
}
fn set(headers: &mut Headers, name: &str, value: &str) {
    headers.retain(|(k, _)| !k.eq_ignore_ascii_case(name));
    headers.push((name.into(), value.into()));
}
pub(super) fn header_bytes(headers: &Headers) -> usize {
    headers.capacity() * std::mem::size_of::<(String, String)>()
        + headers
            .iter()
            .map(|(k, v)| k.capacity() + v.capacity())
            .sum::<usize>()
}
pub(super) fn end_to_end(headers: &Headers) -> Headers {
    let connection: Vec<_> = values(headers, "connection")
        .flat_map(list)
        .map(str::trim)
        .collect();
    headers
        .iter()
        .filter(|(k, _)| {
            !matches!(
                k.to_ascii_lowercase().as_str(),
                "connection"
                    | "keep-alive"
                    | "proxy-authenticate"
                    | "proxy-authorization"
                    | "te"
                    | "trailer"
                    | "transfer-encoding"
                    | "upgrade"
            ) && !connection.iter().any(|name| k.eq_ignore_ascii_case(name))
        })
        .cloned()
        .collect()
}
// RFC 9110 §5.6.4: a quoted-pair consumes its quote or backslash as data.
fn list(value: &str) -> impl Iterator<Item = &str> {
    let (mut quoted, mut escaped) = (false, false);
    value.split(move |c| {
        if escaped {
            escaped = false;
            return false;
        }
        match c {
            '\\' if quoted => escaped = true,
            '"' => quoted = !quoted,
            ',' if !quoted => return true,
            _ => {}
        }
        false
    })
}

fn seconds(s: &str) -> Option<u64> {
    if s.is_empty() || !s.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    Some(s.parse().unwrap_or(u64::MAX))
}

#[derive(Default)]
struct Directives {
    no_store: bool,
    no_cache: bool,
    max_age: Option<u64>,
    min_fresh: Option<u64>,
    invalid_age: bool,
}
impl Directives {
    fn of(headers: &Headers) -> Self {
        let mut out = Self::default();
        let mut age_seen = false;
        for value in values(headers, "cache-control") {
            for directive in list(value) {
                let (name, value) = directive
                    .trim()
                    .split_once('=')
                    .unwrap_or((directive.trim(), ""));
                let value = value.trim().trim_matches('"');
                match name.trim().to_ascii_lowercase().as_str() {
                    "no-store" => out.no_store = true,
                    "no-cache" => out.no_cache = true,
                    "max-age" => {
                        let age = seconds(value);
                        out.invalid_age |= age_seen || age.is_none();
                        age_seen = true;
                        out.max_age = Some(age.unwrap_or(0));
                    }
                    "min-fresh" => out.min_fresh = seconds(value),
                    _ => {}
                }
            }
        }
        if out.invalid_age {
            out.max_age = Some(0);
        }
        out
    }
}

fn epoch(time: SystemTime) -> u64 {
    time.duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub(super) fn format_date(seconds: u64) -> String {
    let days = (seconds / 86400) as i64;
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    year += i64::from(month <= 2);
    let weekday = ["Thu", "Fri", "Sat", "Sun", "Mon", "Tue", "Wed"][(days % 7) as usize];
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ][(month - 1) as usize];
    format!(
        "{weekday}, {day:02} {month} {year:04} {:02}:{:02}:{:02} GMT",
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60
    )
}

// RFC 9110 §5.6.7: recipients accept IMF-fixdate and both obsolete forms.
pub(super) fn http_date(value: &str) -> Option<u64> {
    let parts: Vec<_> = value.split_ascii_whitespace().collect();
    let (day, month, year, time) = match parts.as_slice() {
        [_, day, month, year, time, "GMT"] => (*day, *month, year.parse::<i64>().ok()?, *time),
        [_, date, time, "GMT"] => {
            let fields: Vec<_> = date.split('-').collect();
            let [day, month, year] = fields.as_slice() else {
                return None;
            };
            if year.len() != 2 {
                return None;
            }
            let year = year.parse::<i64>().ok()?;
            // RFC 850 dates more than 50 years ahead mean the preceding century.
            let current = 1970 + (epoch(SystemTime::now()) / 31_556_952) as i64;
            let mut full = current / 100 * 100 + year;
            if full > current + 50 {
                full -= 100;
            }
            (*day, *month, full, *time)
        }
        [_, month, day, time, year] => (*day, *month, year.parse::<i64>().ok()?, *time),
        _ => return None,
    };
    if !(0..=9999).contains(&year) {
        return None;
    }
    let month = [
        "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
    ]
    .iter()
    .position(|m| m.eq_ignore_ascii_case(month))? as i64
        + 1;
    let day = day.parse::<i64>().ok()?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days = [
        31,
        if leap { 29 } else { 28 },
        31,
        30,
        31,
        30,
        31,
        31,
        30,
        31,
        30,
        31,
    ];
    if !(1..=days[(month - 1) as usize]).contains(&day) {
        return None;
    }
    let clock: Vec<_> = time
        .split(':')
        .map(str::parse::<i64>)
        .collect::<Result<_, _>>()
        .ok()?;
    let [h, m, s] = clock.as_slice() else {
        return None;
    };
    if !(0..24).contains(h) || !(0..60).contains(m) || !(0..=60).contains(s) {
        return None;
    }
    let (year, month) = if month <= 2 {
        (year - 1, month + 9)
    } else {
        (year, month - 3)
    };
    let era = year.div_euclid(400);
    let y = year - era * 400;
    let days = era * 146097 + y * 365 + y / 4 - y / 100 + (153 * month + 2) / 5 + day - 1 - 719468;
    u64::try_from(days * 86400 + h * 3600 + m * 60 + s).ok()
}

#[cfg(test)]
#[path = "cache_tests.rs"]
pub(super) mod tests;
