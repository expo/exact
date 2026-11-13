//! Static representations selected before compression or conditional requests.
use super::{stream_file, Request, Response, Shared, MAX_PAGE};
use crate::encode;
use crate::files::{build_version, named_stage, percent_decode, static_file, AUTH_CALLBACK};

fn missing() -> Response {
    Response::text(404, "not found\n").header("Cache-Control", "no-store")
}

pub(super) fn response(request: &Request, shared: &Shared, path: &str) -> Option<Response> {
    let decoded = percent_decode(path, false)?;
    let version = if decoded == "/app.wasm" {
        match build_version(&request.target) {
            Ok(version) => version,
            Err(()) => {
                return Some(
                    Response::text(400, "bad build version\n").header("Cache-Control", "no-store"),
                );
            }
        }
    } else {
        None
    };
    let (file, kind) = match (&version, &shared.generations) {
        (Some(version), Some(generations)) => match generations.file(version) {
            Some(file) => (file.to_path_buf(), "application/wasm"),
            None => return Some(missing()),
        },
        _ => match static_file(&shared.serve.dist, path) {
            Some(found) => found,
            None if version.is_some() => return Some(missing()),
            None => return None,
        },
    };
    if std::fs::metadata(&file).is_ok_and(|meta| meta.len() > MAX_PAGE as u64) {
        let mut response = stream_file(&file, kind, request).unwrap_or_else(missing);
        if let Some((_, tag)) = response.headers.iter().find(|(key, _)| *key == "ETag") {
            let tag = tag.trim_matches('"');
            if version
                .as_ref()
                .is_some_and(|version| !tag.starts_with(version))
            {
                return Some(missing());
            }
            if version.is_some() || named_stage(&decoded, tag) {
                for (key, value) in &mut response.headers {
                    if *key == "Cache-Control" {
                        *value = "public, max-age=31536000, immutable".into();
                    }
                }
            }
        }
        return Some(response);
    }
    let compressible = encode::compressible(kind);
    let Some(served) = shared.variants.serve(&file, request.accepts, compressible) else {
        return Some(missing());
    };
    let tag = served
        .etag
        .trim_matches('"')
        .split('-')
        .next()
        .unwrap_or("");
    if version
        .as_ref()
        .is_some_and(|version| !tag.starts_with(version))
    {
        return Some(missing());
    }
    let named = version.is_some();
    let delta = shared
        .generations
        .as_ref()
        .filter(|_| named && !request.cdn && request.accepts.dcb())
        .zip(request.dictionary.as_deref())
        .and_then(|(generations, hash)| generations.delta(tag, hash));
    let etag = match delta {
        Some(_) => format!("\"{tag}-dcb\""),
        None => served.etag.clone(),
    };
    let auth = decoded.starts_with(AUTH_CALLBACK);
    let cache = if auth {
        "no-store"
    } else if named || named_stage(&decoded, tag) {
        "public, max-age=31536000, immutable"
    } else {
        "no-cache"
    };
    let mut headers = vec![
        ("Content-Type", kind.into()),
        ("Cache-Control", cache.into()),
        ("ETag", etag.clone()),
    ];
    if auth {
        headers.push(("Referrer-Policy", "no-referrer".into()));
    }
    if named && !request.cdn {
        headers.push(("Use-As-Dictionary", "match=\"/app.wasm\"".into()));
    }
    if named && shared.generations.is_some() && !request.cdn {
        // Dictionary negotiation varies even before its delta is ready.
        headers.push(("Vary", "Accept-Encoding, Available-Dictionary".into()));
    } else if compressible {
        headers.push(("Vary", "Accept-Encoding".into()));
    }
    match (delta, served.encoding) {
        (Some(_), _) => headers.push(("Content-Encoding", "dcb".into())),
        (None, Some(encoding)) => headers.push(("Content-Encoding", encoding.name().into())),
        (None, None) => {}
    }
    let fresh = encode::none_match(request.if_none_match.as_deref(), &etag);
    Some(Response {
        status: if fresh { 304 } else { 200 },
        headers,
        body: delta.map_or_else(|| served.body.to_vec(), <[u8]>::to_vec),
        file: None,
        streamed: false,
    })
}
