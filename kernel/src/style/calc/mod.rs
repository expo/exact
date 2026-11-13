//! CSS length arithmetic: parse, type-check, simplify, serialize and evaluate
//! `calc()`, `min()`, `max()` and `clamp()` (CSS Values 4 §10).
//!
//! Arithmetic uses `f64` and narrows once at the result. Precision is
//! implementation-defined; exceptional values follow §10.9. Compact expressions
//! stay in `Dimension`; other trees share an owned entry, so equal
//! styles share a handle until its last owner releases it. Percentages resolve
//! during layout against their basis.
//! @ref LLP 1001 §2

mod parse;
mod simplify;
mod substitute;
mod text;
mod wire;

pub use parse::nonnegative_row;
pub(crate) use parse::{grid, is_env, names_math, parse, refusal, signed, substitutes, Refused};

use super::env::{self, Edge, Env, Region, SegmentVar, TitleBarVar};
use super::intern::{Table, Value};
use super::{Dimension, StyleId, ViewportUnit};
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};

/// The values CSS Values 4 §10.8 has a UA support in one calculation, as
/// written, and the levels of parentheses and functions: the kernel's
/// limits too.
pub const MAX_VALUES: usize = 32;
/// See [`MAX_VALUES`].
pub const MAX_DEPTH: usize = 32;

/// A numeric value's unit: a number, a percentage, a length in px (the
/// canonical unit every absolute length converts to when parsed), in a
/// viewport unit, which resolves only with the viewport, or in `rem`, which
/// resolves with the root font size ([`Env::root_font_size`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Unit {
    /// A number: a coefficient, or a divisor.
    Number,
    /// A percentage of the basis.
    Percent,
    /// px, every absolute length's canonical unit.
    Px,
    /// A viewport unit.
    Viewport(ViewportUnit),
    /// `rem`: the root font size.
    Rem,
}

impl Unit {
    /// The unit as CSS writes it after the number.
    fn name(self) -> &'static str {
        match self {
            Unit::Number => "",
            Unit::Percent => "%",
            Unit::Px => "px",
            Unit::Viewport(unit) => unit.name(),
            Unit::Rem => "rem",
        }
    }
}

/// An `env()` variable a length reads: a safe-area inset, a viewport
/// segment's variable at its indices (CSS Environment Variables 1), a
/// layout region's inset, or a side of the title-bar area.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EnvTerm {
    /// `env(safe-area-inset-<edge>)`.
    Inset(Edge),
    /// `env(viewport-segment-<var> <x> <y>)`.
    Segment(SegmentVar, u8, u8),
    /// `env(-exact-<region>-inset-<edge>)` (LLP 1123 D10).
    Region(Region, Edge),
    /// `env(titlebar-area-<var>)` (Window Controls Overlay). No compact
    /// [`Dimension`] holds it: a length that reads it is always a tree.
    TitleBar(TitleBarVar),
    /// `var(--exact-safe-area-inset-<edge>)` (LLP 1123 D5, D9): the local
    /// inset of the content owner a node is in, which a host sets per
    /// owner, so it resolves per node ([`Env::local_inset`]). A
    /// substitution, as `env()` is; always a tree.
    Local(Edge),
}

impl EnvTerm {
    /// Its points under `env`; `None` for a segment or a title-bar area
    /// `env` does not define.
    fn value(self, env: &Env) -> Option<f64> {
        match self {
            EnvTerm::Inset(edge) => Some(f64::from(env.inset(edge))),
            EnvTerm::Segment(var, x, y) => env::resolve_wide(var, x, y, env),
            EnvTerm::Region(region, edge) => Some(f64::from(env.region_inset(region, edge))),
            EnvTerm::TitleBar(var) => env.titlebar(var).map(f64::from),
            EnvTerm::Local(edge) => Some(f64::from(env.local_inset(edge))),
        }
    }
}

/// `node` with every local inset (`EnvTerm::Local`) replaced by its points
/// under `env`.
fn localize(node: &Node, env: &Env) -> Node {
    let all = |c: &[Node]| c.iter().map(|n| localize(n, env)).collect();
    match node {
        Node::Env(EnvTerm::Local(edge)) => Node::Value(f64::from(env.local_inset(*edge)), Unit::Px),
        Node::Value(..) | Node::Env(..) => node.clone(),
        Node::Sum(c) => Node::Sum(all(c)),
        Node::Product(c) => Node::Product(all(c)),
        Node::Min(c) => Node::Min(all(c)),
        Node::Max(c) => Node::Max(all(c)),
        Node::Negate(c) => Node::Negate(Box::new(localize(c, env))),
        Node::Invert(c) => Node::Invert(Box::new(localize(c, env))),
        Node::Clamp(c) => Node::Clamp(Box::new(
            c.each_ref().map(|n| n.as_ref().map(|n| localize(n, env))),
        )),
    }
}

/// A calculation tree as §10.10 builds one and §10.10.1 leaves it: numeric
/// values and `env()` at the leaves, the calc-operator nodes and the
/// comparisons above them. A simplified tree's sums and products hold their
/// children in §10.13's order, so equal values are equal trees.
#[derive(Debug, Clone)]
pub enum Node {
    /// A number, a percentage or a length in one unit.
    Value(f64, Unit),
    /// `env()` without a fallback: a substitution of a length, which a tree
    /// may hold as a leaf (one with a fallback is held as text, [`Entry`]).
    Env(EnvTerm),
    /// Its children added.
    Sum(Vec<Node>),
    /// Its children multiplied.
    Product(Vec<Node>),
    /// Its child negated: what follows a `-`.
    Negate(Box<Node>),
    /// Its child's reciprocal: what follows a `/`.
    Invert(Box<Node>),
    /// `min()`.
    Min(Vec<Node>),
    /// `max()`.
    Max(Vec<Node>),
    /// `clamp(MIN, VAL, MAX)`; `None` is `none`.
    Clamp(Box<[Option<Node>; 3]>),
}

impl PartialEq for Node {
    /// Equal bit for bit: a NaN is the NaN it is, and `-0` is not `0`.
    fn eq(&self, other: &Node) -> bool {
        use Node::*;
        match (self, other) {
            (Value(a, u), Value(b, v)) => a.to_bits() == b.to_bits() && u == v,
            (Env(a), Env(b)) => a == b,
            (Sum(a), Sum(b)) | (Product(a), Product(b)) | (Min(a), Min(b)) | (Max(a), Max(b)) => {
                a == b
            }
            (Negate(a), Negate(b)) | (Invert(a), Invert(b)) => a == b,
            (Clamp(a), Clamp(b)) => a == b,
            _ => false,
        }
    }
}

impl Node {
    /// The children a calculation reads, in order.
    fn children(&self) -> impl Iterator<Item = &Node> {
        let (nodes, optional): (&[Node], &[Option<Node>]) = match self {
            Node::Value(..) | Node::Env(..) => (&[], &[]),
            Node::Sum(c) | Node::Product(c) | Node::Min(c) | Node::Max(c) => (c, &[]),
            Node::Negate(c) | Node::Invert(c) => (std::slice::from_ref(c), &[]),
            Node::Clamp(c) => (&[], c.as_slice()),
        };
        nodes.iter().chain(optional.iter().flatten())
    }

    /// Whether a node anywhere in it satisfies `f`.
    fn any(&self, f: &dyn Fn(&Node) -> bool) -> bool {
        f(self) || self.children().any(|c| c.any(f))
    }

    /// The value under `env`, a percentage of `basis`, in IEEE-754 `f64`
    /// as §10.9.2 has it inside a tree (an infinity or a NaN included);
    /// `None` where a segment is undefined or a percentage has no basis.
    pub(crate) fn value(&self, env: &Env, basis: Option<f64>) -> Option<f64> {
        Some(match self {
            Node::Value(v, unit) => match unit {
                Unit::Number | Unit::Px => *v,
                Unit::Percent => basis? * v / 100.0,
                Unit::Viewport(u) => f64::from(u.basis(env)) * v / 100.0,
                Unit::Rem => f64::from(env.root_font_size) * v,
            },
            Node::Env(term) => term.value(env)?,
            Node::Sum(c) => {
                let mut total = 0.0;
                for n in c {
                    total += n.value(env, basis)?;
                }
                total
            }
            Node::Product(c) => {
                let mut total = 1.0;
                for n in c {
                    total *= n.value(env, basis)?;
                }
                total
            }
            Node::Negate(c) => -c.value(env, basis)?,
            Node::Invert(c) => 1.0 / c.value(env, basis)?,
            Node::Min(c) | Node::Max(c) => {
                let max = matches!(self, Node::Max(_));
                let mut out: Option<f64> = None;
                for n in c {
                    let v = n.value(env, basis)?;
                    out = Some(out.map_or(v, |o| pick(o, v, max)));
                }
                out?
            }
            Node::Clamp(c) => {
                let v = c[1].as_ref()?.value(env, basis)?;
                let v = match &c[2] {
                    Some(hi) => pick(v, hi.value(env, basis)?, false),
                    None => v,
                };
                match &c[0] {
                    Some(lo) => pick(lo.value(env, basis)?, v, true),
                    None => v,
                }
            }
        })
    }
}

/// The smaller or (when `max`) the larger of two numbers, as §10.9.2 has
/// it: a NaN on either side is NaN, and `-0` is less than `0`.
fn pick(a: f64, b: f64, max: bool) -> f64 {
    if a.is_nan() || b.is_nan() {
        return f64::NAN;
    }
    let a_first = if a == b {
        a.is_sign_negative() != max
    } else {
        (a < b) != max
    };
    if a_first {
        a
    } else {
        b
    }
}

/// The largest length a calculation's result is censored to, as Chrome's
/// layout holds it (2^25 px): §10.9.2 leaves the largest value to the
/// context, and one whose sums stay finite keeps a frame and a radius sum
/// (two `calc(infinity * 1px)` lengths) finite.
pub(crate) const LARGEST: f64 = 33_554_432.0;

/// A top-level calculation's result as the `f32` a length holds, censored
/// as §10.9.2 has it: NaN is 0, an infinity or a value past [`LARGEST`] is
/// it, and `-0` is `0`. The arithmetic before it is `f64`; this is the final
/// conversion to layout precision.
pub(crate) fn narrow(v: f64) -> f32 {
    if v.is_nan() {
        0.0
    } else {
        v.clamp(-LARGEST, LARGEST) as f32 + 0.0
    }
}

/// Whether `v` is an `f32` exactly, so a compact form loses nothing.
fn exact(v: f64) -> bool {
    f64::from(v as f32).to_bits() == v.to_bits()
}

/// A simplified tree as the dimension that holds it: a compact form where
/// one holds exactly this tree (a lone numeric value, `env()`, a percentage
/// beside px, px beside `env()`), else the tree by handle.
pub(crate) fn dimension(tree: Node) -> Dimension {
    let compact = match &tree {
        Node::Value(v, Unit::Px) => Some(Dimension::Points(narrow(*v))),
        // A lone length is its result, its rounding the result's; a
        // percentage or a viewport length is multiplied after, so it is held
        // only where an `f32` holds it exactly
        Node::Value(v, Unit::Percent) if exact(*v) => Some(Dimension::Percent(*v as f32)),
        Node::Value(v, Unit::Viewport(u)) if exact(*v) => Some(Dimension::Viewport(*u, *v as f32)),
        Node::Env(term) => env_dimension(*term, 0.0),
        Node::Sum(c) => match &c[..] {
            [Node::Value(p, Unit::Percent), Node::Value(x, Unit::Px)] if exact(*p) && exact(*x) => {
                Some(Dimension::Calc(*p as f32, *x as f32))
            }
            // A zero is kept in a sum, and `Env`'s zero is no term at all
            [Node::Value(x, Unit::Px), Node::Env(term)] if exact(*x) && *x != 0.0 => {
                env_dimension(*term, *x as f32)
            }
            _ => None,
        },
        _ => None,
    };
    compact.unwrap_or_else(|| Dimension::Compare(Comparison::intern(tree)))
}

/// The compact form of `env()` plus points, where one holds it.
fn env_dimension(term: EnvTerm, plus: f32) -> Option<Dimension> {
    Some(match term {
        EnvTerm::Inset(edge) => Dimension::Env(edge, plus),
        EnvTerm::Segment(var, x, y) => Dimension::Segment(var, x, y, plus),
        EnvTerm::Region(region, edge) => Dimension::Region(region, edge, plus),
        EnvTerm::TitleBar(_) | EnvTerm::Local(_) => return None,
    })
}

/// A bare `env()` without a fallback as the dimension that holds it.
pub(crate) fn lone_env(term: EnvTerm) -> Dimension {
    dimension(Node::Env(term))
}

/// A calculation a row holds: a handle to its interned entry.
#[derive(Clone)]
pub struct Comparison(Arc<Value<Arc<[u8]>, Entry>>);

/// What a handle names: a simplified tree, or a text whose `env()` has a
/// fallback, which CSS Environment Variables 1 §3 substitutes before the
/// row's grammar reads it, so it is held as written, with its row.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum Entry {
    Tree(Node),
    Text { text: String, row: StyleId },
}

static TABLE: LazyLock<Mutex<Table<Arc<[u8]>, Entry>>> = LazyLock::new(|| Mutex::new(Table::new()));

impl PartialEq for Comparison {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.0, &other.0)
    }
}
impl Eq for Comparison {}
impl std::hash::Hash for Comparison {
    fn hash<H: std::hash::Hasher>(&self, h: &mut H) {
        self.0.id.hash(h);
    }
}

impl Comparison {
    /// Read a serialized calculation tree's CSS. Serialization can add
    /// parentheses, so author limits apply to the simplified tree instead.
    /// Parsing is bounded by the wire's node and depth limits.
    pub fn from_css(text: &str) -> Option<Self> {
        let tree = parse::tree(text)?;
        if !wire::within_author_limits(&tree) {
            return None;
        }
        match dimension(tree) {
            Dimension::Compare(c) => Some(c),
            _ => None,
        }
    }

    /// The handle of a simplified tree, interned.
    pub(crate) fn intern(tree: Node) -> Comparison {
        Self::intern_entry(Entry::Tree(tree))
    }

    /// The handle of a text whose `env()` has a fallback, for `row`; the
    /// text as written, trimmed.
    pub(crate) fn substitution(text: &str, row: StyleId) -> Comparison {
        Self::intern_entry(Entry::Text {
            text: text.to_string(),
            row,
        })
    }

    fn intern_entry(entry: Entry) -> Comparison {
        let mut w = crate::wire::codec::Writer::new();
        wire::encode(&entry, &mut w);
        if let Entry::Text { row, .. } = &entry {
            // The row is no part of the wire form, which the decoder reads
            // for its own row: it keys the entry here
            w.u16(*row as u16);
        }
        Comparison(Table::intern(&TABLE, Arc::from(w.into_vec()), entry))
    }

    pub(super) fn identity(&self) -> usize {
        self.0.id
    }

    fn with<T>(&self, f: impl FnOnce(&Entry) -> T) -> T {
        f(&self.0.value)
    }

    /// The points this resolves to under `env`, its percentages of `basis`,
    /// censored as a top-level calculation is ([`narrow`]); `None` where it
    /// is invalid or a percentage has no basis.
    pub fn value(&self, env: &Env, basis: Option<f32>) -> Option<f32> {
        self.wide(env, basis.map(f64::from)).map(narrow)
    }

    /// [`Comparison::value`] before it is censored: what a caller that adds
    /// to it censors after.
    pub(crate) fn wide(&self, env: &Env, basis: Option<f64>) -> Option<f64> {
        match self.tree(|t| t.value(env, basis)) {
            Some(v) => v,
            None => substitute::value(self.substituted(env)?, env, basis),
        }
    }

    /// `f` of the tree, or `None` for a text, without copying either.
    fn tree<T>(&self, f: impl FnOnce(&Node) -> T) -> Option<T> {
        self.with(|e| match e {
            Entry::Tree(t) => Some(f(t)),
            Entry::Text { .. } => None,
        })
    }

    /// A text's value under `env` as its row reads it with every `env()`
    /// substituted (CSS Environment Variables 1 §3): a variable `env`
    /// defines is its own, an undefined one's fallback stands in its place;
    /// `None` when the result is no value of the row, invalid at
    /// computed-value time. A tree is itself.
    pub(crate) fn substituted(&self, env: &Env) -> Option<Dimension> {
        let text = self.with(|e| match e {
            Entry::Tree(_) => None,
            Entry::Text { text, row } => Some((text.clone(), *row)),
        });
        match text {
            None => Some(Dimension::Compare(self.clone())),
            Some((text, row)) => {
                let text = substitute::substitute(&text, env)?;
                // Read as an author's value of the row is, by the one
                // conversion `set_dynamic` makes (`ch`, `in`, `auto` and the
                // rest); a fallback refuses `rem` and `em`
                // (`refusal::RELATIVE_FALLBACK`)
                let value = match parse::is_keyword(&text, "auto") {
                    true => crate::StyleValue::Auto,
                    false => crate::StyleValue::Text(text.into()),
                };
                let mut props = crate::StyleProps::default();
                props.set_dynamic(row, &value).ok()?;
                match props.get(row) {
                    crate::RowValue::Dimension(d) if props.relative.is_empty() => Some(d),
                    _ => None,
                }
            }
        }
    }

    /// Whether it is invalid at computed-value time under `env`, the row
    /// taking its initial value: a segment a tree reads is undefined,
    /// whichever argument it is in, or a text substituted is no value of its
    /// row (CSS Environment Variables 1 §3).
    pub(crate) fn undefined(&self, env: &Env) -> bool {
        self.tree(|t| t.any(&|n| matches!(n, Node::Env(term) if term.value(env).is_none())))
            .unwrap_or_else(|| {
                self.substituted(env)
                    .is_none_or(|d| d.undefined_segment(env))
            })
    }

    /// The CSS text: a tree's as §10.13 serializes it, a text as written,
    /// for a host whose browser resolves it.
    pub fn css(&self, out: &mut String) {
        self.with(|e| match e {
            Entry::Tree(t) => text::math_function(t, out),
            Entry::Text { text, .. } => out.push_str(text),
        });
    }

    /// Whether it reads the safe-area inset at `edge` under `env`, or the
    /// corner-adapted safe area's there, which is never less (LLP 1123 D10):
    /// a text once substituted.
    pub fn reads(&self, edge: Edge, env: &Env) -> bool {
        self.tree(|t| {
            t.any(&|n| {
                matches!(n, Node::Env(EnvTerm::Inset(e) | EnvTerm::Local(e) | EnvTerm::Region(Region::SafeAreaCorners, e)) if *e == edge)
            })
        })
        .unwrap_or_else(|| match self.substituted(env) {
            Some(Dimension::Env(e, _) | Dimension::Region(Region::SafeAreaCorners, e, _)) => e == edge,
            Some(Dimension::Compare(c)) => c.reads(edge, env),
            _ => false,
        })
    }

    /// Append the wire form ([`wire::encode`]).
    pub(crate) fn encode(&self, w: &mut crate::wire::codec::Writer) {
        self.with(|e| wire::encode(e, w));
    }

    /// Read the wire form for `row`, admitting only what the parser makes.
    pub(crate) fn decode(
        r: &mut crate::wire::codec::Reader<'_>,
        row: StyleId,
    ) -> Result<Comparison, crate::error::DecodeError> {
        wire::decode(r, row)
    }

    pub(crate) fn is_length_percentage(&self) -> bool {
        self.tree(|t| !t.any(&|n| matches!(n, Node::Env(_) | Node::Value(_, Unit::Viewport(_)))))
            .unwrap_or(false)
    }

    /// Whether it reads a local inset (`var(--exact-safe-area-inset-*)`,
    /// LLP 1123 D9), which resolves per node: in a tree, at a leaf; a text
    /// whose `env()` has a fallback, wherever it names one.
    pub(crate) fn reads_local(&self) -> bool {
        self.with(|e| match e {
            Entry::Tree(t) => t.any(&|n| matches!(n, Node::Env(EnvTerm::Local(_)))),
            Entry::Text { text, .. } => text.contains(env::LOCAL_PREFIX),
        })
    }

    /// Whether it reads the local inset at `edge`.
    pub(crate) fn reads_local_edge(&self, edge: Edge) -> bool {
        self.with(|e| match e {
            Entry::Tree(t) => t.any(&|n| matches!(n, Node::Env(EnvTerm::Local(e)) if *e == edge)),
            Entry::Text { text, .. } => {
                text.contains(&format!("{}{}", env::LOCAL_PREFIX, edge.name()))
            }
        })
    }

    /// The tree with its local insets as `env` has them, in points, as the
    /// dimension that holds it; `None` when it reads none. A tree handed to
    /// layout, where only the window's environment is known, is localized
    /// first (LLP 1123 D5).
    pub(crate) fn localized(&self, env: &Env) -> Option<Dimension> {
        self.tree(|t| {
            t.any(&|n| matches!(n, Node::Env(EnvTerm::Local(_))))
                .then(|| dimension(simplify::simplify(localize(t, env))))
        })
        .flatten()
    }

    /// Whether a `rem` stands in it, which the root font size resolves.
    pub(crate) fn reads_rem(&self) -> bool {
        self.tree(|t| t.any(&|n| matches!(n, Node::Value(_, Unit::Rem))))
            .unwrap_or(false)
    }

    /// Whether a percentage stands in it: in a tree, at a leaf, which
    /// simplification keeps (a percentage combines only with another); a
    /// text is checked once substituted, as the row reads it.
    pub(crate) fn has_percent(&self) -> bool {
        self.tree(|t| t.any(&|n| matches!(n, Node::Value(_, Unit::Percent))))
            .unwrap_or(false)
    }
}

impl std::fmt::Debug for Comparison {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut css = String::new();
        self.css(&mut css);
        f.debug_tuple("Comparison").field(&css).finish()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detached_calculations_own_entries_and_release_their_index_keys() {
        let env = Env::default();
        let detached = Dimension::parse_calc("min(17px, 53%)").unwrap();
        let Dimension::Compare(comparison) = detached.clone() else {
            unreachable!()
        };
        let id = comparison.identity();
        let mut released = Vec::new();
        // The lowering test uses 51%; these keys must have no other live owners.
        for i in 0..200_000 {
            let Dimension::Compare(c) = Dimension::parse_calc(&format!("min({i}px, 54%)")).unwrap()
            else {
                unreachable!()
            };
            released.push(c.identity());
        }
        let table = TABLE.lock().unwrap();
        assert!(released.into_iter().all(|id| table.get(id).is_none()));
        assert!(table.get(id).is_some());
        drop(table);
        assert_eq!(detached.at_basis(&env, 100.), 17.);
        drop(detached);
        assert_eq!(comparison.value(&env, Some(100.)), Some(17.));
        let key = comparison.0.key.clone();
        drop(comparison);
        let table = TABLE.lock().unwrap();
        assert!(table.get(id).is_none());
        assert!(!table.contains(&key));
    }

    #[test]
    fn canonical_css_enforces_tree_limits_and_bounds_parsing() {
        let nested = |depth| {
            let mut text = "1%".to_string();
            for _ in 0..depth {
                text = format!("clamp(none, {text}, none)");
            }
            text
        };
        assert!(Comparison::from_css(&nested(MAX_DEPTH)).is_some());
        assert!(Comparison::from_css(&nested(MAX_DEPTH + 1)).is_none());
        for count in [MAX_VALUES, MAX_VALUES + 1] {
            let args = (0..count)
                .map(|i| format!("{i}%"))
                .collect::<Vec<_>>()
                .join(", ");
            assert_eq!(
                Comparison::from_css(&format!("min({args})")).is_some(),
                count == MAX_VALUES
            );
        }
        // Redundant functions and sums simplify away, but parsing them is bounded too.
        let too_deep = format!(
            "{}min(12px, 57%){}",
            "calc(".repeat(wire::MAX_LEVELS),
            ")".repeat(wire::MAX_LEVELS)
        );
        assert!(Comparison::from_css(&too_deep).is_none());
        let too_many = format!("calc({}min(12px, 57%))", "1px + ".repeat(wire::MAX_NODES));
        assert!(Comparison::from_css(&too_many).is_none());
    }
}
