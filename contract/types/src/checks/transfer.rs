//! `dataTransfer=DataTransfer(text=, url=, files=[…])` (LLP 1094.000 D1, D2):
//! what a drag carries, by `DataTransfer`'s names, a named-argument construct
//! like `share(…)` and no value of its own. Each argument is optional and one
//! is required; `text` and `url` are strings and `files` a list of strings.
//! A literal `url` is absolute `http:` or `https:`, and a literal file is a
//! file the app ships (`assets/…`) or an `app:/` path. Whether a shipped file
//! exists is lowering's to find; the grants and the host's reach are the
//! runner's.

use super::{err, infer, Scope, Shapes, Ty, TypeError};
use contract_syntax::{Expr, Span};
use std::collections::BTreeSet;

const USAGE: &str = "`dataTransfer=DataTransfer(files=[\"assets/photo.jpg\"])`, with `text=`, `url=` or `files=` by name";

/// The attribute's value: a literal `DataTransfer(…)` and its arguments.
pub(super) fn check(value: &Expr, scope: &Scope, shapes: &Shapes) -> Result<(), TypeError> {
    let Expr::Call(name, args, span) = value else {
        return err(
            "type-data-transfer",
            format!("`dataTransfer` takes {USAGE}"),
            value.span(),
        );
    };
    if name != "DataTransfer" {
        return err(
            "type-data-transfer",
            format!("`dataTransfer` takes {USAGE}, not `{name}(…)`"),
            *span,
        );
    }
    arguments(args, scope, shapes, *span)
}

fn arguments(args: &[Expr], scope: &Scope, shapes: &Shapes, span: Span) -> Result<(), TypeError> {
    const NAMES: [&str; 3] = ["text", "url", "files"];
    let mut seen = BTreeSet::new();
    for arg in args {
        let Expr::NamedArg(name, value, at) = arg else {
            return err(
                "type-data-transfer",
                format!("`DataTransfer` takes named arguments: {USAGE}"),
                arg.span(),
            );
        };
        if !NAMES.contains(&name.as_str()) {
            return err(
                "type-data-transfer",
                format!("`DataTransfer` has no argument `{name}`; it takes `text=`, `url=` and `files=`. A dragged file keeps its own name"),
                *at,
            );
        }
        if !seen.insert(name.as_str()) {
            return err(
                "type-data-transfer",
                format!("`{name}=` is given twice"),
                *at,
            );
        }
        let t = infer(value, scope, shapes)?;
        match name.as_str() {
            "files" => files(value, &t)?,
            _ if Ty::String.unify(&t).is_none() => {
                return err(
                    "type-data-transfer",
                    format!("`{name}=` is a string, not `{t}`"),
                    value.span(),
                )
            }
            "url" => url(value)?,
            _ => {}
        }
    }
    if seen.is_empty() {
        return err(
            "type-data-transfer",
            "`DataTransfer` needs `text=`, `url=` or `files=`",
            span,
        );
    }
    Ok(())
}

/// A literal `url=`: absolute `http:` or `https:`, with a host, as `share`'s
/// runner rule reads one.
fn url(value: &Expr) -> Result<(), TypeError> {
    let Expr::Str(s, at) = value else {
        return Ok(());
    };
    let lower = s.to_ascii_lowercase();
    let rest = lower
        .strip_prefix("https://")
        .or_else(|| lower.strip_prefix("http://"));
    let absolute = rest.is_some_and(|r| {
        !r.is_empty()
            && !r.starts_with(['/', '?', '#'])
            && !s.chars().any(|c| c.is_whitespace() || c.is_control())
    });
    if absolute {
        return Ok(());
    }
    err(
        "type-data-transfer",
        format!("`url=` is an absolute `http:` or `https:` URL, not \"{s}\": another app cannot open this app's path"),
        *at,
    )
}

/// `files=`: a list of strings; a literal list carries something, and each
/// literal entry is a shipped file or an `app:/` path.
fn files(value: &Expr, t: &Ty) -> Result<(), TypeError> {
    if Ty::List(Box::new(Ty::String)).unify(t).is_none() {
        return err(
            "type-data-transfer",
            format!("`files=` is a list of strings, shipped `assets/` files and `app:/` paths, not `{t}`"),
            value.span(),
        );
    }
    let Expr::List(items, at) = value else {
        return Ok(());
    };
    if items.is_empty() {
        return err("type-data-transfer", "`files=[]` carries nothing", *at);
    }
    items.iter().try_for_each(entry)
}

/// A literal entry, or each arm of a choice between entries.
fn entry(e: &Expr) -> Result<(), TypeError> {
    match e {
        Expr::Str(s, at) => match refused_entry(s) {
            Some(reason) => err("type-data-transfer", reason, *at),
            None => Ok(()),
        },
        Expr::Ternary(_, yes, no, _) => entry(yes).and_then(|()| entry(no)),
        _ => Ok(()),
    }
}

/// Why a literal entry is no file a drag carries, if it is not one.
fn refused_entry(s: &str) -> Option<String> {
    const TAKES: &str =
        "`files=` takes a file the app ships, such as `assets/photo.jpg`, or an `app:/` path";
    if s.starts_with("app:/") || exact_plan::is_shipped_drag_file(s) {
        return None;
    }
    let lower = s.to_ascii_lowercase();
    let why = if s.starts_with('#') {
        "A drag carries the file itself; a picture of an element is `share`'s. Name the image's own file"
    } else if ["http:", "https:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        "A drag carries a file this app holds; a remote image's address goes in `url=`"
    } else if ["data:", "symbol:", "doc:"]
        .iter()
        .any(|scheme| lower.starts_with(scheme))
    {
        "A drag carries the file itself, one the app ships or keeps under `app:/`"
    } else {
        "A shipped file's path starts `assets/`, as an `image` names one, with no `.` or `..` segment"
    };
    Some(format!("{TAKES}, not \"{s}\". {why}"))
}
