//! Images in cells (LLP 1101 §4): a PNG decoded once, drawn as the
//! terminal allows — the kitty graphics protocol, iTerm2's inline images, or
//! half-blocks (`▀`, foreground the top pixel and background the bottom),
//! which every truecolour terminal shows, tmux included. A node hears its
//! source's `load` or `error` as HTML's `<img>` does.

use crate::grid::Rgb;
use exact_kernel::{Kernel, NodeKey, NodeType, PropId, ViewId};
use exact_plan::EventKind;
use exact_runner::Event;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};

/// One decoded image.
#[derive(Debug, Clone)]
pub struct Decoded {
    /// Pixels across.
    pub width: u32,
    /// Pixels down.
    pub height: u32,
    /// RGBA, row-major.
    pub rgba: Vec<u8>,
    /// The file as read, for a protocol that takes PNG bytes.
    pub png: Vec<u8>,
}

impl Decoded {
    /// The colour at a pixel, composited over black.
    pub fn at(&self, x: u32, y: u32) -> Rgb {
        let i = ((y.min(self.height - 1) * self.width + x.min(self.width - 1)) * 4) as usize;
        let a = self.rgba[i + 3] as u32;
        let c = |v: u8| ((v as u32 * a) / 255) as u8;
        Rgb(c(self.rgba[i]), c(self.rgba[i + 1]), c(self.rgba[i + 2]))
    }

    /// The image sampled into `cols` × `rows` half-block cells: each cell's
    /// top and bottom colour, averaged over its pixels.
    pub fn half_blocks(&self, cols: usize, rows: usize) -> Vec<Vec<(Rgb, Rgb)>> {
        let (w, h) = (self.width as f32, self.height as f32);
        let sample = |x0: f32, y0: f32, x1: f32, y1: f32| {
            let (mut r, mut g, mut b, mut n) = (0u32, 0u32, 0u32, 0u32);
            let (xa, xb) = (x0 as u32, (x1.ceil() as u32).max(x0 as u32 + 1));
            let (ya, yb) = (y0 as u32, (y1.ceil() as u32).max(y0 as u32 + 1));
            let step = ((xb - xa).max(yb - ya) / 4).max(1);
            for y in (ya..yb).step_by(step as usize) {
                for x in (xa..xb).step_by(step as usize) {
                    let Rgb(pr, pg, pb) = self.at(x, y);
                    (r, g, b, n) = (r + pr as u32, g + pg as u32, b + pb as u32, n + 1);
                }
            }
            let n = n.max(1);
            Rgb((r / n) as u8, (g / n) as u8, (b / n) as u8)
        };
        (0..rows)
            .map(|row| {
                (0..cols)
                    .map(|col| {
                        let x0 = col as f32 * w / cols as f32;
                        let x1 = (col + 1) as f32 * w / cols as f32;
                        let y0 = (row * 2) as f32 * h / (rows * 2) as f32;
                        let ym = (row * 2 + 1) as f32 * h / (rows * 2) as f32;
                        let y1 = (row * 2 + 2) as f32 * h / (rows * 2) as f32;
                        (sample(x0, y0, x1, ym), sample(x0, ym, x1, y1))
                    })
                    .collect()
            })
            .collect()
    }
}

/// The most bytes a source is read to: Apple's bound on a downloaded body
/// (`docs/contract-for-agents.md`, "An `image` hears").
const MAX_SOURCE_BYTES: u64 = 64 * 1024 * 1024;
/// The longest `data:` source, as every host bounds it (LLP 1011 §2;
/// `exact_raster::MAX_DATA_URL_BYTES`).
const MAX_DATA_URL_BYTES: usize = 1024 * 1024;

/// Read and decode one source: its `error` message when it does not load.
fn fetch(src: &str, assets: Option<&Path>) -> Result<Decoded, String> {
    let bytes = match resolve(src, assets)? {
        Source::Bytes(bytes) => bytes,
        Source::File(path) => read_regular(&path)?,
    };
    decode(bytes).ok_or_else(|| "not an image format this host decodes".to_string())
}

/// Where a source's bytes are.
enum Source {
    File(PathBuf),
    Bytes(Vec<u8>),
}

/// A source as the Linux host resolves it (`host/painted/src/image/assets.rs`):
/// a `data:` URL to its bound, a relative path beneath the app's directory
/// and through no `..`, and an absolute path or `file://` URL, which the
/// harness's `/image` names. The terminal downloads nothing and has no app
/// directories yet, so `http(s)` and `app:/` are refused by name.
fn resolve(src: &str, assets: Option<&Path>) -> Result<Source, String> {
    if src.starts_with("data:") {
        return data_url(src)
            .map(Source::Bytes)
            .ok_or_else(|| "a data: URL this host cannot read".into());
    }
    if src.starts_with("http://") || src.starts_with("https://") {
        return Err("the terminal host does not download images".into());
    }
    if src.starts_with("app:/") {
        return Err("the terminal host has no app directories".into());
    }
    if let Some(path) = src.strip_prefix("file://") {
        return Ok(Source::File(PathBuf::from(path)));
    }
    if src.starts_with('/') {
        return Ok(Source::File(PathBuf::from(src)));
    }
    let relative = !src.contains(':')
        && !src.contains('\\')
        && src
            .split('/')
            .all(|p| !p.is_empty() && p != "." && p != "..");
    let root = assets.ok_or("no app directory to resolve a relative source against")?;
    let inside = relative
        .then(|| {
            let root = root.canonicalize().ok()?;
            let path = root.join(src).canonicalize().ok()?;
            path.starts_with(&root).then_some(path)
        })
        .flatten();
    inside
        .map(Source::File)
        .ok_or_else(|| format!("no file {src} in the app's directory"))
}

/// A regular file's bytes, up to [`MAX_SOURCE_BYTES`]. Opened without
/// blocking, so a FIFO is refused rather than waited on, as a device is.
fn read_regular(path: &Path) -> Result<Vec<u8>, String> {
    use std::io::Read as _;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt as _;
        options.custom_flags(libc::O_NONBLOCK);
    }
    let file = options.open(path).map_err(|e| e.to_string())?;
    if !file.metadata().map_err(|e| e.to_string())?.is_file() {
        return Err("not a regular file".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX_SOURCE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_SOURCE_BYTES {
        return Err("too large".into());
    }
    Ok(bytes)
}

/// A `data:` URL's bytes (RFC 2397), as the Linux host reads one.
fn data_url(src: &str) -> Option<Vec<u8>> {
    use base64::Engine as _;
    if src.len() > MAX_DATA_URL_BYTES {
        return None;
    }
    let (meta, body) = src.strip_prefix("data:")?.split_once(',')?;
    let (body, mut bytes, mut i) = (body.as_bytes(), Vec::new(), 0);
    while i < body.len() {
        let hex = body
            .get(i + 1..i + 3)
            .and_then(|h| std::str::from_utf8(h).ok());
        match hex
            .filter(|_| body[i] == b'%')
            .and_then(|h| u8::from_str_radix(h, 16).ok())
        {
            Some(byte) => (bytes.push(byte), i += 3),
            None => (bytes.push(body[i]), i += 1),
        };
    }
    if !meta.to_ascii_lowercase().ends_with(";base64") {
        return Some(bytes);
    }
    bytes.retain(|b| !b.is_ascii_whitespace() && *b != b'=');
    base64::engine::general_purpose::STANDARD_NO_PAD
        .decode(bytes)
        .ok()
}

fn decode(png: Vec<u8>) -> Option<Decoded> {
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    let mut reader = decoder.read_info().ok()?;
    let mut buf = vec![0; reader.output_buffer_size()?];
    let info = reader.next_frame(&mut buf).ok()?;
    let (width, height) = (info.width, info.height);
    let pixels = &buf[..info.buffer_size()];
    let rgba = match info.color_type {
        png::ColorType::Rgba => pixels.to_vec(),
        png::ColorType::Rgb => pixels
            .chunks(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => pixels.iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => pixels
            .chunks(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    (width > 0 && height > 0).then_some(Decoded {
        width,
        height,
        rgba,
        png,
    })
}

/// How this terminal shows an image.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Protocol {
    /// The kitty graphics protocol (kitty, Ghostty, WezTerm, Konsole).
    Kitty,
    /// iTerm2's OSC 1337 inline images (iTerm2, WezTerm, VS Code).
    Iterm,
    /// Half-block cells, everywhere else — inside tmux too.
    Blocks,
}

impl Protocol {
    /// From the environment: `EXACT_TERM_IMAGES` decides, else the terminal
    /// names itself; tmux swallows both protocols without passthrough.
    pub fn detect() -> Protocol {
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        match env("EXACT_TERM_IMAGES").as_str() {
            "kitty" => return Protocol::Kitty,
            "iterm" => return Protocol::Iterm,
            "blocks" => return Protocol::Blocks,
            _ => {}
        }
        if !env("TMUX").is_empty() {
            return Protocol::Blocks;
        }
        let program = env("TERM_PROGRAM");
        if env("TERM") == "xterm-kitty"
            || !env("KITTY_WINDOW_ID").is_empty()
            || program == "ghostty"
            || program == "WezTerm"
        {
            Protocol::Kitty
        } else if program == "iTerm.app" {
            Protocol::Iterm
        } else {
            Protocol::Blocks
        }
    }
}

type Waker = Arc<dyn Fn() + Send + Sync>;

/// Every image the tree names, read and decoded once per source on a
/// thread of its own, off the commit; the loop's wake brings it back.
pub struct Images {
    /// Each source read so far: its picture, or why it did not load.
    by_source: HashMap<String, Result<Decoded, String>>,
    loading: HashSet<String>,
    done: Sender<(String, Result<Decoded, String>)>,
    landed: Receiver<(String, Result<Decoded, String>)>,
    waker: Arc<Mutex<Option<Waker>>>,
    /// The directory a relative source is read beneath: the entry's.
    assets: Option<PathBuf>,
    /// The image nodes of the last commit and their sources.
    pub nodes: Vec<(ViewId, String)>,
    keys: HashMap<ViewId, NodeKey>,
    /// Each node whose `load` or `error` is decided, by the node and the
    /// source it was for: HTML's once per source.
    heard: HashMap<ViewId, (NodeKey, String)>,
    /// `load` (`None`) or `error` (the reason) owed to a node, oldest first.
    pub(crate) events: Vec<(ViewId, NodeKey, String, Option<String>)>,
    delivering: bool,
}

impl Images {
    /// `assets`: the directory a relative source is read beneath.
    pub(crate) fn new(assets: Option<PathBuf>) -> Self {
        let (done, landed) = channel();
        Images {
            by_source: HashMap::new(),
            loading: HashSet::new(),
            done,
            landed,
            waker: Arc::new(Mutex::new(None)),
            assets,
            nodes: Vec::new(),
            keys: HashMap::new(),
            heard: HashMap::new(),
            events: Vec::new(),
            delivering: false,
        }
    }
}

impl Images {
    pub(crate) fn listen(&mut self, waker: Waker) {
        *self.waker.lock().expect("waker") = Some(waker);
    }

    /// Find the tree's images and start reading any new source.
    pub fn load(&mut self, kernel: &Kernel) {
        self.nodes.clear();
        self.keys.clear();
        if kernel.has_type(NodeType::Image) {
            let mut stack = kernel.roots();
            while let Some(id) = stack.pop() {
                let Some(n) = kernel.node(id) else { continue };
                if n.node_type == NodeType::Image {
                    if let Some(src) = n.props.str(PropId::ImageSource) {
                        self.nodes.push((id, src.to_string()));
                        self.keys.insert(id, n.key);
                    }
                }
                stack.extend(n.children());
            }
        }
        for (_, src) in &self.nodes {
            // A symbol is drawn by its label and reads nothing.
            if src.starts_with("symbol:")
                || self.by_source.contains_key(src)
                || !self.loading.insert(src.clone())
            {
                continue;
            }
            let (src, assets) = (src.clone(), self.assets.clone());
            let (done, waker) = (self.done.clone(), self.waker.clone());
            std::thread::spawn(move || {
                let decoded = fetch(&src, assets.as_deref());
                let _ = done.send((src, decoded));
                if let Some(wake) = waker.lock().ok().and_then(|w| w.clone()) {
                    wake();
                }
            });
        }
        self.decide();
    }

    /// Whether a source is still being read.
    pub fn loading(&self) -> bool {
        !self.loading.is_empty()
    }

    /// Take every source read since; whether any landed.
    pub(crate) fn land(&mut self) -> bool {
        let mut any = false;
        while let Ok((src, decoded)) = self.landed.try_recv() {
            self.loading.remove(&src);
            self.by_source.insert(src, decoded);
            any = true;
        }
        if any {
            self.decide();
        }
        any
    }

    /// Wait up to `timeout` for every source being read; whether any landed.
    pub(crate) fn wait(&mut self, timeout: std::time::Duration) -> bool {
        let deadline = std::time::Instant::now() + timeout;
        let mut any = false;
        while !self.loading.is_empty() {
            let left = deadline.saturating_duration_since(std::time::Instant::now());
            let Ok((src, decoded)) = self.landed.recv_timeout(left) else {
                break;
            };
            self.loading.remove(&src);
            self.by_source.insert(src, decoded);
            any = true;
        }
        if any {
            self.decide();
        }
        self.land() || any
    }

    /// Owe each node whose source has been read its `load` or `error`.
    fn decide(&mut self) {
        let keys = &self.keys;
        self.heard.retain(|id, (key, _)| keys.get(id) == Some(key));
        for (id, src) in &self.nodes {
            let (Some(read), Some(key)) = (self.by_source.get(src), self.keys.get(id)) else {
                continue;
            };
            if self
                .heard
                .get(id)
                .is_some_and(|(k, s)| k == key && s == src)
            {
                continue;
            }
            self.heard.insert(*id, (*key, src.clone()));
            let error = read.as_ref().err().cloned();
            self.events.push((*id, *key, src.clone(), error));
        }
    }

    /// A node's decoded image.
    pub fn of(&self, id: ViewId) -> Option<&Decoded> {
        let src = self.nodes.iter().find(|(n, _)| *n == id).map(|(_, s)| s)?;
        self.by_source.get(src)?.as_ref().ok()
    }
}

/// The bytes that draw `image` into `cols` × `rows` cells at the cursor,
/// leaving the cursor where it was.
pub fn protocol_bytes(protocol: Protocol, image: &Decoded, cols: usize, rows: usize) -> String {
    use base64::Engine as _;
    let data = base64::engine::general_purpose::STANDARD.encode(&image.png);
    match protocol {
        Protocol::Kitty => {
            // Chunked: 4096 base64 bytes per escape, `m=1` until the last.
            let mut out = String::new();
            let chunks: Vec<&[u8]> = data.as_bytes().chunks(4096).collect();
            for (i, chunk) in chunks.iter().enumerate() {
                let more = u8::from(i + 1 < chunks.len());
                let chunk = std::str::from_utf8(chunk).expect("base64 is ASCII");
                if i == 0 {
                    out.push_str(&format!("\x1b_Ga=T,f=100,q=2,C=1,c={cols},r={rows},m={more};{chunk}\x1b\\"));
                } else {
                    out.push_str(&format!("\x1b_Gm={more};{chunk}\x1b\\"));
                }
            }
            out
        }
        Protocol::Iterm => format!(
            "\x1b7\x1b]1337;File=inline=1;size={};width={cols};height={rows};preserveAspectRatio=0:{data}\x07\x1b8",
            image.png.len()
        ),
        Protocol::Blocks => String::new(),
    }
}

impl<D: exact_runner::DataSource> crate::host::Host<D> {
    /// Send each image the `load` or `error` its source owes it, if it is
    /// still that node showing that source and hears it (LLP 1011 §4). At
    /// most those waiting when it starts: one a handler causes (a source
    /// already read) is sent at the next wake, so a handler that keeps
    /// changing sources cannot hold the loop.
    pub(crate) fn deliver_image_events(&mut self) {
        if std::mem::replace(&mut self.images.delivering, true) {
            return;
        }
        for (id, key, src, error) in std::mem::take(&mut self.images.events) {
            let same = self.kernel().node(id).is_some_and(|n| {
                n.key == key && n.props.str(PropId::ImageSource) == Some(src.as_str())
            });
            let (kind, event) = match error {
                None => (EventKind::Load, Event::Load),
                Some(why) => (EventKind::Error, Event::Media(EventKind::Error, why)),
            };
            if same && self.runner.handlers_of(id).contains(&kind) {
                self.dispatch(id, event);
            }
        }
        self.images.delivering = false;
        if !self.images.events.is_empty() {
            if let Some(wake) = self.images.waker.lock().ok().and_then(|w| w.clone()) {
                wake();
            }
        }
    }

    /// Take the sources read since the last wake; whether any landed.
    pub(crate) fn land_images(&mut self) -> bool {
        let landed = self.images.land();
        if landed {
            self.changed();
        }
        self.deliver_image_events();
        landed
    }

    /// Wait up to `timeout` for every image source being read, then send
    /// their events: what a headless drive does before it reads the screen.
    pub fn wait_images(&mut self, timeout: std::time::Duration) {
        if self.images.wait(timeout) {
            self.changed();
        }
        self.deliver_image_events();
    }
}
