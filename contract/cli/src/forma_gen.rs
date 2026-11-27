//! `contract forma gen ts|rust|swift|kotlin <app> [-o <dir>]` — every language's
//! generated names over the one table (LLP 1121 D12): from the design's IR,
//! never committed. They go in the app's `.exact/forma/` (gitignored, as
//! all of `.exact/` is) unless `-o` names a directory.
//!
//! - `ts`: `forma.ts`, a `<Kind>Token` union per kind (`ColorToken`) and
//!   each token's resolved CSS per full app tuple, and `forma.css`, those
//!   values as custom properties per tuple, for code outside a plan. The
//!   data module's union is in `contract types`' declarations
//!   ([`crate::typescript()`]).
//! - `rust`: `forma.rs`, `pub const SURFACE_CARD: Token<Color>` per colour
//!   token, which `session.forma().get(token, place)` resolves.
//! - `swift`: `forma.swift`, `ExactFormaToken.surfaceCard` per colour,
//!   length, number and type token, for `ExactForma(view).color(.surfaceCard)`
//!   and `.length(_:)`, `.number(_:)`, `.font(_:)`; a native module's build
//!   compiles it into the module's library when it is there.
//!
//! - `kotlin`: `FormaTokens.kt`, `FormaColor.SURFACE_CARD` (and
//!   `FormaLength`, `FormaNumber`) per token, for
//!   `ExactForma(view).color(FormaColor.SURFACE_CARD)`; the reader's build
//!   compiles it beside the app's hatches when it is there.
//!
//! A component's param (D15) is named by `<Component>.<param>` (`mapCardPin`,
//! `MAP_CARD_PIN`), its value the token name below the IR, which the runtimes
//! resolve as any token's.
//!
//! They name the tokens a binding may (D7): the design's `exports` when it
//! lists them, else every token, which is what an app that can name tokens
//! outside its views keeps in its plan.
//!
//! Names map as D12 says: dots and hyphens become camelCase in TypeScript
//! and Swift and `SCREAMING_SNAKE` in Rust and Kotlin. A design
//! two of whose names collide after mapping is refused, naming both.

use std::fmt::Write;
use std::path::Path;

/// `name` as a camelCase identifier: `surface.card` `surfaceCard`,
/// `-exact-label` `exactLabel`, `CanvasText` `canvasText`.
fn camel(name: &str) -> String {
    let mut out = String::new();
    for word in name.split(['.', '-']).filter(|w| !w.is_empty()) {
        let mut chars = word.chars();
        let first = chars.next().expect("a word has a character");
        match out.is_empty() {
            true => out.extend(first.to_lowercase()),
            false => out.extend(first.to_uppercase()),
        }
        out.extend(chars);
    }
    out
}

/// `name` as a `SCREAMING_SNAKE` identifier: `surface.card` `SURFACE_CARD`,
/// `CanvasText` `CANVAS_TEXT`.
fn screaming(name: &str) -> String {
    let mut out = String::new();
    for word in name.split(['.', '-']).filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push('_');
        }
        let mut lower = false;
        for c in word.chars() {
            if c.is_uppercase() && lower {
                out.push('_');
            }
            lower = c.is_lowercase() || c.is_ascii_digit();
            out.extend(c.to_uppercase());
        }
    }
    out
}

/// The design's names that collide after `map`, as `a` and `b` pairs.
fn collisions(names: &[&str], map: fn(&str) -> String) -> Vec<(String, String)> {
    let mut seen: Vec<(String, &str)> = Vec::new();
    let mut out = Vec::new();
    for name in names {
        let mapped = map(name);
        match seen.iter().find(|(m, _)| *m == mapped) {
            Some((_, first)) => out.push((first.to_string(), name.to_string())),
            None => seen.push((mapped, name)),
        }
    }
    out
}

/// One token as the generators read it: its name, what its binding is
/// named from, its kind and CSS per tuple.
struct Token {
    name: String,
    binding: String,
    kind: String,
    css: Vec<String>,
}

/// What a token's binding is named from: its name, or a component's
/// param's `<Component>.<param>` (D15), never the hash below the IR.
fn binding(name: &str) -> &str {
    name.strip_prefix(contract_lower::params::PREFIX)
        .and_then(|rest| rest.split_once('.'))
        .map_or(name, |(_, named)| named)
}

/// The tokens a binding may name (D7): the design's `exports` when it lists
/// them, else every token; what an app with bindings keeps in its plan; and
/// every component's param (D15).
fn tokens(folding: &exact_forma::Folding) -> Vec<Token> {
    let exported = |t: &exact_forma::Checked| {
        t.name.starts_with(contract_lower::params::PREFIX)
            || folding.design.exports.as_ref().is_none_or(|exports| {
                exports
                    .iter()
                    .any(|x| exact_forma::exports_names(&x.value, &t.name, t.role.as_deref()))
            })
    };
    let vocab = contract_lower::forma_vocab();
    folding
        .tokens
        .iter()
        .zip(&folding.design.tokens)
        .filter(|(t, _)| exported(t))
        .map(|(t, _)| Token {
            name: t.name.clone(),
            binding: binding(&t.name).to_string(),
            kind: t.kind.name().into(),
            css: t
                .values
                .iter()
                .map(|v| match v.color() {
                    Some(c) => standalone(c, &vocab),
                    None => outside(v, &vocab),
                })
                .collect(),
        })
        .collect()
}

/// A length, a type token's size or a shadow's lengths, as CSS outside
/// Exact: a text style, an `em` or the hairline as px at the default root
/// size (16px), which the header of `forma.css` says (stage 6 G); the rest
/// as written.
fn outside(v: &exact_forma::Value, vocab: &exact_forma::Vocab) -> String {
    use exact_forma::{Length, Value};
    let named = |name: &str| {
        vocab
            .sizes
            .iter()
            .find(|(n, _)| n.eq_ignore_ascii_case(name))
            .and_then(|(_, px)| px.parse::<f64>().ok())
    };
    let px = |l: &Length| match l {
        Length::Em(n) => Some(n * 16.0),
        Length::Named(name) => named(name),
        // Over base names (stage 12 B2): each base's px, as a bare one.
        Length::Base(r) => r.fold(&named),
        _ => None,
    };
    let text = |n: f64| Length::Px(n).css();
    match v {
        Value::Length(l) => px(l).map_or_else(|| v.css(), text),
        Value::Type(t) => match px(&t.size) {
            Some(n) => {
                let mut sized = t.clone();
                sized.size = Length::Px(n);
                sized.css()
            }
            None => v.css(),
        },
        // A shadow's lengths over base names likewise (stage 12 B3).
        Value::Shadow(s) => s.folded(&named).map_or_else(|| v.css(), |s| s.css()),
        _ => v.css(),
    }
}

/// A value as CSS outside Exact: an Exact role (`-exact-label`) or a
/// platform colour is no colour a browser reads, so it is its fallback pair
/// (D7); a CSS system colour (`CanvasText`, `AccentColor`) stays.
fn standalone(value: &exact_forma::Folded, vocab: &exact_forma::Vocab) -> String {
    use exact_forma::{Folded, Host};
    let fallback = || {
        exact_forma::fallback(value, vocab).map_or_else(
            || value.css(),
            |[l, d]| match l == d {
                true => l,
                false => format!("light-dark({l}, {d})"),
            },
        )
    };
    match value {
        Folded::Host(Host::Role(row)) if row.starts_with("-exact-") => fallback(),
        Folded::Host(Host::Platform(..)) => fallback(),
        _ => value.css(),
    }
}

/// Each full tuple's key, `axis=value` by commas, in tuple order.
fn tuple_keys(folding: &exact_forma::Folding) -> Vec<String> {
    let axes = &folding.design.axes;
    (0..folding.tuples)
        .map(|tuple| {
            let mut rest = tuple;
            let mut pins: Vec<String> = Vec::new();
            for axis in axes.iter().rev() {
                pins.push(format!(
                    "{}={}",
                    axis.name.value,
                    axis.values[rest % axis.values.len()]
                ));
                rest /= axis.values.len();
            }
            pins.reverse();
            pins.join(",")
        })
        .collect()
}

fn kind_type(kind: &str) -> String {
    let mut name = camel(kind);
    if let Some(first) = name.get(..1) {
        name = first.to_uppercase() + &name[1..];
    }
    format!("{name}Token")
}

/// `forma.ts`: the unions, the axes, and each token's CSS per tuple.
fn typescript(folding: &exact_forma::Folding, source: &str) -> String {
    let tokens = tokens(folding);
    let quoted = |s: &str| serde_json::to_string(s).expect("a string is JSON");
    let mut out = format!(
        "// @generated by `contract forma gen ts` from {source}; do not edit, never committed.\n"
    );
    let mut kinds: Vec<&str> = Vec::new();
    for t in &tokens {
        if !kinds.contains(&t.kind.as_str()) {
            kinds.push(&t.kind);
        }
    }
    for kind in &kinds {
        let names: Vec<String> = tokens
            .iter()
            .filter(|t| t.kind == *kind)
            .map(|t| quoted(&t.name))
            .collect();
        let _ = writeln!(
            out,
            "export type {} = {};",
            kind_type(kind),
            names.join(" | ")
        );
    }
    let _ = writeln!(
        out,
        "/** Each full app tuple, `axis=value` by commas: the index of a token's `css`. */\nexport const tuples = [{}] as const;",
        tuple_keys(folding).iter().map(|k| quoted(k)).collect::<Vec<_>>().join(", ")
    );
    // Each token a property of `tokens`, by its camelCase name: a property
    // may be any word, a declaration of its own could clash.
    out.push_str("export const tokens = {\n");
    for t in &tokens {
        let css: Vec<String> = t.css.iter().map(|c| quoted(c)).collect();
        let _ = writeln!(
            out,
            "  {}: {{ name: {}, css: [{}] }},",
            camel(&t.binding),
            quoted(&t.name),
            css.join(", ")
        );
    }
    out.push_str("} as const;\n");
    out
}

/// `forma.css`: each token as `--<camelName>` per tuple, the first tuple's on
/// `:root` too; a page picks a tuple with `data-forma="axis=value,…"`.
fn css(folding: &exact_forma::Folding) -> String {
    let tokens = tokens(folding);
    let mut out = String::from(
        "/* @generated by `contract forma gen ts`; do not edit, never committed. */\n\
         /* A text style, an `em` and the hairline are px at the default root size (16px): here they do not follow the person's text size. */\n",
    );
    for (i, key) in tuple_keys(folding).iter().enumerate() {
        let selector = format!("[data-forma=\"{key}\"]");
        let _ = write!(
            out,
            "{}{{",
            if i == 0 {
                format!(":root,{selector}")
            } else {
                selector
            }
        );
        // By its camelCase name, which no two tokens share (`admitted`).
        for t in &tokens {
            let _ = write!(out, "--{}:{};", camel(&t.binding), t.css[i]);
        }
        out.push_str("}\n");
    }
    out
}

/// `forma.rs`: a `Token<Color>` per colour token, and since stage 6 G a
/// `Token<Length>`, `Token<Number>` and `Token<Type>` per token of those
/// kinds (a shadow's, a motion token's and an icon's have no binding).
fn rust(folding: &exact_forma::Folding, source: &str) -> String {
    let mut consts = String::new();
    let mut kinds: Vec<&str> = Vec::new();
    for t in tokens(folding) {
        let kind = match t.kind.as_str() {
            "color" => "Color",
            "length" => "Length",
            "number" => "Number",
            "type" => "Type",
            _ => continue,
        };
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
        let name = serde_json::to_string(&t.name).expect("a string is JSON");
        let _ = writeln!(
            consts,
            "/// `{}`.\npub const {}: Token<{kind}> = Token::new({name});",
            t.binding,
            screaming(&t.binding)
        );
    }
    kinds.sort();
    kinds.push("Token");
    format!(
        "// @generated by `contract forma gen rust` from {source}; do not edit, never committed.\n\
         // A token is its name: `session.forma().get(TOKEN, place)` resolves it (LLP 1121 D12).\n\
         use exact_runner::forma::{{{}}};\n{consts}",
        kinds.join(", ")
    )
}

/// `forma.swift`: an `ExactFormaToken` static member per colour, length,
/// number and type token (`.surfaceCard`), for `ExactForma(view).color(_:)`
/// and its siblings, in a native module's
/// library or a host that links ExactKit (it names no module: it is
/// compiled into the one that declares the type).
fn swift(folding: &exact_forma::Folding, source: &str) -> String {
    let mut out = format!(
        "// @generated by `contract forma gen swift` from {source}; do not edit, never committed.\n\
         // A token is its name: `ExactForma(view).color(.surfaceCard)` resolves it (LLP 1121 D12).\n\
         public extension ExactFormaToken {{\n"
    );
    // What `color`, `length`, `number` and `font` read (stage 7 D).
    let read = ["color", "length", "number", "type"];
    for t in tokens(folding)
        .iter()
        .filter(|t| read.contains(&t.kind.as_str()))
    {
        let member = camel(&t.binding);
        let name = serde_json::to_string(&t.name).expect("a string is JSON");
        let _ = writeln!(
            out,
            "    /// `{}`.\n    static let `{member}` = ExactFormaToken({name})",
            t.binding
        );
    }
    out.push_str("}\n");
    out
}

/// `FormaTokens.kt`: an object per kind the reader's `ExactForma` reads
/// (`FormaColor`, `FormaLength`, `FormaNumber`), a `const val` per token
/// of it by its `SCREAMING_SNAKE` name, its value the token name, for
/// `ExactForma(view).color(FormaColor.MAP_CARD_PIN)`; in the app's
/// package, `exact.app`, beside its hatches (stage 7 D).
fn kotlin(folding: &exact_forma::Folding, source: &str) -> String {
    let tokens = tokens(folding);
    let mut out = format!(
        "// @generated by `contract forma gen kotlin` from {source}; do not edit, never committed.\n\
         // A token is its name: `ExactForma(view).color(FormaColor.SURFACE_CARD)` resolves it (LLP 1121 D12).\n\
         package exact.app\n"
    );
    for (kind, object) in [
        ("color", "FormaColor"),
        ("length", "FormaLength"),
        ("number", "FormaNumber"),
    ] {
        let _ = write!(out, "\nobject {object} {{\n");
        for t in tokens.iter().filter(|t| t.kind == kind) {
            let name = serde_json::to_string(&t.name).expect("a string is JSON");
            let _ = writeln!(
                out,
                "    /** `{}`. */\n    const val {} = {name}",
                t.binding,
                screaming(&t.binding)
            );
        }
        out.push_str("}\n");
    }
    out
}

/// Refuse a design whose names collide after any mapping: each pair named.
fn admitted(folding: &exact_forma::Folding) -> Result<(), String> {
    let tokens = tokens(folding);
    let names: Vec<&str> = tokens.iter().map(|t| t.binding.as_str()).collect();
    let mut refused = Vec::new();
    // What no constant can be named: one with no letter, or that starts
    // with a digit.
    for name in &names {
        let mapped = camel(name);
        // `__proto__` in an object literal sets its prototype, no property.
        if !mapped.contains(|c: char| c.is_ascii_alphabetic())
            || mapped.starts_with(|c: char| c.is_ascii_digit())
            || mapped == "__proto__"
        {
            refused.push(format!("`{name}` is `{mapped}` in camelCase, which no binding can name: rename it (LLP 1121 D12)"));
        }
    }
    for (scheme, map) in [
        ("camelCase", camel as fn(&str) -> String),
        ("SCREAMING_SNAKE", screaming),
    ] {
        for (a, b) in collisions(&names, map) {
            refused.push(match a == b {
                // Two components of one name, in two files, each with the param.
                true => format!("`{a}` is a param of two components named alike (in two files), both `{}` in {scheme}: rename one component (LLP 1121 D12)", map(&a)),
                false => format!(
                    "`{a}` and `{b}` are both `{}` in {scheme}: rename one (LLP 1121 D12)",
                    map(&a)
                ),
            });
        }
    }
    match refused.is_empty() {
        true => Ok(()),
        false => Err(refused.join("\n")),
    }
}

/// Generate `ts`, `rust`, `swift` or `kotlin` bindings for a design. The
/// caller checks the app before publishing these files. Diagnostics name
/// the design or the component whose parameter failed validation.
pub fn files(
    target: &str,
    path: &Path,
    text: &str,
    params: &crate::forma_params::AppParams,
) -> Result<Vec<(&'static str, String)>, Vec<String>> {
    let folding = exact_forma::parse(text)
        .and_then(|design| {
            exact_forma::check_design(
                crate::forma_params::laid(&design, params, None),
                &contract_lower::forma_vocab(),
            )
        })
        .map_err(|all| {
            all.iter()
                .map(|d| crate::forma_params::located(d, params, path))
                .collect::<Vec<_>>()
        })?;
    admitted(&folding).map_err(|why| vec![format!("{}: {why}", path.display())])?;
    let source = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
    Ok(match target {
        "ts" => vec![
            ("forma.ts", typescript(&folding, &source)),
            ("forma.css", css(&folding)),
        ],
        "swift" => vec![("forma.swift", swift(&folding, &source))],
        "kotlin" => vec![("FormaTokens.kt", kotlin(&folding, &source))],
        "rust" => vec![("forma.rs", rust(&folding, &source))],
        _ => return Err(vec![format!("unknown Forma binding language: {target}")]),
    })
}

/// Project from the loaded source graph before lowering consumes it. A missing
/// design or a binding collision leaves generation to the CLI, which reports
/// the diagnostic if the host requests names.
pub(crate) fn swift_names(file: &contract_syntax::File) -> Option<String> {
    let design = file.design.as_ref()?;
    let params = crate::forma_params::AppParams {
        tokens: contract_lower::params::tokens(file),
        identities: contract_lower::params::identities(file),
        packages: file.package_designs.clone(),
        ..Default::default()
    };
    let bindings = files("swift", Path::new(&design.path), &design.text, &params).ok()?;
    bindings.into_iter().next().map(|(_, text)| text)
}

/// Like [`crate::compile_path_all`], with optional Swift bindings derived from
/// the same inputs. No bindings escape an unsuccessful compile.
pub fn compile_path_swift(
    path: &Path,
    mapped: bool,
) -> Result<(crate::Plan, Option<crate::SourceMap>, Option<String>), Vec<crate::CompileError>> {
    let src = crate::read_source(path).map_err(|e| vec![e])?;
    let mut swift = None;
    let (plan, map) = crate::compile_path_warned(
        path,
        &src,
        mapped,
        true,
        contract_lower::Profile::Web,
        None,
        None,
        Some(&mut swift),
    )?;
    Ok((plan, map, swift))
}
