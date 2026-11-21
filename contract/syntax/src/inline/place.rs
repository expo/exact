//! The static steps leading to a component use (LLP 1029.000.002 D1).

use super::{subst_expr, Attr, Expr, Node, Subst};
use std::collections::BTreeMap;
use std::fmt::Write;

pub(super) fn root(component: &str) -> String {
    let mut path = String::new();
    step(&mut path, 'c', component, 0);
    path
}

fn step(path: &mut String, kind: char, name: &str, ordinal: u32) {
    // The length keeps author-supplied ids distinct even when they contain
    // the same delimiters as the surrounding steps.
    write!(path, "{kind}{}:{name}:{ordinal}/", name.len()).unwrap();
}

fn literal_id(attrs: &[Attr], subst: &mut Subst<'_, Expr>) -> Option<String> {
    let attr = attrs.iter().find(|attr| attr.name == "id")?;
    let mut value = subst_expr(&attr.value, subst);
    while let Expr::Typed(inner, _, _) = value {
        value = *inner;
    }
    match value {
        Expr::Str(id, _) => Some(id),
        _ => None,
    }
}

/// One source step between a rendered parent (or row) and this site.
/// Only component and slot boundaries can precede it on the same edge.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub struct SourceStep {
    /// The preceding boundary on this edge.
    pub outer: Option<u32>,
    /// What the author wrote.
    pub kind: SourceKind,
    /// The position among source siblings of this kind and name.
    pub ordinal: u32,
    /// Another source sibling declares the same literal id, even in a hidden arm.
    pub shared_id: bool,
}

/// What contributes a step to a rendered place (LLP 1029.000.002 D1).
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum SourceKind {
    /// An authored tag, before lowering selects its kernel node type.
    Element(String),
    /// A component use, including the entry component.
    Component(String),
    /// The loop that owns a row's runtime key.
    Each(String),
    /// Where a slot component renders its fill.
    Children,
}

impl SourceKind {
    fn parts(&self) -> (char, &str) {
        match self {
            Self::Element(tag) => ('e', tag),
            Self::Component(name) => ('c', name),
            Self::Each(key) => ('f', key),
            Self::Children => ('s', ""),
        }
    }
}

#[derive(Default)]
pub(super) struct Steps(BTreeMap<SourceStep, u32>);

impl Steps {
    pub(super) fn intern(&mut self, step: SourceStep) -> u32 {
        let next = self.0.len() as u32;
        *self.0.entry(step).or_insert(next)
    }

    pub(super) fn into_rows(self) -> Vec<SourceStep> {
        let mut rows: Vec<_> = self.0.into_iter().collect();
        rows.sort_unstable_by_key(|(_, index)| *index);
        rows.into_iter().map(|(step, _)| step).collect()
    }
}

#[derive(Default)]
pub(super) struct Siblings {
    ordinals: BTreeMap<SourceKind, u32>,
    ids: BTreeMap<String, usize>,
}

impl Siblings {
    pub(super) fn for_nodes(nodes: &[Node], subst: &mut Subst<'_, Expr>) -> Self {
        let mut siblings = Self::default();
        let branches = Node::walk_with(nodes, (), |node, _| {
            let transparent = matches!(node, Node::When { .. } | Node::Match { .. });
            [transparent.then_some(()); 2]
        });
        for (node, _) in branches {
            if let Node::Element { attrs, .. } = node {
                if let Some(id) = literal_id(attrs, subst) {
                    *siblings.ids.entry(id).or_default() += 1;
                }
            }
        }
        siblings
    }

    pub(super) fn append_step(
        &mut self,
        path: &mut String,
        node: &Node,
        subst: &mut Subst<'_, Expr>,
        outer: Option<u32>,
        source_keys: &BTreeMap<crate::Span, String>,
    ) -> Option<SourceStep> {
        let kind = match node {
            Node::Element { tag, .. } => SourceKind::Element(tag.clone()),
            Node::Use { name, .. } => SourceKind::Component(name.clone()),
            Node::Each { span, .. } => SourceKind::Each(source_keys[span].clone()),
            Node::Children { .. } => SourceKind::Children,
            Node::When { .. } | Node::Match { .. } => return None,
        };
        // Every sibling loop counts even when it computes a different key.
        let counted = match &kind {
            SourceKind::Each(_) => SourceKind::Each(String::new()),
            other => other.clone(),
        };
        let next = self.ordinals.entry(counted).or_default();
        let ordinal = *next;
        *next += 1;
        let id = match node {
            Node::Element { attrs, .. } => literal_id(attrs, subst),
            _ => None,
        };
        let shared_id = id.as_ref().is_some_and(|id| self.ids.get(id) != Some(&1));
        if let Some(id) = id.filter(|_| !shared_id) {
            step(path, 'i', &id, 0);
        } else {
            let (letter, name) = kind.parts();
            step(path, letter, name, ordinal);
        }
        Some(SourceStep {
            outer,
            kind,
            ordinal,
            shared_id,
        })
    }
}
