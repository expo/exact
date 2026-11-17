//! The one traversal of a view and of an expression: what is directly under a
//! [`Node`] or an [`Expr`], with the names bound over it. A pass that only
//! descends uses these instead of matching every kind, so a new kind is
//! one edit here. Rebuilding a tree still needs its own match.

use crate::ast::{Attr, Expr, Node, TemplatePart};

/// The names a node or an expression binds over one list or operand under
/// it: a `for`'s item and position over its body, a `match`'s `some(x)`
/// over that arm, an arrow's parameters over its body, a `let`'s name
/// over its body. Empty everywhere else.
#[derive(Debug, Clone, Copy, Default)]
pub struct Binds<'a> {
    names: &'a [String],
    index: Option<&'a String>,
}

impl<'a> Binds<'a> {
    const NONE: Binds<'static> = Binds {
        names: &[],
        index: None,
    };

    fn one(name: &'a String) -> Self {
        Binds {
            names: std::slice::from_ref(name),
            index: None,
        }
    }

    /// The bound names, in source order.
    pub fn iter(self) -> impl Iterator<Item = &'a str> {
        self.names.iter().chain(self.index).map(String::as_str)
    }

    /// Whether `name` is bound.
    pub fn contains(self, name: &str) -> bool {
        self.iter().any(|bound| bound == name)
    }

    /// Whether nothing is bound.
    pub fn is_empty(self) -> bool {
        self.names.is_empty() && self.index.is_none()
    }
}

impl Node {
    /// The node lists directly under this one, in source order: an
    /// element's or a use's children, `if`'s two arms, a `for`'s body (and
    /// an empty list), `match`'s `some` and `none` arms. `children`, the
    /// slot's place, has two empty lists.
    pub fn child_lists(&self) -> [&[Node]; 2] {
        self.scoped_child_lists().map(|(list, _)| list)
    }

    /// [`Node::child_lists`], mutably.
    pub fn child_lists_mut(&mut self) -> [&mut [Node]; 2] {
        self.scoped_child_lists_mut().map(|(list, _)| list)
    }

    /// [`Node::child_lists`], each with the names bound over it.
    pub fn scoped_child_lists(&self) -> [(&[Node], Binds<'_>); 2] {
        match self {
            Node::Element { children, .. } | Node::Use { children, .. } => {
                [(children, Binds::NONE), (&[], Binds::NONE)]
            }
            Node::Children { .. } => [(&[], Binds::NONE), (&[], Binds::NONE)],
            Node::When {
                then, otherwise, ..
            } => [(then, Binds::NONE), (otherwise, Binds::NONE)],
            Node::Each {
                var, index, body, ..
            } => [
                (
                    body,
                    Binds {
                        names: std::slice::from_ref(var),
                        index: index.as_ref(),
                    },
                ),
                (&[], Binds::NONE),
            ],
            Node::Match { some, none, .. } => [(&some.1, Binds::one(&some.0)), (none, Binds::NONE)],
        }
    }

    /// [`Node::scoped_child_lists`], mutably.
    pub fn scoped_child_lists_mut(&mut self) -> [(&mut [Node], Binds<'_>); 2] {
        match self {
            Node::Element { children, .. } | Node::Use { children, .. } => {
                [(children, Binds::NONE), (&mut [], Binds::NONE)]
            }
            Node::Children { .. } => [(&mut [], Binds::NONE), (&mut [], Binds::NONE)],
            Node::When {
                then, otherwise, ..
            } => [(then, Binds::NONE), (otherwise, Binds::NONE)],
            Node::Each {
                var, index, body, ..
            } => [
                (
                    body,
                    Binds {
                        names: std::slice::from_ref(var),
                        index: index.as_ref(),
                    },
                ),
                (&mut [], Binds::NONE),
            ],
            Node::Match { some, none, .. } => {
                let (name, list) = some;
                [(list, Binds::one(name)), (none, Binds::NONE)]
            }
        }
    }

    /// The nodes directly under this one, in source order: both lists of
    /// [`Node::child_lists`].
    pub fn children(&self) -> impl Iterator<Item = &Node> {
        self.child_lists().into_iter().flatten()
    }

    /// The expressions this node holds itself, in source order: an
    /// element's positionals and attribute values, a use's arguments, `if`'s
    /// condition, a `for`'s list and key, a `match`'s subject. A `for`'s key
    /// may read the item and position it binds.
    pub fn exprs(&self) -> impl Iterator<Item = &Expr> {
        let (first, second, positional, attrs): (_, _, &[Expr], &[Attr]) = match self {
            Node::Element {
                positional, attrs, ..
            } => (None, None, positional, attrs),
            Node::Use { args, .. } => (None, None, &[], args),
            Node::Children { .. } => (None, None, &[], &[]),
            Node::When { cond, .. } => (Some(cond), None, &[], &[]),
            Node::Each { list, key, .. } => (Some(list), Some(key), &[], &[]),
            Node::Match { subject, .. } => (Some(subject), None, &[], &[]),
        };
        first
            .into_iter()
            .chain(second)
            .chain(positional)
            .chain(attrs.iter().map(|a| &a.value))
    }

    /// Every node in `nodes` and under them, each before the nodes under it
    /// and in source order otherwise.
    pub fn walk(nodes: &[Node]) -> Walk<'_> {
        Walk {
            inner: Self::walk_with(nodes, (), |_, _| [Some(()), Some(())]),
        }
    }

    /// Walk with a context inherited by each child list. `descend` supplies
    /// the two contexts in [`Node::child_lists`] order; `None` skips that
    /// list. It runs before the node is yielded. Siblings keep their parent's
    /// context. The heap stack uses O(depth) frames, with no Rust recursion.
    ///
    /// @ref LLP 1172.013 E1, §4 step 4 — one descent for checks with scopes
    /// or ancestor facts, as well as checks that only need the nodes.
    pub fn walk_with<'a, C: Clone, F: FnMut(&'a Node, &C) -> [Option<C>; 2]>(
        nodes: &'a [Node],
        context: C,
        descend: F,
    ) -> WalkWith<'a, C, F> {
        WalkWith {
            events: WalkEvents {
                stack: vec![Frame::List(nodes.iter(), context)],
                descend,
                reverse: false,
            },
        }
    }

    /// Enter and leave each node in source order. A skipped list has no events.
    /// Leave carries the same inherited context as enter.
    pub fn walk_events_with<'a, C: Clone, F: FnMut(&'a Node, &C) -> [Option<C>; 2]>(
        nodes: &'a [Node],
        context: C,
        descend: F,
    ) -> WalkEvents<'a, C, F> {
        WalkEvents {
            stack: vec![Frame::List(nodes.iter(), context)],
            descend,
            reverse: false,
        }
    }

    /// Preorder with roots, siblings and arms in reverse source order.
    pub fn walk_reverse(nodes: &[Node]) -> impl Iterator<Item = &Node> {
        let mut events = Self::walk_with(nodes, (), |_, _| [Some(()), Some(())]).events;
        events.reverse = true;
        events.filter_map(|(node, (), entering)| entering.then_some(node))
    }
}

/// The iterator [`Node::walk`] returns.
pub struct Walk<'a> {
    inner: WalkWith<'a, (), Unscoped>,
}

type Unscoped = fn(&Node, &()) -> [Option<()>; 2];

impl<'a> Iterator for Walk<'a> {
    type Item = &'a Node;

    fn next(&mut self) -> Option<&'a Node> {
        self.inner.next().map(|(node, ())| node)
    }
}

/// The iterator [`Node::walk_with`] returns. Each node carries the context
/// supplied for the list containing it.
pub struct WalkWith<'a, C, F> {
    events: WalkEvents<'a, C, F, false>,
}

impl<'a, C: Clone, F: FnMut(&'a Node, &C) -> [Option<C>; 2]> Iterator for WalkWith<'a, C, F> {
    type Item = (&'a Node, C);

    fn next(&mut self) -> Option<Self::Item> {
        self.events
            .find_map(|(node, context, entering)| entering.then_some((node, context)))
    }
}

enum Frame<'a, C> {
    List(std::slice::Iter<'a, Node>, C),
    Leave(&'a Node, C),
}

/// The iterator returned by [`Node::walk_events_with`]. The boolean is true
/// on entry and false on exit. Storage is O(depth), including wide lists.
pub struct WalkEvents<'a, C, F, const LEAVES: bool = true> {
    stack: Vec<Frame<'a, C>>,
    descend: F,
    reverse: bool,
}

impl<'a, C: Clone, F: FnMut(&'a Node, &C) -> [Option<C>; 2], const LEAVES: bool> Iterator
    for WalkEvents<'a, C, F, LEAVES>
{
    type Item = (&'a Node, C, bool);

    fn next(&mut self) -> Option<Self::Item> {
        while let Some(frame) = self.stack.last_mut() {
            let Frame::List(list, context) = frame else {
                let Some(Frame::Leave(node, context)) = self.stack.pop() else {
                    unreachable!()
                };
                return Some((node, context, false));
            };
            let node = if self.reverse {
                list.next_back()
            } else {
                list.next()
            };
            if let Some(node) = node {
                let context = context.clone();
                let [first, second] = node.child_lists();
                let [a, b] = (self.descend)(node, &context);
                let mut children = [(first, a), (second, b)];
                if !self.reverse {
                    children.reverse();
                }
                if LEAVES {
                    self.stack.push(Frame::Leave(node, context.clone()));
                }
                for (list, child) in children {
                    if let Some(child) = child.filter(|_| !list.is_empty()) {
                        self.stack.push(Frame::List(list.iter(), child));
                    }
                }
                return Some((node, context, true));
            }
            self.stack.pop();
        }
        None
    }
}

/// An expression's operands: up to three single ones with their binders, a
/// list's or a call's items, and a template's parts.
type Operands<'a> = (
    [Option<(&'a Expr, Binds<'a>)>; 3],
    &'a [Expr],
    &'a [TemplatePart],
);

/// [`Operands`], mutably.
type OperandsMut<'a> = (
    [Option<(&'a mut Expr, Binds<'a>)>; 3],
    &'a mut [Expr],
    &'a mut [TemplatePart],
);

impl Expr {
    /// The expressions directly under this one, in source order: a list's
    /// items, a template's holes, a call's arguments, every operand of an
    /// operator, a `match`'s subject and arms, an arrow's body, a `let`'s
    /// value and body. A literal or a name has none.
    pub fn operands(&self) -> impl Iterator<Item = &Expr> {
        self.scoped_operands().map(|(e, _)| e)
    }

    /// [`Expr::operands`], each with the names bound over it.
    pub fn scoped_operands(&self) -> impl Iterator<Item = (&Expr, Binds<'_>)> {
        fn free(e: &Expr) -> Option<(&Expr, Binds<'static>)> {
            Some((e, Binds::NONE))
        }
        let (fixed, list, parts): Operands<'_> = match self {
            Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) | Expr::Ident(..) => {
                ([None, None, None], &[], &[])
            }
            Expr::Template(parts, _) => ([None, None, None], &[], parts),
            Expr::List(items, _) | Expr::Call(_, items, _) => ([None, None, None], items, &[]),
            Expr::Some(a, _)
            | Expr::Member(a, _, _)
            | Expr::NamedArg(_, a, _)
            | Expr::Unary(_, a, _)
            | Expr::Typed(a, _, _) => ([free(a), None, None], &[], &[]),
            Expr::Binary(_, a, b, _) => ([free(a), free(b), None], &[], &[]),
            Expr::Ternary(a, b, c, _) => ([free(a), free(b), free(c)], &[], &[]),
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            } => (
                [free(subject), Some((&**some, Binds::one(var))), free(none)],
                &[],
                &[],
            ),
            Expr::Arrow { params, body, .. } => (
                [
                    Some((
                        &**body,
                        Binds {
                            names: params,
                            index: None,
                        },
                    )),
                    None,
                    None,
                ],
                &[],
                &[],
            ),
            Expr::Let {
                name, value, body, ..
            } => (
                [free(value), Some((&**body, Binds::one(name))), None],
                &[],
                &[],
            ),
        };
        fixed
            .into_iter()
            .flatten()
            .chain(list.iter().map(|e| (e, Binds::NONE)))
            .chain(parts.iter().filter_map(|part| match part {
                TemplatePart::Expr(e) => Some((e, Binds::NONE)),
                TemplatePart::Text(_) => None,
            }))
    }

    /// [`Expr::operands`], mutably.
    pub fn operands_mut(&mut self) -> impl Iterator<Item = &mut Expr> {
        self.scoped_operands_mut().map(|(e, _)| e)
    }

    /// [`Expr::scoped_operands`], mutably.
    pub fn scoped_operands_mut(&mut self) -> impl Iterator<Item = (&mut Expr, Binds<'_>)> {
        fn free(e: &mut Expr) -> Option<(&mut Expr, Binds<'static>)> {
            Some((e, Binds::NONE))
        }
        let (fixed, list, parts): OperandsMut<'_> = match self {
            Expr::Number(..) | Expr::Str(..) | Expr::Bool(..) | Expr::None(_) | Expr::Ident(..) => {
                ([None, None, None], &mut [], &mut [])
            }
            Expr::Template(parts, _) => ([None, None, None], &mut [], parts),
            Expr::List(items, _) | Expr::Call(_, items, _) => ([None, None, None], items, &mut []),
            Expr::Some(a, _)
            | Expr::Member(a, _, _)
            | Expr::NamedArg(_, a, _)
            | Expr::Unary(_, a, _)
            | Expr::Typed(a, _, _) => ([free(a), None, None], &mut [], &mut []),
            Expr::Binary(_, a, b, _) => ([free(a), free(b), None], &mut [], &mut []),
            Expr::Ternary(a, b, c, _) => ([free(a), free(b), free(c)], &mut [], &mut []),
            Expr::Match {
                subject,
                var,
                some,
                none,
                ..
            } => (
                [
                    free(subject),
                    Some((&mut **some, Binds::one(var))),
                    free(none),
                ],
                &mut [],
                &mut [],
            ),
            Expr::Arrow { params, body, .. } => (
                [
                    Some((
                        &mut **body,
                        Binds {
                            names: params,
                            index: None,
                        },
                    )),
                    None,
                    None,
                ],
                &mut [],
                &mut [],
            ),
            Expr::Let {
                name, value, body, ..
            } => (
                [free(value), Some((&mut **body, Binds::one(name))), None],
                &mut [],
                &mut [],
            ),
        };
        fixed
            .into_iter()
            .flatten()
            .chain(list.iter_mut().map(|e| (e, Binds::NONE)))
            .chain(parts.iter_mut().filter_map(|part| match part {
                TemplatePart::Expr(e) => Some((e, Binds::NONE)),
                TemplatePart::Text(_) => None,
            }))
    }

    /// This expression and every one under it, each before its operands
    /// and in source order otherwise. Binders are not tracked: a name read
    /// here may be one an arrow, a `match` or a `let` above it binds.
    pub fn walk(&self) -> impl Iterator<Item = &Expr> {
        let mut stack = vec![self];
        std::iter::from_fn(move || {
            let e = stack.pop()?;
            let at = stack.len();
            stack.extend(e.operands());
            stack[at..].reverse();
            Some(e)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse;

    fn view(src: &str) -> Vec<Node> {
        let file = parse(src).expect("parses");
        file.components[0].view.clone()
    }

    fn tags(nodes: &[Node]) -> Vec<String> {
        Node::walk(nodes)
            .map(|n| match n {
                Node::Element { tag, .. } => tag.clone(),
                Node::Use { name, .. } => name.clone(),
                Node::Children { .. } => "children".into(),
                Node::When { .. } => "if".into(),
                Node::Each { .. } => "for".into(),
                Node::Match { .. } => "match".into(),
            })
            .collect()
    }

    #[test]
    fn a_walk_reaches_every_kind_in_source_order() {
        let nodes = view(
            "component App
  state xs: number[] = []
  state o: option<string> = none
  view
    column
      if true
        a
      else
        b
      for x, i in xs key=x
        c
      match o
        case some(s)
          d
        case none
          e
      Card()
        f
",
        );
        assert_eq!(
            tags(&nodes),
            ["column", "if", "a", "b", "for", "c", "match", "d", "e", "Card", "f"]
        );
        let Node::Element { children, .. } = &nodes[0] else {
            panic!("column")
        };
        let binds: Vec<Vec<&str>> = children
            .iter()
            .flat_map(|n| n.scoped_child_lists())
            .map(|(_, b)| b.iter().collect())
            .collect();
        assert_eq!(
            binds,
            [
                vec![],
                vec![],
                vec!["x", "i"],
                vec![],
                vec!["s"],
                vec![],
                vec![],
                vec![]
            ]
        );
    }

    #[test]
    fn an_expression_walk_names_each_operand_once_with_its_binders() {
        let file = parse(
            "component App
  state o: option<number> = none
  state xs: number[] = []
  derive d = `${match o { case some(v) => v + 1, case none => 0 }}` + f(xs.map((x, i) => x * i), -a.b)
",
        )
        .expect("parses");
        let d = &file.components[0].derives[0].expr;
        let idents: Vec<&str> = d
            .walk()
            .filter_map(|e| match e {
                Expr::Ident(name, _) => Some(name.as_str()),
                _ => None,
            })
            .collect();
        assert_eq!(idents, ["o", "v", "xs", "x", "i", "a"]);
        let mut bound = Vec::new();
        fn binders<'a>(e: &'a Expr, out: &mut Vec<&'a str>) {
            for (child, binds) in e.scoped_operands() {
                out.extend(binds.iter());
                binders(child, out);
            }
        }
        binders(d, &mut bound);
        assert_eq!(bound, ["v", "x", "i"]);
    }

    #[test]
    fn inherited_context_stays_in_its_branch_and_can_prune_a_list() {
        let nodes = view("component App\n  view\n    column\n      if true\n        a\n      else\n        b\n      c\n    d\n");
        let found: Vec<_> = Node::walk_with(&nodes, 0, |node, depth| match node {
            Node::When { .. } => [Some(depth + 1), None],
            _ => [Some(depth + 1), Some(depth + 1)],
        })
        .filter_map(|(node, depth)| match node {
            Node::Element { tag, .. } => Some((tag.as_str(), depth)),
            _ => None,
        })
        .collect();
        assert_eq!(found, [("column", 0), ("a", 2), ("c", 1), ("d", 0)]);
    }
}
