//! `dataTransfer=DataTransfer(text=, url=, files=[…], names=[…])` (LLP
//! 1094.000 D1, D2, D14): what a drag carries, by `DataTransfer`'s names, a
//! named-argument construct like `share(…)` and no value of its own. Each
//! argument is optional and one of `text`, `url` and `files` is required;
//! `text` and `url` are strings, and `files` and `names` lists of strings,
//! `names` only beside `files`. A literal `url` is absolute `http:` or
//! `https:`, a literal file is a file the app ships (`assets/…`) or an `app:/`
//! path, and a literal name keeps D14's rule as written. Whether a shipped
//! file exists is lowering's to find; the grants, the host's reach and the
//! name as offered are the runner's.

use super::{err, infer, Scope, Shapes, Ty, TypeError};
use contract_syntax::{Expr, Span};
use std::collections::BTreeSet;

const USAGE: &str = "`dataTransfer=DataTransfer(files=[\"assets/photo.jpg\"])`, with `text=`, `url=`, `files=` or `names=` by name";

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
    const NAMES: [&str; 4] = ["text", "url", "files", "names"];
    let mut seen = BTreeSet::new();
    let (mut files_given, mut names_given) = (None, None);
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
                format!("`DataTransfer` has no argument `{name}`; it takes `text=`, `url=`, `files=` and `names=`"),
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
            "files" => {
                files(value, &t)?;
                files_given = Some(value);
            }
            "names" => {
                names(value, &t)?;
                names_given = Some((value, *at));
            }
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
    if let Some((names, at)) = names_given {
        let Some(files) = files_given else {
            return err(
                "type-data-transfer",
                "`names=` names the files of `files=`, and this `DataTransfer` has no `files=`: add the files, or remove `names=`",
                at,
            );
        };
        if let (Expr::List(files, _), Expr::List(names, list)) = (&**files, &**names) {
            if names.len() > files.len() {
                return err(
                    "type-data-transfer",
                    format!(
                        "`names=` has {} names for {} {}: each name goes with the file at its place in `files=`, so remove the names past the last file",
                        names.len(),
                        files.len(),
                        if files.len() == 1 { "file" } else { "files" }
                    ),
                    *list,
                );
            }
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

/// `names=`: a list of strings, each literal name one every host can write
/// as written, or empty for the file's own (LLP 1094.000 D14).
fn names(value: &Expr, t: &Ty) -> Result<(), TypeError> {
    if Ty::List(Box::new(Ty::String)).unify(t).is_none() {
        return err(
            "type-data-transfer",
            format!("`names=` is a list of strings, a name for each file of `files=`, not `{t}`"),
            value.span(),
        );
    }
    let Expr::List(items, _) = value else {
        return Ok(());
    };
    items.iter().try_for_each(name)
}

/// A literal name, or each arm of a choice between names.
fn name(e: &Expr) -> Result<(), TypeError> {
    match e {
        Expr::Str(s, _) if s.is_empty() => Ok(()),
        Expr::Str(s, at) => match exact_plan::drag_name_refusal(s) {
            Some(why) => err(
                "type-data-transfer",
                format!("`names=` takes file names, not {s:?}: {why}"),
                *at,
            ),
            None => Ok(()),
        },
        Expr::Ternary(_, yes, no, _) => name(yes).and_then(|()| name(no)),
        _ => Ok(()),
    }
}

/// A literal `url=`: absolute `http:` or `https:`, with a host, by the
/// runner's rule (`exact_plan::http_url_refusal`, LLP 1094.000 D1).
fn url(value: &Expr) -> Result<(), TypeError> {
    let Expr::Str(s, at) = value else {
        return Ok(());
    };
    let Some(why) = exact_plan::http_url_refusal(s) else {
        return Ok(());
    };
    err(
        "type-data-transfer",
        format!("`url=` is an absolute `http:` or `https:` URL, not \"{s}\": {why}"),
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
