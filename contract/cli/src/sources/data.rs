//! The declarations needed by TypeScript imports and package views. Data
//! imports retain their module rows for admission (LLP 1133 D6 rule 1);
//! a view's rows are retained only if the expanded root places that view.
//! The TypeScript walk matches the bake's `usedPackages`
//! (`js/bake/src/typescript.mjs`).

use super::{Loader, Package};
use crate::resolve::{lexical, resolve, Origin};
use contract_syntax::Span;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Loader<'_> {
    /// Reserve data calls before implicit module declarations take their names.
    pub(super) fn data_sources(&self) -> BTreeSet<String> {
        self.units
            .iter()
            .enumerate()
            .flat_map(|(index, unit)| {
                contract_syntax::scope::sources(&mut unit.file.clone())
                    .into_iter()
                    .filter(move |name| !self.names_module(index, name))
            })
            .collect()
    }

    fn names_module(&self, index: usize, name: &str) -> bool {
        let unit = &self.units[index];
        unit.file.modules.iter().any(|m| m.name == name)
            || unit
                .file
                .uses
                .iter()
                .zip(&unit.targets)
                .any(|(u, &target)| {
                    u.names
                        .iter()
                        .any(|n| n.local() == name && self.names_module(target, &n.name))
                })
    }

    /// Load the declaration of each package `app.ts` reaches an import of,
    /// when the program's root is the app's `app.contract`, and mark its
    /// names `imported`. A package that is not a module declaration (a
    /// library, a component package) is left to the bake.
    pub(super) fn load_data_packages(&mut self) -> Result<(), Vec<crate::CompileError>> {
        if self
            .root_given
            .file_name()
            .is_none_or(|n| n != "app.contract")
        {
            return Ok(());
        }
        self.load_packages(imported(self.app_root), true)
    }

    /// Read view packages' declarations without marking their names used:
    /// lowering decides which views the expanded root actually places.
    pub(super) fn load_view_packages(&mut self) -> Result<(), Vec<crate::CompileError>> {
        let tags: BTreeSet<String> = self
            .units
            .iter()
            .flat_map(|u| contract_lower::module_tags(&u.file))
            .map(|(tag, _)| tag)
            .collect();
        if tags.is_empty() {
            return Ok(());
        }
        let views = crate::package_views::declared(self.app_root)
            .map_err(|e| vec![super::name_error(e.id, e.message, Span::default())])?;
        let packages = views
            .values()
            .filter(|v| tags.contains(&v.tag))
            .map(|v| v.package.clone())
            .collect();
        self.load_packages(packages, false)
    }

    fn load_packages(
        &mut self,
        names: BTreeSet<String>,
        imported: bool,
    ) -> Result<(), Vec<crate::CompileError>> {
        let app_root = self.app_root;
        for name in names {
            let mut consulted = Vec::new();
            let Ok(resolved) = resolve(&name, app_root, &Origin::App, app_root, &mut consulted)
            else {
                continue;
            };
            let Origin::Package {
                version,
                root,
                manifest,
                ..
            } = &resolved.origin
            else {
                continue;
            };
            if resolved.key.file_name().is_none_or(|f| f != "exact.json") {
                continue;
            }
            let package = Package {
                name: name.clone(),
                version: version.clone(),
                root: root.clone(),
                manifest: manifest.clone(),
            };
            if !self.sources.packages.contains(&package) {
                self.sources.packages.push(package);
            }
            self.sources.consulted.extend(consulted);
            let described = if imported {
                format!("`{name}`, which the app's data code imports")
            } else {
                format!("`{name}`, which serves a view the app places")
            };
            let span = Span {
                line: 1,
                ..Span::default()
            };
            let view_only = !imported && !self.cache.contains_key(&resolved.key);
            let unit = self.load_resolved(resolved, &described, span)?;
            for m in &mut self.units[unit].file.modules {
                m.imported |= imported;
                m.view_only |= view_only;
            }
        }
        Ok(())
    }
}

/// The package names TypeScript reached from `app.ts` through relative
/// imports, in name order: a bare specifier (`@scope/name/subpath` or `name`), never `exact:` or `node:`, and not a type-only import's.
fn imported(app: &Path) -> BTreeSet<String> {
    let mut names = BTreeSet::new();
    let mut seen = BTreeSet::new();
    let mut next: Vec<PathBuf> = ts_file(&app.join("app")).into_iter().collect();
    while let Some(file) = next.pop() {
        if !seen.insert(file.clone()) || file.components().any(|c| c.as_os_str() == "node_modules")
        {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&file) else {
            continue;
        };
        let dir = file.parent().unwrap_or(app);
        for spec in specifiers(&text) {
            if spec.starts_with('.') {
                next.extend(ts_file(&lexical(&dir.join(&spec))));
            } else if !spec.starts_with("exact:") && !spec.starts_with("node:") {
                names.insert(spec);
            }
        }
    }
    names
}

/// The file an import of `base` names, as the bake finds it: as written,
/// with a TypeScript or JavaScript extension, as a directory's index, or a
/// `.js` or `.mjs` name for the `.ts` or `.tsx` file it is emitted from.
fn ts_file(base: &Path) -> Option<PathBuf> {
    let text = base.to_string_lossy();
    let mut candidates: Vec<String> = [
        "",
        ".ts",
        ".tsx",
        ".mts",
        ".js",
        ".mjs",
        "/index.ts",
        "/index.js",
    ]
    .iter()
    .map(|e| format!("{text}{e}"))
    .collect();
    let stem = text
        .strip_suffix(".mjs")
        .or_else(|| text.strip_suffix(".js"));
    candidates.extend(
        stem.into_iter()
            .flat_map(|s| [format!("{s}.ts"), format!("{s}.tsx")]),
    );
    candidates
        .into_iter()
        .map(PathBuf::from)
        .find(|p| p.is_file())
}

/// A token of JavaScript as ECMA-262 §12 reads source text, to the degree
/// the import walk needs: a comment is no token, and a string, template or
/// regular expression is one token, so nothing inside one is code.
#[derive(Debug, PartialEq)]
enum Token<'a> {
    /// An identifier or keyword.
    Word(&'a str),
    /// A string literal's text between its quotes, as written.
    Str(&'a str),
    Punct(u8),
    /// A number, a template or a regular expression.
    Other,
}

fn space_at(text: &str, at: usize) -> Option<char> {
    text.get(at..)?
        .chars()
        .next()
        .filter(|&c| c.is_whitespace() || c == '\u{feff}')
}

fn word_byte(text: &str, at: usize) -> bool {
    let b = text.as_bytes()[at];
    b.is_ascii_alphanumeric()
        || matches!(b, b'_' | b'$')
        || (b >= 0x80 && space_at(text, at).is_none())
}

/// After `prev`, whether a `/` begins a regular expression rather than a
/// division (§12, "the InputElementRegExp goal"): after an operator, an
/// opening bracket or a keyword that takes an expression, not after a
/// value. A `}` is read as ending an expression.
fn regex_allowed(prev: Option<&Token>) -> bool {
    match prev {
        None => true,
        Some(Token::Punct(c)) => !matches!(c, b')' | b']' | b'}'),
        Some(Token::Word(w)) => matches!(
            *w,
            "return"
                | "typeof"
                | "instanceof"
                | "in"
                | "of"
                | "new"
                | "delete"
                | "void"
                | "throw"
                | "case"
                | "do"
                | "else"
                | "yield"
                | "await"
        ),
        Some(Token::Str(_) | Token::Other) => false,
    }
}

/// From `at`, inside a template, past its next part: the index after its
/// closing backquote, or after a `${`, where `opened` records the brace
/// depth the substitution's `}` returns to.
fn template(code: &[u8], mut at: usize, opened: &mut Vec<usize>, braces: usize) -> usize {
    while at < code.len() {
        match code[at] {
            b'\\' => at += 2,
            b'`' => return at + 1,
            b'$' if code.get(at + 1) == Some(&b'{') => {
                opened.push(braces);
                return at + 2;
            }
            _ => at += 1,
        }
    }
    at
}

fn tokens(text: &str) -> Vec<Token<'_>> {
    let code = text.as_bytes();
    let mut out = Vec::new();
    // The brace depth at each open template substitution, innermost last.
    let mut opened: Vec<usize> = Vec::new();
    let mut braces = 0;
    let mut at = 0;
    while at < code.len() {
        let c = code[at];
        let next = code.get(at + 1).copied();
        if let Some(space) = space_at(text, at) {
            at += space.len_utf8();
        } else if c == b'/' && next == Some(b'/') {
            at = text[at..]
                .find(['\n', '\r', '\u{2028}', '\u{2029}'])
                .map_or(code.len(), |n| at + n);
        } else if c == b'/' && next == Some(b'*') {
            at = text[at + 2..].find("*/").map_or(code.len(), |n| at + n + 4);
        } else if c == b'\'' || c == b'"' {
            // A string ends at its quote; one left open ends at the line.
            let start = at + 1;
            at = start;
            while at < code.len() && code[at] != c && code[at] != b'\n' {
                at += match &code[at..] {
                    [b'\\', b'\r', b'\n', ..] => 3,
                    [b'\\', ..] => 2,
                    _ => 1,
                };
            }
            at = at.min(code.len());
            out.push(Token::Str(&text[start..at]));
            at += 1;
        } else if c == b'`' {
            at = template(code, at + 1, &mut opened, braces);
            out.push(Token::Other);
        } else if c == b'}' && opened.last() == Some(&braces) {
            opened.pop();
            at = template(code, at + 1, &mut opened, braces);
            out.push(Token::Other);
        } else if c == b'/' && regex_allowed(out.last()) {
            let mut class = false;
            at += 1;
            while at < code.len() && code[at] != b'\n' && (class || code[at] != b'/') {
                match code[at] {
                    b'\\' => at += 1,
                    b'[' => class = true,
                    b']' => class = false,
                    _ => {}
                }
                at += 1;
            }
            at += 1;
            while at < code.len() && word_byte(text, at) {
                at += 1;
            }
            out.push(Token::Other);
        } else if c.is_ascii_digit() {
            while at < code.len() && (word_byte(text, at) || code[at] == b'.') {
                at += 1;
            }
            out.push(Token::Other);
        } else if word_byte(text, at) {
            let start = at;
            while at < code.len() && word_byte(text, at) {
                at += 1;
            }
            out.push(Token::Word(&text[start..at]));
        } else {
            match c {
                b'{' => braces += 1,
                b'}' => braces = braces.saturating_sub(1),
                _ => {}
            }
            out.push(Token::Punct(c));
            at += 1;
        }
    }
    out
}

/// The specifiers `code` imports and re-exports from, read from its tokens
/// as the bake reads them from its syntax tree: `import … from "x"` and
/// `export … from "x"` (not `import type` or `export type`), `import "x"`
/// and `import("x")`.
fn specifiers(code: &str) -> Vec<String> {
    let tokens = tokens(code);
    let mut out = Vec::new();
    for (at, token) in tokens.iter().enumerate() {
        let Token::Word(keyword @ ("import" | "export")) = token else {
            continue;
        };
        // A property of that name (`x.import`) is no statement.
        if at > 0 && tokens[at - 1] == Token::Punct(b'.') {
            continue;
        }
        let mut k = at + 1;
        if *keyword == "import" {
            match (tokens.get(k), tokens.get(k + 1)) {
                (Some(Token::Str(spec)), _)
                | (Some(Token::Punct(b'(')), Some(Token::Str(spec))) => {
                    out.push(spec.to_string());
                    continue;
                }
                (Some(Token::Punct(b'(' | b'.')), _) => continue,
                _ => {}
            }
        }
        // `type` is a default import's name before `from` or `,`.
        if tokens.get(k) == Some(&Token::Word("type"))
            && !matches!(
                tokens.get(k + 1),
                Some(Token::Word("from") | Token::Punct(b','))
            )
        {
            continue;
        }
        // The clause, to its `from "x"`; a `;`, a string that is no name
        // (`{ "a-b" as ab }`) or the next statement's keyword ends one that
        // has none (`export const`).
        while let Some(token) = tokens.get(k) {
            let named =
                tokens[k - 1] == Token::Word("as") || tokens.get(k + 1) == Some(&Token::Word("as"));
            match (token, tokens.get(k + 1)) {
                (Token::Word("from"), Some(Token::Str(spec))) => {
                    out.push(spec.to_string());
                    break;
                }
                (Token::Str(_), _) if !named => break,
                (Token::Punct(b';') | Token::Word("import" | "export"), _) => break,
                _ => k += 1,
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_are_read_as_the_bake_reads_them() {
        let code = "import { a } from '@exact/notifications';\n\
             import type { B } from '@exact/clipboard';\n\
             import { type C } from \"zod/v4\";\n\
             export * from './local.js';\n\
             export type { D } from 'types-only';\n\
             import './side';\n\
             const later = await import(\"lazy-pkg\");\n\
             // import { d } from 'commented';\n\
             /* import { e } from 'blocked'; */\n\
             const url = 'https://example.com'; import x from 'after-url';\n\
             import type from 'named-type';\n\
             import { \"a-b\" as ab } from 'string-name';\n\
             const meta = import.meta.url;\n";
        assert_eq!(
            specifiers(code),
            [
                "@exact/notifications",
                "zod/v4",
                "./local.js",
                "./side",
                "lazy-pkg",
                "after-url",
                "named-type",
                "string-name"
            ]
        );
    }

    /// A string, template or regular expression is no code: what it holds
    /// neither imports nor hides an import (ECMA-262 §12).
    #[test]
    fn a_string_or_a_comment_neither_adds_nor_drops_an_import() {
        let read = |code: &str| specifiers(code);
        assert_eq!(
            read("const note = \"/*\";\nimport { rows } from \"@exact/ledger\";\nconst end = \"*/\";\n"),
            ["@exact/ledger"]
        );
        assert_eq!(
            read("const hint = 'import { takePhoto } from \"@exact/camera\"';\nconst u = \"a//b\"; import { x } from \"@exact/real\";\n"),
            ["@exact/real"]
        );
        assert_eq!(
            read("const t = `import \"tpl\" ${await import('inside')} ${{ a: 1 }.a}`;\nimport y from 'after-template';\n"),
            ["inside", "after-template"]
        );
        assert_eq!(
            read("const r = /import \"rx\"|'/g; const q = a / b / c;\nimport z from 'after-regex';\n"),
            ["after-regex"]
        );
        assert_eq!(
            read("// a plain list, not the exact-camera view\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            read("const s = \"\\\r\nimport { open } from '@exact/hid'\";\n"),
            Vec::<String>::new()
        );
        assert_eq!(
            read("\u{feff}// notes\u{2028}import\u{a0}{ open } from \"@exact/hid\";\n"),
            ["@exact/hid"]
        );
    }
}
