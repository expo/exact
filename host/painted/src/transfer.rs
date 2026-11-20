//! Import-selected Android transfer bridge (LLP 1133.017 D3, D4, D18).
//! No exported entry point exists until the app expands `android_transfer_jni!`.
use exact_executor::transfer::{Carrier, Failure, Progress, Refusal, Transfer};
use ibex2::{
    boundary::HostError, grant::GrantSet, secrets::SecretStore, stdlib::abort::AbortController,
};
use serde_json::{json, Value};
use std::{
    collections::HashMap,
    fs::File,
    path::PathBuf,
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Mutex, OnceLock,
    },
};

#[path = "transfer_files.rs"]
mod files;

#[cfg(target_os = "android")]
#[path = "transfer_jni.rs"]
mod jni;
#[cfg(target_os = "android")]
pub use jni::call;

struct Attempt {
    abort: AbortController,
    uploaded: AtomicU64,
    downloaded: AtomicU64,
}
#[derive(Default)]
struct Attempts {
    next: AtomicU64,
    live: Mutex<HashMap<u64, Arc<Attempt>>>,
}
impl Attempts {
    fn open(&self) -> u64 {
        let id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        self.live.lock().unwrap().insert(
            id,
            Arc::new(Attempt {
                abort: AbortController::new(),
                uploaded: AtomicU64::new(0),
                downloaded: AtomicU64::new(0),
            }),
        );
        id
    }
    fn get(&self, id: u64) -> Result<Arc<Attempt>, &'static str> {
        self.live
            .lock()
            .unwrap()
            .get(&id)
            .cloned()
            .ok_or("AbortError")
    }
    fn cancel(&self, id: u64, remove: bool) {
        let attempt = if remove {
            self.live.lock().unwrap().remove(&id)
        } else {
            self.live.lock().unwrap().get(&id).cloned()
        };
        if let Some(attempt) = attempt {
            attempt.abort.abort();
        }
    }
}
static ATTEMPTS: OnceLock<Attempts> = OnceLock::new();

struct Secrets(Arc<dyn exact_data::secrets::Backend>);
impl SecretStore for Secrets {
    fn get(&self, name: &str) -> Result<Option<String>, HostError> {
        self.0
            .read(name)
            .map_err(|_| HostError::Failed("secret storage unavailable".into()))
    }
    fn set(&self, name: &str, value: &str) -> Result<(), HostError> {
        self.0
            .write(name, Some(value))
            .map_err(|_| HostError::Failed("secret storage unavailable".into()))
    }
    fn forget(&self, name: &str) -> Result<(), HostError> {
        self.0
            .write(name, None)
            .map_err(|_| HostError::Failed("secret storage unavailable".into()))
    }
}
struct Ceiling(ibex2::host::Fetch);
impl ibex2::boundary::HopGate for Ceiling {
    fn admit(&self, url: &url::Url) -> Result<(), HostError> {
        self.0.admit(url.as_str())
    }
}
fn grants(text: &str) -> Result<GrantSet, &'static str> {
    GrantSet::parse(&exact_runner::io_grants(text)).map_err(|_| "NotAllowedError")
}
fn string<'a>(v: &'a Value, name: &str) -> Result<&'a str, &'static str> {
    v[name].as_str().ok_or("TypeError")
}
fn refusal(e: Refusal) -> &'static str {
    match e {
        Refusal::InvalidRequest => "TypeError",
        Refusal::NotAllowed => "NotAllowedError",
        Refusal::Storage => "UnknownError",
    }
}
fn carrier(v: &Value, ceiling: &str) -> Result<Carrier, &'static str> {
    let host =
        ibex2::host::Host::with_transport(Box::new(ibex2::transport::RustlsHttpTransport::new()));
    let fetch = host.endow(grants(string(v, "authority")?)?).fetch;
    let fetch = fetch.with_gate(Arc::new(Ceiling(host.endow(grants(ceiling)?).fetch)));
    let origin = match string(v, "origin")? {
        "app" => exact_runner::CallOrigin::App,
        "tool" => exact_runner::CallOrigin::Tool,
        "wake" => exact_runner::CallOrigin::Wake,
        _ => return Err("NotAllowedError"),
    };
    // A background retry must never turn a non-app origin into a prompt.
    // Android has no local-network prompt; other hosts never call this bridge.
    if origin != exact_runner::CallOrigin::App && exact_runner::local_network::prompts() {
        return Err("NotAllowedError");
    }
    let root = PathBuf::from(string(v, "data")?);
    let root = root.parent().ok_or("TypeError")?;
    Ok(Carrier::new(
        fetch,
        Arc::new(Secrets(crate::host::system::secret_backend(root))),
    ))
}
fn private_file(path: &str, write: bool) -> Result<File, &'static str> {
    use std::os::unix::fs::OpenOptionsExt;
    let mut options = std::fs::OpenOptions::new();
    options
        .read(!write)
        .write(write)
        .custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    let file = options.open(path).map_err(|_| "UnknownError")?;
    if !file.metadata().map_err(|_| "UnknownError")?.is_file() {
        return Err("TypeError");
    }
    Ok(file)
}
/// Run a package operation with the current binary's grant ceiling.
#[doc(hidden)]
pub fn dispatch(v: &Value, ceiling: &str) -> Result<Value, &'static str> {
    let attempts = ATTEMPTS.get_or_init(Attempts::default);
    let id = v["handle"].as_u64().unwrap_or(0);
    match string(v, "op")? {
        "snapshot" | "checkSave" | "place" | "removePlaced" => files::dispatch(v, ceiling),
        "open" => Ok(json!({"handle": attempts.open()})),
        "cancel" | "close" => {
            attempts.cancel(id, v["op"] == "close");
            Ok(json!({}))
        }
        "progress" => {
            let a = attempts.get(id)?;
            Ok(
                json!({"uploaded": a.uploaded.load(Ordering::Relaxed), "downloaded": a.downloaded.load(Ordering::Relaxed)}),
            )
        }
        "validate" => Ok(
            json!({"url": exact_executor::transfer::validate_url(string(v, "url")?).map_err(refusal)?}),
        ),
        "keep" => {
            let headers = v["headers"]
                .as_array()
                .ok_or("TypeError")?
                .iter()
                .map(|h| {
                    Ok((
                        string(h, "name")?.to_owned(),
                        string(h, "value")?.to_owned(),
                    ))
                })
                .collect::<Result<Vec<_>, &'static str>>()?;
            let method = exact_executor::transfer::validate_method(
                string(v, "method")?,
                v["body"].as_bool() == Some(true),
            )
            .map_err(refusal)?;
            let form = v.get("form").filter(|f| !f.is_null());
            if let Some(form) = form {
                files::parts(form)?;
            }
            let url = carrier(v, ceiling)?
                .keep_with_form(string(v, "key")?, string(v, "url")?, &headers, form)
                .map_err(refusal)?;
            Ok(json!({"url": url, "method": method}))
        }
        "forget" => {
            carrier(v, ceiling)?
                .forget(string(v, "key")?)
                .map_err(refusal)?;
            Ok(json!({}))
        }
        "run" => {
            let a = attempts.get(id)?;
            let upload = v["upload"]
                .as_str()
                .map(|p| private_file(p, false))
                .transpose()?;
            let transfer = Transfer {
                method: string(v, "method")?.into(),
                credential_key: string(v, "key")?.into(),
                upload,
                response: private_file(string(v, "response")?, true)?,
                remaining: v["remaining"].as_u64(),
            };
            let progress = a.clone();
            let result = carrier(v, ceiling)?.run(
                transfer,
                &a.abort.signal(),
                Arc::new(move |p| match p {
                    Progress::Upload { sent, .. } => {
                        progress.uploaded.store(sent, Ordering::Relaxed)
                    }
                    Progress::Download { received } => {
                        progress.downloaded.store(received, Ordering::Relaxed)
                    }
                }),
            );
            Ok(match result {
                Ok(done) => {
                    json!({"status": done.status, "contentType": done.content_type, "downloaded": done.downloaded})
                }
                Err(error) => json!({"failure": match error {
                    Failure::Cancelled => "cancelled", Failure::DownloadTotalExceeded => "download-total-exceeded",
                    Failure::Storage => "storage",
                    Failure::Network => "network", Failure::Fetch | Failure::InvalidRequest => "fetch-error",
                }}),
            })
        }
        _ => Err("TypeError"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn closing_a_handle_cancels_in_flight_io_and_never_reuses_it() {
        let handles = Attempts::default();
        let id = handles.open();
        let running = handles.get(id).unwrap();
        let (send, receive) = std::sync::mpsc::channel();
        let _registration = running.abort.signal().register(move || {
            send.send(()).unwrap();
        });
        handles.cancel(id, true);
        receive
            .recv_timeout(std::time::Duration::from_secs(1))
            .unwrap();
        assert!(running.abort.signal().aborted());
        assert!(handles.get(id).is_err());
        assert_ne!(id, handles.open());
        handles.cancel(id, true);
    }
    #[test]
    fn android_url_admission_uses_fetch_normalization() {
        for (url, public) in [
            (
                "https://bücher.test/file?token=private#hidden",
                "https://xn--bcher-kva.test/file",
            ),
            ("HTTPS://example.test/file", "https://example.test/file"),
            (
                "http://upload_server.example/file",
                "http://upload_server.example/file",
            ),
        ] {
            assert_eq!(
                dispatch(&json!({"op":"validate", "url":url}), "").unwrap()["url"],
                public
            );
        }
        assert!(dispatch(
            &json!({"op":"validate", "url":"https://user:pass@example.test"}),
            ""
        )
        .is_err());
    }
    #[test]
    fn original_authority_and_current_ceiling_both_admit_a_retry() {
        let host = ibex2::host::Host::new();
        let fetch = host
            .endow(
                grants("net.fetch https://original.test\nnet.fetch https://shared.test").unwrap(),
            )
            .fetch
            .with_gate(Arc::new(Ceiling(
                host.endow(
                    grants("net.fetch https://current.test\nnet.fetch https://shared.test")
                        .unwrap(),
                )
                .fetch,
            )));
        assert!(fetch.admit("https://original.test/a").is_err());
        assert!(fetch.admit("https://current.test/a").is_err());
        assert!(fetch.admit("https://shared.test/a").is_ok());
    }
}
