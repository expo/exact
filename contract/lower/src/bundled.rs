//! A literal file an `image`, `video` or `audio` names is read from the
//! app's `assets/` (or `deck/`), the folders every host ships for them, so
//! `image "public/logo.png"` built green and drew nothing on the phone
//! (expo/exact-build#4). A `host.*.resources` tree is not one: it ships
//! separately, for native code and `fs.read bundle:/` (docs/reference.md). A
//! URL, a symbol, a host path (`/…`) or a computed source is not a file of
//! the app's; a bare word is `lower-image-source`'s warning.

use crate::LowerError;
use contract_syntax::{Expr, File, Node, Span};
use std::path::Path;

/// The folders the build bundles into the app.
const BUNDLED: &[&str] = &["assets", "deck"];

/// Literal bundled paths, and SVG image files checked by the native parser
/// (LLP 1011.001 D5), once per place written.
pub(crate) fn check(view: &[Node], root: &Path) -> (Vec<LowerError>, Vec<LowerError>) {
    let (mut out, mut warnings) = (Vec::new(), Vec::new());
    let native = crate::icons::shipped_targets(root)
        .iter()
        .any(|t| t != "web");
    let mut images = Vec::new();
    sources(view, |what, source, span| {
        if what == "image source" && !images.contains(&(source.clone(), span)) {
            images.push((source.clone(), span));
            image_svg(root, native, &source, span, &mut out, &mut warnings);
        }
        if let Some(e) = refusal(what, &source, span) {
            // A component used twice is inlined twice: one refusal for its source.
            if !out.contains(&e) {
                out.push(e);
            }
        }
    });
    (out, warnings)
}

/// Literal bundled SVG images the build may read, including files not yet
/// present. Watchers use the same path and expression rules as lowering.
pub fn image_svg_paths(file: &File) -> Vec<String> {
    let mut paths = Vec::new();
    let view = contract_syntax::inline(file).unwrap_or_default();
    sources(&view, |what, source, _| {
        if what == "image source" {
            if let Some(path) = svg_path(&source) {
                if !paths.iter().any(|p| p == path) {
                    paths.push(path.to_string());
                }
            }
        }
    });
    paths
}

fn sources(view: &[Node], mut visit: impl FnMut(&str, String, Span)) {
    let mut stack: Vec<&Node> = view.iter().rev().collect();
    while let Some(node) = stack.pop() {
        let children: Vec<&Node> = match node {
            Node::Element {
                tag,
                positional,
                attrs,
                children,
                ..
            } => {
                let mut sources: Vec<(&str, &Expr)> = Vec::new();
                let what = match tag.as_str() {
                    "image" => Some("image source"),
                    "video" => Some("video source"),
                    "audio" => Some("audio source"),
                    _ => None,
                };
                if let Some(what) = what {
                    sources.extend(positional.first().map(|e| (what, e)));
                }
                if matches!(tag.as_str(), "video" | "audio") {
                    for a in attrs {
                        match a.name.as_str() {
                            "src" => sources.push((what.unwrap_or_default(), &a.value)),
                            "poster" => sources.push(("video poster", &a.value)),
                            _ => {}
                        }
                    }
                }
                for (what, e) in sources {
                    let mut found = Vec::new();
                    literals(e, &mut found);
                    for (source, span) in found {
                        visit(what, source, span);
                    }
                }
                children.iter().collect()
            }
            Node::When {
                then, otherwise, ..
            } => then.iter().chain(otherwise).collect(),
            Node::Each { body, .. } => body.iter().collect(),
            Node::Match { some, none, .. } => some.1.iter().chain(none).collect(),
            Node::Use { children, .. } => children.iter().collect(),
            Node::Children { .. } => Vec::new(),
        };
        stack.extend(children.into_iter().rev());
    }
}

fn svg_path(source: &str) -> Option<&str> {
    let mut path = source;
    while let Some(relative) = path.strip_prefix("./") {
        path = relative;
    }
    (exact_plan::is_portable_asset_path(path)
        && path.to_ascii_lowercase().ends_with(".svg")
        && path
            .split_once('/')
            .is_some_and(|(dir, _)| BUNDLED.contains(&dir)))
    .then_some(path)
}

/// A literal SVG under a bundled folder; remote, mutable and computed
/// sources and files unavailable at build are the host's to check at run time.
fn image_svg(
    root: &Path,
    native: bool,
    source: &str,
    span: Span,
    errors: &mut Vec<LowerError>,
    warnings: &mut Vec<LowerError>,
) {
    let Some(path) = svg_path(source) else {
        return;
    };
    let Ok(bytes) = std::fs::read(root.join(path)) else {
        return;
    };
    // Native loaders identify raster formats by bytes, regardless of the suffix.
    if raster(&bytes) {
        return;
    }
    let parsed = String::from_utf8(bytes)
        .map_err(|e| e.to_string())
        .and_then(|text| exact_svg_file::parse(&text).map_err(|e| e.to_string()));
    let diagnostic = |reason: &str, fix: &str| LowerError {
        id: "lower-image-svg",
        message: format!("image source `{source}` on native hosts: {reason}. To fix it, {fix}"),
        span,
    };
    match parsed {
        Ok(document) => {
            for reason in document.warnings {
                warnings.push(diagnostic(
                    &reason,
                    "rewrite the named construct as static SVG shapes or use a raster image; use inline SVG for animation",
                ));
            }
        }
        Err(reason) => {
            let error = diagnostic(
                &format!("the SVG parser refuses this file: {reason}"),
                "correct the SVG or use a raster image",
            );
            if native {
                errors.push(error);
            } else {
                warnings.push(error);
            }
        }
    }
}

fn raster(bytes: &[u8]) -> bool {
    bytes.starts_with(b"\x89PNG\r\n\x1a\n")
        || bytes.starts_with(b"\xff\xd8\xff")
        || bytes.starts_with(b"GIF87a")
        || bytes.starts_with(b"GIF89a")
        || (bytes.starts_with(b"RIFF") && bytes.get(8..12) == Some(b"WEBP"))
        || bytes.starts_with(b"BM")
        || bytes.starts_with(b"II*\0")
        || bytes.starts_with(b"MM\0*")
        || bytes.starts_with(b"\0\0\x01\0")
        || (bytes.get(4..8) == Some(b"ftyp")
            && matches!(
                bytes.get(8..12),
                Some(b"avif" | b"avis" | b"heic" | b"heix" | b"hevc" | b"hevx" | b"mif1" | b"msf1")
            ))
}

/// The literal strings `e` can be: a string, a template of literals, either
/// arm of a choice.
fn literals(e: &Expr, out: &mut Vec<(String, Span)>) {
    match e {
        Expr::Str(s, at) => out.push((s.clone(), *at)),
        Expr::Template(parts, at) => {
            if let Some(s) = crate::symbols::constant(parts) {
                out.push((s, *at));
            }
        }
        Expr::Ternary(_, a, b, _) => {
            literals(a, out);
            literals(b, out);
        }
        Expr::Let { body, .. } => literals(body, out),
        _ => {}
    }
}

/// The refusal for one literal source, when it is a relative file path
/// whose first folder is not one the build bundles.
fn refusal(what: &str, source: &str, span: Span) -> Option<LowerError> {
    if !is_outside(source) {
        return None;
    }
    let name = source.rsplit(['/', '\\']).next().unwrap_or(source);
    // `Assets/` resolves on a Mac's case-insensitive volume and nowhere
    // case-sensitive (Linux, a web server): the folder's own spelling.
    let first = source.split('/').find(|s| !s.is_empty() && *s != ".");
    let message = match first.and_then(|f| BUNDLED.iter().find(|b| b.eq_ignore_ascii_case(f) && **b != f)) {
        Some(folder) => format!(
            "{what} `{source}` spells the {folder}/ folder in another case, which a case-sensitive file system (Linux, a web server) does not find; write it \"{folder}/…\" in lowercase"
        ),
        None => format!(
            "{what} `{source}` is outside assets/, the folder every host ships the app's pictures and media from; move the file under assets/ and write \"assets/{name}\""
        ),
    };
    Some(LowerError {
        id: "lower-asset-path",
        message,
        span,
    })
}

/// A relative file path (it has a folder or an extension) that leaves the
/// app or starts outside [`BUNDLED`]. A scheme (`https:`, `symbol:`,
/// `data:`, a package's), a host path (`/…`) and a bare word are not. Folders
/// are `/`-separated on every host, as for fonts and sounds
/// (`exact_plan::is_portable_asset_path`), so a backslash is outside.
fn is_outside(source: &str) -> bool {
    let scheme = source.split_once(':').is_some_and(|(s, _)| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if source.is_empty() || scheme || source.starts_with('/') || !source.contains(['/', '.', '\\'])
    {
        return false;
    }
    if source.contains('\\') || source.split('/').any(|s| s == "..") {
        return true;
    }
    let mut folders = source.split('/').filter(|s| !s.is_empty() && *s != ".");
    let first = folders.next();
    // `assets/` itself, with no file under it, names nothing either.
    !(first.is_some_and(|f| BUNDLED.contains(&f)) && folders.next().is_some())
}

#[cfg(test)]
mod tests {
    use super::is_outside;

    #[test]
    fn a_literal_file_outside_assets_is_refused_and_urls_and_bundled_paths_are_not() {
        for outside in [
            "public/logo.png",
            "logo.png",
            "./public/logo.png",
            "images/a/b.jpg",
            "assets/../public/logo.png",
            "../logo.png",
            "src/assets/logo.png",
            "public/clip.mp4",
            "assets\\logo.png",
            "assets/..",
        ] {
            assert!(is_outside(outside), "{outside} ships");
        }
        for fine in [
            "assets/logo.png",
            "./assets/logo.png",
            "assets/icons/star.svg",
            "deck/slide.png",
            "https://example.com/logo.png",
            "http://example.com/a.png",
            "symbol:heart",
            "symbol:sf/heart.text.square",
            "data:image/png;base64,iVBORw0",
            "app:/tmp/frame.jpg",
            "blob:abc",
            "/assets/photo.jpg",
            "gear",
            "",
        ] {
            assert!(!is_outside(fine), "{fine} is refused");
        }
    }
}
