//! Images in cells (LLP 1101 §4): a PNG decoded once, drawn as the
//! terminal allows — the kitty graphics protocol, iTerm2's inline images, or
//! half-blocks (`▀`, foreground the top pixel and background the bottom),
//! which every truecolour terminal shows, tmux included. A node hears its
//! source's `load` or `error` as HTML's `<img>` does.

use crate::grid::Rgb;
use exact_kernel::{Kernel, NodeKey, NodeType, PropId, ViewId};
use exact_plan::EventKind;
use exact_runner::Event;
use std::collections::{HashMap, HashSet, VecDeque};
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
        let x = x.min(self.width - 1) as usize;
        let y = y.min(self.height - 1) as usize;
        let i = (y * self.width as usize + x) * 4;
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
/// Most pixels decoded for one image (`exact_raster::MAX_SOURCE_PIXELS`).
const MAX_SOURCE_PIXELS: u64 = 64 * 1024 * 1024;
/// Most bytes allocated for one decoded RGBA frame.
const MAX_DECODED_BYTES: usize = (MAX_SOURCE_PIXELS as usize) * 4;
/// Byte budget for cached images; unreferenced entries evict LRU above this.
const MAX_CACHE_BYTES: usize = 128 * 1024 * 1024;
/// Entry cap for the source cache, bounding retained errors as well as images.
const MAX_CACHE_ENTRIES: usize = 256;
/// Most background image decode threads running at once.
const MAX_DECODE_WORKERS: usize = 4;

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
    // Reject invalid signatures or oversized IHDR dimensions before the
    // decoder allocates scanline or output buffers.
    if png.len() < 24 || &png[..8] != b"\x89PNG\r\n\x1a\n" || &png[12..16] != b"IHDR" {
        return None;
    }
    let ihdr_w = u32::from_be_bytes(png[16..20].try_into().ok()?);
    let ihdr_h = u32::from_be_bytes(png[20..24].try_into().ok()?);
    if ihdr_w == 0 || ihdr_h == 0 || u64::from(ihdr_w) * u64::from(ihdr_h) > MAX_SOURCE_PIXELS {
        return None;
    }
    let mut decoder = png::Decoder::new(std::io::Cursor::new(&png));
    decoder.set_transformations(png::Transformations::normalize_to_color8());
    decoder.set_ignore_text_chunk(true);
    let mut reader = decoder.read_info().ok()?;
    let header = reader.info();
    if header.width == 0
        || header.height == 0
        || u64::from(header.width) * u64::from(header.height) > MAX_SOURCE_PIXELS
    {
        return None;
    }
    let out_len = reader.output_buffer_size()?;
    if out_len > MAX_DECODED_BYTES {
        return None;
    }
    let mut buf = vec![0; out_len];
    let info = reader.next_frame(&mut buf).ok()?;
    let (width, height) = (info.width, info.height);
    if width == 0 || height == 0 || u64::from(width) * u64::from(height) > MAX_SOURCE_PIXELS {
        return None;
    }
    let len = info.buffer_size();
    let rgba = match info.color_type {
        png::ColorType::Rgba => {
            buf.truncate(len);
            buf
        }
        png::ColorType::Rgb => buf[..len]
            .chunks_exact(3)
            .flat_map(|p| [p[0], p[1], p[2], 255])
            .collect(),
        png::ColorType::Grayscale => buf[..len].iter().flat_map(|v| [*v, *v, *v, 255]).collect(),
        png::ColorType::GrayscaleAlpha => buf[..len]
            .chunks_exact(2)
            .flat_map(|p| [p[0], p[0], p[0], p[1]])
            .collect(),
        png::ColorType::Indexed => return None,
    };
    let expected = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    (rgba.len() == expected).then_some(Decoded {
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

struct CacheEntry {
    value: Result<Decoded, String>,
    bytes: usize,
    last_used: u64,
}

impl CacheEntry {
    fn new(value: Result<Decoded, String>, last_used: u64) -> Self {
        let bytes = match &value {
            Ok(decoded) => decoded.rgba.len() + decoded.png.len(),
            Err(why) => why.len(),
        };
        CacheEntry {
            value,
            bytes,
            last_used,
        }
    }
}

#[derive(Default)]
struct DecodeQueue {
    pending: VecDeque<String>,
    workers: usize,
}

/// Every image the tree names, read and decoded once per source on a bounded
/// worker pool off the commit; the loop's wake brings it back.
pub struct Images {
    /// Cached sources: each picture or why it did not load, bounded by LRU eviction.
    by_source: HashMap<String, CacheEntry>,
    cached_bytes: usize,
    tick: u64,
    cache_byte_limit: usize,
    cache_entry_limit: usize,
    loading: HashSet<String>,
    queue: Arc<Mutex<DecodeQueue>>,
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
            cached_bytes: 0,
            tick: 0,
            cache_byte_limit: MAX_CACHE_BYTES,
            cache_entry_limit: MAX_CACHE_ENTRIES,
            loading: HashSet::new(),
            queue: Arc::new(Mutex::new(DecodeQueue::default())),
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

    fn record(&mut self, src: String, decoded: Result<Decoded, String>) {
        self.tick = self.tick.saturating_add(1);
        let entry = CacheEntry::new(decoded, self.tick);
        self.cached_bytes = self.cached_bytes.saturating_add(entry.bytes);
        if let Some(old) = self.by_source.insert(src, entry) {
            self.cached_bytes = self.cached_bytes.saturating_sub(old.bytes);
        }
        self.evict();
    }

    fn evict(&mut self) {
        while self.cached_bytes > self.cache_byte_limit
            || self.by_source.len() > self.cache_entry_limit
        {
            let victim = self
                .by_source
                .iter()
                .filter(|(src, _)| !self.nodes.iter().any(|(_, live)| live == *src))
                .min_by_key(|(_, entry)| entry.last_used)
                .map(|(src, _)| src.clone());
            let Some(victim) = victim else {
                break;
            };
            if let Some(old) = self.by_source.remove(&victim) {
                self.cached_bytes = self.cached_bytes.saturating_sub(old.bytes);
            }
        }
    }

    fn enqueue(&mut self, src: String) {
        let spawn = {
            let mut q = self.queue.lock().unwrap();
            q.pending.push_back(src);
            if q.workers < MAX_DECODE_WORKERS {
                q.workers += 1;
                true
            } else {
                false
            }
        };
        if !spawn {
            return;
        }
        let queue = self.queue.clone();
        let assets = self.assets.clone();
        let done = self.done.clone();
        let waker = self.waker.clone();
        if std::thread::Builder::new()
            .name("exact-terminal-image".into())
            .spawn(move || loop {
                let src = {
                    let mut q = queue.lock().unwrap();
                    match q.pending.pop_front() {
                        Some(src) => src,
                        None => {
                            q.workers -= 1;
                            break;
                        }
                    }
                };
                let decoded = fetch(&src, assets.as_deref());
                if done.send((src, decoded)).is_err() {
                    let mut q = queue.lock().unwrap();
                    q.workers -= 1;
                    break;
                }
                if let Some(wake) = waker.lock().ok().and_then(|w| w.clone()) {
                    wake();
                }
            })
            .is_err()
        {
            let mut failed = Vec::new();
            {
                let mut q = self.queue.lock().unwrap();
                q.workers -= 1;
                if q.workers == 0 {
                    failed.extend(q.pending.drain(..));
                }
            }
            for src in failed {
                let _ = self
                    .done
                    .send((src, Err("could not start image decoder".into())));
            }
        }
    }

    /// Find the tree's images and start reading any new source.
    pub fn load(&mut self, kernel: &Kernel) {
        self.tick = self.tick.saturating_add(1);
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
        let mut to_enqueue = Vec::new();
        for (_, src) in &self.nodes {
            if let Some(entry) = self.by_source.get_mut(src) {
                entry.last_used = self.tick;
                continue;
            }
            // A symbol is drawn by its label and reads nothing.
            if src.starts_with("symbol:") || !self.loading.insert(src.clone()) {
                continue;
            }
            to_enqueue.push(src.clone());
        }
        for src in to_enqueue {
            self.enqueue(src);
        }
        self.evict();
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
            self.record(src, decoded);
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
            self.record(src, decoded);
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
            let error = read.value.as_ref().err().cloned();
            self.events.push((*id, *key, src.clone(), error));
        }
    }

    /// A node's decoded image.
    pub fn of(&self, id: ViewId) -> Option<&Decoded> {
        let src = self.nodes.iter().find(|(n, _)| *n == id).map(|(_, s)| s)?;
        self.by_source.get(src)?.value.as_ref().ok()
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

#[cfg(test)]
mod tests {
    use super::*;

    fn crc32(bytes: &[u8]) -> u32 {
        let mut crc = !0u32;
        for &byte in bytes {
            crc ^= u32::from(byte);
            for _ in 0..8 {
                crc = (crc >> 1) ^ (0xedb8_8320 & (0u32.wrapping_sub(crc & 1)));
            }
        }
        !crc
    }

    fn chunk(kind: &[u8; 4], data: &[u8]) -> Vec<u8> {
        let mut out = Vec::with_capacity(12 + data.len());
        out.extend_from_slice(&(data.len() as u32).to_be_bytes());
        out.extend_from_slice(kind);
        out.extend_from_slice(data);
        out.extend_from_slice(&crc32(&out[4..]).to_be_bytes());
        out
    }

    fn forged_png(width: u32, height: u32) -> Vec<u8> {
        let mut ihdr = Vec::with_capacity(13);
        ihdr.extend_from_slice(&width.to_be_bytes());
        ihdr.extend_from_slice(&height.to_be_bytes());
        ihdr.extend_from_slice(&[8, 6, 0, 0, 0]); // 8-bit RGBA
        let mut png = b"\x89PNG\r\n\x1a\n".to_vec();
        png.extend_from_slice(&chunk(b"IHDR", &ihdr));
        png.extend_from_slice(&chunk(b"IDAT", &[]));
        png.extend_from_slice(&chunk(b"IEND", &[]));
        png
    }

    fn tiny_rgba_png(pixel: [u8; 4]) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, 1, 1);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            encoder
                .write_header()
                .unwrap()
                .write_image_data(&pixel)
                .unwrap();
        }
        out
    }

    #[test]
    fn oversized_ihdr_png_is_refused_before_allocating() {
        use base64::Engine as _;
        for (w, h) in [(1_000_000, 1_000_000), (8_193, 8_193), (0, 10), (10, 0)] {
            let png = forged_png(w, h);
            assert!(decode(png.clone()).is_none(), "{w}x{h} should be refused");
            let url = format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(&png)
            );
            assert!(fetch(&url, None).is_err(), "{w}x{h} data URL should fail");
        }
        let ok = decode(tiny_rgba_png([10, 20, 30, 255])).expect("1x1 RGBA decodes");
        assert_eq!((ok.width, ok.height), (1, 1));
        assert_eq!(ok.rgba, [10, 20, 30, 255]);
    }

    #[test]
    fn image_cache_evicts_unreferenced_entries_and_bounds_workers() {
        use base64::Engine as _;
        let mut images = Images::new(None);
        images.cache_entry_limit = 3;
        images.cache_byte_limit = 1024;
        let sample = tiny_rgba_png([1, 2, 3, 255]);
        let b64 = base64::engine::general_purpose::STANDARD.encode(&sample);
        // Pin node 1 on source 0 while inserting more sources than the cache allows.
        let pinned = format!("data:image/png;base64,{b64}");
        images.nodes.push((1, pinned.clone()));
        for i in 0..8 {
            let src = if i == 0 {
                pinned.clone()
            } else {
                format!("data:image/png;x={i};base64,{b64}")
            };
            images.loading.insert(src.clone());
            images.enqueue(src);
        }
        assert!(images.queue.lock().unwrap().workers <= MAX_DECODE_WORKERS);
        assert!(images.wait(std::time::Duration::from_secs(5)));
        assert!(!images.loading());
        assert!(images.by_source.len() <= 3);
        assert!(
            images.by_source.contains_key(&pinned),
            "referenced source must stay cached"
        );
    }
}
