use super::*;

/// A host's current folder for a role, preserving the platform's I/O errors.
pub type KnownFolderResolver =
    std::sync::Arc<dyn Fn(&str) -> std::io::Result<KnownFolder> + Send + Sync>;

/// A platform path, or a folder already opened through the protected walk.
pub struct KnownFolder {
    pub path: PathBuf,
    pub directory: Option<std::fs::File>,
}

impl From<PathBuf> for KnownFolder {
    fn from(path: PathBuf) -> Self {
        Self {
            path,
            directory: None,
        }
    }
}

impl From<(PathBuf, std::fs::File)> for KnownFolder {
    fn from((path, directory): (PathBuf, std::fs::File)) -> Self {
        Self {
            path,
            directory: Some(directory),
        }
    }
}

/// Host roots for home and bundle paths (LLP 1133.007 D6) and a per-use
/// resolver for well-known folders (LLP 1173 D4).
#[derive(Clone, Default)]
pub struct Roots {
    pub home: Option<PathBuf>,
    pub bundle: Option<PathBuf>,
    /// Resolved on every use; no resolver means this host has no known folders.
    pub known_folder: Option<KnownFolderResolver>,
}

impl std::fmt::Debug for Roots {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Roots")
            .field("home", &self.home)
            .field("bundle", &self.bundle)
            .field("known_folder", &self.known_folder.is_some())
            .finish()
    }
}

/// LLP 1173 D4: these namespaces allow only creation through fs.write.
pub fn is_known_folder(path: &str) -> bool {
    known_parts(path).is_some()
}

fn known_parts(path: &str) -> Option<(&'static str, &str)> {
    [
        "desktop",
        "documents",
        "downloads",
        "music",
        "pictures",
        "videos",
        "screenshots",
    ]
    .into_iter()
    .find_map(|name| Some((name, path.strip_prefix(name)?.strip_prefix(":/")?)))
}

fn parts(rest: &str) -> Result<Vec<String>, HostError> {
    rest.split('/')
        .filter(|part| !part.is_empty())
        .map(|part| {
            if matches!(part, "." | "..") || part.contains('\0') {
                Err(HostError::Failed(
                    "TypeError: path has a . or .. part or NUL".into(),
                ))
            } else {
                Ok(part.to_owned())
            }
        })
        .collect()
}

const UNSUPPORTED: &str = "NotSupportedError: well-known folders are unavailable on this host";

/// A known-folder path as named now, also used to exclude writable PATH programs.
pub fn resolve_known_folder(path: &str) -> Result<PathBuf, HostError> {
    let roots = ROOTS.read().unwrap_or_else(|e| e.into_inner()).clone();
    roots.resolve_known_folder(path)
}

impl Roots {
    /// Protect the resolver from changes the app can make through its other
    /// write grants. Call before app code runs; storage and PATH checks share it.
    #[cfg(unix)]
    pub fn guard_known_folders(
        &mut self,
        grants: &GrantSet,
        settings: &[(&'static str, PathBuf)],
        app_roots: &[PathBuf],
    ) -> Option<KnownFolderGuard> {
        let resolve = self.known_folder.take()?;
        let guard = KnownFolderGuard(std::sync::Arc::new(std::sync::Mutex::new(
            FolderProtection {
                resolve,
                home: self.home.clone(),
                entries: Vec::new(),
                settings: Vec::new(),
                refused: Default::default(),
            },
        )));
        guard.protect(grants, settings, app_roots);
        let shared = guard.clone();
        self.known_folder = Some(std::sync::Arc::new(move |name| {
            let mut protection = shared.0.lock().unwrap_or_else(|e| e.into_inner());
            protection.refuse_linked_settings();
            if protection.refused.contains(name) {
                return Err(std::io::Error::from_raw_os_error(libc::EACCES));
            }
            let folder = (protection.resolve)(name)?;
            if folder.directory.is_some() {
                return Ok(folder);
            }
            let (directory, path) = super::known_directory(&folder.path, &protection.entries)?;
            Ok((path, directory).into())
        }));
        Some(guard)
    }

    /// Resolve a write line using these same host roots, without opening its leaf.
    pub fn resolve_known_folder(&self, path: &str) -> Result<PathBuf, HostError> {
        let (name, rest) =
            known_parts(path).ok_or_else(|| HostError::InvalidArgument(path.into()))?;
        resolve_in(self, name, &parts(rest)?)
    }
}

/// Protection shared by every session using the process's folder resolver.
#[cfg(unix)]
#[derive(Clone)]
pub struct KnownFolderGuard(std::sync::Arc<std::sync::Mutex<FolderProtection>>);

#[cfg(unix)]
struct FolderProtection {
    resolve: KnownFolderResolver,
    home: Option<PathBuf>,
    entries: Vec<PathBuf>,
    settings: Vec<(&'static str, PathBuf)>,
    refused: std::collections::HashSet<&'static str>,
}

#[cfg(unix)]
impl FolderProtection {
    fn refuse_linked_settings(&mut self) {
        use std::os::unix::fs::MetadataExt;
        for (name, path) in &self.settings {
            // Other hard-linked names may lie inside a write grant; their
            // locations cannot be established from this path.
            if std::fs::metadata(path).is_ok_and(|meta| meta.nlink() > 1) {
                self.refused.insert(name);
            }
        }
    }
}

#[cfg(unix)]
impl KnownFolderGuard {
    /// Add a session's write access before its app code runs. Each settings
    /// path names the role it controls; app-writable or hard-linked settings are
    /// never trusted, including at launch, when a previous process may have
    /// written them.
    pub fn protect(
        &self,
        grants: &GrantSet,
        settings: &[(&'static str, PathBuf)],
        app_roots: &[PathBuf],
    ) {
        let mut protection = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let mut entries = app_roots.to_vec();
        for grant in grants.iter() {
            let Grant::FsWrite(prefix) = grant else {
                continue;
            };
            let Some((ns, parts)) = prefix.components().split_first() else {
                continue;
            };
            let base = match ns.as_str() {
                "" => Some(PathBuf::from("/")),
                "home:" => protection.home.clone(),
                _ => None,
            };
            if let Some(base) = base {
                let path = parts.iter().fold(base, |at, part| at.join(part));
                entries.push(path);
            }
        }
        entries.extend(
            entries
                .clone()
                .iter()
                .filter_map(|path| Some(super::realize(path.parent()?).join(path.file_name()?))),
        );
        protection.entries.extend(entries);
        protection.entries.sort();
        protection.entries.dedup();
        protection.settings.extend_from_slice(settings);
        protection.settings.sort();
        protection.settings.dedup();
        protection.refuse_linked_settings();
        for (name, path) in settings {
            if protection.entries.iter().any(|prefix| {
                path.starts_with(prefix) || super::realize(path).starts_with(super::realize(prefix))
            }) {
                protection.refused.insert(name);
            }
        }
    }
}

fn resolve_in(roots: &Roots, name: &str, parts: &[String]) -> Result<PathBuf, HostError> {
    let resolver = roots
        .known_folder
        .as_ref()
        .ok_or_else(|| HostError::Failed(UNSUPPORTED.into()))?;
    let root = resolve_root(resolver, name)?.path;
    Ok(parts.iter().fold(root, |at, part| at.join(part)))
}

fn resolve_root(resolver: &KnownFolderResolver, name: &str) -> Result<KnownFolder, HostError> {
    let root = resolver(name).map_err(|e| HostError::Failed(format!("{name}: {e}")))?;
    if !root.path.is_absolute() {
        return Err(HostError::InvalidArgument(root.path.display().to_string()));
    }
    Ok(root)
}

static ROOTS: std::sync::RwLock<Roots> = std::sync::RwLock::new(Roots {
    home: None,
    bundle: None,
    known_folder: None,
});

/// Install the host's roots and folder resolver, once a process.
pub fn set_roots(roots: Roots) {
    *ROOTS.write().unwrap_or_else(|e| e.into_inner()) = roots;
}

/// A namespace's name, its root as the host named it, and a path's
/// components below that root.
#[cfg(unix)]
type Namespaced = (&'static str, KnownFolder, Vec<String>);

/// A host namespace's root and its components below it; `None`
/// for any other path. A `.` or `..` component is refused.
#[cfg(unix)]
fn namespaced(path: &str) -> Option<Result<Namespaced, HostError>> {
    let (namespace, rest) = match path.strip_prefix("home:/") {
        Some(rest) => ("home", rest),
        None => match path.strip_prefix("bundle:/") {
            Some(rest) => ("bundle", rest),
            None => {
                let (name, rest) = known_parts(path)?;
                return Some((|| {
                    let roots = ROOTS.read().unwrap_or_else(|e| e.into_inner());
                    let resolver = roots
                        .known_folder
                        .as_ref()
                        .ok_or_else(|| HostError::Failed(UNSUPPORTED.into()))?;
                    let root = resolve_root(resolver, name)?;
                    Ok((name, root, parts(rest)?))
                })());
            }
        },
    };
    let roots = ROOTS.read().unwrap_or_else(|e| e.into_inner());
    let root = match namespace {
        "home" => roots.home.clone(),
        _ => roots.bundle.clone(),
    };
    Some((|| {
        let root = root.ok_or_else(|| {
            HostError::Failed(format!("unsupported: no {namespace}: directory here"))
        })?;
        let parts: Vec<String> = (rest.split('/').filter(|part| !part.is_empty()))
            .map(str::to_owned)
            .collect();
        if parts
            .iter()
            .any(|part| part == ".." || part == "." || part.contains('\0'))
        {
            return Err(HostError::Failed(format!(
                "TypeError: path has a . or .. part: {path}"
            )));
        }
        Ok((namespace, root.into(), parts))
    })())
}

/// `path` below its root's retained descriptor, each component to be opened
/// `O_NOFOLLOW` (`app_fs::run_below`).
#[cfg(unix)]
pub(super) fn below(path: &str) -> Result<(crate::stdlib::app_fs::Below, PathBuf), HostError> {
    let (name, root, parts) = namespaced(path).ok_or_else(|| {
        HostError::Failed(format!("unsupported: not a home: or bundle: path: {path}"))
    })??;
    let mut options = std::fs::OpenOptions::new();
    options.read(true);
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC);
    }
    let failed = |e: std::io::Error| HostError::Failed(format!("{path}: {e}"));
    // A protected folder's descriptor must survive replacement of its name.
    let (opened, native) = match root.directory {
        Some(directory) => (directory, root.path),
        None => (
            options.open(&root.path).map_err(failed)?,
            std::fs::canonicalize(&root.path).map_err(failed)?,
        ),
    };
    let native = native.join(parts.join("/"));
    Ok((
        crate::stdlib::app_fs::Below {
            root: opened,
            parts,
            name,
        },
        native,
    ))
}

/// A `home:` or `bundle:` operation (exact2 LLP 1133.007 D6, Exact patch 24):
/// admitted as spelt, a `bundle:` path read only, then run below its root's
/// retained descriptor with every component opened `O_NOFOLLOW`, as an
/// `app:/` path is. A link anywhere below the root, one a grant's holder
/// planted included, is refused rather than followed, so the spelt path is
/// the one the operation reaches and admission needs no realized pass.
pub(super) fn run_namespaced(
    grants: &GrantSet,
    op: FsOp,
    path: &str,
    destination: Option<&str>,
    data: Option<&[u8]>,
) -> Result<FsResult, HostError> {
    let namespace = |p: &str| p.starts_with("home:/") || p.starts_with("bundle:/");
    if !namespace(path) || destination.is_some_and(|d| !namespace(d)) {
        return Err(HostError::Failed(
            "unsupported: a home: or bundle: path pairs only with another".into(),
        ));
    }
    // A `.` or `..` part is refused before any grant is read.
    #[cfg(unix)]
    for p in [Some(path), destination].into_iter().flatten() {
        if let Some(Err(e)) = namespaced(p) {
            return Err(e);
        }
    }
    let (needs_read, needs_write) = op.required();
    // What is written: a one-path operation's path, a two-path one's
    // destination, and a rename's source, which it removes.
    let written = match destination {
        Some(to) => [Some(to), (op == FsOp::Rename).then_some(path)],
        None => [(needs_write && !needs_read).then_some(path), None],
    };
    if let Some(spelled) = written
        .into_iter()
        .flatten()
        .find(|p| p.starts_with("bundle:/"))
    {
        return Err(HostError::Failed(format!(
            "denied: bundle: is read only: {spelled}"
        )));
    }
    admit_as(grants, op, Path::new(path), destination.map(Path::new))?;
    #[cfg(unix)]
    {
        let (source, native) = below(path)?;
        if op == FsOp::Realpath {
            source.open()?;
            return Ok(FsResult::Text(native.display().to_string()));
        }
        let target = destination.map(below).transpose()?.map(|(b, _)| b);
        crate::stdlib::app_fs::run_below(op, source, target, data.unwrap_or(&[]))
    }
    #[cfg(not(unix))]
    {
        let _ = data;
        Err(HostError::Failed(format!("unsupported: no {path} here")))
    }
}

/// A grant line's directory as a native path: a native line as spelt, a
/// namespaced line below its current host root; `None` for any other.
#[cfg(unix)]
pub(super) fn line_native(line: &PathPrefix) -> Option<PathBuf> {
    let (namespace, parts) = line.components().split_first()?;
    let root = match namespace.as_str() {
        "" => PathBuf::from("/"),
        "home:" => (ROOTS.read().unwrap_or_else(|e| e.into_inner()).home.clone())?,
        _ => return resolve_known_folder(&format!("{namespace}/{}", parts.join("/"))).ok(),
    };
    Some(parts.iter().fold(root, |at, part| at.join(part)))
}

/// Both paths face the allowlist before dispatch, including computed paths.
pub(super) fn run_known(
    grants: &GrantSet,
    directories: Option<&crate::stdlib::app_fs::AppDirectories>,
    op: FsOp,
    path: &str,
    destination: Option<&str>,
    data: Option<&[u8]>,
) -> Result<FsResult, HostError> {
    // Unsupported hosts use one refusal for every operation on these namespaces.
    if ROOTS
        .read()
        .unwrap_or_else(|e| e.into_inner())
        .known_folder
        .is_none()
    {
        return Err(HostError::Failed(UNSUPPORTED.into()));
    }
    let copy = op == FsOp::CopyFile
        && path.starts_with("app:/")
        && destination.is_some_and(is_known_folder);
    let single = is_known_folder(path)
        && destination.is_none()
        && matches!(
            op,
            FsOp::WriteFile | FsOp::AtomicWriteFile | FsOp::Mkdir | FsOp::Realpath
        );
    if !copy && !single {
        return Err(HostError::Failed(
            "denied: well-known folders only create files".into(),
        ));
    }
    let target = destination.unwrap_or(path);
    crate::boundary::admit(
        grants,
        &Operation::FsWrite {
            path: target.into(),
        },
    )?;
    #[cfg(unix)]
    {
        let (target, native) = below(target)?;
        if op == FsOp::Realpath {
            target.exists()?;
            return Ok(FsResult::Text(native.display().to_string()));
        }
        let source = if copy {
            Some(super::open_read(grants, directories, path)?)
        } else {
            None
        };
        crate::stdlib::app_fs::create_below(op, target, source, data.unwrap_or_default())
    }
    #[cfg(windows)]
    {
        let (name, rest) =
            known_parts(target).ok_or_else(|| HostError::InvalidArgument(target.into()))?;
        let parts = parts(rest)?;
        for part in &parts {
            super::super::windows_path::validate_name(part)
                .map_err(|e| HostError::Failed(e.to_string()))?;
        }
        let resolver = ROOTS
            .read()
            .unwrap_or_else(|e| e.into_inner())
            .known_folder
            .clone()
            .ok_or_else(|| HostError::Failed(UNSUPPORTED.into()))?;
        let root = resolve_root(&resolver, name)?;
        let source = if copy {
            Some(super::open_read(grants, directories, path)?)
        } else {
            None
        };
        super::super::fs_known_windows::create(root, &parts, op, source, data.unwrap_or_default())
            .map_err(|e| HostError::Failed(format!("{target}: {e}")))
    }
    #[cfg(not(any(unix, windows)))]
    {
        let _ = (directories, data);
        Err(HostError::Failed(UNSUPPORTED.into()))
    }
}

/// Preserve errno when reusing a resolver failure.
pub fn copy_io_error(error: &std::io::Error) -> std::io::Error {
    error.raw_os_error().map_or_else(
        || std::io::Error::new(error.kind(), error.to_string()),
        std::io::Error::from_raw_os_error,
    )
}
