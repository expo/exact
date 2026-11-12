//! A literal file an `image`, `video` or `audio` names ships only from the
//! app's `assets/` (or `deck/`): the build bundles those folders and nothing
//! else (docs/recipes/app-json.md), so `image "public/logo.png"` built green
//! and drew nothing on the phone (expo/exact-build#4). A URL, a symbol, a
//! host path (`/…`) or a computed source is not a file of the app's; a bare
//! word is `lower-image-source`'s warning.

use crate::LowerError;
use contract_syntax::{Expr, Node, Span};
use std::path::{Component, Path};

/// The folders the build bundles into the app.
const BUNDLED: &[&str] = &["assets", "deck"];

/// `lower-asset-path`: each literal source or poster in `view` that names a
/// file outside [`BUNDLED`], once per place written.
pub(crate) fn outside(view: &[Node]) -> Vec<LowerError> {
    let mut out: Vec<LowerError> = Vec::new();
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
                        if let Some(e) = refusal(what, &source, span) {
                            // A component used twice is inlined twice: one refusal for its source.
                            if !out.contains(&e) {
                                out.push(e);
                            }
                        }
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
    out
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
        _ => {}
    }
}

/// The refusal for one literal source, when it is a relative file path
/// whose first folder is not one the build bundles.
fn refusal(what: &str, source: &str, span: Span) -> Option<LowerError> {
    if !is_outside(source) {
        return None;
    }
    let name = Path::new(source)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(source);
    Some(LowerError {
        id: "lower-asset-path",
        message: format!(
            "{what} `{source}` is outside assets/, the folder bundled into the app, so the app would ship without it; move the file under assets/ and write \"assets/{name}\""
        ),
        span,
    })
}

/// A relative file path (it has a folder or an extension) that leaves the
/// app or starts outside [`BUNDLED`]. A scheme (`https:`, `symbol:`,
/// `data:`, a package's), a host path (`/…`) and a bare word are not.
fn is_outside(source: &str) -> bool {
    let scheme = source.split_once(':').is_some_and(|(s, _)| {
        s.starts_with(|c: char| c.is_ascii_alphabetic())
            && s.chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
    });
    if source.is_empty() || scheme || source.starts_with('/') || !source.contains(['/', '.']) {
        return false;
    }
    let path = Path::new(source);
    if path
        .components()
        .any(|c| !matches!(c, Component::Normal(_) | Component::CurDir))
    {
        return true;
    }
    let first = path.components().find_map(|c| match c {
        Component::Normal(first) => first.to_str(),
        _ => None,
    });
    // `assets/` itself, with no file under it, names nothing either.
    !(first.is_some_and(|f| BUNDLED.contains(&f)) && path.components().count() > 1)
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
