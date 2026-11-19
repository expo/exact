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
pub(crate) mod simplify;
mod substitute;
pub(crate) use substitute::position_tokens;
mod text;
pub(crate) use text::ident;
mod wire;

pub use parse::nonnegative_row;
pub(crate) use parse::{
    grid, is_env, length_percentage, names_math, parse, refusal, signed, substitutes, Refused,
};

use super::env::{self, Edge, Env, Region, SegmentVar, TitleBarVar};
use super::intern::{Table, Value};
use super::targets::Target;
use super::{Dimension, StyleId, ViewportUnit};
use std::sync::Arc;
use std::sync::{LazyLock, Mutex};

/// The calculations in a length row or either coordinate of a position row.
pub(crate) fn in_row(row: super::RowValue<'_>) -> impl Iterator<Item = Comparison> {
    let calculation = |d: &Dimension| match d {
        Dimension::Compare(c) => Some(c.clone()),
        _ => None,
    };
    match row {
        super::RowValue::Dimension(d) => [calculation(&d), None],
        super::RowValue::ObjectPosition(p) => p.calculations(),
        _ => [None, None],
    }
    .into_iter()
    .flatten()
}

/// Whether a row directly holds a container length.
pub(crate) fn reads_plain_container(row: &super::RowValue<'_>) -> bool {
    let reads = |d: &Dimension| matches!(d, Dimension::Viewport(..)) && d.reads_container();
    match row {
        super::RowValue::Dimension(d) => reads(d),
        super::RowValue::ObjectPosition(p) => reads(&p.x) || reads(&p.y),
        _ => false,
    }
}

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

/// `var(--name)` of an author's custom property (LLP 1123 D5), read in a
/// length: the property's value where the reading node has one, else the
/// fallback; with neither, the length is invalid at computed-value time.
#[derive(Debug, Clone, PartialEq)]
pub struct Var {
    /// The property's name, with its leading `--`.
    pub name: Arc<str>,
    /// What stands in where the property has no value.
    pub fallback: Option<Box<Node>>,
}

/// What an observation reads (LLP 1123 D10's families): the safe area; the
/// keyboard's guide, docked only or following an undocked keyboard too (D7);
/// the layout regions UIKit's `UIView.LayoutRegion` gives a view, with their
/// corner variants, and the older readable guide (D8, D10); and the owner
/// controller's system minimum, a raw observation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum Family {
    /// `--exact-safe-area-inset-*`.
    SafeArea = 0,
    /// `--exact-keyboard-inset-*`: UIKit's guide with `usesBottomSafeArea`
    /// and `followsUndockedKeyboard` off.
    Keyboard = 1,
    /// `--exact-keyboard-undocked-inset-*`: the guide that also follows an
    /// undocked keyboard.
    KeyboardUndocked = 2,
    /// `--exact-safe-area-corners-inset-*`: `.safeArea(cornerAdaptation:)`.
    SafeAreaCorners = 3,
    /// `--exact-layout-margins-inset-*`: `.margins()`, the margins rectangle.
    LayoutMargins = 4,
    /// `--exact-layout-margins-corners-inset-*`: `.margins(cornerAdaptation:)`.
    LayoutMarginsCorners = 5,
    /// `--exact-readable-content-inset-*`: `.readableContent()`.
    ReadableContent = 6,
    /// `--exact-readable-content-corners-inset-*`:
    /// `.readableContent(cornerAdaptation:)`.
    ReadableContentCorners = 7,
    /// `--exact-readable-content-guide-inset-*`: `readableContentGuide`.
    ReadableContentGuide = 8,
    /// `--exact-system-minimum-layout-margins-inset-*`: the owner controller's
    /// `systemMinimumLayoutMargins`.
    SystemMinimum = 9,
}

/// A reader's components, a bit each ([`Observe::component`]).
pub(crate) type Components = u64;

impl Family {
    /// Every family, in wire order.
    pub const ALL: [Family; 10] = [
        Family::SafeArea,
        Family::Keyboard,
        Family::KeyboardUndocked,
        Family::SafeAreaCorners,
        Family::LayoutMargins,
        Family::LayoutMarginsCorners,
        Family::ReadableContent,
        Family::ReadableContentCorners,
        Family::ReadableContentGuide,
        Family::SystemMinimum,
    ];

    /// The families D8's margins rule gives a box, which a host that reports
    /// no margins resolves to the box's safe area (D10, "Other hosts").
    pub const MARGINS: [Family; 6] = [
        Family::SafeAreaCorners,
        Family::LayoutMargins,
        Family::LayoutMarginsCorners,
        Family::ReadableContent,
        Family::ReadableContentCorners,
        Family::ReadableContentGuide,
    ];

    /// The name's part before the edge: `safe-area-inset-` and the like.
    pub fn infix(self) -> &'static str {
        match self {
            Family::SafeArea => "safe-area-inset-",
            Family::Keyboard => "keyboard-inset-",
            Family::KeyboardUndocked => "keyboard-undocked-inset-",
            Family::SafeAreaCorners => "safe-area-corners-inset-",
            Family::LayoutMargins => "layout-margins-inset-",
            Family::LayoutMarginsCorners => "layout-margins-corners-inset-",
            Family::ReadableContent => "readable-content-inset-",
            Family::ReadableContentCorners => "readable-content-corners-inset-",
            Family::ReadableContentGuide => "readable-content-guide-inset-",
            Family::SystemMinimum => "system-minimum-layout-margins-inset-",
        }
    }

    /// The family by wire index.
    pub fn from_index(i: u8) -> Option<Family> {
        Family::ALL.get(i as usize).copied()
    }

    /// The family and edge a host's observation name reads
    /// (`--exact-safe-area-inset-top`, `--exact-keyboard-inset-bottom`); `None`
    /// for any other name.
    pub fn named(name: &str) -> Option<(Family, Edge)> {
        let observed = name.strip_prefix("--exact-")?;
        Family::ALL.into_iter().find_map(|family| {
            let edge = observed.strip_prefix(family.infix())?;
            Some((family, Edge::from_name(edge)?))
        })
    }

    /// The target, family and edge an observation's name reads (LLP 1123 D9):
    /// the box's own family (`--exact-keyboard-inset-bottom`), or a native
    /// target's raw safe area (`--exact-native-2-safe-area-inset-top`,
    /// `--exact-navigation-safe-area-inset-top`,
    /// `--exact-window-safe-area-inset-left`,
    /// `--exact-target-map-safe-area-inset-top`); `None` for any other name.
    pub fn observed(name: &str) -> Option<(Target, Family, Edge)> {
        if let Some((family, edge)) = Family::named(name) {
            return Some((Target::Box, family, edge));
        }
        let (target, edge) = name
            .strip_prefix("--exact-")?
            .rsplit_once("-safe-area-inset-")?;
        Some((
            Target::named(target)?,
            Family::SafeArea,
            Edge::from_name(edge)?,
        ))
    }

    /// Whether an observation of `self` on `target` is raw, which a reservation
    /// may not read (LLP 1123 D5, "Provenance"): every native target's, and the
    /// owner controller's system minimum.
    pub fn raw(self, target: &Target) -> bool {
        *target != Target::Box || self == Family::SystemMinimum
    }

    /// Its components' bits among a reader's inputs ([`Observe::component`]).
    pub(crate) const fn bits(self) -> Components {
        0b1111 << (self as Components * 4)
    }
}

/// How many values a reader's input holds: four edges a family, each at
/// [`Observe::component`].
pub const COMPONENTS: usize = Family::ALL.len() * 4;

/// An observation (LLP 1123 D9, D10's leaf) of a family on `edge`: the
/// reading box's own, as the solve gives it, or a native target's raw
/// safe-area insets ([`Env::observation`]); else `fallback`,
/// guaranteed-invalid without one. Each is lowered from its name
/// (`var(--exact-keyboard-inset-*)`, `var(--exact-native-2-safe-area-inset-*)`)
/// but the box's own safe area's, which the kernel builds until activation
/// reads `var(--exact-safe-area-inset-*)` as an observation.
#[derive(Debug, Clone, PartialEq)]
pub struct Observe {
    /// Whose: the box's own, or a native target's.
    pub target: super::targets::Target,
    /// What it reads.
    pub family: Family,
    /// The edge observed.
    pub edge: Edge,
    /// What stands in where the observation is unavailable.
    pub fallback: Option<Box<Node>>,
}

impl Observe {
    /// Its index among a reader's inputs: four edges a family.
    pub(crate) fn component(&self) -> usize {
        self.family as usize * 4 + self.edge as usize
    }
}

/// `node` with every local inset (`EnvTerm::Local`) and container unit (LLP
/// 1123 D16) replaced by its points under `env`, and every custom property
/// by its value there (D5);
/// `None` where a property has neither a value nor a fallback.
fn localize(node: &Node, env: &Env) -> Option<Node> {
    let leaves = Leaves {
        local: &|edge| env.local_inset(edge),
        observed: &|leaf| env.observation(leaf),
        // Replace every container leaf, so `localized` leaves none it fires on: the basis
        // is the small viewport's where `env` has no container
        container: &|unit| Some(unit.basis(env)),
    };
    substitute(node, &leaves, &mut |name| env.property(name).cloned())
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
    /// `var()` of an author's custom property, resolved per node.
    Var(Var),
    /// An observation of the box that reads it (LLP 1123 D9, D10's leaf).
    Observe(Observe),
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
            (Var(a), Var(b)) => a == b,
            (Observe(a), Observe(b)) => a == b,
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
    pub(crate) fn children(&self) -> impl Iterator<Item = &Node> {
        let (nodes, optional): (&[Node], &[Option<Node>]) = match self {
            Node::Value(..) | Node::Env(..) => (&[], &[]),
            Node::Var(v) => (
                v.fallback.as_deref().map_or(&[][..], std::slice::from_ref),
                &[],
            ),
            Node::Observe(o) => (
                o.fallback.as_deref().map_or(&[][..], std::slice::from_ref),
                &[],
            ),
            Node::Sum(c) | Node::Product(c) | Node::Min(c) | Node::Max(c) => (c, &[]),
            Node::Negate(c) | Node::Invert(c) => (std::slice::from_ref(c), &[]),
            Node::Clamp(c) => (&[], c.as_slice()),
        };
        nodes.iter().chain(optional.iter().flatten())
    }

    /// Whether a node anywhere in it satisfies `f`.
    pub(crate) fn any(&self, f: &dyn Fn(&Node) -> bool) -> bool {
        f(self) || self.children().any(|c| c.any(f))
    }

    /// Each custom property it reads, fallbacks included.
    pub(crate) fn names(&self, out: &mut Vec<Arc<str>>) {
        if let Node::Var(v) = self {
            if !out.contains(&v.name) {
                out.push(v.name.clone());
            }
        }
        self.children().for_each(|c| c.names(out));
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
            Node::Var(v) => match env.property(&v.name) {
                Some(value) => value.value(env, basis)?,
                None => v.fallback.as_ref()?.value(env, basis)?,
            },
            Node::Observe(o) => match env.observation(o) {
                Some(points) => f64::from(points),
                None => o.fallback.as_ref()?.value(env, basis)?,
            },
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

/// The tree a length is (LLP 1123 D5): a compact form's, or a
/// calculation's own; `None` for `auto`, a size keyword or a text a row
/// substitutes.
pub(crate) fn tree_of(d: &Dimension) -> Option<Node> {
    let plus = |term: EnvTerm, x: f32| match x {
        0.0 => Node::Env(term),
        _ => Node::Sum(vec![Node::Env(term), Node::Value(f64::from(x), Unit::Px)]),
    };
    Some(match d {
        // A size keyword is no length a tree holds
        Dimension::Auto | Dimension::Keyword(_) => return None,
        Dimension::Points(v) => Node::Value(f64::from(*v), Unit::Px),
        Dimension::Percent(p) => Node::Value(f64::from(*p), Unit::Percent),
        Dimension::Calc(p, x) => Node::Sum(vec![
            Node::Value(f64::from(*p), Unit::Percent),
            Node::Value(f64::from(*x), Unit::Px),
        ]),
        Dimension::Env(edge, x) => plus(EnvTerm::Inset(*edge), *x),
        Dimension::Region(region, edge, x) => plus(EnvTerm::Region(*region, *edge), *x),
        Dimension::Segment(var, a, b, x) => plus(EnvTerm::Segment(*var, *a, *b), *x),
        Dimension::Viewport(unit, n) => Node::Value(f64::from(*n), Unit::Viewport(*unit)),
        Dimension::Compare(c) => return c.tree(|t| t.clone()),
    })
}

/// Whether `node` reads an environment value `env` leaves undefined, which
/// makes what holds it invalid at computed-value time (CSS Environment
/// Variables 1 §3); a custom property is read as its value, else its
/// fallback (CSS Variables 1 §3).
pub(crate) fn undefined_in(node: &Node, env: &Env) -> bool {
    match node {
        // A local inset is always defined, and only a node's environment has it
        Node::Env(EnvTerm::Local(_)) => false,
        Node::Env(term) => term.value(env).is_none(),
        Node::Var(v) => match (env.property(&v.name), &v.fallback) {
            (Some(value), _) => undefined_in(value, env),
            (None, Some(fallback)) => undefined_in(fallback, env),
            (None, None) => true,
        },
        Node::Observe(o) => match (env.observation(o), &o.fallback) {
            (Some(_), _) => false,
            (None, Some(fallback)) => undefined_in(fallback, env),
            (None, None) => true,
        },
        n => n.children().any(|c| undefined_in(c, env)),
    }
}

/// Whether `node` reads a raw observation, directly, in a fallback or through a
/// custom property `env` says holds one, which a reservation may not (LLP 1123
/// D5, "Provenance").
pub(crate) fn reads_raw(node: &Node, env: &Env) -> bool {
    node.any(&|n| match n {
        Node::Observe(o) => o.family.raw(&o.target),
        Node::Var(v) => (env.properties.as_ref()).is_some_and(|p| p.raw(&v.name)),
        _ => false,
    })
}

/// The first name a length text's `var()`s read, fallbacks included, that is a
/// raw observation or a custom property `raw` says holds one (LLP 1123 D5,
/// "Provenance"); `None` where it reads neither.
pub fn raw_read(text: &str, raw: &dyn Fn(&str) -> bool) -> Option<String> {
    let mut found = None;
    text_vars(text, &mut |name| {
        let observed =
            Family::observed(name).is_some_and(|(target, family, _)| family.raw(&target));
        if found.is_none() && (observed || raw(name)) {
            found = Some(name.to_string());
        }
    });
    found
}

/// The native targets a text's `var()`s read, as [`targets_of`] gives a tree's.
fn text_targets(text: &str) -> Vec<(Target, u8)> {
    let mut targets = Vec::new();
    text_vars(text, &mut |name| {
        if let Some((target, _, edge)) = Family::observed(name).filter(|(t, ..)| *t != Target::Box)
        {
            targets.push((target, 1 << edge as u8));
        }
    });
    targets
}

/// The native targets `node` reads raw insets from, fallbacks included, each
/// with the edge it reads as a bit, as [`Edge`] numbers them, once per leaf
/// (LLP 1123 D9, "The native chain").
pub(crate) fn targets_of(node: &Node) -> Vec<(Target, u8)> {
    let targets = std::cell::RefCell::new(Vec::new());
    node.any(&|n| {
        if let Node::Observe(o) = n {
            if o.target != Target::Box {
                targets
                    .borrow_mut()
                    .push((o.target.clone(), 1 << o.edge as u8));
            }
        }
        false
    });
    targets.into_inner()
}

/// Whether `node` is small enough to hold once its custom properties are
/// substituted: CSS Variables 1 §3.3 has a long expansion invalid, and the
/// bound is the one every consumer of a tree holds an author's to (CSS
/// Values 4 §10.8's values and levels, which the wire and the Apple host
/// check).
pub(crate) fn within_expansion(node: &Node) -> bool {
    wire::within_author_limits(node)
}

/// A length's CSS: a lone value as itself, any other tree as a math
/// function, a text as written.
pub(crate) fn length_css(d: &Dimension, out: &mut String) {
    let tree = match d {
        Dimension::Compare(c) => match c.tree(|t| text::length(t, out)) {
            Some(()) => return,
            None => return c.css(out),
        },
        d => tree_of(d),
    };
    match tree {
        Some(t) => text::length(&t, out),
        None => out.push_str("auto"),
    }
}

/// What a substitution reads at a node: its local insets, and its
/// observations, `None` where one is unavailable (LLP 1123 D9).
pub(crate) struct Leaves<'a> {
    pub(crate) local: &'a dyn Fn(Edge) -> f32,
    pub(crate) observed: &'a dyn Fn(&Observe) -> Option<f32>,
    /// A container unit's basis in points (LLP 1123 D16); `None` keeps the
    /// leaf, where no node's containers are known.
    pub(crate) container: &'a dyn Fn(ViewportUnit) -> Option<f32>,
}

/// `node` with each local inset and observation as `leaves` gives it and
/// each custom property as `property` resolves it (LLP 1123 D5); `None`
/// where a property or an observation has neither a value nor a fallback.
pub(crate) fn substitute(
    node: &Node,
    leaves: &Leaves<'_>,
    property: &mut dyn FnMut(&Arc<str>) -> Option<Node>,
) -> Option<Node> {
    let all = |c: &[Node], property: &mut dyn FnMut(&Arc<str>) -> Option<Node>| {
        c.iter()
            .map(|n| substitute(n, leaves, property))
            .collect::<Option<Vec<_>>>()
    };
    Some(match node {
        Node::Env(EnvTerm::Local(edge)) => Node::Value(f64::from((leaves.local)(*edge)), Unit::Px),
        Node::Var(v) => match property(&v.name) {
            Some(value) => value,
            None => substitute(v.fallback.as_ref()?, leaves, property)?,
        },
        Node::Observe(o) => match (leaves.observed)(o) {
            Some(points) => Node::Value(f64::from(points), Unit::Px),
            None => substitute(o.fallback.as_ref()?, leaves, property)?,
        },
        Node::Value(v, Unit::Viewport(unit)) if unit.is_container() => {
            match (leaves.container)(*unit) {
                Some(basis) => Node::Value(v * f64::from(basis) / 100.0, Unit::Px),
                None => node.clone(),
            }
        }
        Node::Value(..) | Node::Env(..) => node.clone(),
        Node::Sum(c) => Node::Sum(all(c, property)?),
        Node::Product(c) => Node::Product(all(c, property)?),
        Node::Min(c) => Node::Min(all(c, property)?),
        Node::Max(c) => Node::Max(all(c, property)?),
        Node::Negate(c) => Node::Negate(Box::new(substitute(c, leaves, property)?)),
        Node::Invert(c) => Node::Invert(Box::new(substitute(c, leaves, property)?)),
        Node::Clamp(c) => {
            let [lo, v, hi] = c.as_ref();
            // `none` stays `none`; an argument that substitutes to nothing is invalid
            let mut one = |n: &Option<Node>| match n {
                Some(n) => substitute(n, leaves, property).map(Some),
                None => Some(None),
            };
            Node::Clamp(Box::new([one(lo)?, one(v)?, one(hi)?]))
        }
    })
}

/// Each author's custom property a text's `var()`s name, fallbacks
/// included, by CSS's tokens: a function's name in any case, a property's
/// as written.
fn text_properties(text: &str, out: &mut Vec<Arc<str>>) {
    text_vars(text, &mut |name| {
        if crate::style::safe_area_properties::is_author_name(name)
            && !out.iter().any(|n| &**n == name)
        {
            out.push(name.into());
        }
    });
}

/// The components of its box's own observations a text's `var()`s read, as
/// [`Comparison::observed_components`] gives a tree's. The safe area's name
/// is the local inset until activation (`parse.rs`), so only the other
/// families' readings count.
fn text_observations(text: &str) -> Components {
    let mut components = 0;
    text_vars(text, &mut |name| {
        if let Some((family, edge)) = Family::named(name).filter(|(f, _)| *f != Family::SafeArea) {
            components |= 1 << (family as usize * 4 + edge as usize);
        }
    });
    components
}

/// Each name a text's `var()`s read, fallbacks included, by CSS's tokens: a
/// function's name in any case, a property's as written.
fn text_vars(text: &str, each: &mut dyn FnMut(&str)) {
    use cssparser::{Parser, ParserInput, Token};
    fn walk(p: &mut Parser<'_, '_>, each: &mut dyn FnMut(&str)) {
        while let Ok(token) = p.next() {
            let var = matches!(token, Token::Function(name) if name.eq_ignore_ascii_case("var"));
            if !matches!(token, Token::Function(_) | Token::ParenthesisBlock) {
                continue;
            }
            let _ = p.parse_nested_block(|p| {
                if var {
                    if let Ok(Token::Ident(name)) = p.next() {
                        each(name);
                    }
                }
                walk(p, each);
                Ok::<_, cssparser::ParseError<'_, ()>>(())
            });
        }
    }
    let mut input = ParserInput::new(text);
    walk(&mut Parser::new(&mut input), each);
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

    /// Whether it is held as a tree, not as a text a row substitutes.
    pub(crate) fn is_tree(&self) -> bool {
        self.tree(|_| ()).is_some()
    }

    /// `f` of the tree, or `None` for a text, without copying either.
    pub(crate) fn tree<T>(&self, f: impl FnOnce(&Node) -> T) -> Option<T> {
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

    /// Whether it reads a raw observation under `env` ([`reads_raw`]): a tree's
    /// leaf, or a text's `var()` of one or of a property that holds one.
    pub(crate) fn reads_raw(&self, env: &Env) -> bool {
        self.with(|e| match e {
            Entry::Tree(t) => reads_raw(t, env),
            Entry::Text { text, .. } => {
                let property = |name: &str| (env.properties.as_ref()).is_some_and(|p| p.raw(name));
                // What the row reads once substituted too: an `env()`'s
                // fallback can supply a `var()`'s name
                raw_read(text, &property).is_some()
                    || matches!(self.substituted(env), Some(Dimension::Compare(c)) if c.is_tree() && c.reads_raw(env))
            }
        })
    }

    /// Whether it is invalid at computed-value time under `env`, the row
    /// taking its initial value: a segment a tree reads is undefined,
    /// whichever argument it is in, or a text substituted is no value of its
    /// row (CSS Environment Variables 1 §3).
    pub(crate) fn undefined(&self, env: &Env) -> bool {
        self.tree(|t| undefined_in(t, env)).unwrap_or_else(|| {
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

    /// Whether it reads an observation, its box's own or a native target's
    /// (LLP 1123 D7, D9), a text's as it will be parsed included.
    pub(crate) fn reads_observed(&self) -> bool {
        self.with(|e| match e {
            Entry::Tree(t) => t.any(&|n| matches!(n, Node::Observe(_))),
            Entry::Text { text, .. } => {
                text_observations(text) != 0 || !text_targets(text).is_empty()
            }
        })
    }

    /// The components of its box's own observations it reads, fallbacks
    /// included: a bit per [`Observe::component`].
    pub(crate) fn observed_components(&self) -> Components {
        if let Some(components) = self.with(|e| match e {
            Entry::Text { text, .. } => Some(text_observations(text)),
            Entry::Tree(_) => None,
        }) {
            return components;
        }
        let components = std::cell::Cell::new(0 as Components);
        self.tree(|t| {
            t.any(&|n| {
                if let Node::Observe(o) = n {
                    if o.target == super::targets::Target::Box {
                        components.set(components.get() | 1 << o.component());
                    }
                }
                false
            })
        });
        components.get()
    }

    /// The native targets it reads raw insets from, fallbacks included, each
    /// with the edge it reads as a bit, as [`Edge`] numbers them, once per
    /// leaf (LLP 1123 D9, "The native chain").
    pub(crate) fn targets_read(&self) -> Vec<(super::targets::Target, u8)> {
        self.with(|e| match e {
            Entry::Tree(t) => targets_of(t),
            Entry::Text { text, .. } => text_targets(text),
        })
    }

    /// Whether it reads a container unit (LLP 1123 D16), which resolves per
    /// node: in a tree, at a leaf; a text whose `env()` has a fallback,
    /// wherever it may name one.
    pub(crate) fn reads_container(&self) -> bool {
        match &self.0.value {
            Entry::Tree(t) => {
                t.any(&|n| matches!(n, Node::Value(_, Unit::Viewport(u)) if u.is_container()))
            }
            // An escape may spell the unit (`c\71 w`), so a text with one may read it
            Entry::Text { text, .. } => {
                let text = text.to_ascii_lowercase();
                text.contains("cq") || text.contains('\\')
            }
        }
    }

    /// Whether it reads an author's custom property (LLP 1123 D5), which
    /// resolves per node: in a tree, at a leaf; a text whose `env()` has a
    /// fallback, wherever it names one.
    pub(crate) fn reads_custom(&self) -> bool {
        match &self.0.value {
            Entry::Tree(t) => t.any(&|n| matches!(n, Node::Var(_))),
            Entry::Text { text, .. } => {
                let mut names = Vec::new();
                text_properties(text, &mut names);
                !names.is_empty()
            }
        }
    }

    /// The custom properties it reads, each name with its `--`, fallbacks
    /// included (LLP 1123 D5): a tree's at its leaves, a text's wherever a
    /// `var()` of an author's name stands in it.
    pub(crate) fn properties(&self, out: &mut Vec<Arc<str>>) {
        match &self.0.value {
            Entry::Tree(t) => t.names(out),
            Entry::Text { text, .. } => text_properties(text, out),
        }
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

    /// The tree with its local insets, custom properties, observations and
    /// container units as `env` has them, as the dimension that holds it,
    /// `Auto` (the row's initial value) where a property is invalid; `None`
    /// when it reads none of them. A tree handed to layout, where only the
    /// window's environment is known, is localized first (LLP 1123 D5, D16).
    pub(crate) fn localized(&self, env: &Env) -> Option<Dimension> {
        self.tree(|t| {
            t.any(&|n| {
                matches!(
                    n,
                    Node::Env(EnvTerm::Local(_)) | Node::Var(_) | Node::Observe(_)
                ) || matches!(n, Node::Value(_, Unit::Viewport(u)) if u.is_container())
            })
            .then(|| {
                let t = localize(t, env).map(simplify::simplify);
                t.filter(within_expansion)
                    .map_or(Dimension::Auto, dimension)
            })
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
