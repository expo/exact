use super::*;

static KNOWN_ROOTS: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[test]
fn portable_storage_creates_in_known_folders_without_a_document_service() {
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let root = Paths::new();
    std::fs::create_dir_all(&root.0).unwrap();
    let folder = root.0.clone();
    ibex2::stdlib::fs::set_roots(ibex2::stdlib::fs::Roots {
        known_folder: Some(std::sync::Arc::new(move |_| Ok(folder.clone().into()))),
        ..Default::default()
    });
    let changed = std::sync::atomic::AtomicBool::new(false);
    let grants = "fs.write documents:/
fs.write pictures:/
fs.read app:/tmp";
    let run = |op, args| {
        storage::response(super::native::run(
            Some(&root.directories()),
            grants,
            grants,
            &serde_json::to_vec(&json!({"version":1,"op":op,"args":args})).unwrap(),
            0,
            &changed,
        ))
    };
    run(
        "fs.writeFile",
        json!({"path":"documents:/note","text":"new"}),
    )
    .unwrap();
    assert_eq!(std::fs::read(root.0.join("note")).unwrap(), b"new");
    let error = run(
        "fs.atomicWriteFile",
        json!({"path":"documents:/note","text":"overwrite"}),
    )
    .unwrap_err();
    let exists = if cfg!(windows) {
        &[80, 183][..]
    } else {
        &[17][..]
    };
    assert!(
        exists
            .iter()
            .any(|code| error.contains(&format!("os error {code}"))),
        "{error}"
    );
    run("fs.mkdir", json!({"path":"pictures:/Exports/2026"})).unwrap();
    assert!(root.0.join("Exports/2026").is_dir());
    for path in [
        "pictures:/Exports",
        "pictures:/Exports/2026",
        "pictures:/Exports/2027",
    ] {
        let error = run("fs.mkdir", json!({"path":path})).unwrap_err();
        assert!(
            exists
                .iter()
                .any(|code| error.contains(&format!("os error {code}"))),
            "{path}: {error}"
        );
    }
    assert_eq!(
        std::fs::read_dir(root.0.join("Exports")).unwrap().count(),
        1
    );
    assert!(root.0.join("Exports/2026").is_dir());
    std::fs::create_dir_all(root.0.join("temporary")).unwrap();
    std::fs::write(root.0.join("temporary/capture"), b"png").unwrap();
    run(
        "fs.copyFile",
        json!({"path":"app:/tmp/capture","destination":"pictures:/shot.png"}),
    )
    .unwrap();
    assert_eq!(std::fs::read(root.0.join("shot.png")).unwrap(), b"png");
    std::fs::write(root.0.join("temporary/capture"), b"replacement").unwrap();
    let error = run(
        "fs.copyFile",
        json!({"path":"app:/tmp/capture","destination":"pictures:/shot.png"}),
    )
    .unwrap_err();
    assert!(
        exists
            .iter()
            .any(|code| error.contains(&format!("os error {code}"))),
        "{error}"
    );
    assert_eq!(std::fs::read(root.0.join("shot.png")).unwrap(), b"png");
    assert!(run("fs.rm", json!({"path":"pictures:/shot.png"}))
        .unwrap_err()
        .starts_with("denied:"));
    ibex2::stdlib::fs::set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn agent_known_folders_refuse_links_planted_through_home() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let home = paths.0.join("home");
    let outside = paths.0.join("outside");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let folders = exact_data::known_folders::AgentFolders::new(&home).unwrap();
    set_roots(Roots {
        home: Some(home.clone()),
        known_folder: Some(std::sync::Arc::new(move |name| {
            folders.resolve(name).map(Into::into)
        })),
        ..Default::default()
    });
    let grants = ibex2::grant::GrantSet::parse(
        "fs.write home:/Pictures\nfs.write pictures:/\nfs.write screenshots:/",
    )
    .unwrap();
    run(
        &grants,
        None,
        FsOp::Symlink,
        "home:/Pictures",
        None,
        Some(outside.to_str().unwrap().as_bytes()),
    )
    .unwrap();
    for path in ["pictures:/shot.png", "screenshots:/shot.png"] {
        let result = run(&grants, None, FsOp::WriteFile, path, None, Some(b"png"));
        assert!(result.is_err(), "{path}: {result:?}");
    }
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    std::fs::remove_file(home.join("Pictures")).unwrap();
    std::fs::create_dir(home.join("Pictures")).unwrap();
    run(
        &grants,
        None,
        FsOp::Symlink,
        "home:/Pictures/Screenshots",
        None,
        Some(outside.to_str().unwrap().as_bytes()),
    )
    .unwrap();
    let nested = run(
        &grants,
        None,
        FsOp::WriteFile,
        "screenshots:/shot.png",
        None,
        Some(b"png"),
    );
    assert!(nested.is_err(), "{nested:?}");
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn agent_storage_uses_the_folder_handle_after_its_name_is_replaced() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let home = paths.0.join("home");
    let outside = paths.0.join("outside");
    std::fs::create_dir_all(&outside).unwrap();
    let folders = exact_data::known_folders::AgentFolders::new(&home).unwrap();
    let swapped = std::sync::atomic::AtomicBool::new(false);
    let resolve_home = home.clone();
    set_roots(Roots {
        known_folder: Some(std::sync::Arc::new(move |name| {
            let folder = folders.resolve(name)?;
            if !swapped.swap(true, Ordering::Relaxed) {
                std::fs::rename(resolve_home.join("Pictures"), resolve_home.join("held")).unwrap();
                std::os::unix::fs::symlink(&outside, resolve_home.join("Pictures")).unwrap();
            }
            Ok(folder.into())
        })),
        ..Default::default()
    });
    let grants = ibex2::grant::GrantSet::parse("fs.write pictures:/").unwrap();
    run(
        &grants,
        None,
        FsOp::WriteFile,
        "pictures:/shot.png",
        None,
        Some(b"png"),
    )
    .unwrap();
    assert_eq!(std::fs::read(home.join("held/shot.png")).unwrap(), b"png");
    assert_eq!(
        std::fs::read_dir(paths.0.join("outside")).unwrap().count(),
        0
    );
    set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn desktop_known_folders_refuse_links_planted_through_home() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let home = paths.0.join("home");
    let outside = paths.0.join("outside");
    std::fs::create_dir_all(home.join("Pictures")).unwrap();
    std::fs::create_dir(&outside).unwrap();
    let folder = home.join("Pictures");
    let mut roots = Roots {
        home: Some(home.clone()),
        known_folder: Some(std::sync::Arc::new(move |_| Ok(folder.clone().into()))),
        ..Default::default()
    };
    let grants =
        ibex2::grant::GrantSet::parse("fs.write home:/Pictures\nfs.write pictures:/").unwrap();
    roots.guard_known_folders(&grants, &[], &[]);
    set_roots(roots);
    run(&grants, None, FsOp::Remove, "home:/Pictures", None, None).unwrap();
    run(
        &grants,
        None,
        FsOp::Symlink,
        "home:/Pictures",
        None,
        Some(outside.to_str().unwrap().as_bytes()),
    )
    .unwrap();
    let result = run(
        &grants,
        None,
        FsOp::WriteFile,
        "pictures:/shot.png",
        None,
        Some(b"png"),
    );
    assert!(result.is_err(), "{result:?}");
    assert!(!outside.join("shot.png").exists());
    // On a case-insensitive volume, another spelling still names the same link.
    if home.join("pictures").exists() {
        let mut roots = Roots {
            home: Some(home.clone()),
            known_folder: Some(std::sync::Arc::new({
                let folder = home.join("Pictures");
                move |_| Ok(folder.clone().into())
            })),
            ..Default::default()
        };
        let grants =
            ibex2::grant::GrantSet::parse("fs.write home:/pictures\nfs.write pictures:/").unwrap();
        roots.guard_known_folders(&grants, &[], &[]);
        set_roots(roots);
        assert!(run(
            &grants,
            None,
            FsOp::WriteFile,
            "pictures:/alias.png",
            None,
            Some(b"png")
        )
        .is_err());
    }
    assert_eq!(std::fs::read_dir(&outside).unwrap().count(), 0);
    set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn writable_folder_settings_and_their_hard_links_cannot_redirect_desktop_storage() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let home = paths.0.join("home");
    let config = home.join(".config");
    let pictures = home.join("Pictures");
    let outside = paths.0.join("outside");
    for folder in [&config, &pictures, &outside] {
        std::fs::create_dir_all(folder).unwrap();
    }
    let settings = config.join("user-dirs.dirs");
    let editable = home.join("editable");
    std::fs::create_dir(&editable).unwrap();
    for (write_grant, write_path, link_at_launch) in [
        ("home:/.config", "home:/.config/user-dirs.dirs", false),
        ("home:/editable", "home:/editable/alias", true),
        ("home:/editable", "home:/editable/alias", false),
    ] {
        std::fs::write(&settings, "XDG_PICTURES_DIR=\"$HOME/Pictures\"\n").unwrap();
        let alias = write_grant == "home:/editable";
        if alias && link_at_launch {
            std::fs::hard_link(&settings, editable.join("alias")).unwrap();
        }
        let resolve_home = home.clone();
        let resolve_settings = settings.clone();
        let mut roots = Roots {
            home: Some(home.clone()),
            known_folder: Some(std::sync::Arc::new(move |name| {
                let dirs = std::fs::read_to_string(&resolve_settings)?;
                exact_data::known_folders::user_dir(&dirs, name, &resolve_home)
                    .map(Into::into)
                    .ok_or_else(|| std::io::Error::from_raw_os_error(2))
            })),
            ..Default::default()
        };
        let grants =
            ibex2::grant::GrantSet::parse(&format!("fs.write {write_grant}\nfs.write pictures:/"))
                .unwrap();
        roots.guard_known_folders(&grants, &[("pictures", settings.clone())], &[]);
        let process_roots = roots.clone();
        set_roots(roots);
        if alias && !link_at_launch {
            std::fs::hard_link(&settings, editable.join("alias")).unwrap();
        }
        let changed = format!("XDG_PICTURES_DIR=\"{}\"\n", outside.display());
        run(
            &grants,
            None,
            FsOp::WriteFile,
            write_path,
            None,
            Some(changed.as_bytes()),
        )
        .unwrap();
        if alias && link_at_launch {
            std::fs::remove_file(editable.join("alias")).unwrap();
        }
        let result = run(
            &grants,
            None,
            FsOp::WriteFile,
            "pictures:/shot.png",
            None,
            Some(b"png"),
        );
        assert!(
            matches!(result, Err(ref e) if e.to_string().contains("os error 13")),
            "{result:?}"
        );
        assert!(
            !outside.join("shot.png").exists(),
            "the app rewrote its folder authority"
        );
        assert!(!pictures.join("shot.png").exists());
        if alias && !link_at_launch {
            std::fs::remove_file(editable.join("alias")).unwrap();
        }
        assert!(process_roots.resolve_known_folder("pictures:/").is_err());
        set_roots(Default::default());
    }
}

#[cfg(unix)]
#[test]
fn desktop_folder_links_outside_write_grants_and_settings_changes_are_followed() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let home = paths.0.join("home");
    let outside = paths.0.join("outside");
    std::fs::create_dir_all(&home).unwrap();
    std::fs::create_dir(&outside).unwrap();
    std::os::unix::fs::symlink(&outside, home.join("Pictures")).unwrap();
    let current = std::sync::Arc::new(std::sync::Mutex::new(home.join("Pictures")));
    let resolved = current.clone();
    let mut roots = Roots {
        home: Some(home.clone()),
        known_folder: Some(std::sync::Arc::new(move |_| {
            Ok(resolved.lock().unwrap().clone().into())
        })),
        ..Default::default()
    };
    // The app may create below Exports, but cannot replace Pictures itself.
    let grants =
        ibex2::grant::GrantSet::parse("fs.write home:/Pictures/Exports\nfs.write pictures:/")
            .unwrap();
    roots.guard_known_folders(
        &grants,
        &[("pictures", home.join(".config/user-dirs.dirs"))],
        &[],
    );
    set_roots(roots);
    run(
        &grants,
        None,
        FsOp::WriteFile,
        "pictures:/shot.png",
        None,
        Some(b"first"),
    )
    .unwrap();
    assert_eq!(std::fs::read(outside.join("shot.png")).unwrap(), b"first");
    *current.lock().unwrap() = home.clone();
    run(
        &grants,
        None,
        FsOp::WriteFile,
        "pictures:/shot.png",
        None,
        Some(b"moved"),
    )
    .unwrap();
    assert_eq!(std::fs::read(home.join("shot.png")).unwrap(), b"moved");
    set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn writable_settings_refuse_only_the_roles_they_control() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    std::fs::create_dir_all(&paths.0).unwrap();
    let folder = paths.0.clone();
    let mut roots = Roots {
        home: Some(folder.clone()),
        known_folder: Some(std::sync::Arc::new(move |name| {
            assert_ne!(name, "screenshots", "do not read app-writable settings");
            Ok(folder.clone().into())
        })),
        ..Default::default()
    };
    let grants = ibex2::grant::GrantSet::parse(
        "fs.write home:/Preferences\nfs.write pictures:/\nfs.write screenshots:/",
    )
    .unwrap();
    roots.guard_known_folders(
        &grants,
        &[(
            "screenshots",
            paths.0.join("Preferences/screencapture.plist"),
        )],
        &[],
    );
    set_roots(roots);
    for (path, succeeds) in [
        ("pictures:/pic.png", true),
        ("screenshots:/shot.png", false),
    ] {
        let result = run(&grants, None, FsOp::WriteFile, path, None, Some(b"png"));
        if succeeds {
            result.unwrap();
        } else {
            assert!(result.unwrap_err().to_string().contains("os error 13"));
        }
    }
    assert_eq!(std::fs::read(paths.0.join("pic.png")).unwrap(), b"png");
    assert!(!paths.0.join("shot.png").exists());
    set_roots(Default::default());
}

#[cfg(unix)]
#[test]
fn storage_preserves_resolver_access_denial() {
    use ibex2::stdlib::fs::{run, set_roots, FsOp, Roots};
    let _roots = KNOWN_ROOTS.lock().unwrap_or_else(|e| e.into_inner());
    let paths = Paths::new();
    let mut roots = Roots {
        home: Some(paths.0.clone()),
        known_folder: Some(std::sync::Arc::new(|_| {
            Err(std::io::Error::from_raw_os_error(13))
        })),
        ..Default::default()
    };
    let grants =
        ibex2::grant::GrantSet::parse("fs.write home:/.config\nfs.write pictures:/").unwrap();
    roots.guard_known_folders(&grants, &[], &[]);
    set_roots(roots);
    let error = run(
        &grants,
        None,
        FsOp::WriteFile,
        "pictures:/shot.png",
        None,
        Some(b"png"),
    )
    .unwrap_err()
    .to_string();
    assert!(error.contains("os error 13"), "{error}");
    set_roots(Default::default());
}
