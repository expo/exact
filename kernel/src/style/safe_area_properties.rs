//! An author's custom properties (@ref LLP 1123 D5; CSS Variables 1), which
//! the safe-area API reads in lengths: `--hero-top` set to
//! `var(--exact-safe-area-inset-top)` captures a node's inset for its
//! subtree, which reads it as `var(--hero-top)`.
//!
//! They have CSS's unregistered semantics, held typed: each value is a
//! length, since only a length reads one, and its `var()`s are substituted
//! where it is declared, so a capture holds the declaring node's inset in
//! points. A reader resolves each property it names at the nearest node at or
//! above it that declares one ([`crate::kernel::owner::properties_at`]). One
//! that reads an undefined property without a fallback, or that is in a
//! dependency cycle, is the guaranteed-invalid value: every `var()` of it
//! takes its fallback.

use super::calc::Node;
use super::{Dimension, StyleId, StyleValue};
use cssparser::{Delimiter, ParseError, Parser, Token};
use std::sync::Arc;

/// Names the host defines (LLP 1081 D5), which no author declares.
pub const HOST_PREFIX: &str = "--exact-";

/// Whether `name` is `--` and an identifier an author may declare.
pub fn is_author_name(name: &str) -> bool {
    name.len() > 2 && name.starts_with("--") && !name.starts_with(HOST_PREFIX)
}

/// One node's declarations in written order, each name with its leading
/// `--`. A later declaration of a name replaces an earlier one.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct SafeAreaProperties(Arc<[(Arc<str>, Dimension)]>);

impl SafeAreaProperties {
    /// `--a: <length>; --b: <length>`, each name an author's and each value
    /// one [`Self::value`] takes; `None` otherwise. No declaration at all is
    /// a node that declares none.
    pub fn parse(css: &str) -> Option<Self> {
        let mut p = Parser::new(css);
        let mut out: Vec<(Arc<str>, Dimension)> = Vec::new();
        loop {
            p.skip_whitespace();
            if p.is_exhausted() {
                break;
            }
            let name: Arc<str> = match p.next() {
                Ok(Token::Ident(name)) if is_author_name(name) => name.as_ref().into(),
                _ => return None,
            };
            p.expect_colon().ok()?;
            let start = p.position();
            p.parse_until_before(Delimiter::Semicolon, |p| {
                while p.next_including_whitespace_and_comments().is_ok() {}
                Ok::<_, ParseError<()>>(())
            })
            .ok()?;
            let value = Self::value(p.slice_from(start))?;
            out.retain(|(n, _)| *n != name);
            out.push((name, value));
            if p.expect_semicolon().is_err() && !p.is_exhausted() {
                return None;
            }
        }
        Some(SafeAreaProperties(out.into()))
    }

    /// `name` as CSS writes it, escaped where it must be.
    pub fn write_name(name: &str, out: &mut String) {
        super::calc::ident(name, out);
    }

    /// One declaration's value: a length a calculation tree holds, px and
    /// the absolute units, `rem`, a percentage, a viewport unit, `env()`
    /// without a fallback, `var()`, or a math function of these; `None`
    /// otherwise. An `env()` with a fallback is held as text a row
    /// substitutes, so a declaration refuses it, and a container unit, which
    /// a value resolved where it is declared does not resolve yet (LLP 1123
    /// D5, D16).
    pub fn value(text: &str) -> Option<Dimension> {
        match length(text)? {
            Dimension::Compare(c) if !c.is_tree() => None,
            d if d.reads_container() => None,
            d => Some(d),
        }
    }

    /// The declarations as [`Self::parse`] reads them.
    pub fn css(&self) -> String {
        let mut out = String::new();
        for (i, (name, value)) in self.iter().enumerate() {
            if i > 0 {
                out.push_str("; ");
            }
            Self::write_name(name, &mut out);
            out.push_str(": ");
            super::calc::length_css(value, &mut out);
        }
        out
    }

    /// Each declaration in the order written, a later one of a name having
    /// replaced an earlier one.
    pub fn iter(&self) -> impl Iterator<Item = (&str, &Dimension)> {
        self.0.iter().map(|(n, v)| (&**n, v))
    }

    /// What this node declares `name` as, where it declares it.
    pub fn get(&self, name: &str) -> Option<&Dimension> {
        self.0.iter().find(|(n, _)| &**n == name).map(|(_, v)| v)
    }
}

/// A declaration's value as a length a calculation holds: a margin's, or
/// a bare `rem`, which a row converts where it is set and a tree holds as a
/// leaf. `em`, a text style and a platform length (`-exact-hairline`)
/// convert at their row too, and a tree has no leaf for them.
fn length(text: &str) -> Option<Dimension> {
    let text = text.trim_matches(['\t', '\n', '\u{c}', '\r', ' ']);
    if let Some((super::relative::Unit::Rem, n)) = super::relative::parse(text) {
        return Some(super::calc::dimension(Node::Value(
            f64::from(n),
            super::calc::Unit::Rem,
        )));
    }
    StyleValue::Text(text.into())
        .dimension(StyleId::MarginTop, false)
        .ok()
}

/// The values of the custom properties one node's lengths read, each with
/// its `var()`s substituted where it is declared; a property that is
/// undeclared or invalid there is absent. A node reads few, so a list
/// holds them, and one that reads none allocates nothing.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Resolved(pub(crate) Vec<(Arc<str>, Node, bool)>);

impl Resolved {
    /// `name`'s value at the node; `None` where it is undeclared or invalid.
    pub fn get(&self, name: &str) -> Option<&Node> {
        self.0
            .iter()
            .find(|(n, ..)| &**n == name)
            .map(|(_, v, _)| v)
    }

    /// Whether `name`'s value read a raw observation where it was declared or in a declaration
    /// it read (LLP 1123 D5, "Provenance"), which its substituted value no longer shows.
    pub fn raw(&self, name: &str) -> bool {
        self.0.iter().any(|(n, _, raw)| &**n == name && *raw)
    }
}

#[cfg(test)]
#[path = "safe_area_properties_tests.rs"]
mod tests;
