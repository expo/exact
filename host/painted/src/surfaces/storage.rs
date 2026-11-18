use super::Abi;
use std::path::PathBuf;

impl Abi {
    pub(super) fn storage(&self, id: u32, roots: &[PathBuf; 3]) -> bool {
        let [data, cache, temporary] = roots.each_ref().map(|p| path_bytes(p));
        // The module copies native path bytes during this call.
        unsafe {
            self.symbol::<unsafe extern "C" fn(
                u32,
                *const u8,
                usize,
                *const u8,
                usize,
                *const u8,
                usize,
            ) -> bool>(b"gpu_storage")(
                id,
                data.as_ptr(),
                data.len(),
                cache.as_ptr(),
                cache.len(),
                temporary.as_ptr(),
                temporary.len(),
            )
        }
    }
}

fn path_bytes(path: &std::path::Path) -> Vec<u8> {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        path.as_os_str().as_bytes().to_vec()
    }
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        path.as_os_str()
            .encode_wide()
            .flat_map(u16::to_le_bytes)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{image::Assets, Host};
    use exact_runner::{DataError, DataSource, Value};

    struct Data(&'static str);
    impl DataSource for Data {
        fn app_id(&self) -> &str {
            self.0
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!()
        }
    }

    #[test]
    fn each_canvas_gets_its_hosts_named_store_once() {
        const CHILD: &str = "EXACT_GPU_STORAGE_TEST";
        if std::env::var_os(CHILD).is_none() {
            let root = std::env::current_dir()
                .unwrap()
                .join(format!("target/exact-gpu-storage-{}", std::process::id()));
            std::fs::create_dir_all(&root).unwrap();
            #[cfg(windows)]
            let cache_root = root.clone();
            #[cfg(unix)]
            let cache_root = {
                use std::os::unix::ffi::OsStrExt;
                root.join(std::ffi::OsStr::from_bytes(b"cache-\xff"))
            };
            // APFS refuses such names on disk; delivery needs only the resolved path.
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "surfaces::storage::tests::each_canvas_gets_its_hosts_named_store_once",
                ])
                .env(CHILD, "1")
                .env("XDG_CACHE_HOME", &cache_root)
                .env("EXACT_AGENT", "1")
                .env("EXACT_AGENT_STORAGE", "gpu-files")
                .env_remove("EXACT_AGENT_STORAGE_FRESH")
                .output()
                .unwrap();
            std::fs::remove_dir_all(root).unwrap();
            assert!(
                output.status.success(),
                "{}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
            return;
        }
        let (path, mut compat) = super::super::tests::fixture();
        // The loader fixture normally probes oversized outputs; this surface is idle.
        let source = path.parent().unwrap().join("probe.c");
        let quiet = std::fs::read_to_string(&source)
            .unwrap()
            .replace("268435457", "0");
        std::fs::write(&source, quiet).unwrap();
        super::super::tests::compile_fixture(&source, &path);
        compat["embedded"]["gpu"]["sha256"] =
            super::super::digest::file(&path, true).unwrap().into();
        let abi = Abi::open_path(&path, &compat, "").unwrap();
        let assets = Assets::embedded(path.parent().unwrap().into());
        let mut surfaces = super::super::Surfaces::default();
        surfaces.abis.insert(String::new(), abi);
        surfaces.attempted.insert(String::new());
        for (index, app) in [
            "test.exact.first",
            "test.exact.second",
            "test.exact.refused",
        ]
        .into_iter()
        .enumerate()
        {
            let (mut host, error) = Host::boot(
                &crate::test_plan(
                    "component App\n  view\n    canvas surface=probe() width=10 height=10\n",
                ),
                Data(app),
                Box::new(exact_kernel::MonospaceMeasurer::default()),
                100.,
                100.,
            )
            .unwrap();
            assert!(error.is_none(), "{error:?}");
            let expected = host.app_roots().unwrap();
            #[cfg(unix)]
            assert!(expected.iter().all(|p| p.to_str().is_none()));
            assert!(expected
                .iter()
                .all(|p| p.parent().unwrap().file_name().unwrap() == "gpu-files"));
            // SAFETY: the fixture exports this storage-refusal switch.
            unsafe {
                surfaces.abis[""].symbol::<unsafe extern "C" fn(bool)>(b"test_refuse_storage")(
                    index == 2,
                );
            }
            surfaces.sync(&mut host, &compat.to_string(), &assets);
            assert!(surfaces.error.is_none(), "{:?}", surfaces.error);
            let abi = &surfaces.abis[""];
            // SAFETY: the fixture exports these signatures and retains the path buffer.
            unsafe {
                let paths = abi.symbol::<unsafe extern "C" fn() -> *const std::ffi::c_char>(
                    b"test_storage_paths",
                )();
                assert_eq!(
                    std::slice::from_raw_parts(
                        paths.cast::<u8>(),
                        abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_storage_len")() as usize
                    ),
                    expected
                        .iter()
                        .map(|p| path_bytes(p))
                        .collect::<Vec<_>>()
                        .join(&b'|')
                );
                assert_eq!(
                    abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_storage_calls")(),
                    index as u32 + 1
                );
                assert_eq!(
                    abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_storage_id")(),
                    1
                );
            }
            assert_eq!(surfaces.canvases.len(), 1);
            // SAFETY: the fixture counts successful binds through this export.
            unsafe {
                assert_eq!(
                    abi.symbol::<unsafe extern "C" fn() -> u32>(b"test_bind_calls")(),
                    index as u32 + 1
                );
            }
            surfaces.sync(&mut host, &compat.to_string(), &assets);
            assert_eq!(surfaces.canvases.len(), 1);
            assert_eq!(
                host.agent(r#"{"op":"logs"}"#)
                    .matches("surface probe: storage:")
                    .count(),
                usize::from(index == 2)
            );
            // SAFETY: a second sync must neither recreate nor redeliver storage.
            unsafe {
                assert_eq!(
                    surfaces.abis[""]
                        .symbol::<unsafe extern "C" fn() -> u32>(b"test_storage_calls")(
                    ),
                    index as u32 + 1
                );
            }
            surfaces.canvases.clear();
        }
    }
}
