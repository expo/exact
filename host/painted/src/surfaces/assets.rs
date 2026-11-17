//! GPU asset delivery and the messages it releases.
use super::Surfaces;
use crate::{image::Assets, Host};
use exact_runner::{DataSource, Event};
use serde_json::json;
use std::sync::Arc;

#[path = "../../../../gpu/src/asset_name.rs"]
mod names;

fn read_asset(assets: &Assets, name: &str) -> Result<Option<Arc<[u8]>>, String> {
    if !names::asset_name(name) {
        return Err("invalid asset name".into());
    }
    if name.starts_with("app:/") {
        return Ok(assets.read(name));
    }
    assets.read_asset(&format!("assets/{name}"))
}

impl Surfaces {
    pub(super) fn service_canvases<D: DataSource>(
        &mut self,
        host: &mut Host<D>,
        assets: &Assets,
    ) -> bool {
        let mut changed = false;
        for (&view, c) in &mut self.canvases {
            let abi = &self.abis[&c.artifact];
            let mut delivered = false;
            for _ in 0..16 {
                let names = abi
                    .read(b"gpu_assets", c.id)
                    .and_then(|b| {
                        serde_json::from_slice::<serde_json::Value>(&b)
                            .ok()
                            .and_then(|v| {
                                serde_json::from_value::<Vec<String>>(v["requests"].clone()).ok()
                            })
                    })
                    .unwrap_or_default();
                if names.is_empty() {
                    break;
                }
                for name in names {
                    delivered = true;
                    let bytes = match read_asset(assets, &name) {
                        Ok(bytes) => bytes,
                        Err(reason) => {
                            let ok = unsafe {
                                abi.symbol::<unsafe extern "C" fn(
                                    u32,
                                    *const u8,
                                    usize,
                                    *const u8,
                                    usize,
                                ) -> bool>(b"gpu_asset_failed")(
                                    c.id,
                                    name.as_ptr(),
                                    name.len(),
                                    reason.as_ptr(),
                                    reason.len(),
                                )
                            };
                            if !ok {
                                let error = abi.error().unwrap_or("asset delivery refused".into());
                                self.error =
                                    c.finish_restore(Some(error.clone()), None).or(Some(error));
                            }
                            continue;
                        }
                    };
                    let (ptr, len) = bytes
                        .as_ref()
                        .map_or((std::ptr::null(), 0), |b| (b.as_ptr(), b.len()));
                    let ok = unsafe {
                        abi.symbol::<unsafe extern "C" fn(u32, *const u8, usize, *const u8, usize) -> bool>(b"gpu_asset")(c.id, name.as_ptr(), name.len(), ptr, len)
                    };
                    if !ok {
                        let error = abi.error().unwrap_or("asset delivery refused".into());
                        self.error = c.finish_restore(Some(error.clone()), None).or(Some(error));
                    }
                }
            }
            if delivered {
                // Headless has no first frame to establish the ready world's
                // epoch. Do it after delivery, at the unchanged host clock.
                abi.agent(c.id, &json!({"op":"clock","now":host.now()}));
            }
            if c.restore_input {
                let state = abi.agent(c.id, &json!({"op":"state"}));
                c.finish_restore(None, Some(&state));
            }
            if let Some(bytes) = abi.read(b"gpu_published", c.id) {
                if c.owner {
                    let record = String::from_utf8_lossy(&bytes);
                    self.error = self
                        .error
                        .take()
                        .or(host.surface_record(&c.name, Some(&record)));
                    changed = true;
                }
            }
            if let Some(bytes) = abi.read(b"gpu_messages", c.id) {
                if let Ok(messages) = serde_json::from_slice::<Vec<String>>(&bytes) {
                    for message in messages {
                        if message == "exact:audio" || self.haptics.take(view, &message) {
                            continue;
                        }
                        self.error = self.error.take().or(host.dispatch_at(
                            view,
                            Event::Message(message),
                            host.now(),
                        ));
                        changed = true;
                    }
                }
            }
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn gpu_app_files_bypass_the_bundled_asset_namespace() {
        const CHILD: &str = "EXACT_GPU_APP_ASSET_TEST";
        if std::env::var_os(CHILD).is_none() {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args([
                    "--exact",
                    "surfaces::assets::tests::gpu_app_files_bypass_the_bundled_asset_namespace",
                ])
                .env(CHILD, "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stdout)
            );
            return;
        }
        let base =
            std::env::temp_dir().join(format!("exact-gpu-app-assets-{}", std::process::id()));
        crate::picker::set_roots(base.join("data"), base.join("cache"), base.join("tmp"));
        let assets = Assets::selected(PathBuf::new(), Arc::new(|_| Ok(None)));
        for root in ["data", "cache", "tmp"] {
            let path = format!(
                "app:/{root}/gpu-asset-{:?}.glb",
                std::thread::current().id()
            );
            let file = crate::picker::resolve(&path).unwrap();
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(&file, b"model").unwrap();
            let result = read_asset(&assets, &path);
            std::fs::remove_file(file).unwrap();
            assert_eq!(result.unwrap().as_deref(), Some(b"model".as_slice()));
        }
        assert!(read_asset(&assets, "app:/cache/no-such-gpu-model.glb")
            .unwrap()
            .is_none());
        let assets = Assets::selected(
            PathBuf::new(),
            Arc::new(|name| {
                assert_eq!(name, "assets/part.glb");
                Ok(Some(Arc::from(b"bundled".as_slice())))
            }),
        );
        assert_eq!(
            read_asset(&assets, "part.glb").unwrap().as_deref(),
            Some(b"bundled".as_slice())
        );
        std::fs::remove_dir_all(base).unwrap();
    }

    #[test]
    fn gpu_asset_names_are_validated_before_reading() {
        let assets = Assets::selected(PathBuf::new(), Arc::new(|_| panic!("must not read")));
        for name in [
            "app:/cache/../x",
            "app:/cache/./x",
            "app:/cache/café.glb",
            &format!("app:/cache/{}", "a".repeat(118)),
        ] {
            assert!(read_asset(&assets, name).is_err(), "{name}");
        }
    }
}
