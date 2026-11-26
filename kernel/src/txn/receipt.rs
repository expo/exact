//! What a committed batch tells its readers ([`CommitReceipt`]).

use crate::arena::NodeArena;
use crate::generated::Display;
use crate::id::NodeKey;

/// A destroyed node that leaves with its `-exact-exit-animation` (LLP 1063): the
/// root of a destroyed subtree whose parent survived the batch and that no
/// batch-local creation owns, or a virtualized list's row whose item left the
/// data. Its descendants go with it; their own exit rows never play.
#[derive(Debug, Clone, PartialEq)]
pub struct Exit {
    /// The destroyed node; it resolves to nothing after the commit.
    pub key: NodeKey,
    /// The surviving parent it leaves from.
    pub parent: NodeKey,
    /// Its `-exact-exit-animation` row as it was when destroyed.
    pub animations: exact_motion::Animations,
}

/// What a committed batch changed. Every key is generation-checked; a
/// `destroyed` key resolves to nothing after the commit by construction.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct CommitReceipt {
    /// Producer batch number from the frame (0 for structured apply without one).
    pub batch: u64,
    /// Root scope from the frame (0 = whole kernel).
    pub root_id: u32,
    /// The kernel epoch after this commit.
    pub epoch: u64,
    /// Nodes allocated by this batch and still live at its end, in allocation order.
    pub created: Vec<NodeKey>,
    /// Nodes destroyed by this batch (subtrees included), in destruction order.
    pub destroyed: Vec<NodeKey>,
    /// Live nodes whose props, style, or children changed (created nodes excluded).
    pub touched: Vec<NodeKey>,
    /// The edited field whose action assigned its existing bound text (LLP 1102 §3.16).
    pub reconcile_field: Option<NodeKey>,
    /// Whether any change can move geometry; a host that only paints may skip layout otherwise.
    pub layout_invalidated: bool,
    /// The destroyed nodes that play an exit before a host removes them.
    pub exits: Vec<Exit>,
    /// Shared-element names handed from a destroyed node to a created one
    /// (LLP 1013.000 D3).
    pub handoffs: Vec<crate::Handoff>,
    /// Live nodes whose `display` changed: their descendants' animations
    /// are cancelled or restarted (LLP 1055.000 D15; CSS Animations 1 §3).
    pub display_changed: Vec<NodeKey>,
    /// Live nodes whose subtree may be rendered otherwise than before the
    /// batch: each one a children op detached and kept, each one it brought
    /// from elsewhere into or out of being rendered (any it brought once the
    /// batch had changed what renders), and roots attached; a leaf the batch
    /// made is synced whole as created instead. Motion observes the clips
    /// below each as the commit leaves them (LLP 1003.003 D6); one that stays
    /// rendered keeps what runs, as DOM's `moveBefore()` keeps state. Sorted,
    /// without duplicates.
    pub reparented: Vec<NodeKey>,
    /// Consumers whose `animation-timeline` name resolves to another
    /// timeline after this commit, beyond those it created or touched (LLP
    /// 1057.003 D4): a node inserted, removed or moved, or a row set on
    /// another node, can change what a name finds.
    pub timelines: Vec<NodeKey>,
    /// Live nodes a producer reused for a new mount (LLP 1078: a list row
    /// rebound to another item), whatever else the batch did to them: to
    /// motion and to a host's per-node state each is a node destroyed and
    /// created by this commit (no transition from what it showed, its
    /// animations from their start). The kernel never fills it; the producer
    /// that rebinds does.
    pub renewed: Vec<NodeKey>,
    /// List rows that showed in their port for the first time since they
    /// were mounted out of it ([`crate::Kernel::reveal`]; LLP 1055 D13):
    /// the animations below them that waited for that start now. The
    /// producer that mounts rows fills it, as it does `renewed`.
    pub revealed: Vec<NodeKey>,
}

/// Whether `slot` renders: no `display: none` on it or an ancestor, and a
/// root above it (CSS Transitions 2 §3, LLP 1003.003 D6).
pub(super) fn rendered(arena: &NodeArena, mut slot: u32) -> bool {
    loop {
        if arena.style(slot).display == Display::None {
            return false;
        }
        match arena.parent(slot) {
            Some(parent) => slot = parent,
            None => return arena.is_root(slot),
        }
    }
}
