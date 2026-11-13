//! `lower-row-lookup` (LLP 1116.016 D4): a `for` over a list of records from
//! the app's data whose rows each search another data list of records
//! (`users.filter(u => u.id == r.userId)`, a nested `for`). The list
//! bound is per evaluation of one binding (LLP 1017.003 D3), so each row stays
//! under it while an update does rows × items steps: measured on a Mac's
//! runner, 1,000 rows over 1,000 users took about 100 ms an update, 5,000 over
//! 5,000 about 2 s; the same rows joined in the data module, 16 and 44 ms.
//!
//! A row's own lists (`r.tags.filter(…)`) are not another list; a `for`
//! over strings (one section per category, filtering items by it) is
//! grouping by a short key, and a list of strings is most often a short set
//! of choices or ids: neither is named. A `derive` of the join is no repair:
//! it is one evaluation, and passes the bound at 1,000 × 1,000.

use crate::LowerError;
use contract_syntax::inline::calls::shown;
use contract_syntax::{Expr, Node, Span};
use contract_types::{infer, Checked, Ref, Scope, Shapes, Ty};
use std::collections::BTreeSet;

#[path = "own_list.rs"]
mod own_list;

/// The roster's list work that walks its receiver.
const WALKS: &[&str] = &[
    "filter", "map", "includes", "indexOf", "join", "concat", "slice", "sortBy",
];

/// `data.users`, `favorites`: the receiver as written, when it is a name or
/// a field of one, with the name it starts from.
fn path(e: &Expr) -> Option<(String, &str)> {
    match e {
        Expr::Ident(n, _) => Some((n.clone(), n.as_str())),
        Expr::Member(inner, field, _) => {
            path(inner).map(|(p, root)| (format!("{p}.{field}"), root))
        }
        _ => None,
    }
}

/// Whether `root` names the app's data: a state, derive, resource or prop,
/// not a row, a binding or a parameter.
fn data(root: &str, scope: &Scope) -> bool {
    matches!(
        scope.lookup(root),
        Some((
            Ref::Slot(_) | Ref::Derive(_) | Ref::Resource(_) | Ref::Prop(_),
            _
        ))
    )
}

/// Whether `e` is a list of records: a table, where a list of strings is
/// most often a short set of choices or ids.
fn table(e: &Expr, scope: &Scope, shapes: &Shapes) -> bool {
    matches!(infer(e, scope, shapes), Ok(Ty::List(t)) if matches!(*t, Ty::Record(_)))
}

struct Rows<'a> {
    shapes: &'a Shapes,
    /// The file's own shapes: a router's tabs and stack are records too, but
    /// each of their rows is a screen.
    declared: BTreeSet<&'a str>,
    seen: BTreeSet<(u32, u32, u32, u32)>,
    out: Vec<LowerError>,
}

impl Rows<'_> {
    fn warn(&mut self, outer: &str, inner: &str, how: &str, span: Span) {
        if !self
            .seen
            .insert((span.source_id, span.line, span.col, span.end_col))
        {
            return;
        }
        self.out.push(LowerError {
            id: "lower-row-lookup",
            message: format!(
                "each row of `{outer}` searches `{inner}` ({how}), so an update walks every row times every item (1,000 × 1,000 is about 100 ms on a Mac): join it in the data module, giving each row the field it reads"
            ),
            span,
        });
    }

    /// The other data lists `e` walks in a row of `outer`.
    fn expr(&mut self, e: &Expr, scope: &Scope, outer: &str) {
        if let Expr::Call(name, args, span) = e {
            if let (true, Some((inner, root))) =
                (WALKS.contains(&name.as_str()), args.first().and_then(path))
            {
                if data(root, scope) && inner != outer && table(&args[0], scope, self.shapes) {
                    let how = if contract_syntax::idioms::is_method(name) {
                        format!("`.{name}`")
                    } else {
                        format!("`{name}(…)`")
                    };
                    self.warn(outer, &inner, &how, *span);
                }
            }
        }
        for x in e.operands() {
            self.expr(x, scope, outer);
        }
    }

    /// `outer` is the data list the innermost enclosing row iterates, if any.
    fn nodes(&mut self, nodes: &[Node], scope: &Scope, outer: Option<&str>) {
        for node in nodes {
            match node {
                Node::Element {
                    positional,
                    attrs,
                    children,
                    ..
                } => {
                    if let Some(outer) = outer {
                        for e in positional.iter().chain(attrs.iter().map(|a| &a.value)) {
                            self.expr(e, scope, outer);
                        }
                    }
                    self.nodes(children, scope, outer);
                }
                Node::When {
                    cond,
                    then,
                    otherwise,
                    ..
                } => {
                    if let Some(outer) = outer {
                        self.expr(cond, scope, outer);
                    }
                    self.nodes(then, scope, outer);
                    self.nodes(otherwise, scope, outer);
                }
                Node::Each {
                    var,
                    index,
                    list,
                    body,
                    span,
                    ..
                } => {
                    let item = match infer(list, scope, self.shapes) {
                        Ok(Ty::List(t)) => *t,
                        _ => Ty::Unknown,
                    };
                    let records =
                        matches!(&item, Ty::Record(n) if self.declared.contains(n.as_str()));
                    let listed = path(list).filter(|(_, root)| data(root, scope));
                    if let (Some(outer), Some((inner, _)), true) = (outer, &listed, records) {
                        if inner != outer {
                            self.warn(outer, inner, "a nested `for`", *span);
                        }
                    }
                    let mut inner = scope.clone();
                    inner.push_each(var, index.as_deref(), item);
                    let rows = listed.filter(|_| records).map(|(p, _)| p);
                    self.nodes(body, &inner, rows.as_deref().or(outer));
                }
                Node::Match {
                    subject,
                    some,
                    none,
                    ..
                } => {
                    if let Some(outer) = outer {
                        self.expr(subject, scope, outer);
                    }
                    let bound = match infer(subject, scope, self.shapes) {
                        Ok(Ty::Option(t)) => *t,
                        _ => Ty::Unknown,
                    };
                    let mut inner = scope.clone();
                    inner.push_region(Some((some.0.clone(), Ref::Bound(0), bound)));
                    self.nodes(&some.1, &inner, outer);
                    let mut none_scope = scope.clone();
                    none_scope.push_region(None);
                    self.nodes(none, &none_scope, outer);
                }
                Node::Use { args, children, .. } => {
                    if let Some(outer) = outer {
                        for a in args {
                            self.expr(&a.value, scope, outer);
                        }
                    }
                    self.nodes(children, scope, outer);
                }
                Node::Children { .. } => {}
            }
        }
    }
}

/// `lower-row-lookup`'s sites in the expanded root's view, then
/// `lower-row-own-list`'s and `lower-key-position`'s ([`key_position`]).
pub(super) fn row_lookup(checked: &Checked<'_>) -> Vec<LowerError> {
    let root = &checked.expanded.root;
    let keys = key_position(checked);
    let Some(types) = checked.types.components.first() else {
        return keys;
    };
    let mut rows = Rows {
        shapes: &checked.types.shapes,
        declared: checked
            .file
            .shapes
            .iter()
            .map(|s| s.name.as_str())
            .collect(),
        seen: BTreeSet::new(),
        out: Vec::new(),
    };
    let scope = checked.types.component_scope(root, types);
    rows.nodes(&root.view, &scope, None);
    rows.out.extend(own_list::own_list(checked));
    rows.out.extend(keys);
    rows.out
}

/// Whether `e` reads `name` where it is not rebound (an arrow's parameter, a
/// `let`, a `match` arm shadow it).
fn reads(e: &Expr, name: &str) -> bool {
    match e {
        Expr::Ident(n, _) => n == name,
        _ => e
            .scoped_operands()
            .any(|(x, binds)| !binds.contains(name) && reads(x, name)),
    }
}

/// `lower-key-position` (LLP 1142 D6, as LLP 1156.005 D2 narrows it): a
/// `for` whose `key=` reads its position variable, over rows that hold
/// something ([`holds`]). After an insertion or a removal before them, rows
/// keep a key that now names another record: what an instance holds goes with
/// the position, not the record. A row that holds nothing (a waveform's bars,
/// text over plain values) has nothing to follow, and its position may be all
/// the identity its items have.
fn key_position(checked: &Checked<'_>) -> Vec<LowerError> {
    let held = Held::new(checked);
    let walk = |nodes: &[Node], out: &mut Vec<LowerError>| held.walk(nodes, out);
    let mut out = Vec::new();
    walk(&checked.expanded.root.view, &mut out);
    out
}

/// What makes a row's identity matter: state the row owns, a control
/// that keeps focus or its own value, motion, a scroll offset. Conservative:
/// a tag or a component it cannot see into holds.
struct Held<'c> {
    /// The tags of the regions owning state (`Owner::Arm`).
    owning: BTreeSet<u32>,
    /// The `style` declarations whose rows hold.
    styles: BTreeSet<&'c str>,
}

/// Elements that keep nothing of their own: layout, text, images, vector
/// drawing. Every other tag holds.
const PLAIN: &[&str] = &[
    "view",
    "box",
    "column",
    "row",
    "hr",
    "main",
    "header",
    "nav",
    "section",
    "footer",
    "article",
    "aside",
    "time",
    "text",
    "span",
    "p",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "strong",
    "em",
    "b",
    "i",
    "u",
    "s",
    "small",
    "code",
    "mark",
    "sub",
    "sup",
    "br",
    "image",
    "img",
    "figure",
    "figcaption",
    "svg",
    "g",
    "path",
    "polyline",
    "polygon",
    "circle",
    "ellipse",
    "line",
    "rect",
    "defs",
    "linearGradient",
    "radialGradient",
    "stop",
    "clipPath",
    "tspan",
];

/// An attribute that gives its element something to keep: a handler, a
/// Tab stop, editing, motion (CSS's, and Exact's `-exact-` motion rows such
/// as `-exact-layout-transition` and `-exact-symbol-effect`), a scroll
/// offset.
fn holding_attr(name: &str) -> bool {
    let name = contract_syntax::state::state_of(name).map_or(name, |(_, row)| row);
    let name = name.strip_prefix("-exact-").unwrap_or(name);
    exact_plan::EventKind::from_name(name).is_some()
        || matches!(
            name,
            "tabindex" | "contenteditable" | "autofocus" | "popover"
        )
        || name.contains("transition")
        || name.contains("animation")
        || name.ends_with("effect")
        || name.starts_with("overflow")
        || name.starts_with("scroll-snap")
}

impl<'c> Held<'c> {
    fn new(checked: &'c Checked<'_>) -> Self {
        let owning = (checked.expanded.owners.iter())
            .filter_map(|o| match o {
                contract_syntax::Owner::Arm { tag, .. } => Some(*tag),
                _ => None,
            })
            .collect();
        let styles = (checked.file.styles.iter())
            .filter(|s| s.attrs.iter().any(|a| holding_attr(&a.name)))
            .map(|s| s.name.as_str())
            .collect();
        Held { owning, styles }
    }

    /// Whether any of `nodes` holds something.
    fn holds(&self, nodes: &[Node]) -> bool {
        nodes.iter().any(|node| match node {
            Node::Element {
                tag,
                attrs,
                children,
                ..
            } => {
                !PLAIN.contains(&tag.as_str())
                    || attrs.iter().any(|a| {
                        holding_attr(&a.name) || a.name == "class" && self.class_holds(&a.value)
                    })
                    || self.holds(children)
            }
            Node::When { tag, .. } | Node::Each { tag, .. } | Node::Match { tag, .. } => {
                self.owning.contains(tag)
                    || node.child_lists().into_iter().any(|list| self.holds(list))
            }
            Node::Use { .. } | Node::Children { .. } => true,
        })
    }

    /// `class=Name` or `class=(cond ? A : B)` naming a style that holds.
    fn class_holds(&self, value: &Expr) -> bool {
        match value {
            Expr::Ident(name, _) => self.styles.contains(name.as_str()),
            Expr::Ternary(_, a, b, _) => self.class_holds(a) || self.class_holds(b),
            _ => !self.styles.is_empty(),
        }
    }

    fn walk(&self, nodes: &[Node], out: &mut Vec<LowerError>) {
        for node in nodes {
            match node {
                Node::Element { children, .. } | Node::Use { children, .. } => {
                    self.walk(children, out)
                }
                Node::Each {
                    tag,
                    index,
                    key,
                    body,
                    ..
                } => {
                    if let Some(i) = index
                        .as_deref()
                        .filter(|i| reads(key, i))
                        .filter(|_| self.owning.contains(tag) || self.holds(body))
                    {
                        let i = shown(i);
                        out.push(LowerError {
                            id: "lower-key-position",
                            message: format!("this key reads `{i}`, the row's position: after a row is inserted or removed before others, their keys name other records, so what a row holds (its state, a field's focus or text, its motion) follows the position; key by something the record keeps, such as its id"),
                            span: key.span(),
                        });
                    }
                    self.walk(body, out);
                }
                node => {
                    for list in node.child_lists() {
                        self.walk(list, out);
                    }
                }
            }
        }
    }
}
