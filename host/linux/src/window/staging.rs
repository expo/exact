//! A drag's files as the receiving app is lent them (LLP 1094.000 D13):
//! copies, never the app's own files, made once at the drag's first request
//! for a file type, off the listener's loop. Nothing in the drag deletes a
//! copy whose name it sent, since a receiver may use the name long after
//! the drag: the copies are temporaries left to the cleanup of the folder
//! they are in, which in a Flatpak sandbox is the app's own (see [`sweep`]).

use exact_painted::image::Assets;
use exact_painted::picker::AppFiles;
use rustix::fs::{self as rfs, AtFlags, Dir, Mode, OFlags, StatxFlags};
use std::ffi::{CStr, CString, OsStr};
use std::fs::File;
use std::io::{PipeWriter, Write};
use std::os::fd::OwnedFd;
use std::os::unix::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant, SystemTime};

/// How long a drag's folder in a sandbox's cache goes unused before the
/// app removes it: the age systemd's default rule gives `/var/tmp`
/// (`tmpfiles.d/tmp.conf`).
const AGE: Duration = Duration::from_secs(30 * 24 * 60 * 60);
/// The start of each drag's folder's name.
const PREFIX: &str = "exact-drag-";

/// A drag's files: what to copy until the first request for a file type
/// takes it, then the copies once they are made, with when they were.
pub(super) struct Staging {
    job: Mutex<Option<Job>>,
    copies: Mutex<Option<(Arc<Copies>, Instant)>>,
    ready: Condvar,
}

/// The files a node names, the generation their shipped files are read
/// from, and the session's own files, both pinned at the lift (LLP
/// 1094.000 D3).
struct Job {
    names: Vec<String>,
    assets: Assets,
    app: Option<AppFiles>,
}

/// A drag's copies, in the drag's order, a file whose copy failed left
/// out: each where the URI list names it, and found again for the portal
/// through the drag's folder, held open since it was made.
#[derive(Default)]
pub(super) struct Copies {
    folder: Option<OwnedFd>,
    files: Vec<Copied>,
}

/// A copy: its path, its folder's and its own name in the drag's folder,
/// and the file it is (device and inode).
struct Copied {
    path: PathBuf,
    own: CString,
    leaf: CString,
    id: (u64, u64),
}

impl Copies {
    /// Each copy's path, in the drag's order.
    pub(super) fn paths(&self) -> Vec<&Path> {
        self.files.iter().map(|c| c.path.as_path()).collect()
    }

    pub(super) fn len(&self) -> usize {
        self.files.len()
    }

    /// Copy `n` as a path alone (`O_PATH`), as the portal takes it: found
    /// through the drag's folder following no link, and only while it is
    /// the regular file the copy made, so a link or another file put in its
    /// place since is never registered. The portal refuses a descriptor
    /// opened `O_NOFOLLOW`, which can name a link itself, so the file found
    /// is opened again through `/proc/self/fd`, which names that same file.
    pub(super) fn open(&self, n: usize) -> Result<File, String> {
        let copied = self.files.get(n).ok_or("no such copy")?;
        let folder = self
            .folder
            .as_ref()
            .ok_or("the drag's folder is not open")?;
        let own = rfs::openat(
            folder,
            copied.own.as_c_str(),
            FOUND.union(OFlags::DIRECTORY),
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?;
        let found = rfs::openat(&own, copied.leaf.as_c_str(), FOUND, Mode::empty())
            .map_err(|e| e.to_string())?;
        let stat = rfs::fstat(&found).map_err(|e| e.to_string())?;
        let regular = rfs::FileType::from_raw_mode(stat.st_mode) == rfs::FileType::RegularFile;
        if !regular || (stat.st_dev, stat.st_ino) != copied.id {
            return Err(format!(
                "{} is no longer the copy the drag made",
                copied.path.display()
            ));
        }
        let found = format!("/proc/self/fd/{}", std::os::fd::AsRawFd::as_raw_fd(&found));
        let file = rfs::open(
            found.as_str(),
            OFlags::PATH.union(OFlags::CLOEXEC),
            Mode::empty(),
        )
        .map_err(|e| e.to_string())?;
        Ok(File::from(file))
    }
}

/// How a copy and its folder are found again: as a path alone, following
/// no link.
const FOUND: OFlags = OFlags::PATH.union(OFlags::NOFOLLOW).union(OFlags::CLOEXEC);

impl Staging {
    pub(super) fn new(names: Vec<String>, assets: Assets, app: Option<AppFiles>) -> Arc<Self> {
        Arc::new(Staging {
            job: Mutex::new(Some(Job { names, assets, app })),
            copies: Mutex::new(None),
            ready: Condvar::new(),
        })
    }

    /// Start the copies on a thread of their own, unless a request already
    /// did. When they are made, a byte goes to `made`, and `journal` hears
    /// why each copy that failed did.
    pub(super) fn start(
        self: &Arc<Self>,
        made: &Arc<PipeWriter>,
        journal: impl Fn(String) + Send + 'static,
    ) {
        let Some(job) = self.job.lock().unwrap_or_else(|e| e.into_inner()).take() else {
            return;
        };
        let (staging, says) = (self.clone(), made.clone());
        let copy = move || {
            staging.made(stage(&job, &journal));
            let _ = (&*says).write(&[1]);
        };
        if let Err(e) = std::thread::Builder::new()
            .name("exact-drag-copies".into())
            .spawn(copy)
        {
            // No copies: what waits for them goes on without files
            eprintln!("exact: a drag's files were not copied: {e}");
            self.made(Copies::default());
            let _ = (&**made).write(&[1]);
        }
    }

    /// Keep `copies`, made now, and wake what waits for them.
    pub(super) fn made(&self, copies: Copies) {
        *self.copies.lock().unwrap_or_else(|e| e.into_inner()) =
            Some((copies.into(), Instant::now()));
        self.ready.notify_all();
    }

    /// The copies and when they were made, once they are.
    pub(super) fn copies(&self) -> Option<(Arc<Copies>, Instant)> {
        self.copies
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone()
    }

    /// The copies and when they were made, waiting until they are.
    pub(super) fn wait(&self) -> (Arc<Copies>, Instant) {
        let mut copies = self.copies.lock().unwrap_or_else(|e| e.into_inner());
        loop {
            if let Some(copies) = copies.as_ref() {
                return copies.clone();
            }
            copies = self.ready.wait(copies).unwrap_or_else(|e| e.into_inner());
        }
    }
}

/// Copy each file `job` names into a new folder of the drag's own, in the
/// app's cache in a sandbox, else where systemd puts large temporary files.
fn stage(job: &Job, journal: &dyn Fn(String)) -> Copies {
    // A sandbox's own temporary folders are hidden from the host, so in one the app's cache alone serves
    let base = if super::sandbox::sandboxed() {
        super::sandbox::cache().inspect(|cache| sweep(cache, SystemTime::now()))
    } else {
        temporary_large()
    };
    match base {
        Some(base) => stage_in(&base, &job.names, &job.assets, job.app.as_ref(), journal),
        None => {
            journal("drag: its files were not copied: no folder takes them".into());
            Copies::default()
        }
    }
}

/// [`stage`] in `base`: each file in a folder of its own, so two files of
/// one name keep it, each made through the descriptor of the folder above
/// it, following no link.
fn stage_in(
    base: &Path,
    names: &[String],
    assets: &Assets,
    app: Option<&AppFiles>,
    journal: &dyn Fn(String),
) -> Copies {
    let made = folder(base).and_then(|path| {
        let held = rfs::open(&path, FOUND.union(OFlags::DIRECTORY), Mode::empty())?;
        Ok((path, held))
    });
    let (path, held) = match made {
        Ok(made) => made,
        Err(e) => {
            journal(format!(
                "drag: its files were not copied: {}: {e}",
                base.display()
            ));
            return Copies::default();
        }
    };
    let mut files = Vec::new();
    for (n, name) in names.iter().enumerate() {
        let Some(leaf) = name.rsplit('/').next().filter(|leaf| !leaf.is_empty()) else {
            continue;
        };
        let Ok(leaf) = CString::new(leaf) else {
            journal(format!("drag: {name} was not copied: its name holds a NUL"));
            continue;
        };
        let own = CString::new(n.to_string()).expect("a number holds no NUL");
        match copy_into(&held, &own, &leaf, name, assets, app) {
            Ok(id) => files.push(Copied {
                path: path
                    .join(OsStr::from_bytes(own.as_bytes()))
                    .join(OsStr::from_bytes(leaf.as_bytes())),
                own,
                leaf,
                id,
            }),
            Err(why) => journal(format!("drag: {name} was not copied: {why}")),
        }
    }
    Copies {
        folder: Some(held),
        files,
    }
}

/// Copy the file `name` names to `leaf` in a new folder `own` in `folder`:
/// the copy's device and inode. A copy that fails is removed, with its folder.
fn copy_into(
    folder: &OwnedFd,
    own: &CStr,
    leaf: &CStr,
    name: &str,
    assets: &Assets,
    app: Option<&AppFiles>,
) -> Result<(u64, u64), String> {
    rfs::mkdirat(folder, own, Mode::from_raw_mode(0o777)).map_err(|e| e.to_string())?;
    let copied = rfs::openat(folder, own, FOUND.union(OFlags::DIRECTORY), Mode::empty())
        .map_err(|e| e.to_string())
        .and_then(|inner| {
            let made = OFlags::WRONLY
                .union(OFlags::CREATE)
                .union(OFlags::EXCL)
                .union(OFlags::NOFOLLOW)
                .union(OFlags::CLOEXEC);
            let target = File::from(
                rfs::openat(&inner, leaf, made, Mode::from_raw_mode(0o666))
                    .map_err(|e| e.to_string())?,
            );
            let stat = rfs::fstat(&target).map_err(|e| e.to_string())?;
            let copied = copy_one(name, assets, app, target);
            if copied.is_err() {
                let _ = rfs::unlinkat(&inner, leaf, AtFlags::empty());
            }
            copied.map(|()| (stat.st_dev, stat.st_ino))
        });
    if copied.is_err() {
        let _ = rfs::unlinkat(folder, own, AtFlags::REMOVEDIR);
    }
    copied
}

/// Copy the file `name` names into `target`, new and empty.
fn copy_one(
    name: &str,
    assets: &Assets,
    app: Option<&AppFiles>,
    target: File,
) -> Result<(), String> {
    if name.starts_with("app:/") {
        let app = app.ok_or("the app's files were not open when the drag began")?;
        let source = app.open_path(name)?;
        return crate::drag_copy::copy_app_file(source.into(), target);
    }
    // A shipped file holds no lock of the app's, so it is copied here
    if let Some(path) = assets.path(name) {
        let source = File::open(path).map_err(|e| e.to_string())?;
        return crate::drag_copy::clone_or_copy(&source, &target).map_err(|e| e.to_string());
    }
    match assets.read_asset(name)? {
        Some(bytes) => (&target).write_all(&bytes).map_err(|e| e.to_string()),
        None => Err("the app does not ship it".into()),
    }
}

/// A new folder in `base`, the person's alone (mkdtemp(3)).
fn folder(base: &Path) -> std::io::Result<PathBuf> {
    let template = CString::new(base.join(format!("{PREFIX}XXXXXX")).as_os_str().as_bytes())
        .map_err(std::io::Error::other)?;
    let template = template.into_raw();
    // SAFETY: the template is a terminated, writable string mkdtemp fills in place.
    let made = unsafe { libc::mkdtemp(template) };
    // SAFETY: the pointer came from into_raw above, and mkdtemp kept its length.
    let path = unsafe { CString::from_raw(template) };
    if made.is_null() {
        return Err(std::io::Error::last_os_error());
    }
    Ok(PathBuf::from(OsStr::from_bytes(path.as_bytes())))
}

/// Where systemd puts large temporary files (`systemd-path
/// temporary-large`): the first of `$TMPDIR`, `$TEMP` and `$TMP` that is an
/// absolute, normalized path to an existing folder, else `/var/tmp`.
fn temporary_large() -> Option<PathBuf> {
    let named = ["TMPDIR", "TEMP", "TMP"]
        .into_iter()
        .filter_map(std::env::var_os)
        .map(PathBuf::from)
        .find(|path| normalized(path) && path.is_dir());
    Some(named.unwrap_or_else(|| PathBuf::from("/var/tmp")))
}

/// Whether `path` is absolute and normalized as systemd's
/// `path_is_normalized` has it: no `.` or `..` part, no doubled `/`, and
/// no `/` at the end but the root's.
fn normalized(path: &Path) -> bool {
    let bytes = path.as_os_str().as_bytes();
    bytes.starts_with(b"/")
        && (bytes == b"/"
            || (!bytes.ends_with(b"/")
                && bytes[1..]
                    .split(|&b| b == b'/')
                    .all(|part| !matches!(part, b"" | b"." | b".."))))
}

/// Age the sandbox's drag folders as the app starts ([`sweep`]), on a
/// thread of its own; nothing outside a sandbox.
pub(super) fn sweep_at_start() {
    if !super::sandbox::sandboxed() {
        return;
    }
    let _ = std::thread::Builder::new()
        .name("exact-drag-sweep".into())
        .spawn(|| {
            if let Some(cache) = super::sandbox::cache() {
                sweep(&cache, SystemTime::now());
            }
        });
}

/// Remove each drag's folder in `cache` in which nothing, the folder
/// included, is younger than [`AGE`] by the times systemd's `tmpfiles`
/// reads, following no link: a sandbox's cache is the app's to age, which
/// no system rule reaches. Each folder is aged and emptied through one
/// descriptor of it, and only what was aged is removed, each entry while it
/// is still the one aged, so a folder put in a folder's place meanwhile, or
/// a file added since, stays, and so does its folder. It runs as the app
/// starts and as each drag makes its folder; a folder being filled is new,
/// so another instance's drag is never removed.
pub(super) fn sweep(cache: &Path, now: SystemTime) {
    let Ok(root) = rfs::open(cache, FOLDER.difference(OFlags::NOATIME), Mode::empty()) else {
        return;
    };
    for name in names(&root).unwrap_or_default() {
        if !name.to_bytes().starts_with(PREFIX.as_bytes()) {
            continue;
        }
        let Ok(folder) = rfs::openat(&root, name.as_c_str(), FOLDER, Mode::empty()) else {
            continue;
        };
        let Some((newest, own, inside)) = survey(&folder) else {
            continue;
        };
        let old = now.duration_since(newest).is_ok_and(|age| age > AGE);
        // The folder itself unused since it was aged, then emptied of what was aged
        let unused = || used(&folder, c"").is_some_and(|(time, ..)| time <= own);
        if old && unused() && remove(&folder, &inside).is_some() && same(&root, &name, &folder) {
            let _ = rfs::unlinkat(&root, name.as_c_str(), AtFlags::REMOVEDIR);
        }
    }
}

/// How a folder is opened to be aged: its last part not followed, and read
/// without touching its access time (`O_NOATIME`), which the app may do to
/// the folders it made.
const FOLDER: OFlags = OFlags::RDONLY
    .union(OFlags::DIRECTORY)
    .union(OFlags::NOFOLLOW)
    .union(OFlags::CLOEXEC)
    .union(OFlags::NOATIME);

/// The names in `folder` but `.` and `..`; none when it cannot be read
/// whole.
fn names(folder: &OwnedFd) -> Option<Vec<CString>> {
    // Reopened with the folder's own flags, so its access time stays as it was
    let mut dir = Dir::read_from(folder).ok()?;
    let mut names = Vec::new();
    while let Some(entry) = dir.read() {
        let entry = entry.ok()?;
        let name = entry.file_name();
        if name != c"." && name != c".." {
            names.push(name.to_owned());
        }
    }
    Some(names)
}

/// What [`survey`] found in a folder: each entry's name, identity (device
/// and inode) and last use, and, for a folder, what it holds.
struct Aged {
    name: CString,
    id: (u64, u64),
    used: SystemTime,
    inside: Option<Vec<Aged>>,
}

/// The newest of the times `tmpfiles` ages by over `folder` and all
/// beneath it, links not followed, the folder's own, and what it found
/// there: a file's access, birth, change and modification times, a
/// folder's access, birth and modification. None when a part cannot be
/// read, or not without touching its access time.
fn survey(folder: &OwnedFd) -> Option<(SystemTime, SystemTime, Vec<Aged>)> {
    let (own, ..) = used(folder, c"")?;
    let mut newest = own;
    let mut found = Vec::new();
    for name in names(folder)? {
        let (time, id, is_folder) = used(folder, &name)?;
        newest = newest.max(time);
        let used = time;
        let inside = match is_folder {
            true => {
                let inner = rfs::openat(folder, name.as_c_str(), FOLDER, Mode::empty()).ok()?;
                let (time, _, inside) = survey(&inner)?;
                newest = newest.max(time);
                Some(inside)
            }
            false => None,
        };
        found.push(Aged {
            name,
            id,
            used,
            inside,
        });
    }
    Some((newest, own, found))
}

/// When `name` in `folder` (empty: the folder itself) was last used, by the
/// times `tmpfiles` ages by, its link not followed; the entry's identity
/// (device and inode); and whether it is a folder.
fn used(folder: &OwnedFd, name: &CStr) -> Option<(SystemTime, (u64, u64), bool)> {
    let flags = match name.is_empty() {
        true => AtFlags::SYMLINK_NOFOLLOW | AtFlags::EMPTY_PATH,
        false => AtFlags::SYMLINK_NOFOLLOW,
    };
    let x = rfs::statx(
        folder,
        name,
        flags,
        StatxFlags::BASIC_STATS | StatxFlags::BTIME,
    )
    .ok()?;
    let time = |t: rfs::StatxTimestamp| {
        SystemTime::UNIX_EPOCH + Duration::new(t.tv_sec.max(0) as u64, t.tv_nsec.min(999_999_999))
    };
    let is_folder = rfs::FileType::from_raw_mode(x.stx_mode.into()) == rfs::FileType::Directory;
    let mut newest = time(x.stx_atime).max(time(x.stx_mtime));
    if x.stx_mask & StatxFlags::BTIME.bits() != 0 {
        newest = newest.max(time(x.stx_btime));
    }
    if !is_folder {
        newest = newest.max(time(x.stx_ctime));
    }
    let device = rustix::fs::makedev(x.stx_dev_major, x.stx_dev_minor);
    Some((newest, (device, x.stx_ino), is_folder))
}

/// Remove from `folder`, through descriptors and following no link, the
/// entries [`survey`] found there, each only while its name still holds
/// the entry found, a file only while it is no newer than it was found, a
/// folder once it is empty: none when a part could not be removed, or was
/// replaced, used or added to since.
fn remove(folder: &OwnedFd, found: &[Aged]) -> Option<()> {
    let unchanged = |entry: &Aged, newer: bool| {
        let (time, id, _) = used(folder, &entry.name)?;
        (id == entry.id && (newer || time <= entry.used)).then_some(())
    };
    for entry in found {
        let name = entry.name.as_c_str();
        match &entry.inside {
            Some(inside) => {
                let inner = rfs::openat(folder, name, FOLDER, Mode::empty()).ok()?;
                let (time, id, _) = used(&inner, c"")?;
                if id != entry.id || time > entry.used {
                    return None;
                }
                remove(&inner, inside)?;
                // Emptying it made it newer; its name must still hold it
                unchanged(entry, true)?;
                rfs::unlinkat(folder, name, AtFlags::REMOVEDIR).ok()?;
            }
            None => {
                unchanged(entry, false)?;
                rfs::unlinkat(folder, name, AtFlags::empty()).ok()?;
            }
        }
    }
    Some(())
}

/// Whether `name` in `root` is still the folder `folder` is open on.
fn same(root: &OwnedFd, name: &CStr, folder: &OwnedFd) -> bool {
    let (Ok(now), Ok(held)) = (
        rfs::statat(root, name, AtFlags::SYMLINK_NOFOLLOW),
        rfs::fstat(folder),
    ) else {
        return false;
    };
    (now.st_dev, now.st_ino) == (held.st_dev, held.st_ino)
}

#[cfg(test)]
#[path = "staging_tests.rs"]
mod tests;
