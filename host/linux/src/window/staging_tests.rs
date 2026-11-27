use super::*;
use std::cell::RefCell;
use std::fs::FileTimes;
use std::os::unix::fs::PermissionsExt;

/// An app's asset root holding `files`, each a name and its bytes.
fn shipped(files: &[(&str, &str)]) -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    for (name, bytes) in files {
        let path = root.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    root
}

/// Each of `entries` as a dragged file of its own name.
fn own(entries: &[&str]) -> Vec<DragFile> {
    entries
        .iter()
        .map(|entry| DragFile {
            entry: (*entry).to_owned(),
            name: entry.rsplit('/').next().unwrap().to_owned(),
        })
        .collect()
}

/// `stage_in` a new folder, with the journal's lines.
fn staged(names: &[&str], assets: &Assets) -> (tempfile::TempDir, Vec<PathBuf>, Vec<String>) {
    staged_files(&own(names), assets)
}

/// [`staged`] of files with names of their own.
fn staged_files(
    names: &[DragFile],
    assets: &Assets,
) -> (tempfile::TempDir, Vec<PathBuf>, Vec<String>) {
    let base = tempfile::tempdir().unwrap();
    let lines = RefCell::new(Vec::new());
    let copies = stage_in(base.path(), names, assets, None, &|line| {
        lines.borrow_mut().push(line)
    });
    let paths = copies.paths().into_iter().map(Path::to_path_buf).collect();
    (base, paths, lines.into_inner())
}

#[test]
fn each_copy_keeps_its_files_name_in_a_folder_of_its_own() {
    let root = shipped(&[
        ("assets/a/notes.txt", "Fuji"),
        ("assets/b/notes.txt", "Etna, 3357 m"),
    ]);
    let assets = Assets::embedded(root.path().to_owned());
    let (base, copies, lines) = staged(&["assets/a/notes.txt", "assets/b/notes.txt"], &assets);
    assert!(lines.is_empty(), "{lines:?}");
    let folder = copies[0].parent().unwrap().parent().unwrap();
    assert_eq!(folder.parent(), Some(base.path()));
    assert!(folder
        .file_name()
        .unwrap()
        .as_bytes()
        .starts_with(PREFIX.as_bytes()));
    assert_eq!(
        std::fs::metadata(folder).unwrap().permissions().mode() & 0o777,
        0o700,
        "a drag's folder is the person's alone"
    );
    assert_eq!(
        copies,
        [folder.join("0/notes.txt"), folder.join("1/notes.txt")]
    );
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), "Fuji");
    assert_eq!(std::fs::read_to_string(&copies[1]).unwrap(), "Etna, 3357 m");
}

/// A copy takes the name the file is offered under (LLP 1094.000 D14), and
/// two files given one name each keep it in their own folders.
#[test]
fn each_copy_takes_the_name_it_is_offered_under() {
    let root = shipped(&[("assets/1016.jpg", "Fuji"), ("assets/1017.jpg", "Etna")]);
    let assets = Assets::embedded(root.path().to_owned());
    let named = |entry: &str, name: &str| DragFile {
        entry: entry.into(),
        name: name.into(),
    };
    let (_base, copies, lines) = staged_files(
        &[
            named("assets/1016.jpg", "Mount Fuji.jpg"),
            named("assets/1017.jpg", "Mount Fuji.jpg"),
        ],
        &assets,
    );
    assert!(lines.is_empty(), "{lines:?}");
    let folder = copies[0].parent().unwrap().parent().unwrap();
    assert_eq!(
        copies,
        [
            folder.join("0/Mount Fuji.jpg"),
            folder.join("1/Mount Fuji.jpg")
        ]
    );
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), "Fuji");
    assert_eq!(std::fs::read_to_string(&copies[1]).unwrap(), "Etna");
}

#[test]
fn a_selected_update_is_copied_from_its_verified_bytes_and_a_name_it_lacks_is_left_out() {
    let root = shipped(&[
        ("assets/old.txt", "embedded"),
        ("assets/gone.txt", "embedded"),
    ]);
    let resolver: exact_painted::image::AssetResolver = Arc::new(|name: &str| match name {
        "assets/old.txt" => Ok(Some(Arc::from(&b"updated"[..]))),
        "assets/bad.txt" => Err("its digest does not match".to_owned()),
        _ => Ok(None),
    });
    let assets = Assets::selected(root.path().to_owned(), resolver);
    let (_base, copies, lines) = staged(
        &["assets/old.txt", "assets/gone.txt", "assets/bad.txt"],
        &assets,
    );
    assert_eq!(copies.len(), 1);
    assert_eq!(std::fs::read_to_string(&copies[0]).unwrap(), "updated");
    let folder = copies[0].parent().unwrap().parent().unwrap();
    assert!(
        !folder.join("1").exists() && !folder.join("2").exists(),
        "a file that was not copied leaves nothing"
    );
    assert_eq!(
        lines,
        [
            "drag: assets/gone.txt was not copied: the app does not ship it",
            "drag: assets/bad.txt was not copied: its digest does not match",
        ],
        "the update's missing name is missing though the embedded root has it"
    );
}

#[test]
fn a_named_temporary_folder_is_absolute_and_normalized_as_systemd_has_it() {
    for path in ["/", "/var/tmp", "/home/someone/tmp"] {
        assert!(normalized(Path::new(path)), "{path}");
    }
    for path in [
        "",
        "var/tmp",
        "/var/tmp/",
        "/var//tmp",
        "/var/./tmp",
        "/var/../tmp",
        "./tmp",
    ] {
        assert!(!normalized(Path::new(path)), "{path}");
    }
}

/// A cache with an old drag folder, a drag folder holding a file used
/// later, and a folder that is not a drag's.
fn cache() -> (tempfile::TempDir, SystemTime) {
    let cache = tempfile::tempdir().unwrap();
    for folder in ["exact-drag-old/0", "exact-drag-used/0", "other/0"] {
        std::fs::create_dir_all(cache.path().join(folder)).unwrap();
        std::fs::write(cache.path().join(folder).join("notes.txt"), "Fuji").unwrap();
    }
    let later = SystemTime::now() + AGE + Duration::from_secs(60);
    let used = File::options()
        .write(true)
        .open(cache.path().join("exact-drag-used/0/notes.txt"))
        .unwrap();
    used.set_times(FileTimes::new().set_accessed(later).set_modified(later))
        .unwrap();
    (cache, later)
}

#[test]
fn a_sweep_removes_a_drag_folder_unused_for_30_days_but_one_holding_a_file_used_since() {
    let (cache, later) = cache();
    sweep(cache.path(), later - Duration::from_secs(120));
    assert!(
        cache.path().join("exact-drag-old").exists(),
        "not yet 30 days"
    );
    sweep(cache.path(), later);
    assert!(!cache.path().join("exact-drag-old").exists());
    assert!(
        cache.path().join("exact-drag-used/0/notes.txt").exists(),
        "an old folder holding a file used within 30 days stays"
    );
    assert!(
        cache.path().join("other").exists(),
        "a folder no drag made stays"
    );
}

#[test]
fn a_sweep_leaves_the_access_times_it_reads_as_they_were() {
    let (cache, _) = cache();
    let long_ago = SystemTime::UNIX_EPOCH + Duration::from_secs(1_000_000_000);
    let folder = cache.path().join("exact-drag-old");
    File::open(&folder)
        .unwrap()
        .set_times(
            FileTimes::new()
                .set_accessed(long_ago)
                .set_modified(long_ago),
        )
        .unwrap();
    for _ in 0..3 {
        sweep(cache.path(), SystemTime::now());
    }
    assert_eq!(
        std::fs::metadata(&folder).unwrap().accessed().unwrap(),
        long_ago,
        "listing a drag folder does not make it look used"
    );
}

/// The portal is given a copy only while it is the regular file the drag
/// made, found through the drag's folder: not a link or another file put in
/// its place since.
#[test]
fn a_copy_is_found_again_only_while_it_is_the_file_the_drag_made() {
    let root = shipped(&[("assets/notes.txt", "Fuji")]);
    let assets = Assets::embedded(root.path().to_owned());
    let base = tempfile::tempdir().unwrap();
    let names = own(&["assets/notes.txt"]);
    let copies = stage_in(base.path(), &names, &assets, None, &|line| panic!("{line}"));
    let copy = copies.paths()[0].to_path_buf();
    let opened = copies.open(0).unwrap();
    let flags = rfs::fcntl_getfl(&opened).unwrap();
    assert!(flags.contains(OFlags::PATH), "a path alone");
    assert!(
        !flags.contains(OFlags::NOFOLLOW),
        "the document portal refuses `O_NOFOLLOW`"
    );
    assert_eq!(
        std::fs::read_link(format!(
            "/proc/self/fd/{}",
            std::os::fd::AsRawFd::as_raw_fd(&opened)
        ))
        .unwrap(),
        copy
    );
    std::fs::remove_file(&copy).unwrap();
    std::fs::write(base.path().join("elsewhere.txt"), "elsewhere").unwrap();
    std::os::unix::fs::symlink(base.path().join("elsewhere.txt"), &copy).unwrap();
    assert!(copies.open(0).is_err(), "a link in the copy's place");
    std::fs::remove_file(&copy).unwrap();
    std::fs::write(&copy, "Fuji").unwrap();
    assert!(copies.open(0).is_err(), "another file in the copy's place");
}

/// A sweep removes only what it aged: a file added to an old folder since,
/// or an entry put in another's place, keeps the folder.
#[test]
fn a_sweep_leaves_a_folder_whose_entries_changed_since_it_aged_them() {
    let (cache, _) = cache();
    let open = |name: &str| rfs::open(&cache.path().join(name), FOLDER, Mode::empty()).unwrap();
    let folder = open("exact-drag-old");
    let (_, _, found) = survey(&folder).unwrap();
    std::fs::write(cache.path().join("exact-drag-old/0/new.txt"), "new").unwrap();
    assert!(remove(&folder, &found).is_none());
    assert!(cache.path().join("exact-drag-old/0/new.txt").exists());

    let folder = open("exact-drag-used");
    let (_, _, found) = survey(&folder).unwrap();
    let notes = cache.path().join("exact-drag-used/0/notes.txt");
    std::fs::remove_file(&notes).unwrap();
    std::fs::write(&notes, "replaced").unwrap();
    assert!(remove(&folder, &found).is_none());
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "replaced");

    let (edited, _) = self::cache();
    let folder = open_in(&edited, "exact-drag-old");
    let (_, _, found) = survey(&folder).unwrap();
    let notes = edited.path().join("exact-drag-old/0/notes.txt");
    std::thread::sleep(Duration::from_millis(20));
    File::options()
        .append(true)
        .open(&notes)
        .unwrap()
        .write_all(b"!")
        .unwrap();
    assert!(
        remove(&folder, &found).is_none(),
        "a file edited in place since"
    );
    assert_eq!(std::fs::read_to_string(&notes).unwrap(), "Fuji!");
}

fn open_in(cache: &tempfile::TempDir, name: &str) -> OwnedFd {
    rfs::open(&cache.path().join(name), FOLDER, Mode::empty()).unwrap()
}
