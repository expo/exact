//! Where an app file is written before it is put in place: a directory per
//! launch in each app root, `<root>/.exact-staging/<launch>/`, holding an
//! exclusive lock on its `.lock` for the life of the process that made it
//! (Exact local patch 54). A process that ends during a write, by an exit
//! that does not join its workers, a kill or a crash, never removes its
//! staging file, so a launch's first use of a root removes every launch
//! directory there whose lock no live process holds. Making a launch
//! directory and sweeping both hold the staging directory's own
//! `.namespace.lock`, so a sweep never sees a launch directory before its
//! lock. The image spool (`spool_dir` in Exact's
//! `host/painted/src/image/remote.rs`) works the same way.
//!
//! A path of that name the app made before is left as it is: it is ours only
//! if it is a directory holding `.namespace.lock`, or an empty one. A root
//! with no staging directory of ours, or one that cannot be made, stages
//! beside the target as before.
//!
//! Not covered: on a case-insensitive APFS volume, a name that folds to
//! `.exact-staging` only under Unicode case folding (`ſ`, U+017F) still
//! reaches the directory, as an 8.3 short name can on Windows. A temporary
//! file takes the root's permissions, not its destination folder's.
use super::*;
use std::sync::Mutex;

/// The directory in each app root that holds the launch directories. It is
/// no app path: `parse` refuses it, and a root's listing leaves it out.
pub(super) const STAGING: &str = ".exact-staging";
/// The staging directory's lock, and the mark that it is ours.
const NAMESPACE: &str = ".namespace.lock";
/// A launch directory's lock.
const LOCK: &str = ".lock";

#[cfg(unix)]
type Dir = File;
#[cfg(windows)]
type Dir = Directory;

/// A launch directory this process made, by its root's identity.
struct Launch {
    root: (u64, u64),
    name: String,
    dir: (u64, u64),
    /// Open, and locked, for the life of the process.
    _lock: File,
}

static LAUNCHES: Mutex<Vec<Launch>> = Mutex::new(Vec::new());

/// Whether `name`, the first component below a root, is the staging
/// directory: Windows and a Mac's default volume find it in any case.
pub(super) fn reserved(name: &str) -> bool {
    name.eq_ignore_ascii_case(STAGING)
}

/// A root's names, sorted, without the staging directory, which is left out
/// as the root is read so it never counts against the listing's budget.
pub(super) fn listed(root: &Dir) -> io::Result<Vec<String>> {
    let mut names = Vec::new();
    let mut used = 0;
    let mut keep = |name: String| {
        if reserved(&name) {
            return Ok(());
        }
        crate::stdlib::fs::push_name(&mut names, &mut used, name)
    };
    #[cfg(unix)]
    each_name(root, usize::MAX, |name| {
        keep(name.to_string_lossy().into_owned())
    })?;
    #[cfg(windows)]
    root.visit_names(&mut keep)?;
    names.sort();
    Ok(names)
}

impl AppDirectories {
    /// This launch's staging directory in the root `index` names, made at
    /// the launch's first use of that root.
    fn staging(&self, index: usize) -> Result<Dir, HostError> {
        #[cfg(unix)]
        let root = self.root(index)?;
        #[cfg(windows)]
        let root = &self.roots[index];
        launch(&root).map_err(error)
    }

    /// [`Self::staging`], or `parent`, the target's directory, when the root
    /// has none of ours: a write never fails for want of one.
    pub(super) fn staging_or(&self, index: usize, parent: &Dir) -> Result<Dir, HostError> {
        self.staging(index)
            .or_else(|_| parent.try_clone().map_err(error))
    }
}

/// Make this launch's staging directory now in each of `roots` that exists,
/// once a root, removing those of launches that have ended. A root's first
/// staged or atomic write does it otherwise, so a launch that only reads
/// would leave them.
pub fn claim_staging<P: AsRef<Path>>(roots: &[P]) {
    let mut claimed = Vec::new();
    for root in roots {
        let Ok(root) = open_root(root.as_ref()) else {
            continue;
        };
        let Ok(id) = identity_of(&root) else {
            continue;
        };
        if !claimed.contains(&id) {
            claimed.push(id);
            let _ = launch(&root);
        }
    }
}

fn launch(root: &Dir) -> io::Result<Dir> {
    let mut launches = LAUNCHES.lock().unwrap_or_else(|e| e.into_inner());
    let id = identity_of(root)?;
    if let Some(at) = launches.iter().position(|launch| launch.root == id) {
        match reopen(root, &launches[at].name) {
            Ok(dir) if identity_of(&dir)? == launches[at].dir => return Ok(dir),
            // Made again if it is gone, or if another directory took its name
            // or the identity of a root that was removed.
            Ok(_) => drop(launches.remove(at)),
            Err(e) if e.kind() == io::ErrorKind::NotFound => drop(launches.remove(at)),
            // Any other error (`EMFILE`, a passing failure) keeps the entry and
            // its lock, which a `Staged` in flight relies on: the write stages
            // beside its target this once.
            Err(e) => return Err(e),
        }
    }
    let staging = match child(root, STAGING) {
        Ok(staging) if has_file(&staging, NAMESPACE) || entries(&staging)?.is_empty() => staging,
        Ok(_) => {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "the app's own folder is named .exact-staging",
            ))
        }
        Err(e) if e.kind() == io::ErrorKind::NotFound => make(root, STAGING)?,
        // A file or a link of that name, the app's.
        Err(e) => return Err(e),
    };
    let namespace = lock_file(&staging, NAMESPACE)?;
    // Another launch holds it only to sweep and make its directory: asked a
    // while, never waited on without end.
    let mut attempts = 0;
    while !try_lock(&namespace) {
        attempts += 1;
        if attempts == 20 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "the staging directory is held by another launch",
            ));
        }
        std::thread::sleep(std::time::Duration::from_millis(25));
    }
    sweep(&staging);
    let mut bytes = [0; 12];
    getrandom::getrandom(&mut bytes).map_err(|e| io::Error::other(e.to_string()))?;
    let name: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    let dir = make(&staging, &name)?;
    let lock = lock_file(&dir, LOCK)?;
    if !try_lock(&lock) {
        return Err(io::Error::other("a new launch directory's lock is held"));
    }
    drop(namespace);
    launches.push(Launch {
        root: id,
        name,
        dir: identity_of(&dir)?,
        _lock: lock,
    });
    Ok(dir)
}

/// This launch's directory in `root`, opened again.
fn reopen(root: &Dir, name: &str) -> io::Result<Dir> {
    #[cfg(test)]
    if tests::REOPEN_FAILS.with(std::cell::Cell::get) {
        return Err(io::Error::other("an injected failure, as EMFILE"));
    }
    child(root, STAGING).and_then(|staging| child(&staging, name))
}

/// Remove the launch directories in `staging` whose lock no process holds.
/// Only a launch's name is considered, so nothing else there is removed.
fn sweep(staging: &Dir) {
    let Ok(names) = entries(staging) else {
        return;
    };
    for name in names {
        if name.len() != 24 || !name.bytes().all(|b| b.is_ascii_hexdigit()) {
            continue;
        }
        let Ok(dir) = child(staging, &name) else {
            continue;
        };
        let free = match open_lock(&dir) {
            Ok(lock) => try_lock(&lock),
            // A launch makes its directory and its lock under the namespace
            // lock, so a directory with none is one whose launch ended between
            // the two.
            Err(e) if e.kind() == io::ErrorKind::NotFound => true,
            // A lock that cannot be opened otherwise leaves the directory.
            Err(_) => false,
        };
        drop(dir);
        if free {
            let _ = remove_tree(staging, &name);
        }
    }
}

#[cfg(unix)]
fn identity_of(dir: &File) -> io::Result<(u64, u64)> {
    use std::os::unix::fs::MetadataExt;
    let meta = dir.metadata()?;
    Ok((meta.dev(), meta.ino()))
}
#[cfg(unix)]
fn child(dir: &File, name: &str) -> io::Result<File> {
    directory(dir, name)
}
#[cfg(unix)]
fn make(dir: &File, name: &str) -> io::Result<File> {
    mkdir(dir, name)?;
    directory(dir, name)
}
#[cfg(unix)]
fn lock_file(dir: &File, name: &str) -> io::Result<File> {
    open_at(dir, name, libc::O_RDWR | libc::O_CREAT)
}
#[cfg(unix)]
fn open_lock(dir: &File) -> io::Result<File> {
    open_at(dir, LOCK, libc::O_RDWR)
}
#[cfg(unix)]
fn open_root(path: &Path) -> io::Result<File> {
    use std::os::unix::fs::OpenOptionsExt;
    if !path.is_absolute() {
        return Err(io::Error::other("app directory must be absolute"));
    }
    (std::fs::OpenOptions::new().read(true))
        .custom_flags(libc::O_DIRECTORY | libc::O_NOFOLLOW | libc::O_CLOEXEC)
        .open(path.components().as_path())
}
#[cfg(unix)]
fn has_file(dir: &File, name: &str) -> bool {
    open_at(dir, name, libc::O_RDONLY | libc::O_NONBLOCK)
        .and_then(|file| file.metadata())
        .is_ok_and(|meta| meta.is_file())
}
/// `flock`, which Android has and the standard library's lock does not cover there.
#[cfg(unix)]
fn try_lock(file: &File) -> bool {
    // SAFETY: the descriptor is live through the call.
    unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) == 0 }
}
#[cfg(unix)]
fn entries(dir: &File) -> io::Result<Vec<String>> {
    names(dir)
}
#[cfg(unix)]
fn remove_tree(dir: &File, name: &str) -> io::Result<()> {
    remove(dir, name)
}

#[cfg(windows)]
fn identity_of(dir: &Directory) -> io::Result<(u64, u64)> {
    identity(&dir.0)
}
#[cfg(windows)]
fn child(dir: &Directory, name: &str) -> io::Result<Directory> {
    dir.child(name, false)
}
#[cfg(windows)]
fn make(dir: &Directory, name: &str) -> io::Result<Directory> {
    dir.child(name, true)
}
#[cfg(windows)]
fn lock_file(dir: &Directory, name: &str) -> io::Result<File> {
    dir.file(name, libc::O_RDWR | libc::O_CREAT)
}
#[cfg(windows)]
fn open_lock(dir: &Directory) -> io::Result<File> {
    dir.file(LOCK, libc::O_RDWR)
}
#[cfg(windows)]
fn open_root(path: &Path) -> io::Result<Directory> {
    Directory::root(
        path.to_str()
            .ok_or_else(|| io::Error::other("non-Unicode path"))?,
        false,
    )
}
#[cfg(windows)]
fn has_file(dir: &Directory, name: &str) -> bool {
    dir.kind(name).is_ok_and(|kind| kind.regular)
}
#[cfg(windows)]
fn try_lock(file: &File) -> bool {
    file.try_lock().is_ok()
}
#[cfg(windows)]
fn entries(dir: &Directory) -> io::Result<Vec<String>> {
    dir.names()
}
#[cfg(windows)]
fn remove_tree(dir: &Directory, name: &str) -> io::Result<()> {
    crate::stdlib::windows_fs::remove(dir, name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::stdlib::fs::run;
    use std::{fs, path::PathBuf};

    fn roots(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!(
            "ibex-staging-{name}-{}",
            crate::stdlib::crypto::random_uuid().unwrap()
        ));
        for root in ["data", "cache", "tmp"] {
            fs::create_dir_all(path.join(root)).unwrap();
        }
        path
    }
    fn directories(path: &Path) -> AppDirectories {
        AppDirectories::new(path.join("data"), path.join("cache"), path.join("tmp")).unwrap()
    }
    fn claim(path: &Path) {
        claim_staging(&["data", "cache", "tmp"].map(|root| path.join(root)));
    }
    fn call(
        dirs: &AppDirectories,
        op: FsOp,
        path: &str,
        data: Option<&[u8]>,
    ) -> Result<FsResult, HostError> {
        let grants = GrantSet::parse("fs.read app:/\nfs.write app:/").unwrap();
        run(&grants, Some(dirs), op, path, None, data)
    }
    fn listed(path: PathBuf) -> Vec<String> {
        let mut names: Vec<_> = fs::read_dir(path)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().into_string().unwrap())
            .collect();
        names.sort();
        names
    }
    /// The launch directories in `data`'s staging directory.
    fn launches(path: &Path) -> Vec<String> {
        let mut names = listed(path.join("data").join(STAGING));
        names.retain(|name| name.len() == 24);
        names
    }

    const ENDED: &str = "IBEX_STAGING_ENDED_ROOTS";

    thread_local! {
        /// Reopening this thread's launch directory fails, as on `EMFILE`.
        pub(super) static REOPEN_FAILS: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
    }

    #[test]
    fn a_passing_error_keeps_the_launch_and_its_lock() {
        let path = roots("passing");
        let grants = GrantSet::parse("fs.write app:/data").unwrap();
        let dirs = directories(&path);
        let mut first = dirs.stage(&grants, "app:/data/a").unwrap();
        first.write_all(b"first").unwrap();
        let launch = launches(&path);
        assert_eq!(launch.len(), 1);
        let lock = path.join("data").join(STAGING).join(&launch[0]).join(LOCK);
        REOPEN_FAILS.with(|fails| fails.set(true));
        // The write lands, staged beside its target this once.
        call(&dirs, FsOp::AtomicWriteFile, "app:/data/b", Some(b"b")).unwrap();
        REOPEN_FAILS.with(|fails| fails.set(false));
        // The launch still holds its lock, so no sweep can take `first`.
        let other = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock)
            .unwrap();
        assert!(
            !try_lock(&other),
            "the passing error released the launch's lock"
        );
        drop(other);
        first.publish().unwrap();
        assert_eq!(fs::read(path.join("data/a")).unwrap(), b"first");
        assert_eq!(fs::read(path.join("data/b")).unwrap(), b"b");
        assert_eq!(launches(&path), launch);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn the_sweep_judges_a_lock_it_finds_and_makes_none() {
        let path = roots("judges");
        let staging = path.join("data").join(STAGING);
        // A launch that ended after making its directory, before its lock.
        let unlocked = staging.join("00000000000000000000000a");
        fs::create_dir_all(&unlocked).unwrap();
        fs::write(unlocked.join("left.partial"), b"left").unwrap();
        // A lock that cannot be opened as a file.
        let odd = staging.join("00000000000000000000000b");
        fs::create_dir_all(odd.join(LOCK)).unwrap();
        fs::write(staging.join(NAMESPACE), b"").unwrap();
        claim(&path);
        assert!(!unlocked.exists());
        assert_eq!(listed(odd.clone()), [LOCK]);
        assert!(odd.join(LOCK).is_dir());
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn a_roots_listing_spends_none_of_its_budget_on_the_staging_directory() {
        let path = roots("budget");
        fs::write(path.join("data/a"), b"a").unwrap();
        claim(&path);
        assert!(path.join("data").join(STAGING).is_dir());
        let grants = GrantSet::parse("fs.read app:/data").unwrap();
        let dirs = directories(&path);
        // `["a"]` fits in 16 bytes; with `".exact-staging"` it would not.
        let limits = crate::stdlib::fs::ResultLimits { bytes: 16 };
        let listing = crate::stdlib::fs::run_with_limits(
            &grants,
            Some(&dirs),
            None,
            limits,
            FsOp::ReadDir,
            "app:/data",
            None,
            None,
        );
        assert_eq!(listing.unwrap(), FsResult::Names(vec!["a".into()]));
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn a_root_named_twice_is_claimed_once_and_a_missing_one_is_skipped() {
        let path = roots("once");
        let data = path.join("data");
        let staging = data.join(STAGING);
        fs::create_dir(&staging).unwrap();
        // Another launch holds the namespace lock for longer than a claim
        // asks it.
        let held = fs::File::create(staging.join(NAMESPACE)).unwrap();
        assert!(try_lock(&held));
        let started = std::time::Instant::now();
        claim_staging(&[data.clone(), path.join("absent"), data.clone()]);
        let waited = started.elapsed();
        // One wait of about half a second, not one for each time it is named.
        assert!(waited < std::time::Duration::from_millis(900), "{waited:?}");
        assert!(launches(&path).is_empty());
        drop(held);
        claim_staging(&[data]);
        assert_eq!(launches(&path).len(), 1);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn a_launch_removes_the_staging_files_of_one_that_ended_and_spares_the_rest() {
        if let Some(path) = std::env::var_os(ENDED) {
            // The launch that ends: a download in flight when the process
            // exits without joining it, so no destructor runs.
            let dirs = directories(Path::new(&path));
            let grants = GrantSet::parse("fs.write app:/data").unwrap();
            let mut staged = dirs.stage(&grants, "app:/data/sub/out").unwrap();
            staged.write_all(b"cut off").unwrap();
            std::process::exit(0);
        }
        let path = roots("ended");
        fs::create_dir(path.join("data/sub")).unwrap();
        let test = module_path!().split_once("::").unwrap().1;
        let status = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                &format!("{test}::a_launch_removes_the_staging_files_of_one_that_ended_and_spares_the_rest"),
                "--exact",
            ])
            .env(ENDED, &path)
            .status()
            .unwrap();
        assert!(status.success());
        // What the ended launch left: its file in its own directory, none
        // beside the target.
        let ended = launches(&path);
        assert_eq!(ended.len(), 1);
        let left = path.join("data").join(STAGING).join(&ended[0]);
        let partial: Vec<_> = listed(left.clone())
            .into_iter()
            .filter(|name| name != ".lock")
            .collect();
        assert_eq!(partial.len(), 1);
        assert_eq!(fs::read(left.join(&partial[0])).unwrap(), b"cut off");
        assert!(listed(path.join("data/sub")).is_empty());
        // A launch still running in another process, and a directory that
        // is no launch's.
        let live = path
            .join("data")
            .join(STAGING)
            .join("0123456789abcdef01234567");
        fs::create_dir(&live).unwrap();
        fs::write(live.join("held.partial"), b"live").unwrap();
        let held = fs::File::create(live.join(".lock")).unwrap();
        assert!(try_lock(&held));
        let other = path.join("data").join(STAGING).join("notes");
        fs::create_dir(&other).unwrap();
        fs::write(other.join("kept"), b"kept").unwrap();

        let dirs = directories(&path);
        claim(&path);
        assert!(!left.exists());
        assert_eq!(fs::read(live.join("held.partial")).unwrap(), b"live");
        assert_eq!(fs::read(other.join("kept")).unwrap(), b"kept");
        let now = launches(&path);
        assert_eq!(now.len(), 2);
        assert!(now.iter().any(|name| name == "0123456789abcdef01234567"));
        assert!(!path.join("data/sub/out").exists());

        // The staging directory is no app path, under any case.
        assert_eq!(
            call(&dirs, FsOp::ReadDir, "app:/data", None).unwrap(),
            FsResult::Names(vec!["sub".into()])
        );
        for target in ["app:/data/.exact-staging", "app:/data/.EXACT-STAGING/x"] {
            for op in [FsOp::ReadDir, FsOp::Stat, FsOp::WriteFile, FsOp::Mkdir] {
                assert!(
                    call(&dirs, op, target, Some(b"x")).is_err(),
                    "{op:?} {target}"
                );
            }
        }
        // Below a root, the name is the app's.
        call(&dirs, FsOp::Mkdir, "app:/data/sub/.exact-staging", None).unwrap();
        drop(held);
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn an_apps_own_path_named_like_the_staging_directory_is_left_as_it_is() {
        let path = roots("own");
        // Made before the name was reserved: a folder of the app's, holding
        // a name a launch directory would have, and a file.
        let folder = path.join("data").join(STAGING);
        fs::create_dir_all(folder.join("0123456789abcdef01234567")).unwrap();
        fs::write(folder.join("0123456789abcdef01234567/kept"), b"kept").unwrap();
        fs::write(folder.join("notes"), b"notes").unwrap();
        fs::write(path.join("cache").join(STAGING), b"file").unwrap();
        let dirs = directories(&path);
        claim(&path);
        let grants = GrantSet::parse("fs.write app:/data\nfs.write app:/cache").unwrap();
        let mut staged = dirs.stage(&grants, "app:/data/a").unwrap();
        staged.write_all(b"staged").unwrap();
        staged.publish().unwrap();
        call(
            &dirs,
            FsOp::AtomicWriteFile,
            "app:/cache/b",
            Some(b"atomic"),
        )
        .unwrap();
        // The writes land, staged beside their targets as before.
        assert_eq!(fs::read(path.join("data/a")).unwrap(), b"staged");
        assert_eq!(fs::read(path.join("cache/b")).unwrap(), b"atomic");
        assert_eq!(listed(path.join("data")), [STAGING, "a"]);
        assert_eq!(listed(path.join("cache")), [STAGING, "b"]);
        // Nothing of the app's is changed, and its path stays refused.
        assert_eq!(
            listed(folder.clone()),
            ["0123456789abcdef01234567", "notes"]
        );
        assert_eq!(
            fs::read(folder.join("0123456789abcdef01234567/kept")).unwrap(),
            b"kept"
        );
        assert_eq!(fs::read(path.join("cache").join(STAGING)).unwrap(), b"file");
        assert!(call(
            &dirs,
            FsOp::ReadFile,
            "app:/data/.exact-staging/notes",
            None
        )
        .is_err());
        let _ = fs::remove_dir_all(path);
    }

    #[test]
    fn a_launch_keeps_its_own_staging_files_when_its_storage_is_configured_again() {
        let path = roots("again");
        let grants = GrantSet::parse("fs.write app:/data").unwrap();
        let first = directories(&path);
        let mut staged = first.stage(&grants, "app:/data/a").unwrap();
        staged.write_all(b"first ").unwrap();
        // A development reload or an applied update activates storage again
        // while the first executor's workers may still be writing.
        let again = directories(&path);
        claim(&path);
        call(
            &again,
            FsOp::AtomicWriteFile,
            "app:/data/b",
            Some(b"atomic"),
        )
        .unwrap();
        staged.write_all(b"whole").unwrap();
        staged.publish().unwrap();
        assert_eq!(fs::read(path.join("data/a")).unwrap(), b"first whole");
        assert_eq!(fs::read(path.join("data/b")).unwrap(), b"atomic");
        let launch = launches(&path);
        assert_eq!(launch.len(), 1);
        let own = path.join("data").join(STAGING).join(&launch[0]);
        assert_eq!(listed(own), [".lock"]);
        assert_eq!(listed(path.join("data")), [STAGING, "a", "b"]);

        // A staging directory removed from outside is made again.
        fs::remove_dir_all(path.join("data").join(STAGING)).unwrap();
        call(&again, FsOp::AtomicWriteFile, "app:/data/b", Some(b"again")).unwrap();
        assert_eq!(fs::read(path.join("data/b")).unwrap(), b"again");
        assert_eq!(launches(&path).len(), 1);
        let _ = fs::remove_dir_all(path);
    }
}
