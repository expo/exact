//! The file picker on Linux (LLP 1069.002 D8): there is no desktop host
//! yet, so `showPicker` is refused with `cancel` (ruled, Q4) and only the
//! agent's substitute answers (D9). What both need is here: the app's
//! directories behind `app:/` (D4, D7), the copy into `app:/tmp/picked/`,
//! and a picked image's pixel size read from its header (D3).

use exact_runner::DataError;
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

#[cfg(windows)]
#[path = "picker_windows.rs"]
mod windows;

fn identity(name: &str) -> bool {
    !matches!(name, "" | "." | "..")
        && name
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b".-_".contains(&b))
        && leaf(name)
}

fn leaf(name: &str) -> bool {
    if name.is_empty() || matches!(name, "." | "..") || name.contains('\0') {
        return false;
    }
    #[cfg(windows)]
    {
        let mut parts = Path::new(name).components();
        if exact_grants::WindowsPath::validate_component(name).is_err()
            || !matches!(parts.next(), Some(std::path::Component::Normal(_)))
            || parts.next().is_some()
        {
            return false;
        }
    }
    true
}

/// Whether a `showPicker` is held for the agent: `EXACT_AGENT=1`, which a
/// production bake has already dropped (LLP 1069.007 D2).
pub fn agent() -> bool {
    std::env::var("EXACT_AGENT").as_deref() == Ok("1") || cfg!(test)
}

/// An XDG base directory: `variable` when it holds an absolute path, else
/// `$HOME/<fallback>` when `HOME` does. A relative path in either is
/// invalid and ignored, as the XDG Base Directory Specification says
/// (<https://specifications.freedesktop.org/basedir-spec/latest/>); `None`
/// when neither gives one.
pub fn xdg_base(variable: &str, fallback: &str) -> Option<PathBuf> {
    base_from(
        std::env::var_os(variable),
        std::env::var_os("HOME"),
        fallback,
    )
}

fn base_from(value: Option<OsString>, home: Option<OsString>, fallback: &str) -> Option<PathBuf> {
    let absolute = |v: OsString| Some(PathBuf::from(v)).filter(|p| p.is_absolute());
    value
        .and_then(absolute)
        .or_else(|| home.and_then(absolute).map(|home| home.join(fallback)))
}

/// `app:/data`, `app:/cache`, `app:/tmp`, once storage is configured.
static ROOTS: Mutex<Option<[PathBuf; 3]>> = Mutex::new(None);

/// Record the app's directories, and empty `app:/tmp/picked/` and
/// `app:/tmp/shared/` ([`empty_landings`]).
pub fn set_roots(data: PathBuf, cache: PathBuf, temporary: PathBuf) {
    empty_landings(&temporary);
    *ROOTS.lock().unwrap_or_else(|e| e.into_inner()) = Some([data, cache, temporary]);
}

/// The `app:/tmp` roots whose `picked` and `shared` this process has emptied.
static LANDINGS_EMPTIED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Empty `temporary`'s `picked` and `shared` once a process: what the last
/// launch picked or was shared is gone (D4). A development reload, or an
/// update applied while the app is open, boots a host that activates its
/// storage again; that activation leaves them, since the state it carries
/// may name the files.
fn empty_landings(temporary: &Path) {
    let mut emptied = LANDINGS_EMPTIED.lock().unwrap_or_else(|e| e.into_inner());
    if emptied.iter().any(|t| t == temporary) {
        return;
    }
    emptied.push(temporary.to_path_buf());
    // `temporary` is `<cache base>/exact/<app id>/temporary`, or a drive's
    // `<cache base>/exact/<app id>/agent/<name>/temporary` ([`app_dirs`]).
    let depth = if std::env::var_os("EXACT_AGENT").is_some() {
        5
    } else {
        3
    };
    empty_landings_below(temporary, depth);
}

/// Empty `temporary`'s `picked` and `shared` unless `temporary`, or a
/// directory between it and its ancestor `depth` levels up, is a symbolic
/// link: `remove_dir_all` follows a link above the path it is given, so
/// emptying through one deletes the `picked` and `shared` it points at.
fn empty_landings_below(temporary: &Path, depth: usize) {
    let refused = match first_link(temporary, depth) {
        Ok(None) => {
            let _ = std::fs::remove_dir_all(temporary.join("picked"));
            let _ = std::fs::remove_dir_all(temporary.join("shared"));
            return;
        }
        Ok(Some(at)) => format!("{} is a symbolic link", at.display()),
        Err((at, e)) => format!("could not inspect {}: {e}", at.display()),
    };
    eprintln!("exact: app:/tmp/picked and app:/tmp/shared are not emptied: {refused}");
}

/// The app's `app:/data`, `app:/cache` and `app:/tmp` (LLP 1027.001): under
/// the XDG bases (Windows: the user's LocalAppData), or a scripted drive's scratch tree (`--storage`), apart
/// from the app's real files, with that tree when an authored test starts it
/// empty (`EXACT_AGENT_STORAGE_FRESH`). `None`: an app with no id, or a
/// drive with no scratch store, which has no storage.
#[allow(clippy::type_complexity)]
pub(crate) fn app_dirs(app_id: &str) -> Result<Option<([PathBuf; 3], Option<PathBuf>)>, DataError> {
    // Scripted drives must not read or write the developer's app files;
    // one that names a scratch tree gets storage there instead.
    let scratch = match std::env::var_os("EXACT_AGENT") {
        Some(_) => match agent_scratch()? {
            Some(name) => Some(name),
            None => return Ok(None),
        },
        None => None,
    };
    if app_id.is_empty() {
        return Ok(None);
    }
    if !identity(app_id) {
        return Err(DataError::Unavailable("unsafe app storage identity".into()));
    }
    #[cfg(windows)]
    let (mut data, mut cache) = windows::bases(&windows::local_app_data()?, app_id)?;
    #[cfg(not(windows))]
    let (mut data, mut cache) = {
        let base = |variable: &str, fallback: &str| {
            xdg_base(variable, fallback)
                .map(|base| base.join("exact").join(app_id))
                .ok_or_else(|| DataError::Unavailable("app storage needs an absolute HOME".into()))
        };
        (
            base("XDG_DATA_HOME", ".local/share")?.join("data"),
            base("XDG_CACHE_HOME", ".cache")?,
        )
    };
    let mut fresh = None;
    if let Some(name) = scratch {
        cache = cache.join("agent").join(name);
        if std::env::var_os("EXACT_AGENT_STORAGE_FRESH").is_some() {
            fresh = Some(cache.clone());
        }
        data = cache.join("data");
    }
    // Sibling roots keep app:/cache grants from implicitly reaching tmp.
    // The user's cache base avoids a predictable shared /tmp directory.
    Ok(Some((
        [data, cache.join("cache"), cache.join("temporary")],
        fresh,
    )))
}

/// Root for kept answers and persisted states (`<root>/kv`), including
/// Windows outside the agent. A nameless drive has no persistent storage.
pub(crate) fn store_root(app_id: &str) -> Result<Option<PathBuf>, DataError> {
    Ok(app_dirs(app_id)?.and_then(|(roots, _)| roots[0].parent().map(|path| path.to_path_buf())))
}

/// Where `secret.keep` names live (`<root>/secrets`). A named agent drive
/// keeps them in its scratch tree beside its `app:/data`; outside the agent
/// the app keeps them in its own data root,
/// `$XDG_DATA_HOME/exact/<app id>` (`~/.local/share` when unset), as Ibex's
/// file backends: a file per secret, `0600` in a `0700` directory, written
/// whole and renamed into place, not encrypted (LLP 1027.007 D13, ibex LLP
/// 0069 §3). `None` keeps them in memory: a drive that names no scratch
/// store, an app with no id, and Windows outside the agent (until its
/// credential vault).
pub(crate) fn secret_root(app_id: &str) -> Result<Option<PathBuf>, DataError> {
    if app_id.is_empty() || (cfg!(windows) && std::env::var_os("EXACT_AGENT").is_none()) {
        return Ok(None);
    }
    // `roots[0]` is `<root>/data`. In a drive, secrets live in the scratch
    // tree, so a fresh drive's emptying of that tree ([`empty_fresh_tree`])
    // takes them too.
    store_root(app_id)
}

/// The scratch trees a fresh drive has emptied in this process.
static EMPTIED: Mutex<Vec<PathBuf>> = Mutex::new(Vec::new());

/// Empty a fresh drive's scratch tree (`EXACT_AGENT_STORAGE_FRESH`) once a
/// process: at the first boot that reads its store, before the snapshot and
/// before any commit can write a secret or a kept answer into it. The
/// post-pixel activation and any later boot here leave it as it is; emptying
/// it again there removed what a commit in between had written while memory
/// still held it, so a relaunch read nothing back.
pub(crate) fn empty_fresh_tree(app_id: &str) -> Result<(), DataError> {
    let Some((_, Some(tree))) = app_dirs(app_id)? else {
        return Ok(());
    };
    let mut emptied = EMPTIED.lock().unwrap_or_else(|e| e.into_inner());
    if emptied.contains(&tree) {
        return Ok(());
    }
    unlinked(&tree)?;
    // `remove_dir_all` follows no link inside the tree; the directories
    // above it were just checked.
    match std::fs::remove_dir_all(&tree) {
        Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
            return Err(DataError::Unavailable(format!(
                "EXACT_AGENT_STORAGE_FRESH: could not empty {}: {e}",
                tree.display()
            )));
        }
        _ => {}
    }
    emptied.push(tree);
    Ok(())
}

/// Refuses to empty a scratch tree (`<cache base>/exact/<app id>/agent/<name>`)
/// when it, or a directory between it and the cache base, is a symbolic link:
/// emptying it through one would delete whatever the link points at, such as
/// the app's real files.
fn unlinked(tree: &std::path::Path) -> Result<(), DataError> {
    match first_link(tree, 4) {
        Ok(None) => Ok(()),
        Ok(Some(at)) => Err(DataError::Unavailable(format!(
            "EXACT_AGENT_STORAGE_FRESH: {} is a symbolic link, so the scratch store under it is not emptied: remove the link, or name another store",
            at.display()
        ))),
        Err((at, e)) => Err(DataError::Unavailable(format!(
            "EXACT_AGENT_STORAGE_FRESH: could not inspect {}: {e}",
            at.display()
        ))),
    }
}

/// The first symbolic link among `path` and the directories between it and
/// its ancestor `depth` levels up, looking down from that ancestor. `None`
/// when there is none or a part does not exist yet; an error names the part
/// that could not be inspected.
fn first_link(path: &Path, depth: usize) -> Result<Option<PathBuf>, (PathBuf, std::io::Error)> {
    let Some(base) = path.ancestors().nth(depth) else {
        return Ok(None);
    };
    let mut at = base.to_path_buf();
    for part in path.strip_prefix(base).unwrap_or(path).components() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.file_type().is_symlink() => return Ok(Some(at)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(e) => return Err((at, e)),
            Ok(_) => {}
        }
    }
    Ok(None)
}

/// A scripted drive's scratch storage (`EXACT_AGENT_STORAGE=<name>`): a tree
/// of its own under the cache base, so a drive can exercise storage without
/// touching the app's real files. Absent, a drive has no storage.
fn agent_scratch() -> Result<Option<String>, DataError> {
    let Some(name) = std::env::var_os("EXACT_AGENT_STORAGE") else {
        return Ok(None);
    };
    match name.to_str() {
        Some(name) if identity(name) => Ok(Some(name.to_owned())),
        _ => Err(DataError::Unavailable(
            "EXACT_AGENT_STORAGE: one name of letters, digits, '.', '-' or '_'".into(),
        )),
    }
}

/// The app's directories at boot, before storage is configured after first
/// pixel: an `image "app:/data/…"` resolves from the first frame, whether or
/// not anything was picked (D7; recipes F18). Nothing is emptied until
/// storage is configured ([`set_roots`]).
pub fn know_roots(app_id: &str) {
    if let Ok(Some((roots, _))) = app_dirs(app_id) {
        *ROOTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(roots);
    }
}

/// The app's `data`, `cache` and `tmp` directories. A scripted drive
/// without a scratch store has none; its picks land in a directory of this
/// process's own.
pub fn roots() -> [PathBuf; 3] {
    if let Some(r) = ROOTS.lock().unwrap_or_else(|e| e.into_inner()).clone() {
        return r;
    }
    let base = std::env::temp_dir().join(format!("exact-agent-{}", std::process::id()));
    let r = [
        base.join("data"),
        base.join("cache"),
        base.join("temporary"),
    ];
    *ROOTS.lock().unwrap_or_else(|e| e.into_inner()) = Some(r.clone());
    r
}

/// Where an `app:/data|cache|tmp/…` path names (D7), for a file the host
/// writes there: `None` for any other scheme, root, or a `..`. A file the
/// app names is read through [`open`] or [`file`], never through this.
pub fn resolve(path: &str) -> Option<PathBuf> {
    resolve_from(path, roots)
}

/// The regular file an `app:/` path names, opened as the data module's
/// `readFile` opens it (ibex2's `AppDirectories`): below its root's handle,
/// following no symbolic link at any part (on Windows, no reparse point).
/// An `image`, a picked or shared file's size, a saved copy and an
/// extension's file read through this, so a link the app's own code made
/// in its directories reads nothing outside them.
pub fn open(path: &str) -> Option<std::fs::File> {
    open_from(path, roots)
}

/// The place of the file [`open`] finds, for a reader that needs a path (a
/// player's animation): `None` where `open` refuses.
pub fn file(path: &str) -> Option<PathBuf> {
    let roots = roots();
    open_from(path, || roots.clone())?;
    resolve_from(path, || roots)
}

/// The place an `app:/` path names for an extension, which may write it
/// (`ExtensionContext.file`): the file [`file`] answers, or, where nothing
/// is there yet or a directory is, the place itself when every part of it
/// that exists is a directory reached through no symbolic link. A writer
/// makes what is missing; nothing below a missing part can be a link.
/// That holds when this answers; it checks only the path's own parts, so a
/// caller that walks a directory it answers must not follow links in it.
pub fn place(path: &str) -> Option<PathBuf> {
    place_from(path, roots())
}

fn place_from(path: &str, roots: [PathBuf; 3]) -> Option<PathBuf> {
    if open_from(path, || roots.clone()).is_some() {
        return resolve_from(path, || roots);
    }
    let (index, parts) = parse(path)?;
    let mut at = roots.into_iter().nth(index)?;
    // The root is the host's, opened as `open_from` opens it: its own
    // links followed.
    match std::fs::metadata(&at) {
        Ok(meta) if meta.is_dir() => {}
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            return Some(parts.iter().fold(at, |p, part| p.join(part)))
        }
        _ => return None,
    }
    for (i, part) in parts.iter().enumerate() {
        at.push(part);
        match std::fs::symlink_metadata(&at) {
            Ok(meta) if meta.is_dir() => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                return Some(parts[i + 1..].iter().fold(at, |p, part| p.join(part)))
            }
            _ => return None,
        }
    }
    Some(at)
}

fn open_from(path: &str, roots: impl FnOnce() -> [PathBuf; 3]) -> Option<std::fs::File> {
    static READ: std::sync::LazyLock<ibex2::grant::GrantSet> = std::sync::LazyLock::new(|| {
        ibex2::grant::GrantSet::parse("fs.read app:/data\nfs.read app:/cache\nfs.read app:/tmp")
            .expect("the app roots' read grants parse")
    });
    let (index, _) = parse(path)?;
    let root = roots().into_iter().nth(index)?;
    // The named root stands for all three: another may not exist yet (a
    // picked photo's `app:/tmp` before any storage request made `app:/data`).
    let directories = ibex2::stdlib::app_fs::AppDirectories::new(&root, &root, &root).ok()?;
    directories.open_file(&READ, path).ok()
}

/// The root an `app:/` path is under, by index, and its names below it.
fn parse(path: &str) -> Option<(usize, Vec<&str>)> {
    let rest = path.strip_prefix("app:/")?;
    let (dir, rest) = rest.split_once('/')?;
    let index = match dir {
        "data" => 0,
        "cache" => 1,
        "tmp" => 2,
        _ => return None,
    };
    let parts: Vec<&str> = rest.split('/').collect();
    parts.iter().all(|p| leaf(p)).then_some((index, parts))
}

fn resolve_from(path: &str, roots: impl FnOnce() -> [PathBuf; 3]) -> Option<PathBuf> {
    let (index, parts) = parse(path)?;
    let root = roots().into_iter().nth(index)?;
    Some(parts.iter().fold(root, |p, part| p.join(part)))
}

/// A picked image's pixel size from its header, without decoding: PNG,
/// JPEG (the frame's, with EXIF orientation 5–8 swapping it), GIF, WebP.
pub fn dimensions(bytes: &[u8]) -> Option<(f64, f64)> {
    let be16 = |b: &[u8], i: usize| Some(u16::from_be_bytes([*b.get(i)?, *b.get(i + 1)?]) as f64);
    let le16 = |b: &[u8], i: usize| Some(u16::from_le_bytes([*b.get(i)?, *b.get(i + 1)?]) as f64);
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        let w = u32::from_be_bytes(bytes.get(16..20)?.try_into().ok()?) as f64;
        let h = u32::from_be_bytes(bytes.get(20..24)?.try_into().ok()?) as f64;
        return Some((w, h));
    }
    if bytes.starts_with(b"GIF8") {
        return Some((le16(bytes, 6)?, le16(bytes, 8)?));
    }
    if bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP") {
        let le24 = |i: usize| -> Option<f64> {
            Some(
                (u32::from(*bytes.get(i)?)
                    | u32::from(*bytes.get(i + 1)?) << 8
                    | u32::from(*bytes.get(i + 2)?) << 16) as f64,
            )
        };
        return match bytes.get(12..16)? {
            b"VP8 " => Some((
                (le16(bytes, 26)? as u32 & 0x3fff) as f64,
                (le16(bytes, 28)? as u32 & 0x3fff) as f64,
            )),
            b"VP8L" => {
                let b = u32::from_le_bytes(bytes.get(21..25)?.try_into().ok()?);
                Some(((b & 0x3fff) as f64 + 1.0, ((b >> 14) & 0x3fff) as f64 + 1.0))
            }
            b"VP8X" => Some((le24(24)? + 1.0, le24(27)? + 1.0)),
            _ => None,
        };
    }
    if bytes.starts_with(&[0xff, 0xd8]) {
        let mut i = 2;
        let mut swap = false;
        while i + 4 <= bytes.len() {
            if bytes[i] != 0xff {
                return None;
            }
            let marker = bytes[i + 1];
            let len = be16(bytes, i + 2)? as usize;
            if marker == 0xe1 && bytes.get(i + 4..i + 10) == Some(b"Exif\0\0") {
                swap = exif_rotates(bytes.get(i + 10..i + 2 + len)?).unwrap_or(false);
            }
            if matches!(marker, 0xc0..=0xc3 | 0xc5..=0xc7 | 0xc9..=0xcb | 0xcd..=0xcf) {
                let (h, w) = (be16(bytes, i + 5)?, be16(bytes, i + 7)?);
                return Some(if swap { (h, w) } else { (w, h) });
            }
            i += 2 + len;
        }
    }
    None
}

/// Whether a TIFF-in-EXIF block's orientation is 5–8 (a quarter turn).
fn exif_rotates(tiff: &[u8]) -> Option<bool> {
    let le = tiff.get(0..2)? == b"II";
    let u16_at = |i: usize| -> Option<u16> {
        let b = [*tiff.get(i)?, *tiff.get(i + 1)?];
        Some(if le {
            u16::from_le_bytes(b)
        } else {
            u16::from_be_bytes(b)
        })
    };
    let u32_at = |i: usize| -> Option<u32> {
        let b: [u8; 4] = tiff.get(i..i + 4)?.try_into().ok()?;
        Some(if le {
            u32::from_le_bytes(b)
        } else {
            u32::from_be_bytes(b)
        })
    };
    let ifd = u32_at(4)? as usize;
    for k in 0..u16_at(ifd)? as usize {
        let entry = ifd + 2 + k * 12;
        if u16_at(entry)? == 0x0112 {
            return Some((5..=8).contains(&u16_at(entry + 8)?));
        }
    }
    Some(false)
}

fn read_dimensions(source: impl std::io::Read) -> Option<(f64, f64)> {
    use std::io::Read;
    // Metadata beyond this header stays absent; the picked file itself has no size limit.
    let mut bytes = Vec::new();
    source.take(1024 * 1024).read_to_end(&mut bytes).ok()?;
    dimensions(&bytes)
}

/// Copy the driver's file at `source` to `app_path` (under
/// `app:/tmp/picked/`), and describe it as `change` delivers it. The type
/// comes from the provider or the name; a video's duration is not read here.
pub fn copy_in(source: &Path, app_path: &str, mime: &str) -> Result<exact_runner::Picked, String> {
    let target = resolve(app_path).ok_or_else(|| format!("{app_path}: not an app path"))?;
    if let Some(dir) = target.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let size = copy_size(source, &target, |from, to| std::fs::copy(from, to))?;
    let (width, height) = if matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    ) {
        std::fs::File::open(&target)
            .ok()
            .and_then(read_dimensions)
            .map_or((None, None), |(w, h)| (Some(w), Some(h)))
    } else {
        (None, None)
    };
    Ok(exact_runner::Picked {
        path: app_path.into(),
        name: source
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
        mime: mime.into(),
        size,
        width,
        height,
        duration: None,
    })
}

fn copy_size(
    source: &Path,
    target: &Path,
    copy: impl FnOnce(&Path, &Path) -> std::io::Result<u64>,
) -> Result<f64, String> {
    let result = copy(source, target)
        .and_then(|_| std::fs::metadata(target))
        .map(|metadata| metadata.len() as f64);
    if result.is_err() {
        let _ = std::fs::remove_file(target);
    }
    result.map_err(|e| format!("{}: {e}", source.display()))
}

/// A cold share's claim (LLP 1133.024 D9), whose copies the presenter makes
/// once storage is configured after first pixel, with the descriptors its
/// files arrived as ([`Held`]).
pub(crate) static COLD_SHARE: Mutex<Option<(exact_runner::share_in::Claimed, Held)>> =
    Mutex::new(None);

/// The files a share from Android's share sheet arrived as: each stream the
/// intent's grant opened, named to the claim as `/proc/self/fd/<n>` and read
/// from the descriptor itself, since opening that path again goes through
/// the sender's storage, which the grant does not open. Dropping them, once
/// the claim has copied them, closes them. Empty on every other host.
pub(crate) type Held = Vec<(PathBuf, std::fs::File)>;

/// The descriptors `share`'s files name, taken over: the reader detached
/// each for this library to close (`Shares.kt`). Only a share the reader
/// made is passed here, never the driver's.
#[cfg(target_os = "android")]
#[allow(unsafe_code)]
pub(crate) fn hold(share: &exact_runner::share_in::Pending) -> Held {
    use std::os::fd::FromRawFd;
    let mut seen = std::collections::HashSet::new();
    share
        .files
        .iter()
        .filter_map(|f| {
            let fd = f
                .source
                .strip_prefix("/proc/self/fd/")
                .ok()?
                .to_str()?
                .parse::<i32>()
                .ok()
                .filter(|fd| *fd > 2 && seen.insert(*fd))?;
            // SAFETY: the reader detached each descriptor from its
            // ParcelFileDescriptor and gave it to this library alone.
            Some((f.source.clone(), unsafe { std::fs::File::from_raw_fd(fd) }))
        })
        .collect()
}

/// A cold share (LLP 1133.024 D5, D9): on Android the reader's, from the
/// launching intent (`EXACT_SHARE`), else under the agent the driver's
/// (`EXACT_AGENT_SHARE`); each `{"target", "share"}`, as the `share` op
/// carries them. Claimed before the boot, which takes its location and
/// tokens with the launch's other facts. A host takes a share from the
/// environment nowhere else.
pub fn cold_share(seed: f64) -> Option<(String, exact_runner::time::Launch)> {
    use exact_runner::share_in::{Claims, Pending, ShareTarget};
    let (variable, value) = match std::env::var("EXACT_SHARE") {
        Ok(value) if cfg!(target_os = "android") => ("EXACT_SHARE", value),
        _ if agent() => (
            "EXACT_AGENT_SHARE",
            std::env::var("EXACT_AGENT_SHARE").ok()?,
        ),
        _ => return None,
    };
    std::env::remove_var(variable);
    let request: serde_json::Value = serde_json::from_str(&value).ok()?;
    let (Some(target), Some(share)) = (
        ShareTarget::from_json(&request["target"]),
        Pending::from_json(&request["share"], None),
    ) else {
        eprintln!("exact: {variable} refused: it needs the manifest's share_target and a share");
        return None;
    };
    #[cfg(target_os = "android")]
    let held = if variable == "EXACT_SHARE" {
        hold(&share)
    } else {
        Held::new()
    };
    #[cfg(not(target_os = "android"))]
    let held = Held::new();
    let mut claims = Claims::new(seed);
    let claimed = claims.claim(&target, share);
    let launch = exact_runner::time::Launch {
        shared: claimed.tokens.clone(),
        claims: Some(claims),
        ..exact_runner::time::Launch::NONE
    };
    let location = claimed.location.clone();
    *COLD_SHARE.lock().unwrap_or_else(|e| e.into_inner()) = Some((claimed, held));
    Some((location, launch))
}

/// `app:/tmp/shared/` on this machine, once storage is configured.
pub(crate) fn shared_dir() -> Option<PathBuf> {
    resolve(&format!("{}x", exact_runner::share_in::SHARED))
        .and_then(|p| p.parent().map(Path::to_path_buf))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn an_xdg_base_ignores_a_relative_path_and_has_none_without_home() {
        let some = |s: &str| Some(OsString::from(s));
        let share = ".local/share";
        assert_eq!(
            base_from(some("/x/data"), some("/home/a"), share),
            Some(PathBuf::from("/x/data"))
        );
        assert_eq!(
            base_from(some("updates"), some("/home/a"), share),
            Some(PathBuf::from("/home/a/.local/share"))
        );
        assert_eq!(base_from(some("updates"), None, share), None);
        assert_eq!(base_from(None, None, share), None);
        assert_eq!(base_from(None, some("home"), share), None);
    }

    /// A reset refuses a scratch tree reached through a symbolic link above it
    /// (an `agent` directory linked to the app's real data, say), leaving
    /// what the link points at as it was; an ordinary tree is let through.
    #[cfg(unix)]
    #[test]
    fn a_reset_refuses_a_scratch_tree_reached_through_a_link() {
        let base =
            std::env::temp_dir().join(format!("exact-painted-unlinked-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let real = base.join("real-data");
        std::fs::create_dir_all(real.join("s1")).unwrap();
        std::fs::write(real.join("s1").join("library.json"), "kept").unwrap();
        std::fs::create_dir_all(base.join("cache/exact/app.id")).unwrap();
        std::os::unix::fs::symlink(&real, base.join("cache/exact/app.id/agent")).unwrap();
        let tree = base.join("cache/exact/app.id/agent/s1");
        let refused = unlinked(&tree).unwrap_err();
        assert!(
            format!("{refused:?}").contains("is a symbolic link"),
            "{refused:?}"
        );
        assert_eq!(
            std::fs::read_to_string(real.join("s1").join("library.json")).unwrap(),
            "kept"
        );
        std::fs::remove_file(base.join("cache/exact/app.id/agent")).unwrap();
        std::fs::create_dir_all(&tree).unwrap();
        assert!(unlinked(&tree).is_ok());
        assert!(unlinked(&base.join("cache/exact/app.id/agent/absent")).is_ok());
        let _ = std::fs::remove_dir_all(&base);
    }

    #[test]
    fn image_metadata_reads_only_a_bounded_header_even_for_large_unsupported_files() {
        use std::io::Read;
        let header = include_bytes!("../../../scripts/fixtures/picker/photo.png");
        let mut source = header
            .as_slice()
            .chain(std::io::repeat(0).take(16 * 1024 * 1024));
        assert_eq!(read_dimensions(&mut source), Some((160.0, 106.0)));
        let mut left = Vec::new();
        source.read_to_end(&mut left).unwrap();
        assert!(
            left.len() >= 15 * 1024 * 1024,
            "metadata consumed the whole image"
        );
        let mut tiff = b"II*\0"
            .as_slice()
            .chain(std::io::repeat(0).take(16 * 1024 * 1024));
        assert_eq!(read_dimensions(&mut tiff), None);
        let mut left = Vec::new();
        tiff.read_to_end(&mut left).unwrap();
        assert!(left.len() >= 15 * 1024 * 1024);
    }

    #[test]
    fn headers_give_the_pixel_size() {
        let mut png = b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR".to_vec();
        png.extend(640u32.to_be_bytes());
        png.extend(480u32.to_be_bytes());
        assert_eq!(dimensions(&png), Some((640.0, 480.0)));
        let gif = b"GIF89a\x20\x00\x10\x00";
        assert_eq!(dimensions(gif), Some((32.0, 16.0)));
        // SOI, a baseline frame of 300 × 200.
        let jpeg = [
            0xff, 0xd8, 0xff, 0xc0, 0, 11, 8, 0, 200, 1, 44, 3, 0, 0, 0, 0,
        ];
        assert_eq!(dimensions(&jpeg), Some((300.0, 200.0)));
        assert_eq!(dimensions(b"nothing"), None);
    }

    #[test]
    fn app_paths_resolve_under_their_roots_and_refuse_traversal() {
        assert!(resolve("app:/tmp/picked/1-1.jpg")
            .is_some_and(|p| p.ends_with("temporary/picked/1-1.jpg")));
        assert_eq!(resolve("app:/tmp/../x"), None);
        assert_eq!(resolve("app:/etc/passwd"), None);
        assert_eq!(resolve("/etc/passwd"), None);
    }

    /// An app file opens below its root, and a symbolic link the app's own
    /// code planted there (a file's or a directory's) opens nothing, as
    /// `readFile` refuses it.
    #[cfg(unix)]
    #[test]
    fn app_files_open_below_their_roots_through_no_link() {
        let base = std::env::temp_dir().join(format!("exact-painted-open-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let roots = [
            base.join("data"),
            base.join("cache"),
            base.join("temporary"),
        ];
        std::fs::create_dir_all(roots[0].join("photos")).unwrap();
        std::fs::create_dir_all(base.join("private")).unwrap();
        std::fs::write(base.join("private/secret.png"), "outside").unwrap();
        std::fs::write(roots[0].join("photos/a.png"), "inside").unwrap();
        std::os::unix::fs::symlink(base.join("private/secret.png"), roots[0].join("x.png"))
            .unwrap();
        std::os::unix::fs::symlink(base.join("private"), roots[0].join("linked")).unwrap();
        let read = |path: &str| {
            open_from(path, || roots.clone()).map(|mut f| {
                let mut s = String::new();
                std::io::Read::read_to_string(&mut f, &mut s).unwrap();
                s
            })
        };
        let joined = resolve_from("app:/data/x.png", || roots.clone()).unwrap();
        assert_eq!(
            std::fs::read_to_string(joined).unwrap(),
            "outside",
            "the place a host writes reaches through the link, so no reader opens it"
        );
        assert_eq!(read("app:/data/photos/a.png").as_deref(), Some("inside"));
        assert_eq!(read("app:/data/x.png"), None, "a linked file");
        assert_eq!(
            read("app:/data/linked/secret.png"),
            None,
            "a linked directory"
        );
        assert_eq!(read("app:/data/photos"), None, "a directory");
        assert_eq!(read("app:/tmp/absent.png"), None, "a root not made yet");
        assert_eq!(read("app:/data/../private/secret.png"), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An extension's file may be one it is about to write: a regular file,
    /// a directory or nothing yet answers, at its place below the root, and
    /// a link at any part of the path answers nothing.
    #[cfg(unix)]
    #[test]
    fn an_extension_place_may_be_absent_but_is_reached_through_no_link() {
        let base = std::env::temp_dir().join(format!("exact-painted-place-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let roots = [
            base.join("data"),
            base.join("cache"),
            base.join("temporary"),
        ];
        std::fs::create_dir_all(roots[0].join("photos")).unwrap();
        std::fs::create_dir_all(base.join("private")).unwrap();
        std::fs::write(roots[0].join("photos/a.png"), "inside").unwrap();
        std::os::unix::fs::symlink(base.join("private/new.png"), roots[0].join("dangling.png"))
            .unwrap();
        std::os::unix::fs::symlink(base.join("private"), roots[0].join("linked")).unwrap();
        let place = |path: &str| place_from(path, roots.clone());
        let under = |rest: &str| Some(roots[0].join(rest));
        assert_eq!(place("app:/data/photos/a.png"), under("photos/a.png"));
        assert_eq!(
            place("app:/data/shot-1.jpg"),
            under("shot-1.jpg"),
            "a file not written yet"
        );
        assert_eq!(
            place("app:/data/camera/day/1.jpg"),
            under("camera/day/1.jpg"),
            "a file in directories not made yet"
        );
        assert_eq!(place("app:/data/photos"), under("photos"), "a directory");
        // Only the path's own parts are checked: a link beside an absent
        // name leaves it answered, so a caller that walks the directory
        // must not follow links (`Files.removeTree` on Android).
        std::os::unix::fs::symlink(base.join("private"), roots[0].join("photos/out")).unwrap();
        assert_eq!(place("app:/data/photos/x"), under("photos/x"));
        // A link at the last part is refused, so a module empties a
        // directory by asking for the directory, never a name inside it.
        std::os::unix::fs::symlink(base.join("private"), roots[0].join("photos/x")).unwrap();
        assert_eq!(place("app:/data/photos/x"), None);
        assert_eq!(place("app:/data/photos"), under("photos"));
        assert_eq!(
            place("app:/tmp/shot-1.jpg"),
            Some(roots[2].join("shot-1.jpg")),
            "a root not made yet"
        );
        assert_eq!(
            place("app:/data/dangling.png"),
            None,
            "a link to nothing yet"
        );
        assert_eq!(
            place("app:/data/linked/new.png"),
            None,
            "a linked directory"
        );
        assert_eq!(place("app:/data/photos/a.png/x"), None, "below a file");
        assert_eq!(place("app:/data/../private/new.png"), None);
        let _ = std::fs::remove_dir_all(&base);
    }

    /// `picked` and `shared` are emptied at the first activation in this
    /// process and kept at a later one, a reload's with its state carried.
    #[test]
    fn landings_are_emptied_once_a_process() {
        let temporary =
            std::env::temp_dir().join(format!("exact-landings-once-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&temporary);
        let put = |name: &str| {
            let file = temporary.join(name);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, "x").unwrap();
            file
        };
        let stale = [put("picked/1-1.jpg"), put("shared/1/a.txt")];
        empty_landings(&temporary);
        assert!(stale.iter().all(|f| !f.exists()), "the last launch's files");
        let picked = [put("picked/7-1.jpg"), put("shared/7/b.txt")];
        empty_landings(&temporary);
        assert!(picked.iter().all(|f| f.exists()), "this launch's files");
        let _ = std::fs::remove_dir_all(&temporary);
    }

    /// `app:/tmp` reached through a symbolic link, at it or above it, is
    /// not emptied, and what the link points at keeps its `picked` and
    /// `shared`; an ordinary `app:/tmp` is emptied.
    #[cfg(unix)]
    #[test]
    fn landings_are_not_emptied_through_a_link() {
        use std::os::unix::fs::symlink;
        let base = std::env::temp_dir().join(format!("exact-landings-link-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&base);
        let app = base.join("cache/exact/app.id");
        let temporary = app.join("temporary");
        let fill = |dir: &Path| {
            let files = [dir.join("picked/deep/f"), dir.join("shared/1/g")];
            for file in &files {
                std::fs::create_dir_all(file.parent().unwrap()).unwrap();
                std::fs::write(file, "kept").unwrap();
            }
            files
        };
        // `temporary` itself a link.
        let target = fill(&base.join("target"));
        std::fs::create_dir_all(&app).unwrap();
        symlink(base.join("target"), &temporary).unwrap();
        empty_landings_below(&temporary, 3);
        assert!(target.iter().all(|f| f.exists()), "through app:/tmp");
        // A directory above it a link.
        std::fs::remove_file(&temporary).unwrap();
        std::fs::remove_dir(&app).unwrap();
        let above = fill(&base.join("elsewhere/temporary"));
        symlink(base.join("elsewhere"), &app).unwrap();
        empty_landings_below(&temporary, 3);
        assert!(
            above.iter().all(|f| f.exists()),
            "through the app's directory"
        );
        // No link.
        std::fs::remove_file(&app).unwrap();
        let own = fill(&temporary);
        empty_landings_below(&temporary, 3);
        assert!(own.iter().all(|f| !f.exists()), "an ordinary app:/tmp");
        let _ = std::fs::remove_dir_all(&base);
    }

    /// An `app:/data` image resolves before storage is configured, with
    /// nothing picked (D7; recipes F18): in a child, whose roots are its own.
    #[test]
    fn app_roots_are_known_at_boot_before_storage() {
        const CHILD: &str = "EXACT_LINUX_ROOTS_TEST";
        if std::env::var_os(CHILD).is_none() {
            let home = std::env::temp_dir().join(format!("exact-roots-{}", std::process::id()));
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "picker::tests::app_roots_are_known_at_boot_before_storage",
                ])
                .env(CHILD, "1")
                .env("HOME", &home)
                .env("EXACT_AGENT", "1")
                .env("EXACT_AGENT_STORAGE", "s1")
                .env_remove("XDG_DATA_HOME")
                .env_remove("XDG_CACHE_HOME")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            assert!(!home.exists(), "knowing the roots creates nothing");
            return;
        }
        know_roots("test.exact.roots");
        let file = resolve("app:/data/photos/a.jpg").unwrap();
        #[cfg(not(windows))]
        assert!(
            file.ends_with(".cache/exact/test.exact.roots/agent/s1/data/photos/a.jpg"),
            "{}",
            file.display()
        );
        #[cfg(windows)]
        assert_eq!(
            file,
            windows::local_app_data()
                .unwrap()
                .join("exact/test.exact.roots/agent/s1/data/photos/a.jpg")
        );
    }
}

#[test]
fn failed_copy_removes_its_partial_destination() {
    let target = std::env::temp_dir().join(format!("exact-copy-failure-{}", std::process::id()));
    let result = copy_size(Path::new("source"), &target, |_, to| {
        std::fs::write(to, b"partial bytes")?;
        assert!(to.exists());
        Err(std::io::Error::other("copy failed after writing"))
    });
    assert!(result.unwrap_err().contains("copy failed after writing"));
    let left = target.exists();
    let _ = std::fs::remove_file(target);
    assert!(!left, "a failed copy left its partial destination");
}
