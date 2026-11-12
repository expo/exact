//! The packages the app's TypeScript data code imports, whose module
//! declarations are in the plan as a Contract `use` would put them (LLP 1133
//! D6 rule 1: a data-code call of a declared module is admitted as a
//! Contract read is, which reads the module's rows). The walk is the
//! TypeScript half of the bake's `usedPackages` (`js/bake/src/typescript.mjs`),
//! so the plan names no package the bake does not link.

use super::{Loader, Package};
use crate::resolve::{lexical, resolve, Origin};
use contract_syntax::Span;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

impl Loader<'_> {
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
        let app_root = self.app_root;
        for name in imported(app_root) {
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
            let described = format!("`{name}`, which the app's data code imports");
            let span = Span {
                line: 1,
                ..Span::default()
            };
            let unit = self.load_resolved(resolved, &described, span)?;
            for m in &mut self.units[unit].file.modules {
                m.imported = true;
            }
        }
        Ok(())
    }
}

/// The package names TypeScript reached from `app.ts` through relative
/// imports, in name order: a bare specifier's package (`@scope/name`
/// or `name`), never `exact:` or `node:`, and not a type-only import's.
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
        for spec in specifiers(&uncommented(&text)) {
            if spec.starts_with('.') {
                next.extend(ts_file(&lexical(&dir.join(&spec))));
            } else if !spec.starts_with("exact:") && !spec.starts_with("node:") {
                let parts: Vec<&str> = spec.split('/').collect();
                let take = if spec.starts_with('@') { 2 } else { 1 };
                names.insert(parts[..take.min(parts.len())].join("/"));
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

/// `text` without its comments, its string and template literals kept whole:
/// a block comment, and a line comment whose `//` follows no `:`, quote or
/// backslash. A comment marker inside a literal is the literal's text.
fn uncommented(text: &str) -> String {
    let code = text.as_bytes();
    let mut out = Vec::with_capacity(code.len());
    let mut at = 0;
    while at < code.len() {
        if let Some(end) = literal(code, at) {
            out.extend_from_slice(&code[at..end]);
            at = end;
        } else if code[at..].starts_with(b"/*") {
            match code[at + 2..].windows(2).position(|w| w == b"*/") {
                Some(end) => at += 2 + end + 2,
                None => {
                    // An unclosed `/*` is no comment: kept, as the bake keeps it.
                    out.extend_from_slice(b"/*");
                    at += 2;
                }
            }
        } else if code[at..].starts_with(b"//")
            && (at == 0 || !matches!(code[at - 1], b':' | b'\'' | b'"' | b'\\'))
        {
            at += code[at..]
                .iter()
                .position(|&b| b == b'\n')
                .unwrap_or(code.len() - at);
        } else {
            out.push(code[at]);
            at += 1;
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The end of the string or template literal that starts at `at`. A quote
/// with no match on its line starts none; a template literal may span lines
/// and nest literals in its `${…}`.
fn literal(code: &[u8], at: usize) -> Option<usize> {
    let quote = *code.get(at).filter(|q| matches!(q, b'\'' | b'"' | b'`'))?;
    let mut k = at + 1;
    while let Some(&b) = code.get(k) {
        match b {
            b'\\' => k += 2,
            b'\n' if quote != b'`' => return None,
            b'$' if quote == b'`' && code.get(k + 1) == Some(&b'{') => {
                k = substitution(code, k + 2)?;
            }
            _ if b == quote => return Some(k + 1),
            _ => k += 1,
        }
    }
    None
}

/// The end of a template literal's `${…}` whose expression starts at `k`.
fn substitution(code: &[u8], mut k: usize) -> Option<usize> {
    let mut depth = 1;
    while let Some(&b) = code.get(k) {
        if let Some(end) = literal(code, k) {
            k = end;
            continue;
        }
        match b {
            b'{' => depth += 1,
            b'}' => {
                depth -= 1;
                if depth == 0 {
                    return Some(k + 1);
                }
            }
            _ => {}
        }
        k += 1;
    }
    None
}

fn word(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

/// Whether `code` has `keyword` at `at` after a word boundary.
fn keyword(code: &[u8], at: usize, keyword: &str) -> bool {
    code[at..].starts_with(keyword.as_bytes()) && (at == 0 || !word(code[at - 1]))
}

/// The end of the quoted specifier at `at` and the specifier: a quote, one
/// or more characters that are neither quote nor newline, the same quote.
fn quoted(code: &[u8], at: usize) -> Option<(usize, String)> {
    let quote = *code.get(at).filter(|q| matches!(q, b'\'' | b'"'))?;
    let len = code[at + 1..]
        .iter()
        .position(|&b| matches!(b, b'\'' | b'"' | b'\n'))?;
    (len > 0 && code[at + 1 + len] == quote).then(|| {
        (
            at + len + 2,
            String::from_utf8_lossy(&code[at + 1..at + 1 + len]).into_owned(),
        )
    })
}

fn spaces(code: &[u8], mut at: usize) -> usize {
    while code.get(at).is_some_and(u8::is_ascii_whitespace) {
        at += 1;
    }
    at
}

/// The specifiers `code` imports and re-exports from, as the bake's
/// pattern reads them: `import … from "x"` and `export … from "x"` (not
/// `import type` or `export type`), `import "x"` and `import("x")`. Text
/// inside a string or template literal is no import.
fn specifiers(code: &str) -> Vec<String> {
    let code = code.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < code.len() {
        if let Some(end) = literal(code, at) {
            at = end;
            continue;
        }
        match statement(code, at).or_else(|| bare_import(code, at)) {
            Some((end, spec)) => {
                out.extend(spec);
                at = end;
            }
            None => at += 1,
        }
    }
    out
}

/// `import … from "x"` or `export … from "x"` at `at`: its end, and its
/// specifier unless the statement is type-only.
fn statement(code: &[u8], at: usize) -> Option<(usize, Option<String>)> {
    if !keyword(code, at, "import") && !keyword(code, at, "export") {
        return None;
    }
    let mut k = at + "import".len();
    let after = spaces(code, k);
    let typed = after > k
        && code[after..].starts_with(b"type")
        && !code.get(after + 4).copied().is_some_and(word);
    if typed {
        k = after + 4;
    }
    loop {
        if keyword(code, k, "from") {
            if let Some((end, spec)) = quoted(code, spaces(code, k + 4)) {
                return Some((end, (!typed).then_some(spec)));
            }
        }
        match code.get(k) {
            None | Some(b'\'' | b'"' | b';') => return None,
            Some(_) => k += 1,
        }
    }
}

/// `import "x"` or `import("x")` at `at`.
fn bare_import(code: &[u8], at: usize) -> Option<(usize, Option<String>)> {
    if !keyword(code, at, "import") {
        return None;
    }
    let mut k = spaces(code, at + "import".len());
    if code.get(k) == Some(&b'(') {
        k = spaces(code, k + 1);
    }
    quoted(code, k).map(|(end, spec)| (end, Some(spec)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_are_read_as_the_bake_reads_them() {
        let code = uncommented(
            "import { a } from '@exact/notifications';\n\
             import type { B } from '@exact/clipboard';\n\
             import { type C } from \"zod/v4\";\n\
             export * from './local.js';\n\
             import './side';\n\
             const later = await import(\"lazy-pkg\");\n\
             // import { d } from 'commented';\n\
             /* import { e } from 'blocked'; */\n\
             const url = 'https://example.com'; import x from 'after-url';\n",
        );
        assert_eq!(
            specifiers(&code),
            [
                "@exact/notifications",
                "zod/v4",
                "./local.js",
                "./side",
                "lazy-pkg",
                "after-url"
            ]
        );
    }

    #[test]
    fn an_import_quoted_in_a_literal_is_not_read() {
        let code = uncommented(
            "export const sample = \"import { open } from '@exact/hid';\";\n\
             const escaped = 'it\\'s import \"quoted-pkg\"';\n\
             const page = `import { a } from 'templated';\n${`import 'nested'`} // kept`;\n\
             const glob = 'src/**/*.ts'; import real from 'after-glob'; /* gone */\n\
             const url = \"https://example.com\"; import('after-string');\n",
        );
        assert_eq!(specifiers(&code), ["after-glob", "after-string"]);
        assert!(code.contains("// kept") && !code.contains("gone"));
    }
}
