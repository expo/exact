//! Shared by the art baker and generated game build scripts.
use std::path::Path;

pub(super) fn write_changed(path: &Path, bytes: &[u8]) -> Result<(), String> {
    use std::io::Write;
    if std::fs::read(path).ok().as_deref() == Some(bytes) {
        return Ok(());
    }
    let tmp = path.with_file_name(format!(
        ".{}.{}.tmp",
        path.file_name().unwrap().to_string_lossy(),
        std::process::id()
    ));
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&tmp)
        .map_err(|e| format!("{}: {e}", tmp.display()))?;
    let result = file
        .write_all(bytes)
        .and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result.map_err(|e| format!("{}: {e}", path.display()))
}

/// Validate all tuning files together and publish one indivisible asset transaction.
pub fn bake_game_tables<G: exact_game::Game>(app: impl AsRef<Path>) -> Result<(), String> {
    if G::TABLES.is_empty() {
        return Ok(());
    }
    let mut rows = std::collections::BTreeMap::new();
    for table in G::TABLES {
        let path = app.as_ref().join(table.name);
        println!("cargo:rerun-if-changed={}", path.display());
        let bytes = std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))?;
        check_size(table.name, bytes.len())?;
        let text = String::from_utf8(bytes).map_err(|e| format!("{}: {e}", table.name))?;
        rows.insert(table.name.to_owned(), text);
    }
    let text = exact_game::json::to_string(&rows).map_err(|e| e.to_string())?;
    exact_game::asset::validate_tables::<G>(&text).map_err(|e| e.to_string())?;
    let output = app
        .as_ref()
        .join("assets")
        .join(exact_game::asset::TABLE_ASSET);
    std::fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
    write_changed(&output, text.as_bytes())
}

/// Only streamed audio needs a startup catalog. The total resident PCM includes both declarations.
pub(super) fn sound_catalog<G: exact_game::Game>(
    mut read: impl FnMut(&str) -> Result<Vec<u8>, String>,
) -> Result<Option<Vec<u8>>, String> {
    use exact_game::asset::{asset_name, Content, SOUND_BYTE_BUDGET};
    if !G::STREAMED.iter().any(|name| name.ends_with(".sound")) {
        return Ok(None);
    }
    let mut rows = std::collections::BTreeMap::new();
    let mut total = 0;
    let mut counted = std::collections::BTreeSet::new();
    for (&name, streamed) in G::ASSETS
        .iter()
        .map(|n| (n, false))
        .chain(G::STREAMED.iter().map(|n| (n, true)))
    {
        if !name.ends_with(".sound") {
            continue;
        }
        if !counted.insert(name) {
            if streamed {
                return Err(format!("streamed sound `{name}` was declared twice"));
            }
            continue;
        }
        if !asset_name(name) {
            return Err(format!("invalid sound name `{name}`"));
        }
        let Content::Sound(sound) =
            Content::decode::<false>(name, &read(name)?).map_err(|e| format!("{name}: {e}"))?
        else {
            return Err(format!("{name} is not a sound"));
        };
        total += sound.pcm_bytes();
        if total > SOUND_BYTE_BUDGET {
            return Err(format!(
                "sound `{name}` exceeds the combined {} MiB sound residency budget",
                SOUND_BYTE_BUDGET / (1024 * 1024)
            ));
        }
        if streamed {
            rows.insert(name.to_owned(), sound.info());
        }
    }
    Ok(Some(exact_game::bin::to_vec(&rows)))
}

/// Publish the same sound catalog that hostless tests load from memory.
pub fn bake_game_sounds<G: exact_game::Game>(app: impl AsRef<Path>) -> Result<(), String> {
    let dir = app.as_ref().join("assets");
    let output = dir.join(exact_game::asset::SOUND_CATALOG);
    if let Some(bytes) = sound_catalog::<G>(|name| {
        let path = dir.join(name);
        println!("cargo:rerun-if-changed={}", path.display());
        std::fs::read(&path).map_err(|e| format!("{}: {e}", path.display()))
    })? {
        write_changed(&output, &bytes)
    } else {
        match std::fs::remove_file(&output) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => {
                Err(format!("{}: {e}", output.display()))
            }
            _ => Ok(()),
        }
    }
}

/// Refuse oversized carriers before decoding or writing them.
pub fn check_size(name: &str, size: usize) -> Result<(), String> {
    if size > 64 * 1024 * 1024 {
        Err(format!("asset `{name}` exceeds 64 MiB"))
    } else {
        Ok(())
    }
}

/// Validate the game's single declared JSON level using its `Data` derive, then
/// deliver it separately from executable code. No source bytes enter the module.
pub fn bake_game_level<G: exact_game::Game>(app: impl AsRef<Path>) -> Result<(), String> {
    let receipt = app.as_ref().join(".shells/level.json");
    let Some(level) = G::LEVEL else {
        return match std::fs::remove_file(&receipt) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(format!("{}: {e}", receipt.display())),
        };
    };
    let source = app.as_ref().join(level.name);
    println!("cargo:rerun-if-changed={}", source.display());
    let bytes = std::fs::read(&source).map_err(|e| format!("{}: {e}", source.display()))?;
    check_size(level.name, bytes.len())?;
    let text = std::str::from_utf8(&bytes).map_err(|e| format!("{}: {e}", level.name))?;
    level.decode(text).map_err(|e| e.to_string())?;
    let output = app.as_ref().join("assets").join(level.name);
    std::fs::create_dir_all(output.parent().unwrap()).map_err(|e| e.to_string())?;
    write_changed(&output, &bytes)?;
    // Input inventories must distinguish this output from handwritten assets.
    let name = exact_game::json::to_string(&level.name.to_owned()).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(receipt.parent().unwrap()).map_err(|e| e.to_string())?;
    write_changed(&receipt, name.as_bytes())
}
