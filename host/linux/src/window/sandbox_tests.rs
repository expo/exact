use super::*;
use std::os::unix::fs::symlink;

/// A tree with a shared folder, holding `a.txt`, and a folder the sandbox
/// keeps as its own, holding `x.txt`.
fn tree() -> (tempfile::TempDir, PathBuf, PathBuf) {
    let root = tempfile::tempdir().unwrap();
    let (shared, own) = (root.path().join("shared"), root.path().join("own"));
    std::fs::create_dir_all(&shared).unwrap();
    std::fs::create_dir_all(&own).unwrap();
    std::fs::write(shared.join("a.txt"), "a").unwrap();
    std::fs::write(own.join("x.txt"), "x").unwrap();
    (root, shared, own)
}

#[test]
fn a_shared_path_resolves_one_link_at_a_time_and_never_through_the_sandboxs_own() {
    let (_root, shared, own) = tree();
    let allowed = |path: &Path| match path.starts_with(&own) {
        true => Step::Refused,
        false => Step::Shared([1].into()),
    };
    symlink("a.txt", shared.join("near")).unwrap();
    symlink("../own/x.txt", shared.join("into")).unwrap();
    symlink("../shared/a.txt", own.join("back")).unwrap();
    symlink("../own/back", shared.join("through")).unwrap();
    symlink("loop", shared.join("loop")).unwrap();
    assert_eq!(
        resolve(&shared.join("a.txt"), &allowed),
        Some(shared.join("a.txt"))
    );
    assert_eq!(
        resolve(&shared.join("near"), &allowed),
        Some(shared.join("a.txt")),
        "a link within the shared folder resolves to its file"
    );
    assert_eq!(
        resolve(&shared.join("into"), &allowed),
        None,
        "a link into the sandbox's own folder"
    );
    assert_eq!(
        resolve(&shared.join("through"), &allowed),
        None,
        "a link that passes through the sandbox's own folder and back"
    );
    assert_eq!(resolve(&own.join("x.txt"), &allowed), None);
    assert_eq!(resolve(&shared.join("missing"), &allowed), None);
    assert_eq!(
        resolve(&shared.join("loop"), &allowed),
        None,
        "a link loop ends"
    );
    assert_eq!(
        resolve(Path::new("shared/a.txt"), &allowed),
        None,
        "a relative path"
    );
    assert_eq!(
        resolve(&shared.join("../shared/./a.txt"), &allowed),
        Some(shared.join("a.txt"))
    );
}

/// The shares of a sandbox whose `[Context]` lists `filesystems` and
/// persists `.mozilla`, for someone whose downloads folder is named.
fn shares(filesystems: &str, user_dirs: &str) -> Shares {
    let info = format!(
        "[Application]\nname=org.example.App\npersistent=.not-this\n\n[Context]\nshared=network;ipc;\nfilesystems={filesystems}\npersistent=.mozilla;\n"
    );
    Shares::of(&Context {
        info: &info,
        home: Path::new("/home/someone"),
        runtime: Path::new("/run/user/1000"),
        app: Some("org.example.App"),
        user_dirs,
        docs: vec![
            PathBuf::from("/run/flatpak/doc"),
            PathBuf::from("/run/user/1000/doc"),
        ],
    })
}

#[test]
fn a_sandbox_shares_its_document_mount_its_app_folder_and_what_its_permissions_name() {
    let s = shares(
        "xdg-download;~/Music:ro;xdg-run/gvfs;/var/tmp;/usr/share/doc;xdg-config/example;",
        "# written by xdg-user-dirs-update\nXDG_DOWNLOAD_DIR=\"/home/someone/Downloads\"\n",
    );
    for shared in [
        "/run/user/1000/doc/1a2b/notes.txt",
        "/run/flatpak/doc/1a2b/notes.txt",
        "/home/someone/.var/app/org.example.App/cache/notes.txt",
        "/home/someone/Downloads/notes.txt",
        "/home/someone/Music/song.flac",
        "/run/user/1000/gvfs/smb-share/notes.txt",
        "/var/tmp/notes.txt",
        "/home/someone/.config/example/settings.ini",
    ] {
        assert!(s.shares(Path::new(shared)), "{shared}");
    }
    for own in [
        "/home/someone/notes.txt",
        "/home/someone/.var/app/org.example.Other/notes.txt",
        "/usr/share/doc/notes.txt",
        "/tmp/notes.txt",
        "/run/user/1000/bus",
        "/home/someone",
        "/run/user/1000",
    ] {
        assert!(!s.shares(Path::new(own)), "{own}");
    }
    for on_the_way in [
        "/",
        "/home",
        "/home/someone",
        "/run",
        "/run/user",
        "/run/flatpak",
    ] {
        assert!(
            matches!(s.step(Path::new(on_the_way)), Step::Toward(_)),
            "{on_the_way}"
        );
    }
    assert_eq!(s.step(Path::new("/etc")), Step::Refused);
}

#[test]
fn a_host_share_is_all_but_the_sandboxs_own_trees_and_what_it_persists() {
    let s = shares("host;xdg-run/gvfs;", "");
    for shared in [
        "/home/someone/notes.txt",
        "/opt/example/notes.txt",
        "/run/user/1000/gvfs/notes.txt",
        "/run/user/1000/doc/1a2b/notes.txt",
    ] {
        assert!(s.shares(Path::new(shared)), "{shared}");
    }
    for own in [
        "/tmp/notes.txt",
        "/var/tmp/notes.txt",
        "/run/user/1000/bus",
        "/usr/share/doc/notes.txt",
        "/home/someone/.mozilla/profile/notes.txt",
    ] {
        assert!(!s.shares(Path::new(own)), "{own}");
    }
}

#[test]
fn a_cancelled_share_and_an_unnamed_folder_share_nothing() {
    let s = shares("home;!home;xdg-desktop;xdg-music;host;!host;", "");
    assert!(!s.shares(Path::new("/home/someone/notes.txt")));
    assert!(!s.shares(Path::new("/opt/notes.txt")));
    assert!(
        s.shares(Path::new("/home/someone/Desktop/notes.txt")),
        "the desktop without a named one is ~/Desktop, as GLib has it"
    );
    assert!(!s.shares(Path::new("/home/someone/Music/song.flac")));
    let named = shares("xdg-music;", "XDG_MUSIC_DIR=\"$HOME/Sounds\"\n");
    assert!(named.shares(Path::new("/home/someone/Sounds/song.flac")));
    let home = shares("xdg-music;", "XDG_MUSIC_DIR=\"$HOME/\"\n");
    assert!(
        !home.shares(Path::new("/home/someone/notes.txt")),
        "a person's folder that is the home shares nothing"
    );
}

#[test]
fn a_folder_on_the_way_to_a_share_is_passed_through_but_not_delivered() {
    let (root, shared, _own) = tree();
    let s = Shares {
        roots: vec![shared.clone()],
        ..Shares::default()
    };
    assert_eq!(s.resolve(&shared.join("a.txt")), Some(shared.join("a.txt")));
    assert_eq!(s.resolve(root.path()), None, "the folder above the share");
    assert_eq!(
        s.resolve(&shared.join("../own/x.txt")),
        None,
        "a step back out of the share"
    );
}

/// Two shares under folders of the sandbox's own: a walk on the way to one,
/// or out of it, does not turn by `..` to the other; and, as Linux has it,
/// nothing is below a file.
#[test]
fn a_walk_goes_only_toward_the_share_it_passes_on_the_way_to_and_never_below_a_file() {
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let (one, other) = (base.join("home/one"), base.join("media/other"));
    std::fs::create_dir_all(&one).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::create_dir_all(base.join("home/private")).unwrap();
    std::fs::write(one.join("a.txt"), "a").unwrap();
    std::fs::write(one.join("b.txt"), "b").unwrap();
    std::fs::write(other.join("c.txt"), "c").unwrap();
    let s = Shares {
        roots: vec![one.clone(), other.clone()],
        ..Shares::default()
    };
    assert_eq!(
        s.resolve(&base.join("home/../media/other/c.txt")),
        None,
        "on the way to one share, a step back out turns to the other"
    );
    assert_eq!(
        s.resolve(&one.join("../../media/other/c.txt")),
        None,
        "out of one share, the folders above it lead back to it alone"
    );
    assert_eq!(
        s.resolve(&base.join("home/../home/one/a.txt")),
        Some(one.join("a.txt")),
        "a detour that heads for the same share"
    );
    assert_eq!(
        s.resolve(&one.join("a.txt/../b.txt")),
        None,
        "a file's `..`"
    );
    assert_eq!(
        s.resolve(Path::new(&format!("{}/", one.join("a.txt").display()))),
        None,
        "a file's `/`"
    );
    assert_eq!(
        s.resolve(Path::new(&format!("{}/.", one.join("a.txt").display()))),
        None,
        "a file's `/.`"
    );
    assert_eq!(
        s.resolve(Path::new(&format!("{}/", one.display()))),
        Some(one.clone()),
        "a folder's `/`"
    );
}

/// Under the host's share, a walk heading for the document portal's mount
/// through a folder the sandbox keeps does not turn back up to the host's
/// own tree.
#[test]
fn a_walk_toward_the_document_mount_does_not_turn_to_the_hosts_share() {
    // Not in a temporary folder: the host's share leaves out `/tmp`, a tree the sandbox keeps
    let root = tempfile::tempdir_in(env!("CARGO_MANIFEST_DIR")).unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let (private, doc, other) = (
        base.join("run/user"),
        base.join("run/user/doc"),
        base.join("home"),
    );
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(doc.join("a.txt"), "a").unwrap();
    std::fs::write(other.join("b.txt"), "b").unwrap();
    let s = Shares {
        docs: vec![doc.clone()],
        host: true,
        persisted: vec![private.clone()],
        ..Shares::default()
    };
    assert_eq!(s.resolve(&doc.join("a.txt")), Some(doc.join("a.txt")));
    assert_eq!(
        s.resolve(&other.join("b.txt")),
        Some(other.join("b.txt")),
        "the host's own"
    );
    assert_eq!(
        s.resolve(&private.join("../../home/b.txt")),
        None,
        "on the way to the document mount, back out to the host's share"
    );
}

/// A walk heading for one share does not enter another share directly,
/// as it does not pass toward one.
#[test]
fn a_walk_toward_one_share_does_not_step_into_another() {
    let root = tempfile::tempdir().unwrap();
    let base = std::fs::canonicalize(root.path()).unwrap();
    let (doc, other) = (base.join("run/user/doc"), base.join("home"));
    std::fs::create_dir_all(&doc).unwrap();
    std::fs::create_dir_all(&other).unwrap();
    std::fs::write(other.join("b.txt"), "b").unwrap();
    let s = Shares {
        docs: vec![doc],
        roots: vec![other.clone()],
        ..Shares::default()
    };
    assert_eq!(s.resolve(&other.join("b.txt")), Some(other.join("b.txt")));
    assert_eq!(s.resolve(&base.join("run/user/../../home/b.txt")), None);
}
