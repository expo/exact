//! A Flatpak sandbox as drags meet it (LLP 1094.000 D13, LLP 1094.001 D8):
//! whether this process runs in one, its app's cache as the host spells
//! it, and which paths another app lists name the host's own files here.

use std::collections::BTreeSet;
use std::ffi::OsString;
use std::os::unix::ffi::OsStrExt;
use std::path::{Component, Path, PathBuf};

/// The trees a sandbox keeps as its own beneath a `host` share, at the
/// host's paths but not the host's files. Measured under Flatpak 1.16 with
/// `filesystems=host`: `/app`, `/bin`, `/dev`, `/etc`, `/lib`, `/proc`,
/// `/run`, `/sbin`, `/sys`, `/tmp`, `/usr` and `/var` were the sandbox's;
/// the rest of `/` was the host's. Flatpak reserves the others named here
/// the same way.
const OWN: [&str; 17] = [
    "/.flatpak-info",
    "/app",
    "/bin",
    "/boot",
    "/dev",
    "/etc",
    "/lib",
    "/lib32",
    "/lib64",
    "/proc",
    "/root",
    "/run",
    "/sbin",
    "/sys",
    "/tmp",
    "/usr",
    "/var",
];

/// The trees a share by path never gives the host's files in: Flatpak 1.16
/// refused `/bin`, `/dev`, `/etc`, `/lib`, `/proc`, `/sbin` and `/usr` ("is
/// reserved by Flatpak") though `/.flatpak-info` still lists such a share,
/// and the others are the sandbox's own mounts. A share of `/tmp`,
/// `/var/tmp` or `/var/lib` gave the host's own.
const RESERVED: [&str; 14] = [
    "/.flatpak-info",
    "/app",
    "/bin",
    "/dev",
    "/etc",
    "/lib",
    "/lib32",
    "/lib64",
    "/proc",
    "/run/flatpak",
    "/run/host",
    "/sbin",
    "/sys",
    "/usr",
];

/// Whether this process runs in a Flatpak sandbox, asked once.
pub(super) fn sandboxed() -> bool {
    static SANDBOXED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *SANDBOXED.get_or_init(|| Path::new("/.flatpak-info").exists())
}

/// The app's cache as the host spells it, `~/.var/app/<id>/cache/drag-out`,
/// made if missing: a path that names the same folder inside the sandbox
/// and on the host. `$XDG_CACHE_HOME` is not used, since an app may set it
/// to `/var/cache`, the host's own outside.
pub(super) fn cache() -> Option<PathBuf> {
    let id = std::env::var("FLATPAK_ID").ok().filter(|id| single(id))?;
    let cache = PathBuf::from(std::env::var_os("HOME")?)
        .join(".var/app")
        .join(id)
        .join("cache/drag-out");
    std::fs::create_dir_all(&cache).ok()?;
    Some(cache)
}

/// Where a sandbox shows the host's own files at the host's own paths
/// (LLP 1094.001 D8): beneath the document portal's mount, the app's own
/// `~/.var/app/<id>`, or a folder its Flatpak permissions share
/// (`filesystems=` under `[Context]` in `/.flatpak-info`; `host` shares all
/// of `/` but the trees in [`OWN`]), but not within a folder the sandbox
/// persists, which it keeps as its own there.
#[derive(Debug, Default, PartialEq)]
pub(super) struct Shares {
    docs: Vec<PathBuf>,
    roots: Vec<PathBuf>,
    host: bool,
    persisted: Vec<PathBuf>,
}

/// What a sandbox's [`Shares`] are read from.
pub(super) struct Context<'a> {
    pub info: &'a str,
    pub home: &'a Path,
    pub runtime: &'a Path,
    pub app: Option<&'a str>,
    /// The person's folders (`user-dirs.dirs`, which Flatpak gives the
    /// sandbox with each path written out).
    pub user_dirs: &'a str,
    /// The document portal's mount, each way it is spelled: in a sandbox
    /// `$XDG_RUNTIME_DIR/doc` links to `/run/flatpak/doc`.
    pub docs: Vec<PathBuf>,
}

impl Shares {
    /// This sandbox's shares, from `/.flatpak-info` and its environment.
    pub(super) fn here() -> Shares {
        let var = |name| std::env::var_os(name).map(PathBuf::from);
        let home = var("HOME").unwrap_or_default();
        // SAFETY: getuid(2) takes nothing and always succeeds.
        let runtime = var("XDG_RUNTIME_DIR")
            .unwrap_or_else(|| PathBuf::from(format!("/run/user/{}", unsafe { libc::getuid() })));
        let config = var("XDG_CONFIG_HOME").unwrap_or_else(|| home.join(".config"));
        let doc = runtime.join("doc");
        let read = |path: &Path| std::fs::read_to_string(path).unwrap_or_default();
        let app = std::env::var("FLATPAK_ID").ok();
        Shares::of(&Context {
            info: &read(Path::new("/.flatpak-info")),
            home: &home,
            runtime: &runtime,
            app: app.as_deref(),
            user_dirs: &read(&config.join("user-dirs.dirs")),
            docs: std::fs::canonicalize(&doc)
                .into_iter()
                .chain([doc])
                .collect(),
        })
    }

    pub(super) fn of(context: &Context<'_>) -> Shares {
        let mut shares = Shares {
            docs: context.docs.clone(),
            ..Shares::default()
        };
        if let Some(app) = context.app.filter(|app| single(app)) {
            shares.roots.push(context.home.join(".var/app").join(app));
        }
        let mut host = false;
        let mut shared: Vec<(&str, PathBuf)> = Vec::new();
        for entry in context_list(context.info, "filesystems") {
            let entry = entry
                .strip_suffix(":ro")
                .or_else(|| entry.strip_suffix(":rw"))
                .or_else(|| entry.strip_suffix(":create"))
                .unwrap_or(entry);
            if let Some(cancelled) = entry.strip_prefix('!') {
                shared.retain(|(name, _)| *name != cancelled);
                host &= cancelled != "host";
            } else if entry == "host" {
                host = true;
            } else if let Some(root) = root_of(entry, context) {
                shared.push((entry, root));
            }
        }
        shares.host = host;
        shares
            .roots
            .extend(shared.into_iter().map(|(_, root)| root));
        let persisted = context_list(context.info, "persistent");
        shares.persisted = persisted
            .filter(|f| relative(f))
            .map(|f| context.home.join(f))
            .collect();
        shares
    }

    /// Whether `path` names the host's own file.
    fn shares(&self, path: &Path) -> bool {
        !self.holding(path).is_empty()
    }

    /// The shares that hold `path`, each by its place: the document portal's
    /// mount, under each of its spellings, is one share (0), each folder
    /// shared by permission another, and the host's ([`HOST`]) holds all
    /// but the sandbox's own trees; nothing but the document mount holds a
    /// folder the sandbox persists.
    fn holding(&self, path: &Path) -> BTreeSet<usize> {
        let mut holding = BTreeSet::new();
        if self.docs.iter().any(|root| path.starts_with(root)) {
            holding.insert(0);
        }
        if !self.persisted.iter().any(|folder| path.starts_with(folder)) {
            let roots = self.roots.iter().enumerate();
            holding.extend(
                roots
                    .filter(|(_, root)| path.starts_with(root))
                    .map(|(n, _)| n + 1),
            );
            if self.host && !OWN.iter().any(|own| path.starts_with(own)) {
                holding.insert(HOST);
            }
        }
        holding
    }

    /// How `path` stands with the shares: held by some of them; on the way
    /// to some of them; or neither. The host's share leads from `/`.
    fn step(&self, path: &Path) -> Step {
        let holding = self.holding(path);
        if !holding.is_empty() {
            return Step::Shared(holding);
        }
        let docs = self
            .docs
            .iter()
            .any(|root| root.starts_with(path))
            .then_some(0);
        let roots = self.roots.iter().enumerate();
        let mut leads: BTreeSet<usize> = docs
            .into_iter()
            .chain(
                roots
                    .filter(|(_, root)| root.starts_with(path))
                    .map(|(n, _)| n + 1),
            )
            .collect();
        if self.host && path == Path::new("/") {
            leads.insert(HOST);
        }
        match leads.is_empty() {
            true => Step::Refused,
            false => Step::Toward(leads),
        }
    }

    /// The host's own file `listed` names, as this sandbox sees it: the
    /// path resolved one link at a time, as Flatpak checks a path it
    /// exports, each step shared or on the way to the share the walk heads
    /// for, and the last shared; none when it is not, or when a part cannot
    /// be read.
    pub(super) fn resolve(&self, listed: &Path) -> Option<PathBuf> {
        resolve(listed, &|path| self.step(path)).filter(|path| self.shares(path))
    }
}

/// Where a `filesystems=` entry other than `host` shows the host's files,
/// as Flatpak maps it (flatpak-metadata(5)); none for one that shows them
/// elsewhere (`host-os`, `host-etc`), names a reserved tree, or names the
/// home itself through a person's folder.
fn root_of(entry: &str, context: &Context<'_>) -> Option<PathBuf> {
    let (name, rest) = entry.split_once('/').unwrap_or((entry, ""));
    let under = |base: PathBuf| match rest {
        "" => Some(base),
        rest => relative(rest).then(|| base.join(rest)),
    };
    let root = match name {
        "home" if rest.is_empty() => Some(context.home.to_path_buf()),
        "~" => under(context.home.to_path_buf()),
        "" => relative(rest).then(|| Path::new("/").join(rest)),
        "xdg-config" => under(context.home.join(".config")),
        "xdg-data" => under(context.home.join(".local/share")),
        "xdg-cache" => under(context.home.join(".cache")),
        "xdg-run" if !rest.is_empty() => under(context.runtime.to_path_buf()),
        _ => {
            let folder = name.strip_prefix("xdg-")?;
            let dir = user_dir(context, folder).filter(|dir| dir != context.home)?;
            under(dir)
        }
    }?;
    (!RESERVED.iter().any(|reserved| root.starts_with(reserved)) && root != Path::new("/"))
        .then_some(root)
}

/// The person's folder for `xdg-<folder>` (`desktop`, `download`,
/// `public-share` and the others), as `user-dirs.dirs` names it; without
/// one, the desktop is `~/Desktop` and the others are none, as GLib has it.
fn user_dir(context: &Context<'_>, folder: &str) -> Option<PathBuf> {
    let key = format!("XDG_{}_DIR=", folder.replace('-', "").to_ascii_uppercase());
    let named = context.user_dirs.lines().find_map(|line| {
        let value = line
            .trim()
            .strip_prefix(&key)?
            .trim()
            .strip_prefix('"')?
            .strip_suffix('"')?;
        match value.strip_prefix("$HOME") {
            Some(rest) => Some(context.home.join(rest.trim_start_matches('/'))),
            None => Path::new(value).is_absolute().then(|| PathBuf::from(value)),
        }
    });
    named.or_else(|| (folder == "desktop").then(|| context.home.join("Desktop")))
}

/// The entries of `key=` under `[Context]` in `info`.
fn context_list<'a>(info: &'a str, key: &'a str) -> impl Iterator<Item = &'a str> + 'a {
    let mut context = false;
    info.lines().map(str::trim).flat_map(move |line| {
        if line.starts_with('[') {
            context = line == "[Context]";
        }
        let list = line
            .strip_prefix(key)
            .and_then(|rest| rest.strip_prefix('='))
            .filter(|_| context)
            .unwrap_or("");
        list.split(';').filter(|entry| !entry.is_empty())
    })
}

/// Whether `path` is relative and only names folders down from where it
/// starts.
fn relative(path: &str) -> bool {
    Path::new(path)
        .components()
        .all(|part| matches!(part, Component::Normal(_)))
}

/// Whether `name` is one path component.
fn single(name: &str) -> bool {
    let mut parts = Path::new(name).components();
    matches!(parts.next(), Some(Component::Normal(_))) && parts.next().is_none()
}

/// How a step of a path stands with a sandbox's shares: shared by some of
/// them, a folder on the way to some of them (each by its place, as
/// [`Shares::step`] numbers them), or neither.
#[derive(Clone, Debug, PartialEq)]
enum Step {
    Shared(BTreeSet<usize>),
    Toward(BTreeSet<usize>),
    Refused,
}

/// The host's share in a [`Step`]: all of `/` but the sandbox's own trees.
const HOST: usize = usize::MAX;

/// `listed` resolved one link at a time, at most 40 as Linux follows, each
/// step as `step` has it: none when it is relative, a step is refused, or a
/// part cannot be read. A folder that is only on the way to shares is
/// passed only toward them: from it, by a name or by `..`, the walk goes on
/// only to a step held by, or on the way to, one of those same shares. As
/// Linux has it, nothing is below a file, and a trailing `/` or `/.` names a
/// folder.
fn resolve(listed: &Path, step: &dyn Fn(&Path) -> Step) -> Option<PathBuf> {
    if !listed.is_absolute() {
        return None;
    }
    let mut todo: Vec<OsString> = Vec::new();
    push(&mut todo, listed);
    let mut at = PathBuf::from("/");
    let mut folder = true;
    // While at a folder only on the way to shares, the shares the walk may still reach
    let mut toward: Option<BTreeSet<usize>> = None;
    let mut links = 0;
    while let Some(part) = todo.pop() {
        if !folder {
            return None;
        }
        let next = match part.as_bytes() {
            b"." => continue,
            b".." => at.parent().unwrap_or(&at).to_path_buf(),
            _ => at.join(&part),
        };
        toward = match step(&next) {
            Step::Refused => return None,
            Step::Shared(holding) => {
                if toward.as_ref().is_some_and(|was| holding.is_disjoint(was)) {
                    return None;
                }
                None
            }
            Step::Toward(leads) => {
                let still: BTreeSet<usize> = match &toward {
                    Some(was) => leads.intersection(was).copied().collect(),
                    None => leads,
                };
                if still.is_empty() {
                    return None;
                }
                Some(still)
            }
        };
        if part == ".." {
            at = next;
            continue;
        }
        let found = std::fs::symlink_metadata(&next).ok()?;
        if !found.is_symlink() {
            folder = found.is_dir();
            at = next;
            continue;
        }
        links += 1;
        let target = std::fs::read_link(&next).ok().filter(|_| links <= 40)?;
        if target.is_absolute() {
            at = PathBuf::from("/");
        }
        push(&mut todo, &target);
    }
    Some(at)
}

/// Push `path`'s parts on `todo`, the first last, so it is taken first; a
/// trailing `/` or `/.`, which `Path::components` drops, adds a last part
/// `.`, which only a folder takes.
fn push(todo: &mut Vec<OsString>, path: &Path) {
    let bytes = path.as_os_str().as_bytes();
    if bytes.len() > 1 && (bytes.ends_with(b"/") || bytes.ends_with(b"/.")) {
        todo.push(".".into());
    }
    for part in path.components().rev() {
        match part {
            Component::Normal(name) => todo.push(name.to_owned()),
            Component::ParentDir => todo.push("..".into()),
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "sandbox_tests.rs"]
mod tests;
