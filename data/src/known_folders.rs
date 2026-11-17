//! The person's well-known folders by role (LLP 1173 D4's table): where the
//! platform says each is. Pickers ask at every use; storage refuses roles
//! whose settings the app can write. Used by picker startIn and create-only writes.

use std::path::{Path, PathBuf};

#[path = "known_folders_windows.rs"]
mod windows;
pub use windows::windows_id;
#[cfg(windows)]
pub use windows::windows_storage;

/// Where the well-known folder `name` is on a Linux desktop: its
/// `XDG_<NAME>_DIR` in `user-dirs.dirs` under `$XDG_CONFIG_HOME` (else
/// `~/.config`), and `screenshots` the pictures folder's `Screenshots`,
/// untranslated, where GNOME Shell saves. `None` for a name that is not
/// one, an entry that is unset, relative or the home directory itself
/// (XDG's way of disabling a folder), or a folder that does not exist.
pub fn linux(name: &str) -> Option<PathBuf> {
    linux_storage(name).ok()
}

/// The settings file whose contents determine Linux's folder roles.
pub fn linux_settings() -> std::io::Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(absent)?);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    Ok(config.join("user-dirs.dirs"))
}

/// Storage preserves access errors, while missing or disabled roles return ENOENT.
pub fn linux_storage(name: &str) -> std::io::Result<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME").ok_or_else(absent)?);
    let settings = linux_settings()?;
    linux_in(name, &home, settings.parent().ok_or_else(absent)?)
}

fn absent() -> std::io::Error {
    std::io::Error::from_raw_os_error(2)
}

fn linux_in(name: &str, home: &Path, config: &Path) -> std::io::Result<PathBuf> {
    let dirs = std::fs::read_to_string(config.join("user-dirs.dirs"))?;
    let folder = match name {
        "screenshots" => user_dir(&dirs, "pictures", home)
            .ok_or_else(absent)?
            .join("Screenshots"),
        name => user_dir(&dirs, name, home).ok_or_else(absent)?,
    };
    existing(folder)
}

fn existing(folder: PathBuf) -> std::io::Result<PathBuf> {
    match std::fs::metadata(&folder) {
        Ok(metadata) if metadata.is_dir() => Ok(folder),
        Ok(_) => Err(absent()),
        Err(error) if error.kind() == std::io::ErrorKind::NotADirectory => Err(absent()),
        Err(error) => Err(error),
    }
}

/// The folder `user-dirs.dirs` (the text `dirs`) names for `name`, with
/// `$HOME` expanded and nothing else: the file is shell syntax, and no
/// shell runs it.
pub fn user_dir(dirs: &str, name: &str, home: &Path) -> Option<PathBuf> {
    let key = match name {
        "desktop" => "XDG_DESKTOP_DIR",
        "documents" => "XDG_DOCUMENTS_DIR",
        "downloads" => "XDG_DOWNLOAD_DIR",
        "music" => "XDG_MUSIC_DIR",
        "pictures" => "XDG_PICTURES_DIR",
        "videos" => "XDG_VIDEOS_DIR",
        _ => return None,
    };
    let value = dirs.lines().rev().find_map(|line| {
        let (k, v) = line.trim().split_once('=')?;
        (k.trim() == key).then_some(v.trim())
    })?;
    let quoted = value.strip_prefix('"')?.strip_suffix('"')?;
    let mut text = String::new();
    let mut chars = quoted.chars();
    while let Some(c) = chars.next() {
        text.push(if c == '\\' { chars.next()? } else { c });
    }
    let folder = match text.strip_prefix("$HOME") {
        Some("") => home.to_path_buf(),
        Some(rest) if rest.starts_with('/') => home.join(rest.trim_start_matches('/')),
        Some(_) => return None,
        None if text.starts_with('/') => PathBuf::from(&text),
        None => return None,
    };
    let bare = |p: &Path| p.components().collect::<PathBuf>();
    (bare(&folder) != bare(home)).then_some(folder)
}

/// The drive's home, retained before app code runs, for resolving its folders.
#[cfg(unix)]
pub struct AgentFolders {
    home: std::fs::File,
    path: PathBuf,
}

#[cfg(unix)]
impl AgentFolders {
    pub fn new(home: &Path) -> std::io::Result<Self> {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::create_dir_all(home)?;
        let directory = std::fs::OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_DIRECTORY | libc::O_CLOEXEC)
            .open(home)?;
        Ok(Self {
            home: directory,
            path: std::fs::canonicalize(home)?,
        })
    }

    /// Create and open each component relative to its retained parent,
    /// refusing links. The caller must use the returned descriptor.
    pub fn resolve(&self, name: &str) -> std::io::Result<(PathBuf, std::fs::File)> {
        use std::os::fd::{AsRawFd, FromRawFd};
        let leaf = match name {
            "desktop" => "Desktop",
            "documents" => "Documents",
            "downloads" => "Downloads",
            "music" => "Music",
            "pictures" => "Pictures",
            "videos" => "Movies",
            "screenshots" => "Pictures/Screenshots",
            _ => return Err(absent()),
        };
        let mut parent = self.home.try_clone()?;
        for part in leaf.split('/') {
            let part = std::ffi::CString::new(part)?;
            // SAFETY: the retained parent and terminated component live
            // through both calls. Existing entries are opened without following links.
            let directory = unsafe {
                if libc::mkdirat(parent.as_raw_fd(), part.as_ptr(), 0o700) != 0
                    && std::io::Error::last_os_error().kind() != std::io::ErrorKind::AlreadyExists
                {
                    return Err(std::io::Error::last_os_error());
                }
                libc::openat(
                    parent.as_raw_fd(),
                    part.as_ptr(),
                    libc::O_RDONLY | libc::O_DIRECTORY | libc::O_CLOEXEC | libc::O_NOFOLLOW,
                )
            };
            if directory < 0 {
                return Err(std::io::Error::last_os_error());
            }
            // SAFETY: openat returned a new owned descriptor.
            parent = unsafe { std::fs::File::from_raw_fd(directory) };
        }
        Ok((self.path.join(leaf), parent))
    }
}

#[cfg(target_os = "macos")]
#[path = "known_folders_apple.rs"]
mod apple;
#[cfg(target_os = "macos")]
pub use apple::{macos, macos_settings};

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn agent_folders_keep_the_home_opened_at_launch() {
        let base = std::env::temp_dir().join(format!("exact-agent-folders-{}", std::process::id()));
        let home = base.join("home");
        let outside = base.join("outside");
        std::fs::create_dir_all(&outside).unwrap();
        let folders = AgentFolders::new(&home).unwrap();
        std::fs::rename(&home, base.join("held")).unwrap();
        std::os::unix::fs::symlink(&outside, &home).unwrap();
        assert!(folders.resolve("screenshots").is_ok());
        assert!(base.join("held/Pictures/Screenshots").is_dir());
        assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn disabled_xdg_entries_and_absent_screenshots_resolve_to_nothing() {
        let base = std::env::temp_dir().join(format!("exact-xdg-{}", std::process::id()));
        std::fs::create_dir_all(base.join("config")).unwrap();
        std::fs::create_dir_all(base.join("Bilder")).unwrap();
        let file = base.join("config/user-dirs.dirs");
        std::fs::write(
            &file,
            "XDG_DOWNLOAD_DIR=\"$HOME/\"\nXDG_PICTURES_DIR=\"$HOME/Bilder\"\n",
        )
        .unwrap();
        assert_eq!(
            linux_in("downloads", &base, &base.join("config"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            linux_in("screenshots", &base, &base.join("config"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        assert_eq!(
            linux_in("pictures", &base, &base.join("config")).unwrap(),
            base.join("Bilder")
        );
        std::fs::create_dir(base.join("Bilder/Screenshots")).unwrap();
        assert_eq!(
            linux_in("screenshots", &base, &base.join("config")).unwrap(),
            base.join("Bilder/Screenshots")
        );
        std::fs::write(&file, "XDG_PICTURES_DIR=\"$HOME/\"\n").unwrap();
        assert_eq!(
            linux_in("screenshots", &base, &base.join("config"))
                .unwrap_err()
                .kind(),
            std::io::ErrorKind::NotFound
        );
        std::fs::write(base.join("not-a-folder"), b"file").unwrap();
        std::fs::write(&file, "XDG_PICTURES_DIR=\"$HOME/not-a-folder/child\"\n").unwrap();
        assert_eq!(
            linux_in("pictures", &base, &base.join("config"))
                .unwrap_err()
                .raw_os_error(),
            Some(2)
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[cfg(unix)]
    #[test]
    fn inaccessible_xdg_folders_do_not_look_absent() {
        use std::os::unix::fs::PermissionsExt;
        let base = std::env::temp_dir().join(format!("exact-xdg-denied-{}", std::process::id()));
        let config = base.join("config");
        let locked = base.join("locked");
        std::fs::create_dir_all(&config).unwrap();
        std::fs::create_dir_all(locked.join("Pictures")).unwrap();
        std::fs::write(
            config.join("user-dirs.dirs"),
            "XDG_PICTURES_DIR=\"$HOME/locked/Pictures\"\n",
        )
        .unwrap();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o0)).unwrap();
        let result = linux_in("pictures", &base, &config);
        let os_error = std::fs::metadata(locked.join("Pictures")).unwrap_err();
        std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o700)).unwrap();
        std::fs::remove_dir_all(base).unwrap();
        assert_eq!(os_error.raw_os_error(), Some(libc::EACCES));
        assert_eq!(result.unwrap_err().raw_os_error(), Some(libc::EACCES));
    }

    #[test]
    fn user_dirs_expand_home_and_nothing_else() {
        let home = Path::new("/home/ana");
        let dirs = r#"# written by xdg-user-dirs-update
XDG_DESKTOP_DIR="$HOME/Schreibtisch"
XDG_DOWNLOAD_DIR="$HOME/"
XDG_PICTURES_DIR="$HOME/Bilder"
XDG_MUSIC_DIR="/srv/music/$USER"
XDG_VIDEOS_DIR="Videos"
XDG_DOCUMENTS_DIR="$HOMEWORK/Docs"
XDG_TEMPLATES_DIR="$HOME/Vorlagen \"neu\""
"#;
        let dir = |name| user_dir(dirs, name, home);
        assert_eq!(dir("desktop"), Some(home.join("Schreibtisch")));
        assert_eq!(dir("pictures"), Some(home.join("Bilder")));
        assert_eq!(dir("music"), Some(PathBuf::from("/srv/music/$USER")));
        assert_eq!(dir("downloads"), None, "the home directory: disabled");
        assert_eq!(dir("videos"), None, "relative");
        assert_eq!(dir("documents"), None, "only $HOME expands");
        assert_eq!(dir("screenshots"), None, "not an XDG entry");
        let quoted = "XDG_PICTURES_DIR=\"$HOME/My \\\"Pics\\\"\"\n";
        assert_eq!(
            user_dir(quoted, "pictures", home),
            Some(home.join("My \"Pics\""))
        );
        assert_eq!(
            user_dir("XDG_PICTURES_DIR=$HOME/Bilder\n", "pictures", home),
            None
        );
    }
}
