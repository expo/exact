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

/// `text` without its comments: a block comment, and a line comment whose
/// `//` follows no `:`, quote or backslash (a URL's or a string's stays).
fn uncommented(text: &str) -> String {
    let mut blockless = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find("/*") {
        blockless.push_str(&rest[..start]);
        match rest[start + 2..].find("*/") {
            Some(end) => rest = &rest[start + 2 + end + 2..],
            None => {
                // An unclosed `/*` is no comment: kept, as the bake keeps it.
                blockless.push_str(&rest[start..start + 2]);
                rest = &rest[start + 2..];
            }
        }
    }
    blockless.push_str(rest);
    let mut out = String::with_capacity(blockless.len());
    for line in blockless.split_inclusive('\n') {
        let bytes = line.as_bytes();
        let cut = (0..bytes.len().saturating_sub(1)).find(|&i| {
            &bytes[i..i + 2] == b"//"
                && (i == 0 || !matches!(bytes[i - 1], b':' | b'\'' | b'"' | b'\\'))
        });
        match cut {
            Some(i) => {
                out.push_str(&line[..i]);
                if line.ends_with('\n') {
                    out.push('\n');
                }
            }
            None => out.push_str(line),
        }
    }
    out
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
/// `import type` or `export type`), `import "x"` and `import("x")`.
fn specifiers(code: &str) -> Vec<String> {
    let code = code.as_bytes();
    let mut out = Vec::new();
    let mut at = 0;
    while at < code.len() {
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
}
