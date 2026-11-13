//! Per-module runtime shader packs, admitted together before any source changes.
//! Device-free ownership deliberately never reads or registers shader sources.
use super::{Abi, Surfaces, LIMIT};
use crate::image::Assets;
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, io::Read, sync::Arc};

pub(super) type Pack = BTreeMap<String, Arc<[u8]>>;
pub(crate) type Packs = BTreeMap<String, Pack>;
type Shader = unsafe extern "C" fn(*const u8, usize, *const u8, usize) -> u32;

fn read_pack(compat: &Value, assets: &Assets, names: &[String]) -> Result<Pack, String> {
    let mut pack = Pack::new();
    let mut remaining = LIMIT;
    for name in names {
        if name.is_empty()
            || !name
                .bytes()
                .enumerate()
                .all(|(i, b)| b == b'_' || b.is_ascii_alphabetic() || (i > 0 && b.is_ascii_digit()))
            || pack.contains_key(name)
        {
            return Err(format!("shader `{name}`: invalid or duplicate roster name"));
        }
        let asset = format!("shaders/{name}.wgsl");
        let refuse = |reason: String| format!("shader `{name}` ({asset}): {reason}");
        let bytes = if assets.is_selected() {
            // The selected resolver owns its signed roster and integrity. Never
            // fall back to entry zero, even on a tombstone or integrity error.
            assets.read_asset(&asset).map_err(&refuse)?
        } else if let Some(path) = assets.path(&asset) {
            let file = std::fs::File::open(path).map_err(|e| refuse(e.to_string()))?;
            if file.metadata().map_err(|e| refuse(e.to_string()))?.len() > remaining as u64 {
                return Err(refuse("shader pack exceeds 256 MiB".into()));
            }
            // Bound the actual read too, including a file that grows after stat.
            let mut bytes = Vec::new();
            file.take(remaining as u64 + 1)
                .read_to_end(&mut bytes)
                .map_err(|e| refuse(e.to_string()))?;
            Some(Arc::from(bytes))
        } else {
            None
        }
        .ok_or_else(|| refuse("missing source".into()))?;
        if bytes.len() > remaining {
            return Err(refuse("shader pack exceeds 256 MiB".into()));
        }
        remaining -= bytes.len();
        std::str::from_utf8(&bytes).map_err(|e| refuse(format!("invalid UTF-8: {e}")))?;
        if !assets.is_selected() {
            let cards: Vec<_> = compat["embedded"]["assets"]
                .as_array()
                .into_iter()
                .flatten()
                .filter(|card| card["name"] == asset)
                .collect();
            if cards.len() != 1
                || cards[0]["bytes"].as_u64() != Some(bytes.len() as u64)
                || cards[0]["sha256"].as_str()
                    != Some(format!("{:x}", Sha256::digest(&bytes)).as_str())
            {
                return Err(refuse("baked size or digest mismatch".into()));
            }
        }
        pack.insert(name.clone(), bytes);
    }
    Ok(pack)
}

impl Abi {
    pub(super) fn prepare_shaders(
        &self,
        compat: &Value,
        assets: &Assets,
    ) -> Result<Option<Pack>, String> {
        if !self.device_loaded {
            return Ok(None);
        }
        let names = unsafe {
            self.library
                .get::<unsafe extern "C" fn() -> u32>(b"gpu_shader_names")
        }
        .map_err(|e| format!("GPU ABI gpu_shader_names: {e}"))?;
        let bytes = self.bytes(unsafe { names() }).ok_or_else(|| {
            self.error()
                .unwrap_or("GPU shader roster unavailable".into())
        })?;
        let names: Vec<String> =
            serde_json::from_slice(&bytes).map_err(|e| format!("GPU shader roster: {e}"))?;
        let pack = read_pack(compat, assets, &names)?;
        // Read integrity first: a cached good namespace must not hide a corrupt
        // selected generation. Ordinary surface binds never enter this path.
        if pack == self.shaders {
            return Ok(None);
        }
        for name in ["gpu_shader_validate", "gpu_shaders_clear", "gpu_shader"] {
            unsafe { self.library.get::<*const ()>(name.as_bytes()) }
                .map_err(|e| format!("GPU ABI {name}: {e}"))?;
        }
        for (name, bytes) in &pack {
            if unsafe {
                self.symbol::<Shader>(b"gpu_shader_validate")(
                    name.as_ptr(),
                    name.len(),
                    bytes.as_ptr(),
                    bytes.len(),
                )
            } != 0
            {
                return Err(format!(
                    "shader `{name}`: {}",
                    self.error().unwrap_or("validation refused".into())
                ));
            }
        }
        Ok(Some(pack))
    }

    pub(super) fn commit_shaders(&mut self, pack: Option<Pack>) -> Result<(), String> {
        let Some(pack) = pack else {
            return Ok(());
        };
        self.replace_shaders(&pack)?;
        self.shaders = pack;
        Ok(())
    }

    fn replace_shaders(&self, pack: &Pack) -> Result<(), String> {
        unsafe { self.symbol::<unsafe extern "C" fn()>(b"gpu_shaders_clear")() };
        for (name, bytes) in pack {
            if unsafe {
                self.symbol::<Shader>(b"gpu_shader")(
                    name.as_ptr(),
                    name.len(),
                    bytes.as_ptr(),
                    bytes.len(),
                )
            } != 0
            {
                // An ABI refusal after successful validation is exceptional,
                // but must not leave a partial namespace advertised as ready.
                let error = self.error().unwrap_or("registration refused".into());
                unsafe { self.symbol::<unsafe extern "C" fn()>(b"gpu_shaders_clear")() };
                for (old_name, old_bytes) in &self.shaders {
                    if unsafe {
                        self.symbol::<Shader>(b"gpu_shader")(
                            old_name.as_ptr(),
                            old_name.len(),
                            old_bytes.as_ptr(),
                            old_bytes.len(),
                        )
                    } != 0
                    {
                        return Err(format!(
                            "shader `{name}`: {error}; restoring prior namespace failed: {}",
                            self.error().unwrap_or_default()
                        ));
                    }
                }
                return Err(format!("shader `{name}`: {error}"));
            }
        }
        Ok(())
    }
}

impl Surfaces {
    pub(crate) fn prepare_shaders(&self, compat: &str, assets: &Assets) -> Result<Packs, String> {
        if !self.abis.values().any(|abi| abi.device_loaded) {
            return Ok(Packs::new());
        }
        let compat =
            serde_json::from_str(compat).map_err(|e| format!("shader compatibility: {e}"))?;
        let mut packs = Packs::new();
        for (artifact, abi) in &self.abis {
            if let Some(pack) = abi.prepare_shaders(&compat, assets)? {
                packs.insert(artifact.clone(), pack);
            }
        }
        Ok(packs)
    }

    /// Shader registration and selected-store acceptance are one transaction.
    /// No host/assets swap follows unless both succeed.
    pub(crate) fn activate_shaders(
        &mut self,
        packs: Packs,
        accept: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let previous = self.previous_shaders(&packs);
        self.commit_shaders(packs)?;
        if let Err(error) = accept() {
            self.commit_shaders(previous)
                .map_err(|rollback| format!("{error}; restoring shaders failed: {rollback}"))?;
            return Err(error);
        }
        Ok(())
    }

    fn previous_shaders(&self, packs: &Packs) -> Packs {
        packs
            .keys()
            .map(|artifact| (artifact.clone(), self.abis[artifact].shaders.clone()))
            .collect()
    }

    pub(crate) fn commit_shaders(&mut self, packs: Packs) -> Result<(), String> {
        let previous = self.previous_shaders(&packs);
        for (artifact, pack) in packs {
            if let Err(error) = self
                .abis
                .get_mut(&artifact)
                .unwrap()
                .commit_shaders(Some(pack))
            {
                // The refusing ABI restores itself. Restore every earlier module
                // too, before the candidate can reach the selected store.
                for (name, old) in previous.range(..artifact.clone()) {
                    self.abis
                        .get_mut(name)
                        .unwrap()
                        .commit_shaders(Some(old.clone()))
                        .map_err(|rollback| {
                            format!("{error}; restoring shaders failed: {rollback}")
                        })?;
                }
                return Err(error);
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
