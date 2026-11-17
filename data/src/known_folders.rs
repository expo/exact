//! The person's well-known folders by role (LLP 1173 D4's table): where the
//! platform says each is, asked again at every use, so a folder the person
//! moves is followed. Today a picker's `startIn` reads them (D2).

use std::path::{Path, PathBuf};

/// Where the well-known folder `name` is on a Linux desktop: its
/// `XDG_<NAME>_DIR` in `user-dirs.dirs` under `$XDG_CONFIG_HOME` (else
/// `~/.config`), and `screenshots` the pictures folder's `Screenshots`,
/// untranslated, where GNOME Shell saves. `None` for a name that is not
/// one, an entry that is unset, relative or the home directory itself
/// (XDG's way of disabling a folder), or a folder that does not exist.
pub fn linux(name: &str) -> Option<PathBuf> {
    let home = PathBuf::from(std::env::var_os("HOME")?);
    let config = std::env::var_os("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .filter(|p| p.is_absolute())
        .unwrap_or_else(|| home.join(".config"));
    let dirs = std::fs::read_to_string(config.join("user-dirs.dirs")).ok()?;
    let folder = match name {
        "screenshots" => user_dir(&dirs, "pictures", &home)?.join("Screenshots"),
        name => user_dir(&dirs, name, &home)?,
    };
    folder.is_dir().then_some(folder)
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

#[cfg(test)]
mod tests {
    use super::*;

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
