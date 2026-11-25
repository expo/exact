//! Where an app file is written before it is put in place: a directory per
//! launch in each app root, `<root>/.exact-staging/<launch>/`, holding an
//! exclusive lock on its `.lock` for the life of the process that made it
//! (Exact local patch 54). A process that ends during a write, by an exit
//! that does not join its workers, a kill or a crash, never removes its
//! staging file, so a launch's first use of a root removes every launch
//! directory there whose lock no live process holds. Making a launch
//! directory and sweeping both hold the staging directory's own `.lock`, so
//! a sweep never sees a launch directory before its lock. The image spool
//! (`spool_dir` in Exact's `host/painted/src/image/remote.rs`) works the
//! same way.
use super::*;
use std::sync::Mutex;

/// The directory in each app root that holds the launch directories. It is
/// no app path: `parse` refuses it, and a root's listing leaves it out.
pub(super) const STAGING: &str = ".exact-staging";

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

/// A root's listing, without the staging directory.
pub(super) fn unlisted(result: FsResult, root: bool) -> FsResult {
    match result {
        FsResult::Names(mut names) if root => {
            names.retain(|name| !reserved(name));
            FsResult::Names(names)
        }
        result => result,
    }
}

impl AppDirectories {
    /// This launch's staging directory in the root `index` names, made at
    /// the launch's first use of that root.
    pub(super) fn staging(&self, index: usize) -> Result<Dir, HostError> {
        #[cfg(unix)]
        let root = self.root(index)?;
        #[cfg(windows)]
        let root = &self.roots[index];
        launch(&root).map_err(error)
    }

    /// Make this launch's staging directory in each root now, removing those
    /// of launches that have ended. A root's first staged or atomic write
    /// does it otherwise.
    pub fn claim_staging(&self) {
        for index in 0..3 {
            let _ = self.staging(index);
        }
    }
}

fn launch(root: &Dir) -> io::Result<Dir> {
    let mut launches = LAUNCHES.lock().unwrap_or_else(|e| e.into_inner());
    let id = identity_of(root)?;
    if let Some(at) = launches.iter().position(|launch| launch.root == id) {
        // Made again if it is gone, or if another directory took the
        // identity of a root that was removed.
        let found = child(root, STAGING).and_then(|staging| child(&staging, &launches[at].name));
        match found {
            Ok(dir) if identity_of(&dir)? == launches[at].dir => return Ok(dir),
            _ => drop(launches.remove(at)),
        }
    }
    let staging = make(root, STAGING)?;
    let namespace = lock_file(&staging)?;
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
    let lock = lock_file(&dir)?;
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
        let free = lock_file(&dir).is_ok_and(|lock| try_lock(&lock));
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
fn lock_file(dir: &File) -> io::Result<File> {
    open_at(dir, ".lock", libc::O_RDWR | libc::O_CREAT)
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
fn lock_file(dir: &Directory) -> io::Result<File> {
    dir.file(".lock", libc::O_RDWR | libc::O_CREAT)
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
        dirs.claim_staging();
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
    fn a_launch_keeps_its_own_staging_files_when_its_storage_is_configured_again() {
        let path = roots("again");
        let grants = GrantSet::parse("fs.write app:/data").unwrap();
        let first = directories(&path);
        let mut staged = first.stage(&grants, "app:/data/a").unwrap();
        staged.write_all(b"first ").unwrap();
        // A development reload or an applied update activates storage again
        // while the first executor's workers may still be writing.
        let again = directories(&path);
        again.claim_staging();
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
