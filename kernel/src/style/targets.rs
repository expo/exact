//! The native targets a box reads raw safe-area insets from (@ref LLP 1123
//! D9, "The native chain"): its owner's containment chain, the relational
//! targets beside it, and the targets native code registers. A raw
//! observation is the target view's `safeAreaInsets` in its own
//! coordinates, as the host reports it, so it does not depend on where
//! the reading box is laid out.

use std::collections::BTreeMap;
use std::sync::Arc;

/// Which view an observation reads (D10's leaf target).
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Target {
    /// The box itself: its incoming safe area, measured on its final box
    /// (`--exact-safe-area-inset-*`), or its keyboard readings
    /// (`--exact-keyboard-inset-*`).
    Box,
    /// The `n`th entry of the native chain, from 1
    /// (`--exact-native-<n>-safe-area-inset-*`).
    Chain(u8),
    /// The nearest chain entry of a kind (`--exact-<kind>-safe-area-inset-*`).
    Kind(Kind),
    /// A relational target (`--exact-<relation>-safe-area-inset-*`).
    Relation(Relation),
    /// A target native code registered by name
    /// (`--exact-target-<name>-safe-area-inset-*`).
    Registered(Arc<str>),
}

impl Target {
    /// The target that the part of a name between `--exact-` and its
    /// family names: `native-<n>` for a chain entry from 1 to 255, a kind,
    /// a relation, or `target-<name>` for a registered one; `None` for any
    /// other part.
    pub fn named(part: &str) -> Option<Target> {
        if let Some(digits) = part.strip_prefix("native-") {
            // As written: `native-02` and `native-+2` are no names
            let n = digits.parse::<u8>().ok();
            return n
                .filter(|&n| n >= 1 && n.to_string() == digits)
                .map(Target::Chain);
        }
        if let Some(name) = part.strip_prefix("target-") {
            return valid_name(name).then(|| Target::Registered(name.into()));
        }
        (Kind::ALL
            .into_iter()
            .find(|k| k.name() == part)
            .map(Target::Kind))
        .or_else(|| {
            (Relation::ALL.into_iter())
                .find(|r| r.name() == part)
                .map(Target::Relation)
        })
    }

    /// The custom property's name up to its edge, as an author writes it:
    /// `--exact-native-2-safe-area-inset-` for the second chain entry's
    /// safe area.
    pub fn prefix(&self, family: super::calc::Family, out: &mut String) {
        out.push_str("--exact-");
        match self {
            Target::Box => {}
            Target::Chain(n) => {
                out.push_str("native-");
                out.push_str(&n.to_string());
                out.push('-');
            }
            Target::Kind(kind) => {
                out.push_str(kind.name());
                out.push('-');
            }
            Target::Relation(relation) => {
                out.push_str(relation.name());
                out.push('-');
            }
            Target::Registered(name) => {
                out.push_str("target-");
                out.push_str(name);
                out.push('-');
            }
        }
        out.push_str(family.infix());
    }
}

/// A chain entry's kind: `owner` and `parent` by position, the rest by the
/// controller the entry is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Kind {
    /// Entry 1, the owner's controller (`viewController.view`).
    Owner,
    /// Entry 2, the owner's controller's `parent`.
    Parent,
    /// `navigationController`: the navigation controller holding the owner,
    /// UIKit's More navigation controller while a tab's routes are there.
    Navigation,
    /// `tabBarController`.
    Tab,
    /// `splitViewController`.
    Split,
    /// The split view controller's column holding the owner.
    Column,
}

impl Kind {
    /// Every kind, in wire order.
    pub const ALL: [Kind; 6] = [
        Kind::Owner,
        Kind::Parent,
        Kind::Navigation,
        Kind::Tab,
        Kind::Split,
        Kind::Column,
    ];

    /// Its name in `--exact-<kind>-safe-area-inset-*`.
    pub fn name(self) -> &'static str {
        match self {
            Kind::Owner => "owner",
            Kind::Parent => "parent",
            Kind::Navigation => "navigation",
            Kind::Tab => "tab",
            Kind::Split => "split",
            Kind::Column => "column",
        }
    }
}

/// A target related to the owner rather than in its chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Relation {
    /// `presentingViewController`.
    Presenter,
    /// The session view.
    Session,
    /// `window.rootViewController`, whether or not it is in the chain.
    Root,
    /// The window.
    Window,
    /// `UITabBarController.contentLayoutGuide`.
    TabContent,
    /// `NSWindow.contentLayoutRect` (macOS).
    WindowContent,
    /// `NSScreen.safeAreaInsets` (macOS).
    Screen,
}

impl Relation {
    /// Every relation, in wire order.
    pub const ALL: [Relation; 7] = [
        Relation::Presenter,
        Relation::Session,
        Relation::Root,
        Relation::Window,
        Relation::TabContent,
        Relation::WindowContent,
        Relation::Screen,
    ];

    /// Its name in `--exact-<relation>-safe-area-inset-*`.
    pub fn name(self) -> &'static str {
        match self {
            Relation::Presenter => "presenter",
            Relation::Session => "session",
            Relation::Root => "root",
            Relation::Window => "window",
            Relation::TabContent => "tab-content",
            Relation::WindowContent => "window-content",
            Relation::Screen => "screen",
        }
    }
}

/// A native view or controller whose raw safe-area insets a box can read.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct NativeTarget {
    /// Its identity, the host's.
    pub id: u64,
    /// Its generation, which a report of the same target never lowers and
    /// a registration's replacement raises.
    pub generation: u64,
    /// Its `safeAreaInsets`, top right bottom left in its own coordinates;
    /// `None` while it is unavailable (unattached, unconvertible).
    pub insets: Option<[f32; 4]>,
}

/// An entry of the native chain.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ChainEntry {
    /// The controller's view.
    pub target: NativeTarget,
    /// The controller it is, where [`Kind`] names one: `Navigation`, `Tab`,
    /// `Split` or `Column`; `None` for any other.
    pub kind: Option<Kind>,
}

/// An owner's native targets, or the session view's (D2's owner update).
#[derive(Debug, Clone, Default, PartialEq)]
pub struct NativeTargets {
    /// The owner's containment chain from its controller up.
    pub chain: Vec<ChainEntry>,
    /// The relational targets, each at most once.
    pub relations: Vec<(Relation, NativeTarget)>,
}

impl NativeTargets {
    /// What `target` reads here: `None` where it is not one of these, or is
    /// absent or unavailable.
    pub fn insets(&self, target: &Target) -> Option<[f32; 4]> {
        let chain = |i: usize| self.chain.get(i).map(|e| &e.target);
        let found = match target {
            Target::Chain(n) => chain(usize::from(*n).checked_sub(1)?),
            Target::Kind(Kind::Owner) => chain(0),
            Target::Kind(Kind::Parent) => chain(1),
            Target::Kind(kind) => (self.chain.iter())
                .find(|e| e.kind == Some(*kind))
                .map(|e| &e.target),
            Target::Relation(r) => (self.relations.iter())
                .find(|(x, _)| x == r)
                .map(|(_, t)| t),
            Target::Box | Target::Registered(_) => None,
        };
        found.and_then(|t| t.insets)
    }

    /// Every target the report holds.
    pub(crate) fn all(&self) -> impl Iterator<Item = &NativeTarget> {
        (self.chain.iter().map(|e| &e.target)).chain(self.relations.iter().map(|(_, t)| t))
    }

    /// Why the report is refused, if it is; `seen` holds the highest
    /// generation earlier reports gave each target, whether or not they
    /// still hold it.
    pub(crate) fn refusal(&self, seen: &crate::id::IdMap<u64, u64>) -> Option<&'static str> {
        if self.chain.len() > usize::from(u8::MAX) {
            return Some("a native chain longer than 255 entries; numbered names reach 255");
        }
        if (self.chain.iter()).any(|e| matches!(e.kind, Some(Kind::Owner | Kind::Parent))) {
            return Some(
                "a chain entry of kind owner or parent; those are entries 1 and 2 by position",
            );
        }
        let relations = &self.relations;
        if (relations.iter().enumerate())
            .any(|(i, (r, _))| relations[..i].iter().any(|(x, _)| x == r))
        {
            return Some("a relational target reported twice; report each at most once");
        }
        self.all().find_map(|t| {
            let stale = seen.get(&t.id).is_some_and(|&g| t.generation < g);
            insets_refusal(t).or(stale.then_some(STALE))
        })
    }
}

/// The targets native code registered for the session, by name.
pub type Registry = BTreeMap<Arc<str>, NativeTarget>;

/// What a box reads raw insets from: its owner's targets, or the session
/// view's outside every owner, and the session's registered ones.
#[derive(Debug, Clone, PartialEq)]
pub struct Scope {
    /// The owner's targets, or the session view's; `None` where none were
    /// reported.
    pub placed: Option<Arc<NativeTargets>>,
    /// The session's registered targets.
    pub registered: Arc<Registry>,
}

impl Scope {
    /// What `target` reads; `None` where it is absent or unavailable.
    pub fn insets(&self, target: &Target) -> Option<[f32; 4]> {
        match target {
            Target::Registered(name) => self.registered.get(name).and_then(|t| t.insets),
            other => self.placed.as_ref()?.insets(other),
        }
    }
}

const STALE: &str =
    "a stale generation: a report older than one already taken for this target; send its current one";

/// Why `target`'s insets are refused, if they are.
fn insets_refusal(target: &NativeTarget) -> Option<&'static str> {
    (target.insets)
        .is_some_and(|i| !i.iter().all(|v| v.is_finite() && *v >= 0.0))
        .then_some("nonfinite or negative insets; report a target without insets as unavailable")
}

/// Why `target` cannot be registered over `last`, the name's last
/// registration, a removed one included, if it cannot: a generation older
/// than its, or another view at its generation.
pub(crate) fn registration_refusal(
    target: &NativeTarget,
    last: Option<&NativeTarget>,
) -> Option<&'static str> {
    insets_refusal(target).or_else(|| {
        let last = last?;
        if target.generation < last.generation {
            return Some(STALE);
        }
        (target.id != last.id && target.generation == last.generation).then_some(
            "another view at the registration's generation; a replacement takes a newer one",
        )
    })
}

/// Whether `name` can name a registered target: ASCII lowercase letters,
/// digits and inner hyphens, as its custom property name spells it.
pub fn valid_name(name: &str) -> bool {
    let inner = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-';
    !name.is_empty()
        && name.len() <= 64
        && name.chars().all(inner)
        && !name.starts_with('-')
        && !name.ends_with('-')
}
