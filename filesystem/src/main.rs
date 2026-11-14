// Tooling-only filesystem operations; never linked by an app. @ref LLP 1030.002.
use base64::{engine::general_purpose::STANDARD, Engine};
use directory::{identity, lock_exclusive, refuse, Directory, Kind};
use exact_filesystem as directory;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::{self, BufRead, Read, Seek, SeekFrom, Write};

fn field<'a>(input: &'a Value, key: &str) -> io::Result<&'a str> {
    input[key]
        .as_str()
        .ok_or_else(|| refuse(format!("missing {key}")))
}
fn bytes(input: &Value) -> io::Result<Vec<u8>> {
    STANDARD
        .decode(field(input, "bytes")?)
        .map_err(|_| refuse("invalid base64"))
}
fn tree(
    dir: &Directory,
    prefix: &str,
    out: &mut serde_json::Map<String, Value>,
    read: bool,
) -> io::Result<()> {
    dir.visit_files(prefix, &mut |path, parent, leaf| {
        out.insert(
            path.to_owned(),
            if read {
                Value::String(STANDARD.encode(parent.read(leaf)?))
            } else {
                Value::Null
            },
        );
        Ok(())
    })
}

// One reply cannot hold a large tree. A page reads bodies until `limit` bytes
// and gives null for each remaining file, which the next request names in `paths`.
fn page(root: &Directory, input: &Value) -> io::Result<Value> {
    let mut entries = serde_json::Map::new();
    let mut left = input["limit"].as_u64().unwrap_or(u64::MAX);
    let mut read = |path: &str, parent: &Directory, leaf: &str| {
        let value = if left == 0 {
            Value::Null
        } else {
            let body = parent.read(leaf)?;
            left = left.saturating_sub(body.len() as u64);
            Value::String(STANDARD.encode(body))
        };
        entries.insert(path.to_owned(), value);
        Ok(())
    };
    match input["paths"].as_array() {
        Some(paths) => {
            for path in paths {
                let path = path.as_str().ok_or_else(|| refuse("invalid path"))?;
                let (parent, leaf) = root.parent(path, false)?;
                read(path, &parent, &leaf)?;
            }
        }
        None => root.visit_files("", &mut read)?,
    }
    Ok(Value::Object(entries))
}

// Retained dev generations are ordinary immutable files. A blob is a payload
// under `blobs/`, named by its SHA-256 and shared by every generation. Quota admission and
// publication share one OS lock; exhaustion never evicts an existing reader.
fn retained_bytes(dir: &Directory) -> io::Result<u64> {
    let mut bytes = 0u64;
    for leaf in dir.names()? {
        let info = dir.kind(&leaf)?;
        let size = match info {
            Kind {
                directory: true, ..
            } => retained_bytes(&dir.child(&leaf, false)?)?,
            Kind {
                regular: true, len, ..
            } => len,
            _ => {
                return Err(refuse(
                    "retained files must be regular files or directories",
                ))
            }
        };
        bytes = bytes
            .checked_add(size)
            .ok_or_else(|| refuse("retained size overflow"))?;
    }
    Ok(bytes)
}

fn retain(root: &Directory, input: &Value) -> io::Result<Value> {
    let token = field(input, "token")?;
    if token.len() != 48 || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(refuse("invalid temporary-file token"));
    }
    let mut lock = Lock::acquire(root, ".retained/.lock", token)?;
    let quota = input["quota"]
        .as_u64()
        .ok_or_else(|| refuse("missing quota"))?;
    let prefix = field(input, "path")?;
    let (epoch, seq) = prefix
        .split_once('/')
        .ok_or_else(|| refuse("invalid dev generation path"))?;
    if epoch.len() != 32
        || !epoch.bytes().all(|byte| byte.is_ascii_hexdigit())
        || seq.is_empty()
        || !seq.bytes().all(|byte| byte.is_ascii_digit())
    {
        return Err(refuse("invalid dev generation path"));
    }
    let none = serde_json::Map::new();
    let blobs = input["blobs"].as_object().unwrap_or(&none);
    // A request with blobs alone carries part of a generation; its envelope follows.
    let files = match input["files"].as_object() {
        Some(files) => files,
        None if !blobs.is_empty() => &none,
        None => return Err(refuse("missing files")),
    };
    if !files.is_empty() && !files.contains_key("exact.json") {
        return Err(refuse("retained generation needs an envelope"));
    }
    // Only the immediately preceding successful holder can reuse its count.
    // Acquisition has already invalidated that record before any possible write.
    let completed = input["previousToken"].as_str().and_then(|previous| {
        let record = std::str::from_utf8(&lock.previous).ok()?;
        (previous.len() == 48
            && record.len() == 68
            && record.is_ascii()
            && &record[..48] == previous
            && record[48..].bytes().all(|byte| byte.is_ascii_digit()))
        .then(|| {
            record[48..]
                .parse::<u64>()
                .ok()
                .filter(|total| *total >= 68)
        })
        .flatten()
    });
    let mut total = match completed {
        Some(total) => total,
        // The live lock currently occupies 48 bytes. Its completed count adds 20.
        None => retained_bytes(root)?
            .checked_add(20)
            .ok_or_else(|| refuse("retained size overflow"))?,
    };
    let mut candidate = Vec::new();
    let own = files.iter().map(|(name, body)| (false, name, body));
    for (blob, name, encoded) in own.chain(blobs.iter().map(|(name, body)| (true, name, body))) {
        let path = format!("{}/{name}", if blob { "blobs" } else { prefix });
        let body = STANDARD
            .decode(encoded.as_str().ok_or_else(|| refuse("invalid bytes"))?)
            .map_err(|_| refuse("invalid base64"))?;
        if blob && format!("{:x}", Sha256::digest(&body)) != *name {
            return Err(refuse("a blob is not named by its digest"));
        }
        let previous = root
            .parent(&path, false)
            .and_then(|(parent, leaf)| parent.read(&leaf));
        match previous {
            Ok(old) if old == body => continue,
            Ok(_) => return Err(refuse("immutable retained file changed")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
        total = total
            .checked_add(body.len() as u64)
            .ok_or_else(|| refuse("retained size overflow"))?;
        candidate.push((!blob && name == "exact.json", path, body));
    }
    // `reserve` is the bytes the publication's later requests carry. Admitting the
    // whole publication here means a refusal comes before its first blob is written.
    let reserve = input["reserve"].as_u64().unwrap_or(0);
    if total.checked_add(reserve).is_none_or(|all| all > quota) {
        return Err(refuse("retained generation cache quota exceeded"));
    }
    // A blob can be released between the request that kept it and its envelope.
    let index = format!("{prefix}/assets.json");
    if let Some((_, _, index)) = candidate.iter().find(|(_, path, _)| *path == index) {
        for digest in indexed_blobs(index)? {
            let kept = blobs.contains_key(&digest)
                || root
                    .parent(&format!("blobs/{digest}"), false)
                    .and_then(|(dir, leaf)| dir.kind(&leaf))
                    .is_ok();
            if !kept {
                return Err(refuse("a retained generation names a missing blob"));
            }
        }
    }
    // This is a development cache, not a durable publication stream. Complete
    // immutable payloads precede the envelope; disk flushes are not on the edit
    // path. A machine crash may lose cached generations, which readers verify.
    candidate.sort_by_key(|(envelope, _, _)| *envelope);
    for (_, path, body) in candidate {
        let (parent, leaf) = root.parent(&path, true)?;
        parent.write_cached(&leaf, &body, token, || lock.verify(root))?;
    }
    lock.complete_retention(root, total)?;
    Ok(json!({"bytes":total, "token":token}))
}

/// Whether `leaf` in `dir`, reached by the four `parts`, is a stream's head or
/// lock under another spelling. A lookup can match more than ASCII case: APFS
/// folds Unicode case (`ſ` is `s`), and NTFS also matches 8.3 short names. So
/// the files are compared, not the names.
fn names_stream_file(root: &Directory, dir: &Directory, leaf: &str, parts: &[&str]) -> bool {
    let same = |a: io::Result<Kind>, b: io::Result<Kind>| matches!((a, b), (Ok(a), Ok(b)) if a.identity == b.identity);
    parts.len() == 4
        && same(root.kind(parts[0]), root.kind(".exact"))
        && ["exact.json", ".lock"]
            .iter()
            .any(|guarded| same(dir.kind(leaf), dir.kind(guarded)))
}

fn digest_name(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn indexed_blobs(index: &[u8]) -> io::Result<Vec<String>> {
    let unreadable = || refuse("unreadable retained asset index");
    let index: Value = serde_json::from_slice(index).map_err(|_| unreadable())?;
    let assets = index["assets"].as_array().ok_or_else(unreadable)?;
    assets
        .iter()
        .map(|asset| {
            asset["sha256"]
                .as_str()
                .filter(|digest| digest_name(digest))
                .map(str::to_owned)
                .ok_or_else(unreadable)
        })
        .collect()
}

// A publication refused after some of its blobs were kept releases them: each
// named blob that no published generation's index names is removed, so an
// abandoned publication is not charged against the quota.
fn release(root: &Directory, input: &Value) -> io::Result<Value> {
    let _lock = Lock::acquire(root, ".retained/.lock", field(input, "token")?)?;
    let mut named = std::collections::HashSet::new();
    for epoch in root.names()? {
        if epoch.len() != 32 || !epoch.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            continue;
        }
        let epoch = root.child(&epoch, false)?;
        for seq in epoch.names()? {
            let generation = epoch.child(&seq, false)?;
            if generation.kind("exact.json").is_err() {
                continue;
            }
            match generation.read("assets.json") {
                Ok(index) => named.extend(indexed_blobs(&index)?),
                Err(error) if error.kind() == io::ErrorKind::NotFound => {}
                Err(error) => return Err(error),
            }
        }
    }
    let candidates = input["blobs"]
        .as_array()
        .ok_or_else(|| refuse("missing blobs"))?;
    let mut released = 0;
    for digest in candidates {
        let digest = digest
            .as_str()
            .filter(|digest| digest_name(digest))
            .ok_or_else(|| refuse("invalid blob name"))?;
        if named.contains(digest) {
            continue;
        }
        let (dir, leaf) = root.parent(&format!("blobs/{digest}"), false)?;
        match dir.unlink(&leaf) {
            Ok(()) => released += 1,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(error) => return Err(error),
        }
    }
    Ok(json!(released))
}

fn operate(root: &Directory, input: &Value, locked: Option<&Lock>) -> io::Result<Value> {
    if let Some(lock) = locked {
        lock.verify(root)?;
    }
    let op = field(input, "op")?;
    #[cfg(windows)]
    if op == "head" {
        return Err(refuse(
            "durable stream-head publication is not qualified on Windows",
        ));
    }
    if op == "retain" {
        return retain(root, input);
    }
    if op == "release" {
        return release(root, input);
    }
    if op == "inventory" {
        let mut entries = serde_json::Map::new();
        root.visit_files("", &mut |path, parent, leaf| {
            let mut file = parent.file(leaf, libc::O_RDONLY)?;
            let mut hash = Sha256::new();
            let mut bytes = 0u64;
            let mut buffer = [0u8; 64 * 1024];
            loop {
                let count = file.read(&mut buffer)?;
                if count == 0 {
                    break;
                }
                hash.update(&buffer[..count]);
                bytes += count as u64;
            }
            entries.insert(
                path.to_owned(),
                json!({"sha256": format!("{:x}", hash.finalize()), "bytes": bytes}),
            );
            Ok(())
        })?;
        return Ok(Value::Object(entries));
    }
    if op == "tree" && (input["limit"].is_u64() || input["paths"].is_array()) {
        return page(root, input);
    }
    if op == "tree" || op == "names" || op == "copy" {
        let mut entries = serde_json::Map::new();
        tree(root, "", &mut entries, op != "names")?;
        if op == "copy" {
            let target = Directory::root(field(input, "target")?, true)?;
            for (path, data) in &entries {
                let (parent, leaf) = target.parent(path, true)?;
                let bytes = STANDARD
                    .decode(data.as_str().unwrap())
                    .map_err(|_| refuse("invalid bytes"))?;
                parent.write(&leaf, &bytes, false, field(input, "token")?)?;
            }
            return Ok(Value::Bool(true));
        }
        return Ok(Value::Object(entries));
    }
    let path = field(input, "path")?;
    let parts: Vec<_> = path.split('/').collect();
    if op == "put"
        && parts.len() == 4
        && parts[0].eq_ignore_ascii_case(".exact")
        && (parts[3].eq_ignore_ascii_case("exact.json") || parts[3].eq_ignore_ascii_case(".lock"))
    {
        return Err(refuse(
            "stream heads require putHead; stream locks are never replaced",
        ));
    }
    let (dir, leaf) = root.parent(path, op == "put" || op == "head")?;
    match op {
        "get" => Ok(Value::String(STANDARD.encode(dir.read(&leaf)?))),
        "list" => Ok(json!(dir.child(&leaf, false)?.names()?)),
        "put" | "head" => {
            let commit = || {
                if let Some(lock) = locked {
                    lock.verify(root)?;
                }
                if op == "put" && names_stream_file(root, &dir, &leaf, &parts) {
                    return Err(refuse(
                        "stream heads require putHead; stream locks are never replaced",
                    ));
                }
                if op == "head" {
                    let lock =
                        locked.ok_or_else(|| refuse("head requires the held stream lock"))?;
                    if path != lock.head {
                        return Err(refuse("head does not belong to the held stream lock"));
                    }
                    let digest = match dir.read(&leaf) {
                        Ok(bytes) => Value::String(format!("{:x}", Sha256::digest(bytes))),
                        Err(e) if e.kind() == io::ErrorKind::NotFound => Value::Null,
                        Err(e) => return Err(e),
                    };
                    if digest != input["previousDigest"] {
                        return Err(refuse("the head changed underneath; classify again"));
                    }
                }
                Ok(())
            };
            Ok(json!(dir.write_checked(
                &leaf,
                &bytes(input)?,
                input["immutable"] == true,
                field(input, "token")?,
                commit
            )?))
        }

        _ => Err(refuse("unknown filesystem operation")),
    }
}
struct Lock {
    file: File,
    dir: Directory,
    leaf: String,
    token: Vec<u8>,
    previous: Vec<u8>,
    head: String,
    path: String,
}
impl Lock {
    fn acquire(root: &Directory, path: &str, token: &str) -> io::Result<Self> {
        let (dir, leaf) = root.parent(path, true)?;
        let mut file = dir.file(&leaf, libc::O_RDWR | libc::O_CREAT)?;
        // Locks are host-independent OS ownership, not wall-clock/PID leases.
        if lock_exclusive(&file).is_err() {
            return Err(refuse("stream is locked by another publisher"));
        }
        let old = file.metadata()?;
        let named = dir.kind(&leaf)?;
        if identity(&file)? != named.identity {
            return Err(refuse("lock changed while acquiring"));
        }
        let mut previous = Vec::new();
        if path == ".retained/.lock" && old.len() == 68 {
            Read::by_ref(&mut file)
                .take(69)
                .read_to_end(&mut previous)?;
            file.seek(SeekFrom::Start(0))?;
        }
        // Invalidate the previous completion even if this operation later fails.
        file.set_len(0)?;
        file.write_all(token.as_bytes())?;
        // This token identifies a live flock holder, not recovery state. Reads
        // see the write immediately; process exit/reboot releases the OS lock.
        // Persisting the token adds no ownership guarantee.
        Ok(Self {
            file,
            dir,
            leaf,
            token: token.as_bytes().to_vec(),
            previous,
            path: path.to_owned(),
            head: format!(
                "{}/exact.json",
                path.strip_suffix("/.lock")
                    .ok_or_else(|| refuse("invalid stream lock path"))?
            ),
        })
    }
    fn complete_retention(&mut self, root: &Directory, total: u64) -> io::Result<()> {
        self.verify(root)?;
        // The acquisition token is already written. A partial count has the
        // wrong record length and cannot seed another holder's quota check.
        self.file.seek(SeekFrom::Start(48))?;
        self.file.write_all(format!("{total:020}").as_bytes())?;
        Ok(())
    }
    fn verify(&self, root: &Directory) -> io::Result<()> {
        let (current, _) = root.parent(&self.path, false)?;
        if identity(&self.dir.0)? != identity(&current.0)? {
            return Err(refuse("locked stream directory was replaced"));
        }
        let named = self.dir.kind(&self.leaf)?;
        if identity(&self.file)? != named.identity || self.dir.read(&self.leaf)? != self.token {
            return Err(refuse(
                "stream lock was replaced; refusing publisher operation",
            ));
        }
        Ok(())
    }
    // Drop closes the owned fd. The permanent lock file is never unlinked,
    // so an old owner cannot remove a successor's lock on release.
}
fn response(result: io::Result<Value>, missing: bool) -> Value {
    match result {
        Ok(value) => json!({"value": value}),
        Err(error) if missing && error.kind() == io::ErrorKind::NotFound => json!({"value":null}),
        Err(error) => json!({"error":error.to_string(), "errno":error.raw_os_error()}),
    }
}
fn emit(value: Value) -> io::Result<()> {
    println!("{value}");
    io::stdout().flush()
}
// A serving process keeps this pipe open, but every request reopens its root.
// No directory or file bytes survive a request, so atomic root swaps stay visible.
fn read_request(input: &Value) -> io::Result<Value> {
    if input["op"] != "get" {
        return Err(refuse("read session only accepts get"));
    }
    let root = Directory::root(field(input, "root")?, false)?;
    operate(&root, input, None)
}
fn main() -> io::Result<()> {
    if std::env::args().nth(1).as_deref() == Some("--serve-reads") {
        for line in io::stdin().lock().lines() {
            let input: Value = serde_json::from_str(&line?)?;
            emit(response(read_request(&input), true))?;
        }
        return Ok(());
    }
    let mut lines = io::stdin().lock().lines();
    let first: Value =
        serde_json::from_str(&lines.next().ok_or_else(|| refuse("missing request"))??)?;
    let session = first["op"] == "lock";
    let write = session || first["op"] == "put" || first["op"] == "retain";
    let root = match Directory::root(field(&first, "root")?, write) {
        Ok(root) => root,
        Err(error) => {
            emit(response(
                Err(error),
                first["optionalRoot"] == true
                    || matches!(first["op"].as_str(), Some("get" | "list")),
            ))?;
            return Ok(());
        }
    };
    if !session {
        emit(response(
            operate(&root, &first, None),
            matches!(first["op"].as_str(), Some("get" | "list")),
        ))?;
        return Ok(());
    }
    let lock = match Lock::acquire(&root, field(&first, "path")?, field(&first, "token")?) {
        Ok(lock) => lock,
        Err(error) => {
            emit(response(Err(error), false))?;
            return Ok(());
        }
    };
    emit(json!({"value":true}))?;
    for line in lines {
        let input: Value = serde_json::from_str(&line?)?;
        emit(response(
            operate(&root, &input, Some(&lock)),
            matches!(input["op"].as_str(), Some("get" | "list")),
        ))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests;
#[cfg(all(test, windows))]
mod windows_tests;
