//! The path crates a data crate is built from (LLP 1030 D3a): their trees
//! are in the compatibility id beside the crate's own.
use std::path::{Path, PathBuf};

/// The normal and build path dependencies the data crate at `dir` can be
/// built from, less Exact's own crates, whose changes the id's other inputs
/// name. Registry and git dependencies are pinned by the lockfile.
///
/// A declared optional dependency counts whether or not Cargo enabled it
/// here: the id is computed without the features of the build that links
/// the crate, which can enable any of them (`--features`, a host's own
/// dependency on the crate).
///
/// A path crate that `[patch]` puts in place of a registry or git dependency
/// counts only where Cargo resolved it. Beneath an optional dependency Cargo
/// left off it is missed: the declaration names no path, and the patch table
/// is in the manifest of the workspace that builds the binary, which this
/// does not read.
pub(crate) fn path_crates(dir: &Path) -> Result<Vec<(String, PathBuf)>, String> {
    let exact = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .canonicalize()
        .map_err(|e| format!("exact2 checkout: {e}"))?;
    let own = |path: &Path| path.starts_with(&exact) && !path.starts_with(exact.join("apps"));
    let start = dir
        .canonicalize()
        .map_err(|e| format!("{}: {e}", dir.display()))?;
    let mut packages = metadata(dir, true)?;
    let mut found = std::collections::BTreeMap::new();
    let mut queue = vec![start.clone()];
    while let Some(at) = queue.pop() {
        // Cargo lists every crate it enabled, so this one is optional and
        // off, or reached only through such a crate.
        if !packages.contains_key(&at) {
            // No build can enable a crate that has no manifest.
            if !at.join("Cargo.toml").is_file() {
                continue;
            }
            for (path, package) in metadata(&at, false)? {
                packages.entry(path).or_insert(package);
            }
        }
        let (name, deps) = packages.get(&at).ok_or_else(|| {
            format!(
                "cargo metadata does not list the path dependency {}",
                at.display()
            )
        })?;
        if at != start {
            found.insert(at, name.clone());
        }
        for dep in deps {
            if dep != &start && !own(dep) && !found.contains_key(dep) {
                queue.push(dep.clone());
            }
        }
    }
    let mut out: Vec<_> = found.into_iter().map(|(p, n)| (n, p)).collect();
    out.sort();
    Ok(out)
}

type Packages = std::collections::HashMap<PathBuf, (String, Vec<PathBuf>)>;

/// Cargo's path packages for the manifest in `dir`, by canonical directory:
/// each one's name and its normal and build path dependencies. With
/// `resolved` those are the ones Cargo enabled and the optional ones the
/// package declares. Without, they are all it declares: `--no-deps` resolves
/// nothing, so a crate Cargo left out of the build needs no registry and
/// gets no lockfile.
fn metadata(dir: &Path, resolved: bool) -> Result<Packages, String> {
    let cargo = std::env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let mut command = std::process::Command::new(cargo);
    command
        .args([
            "metadata",
            "--format-version",
            "1",
            "--offline",
            "--manifest-path",
        ])
        .arg(dir.join("Cargo.toml"));
    if !resolved {
        command.arg("--no-deps");
    }
    let output = command
        .output()
        .map_err(|e| format!("cargo metadata: {e}"))?;
    if !output.status.success() {
        return Err(format!(
            "cargo metadata for {}: {}",
            dir.display(),
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let meta: serde_json::Value =
        serde_json::from_slice(&output.stdout).map_err(|e| format!("cargo metadata: {e}"))?;
    let canonical = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let mut ids = std::collections::HashMap::new();
    let mut packages = Packages::new();
    let listed = meta["packages"].as_array().into_iter().flatten();
    for package in listed.filter(|p| p["source"].is_null()) {
        let (Some(id), Some(name), Some(manifest)) = (
            package["id"].as_str(),
            package["name"].as_str(),
            package["manifest_path"].as_str(),
        ) else {
            continue;
        };
        let Some(at) = Path::new(manifest).parent().map(canonical) else {
            continue;
        };
        let declared = package["dependencies"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| d["kind"].as_str() != Some("dev") && (!resolved || d["optional"] == true))
            .filter_map(|d| d["path"].as_str())
            .map(|path| canonical(Path::new(path)))
            .collect();
        ids.insert(id, at.clone());
        packages.insert(at, (name.to_string(), declared));
    }
    for node in meta["resolve"]["nodes"].as_array().into_iter().flatten() {
        let at = node["id"].as_str().and_then(|id| ids.get(id));
        let Some((_, deps)) = at.and_then(|at| packages.get_mut(at)) else {
            continue;
        };
        let enabled = node["deps"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|d| {
                d["dep_kinds"]
                    .as_array()
                    .is_some_and(|kinds| kinds.iter().any(|k| k["kind"].as_str() != Some("dev")))
            })
            .filter_map(|d| d["pkg"].as_str().and_then(|id| ids.get(id)).cloned());
        deps.extend(enabled);
    }
    Ok(packages)
}

#[cfg(test)]
mod tests {
    use super::path_crates;
    use std::path::Path;

    /// exact2's own crates stay out of the closure, since the id's other inputs
    /// name them; another app's crates are this app's code.
    #[test]
    fn the_closure_leaves_out_exact2s_own_crates() {
        let apps = Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps");
        let names = |app: &str| -> Vec<String> {
            path_crates(&apps.join(app).join("data"))
                .unwrap()
                .into_iter()
                .map(|(name, _)| name)
                .collect()
        };
        assert_eq!(names("markdown"), ["markdown-parse"]);
        assert_eq!(
            names("exact-live"),
            [
                "completion-storm-data",
                "interaction-gallery-data",
                "markdown-parse",
                "markdown-stress-data",
                "messages-stress-data"
            ]
        );
        assert!(names("caltrain").is_empty());
    }
}
