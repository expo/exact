//! A 2D canvas's kept bitmap written as a PNG file (LLP 1056 D9, as amended
//! for expo/exact#188): `captureCanvas(surface, path)` in TypeScript,
//! [`Request::capture_canvas`] in Rust.
//!
//! @ref LLP 1056 D9 (readback stays refused; the capture is the asynchronous
//! form) / LLP 1133.032 D7 (`captureImage`'s shape: a path, its size) /
//! LLP 1133.009 D10 item 4 (a path to write, under `fs.write`)
//!
//! The capture is presenter work: the host that keeps the canvas's bitmap
//! encodes what it has drawn and kept (every list applied, its images and
//! text as drawn) as a straight-alpha sRGB PNG at device pixels, and
//! writes it atomically to an `app:/` path under the source's `fs.write`.
//! The answer is `{path, width, height, scale}`; no pixel crosses the
//! source. A capture takes a completed drawing at or after its requesting
//! commit. Later drawings can satisfy a pending job; unmount or replacement
//! retires it. A ticket no longer held writes nothing.
//! A refusal is a DOM exception's name and a message:
//!
//! | Name | When |
//! |---|---|
//! | `NotFoundError` | no canvas of the surface is mounted |
//! | `InvalidStateError` | several are, or its size was refused (`state`) |
//! | `IndexSizeError` | it has no bitmap: a zero-sized box, or not laid out yet |
//! | `QuotaExceededError` | over [`MAX_PIXELS`] |
//! | `NotSupportedError` | a `display-p3` or `float16` canvas, or a host that cannot |
//! | `NotAllowedError` | `path` is not an `app:/` file under `fs.write` |
//! | `EncodingError` | the canvas was unmounted or replaced, its drawing was rejected, or encoding or writing failed |

use crate::request::{Outcome, Request, Response, SurfaceRequest};
use exact_kernel::ViewId;
use std::sync::{Arc, Mutex, Weak};

/// The URL a TypeScript source's `captureCanvas` fetches with its
/// arguments as a JSON body; the runner makes it surface work when the
/// request is enqueued, so no host sees it as HTTP.
pub const CANVAS_URL: &str = "exact-canvas:";

/// The most pixels a capture writes: 8,192², the pixels `compressImage`
/// decodes at most, and iOS's whole canvas area (LLP 1056 D4). Its RGBA
/// copy is 256 MiB before encoding.
pub const MAX_PIXELS: u64 = 1 << 26;

/// The longest `path`, in bytes, as `exactBodyTo` takes.
pub const MAX_PATH: usize = 4096;

/// What a capture answers: where the PNG is, and its size in device pixels
/// with the device pixels per CSS px it was drawn at (1 for a canvas with
/// an explicit bitmap size).
#[derive(Debug, Clone, PartialEq)]
pub struct CapturedImage {
    /// The `app:/` path written.
    pub path: String,
    /// Width, px.
    pub width: u32,
    /// Height, px.
    pub height: u32,
    /// Device pixels per CSS px.
    pub scale: f64,
}

/// The canvas a capture names, as the runner resolved it: the host encodes
/// this view's kept bitmap, which is `width` × `height` px in generation
/// `generation` of `lifetime`, after the requesting turn's drawing has run.
/// A generation whose draw is still owed must wait; a completed empty draw
/// keeps the bitmap, transparent if nothing has painted it.
#[derive(Debug, Clone, PartialEq)]
pub struct CanvasCapture {
    /// The canvas node.
    pub view: ViewId,
    /// Its lifetime.
    pub lifetime: u64,
    /// Its size generation.
    pub generation: u32,
    /// The completed drawing within this lifetime.
    pub sequence: u64,
    /// The pending job, absent for an inspection without a request.
    pub job: Option<CaptureJob>,
    /// The `app:/` path to write.
    pub path: String,
    /// The backing width, px.
    pub width: u32,
    /// The backing height, px.
    pub height: u32,
    /// Device pixels per CSS px.
    pub scale: f64,
}

impl CanvasCapture {
    /// The answer once the PNG is at `path`.
    pub fn captured(&self) -> Outcome {
        if let Some(job) = &self.job {
            let mut state = job.0.lock().unwrap();
            if !matches!(*state, CaptureState::Snapshotted) {
                return boundary_lost();
            }
            *state = CaptureState::Answered;
        }
        let mut body = String::from("{\"path\":");
        crate::agent::quote(&self.path, &mut body);
        body.push_str(&format!(
            ",\"width\":{},\"height\":{},\"scale\":{}}}",
            self.width,
            self.height,
            crate::agent::num(self.scale)
        ));
        json(200, body)
    }
}

/// pending → snapshotted → answered, or retired on unmount, replacement or
/// session end. Later commits and frames update the pending drawing. The host
/// copies pixels after replay, then marks the job snapshotted before publication.
#[derive(Debug, Clone)]
pub struct CaptureJob(Arc<Mutex<CaptureState>>, Weak<()>);

#[derive(Debug)]
pub(crate) enum CaptureState {
    Pending {
        target: Option<(ViewId, u64, u64)>,
        result: Option<Result<CanvasCapture, Outcome>>,
    },
    Snapshotted,
    Answered,
    Retired,
}

impl PartialEq for CaptureJob {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for CaptureJob {}

impl CaptureJob {
    pub(crate) fn new(
        target: Option<(ViewId, u64, u64)>,
        result: Option<Result<CanvasCapture, Outcome>>,
        owner: &Arc<()>,
    ) -> Self {
        Self(
            Arc::new(Mutex::new(CaptureState::Pending { target, result })),
            Arc::downgrade(owner),
        )
    }
    pub(crate) fn weak(&self) -> Weak<Mutex<CaptureState>> {
        Arc::downgrade(&self.0)
    }
    pub(crate) fn fail(&self) {
        self.0.lock().unwrap().complete(Err(refused(
            "EncodingError",
            "the capture's drawing could not be completed",
        )));
    }
    fn pending(&self) -> bool {
        self.1.strong_count() > 0
            && matches!(
                *self.0.lock().unwrap(),
                CaptureState::Pending { result: None, .. }
            )
    }
    fn take(&self) -> Result<CanvasCapture, Outcome> {
        let mut state = self.0.lock().unwrap();
        if self.1.strong_count() == 0 {
            *state = CaptureState::Retired;
        }
        match &mut *state {
            CaptureState::Pending {
                result: Some(result),
                ..
            } => {
                let result = result.clone();
                match result {
                    Ok(mut c) => {
                        c.job = Some(self.clone());
                        Ok(c)
                    }
                    Err(e) => {
                        *state = CaptureState::Answered;
                        Err(e)
                    }
                }
            }
            _ => Err(boundary_lost()),
        }
    }
}

fn boundary_lost() -> Outcome {
    refused(
        "EncodingError",
        "the capture's canvas is no longer mounted in this session",
    )
}

impl CanvasCapture {
    /// The host has copied this sequence's pixels, before encoding or publication.
    pub fn snapshotted(&self) -> Result<(), Outcome> {
        let Some(job) = &self.job else { return Ok(()) };
        let mut state = job.0.lock().unwrap();
        if job.1.strong_count() > 0
            && matches!(*state, CaptureState::Pending { target: Some((view, lifetime, first)), .. }
                if view == self.view && lifetime == self.lifetime && self.sequence >= first)
        {
            *state = CaptureState::Snapshotted;
            Ok(())
        } else {
            Err(boundary_lost())
        }
    }
}

impl CaptureState {
    pub(crate) fn complete(&mut self, result: Result<CanvasCapture, Outcome>) {
        if let Self::Pending {
            target,
            result: slot,
        } = self
        {
            if target.is_none() && slot.is_some() {
                return;
            }
            *slot = match result {
                Ok(c)
                    if target.is_some_and(|(v, l, first)| {
                        c.view == v && c.lifetime == l && c.sequence >= first
                    }) =>
                {
                    Some(Ok(c))
                }
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            };
        }
    }
    pub(crate) fn unmount(&mut self, alive: impl Fn(ViewId, u64) -> bool) {
        if matches!(self, Self::Pending { target: Some((view, lifetime, _)), .. } if !alive(*view, *lifetime))
        {
            *self = Self::Retired;
        }
    }
}

impl Request {
    /// A canvas request with no completed drawing since its requesting commit.
    pub fn canvas_capture_pending(&self) -> bool {
        matches!(self.surface.as_deref(), Some(SurfaceRequest::Canvas { job: Some(job), .. }) if job.pending())
    }

    /// Read the latest completed drawing after applying geometry and draws.
    /// Taking leaves the job pending until the host copies its pixels.
    pub fn take_canvas_capture(&self) -> Option<Result<CanvasCapture, Outcome>> {
        let SurfaceRequest::Canvas { job, .. } = self.surface.as_deref()? else {
            return None;
        };
        Some(
            job.as_ref()
                .map_or_else(|| Err(boundary_lost()), CaptureJob::take),
        )
    }

    /// Capture the kept bitmap of the one mounted 2D canvas of `surface` as
    /// a PNG at the `app:/` file `path` (see the module).
    pub fn capture_canvas(surface: impl Into<String>, path: impl Into<String>) -> Request {
        Request {
            surface: Some(Box::new(SurfaceRequest::Canvas {
                name: surface.into(),
                path: path.into(),
                job: None,
            })),
            ..Request::get("")
        }
    }
}

/// A TypeScript source's `fetch(CANVAS_URL, …)` as the surface work it
/// stands for; any other request is left as it is. A body that is not
/// `{"surface", "path"}` strings names an empty surface, which is refused.
pub(crate) fn from_fetch(request: &mut Request) {
    if request.url != CANVAS_URL
        || request.surface.is_some()
        || request.storage.is_some()
        || request.continuation.is_some()
    {
        return;
    }
    let body: serde_json::Value = serde_json::from_slice(&request.body).unwrap_or_default();
    let text = |k: &str| body[k].as_str().unwrap_or_default().to_string();
    let converted = Request::capture_canvas(text("surface"), text("path"));
    *request = Request {
        grants: request.grants.take(),
        fetch: request.fetch,
        due_ms: request.due_ms,
        wake_ms: request.wake_ms,
        ..converted
    };
}

/// Why `path` cannot be written, before any host looks at the canvas.
pub(crate) fn path_refusal(path: &str) -> Option<String> {
    let mut parts = path
        .strip_prefix("app:/")
        .unwrap_or("")
        .split('/')
        .filter(|p| !p.is_empty());
    let root = matches!(parts.next(), Some("data" | "cache" | "tmp"));
    let names: Vec<_> = parts.collect();
    let file = root
        && !names.is_empty()
        && !path.ends_with('/')
        && names
            .iter()
            .all(|p| !matches!(*p, "." | "..") && !p.contains('\0'));
    if path.len() > MAX_PATH {
        Some(format!("the path is longer than {MAX_PATH} bytes"))
    } else if !file {
        Some(format!("{path:?} is not an app:/ file"))
    } else {
        None
    }
}

/// A refusal: `name` a DOM exception's, `message` the reason.
pub fn refused(name: &str, message: &str) -> Outcome {
    let mut body = String::from("{\"name\":");
    crate::agent::quote(name, &mut body);
    body.push_str(",\"message\":");
    crate::agent::quote(message, &mut body);
    body.push('}');
    json(400, body)
}

fn json(status: u16, body: String) -> Outcome {
    Outcome::Response(Response {
        status,
        headers: vec![("content-type".into(), "application/json".into())],
        body: body.into_bytes(),
    })
}

/// A Rust source's reading of a capture's outcome: the image, or the
/// refusal as `"<DOM exception name>: <message>"`.
pub fn captured_image(outcome: &Outcome) -> Result<CapturedImage, String> {
    let Outcome::Response(r) = outcome else {
        return Err(format!("EncodingError: {}", outcome.summary()));
    };
    let json: serde_json::Value = serde_json::from_slice(&r.body).unwrap_or_default();
    if r.status != 200 {
        let name = json["name"].as_str().unwrap_or("EncodingError");
        return Err(format!(
            "{name}: {}",
            json["message"].as_str().unwrap_or("")
        ));
    }
    let size = |k: &str| json[k].as_u64().and_then(|n| u32::try_from(n).ok());
    match (
        json["path"].as_str(),
        size("width"),
        size("height"),
        json["scale"].as_f64(),
    ) {
        (Some(path), Some(width), Some(height), Some(scale)) => Ok(CapturedImage {
            path: path.into(),
            width,
            height,
            scale,
        }),
        _ => Err("EncodingError: the host's answer is not {path, width, height, scale}".into()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typescript_fetch_becomes_surface_work_and_its_answer_reads_back() {
        let body = r#"{"surface":"sketch","path":"app:/tmp/a.png"}"#;
        let mut fetched = Request::post_json(CANVAS_URL, body);
        fetched.grants = Some("fs.write app:/tmp".into());
        from_fetch(&mut fetched);
        let mut built = Request::capture_canvas("sketch", "app:/tmp/a.png");
        built.grants = Some("fs.write app:/tmp".into());
        assert_eq!(fetched, built);
        let mut other = Request::post_json("https://x.test", body);
        from_fetch(&mut other);
        assert_eq!(other.url, "https://x.test");

        let capture = CanvasCapture {
            view: 3,
            lifetime: 1,
            generation: 0,
            sequence: 1,
            job: None,
            path: "app:/tmp/a.png".into(),
            width: 40,
            height: 20,
            scale: 2.0,
        };
        assert_eq!(
            captured_image(&capture.captured()),
            Ok(CapturedImage {
                path: "app:/tmp/a.png".into(),
                width: 40,
                height: 20,
                scale: 2.0
            })
        );
        assert_eq!(
            captured_image(&refused("NotFoundError", "no canvas")),
            Err("NotFoundError: no canvas".into())
        );
    }

    #[test]
    fn conversion_keeps_the_fetch_and_its_continuation_times() {
        let mut request = Request::post_json(
            CANVAS_URL,
            r#"{"surface":"sketch","path":"app:/tmp/a.png"}"#,
        );
        request.fetch = Some(42);
        request.due_ms = Some(100);
        request.wake_ms = Some(200);
        from_fetch(&mut request);
        assert_eq!(request.fetch, Some(42));
        assert_eq!(request.due_ms, Some(100));
        assert_eq!(request.wake_ms, Some(200));
    }

    #[test]
    fn a_path_is_an_app_file() {
        let vectors: Vec<(String, bool)> =
            serde_json::from_str(include_str!("../tests/fixtures/canvas-capture-paths.json"))
                .unwrap();
        for (path, valid) in vectors {
            assert_eq!(path_refusal(&path).is_none(), valid, "{path:?}");
        }
        for (bytes, valid) in [(4096, true), (4097, false)] {
            let path = format!("app:/tmp/{}", "a".repeat(bytes - 9));
            assert_eq!(path_refusal(&path).is_none(), valid);
        }
    }
}
