//! LLP 1173 D4 qualification: run on Windows with OneDrive Pictures and a
//! UNC-redirected folder. Storage is exercised in temporary subdirectories.
//! To exercise a dehydrated child folder, append `-- pictures OnlineOnly` with its name.
//! Each step prints its answer; the probe removes what it creates.
#[cfg(windows)]
fn main() -> std::io::Result<()> {
    probe::run()
}

#[cfg(not(windows))]
fn main() {
    eprintln!("Run this probe on Windows.");
    std::process::exit(1);
}

#[cfg(windows)]
mod probe {
    #![allow(unsafe_code)]
    use ibex2::{
        grant::GrantSet,
        stdlib::fs::{self, FsOp, Roots},
    };
    use std::{
        fs::OpenOptions,
        io,
        os::windows::{fs::OpenOptionsExt, io::AsRawHandle},
        path::{Component, Path, PathBuf, Prefix},
        sync::Arc,
    };
    use windows_sys::{
        Wdk::Storage::FileSystem::RtlSetThreadPlaceholderCompatibilityMode,
        Win32::Storage::FileSystem::{
            FileAttributeTagInfo, GetFileInformationByHandleEx, FILE_ATTRIBUTE_TAG_INFO,
            FILE_FLAG_BACKUP_SEMANTICS, FILE_FLAG_OPEN_REPARSE_POINT,
        },
    };

    struct Compatibility(i8);
    impl Drop for Compatibility {
        fn drop(&mut self) {
            unsafe {
                RtlSetThreadPlaceholderCompatibilityMode(self.0);
            }
        }
    }

    pub fn run() -> io::Result<()> {
        let args: Vec<String> = std::env::args().skip(1).collect();
        if !args.is_empty()
            && (args.len() != 2
                || exact_data::known_folders::windows_id(&args[0]).is_none()
                || args[1].is_empty()
                || !args[1].split('/').all(|part| {
                    !part.is_empty() && part != "." && part != ".." && !part.contains(['\\', ':'])
                }))
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "expected a folder role and a relative child folder path using /",
            ));
        }
        // Windows can disguise cloud tags unless the querying thread asks for them.
        // https://learn.microsoft.com/windows-hardware/drivers/ddi/ntifs/nf-ntifs-rtlsetthreadplaceholdercompatibilitymode
        let previous = unsafe { RtlSetThreadPlaceholderCompatibilityMode(2) };
        if previous < 0 {
            return Err(io::Error::other(format!(
                "cannot expose placeholders: {previous}"
            )));
        }
        let _compatibility = Compatibility(previous);
        println!("placeholder compatibility: EXPOSE (previous {previous})");
        fs::set_roots(Roots {
            known_folder: Some(Arc::new(|name| {
                exact_data::known_folders::windows_storage(name).map(Into::into)
            })),
            ..Default::default()
        });
        let token = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        for name in [
            "desktop",
            "documents",
            "downloads",
            "music",
            "pictures",
            "videos",
            "screenshots",
        ] {
            println!("\n{name}:");
            match exact_data::known_folders::windows_storage(name) {
                Ok(folder) => inspect(name, &folder, "", token),
                Err(error) => println!("resolve: {error}"),
            }
        }
        if let [name, relative] = args.as_slice() {
            println!("\n{name}:/{relative}:");
            let folder = exact_data::known_folders::windows_storage(name)?;
            inspect(name, &folder.join(relative), relative, token);
        }
        fs::set_roots(Default::default());
        Ok(())
    }

    fn inspect(name: &str, folder: &Path, relative: &str, token: u128) {
        let unc = matches!(folder.components().next(), Some(Component::Prefix(p)) if matches!(p.kind(), Prefix::UNC(..) | Prefix::VerbatimUNC(..)));
        println!("path: {}\nUNC: {unc}", folder.display());
        let tag = (|| -> io::Result<FILE_ATTRIBUTE_TAG_INFO> {
            let file = OpenOptions::new()
                .read(true)
                .custom_flags(FILE_FLAG_BACKUP_SEMANTICS | FILE_FLAG_OPEN_REPARSE_POINT)
                .open(folder)?;
            let mut info = FILE_ATTRIBUTE_TAG_INFO::default();
            if unsafe {
                GetFileInformationByHandleEx(
                    file.as_raw_handle(),
                    FileAttributeTagInfo,
                    (&mut info as *mut FILE_ATTRIBUTE_TAG_INFO).cast(),
                    std::mem::size_of_val(&info) as u32,
                )
            } == 0
            {
                return Err(io::Error::last_os_error());
            }
            Ok(info)
        })();
        match tag {
            Ok(info) => println!(
                "attributes: {:#010x}\nreparse tag: {:#010x}",
                info.FileAttributes, info.ReparseTag
            ),
            Err(error) => println!("attributes/tag: {error}"),
        }
        let leaf = format!(".exact-known-probe-{}-{token}", std::process::id());
        let path = if relative.is_empty() {
            format!("{name}:/{leaf}")
        } else {
            format!("{name}:/{relative}/{leaf}")
        };
        // Only what this run made is removed, on every path out.
        let mut made = Cleanup(Vec::new());
        let grants = GrantSet::parse(&format!("fs.write {path}")).unwrap();
        let directory = folder.join(&leaf);
        if relative.is_empty() {
            let existed = directory.exists();
            let answer = fs::run(&grants, None, FsOp::Mkdir, &path, None, None);
            println!("mkdir: {answer:?}");
            made.note(&directory, existed, answer.is_ok(), "mkdir");
            let nested = format!("{name}:/{leaf}-nested/inner");
            let nested_grants = GrantSet::parse(&format!("fs.write {nested}")).unwrap();
            let outer = folder.join(format!("{leaf}-nested"));
            let existed = outer.exists();
            let answer = fs::run(&nested_grants, None, FsOp::Mkdir, &nested, None, None);
            println!("mkdir of two new levels: {answer:?}");
            made.note(&outer, existed, answer.is_ok(), "mkdir of two new levels");
        }
        if !directory.is_dir() {
            // Known-folder mkdir refuses existing components, so a child's
            // temporary directory (or one mkdir could not make) is made natively;
            // the steps below still walk from the OS-selected root.
            if let Err(error) = std::fs::create_dir(&directory) {
                println!("native temporary subdirectory: {error}");
                return;
            }
            made.0.push(directory);
        }
        println!(
            "mkdir collision (must be EEXIST): {:?}",
            fs::run(&grants, None, FsOp::Mkdir, &path, None, None)
        );
        for (label, op, target) in [
            (
                "CREATE_NEW in the new directory",
                FsOp::WriteFile,
                "created",
            ),
            (
                "CREATE_NEW collision (must be EEXIST)",
                FsOp::WriteFile,
                "created",
            ),
            ("no-replace rename", FsOp::AtomicWriteFile, "atomic"),
            (
                "rename collision (must be EEXIST)",
                FsOp::AtomicWriteFile,
                "atomic",
            ),
        ] {
            let answer = fs::run(
                &grants,
                None,
                op,
                &format!("{path}/{target}"),
                None,
                Some(b"Exact folder probe\n"),
            );
            println!("{label}: {answer:?}");
        }
    }

    /// Directories this run made, removed when the folder's steps end.
    struct Cleanup(Vec<PathBuf>);
    impl Cleanup {
        /// Keep `path` if the step made it, or if its failure left it behind,
        /// and say which; a path that existed before the step is never kept.
        fn note(&mut self, path: &Path, existed: bool, succeeded: bool, step: &str) {
            if existed {
                println!("{step}: {} existed before the step", path.display());
                return;
            }
            if succeeded {
                self.0.push(path.to_owned());
                return;
            }
            match path.try_exists() {
                Ok(left) => {
                    println!("{step} left its directory: {left}");
                    if left {
                        self.0.push(path.to_owned());
                    }
                }
                Err(error) => println!("{step} left its directory: unknown ({error})"),
            }
        }
    }
    impl Drop for Cleanup {
        fn drop(&mut self) {
            for directory in &self.0 {
                println!(
                    "cleanup {}: {:?}",
                    directory.display(),
                    std::fs::remove_dir_all(directory)
                );
            }
        }
    }
}
