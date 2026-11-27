//! `class=` (LLP 1017 P6): a node names a `style`, or chooses between two
//! with `class=(cond ? A : B)`. Either way its rows come in ahead of the
//! node's own attributes, which win for the same name.

use crate::tags::{self, AttrTarget};
use crate::{err, LowerError, Lowerer};
use contract_syntax::state::state_of;
use contract_syntax::{Attr, Expr, Node, Span};
use std::collections::BTreeSet;

impl Lowerer<'_> {
    /// A node's class rows as attributes ([`rows`]), from this file's styles.
    pub(crate) fn class_rows(
        &self,
        attrs: &[Attr],
    ) -> Result<Option<(String, Vec<Attr>)>, LowerError> {
        rows(attrs, &|name| self.styles.get(name).map(Vec::as_slice))
    }
}

/// A node's class rows as attributes, with a label for the source map: a
/// named style's rows as declared; a choice's as one `cond ? A's : B's` per
/// attribute either style sets. A side that does not set a row is `none`,
/// letting any earlier implied row (a tag's fixed style, a link's colour, a
/// grouped sheet) stand unwritten or clearing to the kernel's default.
/// Attributes that write one row under different names (a shorthand and its
/// longhand) are paired row by row, so neither side's unset clears what it
/// set under the other name. Rows the node itself sets are left out.
/// `style` finds a style's rows by name.
pub(crate) fn rows<'s>(
    attrs: &[Attr],
    style: &dyn Fn(&str) -> Option<&'s [Attr]>,
) -> Result<Option<(String, Vec<Attr>)>, LowerError> {
    Ok(rows_with_fallbacks(attrs, style)?.map(|(label, rows, _)| (label, rows)))
}

/// [`rows`], plus the `(name, span)` of every choice row where one style
/// omitted the attribute so an earlier compiler-implied row stays underneath.
pub(crate) fn rows_with_fallbacks<'s>(
    attrs: &[Attr],
    style: &dyn Fn(&str) -> Option<&'s [Attr]>,
) -> Result<Option<(String, Vec<Attr>, BTreeSet<(String, Span)>)>, LowerError> {
    let Some(c) = attrs.iter().find(|a| a.name == "class") else {
        return Ok(None);
    };
    let own = |name: &str| attrs.iter().any(|a| a.name == name);
    let style = |name: &str, span| -> Result<&'s [Attr], LowerError> {
        style(name).ok_or_else(|| LowerError {
            id: "lower-unknown-class",
            message: format!("`class={name}`: no `style {name}` in this file"),
            span,
        })
    };
    match &c.value {
        Expr::Ident(name, _) => {
            let rows = style(name, c.span)?
                .iter()
                .filter(|s| !own(&s.name))
                .cloned()
                .collect();
            Ok(Some((name.clone(), rows, BTreeSet::new())))
        }
        Expr::Ternary(cond, yes, no, span) => {
            let (Expr::Ident(a, _), Expr::Ident(b, _)) = (&**yes, &**no) else {
                return err(
                    "lower-class-name",
                    "`class=(cond ? A : B)` chooses between two styles by name",
                    c.span,
                );
            };
            let (sa, sb) = (style(a, yes.span())?, style(b, no.span())?);
            let both: Vec<&Attr> = sa.iter().chain(sb).collect();
            let (sa, sb) = (by_row(sa, &both), by_row(sb, &both));
            let mut rows: Vec<Attr> = Vec::new();
            let mut fallbacks: BTreeSet<(String, Span)> = BTreeSet::new();
            for s in sa.iter().chain(&sb) {
                if own(&s.name) || rows.iter().any(|r| r.name == s.name) {
                    continue;
                }
                let side = |style: &[Attr]| {
                    style
                        .iter()
                        .rfind(|r| r.name == s.name)
                        .map(|r| r.value.clone())
                };
                let (oa, ob) = (side(&sa), side(&sb));
                let omitted = oa.is_none() || ob.is_none();
                let va = oa.unwrap_or(Expr::None(*span));
                let vb = ob.unwrap_or(Expr::None(*span));
                // @ref LLP 1069.011 D12 — a prop has no `none` to fall to.
                let styleable = matches!(crate::tags::attr(&s.name), Some(crate::tags::AttrTarget::Prop(p)) if p.styleable());
                if styleable && (matches!(va, Expr::None(_)) || matches!(vb, Expr::None(_))) {
                    return err(
                        "lower-style-prop",
                        format!("`class=(cond ? {a} : {b})`: `{}` is set by one style and not the other; set it in both or neither", s.name),
                        s.span,
                    );
                }
                // The same literal on both sides is that literal: what lets
                // a shared `font-family` be chosen.
                let value = match (&va, &vb) {
                    (Expr::Str(x, _), Expr::Str(y, _)) if x == y => va.clone(),
                    (Expr::Number(x, _), Expr::Number(y, _)) if x == y => va.clone(),
                    _ => Expr::Ternary(cond.clone(), Box::new(va), Box::new(vb), *span),
                };
                if omitted && s.written {
                    fallbacks.insert((s.name.clone(), s.span));
                }
                rows.push(Attr {
                    value,
                    ..s.clone()
                });
            }
            Ok(Some((format!("{a}/{b}"), rows, fallbacks)))
        }
        _ => err(
            "lower-class-name",
            "`class=` names a style declared with `style Name`, or chooses between two: `class=(cond ? A : B)`",
            c.span,
        ),
    }
}

/// `style`'s attributes in a class choice, `both` the two styles'
/// attributes together: each that writes a row another name among `both`
/// writes in the same state layer, as one attribute a row ([`longhands`]).
fn by_row(style: &[Attr], both: &[&Attr]) -> Vec<Attr> {
    let layer = |a: &Attr| state_of(&a.name).map(|(s, _)| s.to_string());
    style
        .iter()
        .flat_map(|a| {
            let ids = crate::anatomy::rows_of(a);
            let shared = both.iter().any(|o| {
                o.name != a.name
                    && layer(o) == layer(a)
                    && crate::anatomy::rows_of(o).iter().any(|id| ids.contains(id))
            });
            shared
                .then(|| longhands(a))
                .flatten()
                .unwrap_or_else(|| vec![a.clone()])
        })
        .collect()
}

/// `a` as one attribute per row it writes, each named by the row's own
/// longhand and valued as `a`'s lowering projects it onto that row. `None`
/// where a row has no longhand, for `font` (whose shorthand also marks the
/// node), and for a computed value of a box shorthand, which only its own
/// lowering splits (`shorthands::computed_list`).
fn longhands(a: &Attr) -> Option<Vec<Attr>> {
    fn literal(e: &Expr) -> bool {
        match e {
            Expr::Str(..) | Expr::Number(..) | Expr::None(_) => true,
            Expr::Ternary(_, yes, no, _) => literal(yes) && literal(no),
            _ => false,
        }
    }
    let (state, name) = match state_of(&a.name) {
        Some((s, row)) => (Some(s), row),
        None => (None, a.name.as_str()),
    };
    let ids = crate::anatomy::rows_of(a);
    let values: Vec<Expr> = match tags::attr_valued(name, &a.value)? {
        AttrTarget::Styles([_]) => vec![a.value.clone()],
        AttrTarget::Styles(rows) if crate::values::four_sided(rows) => {
            match crate::shorthands::sides(name, &a.value).ok()? {
                Some(sides) => sides,
                None if literal(&a.value) => vec![a.value.clone(); rows.len()],
                None => return None,
            }
        }
        AttrTarget::Shorthand if name != "font" => (0..ids.len())
            .map(|i| crate::shorthands::component(&a.value, name, i).ok())
            .collect::<Option<_>>()?,
        AttrTarget::Flex => (0..ids.len())
            .map(|i| crate::values::flex_component(&a.value, i).ok())
            .collect::<Option<_>>()?,
        _ => return None,
    };
    ids.iter()
        .zip(values)
        .map(|(id, value)| {
            let longhand = crate::style_names::STYLE_NAMES
                .iter()
                .find(|(_, _, t)| matches!(t, AttrTarget::Styles(rows) if *rows == [*id]))?
                .0;
            let name = match state {
                Some(s) => contract_syntax::state::state_attr(&format!(":{s}"), longhand),
                None => longhand.to_string(),
            };
            Some(Attr {
                name,
                value,
                ..a.clone()
            })
        })
        .collect()
}

impl Lowerer<'_> {
    /// Whether an absolutely positioned box can be among `nodes` or under
    /// them (LLP 1074 T1): a `position` that is not a literal `static` or
    /// `relative`, its own or its class's; a component, a slot or a native
    /// view, whose insides aren't seen here; a canvas; a row that exits or
    /// moves in its layout (the web host takes a leaving row out of flow). A
    /// box that clips or transforms with none of these under it is the
    /// containing block of nothing, so it needs no `position: relative`.
    pub(crate) fn may_hold_absolute(&self, nodes: &[Node]) -> bool {
        nodes.iter().any(|n| match n {
            Node::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                let class = self.class_rows(attrs).ok().flatten().map(|(_, rows)| rows);
                let rows = attrs.iter().chain(class.iter().flatten());
                let positioned = |a: &Attr| {
                    a.name == "position"
                        && !matches!(&a.value, Expr::Str(v, _) if v == "static" || v == "relative" || v == "sticky")
                };
                let moves = |a: &Attr| {
                    matches!(
                        a.name.as_str(),
                        "-exact-exit-animation" | "-exact-layout-transition"
                    )
                };
                rows.clone().any(|a| positioned(a) || moves(a))
                    || crate::tags::tag(tag).is_none()
                    || tag == "canvas"
                    || self.may_hold_absolute(children)
            }
            Node::Use { .. } | Node::Children { .. } => true,
            node => node
                .child_lists()
                .into_iter()
                .any(|list| self.may_hold_absolute(list)),
        })
    }
}
