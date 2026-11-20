//! The HTTP part of a native background-transfer carrier (LLP 1133.017 D3/D4).
//! Runs on the platform's worker, without a runner, JS engine or app session.
//! The caller owns scheduling, generation checks, file custody and durable results.

use ibex2::{
    boundary::HostError,
    host::Fetch,
    secrets::SecretStore,
    stdlib::{
        abort::AbortSignal,
        fetch::{self, BodyFile, FileBody, Request, Sent},
    },
};
use std::{
    fs::File,
    io::{Seek, Write},
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

const CHUNK: usize = 64 * 1024;
const METADATA_BYTES: usize = 64 * 1024;
const HEADER_COUNT: usize = 256;

/// A source-endowed fetch binding and the host's existing secret store.
/// The binding must retain the original call's origin gate and source grants,
/// including after a process restart. This type creates neither a transport nor
/// a store: Android supplies the same ones it supplies to ordinary fetch.
pub struct Carrier {
    fetch: Fetch,
    secrets: Arc<dyn SecretStore>,
}

/// Preserve the source's gate while refusing credentials on transfer redirects too.
struct TransferGate(Fetch);
impl ibex2::boundary::HopGate for TransferGate {
    fn admit(&self, url: &url::Url) -> Result<(), HostError> {
        if !url.username().is_empty() || url.password().is_some() {
            return Err(HostError::Failed("TypeError: invalid HTTP URL".into()));
        }
        self.0.admit(url.as_str())
    }
}

/// One unfinished request. Files are already admitted and opened by the host.
/// `upload` is the carrier's private snapshot, never the app's mutable original.
/// `response` is an empty staging file, never `saveTo` itself. Publish it only
/// after success and a generation check; discard it on failure or cancellation.
pub struct Transfer {
    /// HTTP method, normalized for Fetch before sending.
    pub method: String,
    /// Generation-specific key previously written by `Carrier::keep`.
    pub credential_key: String,
    /// Opened carrier-owned upload snapshot, if this request sends a body.
    pub upload: Option<File>,
    /// Empty private staging file; the caller owns its eventual placement.
    pub response: File,
    /// Bytes still allowed by the registration's ceiling, after completed
    /// requests. `None` is unlimited; `Some(0)` allows an empty response only.
    pub remaining: Option<u64>,
}

/// Byte counts for this attempt, never response or request contents.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Progress {
    /// Per attempt, starting again at zero when a redirect resends the body.
    Upload {
        /// Bytes the transport has written.
        sent: u64,
        /// Length of the upload snapshot.
        total: u64,
    },
    /// Response bytes copied to the staging file.
    Download {
        /// Bytes copied in this attempt.
        received: u64,
    },
}

/// No raw transport error crosses the carrier: it may contain a signed URL.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Failure {
    /// A malformed method or a body on GET/HEAD; never retry as a network error.
    InvalidRequest,
    /// The scheduler distinguishes a system stop from an app abort.
    Cancelled,
    /// Admission, redirect, credential decoding or network failure.
    Fetch,
    /// The response would exceed the registration's remaining allowance.
    DownloadTotalExceeded,
    /// A transport interruption that the platform may retry from byte zero.
    Network,
    /// Reading credentials or accessing a carrier-owned file failed.
    Storage,
}

/// A failure before the platform has accepted the registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// Malformed or oversized URL, method or headers.
    InvalidRequest,
    /// The source's grant or origin gate denied the URL.
    NotAllowed,
    /// Reading credentials or accessing a carrier-owned file failed.
    Storage,
}

/// Metadata safe for a durable public record. A non-2xx status is a completed
/// response whose registration ends `bad-status`; its response file is kept too.
#[derive(Debug, PartialEq, Eq)]
pub struct Completed {
    /// Original URL without userinfo, query or fragment.
    pub url: String,
    /// Final HTTP response status.
    pub status: u16,
    /// Final response's Content-Type, or an empty string.
    pub content_type: String,
    /// Bytes durably written to the staging file.
    pub downloaded: u64,
}

impl Carrier {
    /// Use a host-endowed binding and its existing credential-store backend.
    pub fn new(fetch: Fetch, secrets: Arc<dyn SecretStore>) -> Self {
        let fetch = fetch.clone().with_gate(Arc::new(TransferGate(fetch)));
        Self { fetch, secrets }
    }

    /// After `validate_method`, validate and keep the URL (including its query) and
    /// headers only in the secret store. The key is host-owned and includes the
    /// registration's generation and request index; WorkManager carries only the registration identity.
    pub fn keep(
        &self,
        key: &str,
        url: &str,
        headers: &[(String, String)],
    ) -> Result<String, Refusal> {
        self.keep_with_form(key, url, headers, None)
    }

    /// Keep the multipart recipe with its URL and headers for recovery during snapshot preparation.
    pub fn keep_with_form(
        &self,
        key: &str,
        url: &str,
        headers: &[(String, String)],
        form: Option<&serde_json::Value>,
    ) -> Result<String, Refusal> {
        let mut headers = headers.to_vec();
        let mut value = serde_json::json!({ "url": url });
        if let Some(parts) = form {
            if headers
                .iter()
                .any(|(name, _)| name.eq_ignore_ascii_case("content-type"))
            {
                return Err(Refusal::InvalidRequest);
            }
            let boundary =
                ibex2::stdlib::multipart::generate_boundary().map_err(|_| Refusal::Storage)?;
            headers.push((
                "Content-Type".into(),
                format!("multipart/form-data; boundary={boundary}"),
            ));
            value["boundary"] = boundary.into();
            value["form"] = parts.clone();
        }
        let (request, public) = credentials(url, &headers)?;
        self.fetch.admit(&request.url).map_err(refusal)?;
        value["headers"] = serde_json::json!(headers);
        let value = value.to_string();
        if value.len() > METADATA_BYTES {
            return Err(Refusal::InvalidRequest);
        }
        self.secrets
            .set(key, &value)
            .map_err(|_| Refusal::Storage)?;
        Ok(public)
    }

    /// The boundary and ordered parts that belong to this generation's private snapshot.
    pub fn form(&self, key: &str) -> Result<(String, serde_json::Value), Refusal> {
        let value = self
            .secrets
            .get(key)
            .map_err(|_| Refusal::Storage)?
            .ok_or(Refusal::Storage)?;
        let mut value: serde_json::Value =
            serde_json::from_str(&value).map_err(|_| Refusal::Storage)?;
        let boundary = value["boundary"]
            .as_str()
            .ok_or(Refusal::Storage)?
            .to_owned();
        Ok((boundary, value["form"].take()))
    }

    /// Called on retirement, after the caller has invalidated this generation.
    pub fn forget(&self, key: &str) -> Result<(), Refusal> {
        self.secrets.forget(key).map_err(|_| Refusal::Storage)
    }

    /// Run one request from byte zero. Errors discovered after enqueueing,
    /// including a denied redirect, are `Failure::Fetch`, never a TypeError.
    /// Progress runs synchronously on the worker; the carrier coalesces public
    /// notifications to at most once a second. No locks are held across it.
    pub fn run(
        &self,
        transfer: Transfer,
        signal: &AbortSignal,
        progress: Arc<dyn Fn(Progress) + Send + Sync>,
    ) -> Result<Completed, Failure> {
        let method = validate_method(&transfer.method, transfer.upload.is_some())
            .map_err(|_| Failure::InvalidRequest)?;
        cancelled(signal)?;
        let value = self
            .secrets
            .get(&transfer.credential_key)
            .map_err(|_| Failure::Storage)?
            .ok_or(Failure::Fetch)?;
        if value.len() > METADATA_BYTES {
            return Err(Failure::Fetch);
        }
        let value: serde_json::Value = serde_json::from_str(&value).map_err(|_| Failure::Fetch)?;
        let url = value["url"].as_str().ok_or(Failure::Fetch)?;
        let headers: Vec<(String, String)> =
            serde_json::from_value(value["headers"].clone()).map_err(|_| Failure::Fetch)?;
        let (mut request, public) = credentials(url, &headers).map_err(|_| Failure::Fetch)?;
        request.method = method;
        // Recheck authority before opening the upload; stream checks it again
        // on each hop, through the very same fetch loop ordinary requests use.
        self.fetch.admit(&request.url).map_err(|_| Failure::Fetch)?;
        if let Some(file) = transfer.upload {
            let meta = file.metadata().map_err(|_| Failure::Storage)?;
            if !meta.is_file() {
                return Err(Failure::Storage);
            }
            request.body_file = Some(BodyFile(Arc::new(Upload {
                file: Arc::new(file),
                len: meta.len(),
                signal: signal.clone(),
            })));
        }
        let mut output = transfer.response;
        let meta = output.metadata().map_err(|_| Failure::Storage)?;
        if !meta.is_file() || meta.len() != 0 {
            return Err(Failure::Storage);
        }
        output.rewind().map_err(|_| Failure::Storage)?;
        let progress = Arc::new(ProgressReporter::new(
            progress,
            request.body_file.as_ref().map(|body| body.0.len()),
        ));
        let _finish = FinishProgress(progress.clone());
        // No buffered-fetch ceiling or total deadline on a file transfer.
        // Socket waits still time out, and the platform's stop aborts the signal.
        request.max_body = Some(usize::MAX);
        request.idle_timeout = Some(Duration::from_secs(60));
        let sent = progress.clone();
        request.sent = Some(Sent(
            Arc::new(move |sent_bytes, total| {
                sent.update(Progress::Upload {
                    sent: sent_bytes,
                    total,
                });
            }),
            None,
        ));
        let mut response = self
            .fetch
            .stream(request, signal)
            .map_err(|error| match error {
                HostError::InvalidArgument(_)
                | HostError::Unavailable { .. }
                | HostError::Denied { .. }
                | HostError::DeniedRedirect { .. }
                | HostError::NotAllowed { .. } => Failure::Fetch,
                HostError::Failed(why)
                    if why == "TypeError: invalid HTTP URL"
                        || why == "TypeError: invalid URL"
                        || why.starts_with("TypeError: invalid redirect target:")
                        || why == "TypeError: Failed to fetch — too many redirects" =>
                {
                    Failure::Fetch
                }
                _ => network_failure(signal),
            })?;
        let mut completed = Completed {
            url: public,
            status: response.status,
            content_type: response
                .headers
                .get("content-type")
                .unwrap_or_default()
                .to_owned(),
            downloaded: 0,
        };
        let mut chunk = [0; CHUNK];
        loop {
            cancelled(signal)?;
            let available = transfer
                .remaining
                .map_or(u64::MAX, |limit| limit - completed.downloaded);
            // Probe only one byte past the allowance, never write that byte.
            let size = available.saturating_add(1).min(CHUNK as u64) as usize;
            let count = response
                .body
                .read(&mut chunk[..size])
                .map_err(|_| network_failure(signal))?;
            if count == 0 {
                break;
            }
            if count as u64 > available {
                return Err(Failure::DownloadTotalExceeded);
            }
            output
                .write_all(&chunk[..count])
                .map_err(|_| Failure::Storage)?;
            completed.downloaded += count as u64;
            progress.update(Progress::Download {
                received: completed.downloaded,
            });
        }
        cancelled(signal)?;
        output.sync_all().map_err(|_| Failure::Storage)?;
        cancelled(signal)?;
        Ok(completed)
    }
}

struct ProgressState {
    last: Option<Instant>,
    upload: Option<Progress>,
    download: Option<Progress>,
}
struct ProgressReporter {
    state: Mutex<ProgressState>,
    tell: Arc<dyn Fn(Progress) + Send + Sync>,
}
impl ProgressReporter {
    fn new(tell: Arc<dyn Fn(Progress) + Send + Sync>, upload: Option<u64>) -> Self {
        Self {
            state: Mutex::new(ProgressState {
                last: None,
                upload: upload.map(|total| Progress::Upload { sent: 0, total }),
                download: Some(Progress::Download { received: 0 }),
            }),
            tell,
        }
    }
    fn update(&self, value: Progress) {
        self.update_at(value, Instant::now());
    }
    fn update_at(&self, value: Progress, now: Instant) {
        let mut state = self.state.lock().unwrap();
        match value {
            Progress::Upload { .. } => state.upload = Some(value),
            Progress::Download { .. } => state.download = Some(value),
        }
        let emit = state
            .last
            .is_none_or(|last| now.duration_since(last) >= Duration::from_secs(1));
        if emit {
            state.last = Some(now);
        }
        drop(state);
        if emit {
            (self.tell)(value);
        }
    }
    fn finish(&self) {
        let state = self.state.lock().unwrap();
        let final_counts = [state.upload, state.download];
        drop(state);
        for value in final_counts.into_iter().flatten() {
            (self.tell)(value);
        }
    }
}
// Flush even if the request fails midway through an upload or response.
struct FinishProgress(Arc<ProgressReporter>);
impl Drop for FinishProgress {
    fn drop(&mut self) {
        self.0.finish();
    }
}

/// Validate and project a URL with the same parser used by the transfer transport.
pub fn validate_url(url: &str) -> Result<String, Refusal> {
    credentials(url, &[]).map(|(_, public)| public)
}

fn credentials(url: &str, headers: &[(String, String)]) -> Result<(Request, String), Refusal> {
    if url.len() > METADATA_BYTES || headers.len() > HEADER_COUNT {
        return Err(Refusal::InvalidRequest);
    }
    let mut parsed = url::Url::parse(url).map_err(|_| Refusal::InvalidRequest)?;
    if !matches!(parsed.scheme(), "http" | "https")
        || parsed.host_str().is_none()
        || !parsed.username().is_empty()
        || parsed.password().is_some()
    {
        return Err(Refusal::InvalidRequest);
    }
    let mut request = Request::get(parsed.as_str());
    let mut bytes = url.len();
    for (name, value) in headers {
        bytes = bytes.saturating_add(name.len()).saturating_add(value.len());
        if bytes > METADATA_BYTES
            || !fetch::is_valid_name(name)
            || !fetch::is_valid_value(value)
            || value.bytes().any(|b| (b < 0x20 && b != b'\t') || b == 0x7f)
        {
            return Err(Refusal::InvalidRequest);
        }
        request.headers.append(name, value);
    }
    parsed.set_query(None);
    parsed.set_fragment(None);
    Ok((request, parsed.to_string()))
}

/// Validate before scheduling; the returned method is the form sent on the wire.
pub fn validate_method(value: &str, has_body: bool) -> Result<String, Refusal> {
    if !fetch::is_valid_name(value) || value.len() > 64 {
        return Err(Refusal::InvalidRequest);
    }
    let upper = value.to_ascii_uppercase();
    match upper.as_str() {
        "CONNECT" | "TRACE" | "TRACK" => Err(Refusal::InvalidRequest),
        "GET" | "HEAD" if has_body => Err(Refusal::InvalidRequest),
        _ => Ok(upper),
    }
}

fn refusal(error: HostError) -> Refusal {
    match error {
        HostError::Denied { .. }
        | HostError::DeniedRedirect { .. }
        | HostError::NotAllowed { .. } => Refusal::NotAllowed,
        _ => Refusal::InvalidRequest,
    }
}

fn cancelled(signal: &AbortSignal) -> Result<(), Failure> {
    if signal.aborted() {
        Err(Failure::Cancelled)
    } else {
        Ok(())
    }
}

fn network_failure(signal: &AbortSignal) -> Failure {
    if signal.aborted() {
        Failure::Cancelled
    } else {
        Failure::Network
    }
}

struct Upload {
    file: Arc<File>,
    len: u64,
    signal: AbortSignal,
}

impl FileBody for Upload {
    fn len(&self) -> u64 {
        self.len
    }
    fn reader(&self) -> std::io::Result<Box<dyn std::io::Read + Send>> {
        Ok(Box::new(Reader {
            file: self.file.clone(),
            len: self.len,
            at: 0,
            signal: self.signal.clone(),
        }))
    }
}

struct Reader {
    file: Arc<File>,
    len: u64,
    at: u64,
    signal: AbortSignal,
}

impl std::io::Read for Reader {
    fn read(&mut self, out: &mut [u8]) -> std::io::Result<usize> {
        if self.signal.aborted() {
            return Err(std::io::ErrorKind::Interrupted.into());
        }
        let size = (self.len - self.at).min(out.len() as u64).min(CHUNK as u64) as usize;
        if size == 0 {
            return Ok(0);
        }
        let count = super::body::read_at(&self.file, &mut out[..size], self.at)?;
        if count == 0 {
            return Err(std::io::ErrorKind::UnexpectedEof.into());
        }
        self.at += count as u64;
        Ok(count)
    }
}

#[cfg(test)]
#[path = "transfer_tests.rs"]
mod tests;
