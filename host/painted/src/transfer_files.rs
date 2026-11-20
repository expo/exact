//! App-file custody for the import-selected transfer carrier.
use super::{grants, private_file, string};
use ibex2::{
    boundary::{admit, HostError},
    grant::{Grant, GrantSet, Operation, PathPrefix},
    stdlib::{
        app_fs::AppDirectories,
        fs::{self, FsOp},
    },
};
use serde_json::{json, Value};
use std::io::Write;

fn error(error: HostError) -> &'static str {
    match error {
        HostError::Denied { .. } | HostError::NotAllowed { .. } => "NotAllowedError",
        _ => "TypeError",
    }
}

pub(super) fn dispatch(v: &Value, ceiling: &str) -> Result<Value, &'static str> {
    let roots = AppDirectories::new(
        string(v, "data")?,
        string(v, "cache")?,
        string(v, "temporary")?,
    )
    .map_err(error)?;
    let op = string(v, "op")?;
    let source = grants(string(v, "authority")?)?;
    let current = grants(ceiling)?;
    if op == "snapshot" {
        let mut output = private_file(string(v, "file")?, true)?;
        if output.metadata().map_err(|_| "UnknownError")?.len() != 0 {
            return Err("TypeError");
        }
        let open = |path: &str| {
            if !path.starts_with("app:/") {
                return Err("NotAllowedError");
            }
            admit(&current, &Operation::FsRead { path: path.into() }).map_err(error)?;
            roots.open_file(&source, path).map_err(error)
        };
        if let Some(path) = v["path"].as_str() {
            std::io::copy(&mut open(path)?, &mut output).map_err(|_| "UnknownError")?;
        } else {
            let (boundary, form) = super::carrier(v, ceiling)?
                .form(string(v, "key")?)
                .map_err(super::refusal)?;
            write_form(&boundary, &parts(&form)?, &mut output, open)?;
        }
        output.sync_all().map_err(|_| "UnknownError")?;
        return Ok(json!({"size": output.metadata().map_err(|_| "UnknownError")?.len()}));
    }
    let path = string(v, "path")?;
    if !path.starts_with("app:/") {
        return Err("NotAllowedError");
    }
    // Only a registration's kept responses get implicit write custody: a file
    // to place, the whole directory to remove.
    let kept = v["kept"].as_bool() == Some(true);
    if kept && !kept_path(path, op == "removePlaced") {
        return Err("NotAllowedError");
    }
    let access = Operation::FsWrite { path: path.into() };
    if !kept {
        admit(&source, &access).map_err(error)?;
        admit(&current, &access).map_err(error)?;
    }
    let granted = if kept { write_grant(path)? } else { source };
    if op == "removePlaced" && !kept {
        return Err("NotAllowedError");
    }
    // Authorization is for the destination file. Parent creation uses the same
    // pinned roots and never traverses symlinks, including on a later retry.
    let parent = path.rsplit_once('/').ok_or("TypeError")?.0;
    let parents = write_grant(parent)?;
    fs::run(&parents, Some(&roots), FsOp::Mkdir, parent, None, None).map_err(error)?;
    if op == "removePlaced" {
        // A placement that died before it made the directory left none to remove.
        fs::run(&granted, Some(&roots), FsOp::Remove, path, None, None).map_err(error)?;
        return Ok(json!({}));
    }
    if op == "checkSave" {
        roots.stage(&granted, path).map_err(error)?;
        return Ok(json!({}));
    }
    let present = roots
        .publish_file(&granted, path, std::path::Path::new(string(v, "file")?))
        .map_err(error)?;
    Ok(json!({"bodyLost": !present}))
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) struct Part<'a> {
    name: &'a str,
    value: Option<&'a str>,
    from: Option<&'a str>,
    filename: Option<&'a str>,
    content_type: Option<&'a str>,
}

pub(super) fn parts(form: &Value) -> Result<Vec<Part<'_>>, &'static str> {
    use serde::Deserialize;
    let list = form
        .as_array()
        .filter(|parts| parts.len() <= 256)
        .ok_or("TypeError")?;
    list.iter()
        .map(|entry| {
            let part = Part::deserialize(entry).map_err(|_| "TypeError")?;
            if part.from.is_some() == part.value.is_some()
                || (part.from.is_none() && (part.filename.is_some() || part.content_type.is_some()))
                || part
                    .content_type
                    .is_some_and(|s| !s.bytes().all(|b| (0x20..=0x7e).contains(&b)))
            {
                return Err("TypeError");
            }
            Ok(part)
        })
        .collect()
}

fn write_form(
    boundary: &str,
    parts: &[Part<'_>],
    output: &mut impl Write,
    open: impl Fn(&str) -> Result<std::fs::File, &'static str>,
) -> Result<(), &'static str> {
    use ibex2::stdlib::multipart::{normalized_newlines, part_header};
    let mut header = Vec::new();
    for part in parts {
        let file = part.from.map(|path| {
            (
                part.filename
                    .unwrap_or_else(|| path.rsplit('/').next().unwrap_or(path)),
                part.content_type.unwrap_or_default(),
            )
        });
        header.clear();
        part_header(&mut header, boundary, part.name, file).map_err(|_| "TypeError")?;
        output.write_all(&header).map_err(|_| "UnknownError")?;
        if let Some(path) = part.from {
            std::io::copy(&mut open(path)?, output).map_err(|_| "UnknownError")?;
        } else {
            output
                .write_all(&normalized_newlines(part.value.unwrap_or_default()))
                .map_err(|_| "UnknownError")?;
        }
        output.write_all(b"\r\n").map_err(|_| "UnknownError")?;
    }
    write!(output, "--{boundary}--\r\n").map_err(|_| "UnknownError")
}

fn write_grant(path: &str) -> Result<GrantSet, &'static str> {
    Ok(GrantSet::none().with(Grant::FsWrite(PathPrefix::new(path).ok_or("TypeError")?)))
}

const KEPT: &str = "app:/data/transfers";

/// Whether `path` is where a registration's responses with no `saveTo` are
/// kept (LLP 1133.017 D3): `app:/data/transfers/<id>`, or a file `<index>` in
/// it. The package writes the id as one segment, percent-encoded.
fn kept_path(path: &str, directory: bool) -> bool {
    let Some(rest) = path.strip_prefix(KEPT).and_then(|p| p.strip_prefix('/')) else {
        return false;
    };
    let (id, index) = match rest.split_once('/') {
        Some((id, index)) => (id, Some(index)),
        None => (rest, None),
    };
    let segment = !id.is_empty()
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'%'));
    segment
        && match index {
            None => directory,
            Some(index) => {
                !directory
                    && matches!(index.len(), 1..=3)
                    && index.bytes().all(|b| b.is_ascii_digit())
            }
        }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};
    struct Files(PathBuf);
    impl Files {
        fn new() -> Self {
            static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
            let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/test-transfer-files")
                .join(format!(
                    "{}-{}",
                    std::process::id(),
                    NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
                ));
            for name in ["data", "cache", "tmp", "private"] {
                fs::create_dir_all(dir.join(name)).unwrap();
            }
            Self(dir)
        }
        fn request(&self, op: &str, path: &str) -> Value {
            json!({"op":op, "path":path, "authority":"fs.read app:/data\nfs.write app:/data", "origin":"app",
                "data":self.0.join("data"), "cache":self.0.join("cache"), "temporary":self.0.join("tmp"),
                "file":self.0.join("private/body")})
        }
    }
    impl Drop for Files {
        fn drop(&mut self) {
            fs::remove_dir_all(&self.0).unwrap();
        }
    }
    const GRANTS: &str = "fs.read app:/data\nfs.write app:/data";

    #[test]
    fn ordered_parts_match_form_data_with_binary_bytes_and_escaped_names() {
        use ibex2::stdlib::multipart::{EncodedMultipart, FormData};
        let files = Files::new();
        let binary = [0, 255, 13, 10];
        let source = files.0.join("data/clip.bin");
        fs::write(&source, binary).unwrap();
        let form = json!([{"name":"tag","value":"one\ntwo"}, {"name":"q\"\n雪","from":"app:/data/clip.bin"}, {"name":"tag","value":""}]);
        let mut wire = Vec::new();
        write_form(
            "fixed-boundary",
            &parts(&form).unwrap(),
            &mut wire,
            |path| {
                assert_eq!(path, "app:/data/clip.bin");
                Ok(fs::File::open(&source).unwrap())
            },
        )
        .unwrap();
        let mut reference = FormData::new();
        reference.append_text("tag", "one\ntwo");
        reference.append_file("q\"\n雪", binary.to_vec(), "clip.bin", "");
        reference.append_text("tag", "");
        assert_eq!(
            wire,
            EncodedMultipart::with_boundary(&reference, "fixed-boundary")
                .unwrap()
                .as_bytes()
        );
        for invalid in [
            json!({"name":"x"}),
            json!({"name":"x","from":"a","value":""}),
            json!({"name":"x","value":"","filename":""}),
            json!({"name":"x","from":"a","contentType":"text/plain\n"}),
        ] {
            assert!(parts(&json!([invalid])).is_err());
        }
    }

    #[test]
    fn save_replaces_a_file_but_preserves_directories_and_symlink_targets() {
        let files = Files::new();
        fs::write(files.0.join("private/body"), b"new").unwrap();
        let r = files.request("place", "app:/data/nested/saved");
        dispatch(&r, GRANTS).unwrap();
        assert_eq!(fs::read(files.0.join("data/nested/saved")).unwrap(), b"new");
        assert!(
            !files.0.join("private/body").exists(),
            "publication moves the response"
        );
        // Recovery after rename preserves a destination the app may already have changed.
        fs::write(files.0.join("data/nested/saved"), b"app changed it").unwrap();
        dispatch(&r, GRANTS).unwrap();
        assert_eq!(
            fs::read(files.0.join("data/nested/saved")).unwrap(),
            b"app changed it"
        );
        fs::write(files.0.join("private/body"), b"new").unwrap();
        fs::write(files.0.join("data/nested/saved"), b"old").unwrap();
        dispatch(&r, GRANTS).unwrap();
        assert_eq!(fs::read(files.0.join("data/nested/saved")).unwrap(), b"new");
        fs::remove_file(files.0.join("data/nested/saved")).unwrap();
        fs::create_dir(files.0.join("data/nested/saved")).unwrap();
        fs::write(files.0.join("data/nested/saved/child"), b"kept").unwrap();
        assert!(dispatch(&r, GRANTS).is_err());
        assert_eq!(
            fs::read(files.0.join("data/nested/saved/child")).unwrap(),
            b"kept"
        );
        fs::rename(files.0.join("data/nested"), files.0.join("private/outside")).unwrap();
        std::os::unix::fs::symlink(files.0.join("private/outside"), files.0.join("data/nested"))
            .unwrap();
        assert!(dispatch(&r, GRANTS).is_err());
        assert_eq!(
            fs::read(files.0.join("private/outside/saved/child")).unwrap(),
            b"kept"
        );
    }

    #[test]
    fn snapshot_and_save_require_both_original_and_current_grants() {
        let files = Files::new();
        fs::write(files.0.join("data/upload"), b"snapshot").unwrap();
        fs::write(files.0.join("private/body"), b"").unwrap();
        let mut r = files.request("snapshot", "app:/data/upload");
        assert!(dispatch(&r, "fs.write app:/data").is_err());
        r["authority"] = json!("fs.write app:/data");
        assert!(dispatch(&r, GRANTS).is_err());
        r["authority"] = json!(GRANTS);
        dispatch(&r, GRANTS).unwrap();
        fs::write(files.0.join("data/upload"), b"changed").unwrap();
        assert_eq!(fs::read(files.0.join("private/body")).unwrap(), b"snapshot");
        r["op"] = json!("place");
        r["path"] = json!("app:/data/saved");
        r["authority"] = json!("fs.read app:/data");
        assert!(dispatch(&r, GRANTS).is_err());
        r["authority"] = json!(GRANTS);
        assert!(dispatch(&r, "fs.read app:/data").is_err());
        assert!(!files.0.join("data/saved").exists());
    }

    #[test]
    fn only_a_registrations_kept_responses_get_implicit_custody() {
        let files = Files::new();
        let mut r = files.request("removePlaced", "app:/data/transfers/visitor");
        r["authority"] = json!("");
        r["kept"] = json!(true);
        // A placement that died before it made the directory left none to remove.
        dispatch(&r, "").unwrap();
        assert!(!files.0.join("data/transfers/visitor").exists());
        r["op"] = json!("place");
        for path in [
            "app:/data/elsewhere",
            "app:/data/transfers/visitor",
            "app:/data/transfers/visitor/",
            "app:/data/transfers/visitor/x",
            "app:/data/transfers/visitor/0/1",
            "app:/data/transfers/a.b/0",
            "app:/data/transfers/../0",
            "app:/data/transfers//0",
        ] {
            fs::write(files.0.join("private/body"), b"response").unwrap();
            r["path"] = json!(path);
            assert!(dispatch(&r, "").is_err(), "{path}");
        }
        r["path"] = json!("app:/data/transfers/a%2Fb/0");
        dispatch(&r, "").unwrap();
        assert_eq!(
            fs::read(files.0.join("data/transfers/a%2Fb/0")).unwrap(),
            b"response"
        );
        // A retirement removes the directory, never one file of it.
        r["op"] = json!("removePlaced");
        assert!(dispatch(&r, "").is_err());
        r["path"] = json!("app:/data/transfers/a%2Fb");
        dispatch(&r, "").unwrap();
        dispatch(&r, "").unwrap();
        assert!(!files.0.join("data/transfers/a%2Fb").exists());
    }
}
