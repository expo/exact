use super::*;

#[test]
fn creation_and_atomic_publication_never_replace_existing_entries() {
    let base = std::env::temp_dir().join(format!("ibex-known-create-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let run = |op, leaf: &str| create(base.clone().into(), &[leaf.into()], op, None, b"first");
    for (op, leaf) in [
        (FsOp::WriteFile, "plain"),
        (FsOp::AtomicWriteFile, "atomic"),
    ] {
        run(op, leaf).unwrap();
        assert_eq!(
            run(op, leaf).unwrap_err().kind(),
            io::ErrorKind::AlreadyExists
        );
        assert_eq!(std::fs::read(base.join(leaf)).unwrap(), b"first");
    }
    std::fs::create_dir(base.join("directory")).unwrap();
    for leaf in ["directory", "CON.png", "a:b", "a.", "..", "LPT¹"] {
        assert!(run(FsOp::WriteFile, leaf).is_err(), "{leaf}");
        assert!(run(FsOp::AtomicWriteFile, leaf).is_err(), "{leaf}");
    }
    assert_eq!(std::fs::read_dir(&base).unwrap().count(), 3);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn atomic_creation_holds_its_parent_against_an_ancestor_rename() {
    let base = std::env::temp_dir().join(format!("ibex-known-rename-{}", std::process::id()));
    std::fs::create_dir_all(base.join("root/sub")).unwrap();
    let root = Directory::known(&base.join("root"), None).unwrap();
    let parent = root.child("sub", false).unwrap();
    // Windows refuses to rename a directory while a handle below it is open,
    // so the walk's held parent cannot be moved out from under the write.
    atomic(&parent, "shot", b"png", || {
        let refused = std::fs::rename(base.join("root"), base.join("held")).unwrap_err();
        assert_eq!(refused.kind(), io::ErrorKind::PermissionDenied);
        Ok(())
    })
    .unwrap();
    assert_eq!(std::fs::read(base.join("root/sub/shot")).unwrap(), b"png");
    assert_eq!(
        atomic(&parent, "shot", b"replaced", || Ok(()))
            .unwrap_err()
            .kind(),
        io::ErrorKind::AlreadyExists
    );
    drop(parent);
    drop(root);
    std::fs::rename(base.join("root"), base.join("held")).unwrap();
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn links_below_a_known_root_are_refused_and_agent_roots_stay_pinned() {
    let base = std::env::temp_dir().join(format!("ibex-known-links-{}", std::process::id()));
    std::fs::create_dir_all(base.join("outside")).unwrap();
    let home = base.join("home");
    let folders = WindowsAgentFolders::new(&home).unwrap();
    std::fs::rename(&home, base.join("held")).unwrap();
    std::fs::create_dir(&home).unwrap();
    let folder = folders.resolve("screenshots").unwrap();
    create(folder, &["shot".into()], FsOp::WriteFile, None, b"png").unwrap();
    assert!(base.join("held/Pictures/Screenshots/shot").is_file());
    assert!(!home.join("Pictures").exists());
    // Developer Mode or elevated symlink privilege is needed on Windows.
    match std::os::windows::fs::symlink_dir(base.join("outside"), home.join("link")) {
        Ok(()) => {
            assert!(create(
                home.clone().into(),
                &["link".into(), "shot".into()],
                FsOp::WriteFile,
                None,
                b"png"
            )
            .is_err());
            std::fs::remove_dir(home.join("link")).unwrap();
            assert!(!base.join("outside/shot").exists());
        }
        Err(error) if error.raw_os_error() == Some(1314) => {
            eprintln!("symlink check not run: {error}")
        }
        Err(error) => panic!("{error}"),
    }
    drop(folders);
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn mkdir_creates_every_component_and_refuses_an_existing_one() {
    let base = std::env::temp_dir().join(format!("ibex-known-mkdir-{}", std::process::id()));
    std::fs::create_dir_all(&base).unwrap();
    let make = |parts: &[&str]| {
        let parts: Vec<String> = parts.iter().map(|part| part.to_string()).collect();
        create(base.clone().into(), &parts, FsOp::Mkdir, None, b"")
    };
    make(&["made", "nested"]).unwrap();
    assert!(base.join("made/nested").is_dir());
    assert_eq!(
        make(&["made"]).unwrap_err().kind(),
        io::ErrorKind::AlreadyExists
    );
    // The new directories are ordinary parents of a create.
    create(
        base.clone().into(),
        &["made".into(), "nested".into(), "shot".into()],
        FsOp::WriteFile,
        None,
        b"png",
    )
    .unwrap();
    assert_eq!(
        std::fs::read(base.join("made/nested/shot")).unwrap(),
        b"png"
    );
    // A later component's failure removes the ones this call made.
    let parts: Vec<String> = ["undone", "middle", "last"].map(Into::into).into();
    let refused = mkdir(
        Directory::known(&base, None).unwrap(),
        &parts,
        |parent: &Directory, leaf: &str| {
            if leaf == "last" {
                Err(io::Error::from_raw_os_error(5))
            } else {
                parent.create_child(leaf)
            }
        },
    )
    .unwrap_err();
    assert_eq!(refused.raw_os_error(), Some(5));
    assert!(!base.join("undone").exists());
    std::fs::remove_dir_all(base).unwrap();
}
