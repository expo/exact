//! Retained builds of `app.wasm`, selected by their content-addressed URLs,
//! and Compression Dictionary Transport between them (LLP 1047.000 §9).
//!
//! `--generations <dir>` keeps the newest four builds. Each retained build
//! is available at its own `?v=<digest>` for pages still open across deploys
//! (LLP 1048.000 D10). Deltas are made off the request path; until one is
//! ready, its selected build goes as it otherwise would.
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};
use std::time::SystemTime;

/// Builds kept, the one served included.
pub(crate) const KEEP: usize = 4;

type Delta = Arc<OnceLock<Option<Vec<u8>>>>;

pub(crate) struct Generations {
    /// Verified full SHA-256 names and their files.
    builds: HashMap<String, PathBuf>,
    /// Target ETag, then dictionary SHA-256 in base64.
    deltas: HashMap<String, HashMap<String, Delta>>,
}

impl Generations {
    /// Keep `wasm` beside the newest earlier builds, drop the rest, and
    /// start making deltas for each retained representation.
    pub(crate) fn open(dir: &Path, wasm: &[u8]) -> std::io::Result<Generations> {
        use sha2::{Digest, Sha256};
        std::fs::create_dir_all(dir)?;
        let dir = dir.canonicalize()?;
        let current = dir.join(format!("{:x}.wasm", Sha256::digest(wasm)));
        // Written again, so it is the newest.
        std::fs::write(&current, wasm)?;
        let mut kept: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(&dir)?
            .filter_map(|entry| {
                let entry = entry.ok()?;
                if !entry.file_type().ok()?.is_file() {
                    return None;
                }
                let path = entry.path();
                let hex = path.file_name()?.to_str()?.strip_suffix(".wasm")?;
                if hex.len() != 64 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
                    return None;
                }
                Some((entry.metadata().ok()?.modified().ok()?, path))
            })
            .filter(|(_, path)| *path != current)
            .collect();
        kept.sort_by_key(|k| std::cmp::Reverse(k.0));
        let mut builds = HashMap::new();
        let mut bytes = vec![Arc::new(wasm.to_vec())];
        builds.insert(format!("{:x}", Sha256::digest(wasm)), current);
        for (i, (_, path)) in kept.into_iter().enumerate() {
            if i + 1 >= KEEP {
                let _ = std::fs::remove_file(&path);
                continue;
            }
            let Ok(earlier) = std::fs::read(&path) else {
                continue;
            };
            let digest = format!("{:x}", Sha256::digest(&earlier));
            if path.file_stem().and_then(|name| name.to_str()) != Some(&digest) {
                continue;
            }
            builds.insert(digest, path);
            bytes.push(Arc::new(earlier));
        }
        let mut deltas = HashMap::new();
        for build in &bytes {
            let mut pairs = HashMap::new();
            let mut work = Vec::new();
            for earlier in &bytes {
                if Arc::ptr_eq(build, earlier) {
                    continue;
                }
                let hash: [u8; 32] = Sha256::digest(earlier.as_slice()).into();
                let made = Arc::new(OnceLock::new());
                work.push((Arc::clone(earlier), hash, Arc::clone(&made)));
                pairs.insert(exact_data::envelope::base64(&hash), made);
            }
            deltas.insert(crate::encode::tag(build), pairs);
            let build = Arc::clone(build);
            std::thread::Builder::new()
                .name("exact-render-generation".into())
                .spawn(move || {
                    for (earlier, hash, slot) in work {
                        let _ = slot.set(crate::encode::against_at(&build, &earlier, &hash, 11));
                    }
                })?;
        }
        Ok(Generations { builds, deltas })
    }

    /// A version must identify exactly one verified retained build.
    pub(crate) fn file(&self, version: &str) -> Option<&Path> {
        let mut matches = self
            .builds
            .iter()
            .filter(|(hash, _)| hash.starts_with(version));
        let (_, path) = matches.next()?;
        matches.next().is_none().then_some(path.as_path())
    }

    pub(crate) fn files(&self) -> Vec<PathBuf> {
        self.builds.values().cloned().collect()
    }

    /// The selected build against `hash`, once its delta is made.
    pub(crate) fn delta(&self, tag: &str, hash: &str) -> Option<&[u8]> {
        self.deltas.get(tag)?.get(hash)?.get()?.as_deref()
    }

    /// Whether every delta is made (a test's wait).
    #[cfg(test)]
    pub(crate) fn made(&self) -> bool {
        self.deltas
            .values()
            .flat_map(HashMap::values)
            .all(|made| made.get().is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use std::time::Duration;

    #[test]
    fn the_newest_builds_are_kept_and_the_rest_dropped() {
        let dir = std::env::temp_dir().join(format!("exact-generations-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let build = |n: u8| vec![n; 4096];
        let file = |n: u8| dir.join(format!("{:x}.wasm", Sha256::digest(build(n))));
        // Five earlier builds, the first oldest, and a file that isn't one.
        let now = SystemTime::now();
        for n in 1..=5u8 {
            std::fs::write(file(n), build(n)).unwrap();
            let at = now - Duration::from_secs(60 * u64::from(6 - n));
            std::fs::File::options()
                .write(true)
                .open(file(n))
                .unwrap()
                .set_modified(at)
                .unwrap();
        }
        std::fs::write(dir.join("notes.wasm"), b"not a build").unwrap();
        let generations = Generations::open(&dir, &build(6)).unwrap();
        let kept: Vec<bool> = (1..=6).map(|n| file(n).is_file()).collect();
        assert_eq!(kept, [false, false, true, true, true, true]);
        assert!(dir.join("notes.wasm").is_file());
        let started = std::time::Instant::now();
        while !generations.made() {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "deltas did not finish"
            );
            std::thread::sleep(Duration::from_millis(50));
        }
        let name = |n: u8| exact_data::envelope::base64(&Sha256::digest(build(n)));
        assert!(generations
            .delta(&crate::encode::tag(&build(6)), &name(5))
            .is_some());
        assert!(generations
            .delta(&crate::encode::tag(&build(6)), &name(3))
            .is_some());
        assert!(generations
            .delta(&crate::encode::tag(&build(6)), &name(2))
            .is_none());
        assert_eq!(
            generations.file(&format!("{:x}", Sha256::digest(build(6)))[..16]),
            Some(file(6).canonicalize().unwrap().as_path())
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_version_must_name_exactly_one_retained_digest() {
        // Model a prefix collision without searching for colliding wasm bytes.
        let mut generations = Generations {
            builds: HashMap::from([
                (
                    format!("0123456789abcdef{}", "0".repeat(48)),
                    "first.wasm".into(),
                ),
                (
                    format!("0123456789abcdef{}", "1".repeat(48)),
                    "second.wasm".into(),
                ),
            ]),
            deltas: HashMap::new(),
        };
        assert!(generations.file("0123456789abcdef").is_none());
        generations
            .builds
            .remove(&format!("0123456789abcdef{}", "1".repeat(48)));
        assert_eq!(
            generations.file("0123456789abcdef"),
            Some(Path::new("first.wasm"))
        );
        assert!(generations.file("fedcba9876543210").is_none());
    }

    #[test]
    fn a_retained_filename_must_match_its_bytes() {
        let dir =
            std::env::temp_dir().join(format!("exact-generations-invalid-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let digest = format!("{:x}", Sha256::digest(b"original build"));
        std::fs::write(dir.join(format!("{digest}.wasm")), b"replaced bytes").unwrap();
        let generations = Generations::open(&dir, b"current").unwrap();
        assert!(generations.file(&digest[..16]).is_none());
        assert!(generations
            .delta(
                &crate::encode::tag(b"current"),
                &exact_data::envelope::base64(&Sha256::digest(b"replaced bytes"))
            )
            .is_none());
        std::fs::remove_dir_all(dir).unwrap();
    }
}
