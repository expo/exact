//! Length tokens written as their values where the theme cannot bind one:
//! in grid tracks, which the kernel's track parser reads; in a custom
//! property's declaration, which the kernel holds as a tree; and in a
//! `var()`'s fallback, which a row reads at run time (LLP 1123 D5). A token
//! that varies by mode stays a name in grid tracks, and is refused in the
//! other two, where nothing binds a mode.

use crate::LowerError;
use contract_syntax::{Attr, Expr, TemplatePart};
use exact_kernel::StyleId;
use std::borrow::Cow;

/// A grid track list with each length token it names written as the token's
/// value (`minmax(card-min-width, 1fr)`, the loader having written
/// `var(--card-min-width)` as the name), as a browser substitutes `var()` before
/// it parses the tracks (CSS Variables 1 §3). Only a token with one value in
/// every app mode is written in; the kernel's track parser refuses any other.
pub(crate) fn grid_tokens(a: &Attr, rows: &[StyleId]) -> Option<Attr> {
    if !rows
        .iter()
        .any(|r| matches!(r, StyleId::GridTemplateColumns | StyleId::GridTemplateRows))
    {
        return None;
    }
    let mut out = a.clone();
    texts(&mut out.value, &mut |text, whole| {
        whole && written_tokens(text, None)
    })
    .then_some(out)
}

/// A custom property's declared value `e` with each design token in it
/// written as its value where the token is a length with one value in every
/// app mode, as a length row reads a token: through `var()`, which the
/// loader leaves as written here (LLP 1123 D5), or by its name, where the
/// text is literal whole; whether any was. A template whose holes are all
/// literal once a component is expanded is that literal. In a template with
/// a hole computed at run time only a `var()` is a token, since the hole's
/// text is unknown here. `varying` takes the first length token with more
/// than one value, which nothing binds where a value is read at run time.
pub(crate) fn call_tokens(e: &mut Expr, varying: &mut Option<String>) -> bool {
    texts(e, &mut |text, whole| {
        let explicit = token_calls(text, varying);
        explicit | whole && written_tokens(text, Some(varying))
    })
}

/// A length row's value with each token `var()` in it written as the
/// token's value, refused where the token varies by mode. The loader leaves
/// such a call as written only in a runtime fallback, having written every
/// other as the token's name, which the theme binds. A row's attribute goes
/// through this before a box shorthand splits its sides, so each side's
/// fallback is written too.
pub(crate) fn fallback_tokens<'a>(
    a: &'a Attr,
    rows: &[StyleId],
) -> Result<Cow<'a, Attr>, LowerError> {
    if !rows
        .iter()
        .all(|r| r.codec() == exact_kernel::StyleCodec::Dimension)
    {
        return Ok(Cow::Borrowed(a));
    }
    let mut out = a.clone();
    let mut varying = None;
    let changed = texts(&mut out.value, &mut |text, _| {
        token_calls(text, &mut varying)
    });
    if let Some(token) = varying {
        return Err(LowerError {
            id: "lower-attr-value",
            message: format!("`{}`: the design token `{token}` differs between the app's modes, and a custom property's fallback is read at run time, where nothing binds it; give the property a value, or write a fixed length as its fallback", a.name),
            span: a.span,
        });
    }
    Ok(match changed {
        true => Cow::Owned(out),
        false => Cow::Borrowed(a),
    })
}

/// `text` with each `var()` of a length token that has one value in every
/// app mode written as the value; whether any was. A call in a comment, of a
/// host's name (`--exact-`, LLP 1081 D5) or of a name no length token has
/// stays.
fn token_calls(text: &mut String, varying: &mut Option<String>) -> bool {
    let name_char = |c: char| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.');
    let mut out = String::with_capacity(text.len());
    let (mut at, mut i, mut changed) = (0, 0, false);
    while let Some(c) = text[i..].chars().next() {
        if let Some(past) = comment_end(text, i) {
            i = past;
            continue;
        }
        let call = text[i..]
            .get(..4)
            .is_some_and(|s| s.eq_ignore_ascii_case("var("))
            && !text[..i].ends_with(name_char);
        if !call {
            i += c.len_utf8();
            continue;
        }
        let args = i + 4;
        let rest = text[args..].trim_start();
        let name = rest.strip_prefix("--").unwrap_or_default();
        let name = &name[..name.find(|c| !name_char(c)).unwrap_or(name.len())];
        let Some(close) = call_end(text, args) else {
            break;
        };
        match crate::theme::measure_written(name).filter(|_| !name.starts_with("exact-")) {
            Some((exact_forma::Kind::Length, values)) if values.len() == 1 => {
                out.push_str(&text[at..i]);
                out.push_str(&values[0]);
                (at, i, changed) = (close + 1, close + 1, true);
            }
            Some((exact_forma::Kind::Length, _)) => {
                varying.get_or_insert_with(|| name.to_string());
                i = args;
            }
            _ => i = args,
        }
    }
    out.push_str(&text[at..]);
    *text = out;
    changed
}

/// The byte of the `)` that closes a function whose arguments start at
/// `from`, past nested parentheses, strings and comments.
fn call_end(text: &str, from: usize) -> Option<usize> {
    let (mut depth, mut quote, mut i) = (0usize, None, from);
    while let Some(c) = text[i..].chars().next() {
        if quote.is_none() {
            if let Some(past) = comment_end(text, i) {
                i = past;
                continue;
            }
        }
        match (quote, c) {
            // An escape takes the whole character after it
            (Some(_), '\\') => {
                i += 1;
                if let Some(escaped) = text[i..].chars().next() {
                    i += escaped.len_utf8();
                }
                continue;
            }
            (Some(q), c) if c == q => quote = None,
            (Some(_), _) => {}
            (None, '"' | '\'') => quote = Some(c),
            (None, '(') => depth += 1,
            (None, ')') if depth == 0 => return Some(i),
            (None, ')') => depth -= 1,
            _ => {}
        }
        i += c.len_utf8();
    }
    None
}

/// Each literal text of `e`, in either arm of a choice, under a binding and
/// between a template's holes, passed to `f` with whether it is the whole
/// value; whether `f` changed any.
fn texts(e: &mut Expr, f: &mut dyn FnMut(&mut String, bool) -> bool) -> bool {
    texts_in(e, f, true)
}

/// [`texts`], `whole` where `e` is not inside a template, whose texts and
/// holes are each one part of the value.
fn texts_in(e: &mut Expr, f: &mut dyn FnMut(&mut String, bool) -> bool, whole: bool) -> bool {
    match e {
        Expr::Str(text, _) => f(text, whole),
        Expr::Template(parts, _) => {
            let mut changed = false;
            for part in parts {
                changed |= match part {
                    TemplatePart::Text(text) => f(text, false),
                    TemplatePart::Expr(e) => texts_in(e, f, false),
                };
            }
            changed
        }
        Expr::Ternary(_, yes, no, _)
        | Expr::Match {
            some: yes,
            none: no,
            ..
        } => texts_in(yes, f, whole) | texts_in(no, f, whole),
        Expr::Let { body, .. } | Expr::Typed(body, ..) => texts_in(body, f, whole),
        _ => false,
    }
}

/// `text`, a whole literal value, with each word that names a length token
/// with one value in every app mode written as the value, as a length row
/// reads a token by name; whether any was. `varying`, where given, takes the
/// first length token with more than one value. A function's name, a custom
/// property's (after `-`), the variable `env()` names and a number's unit
/// are no token, and a comment is copied as it stands.
fn written_tokens(text: &mut String, mut varying: Option<&mut Option<String>>) -> bool {
    let mut out = String::with_capacity(text.len());
    let (mut changed, mut env, mut i) = (false, false, 0);
    // What stands since the last word, comments aside
    let mut between = String::new();
    while let Some(c) = text[i..].chars().next() {
        if let Some(past) = comment_end(text, i) {
            out.push_str(&text[i..past]);
            // A comment separates what stands on either side, as white space does
            between.push(' ');
            i = past;
            continue;
        }
        // A word starts with a letter or `_`, so `--_pad` is one name after `--`
        if !c.is_ascii_alphabetic() && c != '_' {
            out.push(c);
            between.push(c);
            i += c.len_utf8();
            continue;
        }
        let end = text[i..]
            .find(|c: char| !(c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.')))
            .map_or(text.len(), |e| i + e);
        let word = &text[i..end];
        // A word that follows a digit is the number's unit (`1fr`, `9.5rem`)
        let unit = between.ends_with(|c: char| c.is_ascii_digit() || c == '.');
        let function = text[end..].starts_with('(');
        let variable = env && between.trim() == "(";
        let named = between.ends_with('-') || function || variable;
        match crate::theme::measure_written(word).filter(|_| !unit && !named) {
            Some((exact_forma::Kind::Length, values)) if values.len() == 1 => {
                out.push_str(&values[0]);
                changed = true;
            }
            Some((exact_forma::Kind::Length, _)) => {
                if let Some(v) = varying.as_deref_mut() {
                    v.get_or_insert_with(|| word.to_string());
                }
                out.push_str(word);
            }
            _ => out.push_str(word),
        }
        env = function && word.eq_ignore_ascii_case("env");
        between.clear();
        i = end;
    }
    *text = out;
    changed
}

/// Where a comment that opens at `i` ends, past its `*/` (or at the text's
/// end); `None` where none opens there.
fn comment_end(text: &str, i: usize) -> Option<usize> {
    text[i..].starts_with("/*").then(|| {
        text[i + 2..]
            .find("*/")
            .map_or(text.len(), |e| i + 2 + e + 2)
    })
}
