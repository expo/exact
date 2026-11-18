//! `home:/` and `bundle:/` (Exact patch 24): admitted as spelt, `bundle:`
//! read only, run below the host's directories with every component opened
//! `O_NOFOLLOW`, so a link anywhere below a root is refused, not followed.
use super::*;

/// The roots are the process's: one test names them at a time.
pub(super) static ROOTED: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// A home with `.t3` granted and a bundle with `Resources`, beside a
/// directory outside both; their roots named for this process.
fn roots(name: &str) -> (PathBuf, PathBuf, GrantSet) {
    let base = std::env::temp_dir().join(format!("ibex2-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&base);
    let (home, bundle) = (base.join("home"), base.join("bundle"));
    std::fs::create_dir_all(home.join(".t3")).unwrap();
    std::fs::create_dir_all(bundle.join("Resources")).unwrap();
    std::fs::create_dir_all(base.join("outside")).unwrap();
    std::fs::write(bundle.join("Resources/runtime.tar"), b"tar").unwrap();
    std::fs::write(base.join("outside/secret"), b"no").unwrap();
    set_roots(Roots {
        home: Some(home.clone()),
        bundle: Some(bundle),
        known_folder: None,
    });
    let grants =
        GrantSet::parse("fs.write home:/.t3\nfs.read home:/.t3\nfs.read bundle:/Resources")
            .unwrap();
    (base, home, grants)
}

fn call(
    grants: &GrantSet,
    op: FsOp,
    path: &str,
    dest: Option<&str>,
    data: Option<&[u8]>,
) -> Result<FsResult, HostError> {
    run(grants, None, op, path, dest, data)
}

#[test]
fn home_and_bundle_paths_run_on_the_hosts_directories() {
    let _rooted = ROOTED.lock().unwrap_or_else(|e| e.into_inner());
    let (base, home, grants) = roots("namespaces");
    let call = |op, path: &str, dest: Option<&str>, data: Option<&[u8]>| {
        call(&grants, op, path, dest, data)
    };
    call(
        FsOp::WriteFile,
        "home:/.t3/settings.json",
        None,
        Some(b"{}"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(home.join(".t3/settings.json")).unwrap(),
        b"{}"
    );
    assert_eq!(
        call(FsOp::ReadFile, "home:/.t3/settings.json", None, None).unwrap(),
        FsResult::Bytes(b"{}".to_vec())
    );
    // Copied out of the bundle into the home directory, as an install does.
    call(
        FsOp::CopyFile,
        "bundle:/Resources/runtime.tar",
        Some("home:/.t3/runtime.tar"),
        None,
    )
    .unwrap();
    assert_eq!(std::fs::read(home.join(".t3/runtime.tar")).unwrap(), b"tar");
    let denied = |r: Result<FsResult, HostError>| format!("{r:?}");
    assert!(denied(call(
        FsOp::WriteFile,
        "bundle:/Resources/x",
        None,
        Some(b"x")
    ))
    .contains("denied"));
    assert!(denied(call(FsOp::ReadFile, "home:/.ssh/config", None, None)).contains("fs.read"));
    assert!(denied(call(FsOp::ReadFile, "home:/.t3/../.ssh", None, None)).contains(". or .."));
    // A link itself is the link's to read: an lstat or a readlink names it,
    // and nothing follows it.
    std::os::unix::fs::symlink(base.join("outside"), home.join(".t3/escape")).unwrap();
    let FsResult::Stat(link) = call(FsOp::Lstat, "home:/.t3/escape", None, None).unwrap() else {
        panic!()
    };
    assert!(link.is_symlink);
    assert_eq!(
        call(FsOp::Readlink, "home:/.t3/escape", None, None).unwrap(),
        FsResult::Text(base.join("outside").display().to_string())
    );
    assert!(call(FsOp::ReadFile, "home:/.t3/escape/secret", None, None).is_err());
    assert!(call(FsOp::Stat, "home:/.t3/escape", None, None).is_err());
    let realpath = call(FsOp::Realpath, "home:/.t3/settings.json", None, None).unwrap();
    assert_eq!(
        realpath,
        FsResult::Text(
            std::fs::canonicalize(home.join(".t3/settings.json"))
                .unwrap()
                .display()
                .to_string()
        )
    );
}

#[test]
fn a_link_the_grants_holder_plants_reaches_nothing_outside_the_grant() {
    let _rooted = ROOTED.lock().unwrap_or_else(|e| e.into_inner());
    let (base, home, grants) = roots("planted");
    let call = |op, path: &str, data: Option<&[u8]>| call(&grants, op, path, None, data);
    let outside = base.join("outside");
    // The granted directory itself replaced by a link that leads outside: its
    // file is not read through it.
    std::fs::remove_dir(home.join(".t3")).unwrap();
    let target = outside.display().to_string();
    call(FsOp::Symlink, "home:/.t3", Some(target.as_bytes())).unwrap();
    assert!(home.join(".t3").is_symlink());
    let read = call(FsOp::ReadFile, "home:/.t3/secret", None);
    assert!(read.is_err(), "{read:?}");
    assert!(call(FsOp::Lstat, "home:/.t3/secret", None).is_err());
    std::fs::remove_file(home.join(".t3")).unwrap();
    std::fs::create_dir(home.join(".t3")).unwrap();
    // A dangling link inside it: a write does not make its target, and a
    // chmod, a digest or an access does not reach through it.
    let created = outside.join("created").display().to_string();
    call(FsOp::Symlink, "home:/.t3/p", Some(created.as_bytes())).unwrap();
    let write = call(FsOp::WriteFile, "home:/.t3/p", Some(b"x"));
    assert!(write.is_err(), "{write:?}");
    assert!(!outside.join("created").exists());
    std::os::unix::fs::symlink(outside.join("secret"), home.join(".t3/secret")).unwrap();
    for op in [FsOp::ReadFile, FsOp::Digest, FsOp::Access, FsOp::Stat] {
        let data: &[u8] = if op == FsOp::Digest { b"SHA-256" } else { b"4" };
        let r = call(op, "home:/.t3/secret", Some(data));
        assert!(r.is_err(), "{op:?}: {r:?}");
    }
    let mode = |p: &Path| {
        std::os::unix::fs::PermissionsExt::mode(&std::fs::metadata(p).unwrap().permissions())
            & 0o777
    };
    let before = mode(&outside.join("secret"));
    assert!(call(FsOp::Chmod, "home:/.t3/secret", Some(b"511")).is_err());
    assert_eq!(
        mode(&outside.join("secret")),
        before,
        "chmod followed the link"
    );
    // The link itself still answers for what it is.
    assert!(
        matches!(call(FsOp::Lstat, "home:/.t3/p", None), Ok(FsResult::Stat(s)) if s.is_symlink)
    );
}

/// A `bundle:` root installed at a folder inside the app (the shape iOS and
/// tvOS use, exact2 LLP 1133.007 D6) confines its paths to that folder: a
/// file beside it is not under it, `..` is refused, no `fs.write bundle:`
/// line parses, and with `home:` unset a `home:` path refuses as
/// unsupported. Which folder the host names is exact2's to test
/// (`host/apple/tests/bundle_root.rs`).
#[test]
fn a_bundle_root_of_declared_trees_reaches_nothing_beside_them() {
    let _rooted = ROOTED.lock().unwrap_or_else(|e| e.into_inner());
    let app = std::env::temp_dir().join(format!("ibex2-declared-{}.app", std::process::id()));
    let _ = std::fs::remove_dir_all(&app);
    std::fs::create_dir_all(app.join("ExactResources/Resources/data")).unwrap();
    std::fs::write(app.join("ExactResources/Resources/data/cities.json"), b"[]").unwrap();
    std::fs::write(app.join("Info.plist"), b"plist").unwrap();
    set_roots(Roots {
        home: None,
        bundle: Some(app.join("ExactResources")),
        known_folder: None,
    });
    let grants =
        GrantSet::parse("fs.read bundle:/Resources/data\nfs.read bundle:/Info.plist").unwrap();
    let read = |path: &str| call(&grants, FsOp::ReadFile, path, None, None);
    assert_eq!(
        read("bundle:/Resources/data/cities.json").unwrap(),
        FsResult::Bytes(b"[]".to_vec())
    );
    // The app's own files are beside the root, not below it.
    let beside = format!("{:?}", read("bundle:/Info.plist"));
    assert!(beside.contains("os error 2"), "{beside}");
    assert!(
        format!("{:?}", read("bundle:/Resources/data/../../../Info.plist")).contains(". or ..")
    );
    assert!(GrantSet::parse("fs.write bundle:/Resources/data").is_err());
    let home = GrantSet::parse("fs.read home:/.t3").unwrap();
    let got = format!(
        "{:?}",
        call(&home, FsOp::ReadFile, "home:/.t3/x", None, None)
    );
    assert!(
        got.contains("unsupported: no home: directory here"),
        "{got}"
    );
    std::fs::remove_dir_all(&app).unwrap();
}

#[test]
fn known_folder_realpath_needs_no_file_read_access_and_refuses_links() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let _rooted = ROOTED.lock().unwrap_or_else(|e| e.into_inner());
    let (base, home, _) = roots("known-realpath");
    let folder = home.clone();
    set_roots(Roots {
        known_folder: Some(std::sync::Arc::new(move |_| Ok(folder.clone().into()))),
        ..Default::default()
    });
    let grants = GrantSet::parse("fs.write pictures:/").unwrap();
    std::fs::write(home.join("write-only"), b"png").unwrap();
    std::fs::set_permissions(
        home.join("write-only"),
        std::fs::Permissions::from_mode(0o200),
    )
    .unwrap();
    let result = call(&grants, FsOp::Realpath, "pictures:/write-only", None, None);
    assert_eq!(
        result.unwrap(),
        FsResult::Text(
            std::fs::canonicalize(home.join("write-only"))
                .unwrap()
                .display()
                .to_string()
        )
    );
    for (leaf, target) in [("link", "write-only"), ("dangling", "absent")] {
        symlink(target, home.join(leaf)).unwrap();
        let result = call(
            &grants,
            FsOp::Realpath,
            &format!("pictures:/{leaf}"),
            None,
            None,
        );
        assert!(result
            .unwrap_err()
            .to_string()
            .contains(&format!("os error {}", libc::ELOOP)));
    }
    assert!(
        call(&grants, FsOp::Realpath, "pictures:/absent", None, None)
            .unwrap_err()
            .to_string()
            .contains("os error 2")
    );
    let FsResult::Text(root) = call(&grants, FsOp::Realpath, "pictures:/", None, None).unwrap()
    else {
        panic!("realpath did not return a path")
    };
    assert_eq!(PathBuf::from(root), std::fs::canonicalize(&home).unwrap());
    set_roots(Roots::default());
    std::fs::remove_dir_all(base).unwrap();
}

#[test]
fn known_folders_only_create_and_documents_is_not_doc() {
    let _rooted = ROOTED.lock().unwrap_or_else(|e| e.into_inner());
    let (base, home, _) = roots("known");
    let current = std::sync::Arc::new(std::sync::Mutex::new(home.clone()));
    let resolve = current.clone();
    set_roots(Roots {
        known_folder: Some(std::sync::Arc::new(move |name| {
            (name != "downloads")
                .then(|| resolve.lock().unwrap().clone().into())
                .ok_or_else(|| std::io::Error::from_raw_os_error(2))
        })),
        ..Roots::default()
    });
    let grants = GrantSet::parse(
        "fs.write pictures:/
fs.write documents:/
fs.write downloads:/
fs.read app:/tmp",
    )
    .unwrap();
    for (op, leaf) in [
        (FsOp::WriteFile, "plain"),
        (FsOp::AtomicWriteFile, "atomic"),
    ] {
        let path = format!("pictures:/{leaf}");
        call(&grants, op, &path, None, Some(b"first")).unwrap();
        let error = call(&grants, op, &path, None, Some(b"replaced"))
            .unwrap_err()
            .to_string();
        assert!(error.contains("os error 17"), "{error}");
        assert_eq!(std::fs::read(home.join(leaf)).unwrap(), b"first");
    }
    run_with_documents(
        &grants,
        None,
        None,
        FsOp::WriteFile,
        "documents:/note",
        None,
        Some(b"doc"),
    )
    .unwrap();
    assert_eq!(std::fs::read(home.join("note")).unwrap(), b"doc");
    let real = call(&grants, FsOp::Realpath, "documents:/note", None, None).unwrap();
    assert_eq!(
        real,
        FsResult::Text(
            std::fs::canonicalize(home.join("note"))
                .unwrap()
                .display()
                .to_string()
        )
    );
    call(&grants, FsOp::Mkdir, "pictures:/Exports/nested", None, None).unwrap();
    assert!(home.join("Exports/nested").is_dir());
    for op in [
        FsOp::ReadFile,
        FsOp::ReadDir,
        FsOp::Remove,
        FsOp::Stat,
        FsOp::AppendFile,
        FsOp::Lstat,
        FsOp::Readlink,
        FsOp::Access,
        FsOp::Chmod,
        FsOp::Symlink,
        FsOp::Digest,
        FsOp::Flock,
        FsOp::Unlock,
    ] {
        assert!(
            call(&grants, op, "pictures:/plain", None, Some(b"x"))
                .unwrap_err()
                .to_string()
                .starts_with("denied:"),
            "{op:?}"
        );
    }
    for (from, to) in [
        ("pictures:/plain", "app:/tmp/x"),
        ("pictures:/plain", "pictures:/x"),
        ("home:/.t3/x", "pictures:/x"),
    ] {
        for op in [FsOp::CopyFile, FsOp::Rename] {
            assert!(call(&grants, op, from, Some(to), None)
                .unwrap_err()
                .to_string()
                .starts_with("denied:"));
        }
    }
    for leaf in ["data", "cache", "tmp"] {
        std::fs::create_dir_all(base.join(leaf)).unwrap();
    }
    let dirs = crate::stdlib::app_fs::AppDirectories::new(
        base.join("data"),
        base.join("cache"),
        base.join("tmp"),
    )
    .unwrap();
    std::fs::write(base.join("tmp/capture"), b"png").unwrap();
    let copy = || {
        run(
            &grants,
            Some(&dirs),
            FsOp::CopyFile,
            "app:/tmp/capture",
            Some("pictures:/copy"),
            None,
        )
    };
    copy().unwrap();
    assert!(copy().unwrap_err().to_string().contains("os error 17"));
    assert_eq!(std::fs::read(home.join("copy")).unwrap(), b"png");
    assert!(run(
        &GrantSet::parse("fs.write pictures:/").unwrap(),
        Some(&dirs),
        FsOp::CopyFile,
        "app:/tmp/capture",
        Some("pictures:/denied"),
        None
    )
    .is_err());
    assert!(!home.join("denied").exists());
    let missing = call(&grants, FsOp::WriteFile, "downloads:/x", None, Some(b"x"))
        .unwrap_err()
        .to_string();
    assert!(missing.contains("os error 2"), "{missing}");
    std::os::unix::fs::symlink(base.join("outside"), home.join("escape")).unwrap();
    assert!(call(
        &grants,
        FsOp::WriteFile,
        "pictures:/escape/x",
        None,
        Some(b"x")
    )
    .is_err());
    assert!(call(&grants, FsOp::WriteFile, "pictures:/../x", None, Some(b"x")).is_err());
    *current.lock().unwrap() = base.join("outside");
    call(
        &grants,
        FsOp::WriteFile,
        "pictures:/moved",
        None,
        Some(b"new root"),
    )
    .unwrap();
    assert_eq!(
        std::fs::read(base.join("outside/moved")).unwrap(),
        b"new root"
    );
    set_roots(Roots::default());
    for op in [FsOp::WriteFile, FsOp::Remove, FsOp::ReadFile] {
        assert!(call(&grants, op, "pictures:/x", None, None)
            .unwrap_err()
            .to_string()
            .starts_with("NotSupportedError:"));
    }
    std::fs::remove_dir_all(base).unwrap();
}
