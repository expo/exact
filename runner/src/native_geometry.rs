//! Geometry facts (LLP 1123 D6): `exactGeometry(id[, space[, node[, regions]]])`,
//! `exactNativeChain(id)` and `exactSafeAreaUnsettled()`, reserved sources answered from the
//! layout just committed and answered again after a later layout changes what they read, so a
//! branch on one lays out in a later commit and never inside the layout it read.
//!
//! A fact is a tree of named quantities ([`Fact`]) that the runner projects onto the shape the
//! resource declares: the shape chooses what is read, a quantity the host does not have is
//! `none` in an `option` field (never 0), and a field the source does not have is refused.

use crate::geometry::view_of;
use crate::DataError;
use exact_kernel::style::calc::Family;
use exact_kernel::style::targets::{Kind, NativeTarget};
use exact_kernel::{Kernel, OwnerChrome, ViewId};
use exact_plan::{Plan, TypeKind, TypesId, Value};

/// `exactGeometry(id[, space[, node[, regions]]])`, whose `regions` names the query options of
/// the `regions` field, which no host reports yet.
pub const GEOMETRY: &str = "exactGeometry";
/// `exactNativeChain(id)`.
pub const CHAIN: &str = "exactNativeChain";
/// `exactSafeAreaUnsettled()`.
pub const UNSETTLED: &str = "exactSafeAreaUnsettled";

/// What `exactGeometry` answers, filled by name.
pub const GEOMETRY_FIELDS: &[&str] = &[
    "available",
    "generation",
    "size",
    "frame",
    "observations",
    "cornerInsets",
    "cornerRadii",
    "margins",
    "regions",
    "scroll",
    "keyboard",
    "chrome",
];
/// What `exactNativeChain` answers.
pub const CHAIN_FIELDS: &[&str] = &["available", "generation", "entries"];
/// What `exactSafeAreaUnsettled` answers: how many boxes did not settle, and the `id`s of
/// those that have one.
pub const UNSETTLED_FIELDS: &[&str] = &["available", "generation", "count", "ids"];
/// Each geometry source with what it answers.
pub const SOURCES: [(&str, &[&str]); 3] = [
    (GEOMETRY, GEOMETRY_FIELDS),
    (CHAIN, CHAIN_FIELDS),
    (UNSETTLED, UNSETTLED_FIELDS),
];

/// Whether `source` is a geometry source.
pub fn is_geometry(source: &str) -> bool {
    SOURCES.iter().any(|(s, _)| *s == source)
}

/// A quantity a fact holds, named where it is a record's.
#[derive(Debug, Clone, PartialEq)]
pub enum Fact {
    /// A length, a coordinate or a count.
    Number(f64),
    /// A flag.
    Bool(bool),
    /// A name: a node's `id`.
    Str(String),
    /// Named quantities, filled into a record by name.
    Record(Vec<(&'static str, Fact)>),
    /// Quantities in order.
    List(Vec<Fact>),
    /// What the host does not have: `none` where the shape declares an `option`.
    Absent,
}

impl Fact {
    fn rect(x: f64, y: f64, width: f64, height: f64) -> Fact {
        Fact::Record(vec![
            ("x", Fact::Number(x)),
            ("y", Fact::Number(y)),
            ("width", Fact::Number(width)),
            ("height", Fact::Number(height)),
        ])
    }

    fn edges([top, right, bottom, left]: [bool; 4]) -> Fact {
        Fact::Record(vec![
            ("top", Fact::Bool(top)),
            ("right", Fact::Bool(right)),
            ("bottom", Fact::Bool(bottom)),
            ("left", Fact::Bool(left)),
        ])
    }

    fn insets([top, right, bottom, left]: [f64; 4]) -> Fact {
        Fact::Record(vec![
            ("top", Fact::Number(top)),
            ("right", Fact::Number(right)),
            ("bottom", Fact::Number(bottom)),
            ("left", Fact::Number(left)),
        ])
    }
}

/// `fact` as the value of type `ty`, `source` naming it in a refusal: each record field by its
/// name, an absent quantity `none` where the type is an `option`.
pub fn project(plan: &Plan, ty: TypesId, fact: &Fact, source: &str) -> Result<Value, DataError> {
    project_at(plan, ty, fact, source, "")
}

/// [`project`] of the quantity at `path`, the fields above it dotted.
fn project_at(
    plan: &Plan,
    ty: TypesId,
    fact: &Fact,
    source: &str,
    path: &str,
) -> Result<Value, DataError> {
    let t = plan.type_(ty);
    let at = if path.is_empty() { "the answer" } else { path };
    let wrong = |what: &str| {
        DataError::Unavailable(format!(
            "{source} answers {what} for `{at}`, where the shape declares `{}`",
            plan.str(t.name)
        ))
    };
    match (t.kind, fact) {
        (TypeKind::Option, Fact::Absent) => Ok(Value::NONE),
        (TypeKind::Option, fact) => {
            let elem = t.elem.ok_or_else(|| wrong("a value"))?;
            Ok(Value::some(project_at(plan, elem, fact, source, path)?))
        }
        (_, Fact::Absent) => Err(DataError::Unavailable(format!(
            "{source} has no `{at}` before the node is laid out or where the host does not have it, so it answers `none` there: declare `{at}` as an `option`"
        ))),
        (TypeKind::Number, Fact::Number(n)) => Ok(Value::Number(*n)),
        (TypeKind::Bool, Fact::Bool(b)) => Ok(Value::Bool(*b)),
        (TypeKind::String, Fact::Str(s)) => Ok(Value::str(s)),
        (TypeKind::List, Fact::List(items)) => {
            let elem = t.elem.ok_or_else(|| wrong("a list"))?;
            let items = items.iter().map(|f| project_at(plan, elem, f, source, path));
            Ok(Value::list(items.collect::<Result<_, _>>()?))
        }
        (TypeKind::Record, Fact::Record(named)) => {
            let fields = t.fields.iter().map(|f| {
                let field = plan.field(f);
                let name = plan.str(field.name);
                let inner = if path.is_empty() { name.to_string() } else { format!("{path}.{name}") };
                let Some((_, value)) = named.iter().find(|(n, _)| *n == name) else {
                    let has: Vec<&str> = named.iter().map(|(n, _)| *n).collect();
                    return Err(DataError::Unavailable(format!(
                        "{source} has no `{inner}`; {at} has {}",
                        has.join(", ")
                    )));
                };
                project_at(plan, field.ty, value, source, &inner)
            });
            Ok(Value::record(fields.collect::<Result<_, _>>()?))
        }
        (_, Fact::Number(_)) => Err(wrong("a number")),
        (_, Fact::Bool(_)) => Err(wrong("a boolean")),
        (_, Fact::Str(_)) => Err(wrong("a string")),
        (_, Fact::List(_)) => Err(wrong("a list")),
        (_, Fact::Record(_)) => Err(wrong("a record")),
    }
}

/// What a geometry source's arguments ask.
#[derive(Debug, Clone, PartialEq)]
pub enum Query {
    /// `exactGeometry`: the node `id` names, in `space`.
    Geometry {
        /// The node's `id`.
        id: String,
        /// The space its frames are in.
        space: Space,
    },
    /// `exactNativeChain`: the targets of the owner of the node `id` names.
    Chain {
        /// The node's `id`.
        id: String,
    },
    /// `exactSafeAreaUnsettled`.
    Unsettled,
}

/// The space a geometry read's frames are in: its owner's, the window's, or a node's border box.
#[derive(Debug, Clone, PartialEq)]
pub enum Space {
    /// The box's owner's, as the kernel lays it out; the document's outside every owner.
    Owner,
    /// The window's, where the host shows the document and its owners.
    Window,
    /// The border box of the node this `id` names.
    Node(String),
}

/// What `source`'s arguments ask, or why they cannot: `exactGeometry` takes the node's `id`, a
/// space, the `id` of a `"node"` space's node, and UIKit's region query options; the chain
/// takes an owner's node `id`, and the unsettled list nothing.
pub fn query(source: &str, args: &[Value]) -> Result<Query, DataError> {
    let most = match source {
        GEOMETRY => 4,
        CHAIN => 1,
        _ => 0,
    };
    if args.len() > most {
        return Err(DataError::BadArguments(format!(
            "{source} takes at most {most} arguments, not {}: remove the extra ones",
            args.len()
        )));
    }
    let text = |n: usize, what: &str| match args.get(n) {
        None => Ok(None),
        Some(v) => v.as_str().map(Some).ok_or_else(|| {
            DataError::BadArguments(format!(
                "{source}'s {what} is a string, not {v:?}: pass it as text"
            ))
        }),
    };
    let id = |what: &str| -> Result<String, DataError> {
        match text(0, what)? {
            Some(id) if !id.is_empty() => Ok(id.to_string()),
            _ => Err(DataError::BadArguments(format!(
                "{source} takes the `id` of {what} first, as `{source}(\"hero\")`"
            ))),
        }
    };
    match source {
        GEOMETRY => {
            let id = id("the node it reads")?;
            let space = match (text(1, "space")?.unwrap_or("owner"), text(2, "node")?) {
                ("owner", _) => Space::Owner,
                ("window", _) => Space::Window,
                ("node", Some(node)) if !node.is_empty() => Space::Node(node.to_string()),
                ("node", _) => {
                    return Err(DataError::BadArguments(format!(
                        "{GEOMETRY}'s space \"node\" needs the `id` of the node whose border box it is, as its third argument"
                    )))
                }
                (other, _) => {
                    return Err(DataError::BadArguments(format!(
                        "{GEOMETRY}'s space is \"owner\", \"window\" or \"node\", not \"{other}\""
                    )))
                }
            };
            let regions = text(3, "regions")?.unwrap_or("");
            if let Some(bad) = regions
                .split_whitespace()
                .find(|o| !["inactive", "mirrored"].contains(o))
            {
                return Err(DataError::BadArguments(format!(
                    "{GEOMETRY}'s regions are \"inactive\", \"mirrored\" or both, space-separated, not \"{bad}\""
                )));
            }
            Ok(Query::Geometry { id, space })
        }
        CHAIN => id("the node whose owner's chain it reads").map(|id| Query::Chain { id }),
        _ => Ok(Query::Unsettled),
    }
}

/// What `exactGeometry` reads of the node `id` names, `generation` the number of the layout it
/// reads. A node that is gone or was not laid out is unavailable, with nothing else; a frame
/// the kernel cannot place in `space` is `none`, as are the rectangles in it.
pub fn geometry(kernel: &Kernel, id: &str, space: &Space, generation: u64) -> Fact {
    let Some(laid) = view_of(kernel, id).and_then(|view| laid_out(kernel, view)) else {
        return unavailable(generation);
    };
    let observed = kernel.observations_now(laid.view).unwrap_or_default();
    let reading = |family: Family| -> Option<[f64; 4]> {
        let (_, r) = observed.iter().find(|(f, _)| *f == family)?;
        Some([r[0]? as f64, r[1]? as f64, r[2]? as f64, r[3]? as f64])
    };
    answer(
        generation,
        laid.size,
        frame_in(kernel, &laid, space),
        reading,
        keyboard_fact(kernel, &laid, space),
        chrome_fact(kernel.owner_chrome(laid.view)),
    )
}

/// What `exactSafeAreaUnsettled` reads: how many boxes the last layout left unsettled, readers,
/// boxes whose taking of the band did not settle and parents whose bars' slots did not, and the
/// `id` of each that has one. A
/// kernel that has not laid out, as on the web's wasm target, measured none: unavailable.
pub fn unsettled(kernel: &Kernel, generation: u64) -> Fact {
    let boxes = kernel.safe_area_unsettled();
    let ids = boxes.iter().filter_map(|&view| {
        let id = kernel.node(view)?.props.str(exact_kernel::PropId::Id)?;
        Some(Fact::Str(id.to_string()))
    });
    unsettled_fact(kernel.layouts() > 0, generation, boxes.len(), ids.collect())
}

/// What `exactNativeChain` reads of the node `id` names (D6, D9): the native chain of its owner,
/// or the session view's outside every owner, then the relational targets beside it, as the host
/// last reported them. A node that is gone, or whose holder's host reported no targets, is
/// unavailable.
pub fn chain(kernel: &Kernel, id: &str, generation: u64) -> Fact {
    let Some(targets) = view_of(kernel, id).and_then(|view| kernel.native_targets(view)) else {
        return chain_fact(false, generation, Fact::Absent);
    };
    let numbered = (targets.chain.iter().zip(1..))
        .map(|(e, number)| target_fact(Some(number), None, e.kinds, &e.target));
    let related = (targets.relations.iter()).map(|(r, t)| target_fact(None, Some(r.name()), 0, t));
    chain_fact(
        true,
        generation,
        Fact::List(numbered.chain(related).collect()),
    )
}

fn chain_fact(available: bool, generation: u64, entries: Fact) -> Fact {
    Fact::Record(vec![
        ("available", Fact::Bool(available)),
        ("generation", Fact::Number(generation as f64)),
        ("entries", entries),
    ])
}

/// One target of a chain answer: its number in the chain from 1 (the owner's controller) or its
/// relation's name, its kinds' names, its identity and generation, its frame in its window, and
/// its raw insets in its own coordinates; the frame and insets `none` while it is unavailable.
fn target_fact(number: Option<u32>, relation: Option<&str>, kinds: u8, t: &NativeTarget) -> Fact {
    let kinds = (Kind::ALL.iter())
        .filter(|k| kinds & k.bit() != 0)
        .map(|k| Fact::Str(k.name().to_string()));
    let four = |v: Option<[f32; 4]>| v.map(|v| v.map(f64::from));
    Fact::Record(vec![
        (
            "number",
            number.map_or(Fact::Absent, |n| Fact::Number(n.into())),
        ),
        (
            "relation",
            relation.map_or(Fact::Absent, |r| Fact::Str(r.to_string())),
        ),
        ("kinds", Fact::List(kinds.collect())),
        ("id", Fact::Number(t.id as f64)),
        ("generation", Fact::Number(t.generation as f64)),
        (
            "frame",
            four(t.frame).map_or(Fact::Absent, |[x, y, w, h]| Fact::rect(x, y, w, h)),
        ),
        ("insets", four(t.insets).map_or(Fact::Absent, Fact::insets)),
    ])
}

/// The answers `source` can give, for the bake to project a declared shape onto: every quantity
/// this stage answers present, every one absent, and unavailable. A shape one of them cannot
/// fill would be refused at run time. The quantities only a host has are absent in each, so
/// their fields must be `option`s, and what is inside one is checked once a host reports it.
pub fn templates(source: &str) -> Vec<Fact> {
    match source {
        GEOMETRY => vec![
            answer(
                0,
                (1.0, 1.0),
                Some([0.0, 0.0, 1.0, 1.0]),
                |_| Some([0.0; 4]),
                Fact::Record(vec![
                    ("guide", Fact::rect(0.0, 0.0, 0.0, 0.0)),
                    ("occupancy", Fact::rect(0.0, 0.0, 0.0, 0.0)),
                    ("docked", Fact::Bool(false)),
                    ("near", Fact::edges([false; 4])),
                    ("away", Fact::edges([false; 4])),
                ]),
                Fact::Record(vec![
                    ("verticalBarEdge", Fact::Str(String::new())),
                    ("sidebarAvailable", Fact::Bool(false)),
                    ("overscan", Fact::Str(String::new())),
                    ("auxiliaryTopLeftArea", Fact::rect(0.0, 0.0, 0.0, 0.0)),
                    ("auxiliaryTopRightArea", Fact::rect(0.0, 0.0, 0.0, 0.0)),
                ]),
            ),
            answer(
                0,
                (1.0, 1.0),
                None,
                |_| None,
                Fact::Record(vec![
                    ("guide", Fact::Absent),
                    ("occupancy", Fact::Absent),
                    ("docked", Fact::Absent),
                    ("near", Fact::Absent),
                    ("away", Fact::Absent),
                ]),
                chrome_fact(Some(Default::default())),
            ),
            answer(0, (1.0, 1.0), None, |_| None, Fact::Absent, Fact::Absent),
            unavailable(0),
        ],
        CHAIN => {
            let target = NativeTarget {
                id: 1,
                generation: 1,
                insets: Some([0.0; 4]),
                frame: Some([0.0; 4]),
            };
            let none = NativeTarget {
                insets: None,
                frame: None,
                ..target
            };
            vec![
                chain_fact(
                    true,
                    0,
                    Fact::List(vec![target_fact(
                        Some(1),
                        None,
                        Kind::Navigation.bit(),
                        &target,
                    )]),
                ),
                chain_fact(
                    true,
                    0,
                    Fact::List(vec![target_fact(None, Some("window"), 0, &none)]),
                ),
                chain_fact(false, 0, Fact::Absent),
            ]
        }
        _ => vec![unsettled_fact(true, 0, 1, vec![Fact::Str(String::new())])],
    }
}

/// An `exactGeometry` answer for a laid-out box of `size`, `frame` in the space read, what it
/// observes of each family, its `keyboard` and its owner's `chrome`.
fn answer(
    generation: u64,
    (width, height): (f64, f64),
    frame: Option<[f64; 4]>,
    reading: impl Fn(Family) -> Option<[f64; 4]>,
    keyboard: Fact,
    chrome: Fact,
) -> Fact {
    let observations = Family::ALL
        .iter()
        .map(|&f| (name(f), reading(f).map_or(Fact::Absent, Fact::insets)))
        .collect();
    // D8's rectangles: the box's border box inset by each region's reading
    let inset = |family: Family| match (frame, reading(family)) {
        (Some([x, y, width, height]), Some([t, r, b, l])) => Fact::rect(
            x + l,
            y + t,
            (width - l - r).max(0.0),
            (height - t - b).max(0.0),
        ),
        _ => Fact::Absent,
    };
    Fact::Record(vec![
        ("available", Fact::Bool(true)),
        ("generation", Fact::Number(generation as f64)),
        (
            "size",
            Fact::Record(vec![
                ("width", Fact::Number(width)),
                ("height", Fact::Number(height)),
            ]),
        ),
        (
            "frame",
            frame.map_or(Fact::Absent, |[x, y, w, h]| Fact::rect(x, y, w, h)),
        ),
        ("observations", Fact::Record(observations)),
        (
            "margins",
            Fact::Record(vec![
                ("layoutMargins", inset(Family::LayoutMargins)),
                ("readableContent", inset(Family::ReadableContent)),
                ("readableContentGuide", inset(Family::ReadableContentGuide)),
            ]),
        ),
        // What only the host has, until it reports it (LLP 1123 stage 3)
        ("cornerInsets", Fact::Absent),
        ("cornerRadii", Fact::Absent),
        ("regions", Fact::Absent),
        ("scroll", Fact::Absent),
        ("keyboard", keyboard),
        ("chrome", chrome),
    ])
}

/// D7's keyboard facts for `laid` in `space`: its guide and its owner's occupancy, each `none`
/// where the kernel cannot place it there, and whether a docked keyboard shows; `none` where the
/// host reported nothing of the keyboard. The tracking state, `near` and `away`, is `none` until a
/// host tracks it (`UITrackingLayoutGuide`).
fn keyboard_fact(kernel: &Kernel, laid: &Laid, space: &Space) -> Fact {
    let Some(k) = kernel.keyboard_facts(laid.view) else {
        return Fact::Absent;
    };
    let rect = |r: [f32; 4]| match rect_in(kernel, laid, space, r.map(f64::from)) {
        Some([x, y, w, h]) => Fact::rect(x, y, w, h),
        None => Fact::Absent,
    };
    Fact::Record(vec![
        ("guide", k.guide.map_or(Fact::Absent, rect)),
        ("occupancy", rect(k.occupancy)),
        ("docked", Fact::Bool(k.docked)),
        ("near", Fact::Absent),
        ("away", Fact::Absent),
    ])
}

/// D6's `chrome`: what the box's owner's host reported of it, `none` where it reported none, as
/// outside every owner. A quantity the host does not have is `none`, as the tvOS overscan and the
/// Mac's auxiliary areas are until those hosts report them (stage 4).
fn chrome_fact(chrome: Option<OwnerChrome>) -> Fact {
    let Some(c) = chrome else {
        return Fact::Absent;
    };
    let edge = c.vertical_bar_edge.map(|e| Fact::Str(e.name().to_string()));
    Fact::Record(vec![
        ("verticalBarEdge", edge.unwrap_or(Fact::Absent)),
        (
            "sidebarAvailable",
            c.sidebar_available.map_or(Fact::Absent, Fact::Bool),
        ),
        ("overscan", Fact::Absent),
        ("auxiliaryTopLeftArea", Fact::Absent),
        ("auxiliaryTopRightArea", Fact::Absent),
    ])
}

/// An `exactGeometry` answer for a node that is gone or was not laid out.
fn unavailable(generation: u64) -> Fact {
    Fact::Record(
        GEOMETRY_FIELDS
            .iter()
            .map(|&name| match name {
                "available" => (name, Fact::Bool(false)),
                "generation" => (name, Fact::Number(generation as f64)),
                _ => (name, Fact::Absent),
            })
            .collect(),
    )
}

fn unsettled_fact(available: bool, generation: u64, count: usize, ids: Vec<Fact>) -> Fact {
    Fact::Record(vec![
        ("available", Fact::Bool(available)),
        ("generation", Fact::Number(generation as f64)),
        ("count", Fact::Number(count as f64)),
        ("ids", Fact::List(ids)),
    ])
}

/// The name of `family`'s observation in a geometry fact, as D6 names it.
fn name(family: Family) -> &'static str {
    match family {
        Family::SafeArea => "safeArea",
        Family::Keyboard => "keyboard",
        Family::KeyboardUndocked => "keyboardUndocked",
        Family::SafeAreaCorners => "safeAreaCorners",
        Family::LayoutMargins => "layoutMargins",
        Family::LayoutMarginsCorners => "layoutMarginsCorners",
        Family::ReadableContent => "readableContent",
        Family::ReadableContentCorners => "readableContentCorners",
        Family::ReadableContentGuide => "readableContentGuide",
        Family::SystemMinimum => "systemMinimumLayoutMargins",
        Family::Bar => "bar",
    }
}

/// A box as the last layout left it: its size, and where the kernel places it, the owner whose
/// space its frame is in (`None` for the document's) with its frame's origin there.
struct Laid {
    view: ViewId,
    size: (f64, f64),
    placed: Option<(Option<ViewId>, [f64; 2])>,
}

/// `view` as the last layout left it; `None` where layout gave it no box (not laid out since it
/// was created, under `display: none`, an inline run, or on a kernel that does not lay out).
fn laid_out(kernel: &Kernel, view: ViewId) -> Option<Laid> {
    let key = kernel.arena().key_of(view)?;
    let (f, _) = kernel.laid_out_frame(key)?;
    let placed = kernel
        .frame_space(view)
        .map(|(owner, (dx, dy))| (owner, [f64::from(f.x + dx), f64::from(f.y + dy)]));
    let size = (f64::from(f.width), f64::from(f.height));
    Some(Laid { view, size, placed })
}

/// `laid`'s border box in `space`, `x`, `y`, `width` and `height`. In its owner's space as the
/// kernel lays it out, a top-layer box placed in the viewport; in a node's border box where both
/// are in one space; in the window's through the map its host reported for the box's owner, or
/// for the document outside every owner, the bounds of its mapped corners, and `None` where the
/// host reported none.
fn frame_in(kernel: &Kernel, laid: &Laid, space: &Space) -> Option<[f64; 4]> {
    let (_, [x, y]) = laid.placed?;
    rect_in(kernel, laid, space, [x, y, laid.size.0, laid.size.1])
}

/// `rect`, x y width height in `laid`'s owner's space, in `space`, as [`frame_in`] places
/// `laid`'s own border box.
fn rect_in(
    kernel: &Kernel,
    laid: &Laid,
    space: &Space,
    [x, y, w, h]: [f64; 4],
) -> Option<[f64; 4]> {
    let (owner, _) = laid.placed?;
    match space {
        Space::Owner => Some([x, y, w, h]),
        Space::Window => kernel.window_map(laid.view).map(|m| m.rect([x, y, w, h])),
        Space::Node(id) => {
            let other = view_of(kernel, id).and_then(|view| laid_out(kernel, view))?;
            let (other_owner, [ox, oy]) = other.placed?;
            (other_owner == owner).then_some([x - ox, y - oy, w, h])
        }
    }
}
