//! Tests Observe's painted-host launch parts against a local sink. A separate
//! test binary because it installs a panic hook and sets process environment.

mod observe {
    include!("../../../../modules/observe/linux/launch.rs");

    #[cfg(test)]
    mod tests;
    #[cfg(test)]
    mod wire;
}

#[allow(dead_code)]
mod android {
    include!("../../../../modules/observe/android/launch.rs");

    #[cfg(not(target_os = "android"))]
    fn property(_: &std::ffi::CStr) -> Option<String> {
        None
    }

    #[test]
    fn replacing_the_android_apk_starts_a_cold_launch() {
        let dir = crate::TestDir::new("observe-android-build");
        let apk = dir.join("app.apk");
        let previous = std::env::var_os("EXACT_ANDROID_APK");
        std::env::set_var("EXACT_ANDROID_APK", &apk);
        std::fs::write(&apk, b"app build 1").unwrap();
        let service = Service::new(
            Map::new(),
            &json!({}),
            false,
            dir.to_path_buf(),
            "session".into(),
            0.,
        );
        let boot = Some("same-os-boot".to_owned());
        let first = build_identity();
        assert!(first.is_some(), "the APK must identify the build");
        assert_eq!(
            service.launch_type_for(boot.clone(), first.clone(), false),
            "cold"
        );
        assert_eq!(
            service.launch_type_for(boot.clone(), build_identity(), false),
            "warm"
        );
        let modified = std::fs::metadata(&apk).unwrap().modified().unwrap();
        std::fs::write(&apk, b"app build 2").unwrap();
        std::fs::File::options()
            .write(true)
            .open(&apk)
            .unwrap()
            .set_modified(modified)
            .unwrap();
        assert_ne!(build_identity(), first, "an app update changed the build");
        assert_eq!(
            service.launch_type_for(boot.clone(), build_identity(), false),
            "cold"
        );
        assert_eq!(
            service.launch_type_for(boot.clone(), build_identity(), false),
            "warm"
        );
        assert_eq!(
            service.launch_type_for(boot.clone(), build_identity(), true),
            "cold"
        );
        std::fs::remove_file(&apk).unwrap();
        assert_eq!(
            service.launch_type_for(boot, build_identity(), false),
            "cold"
        );
        match previous {
            Some(value) => std::env::set_var("EXACT_ANDROID_APK", value),
            None => std::env::remove_var("EXACT_ANDROID_APK"),
        }
    }
}

/// An empty directory `<name>-<pid>` under the temp dir, removed when dropped.
struct TestDir(std::path::PathBuf);

impl TestDir {
    fn new(name: &str) -> TestDir {
        let dir = std::env::temp_dir().join(format!("{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TestDir(dir)
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

impl std::ops::Deref for TestDir {
    type Target = std::path::Path;
    fn deref(&self) -> &std::path::Path {
        &self.0
    }
}
