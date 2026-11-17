//! Tests Observe's Linux launch part (`modules/observe/linux/launch.rs`)
//! against a local sink. A separate test binary because it installs a panic
//! hook and sets `XDG_STATE_HOME`.

mod observe {
    include!("../../../../modules/observe/linux/launch.rs");

    #[cfg(test)]
    mod tests;
    #[cfg(test)]
    mod wire;
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
