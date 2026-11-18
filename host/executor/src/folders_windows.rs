//! Windows storage roots without enabling child processes or home: (LLP 1173 D4).
use ibex2::stdlib::fs::{copy_io_error, KnownFolderResolver, Roots, WindowsAgentFolders};
use std::{path::PathBuf, sync::Arc};

pub(super) fn from_env() -> Roots {
    let agent_home = std::env::var("EXACT_AGENT")
        .is_ok_and(|v| v == "1")
        .then(|| {
            std::env::var_os("EXACT_AGENT_HOME")
                .map(PathBuf::from)
                .unwrap_or_else(|| {
                    std::env::temp_dir().join(format!("exact-agent-home-{}", std::process::id()))
                })
        });
    roots(agent_home)
}

fn roots(agent_home: Option<PathBuf>) -> Roots {
    let resolve: KnownFolderResolver = match agent_home {
        Some(home) => {
            let folders = WindowsAgentFolders::new(&home);
            Arc::new(move |name| match &folders {
                Ok(folders) => folders.resolve(name),
                Err(error) => Err(copy_io_error(error)),
            })
        }
        None => Arc::new(|name| exact_data::known_folders::windows_storage(name).map(Into::into)),
    };
    Roots {
        known_folder: Some(resolve),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_roots_remain_inside_the_drives_home() {
        let home =
            std::env::temp_dir().join(format!("exact-windows-folders-{}", std::process::id()));
        let roots = roots(Some(home.clone()));
        assert!(roots.home.is_none());
        assert!(roots.bundle.is_none());
        let resolve = roots.known_folder.unwrap();
        for name in [
            "desktop",
            "documents",
            "downloads",
            "music",
            "pictures",
            "videos",
            "screenshots",
        ] {
            let folder = resolve(name).unwrap();
            assert!(folder
                .path
                .starts_with(std::fs::canonicalize(&home).unwrap()));
            assert!(folder.directory.unwrap().metadata().unwrap().is_dir());
        }
        assert!(home.join("Pictures/Screenshots").is_dir());
        drop(resolve);
        std::fs::remove_dir_all(home).unwrap();
    }
}
