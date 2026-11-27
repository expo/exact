//! The store's files: writes that survive a crash, whole-or-absent
//! temporaries, reads checked against their signed cards, and content kept
//! once by digest.
//!
//! @ref LLP 1026 D11 (crash recovery; assets by digest)

use super::EmbeddedAsset;
use crate::envelope::{resolve_url, sha256_hex, CohortReceipt, FileCard};
use std::collections::BTreeSet;
#[cfg(not(test))]
use std::fs::hard_link;
use std::io::{Read as _, Write as _};
use std::path::{Path, PathBuf};
#[cfg(test)]
use tests::hard_link;

/// Refuse an unqualified durable-store platform before touching its directory.
pub(super) fn require_durable_store() -> Result<(), String> {
    if cfg!(windows) {
        Err("Windows durable update storage is unsupported (LLP 1026 D11a); no store files were touched".into())
    } else {
        Ok(())
    }
}

/// Remove every `.tmp-…` left in `dir` by a write that did not finish.
pub(super) fn sweep(dir: &Path) {
    let Ok(read) = std::fs::read_dir(dir) else {
        return;
    };
    for item in read.flatten() {
        if !item.file_name().to_string_lossy().starts_with(".tmp-") {
            continue;
        }
        let path = item.path();
        if path.is_dir() {
            let _ = std::fs::remove_dir_all(path);
        } else {
            let _ = std::fs::remove_file(path);
        }
    }
}

/// Take `file`'s advisory lock without waiting, exclusive or shared;
/// `Ok(false)` when another holder has it. std's `File::try_lock` is
/// unsupported on Android, so there it is `flock(2)`, as std's is on Linux.
pub(super) fn try_lock(file: &std::fs::File, exclusive: bool) -> std::io::Result<bool> {
    #[cfg(target_os = "android")]
    {
        use std::os::fd::AsRawFd as _;
        let kind = if exclusive {
            libc::LOCK_EX
        } else {
            libc::LOCK_SH
        };
        // SAFETY: `flock` reads only the descriptor, which `file` owns.
        #[allow(unsafe_code)]
        let done = unsafe { libc::flock(file.as_raw_fd(), kind | libc::LOCK_NB) };
        if done == 0 {
            return Ok(true);
        }
        let error = std::io::Error::last_os_error();
        if error.kind() == std::io::ErrorKind::WouldBlock {
            Ok(false)
        } else {
            Err(error)
        }
    }
    #[cfg(not(target_os = "android"))]
    {
        let taken = if exclusive {
            file.try_lock()
        } else {
            file.try_lock_shared()
        };
        match taken {
            Ok(()) => Ok(true),
            Err(std::fs::TryLockError::WouldBlock) => Ok(false),
            Err(std::fs::TryLockError::Error(e)) => Err(e),
        }
    }
}

/// A shared lock on an entry's `exact.json`, for a process that runs the
/// entry without owning the store. [`remove_entry`] keeps an entry so held.
pub(super) fn hold_entry(entry: &Path) -> Result<std::fs::File, String> {
    let path = entry.join("exact.json");
    let file =
        std::fs::File::open(&path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    match try_lock(&file, false) {
        Ok(true) => Ok(file),
        Ok(false) => Err(format!(
            "cannot hold {}: it is being removed",
            path.display()
        )),
        Err(e) => Err(format!("cannot hold {}: {e}", path.display())),
    }
}

/// Remove an entry unless a reader holds it ([`hold_entry`]); `false` when
/// one does. The exclusive lock taken here lasts through the removal, so a
/// reader that locks after it finds the entry gone.
pub(super) fn remove_entry(entry: &Path) -> bool {
    let lock = std::fs::File::open(entry.join("exact.json"));
    if let Ok(file) = &lock {
        if matches!(try_lock(file, true), Ok(false)) {
            return false;
        }
    }
    let _ = std::fs::remove_dir_all(entry);
    true
}

/// Take an exclusive non-blocking advisory lock on an entry's `exact.json` so
/// replacement never touches a directory a secondary reader runs ([`hold_entry`]).
/// Returns `Ok(None)` when `exact.json` is absent.
pub(super) fn lock_entry(entry: &Path) -> Result<Option<std::fs::File>, String> {
    let path = entry.join("exact.json");
    let file = match std::fs::File::open(&path) {
        Ok(file) => file,
        Err(e)
            if matches!(
                e.kind(),
                std::io::ErrorKind::NotFound | std::io::ErrorKind::NotADirectory
            ) =>
        {
            return Ok(None);
        }
        Err(e) => return Err(format!("cannot open {}: {e}", path.display())),
    };
    match try_lock(&file, true) {
        Ok(true) => Ok(Some(file)),
        Ok(false) => Err(format!(
            "cannot replace {}: another instance is using it",
            entry.display()
        )),
        Err(e) => Err(format!("cannot lock {}: {e}", path.display())),
    }
}

fn remove_path(path: &Path) -> Result<(), String> {
    match std::fs::symlink_metadata(path) {
        Ok(meta) if meta.file_type().is_dir() => std::fs::remove_dir_all(path),
        Ok(_) => std::fs::remove_file(path),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) => Err(e),
    }
    .map_err(|e| format!("cannot clear {}: {e}", path.display()))
}

/// Replace an existing entry's payload files from `tmp` while keeping its
/// verified `exact.json` floor witness intact and publishing `app.plan` last.
pub(super) fn replace_entry_in_place(tmp: &Path, target: &Path) -> Result<(), String> {
    remove_path(&target.join("app.plan"))?;
    remove_path(&target.join("assets"))?;
    if tmp.join("assets").exists() {
        std::fs::rename(tmp.join("assets"), target.join("assets"))
            .map_err(|e| format!("cannot replace assets: {e}"))?;
    }
    remove_path(&target.join("assets.json"))?;
    if tmp.join("assets.json").exists() {
        std::fs::rename(tmp.join("assets.json"), target.join("assets.json"))
            .map_err(|e| format!("cannot replace assets.json: {e}"))?;
    }
    sync_dir(target)?;
    std::fs::rename(tmp.join("app.plan"), target.join("app.plan"))
        .map_err(|e| format!("cannot replace app.plan: {e}"))?;
    sync_dir(target)
}

/// Swap a verified `tmp` directory into an existing `target` that holds no
/// valid witness, restoring `target` if the rename into place fails.
pub(super) fn swap_entry(tmp: &Path, target: &Path) -> Result<(), String> {
    let parent = target
        .parent()
        .ok_or_else(|| format!("{} has no parent", target.display()))?;
    let backup = parent.join(temporary_name());
    std::fs::rename(target, &backup)
        .map_err(|e| format!("cannot move {} aside: {e}", target.display()))?;
    if let Err(e) = std::fs::rename(tmp, target) {
        let _ = std::fs::rename(&backup, target);
        return Err(format!("cannot put {} in place: {e}", target.display()));
    }
    sync_dir(parent)?;
    let _ = remove_path(&backup);
    Ok(())
}

pub(super) fn regular_file(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|metadata| metadata.file_type().is_file())
}

pub(super) fn read_bounded(path: &Path, max: u64) -> Result<Vec<u8>, String> {
    if !regular_file(path) {
        return Err(format!("{} is not a regular file", path.display()));
    }
    let file =
        std::fs::File::open(path).map_err(|e| format!("cannot open {}: {e}", path.display()))?;
    let metadata = file
        .metadata()
        .map_err(|e| format!("cannot stat {}: {e}", path.display()))?;
    if !metadata.is_file() {
        return Err(format!("{} is not a regular file", path.display()));
    }
    if max >= isize::MAX as u64 {
        return Err(format!(
            "{} has an unsupported size limit {max}",
            path.display()
        ));
    }
    if metadata.len() > max {
        return Err(format!(
            "{} is {} bytes, exceeds {max} before reading",
            path.display(),
            metadata.len()
        ));
    }
    let mut bytes = Vec::new();
    file.take(max + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
    if bytes.len() as u64 > max {
        return Err(format!(
            "{} grew beyond {max} bytes while reading",
            path.display()
        ));
    }
    Ok(bytes)
}

pub(super) fn read_card(path: &Path, card: &FileCard) -> Result<Vec<u8>, String> {
    let bytes = read_bounded(path, card.bytes)?;
    if bytes.len() as u64 != card.bytes {
        return Err(format!(
            "{} is {} bytes; the signed card declared {}",
            card.name,
            bytes.len(),
            card.bytes
        ));
    }
    let digest = sha256_hex(&bytes);
    if digest != card.sha256 {
        return Err(format!(
            "{} hashes to {digest}; the signed card declared {}",
            card.name, card.sha256
        ));
    }
    Ok(bytes)
}

/// Write a new file, making its directories, and sync it to disk before
/// returning: a rename that publishes it must never outlive its bytes.
pub(super) fn write_file(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
    }
    let mut file =
        std::fs::File::create(path).map_err(|e| format!("cannot write {}: {e}", path.display()))?;
    file.write_all(bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot write {}: {e}", path.display()))
}

/// Sync a directory's entries — a name added, renamed or removed in it —
/// to disk.
pub(super) fn sync_dir(dir: &Path) -> Result<(), String> {
    std::fs::File::open(dir)
        .and_then(|d| d.sync_all())
        .map_err(|e| format!("cannot sync {}: {e}", dir.display()))
}

/// Write a file whole or not at all: a temporary beside it, then a rename
/// over the old one. `durable` syncs the temporary before the rename and the
/// directory after it, so the rename survives a crash too; without the syncs
/// a power loss can leave the new name holding no bytes (LLP 1026 D11: the
/// record is the rollback floor).
pub(super) fn write_atomic(path: &Path, bytes: &[u8], durable: bool) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| format!("{} has no directory", path.display()))?;
    let tmp = parent.join(temporary_name());
    let written = if durable {
        write_file(&tmp, bytes)
    } else {
        std::fs::write(&tmp, bytes).map_err(|e| format!("cannot write {}: {e}", tmp.display()))
    };
    written.inspect_err(|_| {
        let _ = std::fs::remove_file(&tmp);
    })?;
    std::fs::rename(&tmp, path).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("cannot put {} in place: {e}", path.display())
    })?;
    if durable {
        sync_dir(parent)?;
    }
    Ok(())
}

/// A name nothing else in this directory holds: the process, the clock, and a
/// counter, so two threads of one process cannot collide either.
pub(super) fn temporary_name() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    format!(
        ".tmp-{}-{nanos}-{}",
        std::process::id(),
        COUNTER.fetch_add(1, Ordering::Relaxed)
    )
}

/// Content stored once by SHA-256 at `blobs/<digest>`. An entry's plan and
/// assets are hard links to it where allowed, synced copies otherwise.
/// Finding what the store holds is a path, not a scan.
pub(super) struct Blobs(PathBuf);

impl Blobs {
    pub(super) fn at(store: &Path) -> Blobs {
        Blobs(store.join("blobs"))
    }

    pub(super) fn dir(&self) -> &Path {
        &self.0
    }

    /// Whether the store holds `card`'s bytes whole: read and checked against
    /// the card, so a damaged blob is fetched again, never linked forward.
    pub(super) fn holds(&self, card: &FileCard) -> bool {
        read_card(&self.0.join(&card.sha256), card).is_ok()
    }

    /// Reuse a verified file from the blob store or the binary before fetching it.
    pub(super) fn obtain(
        &self,
        card: &FileCard,
        base: &str,
        fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
        embedded_asset: &mut dyn FnMut(&str) -> Option<EmbeddedAsset>,
    ) -> Result<(), String> {
        let url = resolve_url(base, &card.url)?;
        if self.holds(card) {
            return Ok(());
        }
        // The binary's digest only rules a file out. Its bytes may have changed.
        if let Some(embedded) = embedded_asset(&card.name).filter(|e| e.sha256 == card.sha256) {
            if let Ok(bytes) = read_card(&embedded.path, card) {
                return self.put(&card.sha256, &bytes);
            }
        }
        let bytes = fetch(&url)?;
        if bytes.len() as u64 != card.bytes {
            return Err(format!(
                "{} is {} bytes; the head declared {}",
                card.name,
                bytes.len(),
                card.bytes
            ));
        }
        let digest = sha256_hex(&bytes);
        if digest != card.sha256 {
            return Err(format!(
                "{} hashes to {digest}; the head declared {}",
                card.name, card.sha256
            ));
        }
        self.put(&card.sha256, &bytes)
    }

    pub(super) fn obtain_cohort(
        &self,
        receipt: &CohortReceipt,
        head_url: &str,
        fetch: &mut dyn FnMut(&str) -> Result<Vec<u8>, String>,
    ) -> Result<(), String> {
        self.obtain(&receipt.card, head_url, fetch, &mut |_| None)?;
        if receipt.chunked {
            let base = resolve_url(head_url, &receipt.card.url)?;
            for card in self.cohort_chunks(receipt)? {
                self.obtain(&card, &base, fetch, &mut |_| None)?;
            }
        }
        Ok(())
    }

    fn cohort_chunks(&self, receipt: &CohortReceipt) -> Result<Vec<FileCard>, String> {
        CohortReceipt::chunks(&read_card(
            &self.0.join(&receipt.card.sha256),
            &receipt.card,
        )?)
    }

    pub(super) fn keep_cohort(&self, receipt: &CohortReceipt, keep: &mut BTreeSet<String>) {
        keep.insert(receipt.card.sha256.clone());
        if !receipt.chunked {
            return;
        }
        if let Ok(chunks) = self.cohort_chunks(receipt) {
            keep.extend(chunks.into_iter().map(|card| card.sha256));
        }
    }

    /// Keep `bytes`, already checked against `digest`, whole and durable.
    pub(super) fn put(&self, digest: &str, bytes: &[u8]) -> Result<(), String> {
        std::fs::create_dir_all(&self.0)
            .map_err(|e| format!("cannot make {}: {e}", self.0.display()))?;
        write_atomic(&self.0.join(digest), bytes, true)
    }

    /// Give an entry blob `digest` at `path`: a hard link where allowed, else a synced copy.
    pub(super) fn place(&self, digest: &str, path: &Path) -> Result<(), String> {
        // Android's app_neverallows.te forbids hard links for untrusted apps.
        self.place_with_links(digest, path, !cfg!(target_os = "android"))
    }

    fn place_with_links(&self, digest: &str, path: &Path, link: bool) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .map_err(|e| format!("cannot make {}: {e}", parent.display()))?;
        }
        let blob = self.0.join(digest);
        if link && hard_link(&blob, path).is_ok() {
            return Ok(());
        }
        std::fs::copy(&blob, path)
            .and_then(|_| std::fs::File::open(path)?.sync_all())
            .map_err(|e| format!("cannot place {}: {e}", path.display()))
    }

    /// Remove every blob no digest in `keep` names; temporaries of a write in
    /// flight are left alone. Best effort: a failure waits for the next pass.
    pub(super) fn collect(&self, keep: &BTreeSet<String>) {
        let Ok(read) = std::fs::read_dir(&self.0) else {
            return;
        };
        for item in read.flatten() {
            let name = item.file_name().to_string_lossy().into_owned();
            if !name.starts_with(".tmp-") && !keep.contains(&name) {
                let _ = std::fs::remove_file(item.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    std::thread_local! {
        static LINK_CALLS: std::cell::Cell<(usize, usize, bool)> = const {
            std::cell::Cell::new((0, 0, false))
        };
    }

    pub(super) fn hard_link(source: &Path, target: &Path) -> std::io::Result<()> {
        let (attempts, successes, refuse) = LINK_CALLS.get();
        let result = if refuse {
            Err(std::io::ErrorKind::PermissionDenied.into())
        } else {
            std::fs::hard_link(source, target)
        };
        LINK_CALLS.set((
            attempts + 1,
            successes + usize::from(result.is_ok()),
            refuse,
        ));
        result
    }

    #[cfg(unix)]
    #[test]
    fn placement_links_only_when_allowed() {
        use std::os::unix::fs::MetadataExt as _;

        let store = std::env::temp_dir().join(temporary_name());
        let blobs = Blobs::at(&store);
        let digest = sha256_hex(b"plan");
        blobs.put(&digest, b"plan").unwrap();
        let blob = blobs.dir().join(&digest);
        let platform_links = !cfg!(target_os = "android");
        for (name, link, refuse) in [
            ("copy", false, false),
            ("copy-refused", false, true),
            ("platform", platform_links, false),
            ("platform-refused", platform_links, true),
            ("refused", true, true),
        ] {
            LINK_CALLS.set((0, 0, refuse));
            let placed = store.join("entries").join(name).join("app.plan");
            if matches!(name, "platform" | "platform-refused") {
                blobs.place(&digest, &placed).unwrap();
            } else {
                blobs.place_with_links(&digest, &placed, link).unwrap();
            }
            let (attempts, successes, _) = LINK_CALLS.get();
            assert_eq!(attempts, usize::from(link), "{name}");
            assert_eq!(std::fs::read(&placed).unwrap(), b"plan");
            let source = std::fs::metadata(&blob).unwrap();
            let entry = std::fs::metadata(&placed).unwrap();
            let linked = (source.dev(), source.ino()) == (entry.dev(), entry.ino());
            std::fs::remove_file(&placed).unwrap();
            assert_eq!(linked, successes == 1, "{name}");
            assert_eq!(std::fs::read(&blob).unwrap(), b"plan");
        }
        LINK_CALLS.set((0, 0, false));
        std::fs::remove_dir_all(store).unwrap();
    }

    #[test]
    fn oversized_card_is_refused_before_reading_its_bytes() {
        let path = std::env::temp_dir().join(temporary_name());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(4 * 1024 * 1024).unwrap();
        drop(file);
        let card = FileCard {
            name: "app.plan".into(),
            url: "app.plan".into(),
            bytes: 1,
            sha256: "0".repeat(64),
        };
        let error = read_card(&path, &card).unwrap_err();
        for max in [
            crate::MAX_ENVELOPE_BYTES as u64,
            super::super::record::MAX_RECORD_BYTES,
        ] {
            assert!(read_bounded(&path, max)
                .unwrap_err()
                .contains("before reading"));
        }
        std::fs::remove_file(path).unwrap();
        assert!(error.contains("before reading"), "{error}");
    }
}
