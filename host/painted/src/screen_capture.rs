//! Still files and the agent's display cards (LLP 1178 D3, D6).
//! The platform supplies pixels; publication and cancellation stay the same.

use crate::package::{json, Context, Json, Package, Reply};
use exact_runner::canvas_capture::Encoding;
use ibex2::grant::{Grant, GrantSet, PathPrefix};
use ibex2::stdlib::{app_fs::AppDirectories, fs};
use std::path::PathBuf;
use std::sync::{Arc, Mutex, OnceLock, Weak};

/// A capture refusal, by DOMException name.
pub type Failure = (&'static str, String);

/// One display snapshotted at the call, before any capture starts.
pub struct Display {
    /// Its `ScreenDetailed` and `clientRect`.
    pub screen: Json,
    /// Physical pixel width.
    pub width: u32,
    /// Physical pixel height.
    pub height: u32,
    /// Read one opaque, top-down RGBA8 frame, without showing a window.
    pub read: Box<dyn FnOnce() -> Result<Vec<u8>, Failure> + Send>,
}

/// Validate all sizes before allocating any bitmap (LLP 1178 D3).
pub fn sizes(sizes: impl IntoIterator<Item = (u32, u32)>) -> Result<(), Failure> {
    let mut total = 0u64;
    for (width, height) in sizes {
        let pixels = u64::from(width) * u64::from(height);
        total = total.saturating_add(pixels);
        if pixels == 0 || pixels > exact_runner::canvas_capture::MAX_PIXELS || total > 1 << 27 {
            return Err((
                "QuotaExceededError",
                "capture exceeds the pixel limit".into(),
            ));
        }
    }
    Ok(())
}

fn failed(error: impl std::fmt::Display) -> Failure {
    ("EncodingError", error.to_string())
}

fn file_error(error: ibex2::boundary::HostError) -> Failure {
    match error {
        ibex2::boundary::HostError::Denied { .. } => ("NotAllowedError", error.to_string()),
        _ => failed(error),
    }
}

type Job = Box<dyn FnOnce() + Send>;
type Pending = (u64, Weak<Mutex<bool>>);

/// The session's pending captures. Work is queued in call order; each job
/// captures and encodes its displays together, off the presentation thread.
#[derive(Default)]
pub struct Captures {
    pending: Vec<Pending>,
}

impl Captures {
    /// Capture `displays` into the admitted directory in `args`.
    pub fn start(&mut self, context: &Context, args: &Json, displays: Vec<Display>, reply: Reply) {
        if displays.is_empty() {
            return reply.error("EncodingError", Some("no display"));
        }
        if let Err((code, why)) = sizes(displays.iter().map(|d| (d.width, d.height))) {
            return reply.error(code, Some(&why));
        }
        let directory = args["directory"]
            .as_str()
            .unwrap_or_default()
            .trim_end_matches('/')
            .to_owned();
        if !directory.starts_with("app:/") || directory.contains(['\n', '\r', '\0']) {
            return reply.error("NotAllowedError", Some("capture needs an app:/ directory"));
        }
        let encoding = Encoding::new(
            args["type"].as_str().unwrap_or_default(),
            args["quality"].as_f64(),
        );
        let roots = [context.data(), context.cache(), context.temporary()];
        let held = Arc::new(Mutex::new(true));
        self.pending.retain(|(_, p)| p.strong_count() != 0);
        self.pending
            .push((context.request().unwrap_or(0), Arc::downgrade(&held)));
        static WORKER: OnceLock<std::sync::mpsc::Sender<Job>> = OnceLock::new();
        let worker = WORKER.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<Job>();
            std::thread::spawn(move || {
                for job in rx {
                    job();
                }
            });
            tx
        });
        let notifier = context.notifier();
        let job = Box::new(move || {
            let result = write(&roots, &directory, displays, &encoding, &held, &|path| {
                notifier.picture_changed(path)
            });
            match result {
                Ok(answer) => reply.send(answer),
                Err((code, why)) => reply.error(code, Some(&why)),
            }
        });
        // The worker lives for the process and catches no fallible platform
        // operation by panic: every capture and encoder answers a Result.
        let _ = worker.send(job);
    }

    /// A send no longer has a reader.
    pub fn cancel(&mut self, request: u64) {
        for (id, pending) in &self.pending {
            if *id == request {
                if let Some(held) = pending.upgrade() {
                    *held.lock().unwrap() = false;
                }
            }
        }
    }
}

impl Drop for Captures {
    fn drop(&mut self) {
        for (_, pending) in &self.pending {
            if let Some(held) = pending.upgrade() {
                *held.lock().unwrap() = false;
            }
        }
    }
}

fn write(
    roots: &[PathBuf; 3],
    directory: &str,
    displays: Vec<Display>,
    encoding: &Encoding,
    held: &Mutex<bool>,
    published: &dyn Fn(&str),
) -> Result<Json, Failure> {
    if !*held.lock().unwrap() {
        return Err(("AbortError", "capture cancelled".into()));
    }
    let ext = if encoding.media_type == "image/jpeg" {
        "jpg"
    } else {
        "png"
    };
    let mut random = [0u8; 16];
    getrandom::fill(&mut random).map_err(failed)?;
    let capture = u128::from_le_bytes(random);
    let paths: Vec<_> = (0..displays.len())
        .map(|index| format!("{directory}/screen-{capture:032x}-{index}.{ext}"))
        .collect();
    if paths
        .iter()
        .any(|p| crate::picker::place_from(p, roots.clone()).is_none())
    {
        return Err(("NotAllowedError", "capture path is not writable".into()));
    }
    for root in roots {
        std::fs::create_dir_all(root).map_err(failed)?;
    }
    let directories = AppDirectories::new(&roots[0], &roots[1], &roots[2]).map_err(file_error)?;
    // The runner admitted this directory against the caller's fs.write
    // before dispatch. Each child is resolved again through held directories.
    let prefix = PathPrefix::new(directory)
        .ok_or_else(|| ("NotAllowedError", "capture path is not writable".into()))?;
    let grants = GrantSet::none().with(Grant::FsWrite(prefix));
    fs::run(
        &grants,
        Some(&directories),
        fs::FsOp::Mkdir,
        directory,
        None,
        None,
    )
    .map_err(file_error)?;
    let staged = std::thread::scope(|scope| {
        let jobs: Vec<_> = displays
            .into_iter()
            .zip(paths)
            .map(|(display, path)| {
                let (directories, grants) = (&directories, &grants);
                scope.spawn(move || encode_file(directories, grants, &path, display, encoding))
            })
            .collect();
        jobs.into_iter()
            .map(|job| {
                job.join()
                    .unwrap_or_else(|_| Err(failed("capture worker failed")))
            })
            .collect::<Result<Vec<_>, Failure>>()
    })?;
    let held = held.lock().unwrap();
    if !*held {
        return Err(("AbortError", "capture cancelled".into()));
    }
    let mut captures = Vec::with_capacity(staged.len());
    for (file, answer) in staged {
        file.publish().map_err(file_error)?;
        published(answer["path"].as_str().unwrap());
        captures.push(answer);
    }
    Ok(json!({"captures": captures}))
}

fn encode_file(
    directories: &AppDirectories,
    grants: &ibex2::grant::GrantSet,
    path: &str,
    display: Display,
    encoding: &Encoding,
) -> Result<(ibex2::stdlib::app_fs::Staged, Json), Failure> {
    let bytes = (display.read)()?;
    let size = tiny_skia::IntSize::from_wh(display.width, display.height)
        .ok_or_else(|| failed("empty display"))?;
    let pixels = tiny_skia::Pixmap::from_vec(bytes, size)
        .ok_or_else(|| failed("capture returned the wrong pixel count"))?;
    let (bytes, media_type) = encode(&pixels, encoding)?;
    let mut file = directories.stage(grants, path).map_err(file_error)?;
    file.write_all(&bytes).map_err(file_error)?;
    let answer = json!({
        "path": path, "width": display.width, "height": display.height,
        "type": media_type, "screen": display.screen,
    });
    Ok((file, answer))
}

fn encode(
    pixels: &tiny_skia::Pixmap,
    encoding: &Encoding,
) -> Result<(Vec<u8>, &'static str), Failure> {
    if encoding.media_type == "image/jpeg" {
        return crate::canvas2d::capture::encode(pixels, encoding).map_err(failed);
    }
    crate::canvas2d::capture::png(pixels.width(), pixels.height(), pixels.data())
        .map(|out| (out, "image/png"))
        .map_err(failed)
}

/// Fixture screens, with no native screen read. A drive can set `screens`
/// to the same rectangles it prefers for `getScreenDetails`.
pub fn fixtures(preferred: &Json) -> Result<Vec<Display>, Failure> {
    let default = json!([{"left":0,"top":0,"width":1440,"height":900,
        "availLeft":0,"availTop":0,"availWidth":1440,"availHeight":900,
        "devicePixelRatio":2,"isPrimary":true,"isInternal":true,"label":"Agent display",
        "clientRect":{"x":0,"y":0,"width":1440,"height":900}}]);
    let mut screens = preferred.as_array().or(default.as_array()).unwrap().clone();
    screens.sort_by(|a, b| {
        let coord = |v: &Json, k: &str| v[k].as_f64().unwrap_or(0.);
        coord(a, "left")
            .total_cmp(&coord(b, "left"))
            .then_with(|| coord(a, "top").total_cmp(&coord(b, "top")))
    });
    let dimensions: Vec<_> = screens
        .iter()
        .map(|s| {
            let scale = s["devicePixelRatio"].as_f64().unwrap_or(1.);
            let dimension = |k: &str| (s[k].as_f64().unwrap_or(0.) * scale).round() as u32;
            (dimension("width"), dimension("height"))
        })
        .collect();
    sizes(dimensions.iter().copied())?;
    Ok(screens
        .into_iter()
        .zip(dimensions)
        .enumerate()
        .map(|(index, (mut screen, (width, height)))| {
            if screen.get("clientRect").is_none() {
                screen["clientRect"] = Json::Null;
            }
            Display {
                screen,
                width,
                height,
                read: Box::new(move || Ok(card(index, width, height))),
            }
        })
        .collect())
}

fn card(index: usize, width: u32, height: u32) -> Vec<u8> {
    let mut pixels = vec![0; width as usize * height as usize * 4];
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.copy_from_slice(&[32 + (index % 6) as u8 * 30, 64, 128, 255]);
    }
    // A fixed bitmap font makes the index and physical size identical on
    // every host, independent of installed fonts and display scaling.
    const GLYPHS: [[u8; 5]; 11] = [
        [7, 5, 5, 5, 7],
        [2, 6, 2, 2, 7],
        [7, 1, 7, 4, 7],
        [7, 1, 7, 1, 7],
        [5, 5, 7, 1, 1],
        [7, 4, 7, 1, 7],
        [7, 4, 7, 5, 7],
        [7, 1, 1, 1, 1],
        [7, 5, 7, 5, 7],
        [7, 5, 7, 1, 7],
        [0, 5, 2, 5, 0],
    ];
    for (at, ch) in format!("{index} {width}x{height}").chars().enumerate() {
        let glyph = if ch == 'x' {
            Some(10)
        } else {
            ch.to_digit(10).map(|n| n as usize)
        };
        let Some(glyph) = glyph else { continue };
        for (y, row) in GLYPHS[glyph].iter().enumerate() {
            for x in 0..3 {
                if row & (4 >> x) == 0 {
                    continue;
                }
                for dy in 0..4 {
                    for dx in 0..4 {
                        let (x, y) = (8 + at * 16 + x * 4 + dx, 8 + y * 4 + dy);
                        if x < width as usize && y < height as usize {
                            pixels[(y * width as usize + x) * 4..][..4].fill(255);
                        }
                    }
                }
            }
        }
    }
    pixels
}

/// A host with no desktop capture can still run the deterministic substitute.
pub struct FixtureCapture {
    context: Context,
    captures: Captures,
    screens: Json,
}

impl Package for FixtureCapture {
    fn new(context: Context) -> Self {
        Self {
            context,
            captures: Captures::default(),
            screens: Json::Null,
        }
    }
    fn later(&mut self, name: &str, args: &Json, preferred: Option<&Json>, reply: Reply) {
        if name == "screenRecordingStatus" {
            return reply.send(json!({"available": self.context.agent(), "state": if self.context.agent() { "granted" } else { "denied" }}));
        }
        if !self.context.agent() {
            return reply.error("NotSupportedError", None);
        }
        match name {
            "requestScreenRecording" => reply.send(json!({"available":true,"state":"granted"})),
            "captureScreens" => match fixtures(if self.screens.is_array() {
                &self.screens
            } else {
                preferred
                    .and_then(|p| p.get("screens"))
                    .unwrap_or(&Json::Null)
            }) {
                Ok(displays) => self.captures.start(&self.context, args, displays, reply),
                Err((code, why)) => reply.error(code, Some(&why)),
            },
            _ => reply.error("NotSupportedError", None),
        }
    }
    fn preferred(&mut self, values: &Json, _: &Json) {
        self.screens = values["screens"].clone();
    }
    fn cancel(&mut self, request: u64) {
        self.captures.cancel(request);
    }
    fn destroy(&mut self) {
        self.captures = Captures::default();
    }
}

#[cfg(test)]
mod tests;
