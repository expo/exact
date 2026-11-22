//! A font directory's faces, cached: a phone's `/system/fonts` holds about
//! 300 files, and reading every one's tables on each launch was most of a
//! cold start's text setup (LLP 1076). The cache is a small text file in the
//! app's cache directory, keyed by the directory's listing (each file's
//! path, length and modification time, subdirectories included), so a
//! system update that changes a font reads the directory again.
//!
//! Each face is kept as fontique matches it (LLP 1085.000 §4, "The font
//! cache"): its family, width, style and weight, every variation axis, and
//! where its charmap is. A cached launch registers the faces described and
//! opens no font file; one is mapped when a shape first uses it.
use super::fonts::Described;
use fontique::{AxisInfo, CharmapIndex, FontStyle, FontWeight, FontWidth, SourceKind};
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const HEADER: &str = "exact-fontique 2";

/// `dir`'s faces, from the cache when the directory has not changed since
/// it was written; otherwise scanned (every file read once), and cached
/// when the scan read every file and the cache holds every face exactly.
pub(super) fn load(dir: &Path) -> Vec<Described> {
    if !dir.is_dir() {
        return Vec::new();
    }
    load_cached(dir, cache_file(dir))
}

fn load_cached(dir: &Path, cache: Option<PathBuf>) -> Vec<Described> {
    // The scan reads exactly the files the key lists.
    let files = listing(dir);
    let key = key_of(&files);
    if let Some(faces) = cache
        .as_ref()
        .and_then(|c| std::fs::read_to_string(c).ok())
        .and_then(|t| parse(&t, key))
    {
        return faces;
    }
    let (faces, complete) = scan(files.iter().map(|(path, ..)| path));
    // A file that could not be opened keeps its length and modification
    // time, so a manifest without its faces would outlive the failure.
    if let Some(cache) = cache.filter(|_| complete) {
        write(&cache, key, &faces);
    }
    faces
}

/// Every face in these files and directories, read by fontique (the patched
/// scan registers each file once; a face fontique refuses, LLP 1085.000 G2,
/// is not listed), and whether fontique read every path.
pub(super) fn scan(paths: impl IntoIterator<Item = impl AsRef<Path>>) -> (Vec<Described>, bool) {
    let mut collection = super::fonts::empty_collection();
    let complete = collection.load_fonts_from_paths(paths);
    let mut names: Vec<String> = collection.family_names().map(str::to_owned).collect();
    names.sort();
    let mut faces = Vec::new();
    for name in names {
        let Some(family) = collection.family_by_name(&name) else {
            continue;
        };
        for font in family.fonts() {
            let SourceKind::Path(path) = font.source().kind() else {
                continue;
            };
            faces.push(Described {
                family: name.clone(),
                path: path.clone(),
                index: font.index(),
                width: font.width(),
                style: font.style(),
                weight: font.weight(),
                axes: font.axes().to_vec(),
                charmap: font.charmap_index(),
            });
        }
    }
    (faces, complete)
}

fn write(cache: &Path, key: u64, faces: &[Described]) {
    let mut out = format!("{HEADER} {key:016x}\n");
    for f in faces {
        let Some(path) = f.path.to_str() else {
            return;
        };
        let (offset, symbol, mac) = f.charmap.to_parts();
        let axes: Vec<String> = f
            .axes
            .iter()
            .map(|a| format!("{}:{}:{}:{}", a.tag, a.min, a.max, a.default))
            .collect();
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{}\t{}\t{},{},{}\t{}\n",
            f.family,
            path,
            f.index,
            f.width.ratio(),
            style_str(f.style),
            f.weight.value(),
            offset,
            u8::from(symbol),
            u8::from(mac),
            axes.join(";"),
        ));
    }
    // A face that reads back changed, such as a tab in a name or a `;` in an
    // axis tag, would be missing or wrong on a cached launch, so the
    // directory stays uncached.
    let read = parse(&out, key);
    if !read.is_some_and(|read| read.len() == faces.len() && read.iter().zip(faces).all(same)) {
        return;
    }
    if let Some(parent) = cache.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    // Written whole under a name of this writer's own, then moved into
    // place: a reader never sees half of it, and two writers never share a
    // temporary file.
    static SERIAL: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let serial = SERIAL.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let tmp = cache.with_extension(format!("tmp-{}-{serial}", std::process::id()));
    if std::fs::write(&tmp, out).is_ok() && std::fs::rename(&tmp, cache).is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
}

fn same((a, b): (&Described, &Described)) -> bool {
    let axes = |f: &Described| -> Vec<_> {
        f.axes
            .iter()
            .map(|x| (x.tag, x.min, x.max, x.default))
            .collect()
    };
    a.family == b.family
        && a.path == b.path
        && (a.index, a.width, a.style, a.weight, a.charmap)
            == (b.index, b.width, b.style, b.weight, b.charmap)
        && axes(a) == axes(b)
}

fn style_str(s: FontStyle) -> String {
    match s {
        FontStyle::Normal => "n".into(),
        FontStyle::Italic => "i".into(),
        FontStyle::Oblique(None) => "o".into(),
        FontStyle::Oblique(Some(a)) => format!("o{a}"),
    }
}

fn style_of(s: &str) -> Option<FontStyle> {
    Some(match s {
        "n" => FontStyle::Normal,
        "i" => FontStyle::Italic,
        "o" => FontStyle::Oblique(None),
        o => FontStyle::Oblique(Some(o.strip_prefix('o')?.parse().ok()?)),
    })
}

/// `exact/fonts-<dir hash>` under `$XDG_CACHE_HOME` or `$HOME/.cache`, or on
/// Windows without either, under the local app data folder.
fn cache_file(dir: &Path) -> Option<PathBuf> {
    let base = crate::picker::xdg_base("XDG_CACHE_HOME", ".cache");
    #[cfg(windows)]
    let base = base.or_else(|| crate::picker::local_app_data().ok());
    let base = base?;
    let mut h = std::collections::hash_map::DefaultHasher::new();
    HEADER.hash(&mut h);
    dir.as_os_str().hash(&mut h);
    Some(
        base.join("exact")
            .join(format!("fonts-{:016x}", h.finish())),
    )
}

/// Each regular file under `dir` with its length and modification time, in
/// the order fontique would visit them, subdirectories included. A
/// subdirectory that cannot be listed, or a file whose metadata cannot be
/// read, is left out, so the listing changes when it can be.
fn listing(dir: &Path) -> Vec<(PathBuf, u64, u128)> {
    fn walk(dir: &Path, depth: u32, out: &mut Vec<(PathBuf, u64, u128)>) {
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let Ok(m) = std::fs::metadata(&path) else {
                continue;
            };
            if m.is_dir() {
                if depth < 16 {
                    walk(&path, depth + 1, out);
                }
                continue;
            }
            // Opening a FIFO, say, can block.
            if !m.is_file() {
                continue;
            }
            let t = m
                .modified()
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
                .map_or(0, |d| d.as_nanos());
            out.push((path, m.len(), t));
        }
    }
    let mut files = Vec::new();
    walk(dir, 0, &mut files);
    files
}

/// The listing as a key, in traversal order: of two equivalent faces, the
/// one registered first is matched. Each path is hashed as its bytes:
/// `Path`'s hash skips separators, so `/a/b` would collide with `a/b`.
fn key_of(files: &[(PathBuf, u64, u128)]) -> u64 {
    let mut h = std::collections::hash_map::DefaultHasher::new();
    for (path, len, time) in files {
        (path.as_os_str(), len, time).hash(&mut h);
    }
    h.finish()
}

/// The faces a cache file written for `key` lists, or `None` (stale,
/// short, or not one).
fn parse(text: &str, key: u64) -> Option<Vec<Described>> {
    let mut lines = text.lines();
    if lines.next()? != format!("{HEADER} {key:016x}") {
        return None;
    }
    let mut paths: std::collections::HashMap<&str, Arc<Path>> = Default::default();
    lines
        .map(|line| {
            let f: Vec<&str> = line.split('\t').collect();
            let [family, path, index, width, style, weight, cmap, axes] = f[..] else {
                return None;
            };
            let path = paths
                .entry(path)
                .or_insert_with(|| Arc::from(Path::new(path)))
                .clone();
            let mut cm = cmap.split(',');
            let charmap = CharmapIndex::from_parts(
                cm.next()?.parse().ok()?,
                cm.next()? == "1",
                cm.next()? == "1",
            );
            let axes = axes
                .split(';')
                .filter(|s| !s.is_empty())
                .map(|a| {
                    let p: Vec<&str> = a.split(':').collect();
                    let [tag, min, max, default] = p[..] else {
                        return None;
                    };
                    Some(AxisInfo {
                        tag: tag.parse().ok()?,
                        min: min.parse().ok()?,
                        max: max.parse().ok()?,
                        default: default.parse().ok()?,
                    })
                })
                .collect::<Option<Vec<_>>>()?;
            Some(Described {
                family: family.to_string(),
                path,
                index: index.parse().ok()?,
                width: FontWidth::from_ratio(width.parse().ok()?),
                style: style_of(style)?,
                weight: FontWeight::new(weight.parse().ok()?),
                axes,
                charmap,
            })
        })
        .collect()
}

#[cfg(all(test, any(unix, windows)))]
mod tests {
    use super::*;

    const INTER: &[u8] = include_bytes!("../../tests/fonts/Inter-Regular.ttf");
    const ARABIC: &[u8] = include_bytes!("../../tests/fonts/NotoSansArabic.ttf");
    const FLEX: &[u8] = include_bytes!("../../tests/fonts/RobotoFlex-subset.ttf");

    fn families(faces: &[Described]) -> Vec<&str> {
        let mut families: Vec<&str> = faces.iter().map(|f| f.family.as_str()).collect();
        families.dedup();
        families
    }

    /// Another open of the file fails while this is held; its length and
    /// modification time stay as they were.
    #[cfg(unix)]
    struct Unreadable(PathBuf);
    #[cfg(unix)]
    impl Unreadable {
        fn hold(path: &Path) -> Self {
            use std::os::unix::fs::PermissionsExt;
            std::fs::set_permissions(path, std::fs::Permissions::from_mode(0)).unwrap();
            Self(path.into())
        }
    }
    #[cfg(unix)]
    impl Drop for Unreadable {
        fn drop(&mut self) {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&self.0, std::fs::Permissions::from_mode(0o644));
        }
    }
    #[cfg(windows)]
    struct Unreadable(#[allow(dead_code)] std::fs::File);
    #[cfg(windows)]
    impl Unreadable {
        fn hold(path: &Path) -> Self {
            use std::os::windows::fs::OpenOptionsExt;
            let file = std::fs::OpenOptions::new()
                .read(true)
                .share_mode(0)
                .open(path);
            Self(file.unwrap())
        }
    }

    #[test]
    fn a_font_that_cannot_be_opened_leaves_its_directory_uncached() {
        let root = crate::TestDir::new("exact-font-cache-unreadable");
        let (fonts, cache) = (root.join("fonts"), root.join("cache"));
        std::fs::create_dir(&fonts).unwrap();
        std::fs::write(fonts.join("a.ttf"), INTER).unwrap();
        std::fs::write(fonts.join("b.ttf"), ARABIC).unwrap();
        let key = key_of(&listing(&fonts));
        {
            let _held = Unreadable::hold(&fonts.join("b.ttf"));
            // A superuser opens any file.
            #[cfg(unix)]
            if std::fs::File::open(fonts.join("b.ttf")).is_ok() {
                return;
            }
            assert_eq!(key_of(&listing(&fonts)), key);
            let faces = load_cached(&fonts, Some(cache.clone()));
            assert_eq!(families(&faces), ["Inter"]);
            assert!(!cache.exists());
        }
        let faces = load_cached(&fonts, Some(cache.clone()));
        assert_eq!(families(&faces), ["Inter", "Noto Sans Arabic"]);
        let cached = parse(&std::fs::read_to_string(&cache).unwrap(), key).unwrap();
        assert_eq!(families(&cached), ["Inter", "Noto Sans Arabic"]);
        assert!(
            !scan([fonts.join("c.ttf")]).1,
            "a missing path was not read"
        );
        #[cfg(unix)]
        {
            let _socket = std::os::unix::net::UnixListener::bind(fonts.join("c.sock")).unwrap();
            assert_eq!(listing(&fonts).len(), 2, "only regular files are listed");
        }
    }

    #[test]
    fn a_face_the_manifest_cannot_hold_leaves_its_directory_uncached() {
        let root = crate::TestDir::new("exact-font-cache-inexact");
        let (fonts, cache) = (root.join("fonts"), root.join("cache"));
        std::fs::create_dir(&fonts).unwrap();
        std::fs::write(fonts.join("a.ttf"), INTER).unwrap();
        std::fs::write(fonts.join("b.ttf"), FLEX).unwrap();
        let (faces, complete) = scan([&fonts]);
        assert!(complete && faces.iter().any(|f| !f.axes.is_empty()));
        write(&cache, 1, &faces);
        let text = std::fs::read_to_string(&cache).unwrap();
        let read = parse(&text, 1).unwrap();
        assert!(read.len() == faces.len() && read.iter().zip(&faces).all(same));
        std::fs::remove_file(&cache).unwrap();

        let mut tab = faces.clone();
        tab[0].family = "Inter\tDisplay".into();
        let mut path = faces.clone();
        path[0].path = Arc::from(not_unicode("a.ttf").as_path());
        let mut tag = faces.clone();
        let axes = &mut tag.iter_mut().find(|f| !f.axes.is_empty()).unwrap().axes;
        axes[0].tag = ";ABC".parse().unwrap();
        for faces in [tab, path, tag] {
            write(&cache, 1, &faces);
            assert!(!cache.exists());
        }
    }

    /// `name` after a character that is not Unicode, a byte on Unix and an
    /// unpaired surrogate on Windows.
    fn not_unicode(name: &str) -> PathBuf {
        #[cfg(unix)]
        let prefix = {
            use std::os::unix::ffi::OsStrExt;
            std::ffi::OsStr::from_bytes(b"\xff").to_owned()
        };
        #[cfg(windows)]
        let prefix = {
            use std::os::windows::ffi::OsStringExt;
            std::ffi::OsString::from_wide(&[0xD800])
        };
        let mut path = prefix;
        path.push(name);
        path.into()
    }

    #[test]
    fn listings_that_differ_have_different_keys() {
        let printed = PathBuf::from("\u{FFFD}a.ttf");
        assert_eq!(
            not_unicode("a.ttf").to_string_lossy(),
            printed.to_string_lossy()
        );
        let key = |path: PathBuf| key_of(&[(path, 1, 1)]);
        assert_ne!(key(not_unicode("a.ttf")), key(printed));
        assert_ne!(key("/fonts/a.ttf".into()), key("fonts/a.ttf".into()));
        let (a, b) = (
            (PathBuf::from("a.ttf"), 1, 1),
            (PathBuf::from("b.ttf"), 1, 1),
        );
        assert_ne!(key_of(&[a.clone(), b.clone()]), key_of(&[b, a]));
    }

    #[cfg(windows)]
    #[test]
    fn windows_default_cache_file_without_home_or_xdg() {
        const CHILD: &str = "EXACT_WINDOWS_FONT_CACHE_TEST";
        if std::env::var_os(CHILD).is_none() {
            // Unset, empty and relative: none of them is a cache base.
            for value in [None, Some(""), Some("cache")] {
                let mut child = std::process::Command::new(std::env::current_exe().unwrap());
                child
                    .args([
                        "--exact",
                        "text::font_cache::tests::windows_default_cache_file_without_home_or_xdg",
                    ])
                    .env(CHILD, "1")
                    .env_remove("LOCALAPPDATA");
                for variable in ["HOME", "XDG_CACHE_HOME"] {
                    match value {
                        Some(value) => child.env(variable, value),
                        None => child.env_remove(variable),
                    };
                }
                let output = child.output().unwrap();
                assert!(
                    output.status.success(),
                    "{value:?}: {}{}",
                    String::from_utf8_lossy(&output.stdout),
                    String::from_utf8_lossy(&output.stderr)
                );
            }
            return;
        }

        let cache = cache_file(Path::new(r"C:\Windows\Fonts")).unwrap();
        let base = crate::picker::local_app_data().unwrap().join("exact");
        assert_eq!(cache.parent(), Some(base.as_path()));
    }
}
