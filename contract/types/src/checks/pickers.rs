//! The File System Access API's pickers (LLP 1069.010 D2) and `saveFile`
//! (D3), each with the API's `startIn=` (LLP 1173 D2).

use super::{err, infer, Scope, Shapes, Ty, TypeError};
use contract_syntax::{Expr, Span};

/// What `startIn=` takes as a literal: the API's `WellKnownDirectory`
/// values and `screenshots`, Exact's addition. A computed value may also
/// be a `doc:/` path the session holds; a host refuses any other into the
/// journal and opens the picker where it would have.
const WELL_KNOWN: [&str; 7] = [
    "desktop",
    "documents",
    "downloads",
    "music",
    "pictures",
    "videos",
    "screenshots",
];

/// Each option takes one string. A literal `startIn` must name a well-known folder.
fn string_option(
    option: &str,
    code: &'static str,
    name: &str,
    args: &[Expr],
    scope: &Scope,
    shapes: &Shapes,
) -> Result<(), TypeError> {
    let mut given = args.iter().filter_map(|a| match a {
        Expr::NamedArg(n, value, at) if n == option => Some((value.as_ref(), *at)),
        _ => None,
    });
    let Some((value, _)) = given.next() else {
        return Ok(());
    };
    if let Some((_, at)) = given.next() {
        return err(code, format!("`{option}=` is given twice"), at);
    }
    let t = infer(value, scope, shapes)?;
    if Ty::String.unify(&t).is_none() {
        return err(
            code,
            format!("`{name}`'s `{option}=` is a string, not `{t}`"),
            value.span(),
        );
    }
    if let Expr::Str(folder, at) = value {
        if option == "startIn" && !WELL_KNOWN.contains(&folder.as_str()) {
            return err(
                code,
                format!(
                    "`startIn=` is a well-known folder ({}) or a `doc:/` path the person chose, not \"{folder}\"",
                    WELL_KNOWN.map(|w| format!("\"{w}\"")).join(", ")
                ),
                *at,
            );
        }
    }
    Ok(())
}

/// The three pickers' positional arguments: an element id, then `multiple`
/// (a bool) for `showOpenFilePicker` or `suggestedName` (a string) for
/// `showSaveFilePicker`; then, named, `startIn` and `apple.message`.
pub(super) fn picker_args(
    name: &str,
    args: &[Expr],
    scope: &Scope,
    shapes: &Shapes,
    span: Span,
) -> Result<(), TypeError> {
    let (usage, second) = match name {
        "showOpenFilePicker" => (
            "showOpenFilePicker(id) or showOpenFilePicker(id, multiple)",
            Some((Ty::Bool, false)),
        ),
        "showSaveFilePicker" => (
            "showSaveFilePicker(id, suggestedName)",
            Some((Ty::String, true)),
        ),
        _ => ("showDirectoryPicker(id)", None),
    };
    let wrong = |at| {
        err(
            "type-file-picker-argument",
            format!("`{name}` takes `{usage}`, with optional `startIn=` and `apple.message=`"),
            at,
        )
    };
    let positional: Vec<&Expr> = args
        .iter()
        .filter(|a| !matches!(a, Expr::NamedArg(..)))
        .collect();
    let most = if second.is_some() { 2 } else { 1 };
    let least = match second {
        Some((_, true)) => 2,
        _ => 1,
    };
    for arg in args {
        if let Expr::NamedArg(n, _, at) = arg {
            if !matches!(n.as_str(), "startIn" | "apple.message") {
                return err(
                    "type-file-picker-argument",
                    format!("`{name}` does not take `{n}=`. Its named arguments are `startIn=` and `apple.message=`"),
                    *at,
                );
            }
        }
    }
    if positional.len() < least
        || positional.len() > most
        || matches!(args.first(), Some(Expr::NamedArg(..)))
    {
        return wrong(span);
    }
    for (i, arg) in positional.into_iter().enumerate() {
        let want = if i == 0 {
            Ty::String
        } else {
            second.as_ref().map_or(Ty::String, |(t, _)| t.clone())
        };
        let t = infer(arg, scope, shapes)?;
        if want.unify(&t).is_none() {
            return wrong(arg.span());
        }
    }
    for option in ["startIn", "apple.message"] {
        string_option(
            option,
            "type-file-picker-argument",
            name,
            args,
            scope,
            shapes,
        )?;
    }
    Ok(())
}

/// `saveFile(id, from, suggestedName)` (LLP 1069.010 D3): three strings,
/// positional. Whether `from` is granted is the host's to refuse. Or the
/// file's text itself, `saveFile(id, text=…, suggestedName=…)`, so
/// exporting what is on screen is one press (x2apps notes #4): the names
/// are `share`'s `text` and `showSaveFilePicker`'s `suggestedName`.
/// Either takes `startIn=`.
pub(super) fn save_file_args(
    args: &[Expr],
    scope: &Scope,
    shapes: &Shapes,
    span: Span,
) -> Result<(), TypeError> {
    let named = |want: &str| {
        args.iter()
            .filter(|a| matches!(a, Expr::NamedArg(n, ..) if n == want))
            .count()
    };
    let positional = args
        .iter()
        .filter(|a| !matches!(a, Expr::NamedArg(..)))
        .count();
    let rest = args.len() - named("startIn").min(1);
    let file = rest == 3 && positional == 3;
    let text = rest == 3
        && positional == 1
        && named("text") == 1
        && named("suggestedName") == 1
        && !matches!(args[0], Expr::NamedArg(..));
    if !file && !text {
        return err(
            "type-save-file-argument",
            "`saveFile` takes three strings, `saveFile(\"export-file\", \"app:/data/export.json\", \"export.json\")`, or the text to save, `saveFile(\"export-file\", text=body, suggestedName=\"note.md\")`, and `startIn=`",
            span,
        );
    }
    for arg in args {
        if matches!(arg, Expr::NamedArg(n, ..) if n == "startIn") {
            continue;
        }
        let arg = match arg {
            Expr::NamedArg(_, value, _) => value.as_ref(),
            arg => arg,
        };
        let t = infer(arg, scope, shapes)?;
        if Ty::String.unify(&t).is_none() {
            return err(
                "type-save-file-argument",
                format!("`saveFile`'s arguments are strings, not `{t}`"),
                arg.span(),
            );
        }
    }
    string_option(
        "startIn",
        "type-save-file-argument",
        "saveFile",
        args,
        scope,
        shapes,
    )
}
