//! The web host's integration tests: one binary, so one link and one launch.

mod again;
mod agent;
mod animation;
mod aria;
mod borders;
mod canvas;
mod collection;
mod dev;
mod document;
mod height;
mod height_drag;
mod holds;
mod host;
mod icon_rule;
mod lengths;
mod lists;
mod looks;
mod modes;
mod namespace;
mod navigation;
mod output_freeze;
mod page;
mod paint_order;
mod pan_release;
mod parity;
mod presence;
mod projection;
mod request;
mod request_refusal;
mod spacing;
mod springs;
mod surface_record;
mod symbol_color;
mod tables;
mod tint;
mod transform_drag;
mod video;

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
