//! A calculation tree on the wire (dimension kind 24): each node a tag and
//! its payload, children in order. The decoder admits only a tree the
//! parser makes, one whose own text (§10.13) parses back to it, so the wire,
//! the text and the parser hold one form of every value.

use super::super::env::{Edge, Region, SegmentVar, TitleBarVar};
use super::super::targets::{self, Kind, Relation, Target};
use super::super::ViewportUnit;
use super::{Comparison, Entry, EnvTerm, Node, Unit};
use crate::error::DecodeError;
use crate::wire::codec::{Reader, Writer};
use crate::StyleId;

/// Nodes and levels a tree on the wire may hold while it is read, bounding
/// the work before it is checked: past what any text within §10.8's limits
/// makes, a level's `clamp(none, …, none)` and the parentheses the kernel
/// writes included.
pub(super) const MAX_NODES: usize = 2 * super::MAX_VALUES * (super::MAX_DEPTH + 1);
pub(super) const MAX_LEVELS: usize = 2 * super::MAX_DEPTH + 2;

/// Append an entry: a text whose `env()` has a fallback as tag 9 and the
/// codec's string (a `u32` length and UTF-8 bytes); a tree node by node,
/// each a tag (0 a value, 1 `env()`, 2 a sum, 3 a product, 4 a negation, 5
/// an inversion, 6 min, 7 max, 8 clamp), then a value's `f64` and unit (0 a
/// number, 1 a percentage, 2 px, 3–12 a viewport unit, 128 rem); an `env()`'s tag
/// (0–3 an inset's edge, 8–13 a segment's variable, then its two indices,
/// 16–35 a layout region's inset, 36–39 a title-bar area variable);
/// a list's `u8` count; clamp's three presence bytes, each before its
/// argument; 10 an author's custom property (LLP 1123 D5), its name as the
/// codec's string, then its fallback's presence byte and the fallback; 11
/// an observation of the safe area (LLP 1123 D10's leaf): its target (0 the
/// box itself; 1 a chain entry, then its number from 1; 2 a chain kind and
/// 3 a relation, then its index in [`Kind::ALL`] or [`Relation::ALL`]; 4 a
/// registered target, then its name as the codec's string), its edge, then
/// its fallback's presence byte and the fallback, which the round trip
/// through the text refuses until the parser reads the leaf.
pub(super) fn encode(entry: &Entry, w: &mut Writer) {
    match entry {
        Entry::Tree(n) => node_out(n, w),
        // A text whose `env()` has a fallback, as written
        Entry::Text { text, .. } => {
            w.u8(9);
            w.string(text);
        }
    }
}

/// `rem`'s unit tag, past the viewport units' (3 onward).
const REM: u8 = 128;

fn node_out(n: &Node, w: &mut Writer) {
    match n {
        Node::Value(v, unit) => {
            w.u8(0);
            w.f64(*v);
            w.u8(match unit {
                Unit::Number => 0,
                Unit::Percent => 1,
                Unit::Px => 2,
                Unit::Viewport(u) => 3 + *u as u8,
                Unit::Rem => REM,
            });
        }
        Node::Env(term) => {
            w.u8(1);
            match term {
                EnvTerm::Inset(edge) => w.u8(*edge as u8),
                // A local inset (LLP 1123 D5): 4 + edge
                EnvTerm::Local(edge) => w.u8(4 + *edge as u8),
                EnvTerm::Segment(var, x, y) => {
                    w.u8(8 + *var as u8);
                    w.u8(*x);
                    w.u8(*y);
                }
                // A layout region's inset (LLP 1123 D10): 16 + region × 4 + edge
                EnvTerm::Region(region, edge) => w.u8(16 + *region as u8 * 4 + *edge as u8),
                EnvTerm::TitleBar(var) => w.u8(36 + *var as u8),
            }
        }
        Node::Sum(c) | Node::Product(c) | Node::Min(c) | Node::Max(c) => {
            w.u8(match n {
                Node::Sum(_) => 2,
                Node::Product(_) => 3,
                Node::Min(_) => 6,
                _ => 7,
            });
            w.u8(c.len() as u8);
            c.iter().for_each(|c| node_out(c, w));
        }
        Node::Negate(c) | Node::Invert(c) => {
            w.u8(if matches!(n, Node::Negate(_)) { 4 } else { 5 });
            node_out(c, w);
        }
        Node::Clamp(c) => {
            w.u8(8);
            c.iter().for_each(|a| optional(a.as_ref(), w));
        }
        Node::Var(v) => {
            w.u8(10);
            w.string(&v.name);
            optional(v.fallback.as_deref(), w);
        }
        Node::Observe(o) => {
            w.u8(11);
            target_out(&o.target, w);
            w.u8(o.edge as u8);
            optional(o.fallback.as_deref(), w);
        }
    }
}

fn optional(n: Option<&Node>, w: &mut Writer) {
    match n {
        Some(n) => {
            w.u8(1);
            node_out(n, w);
        }
        None => w.u8(0),
    }
}

/// Read an entry for `row`, refusing one the parser would not make.
pub(super) fn decode(r: &mut Reader<'_>, row: StyleId) -> Result<Comparison, DecodeError> {
    let refused = DecodeError::InvalidComparison;
    // A text whose `env()` has a fallback: admitted where the parser holds
    // it so, for this row
    let start = r.clone();
    if r.u8()? == 9 {
        let text = r.string()?;
        return match crate::StyleValue::Text(text.into()).dimension(row, row.admits_auto()) {
            Ok(crate::Dimension::Compare(c)) if c == Comparison::substitution(text, row) => Ok(c),
            _ => Err(refused),
        };
    }
    *r = start;
    let mut nodes = 0;
    let tree = node(r, 0, &mut nodes).ok_or(refused.clone())?;
    if !within_author_limits(&tree) {
        return Err(refused);
    }
    let mut text = String::new();
    super::text::math_function(&tree, &mut text);
    // A tree a compact form holds travels as that form, not as a tree
    match super::parse::tree(&text)
        .filter(|parsed| *parsed == tree)
        .map(super::dimension)
    {
        Some(crate::Dimension::Compare(c)) => Ok(c),
        _ => Err(refused),
    }
}

/// Check the simplified tree, excluding parentheses added by serialization.
pub(super) fn within_author_limits(tree: &Node) -> bool {
    let (values, levels) = authored(tree, true);
    values <= super::MAX_VALUES && levels <= super::MAX_DEPTH
}

/// The values `n` holds and the levels of parentheses and functions it
/// takes as written; `opened` when parentheses open it: the root, which
/// `calc()` opens unless it is a function, and a sum a product holds.
fn authored(n: &Node, opened: bool) -> (usize, usize) {
    let function = matches!(n, Node::Min(_) | Node::Max(_) | Node::Clamp(_));
    let product = matches!(n, Node::Product(_));
    let leaf = matches!(
        n,
        Node::Value(..) | Node::Env(..) | Node::Var(..) | Node::Observe(..)
    );
    let mut values = usize::from(leaf);
    let mut deepest = 0;
    for c in n.children() {
        let (v, l) = authored(c, product && matches!(c, Node::Sum(_)));
        values += v;
        deepest = deepest.max(l);
    }
    (values, deepest + usize::from(function || opened))
}

fn node(r: &mut Reader<'_>, level: usize, nodes: &mut usize) -> Option<Node> {
    *nodes += 1;
    if level > MAX_LEVELS || *nodes > MAX_NODES {
        return None;
    }
    let list = |r: &mut Reader<'_>, nodes: &mut usize| -> Option<Vec<Node>> {
        let count = usize::from(r.u8().ok()?);
        (0..count).map(|_| node(r, level + 1, nodes)).collect()
    };
    let optional = |r: &mut Reader<'_>, nodes: &mut usize| -> Option<Option<Node>> {
        match r.u8().ok()? {
            0 => Some(None),
            1 => Some(Some(node(r, level + 1, nodes)?)),
            _ => None,
        }
    };
    Some(match r.u8().ok()? {
        0 => {
            let v = r.f64().ok()?;
            let unit = match r.u8().ok()? {
                0 => Unit::Number,
                1 => Unit::Percent,
                2 => Unit::Px,
                REM => Unit::Rem,
                u => Unit::Viewport(*ViewportUnit::ALL.get(usize::from(u.checked_sub(3)?))?),
            };
            Node::Value(v, unit)
        }
        1 => {
            let term = match r.u8().ok()? {
                tag @ 0..=3 => EnvTerm::Inset(Edge::from_index(tag)?),
                tag @ 4..=7 => EnvTerm::Local(Edge::from_index(tag - 4)?),
                tag @ 8..=13 => {
                    let var = SegmentVar::from_index(tag - 8)?;
                    EnvTerm::Segment(var, r.u8().ok()?, r.u8().ok()?)
                }
                tag @ 16..=35 => EnvTerm::Region(
                    Region::from_index((tag - 16) / 4)?,
                    Edge::from_index((tag - 16) % 4)?,
                ),
                tag @ 36..=39 => EnvTerm::TitleBar(TitleBarVar::from_index(tag - 36)?),
                _ => return None,
            };
            Node::Env(term)
        }
        2 => Node::Sum(list(r, nodes)?),
        3 => Node::Product(list(r, nodes)?),
        4 => Node::Negate(Box::new(node(r, level + 1, nodes)?)),
        5 => Node::Invert(Box::new(node(r, level + 1, nodes)?)),
        6 => Node::Min(list(r, nodes)?),
        7 => Node::Max(list(r, nodes)?),
        8 => Node::Clamp(Box::new([
            optional(r, nodes)?,
            optional(r, nodes)?,
            optional(r, nodes)?,
        ])),
        10 => {
            let name = r.string().ok()?;
            Node::Var(super::Var {
                name: name.into(),
                fallback: optional(r, nodes)?.map(Box::new),
            })
        }
        11 => Node::Observe(super::Observe {
            target: target(r)?,
            edge: Edge::from_index(r.u8().ok()?)?,
            fallback: optional(r, nodes)?.map(Box::new),
        }),
        _ => return None,
    })
}

fn target_out(target: &Target, w: &mut Writer) {
    match target {
        Target::Box => w.u8(0),
        Target::Chain(n) => {
            w.u8(1);
            w.u8(*n);
        }
        Target::Kind(kind) => {
            w.u8(2);
            w.u8(Kind::ALL.iter().position(|k| k == kind).unwrap_or(0) as u8);
        }
        Target::Relation(relation) => {
            w.u8(3);
            w.u8(Relation::ALL
                .iter()
                .position(|r| r == relation)
                .unwrap_or(0) as u8);
        }
        Target::Registered(name) => {
            w.u8(4);
            w.string(name);
        }
    }
}

/// A leaf's target; `None` for an unknown tag, a chain number of 0, an
/// index past its list, or a name no target can have.
fn target(r: &mut Reader<'_>) -> Option<Target> {
    Some(match r.u8().ok()? {
        0 => Target::Box,
        1 => Target::Chain(r.u8().ok().filter(|n| *n > 0)?),
        2 => Target::Kind(*Kind::ALL.get(usize::from(r.u8().ok()?))?),
        3 => Target::Relation(*Relation::ALL.get(usize::from(r.u8().ok()?))?),
        4 => {
            let name = r.string().ok()?;
            Target::Registered(targets::valid_name(name).then(|| name.into())?)
        }
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_leaf_carries_its_target_on_the_wire() {
        let targets = [
            Target::Box,
            Target::Chain(3),
            Target::Kind(Kind::Column),
            Target::Relation(Relation::Screen),
            Target::Registered("map".into()),
        ];
        for target in targets {
            let leaf = Node::Observe(super::super::Observe {
                target,
                edge: Edge::Left,
                fallback: Some(Box::new(Node::Value(7.0, Unit::Px))),
            });
            let mut w = Writer::new();
            node_out(&leaf, &mut w);
            let bytes = w.into_vec();
            assert_eq!(node(&mut Reader::new(&bytes), 0, &mut 0), Some(leaf));
        }
        // A chain entry 0, an index past its list, a name no target has, an unknown target
        let refused = |write: &dyn Fn(&mut Writer)| {
            let mut w = Writer::new();
            w.u8(11);
            write(&mut w);
            w.u8(Edge::Top as u8);
            w.u8(0);
            let bytes = w.into_vec();
            node(&mut Reader::new(&bytes), 0, &mut 0).is_none()
        };
        assert!(refused(&|w| {
            w.u8(1);
            w.u8(0);
        }));
        assert!(refused(&|w| {
            w.u8(2);
            w.u8(Kind::ALL.len() as u8);
        }));
        assert!(refused(&|w| {
            w.u8(3);
            w.u8(Relation::ALL.len() as u8);
        }));
        assert!(refused(&|w| {
            w.u8(4);
            w.string("Map");
        }));
        assert!(refused(&|w| w.u8(5)));
    }
}
