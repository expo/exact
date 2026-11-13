//! A content owner's local safe-area insets (@ref LLP 1123 D9).
//!
//! A native container that hosts Exact content (a route under a navigation
//! bar, a tab panel above a tab bar, a sheet under its grabber) clears its
//! own chrome, so a box inside it has an inset of its own that `env()`,
//! the document's, does not report. The host says each owner's insets on
//! the owner's root; `var(--exact-safe-area-inset-<edge>)` reads the
//! nearest one at or above a node, as an inherited custom property reads
//! its nearest declaration, and the session root's
//! ([`crate::Env::local`]) where no owner holds the node.
//!
//! An author may declare the property on any node, spelled as it is read
//! (`--exact-safe-area-inset-bottom="0px"`); its subtree then reads that,
//! as CSS Variables 1 inherits a custom property's computed value. On an
//! owner's root the authored value stands over the host's, which stands
//! over the parent's.
//!
//! The values change no layout by themselves: a length that reads one is
//! resolved per node where its engine style is derived, so the bars' room
//! is a length an author can pad with, or pull a background under. The
//! owner's own clearance of its chrome (D3) is the covers' (`cover.rs`).

use super::Kernel;
use crate::arena::NodeArena;
use crate::error::{KernelError, LayoutError};
use crate::generated::{StyleId, StyleProps};
use crate::id::{NodeFlags, ViewId};
use crate::layout::LayoutMirror;
use crate::style::{Dimension, Edge, Env, RowValue};
use std::borrow::Cow;

/// The rows an author overrides the four insets with, top right bottom left.
pub(crate) const OVERRIDES: [StyleId; 4] = [
    StyleId::LocalInsetTop,
    StyleId::LocalInsetRight,
    StyleId::LocalInsetBottom,
    StyleId::LocalInsetLeft,
];

/// Whether a style holds a length that reads a local inset, its own
/// overrides aside (they read their parent's, [`local_at`]).
pub(crate) fn reads_local(style: &StyleProps) -> bool {
    style.mask.iter().any(|id| {
        !OVERRIDES.contains(&id)
            && matches!(style.get(id), RowValue::Dimension(Dimension::Compare(c)) if c.reads_local())
    })
}

/// The environment `slot`'s lengths, `style`, resolve against: the
/// arena's, unless they read a local inset ([`env_at`]).
pub(crate) fn env_for<'a>(arena: &'a NodeArena, slot: u32, style: &StyleProps) -> Cow<'a, Env> {
    match reads_local(style) {
        true => env_at(arena, slot),
        false => Cow::Borrowed(arena.env()),
    }
}

/// The environment `slot`'s lengths resolve against: the arena's, with the
/// local insets `slot` inherits ([`local_at`]).
pub(crate) fn env_at(arena: &NodeArena, slot: u32) -> Cow<'_, Env> {
    let mut env = arena.env().clone();
    env.local = Some(local_at(arena, slot));
    Cow::Owned(env)
}

impl NodeArena {
    /// The environment a box's lengths resolve against (LLP 1123 D9): the
    /// document's, with the box's own local insets where its style reads
    /// one. Every reader of a box's style rows takes this, never
    /// [`NodeArena::env`]: the document's has no local insets, and a
    /// length that reads one there would take the session root's under a
    /// route's bar (a debug build stops at it, `Env::local_inset`).
    pub fn env_of(&self, slot: u32) -> Cow<'_, Env> {
        env_for(self, slot, &self.effective_style(slot))
    }
}

/// The four insets at `slot`, top right bottom left (LLP 1123 D9): an
/// override `slot` declares, else its owner's host value when `slot` is an
/// owner's root, else its parent's; at a root, the session root's. An
/// override's `var()` reads the edge as `slot` has it: that edge's override
/// on `slot`, else what `slot` inherits. Overrides that read themselves,
/// directly or through one another, form a cycle, invalid at computed-value
/// time, and so unset (CSS Variables 1 §3).
pub(crate) fn local_at(arena: &NodeArena, slot: u32) -> [f32; 4] {
    let base = match arena.blocks.local(slot) {
        Some(host) => host,
        None => match arena.parent(slot) {
            Some(parent) => local_at(arena, parent),
            None => Edge::ALL.map(|e| arena.env().root_inset(e)),
        },
    };
    let style = arena.style(slot);
    if !OVERRIDES.iter().any(|&id| style.mask.has(id)) {
        return base;
    }
    let declared = OVERRIDES.map(|id| match style.get(id) {
        RowValue::Dimension(d) if style.mask.has(id) => Some(d),
        _ => None,
    });
    // Which declared edge reads which, and so, closed, which reads itself
    // through others: a cycle, whose members are unset (CSS Variables 1 §3)
    let mut reads = [[false; 4]; 4];
    for (i, d) in declared.iter().enumerate() {
        if let Some(Dimension::Compare(c)) = d {
            reads[i] = Edge::ALL.map(|e| declared[e as usize].is_some() && c.reads_local_edge(e));
        }
    }
    for k in 0..4 {
        for i in 0..4 {
            for j in 0..4 {
                reads[i][j] |= reads[i][k] && reads[k][j];
            }
        }
    }
    let mut out: [Option<f32>; 4] = [None; 4];
    for i in 0..4 {
        resolve(i, &declared, &reads, base, arena.env(), &mut out);
    }
    out.map(|v| v.unwrap_or(0.0))
}

/// Edge `i`'s value at a node: its override, read against the node's own
/// values of the edges it references (resolved first, the graph being
/// acyclic past its cycles), else what the node inherits (`base`), which a
/// cycle's member takes too.
fn resolve(
    i: usize,
    declared: &[Option<Dimension>; 4],
    reads: &[[bool; 4]; 4],
    base: [f32; 4],
    env: &Env,
    out: &mut [Option<f32>; 4],
) -> f32 {
    if let Some(v) = out[i] {
        return v;
    }
    let value = match &declared[i] {
        Some(d) if !reads[i][i] => {
            let mut local = base;
            for (j, v) in local.iter_mut().enumerate() {
                if j != i && reads[i][j] {
                    *v = resolve(j, declared, reads, base, env, out);
                }
            }
            let mut env = env.clone();
            env.local = Some(local);
            d.at_basis(&env, 0.0)
        }
        _ => base[i],
    };
    out[i] = Some(value);
    value
}

/// A box moved to another parent, created ones included, may be in another
/// owner now: what in its subtree reads a local inset is derived again.
/// Whether anything was.
pub(crate) fn moved(arena: &mut NodeArena, layout: &mut dyn LayoutMirror, slot: u32) -> bool {
    if !arena.blocks.has_locals() {
        return false;
    }
    let mut any = false;
    for s in arena.subtree(slot) {
        if !reads_local(&arena.effective_style(s)) {
            continue;
        }
        if let Some(node) = arena.taffy(s) {
            layout.restyle(arena, s, node);
            layout.mark_dirty(node);
        }
        arena.flags_mut(s).insert(NodeFlags::STYLE_DIRTY);
        any = true;
    }
    any
}

impl Kernel {
    /// Set or clear (`None`) the local insets of the content owner whose
    /// root is `view`, top right bottom left in points: what its container's
    /// chrome takes of its box, which `var(--exact-safe-area-inset-*)`
    /// reads in its subtree (LLP 1123 D9). Re-derives and dirties exactly the
    /// nodes there whose style reads one; whether any did (a layout is owed
    /// then). Nonfinite or negative insets are refused.
    pub fn set_local_insets(
        &mut self,
        view: ViewId,
        insets: Option<[f32; 4]>,
    ) -> Result<bool, KernelError> {
        let slot = self
            .arena
            .slot_of(view)
            .ok_or(LayoutError::UnknownView(view))?;
        if !insets
            .unwrap_or([0.0; 4])
            .iter()
            .all(|v| v.is_finite() && *v >= 0.0)
        {
            return Err(LayoutError::InvalidEnv.into());
        }
        if self.arena.blocks.local(slot) == insets {
            return Ok(false);
        }
        self.arena.blocks.set_local(slot, insets);
        let readers: Vec<u32> = (self.arena.iter_live())
            .filter(|&s| s == slot || self.arena.is_ancestor(slot, s))
            .filter(|&s| reads_local(&self.arena.effective_style(s)))
            .collect();
        self.restyle_readers(&readers);
        Ok(!readers.is_empty())
    }

    /// Set the session root's local insets (`None`: the safe-area insets,
    /// `env()`'s), which a node outside every owner reads: the insets of
    /// the area the document covers (LLP 1123 D9). A host whose document
    /// covers its view while `env()` is 0 sets them to the view's. Returns
    /// whether a layout is owed, as [`Kernel::set_env`] does.
    pub fn set_root_local_insets(&mut self, insets: Option<[f32; 4]>) -> Result<bool, KernelError> {
        let mut next = self.arena.env().clone();
        next.root_local = insets;
        if !next.is_finite() || insets.is_some_and(|l| l.iter().any(|v| *v < 0.0)) {
            return Err(LayoutError::InvalidEnv.into());
        }
        self.replace_env(next)
    }

    /// What `var(--exact-safe-area-inset-*)` reads at `view`, top right
    /// bottom left: its owner's local insets, or the session root's.
    pub fn local_insets(&self, view: ViewId) -> Option<[f32; 4]> {
        let slot = self.arena.slot_of(view)?;
        let env = env_at(&self.arena, slot);
        Some(Edge::ALL.map(|e| env.local_inset(e)))
    }

    /// The environment `view`'s lengths resolve against: the page's, with
    /// its owner's local insets (LLP 1123 D9). A host that resolves a row
    /// itself reads this one.
    pub fn env_of(&self, view: ViewId) -> Env {
        match self.arena.slot_of(view) {
            Some(slot) => env_at(&self.arena, slot).into_owned(),
            // A node gone (a leaving view's last style) reads the root's
            None => Env {
                local: Some(Edge::ALL.map(|e| self.arena.env().root_inset(e))),
                ..self.env()
            },
        }
    }

    fn restyle_readers(&mut self, readers: &[u32]) {
        if readers.is_empty() {
            return;
        }
        if let Some(r) = &mut self.region {
            r.invalidate();
        }
        for &slot in readers {
            if let (Some(node), Some(layout)) = (self.arena.taffy(slot), self.layout.as_deref_mut())
            {
                layout.restyle(&self.arena, slot, node);
                layout.mark_dirty(node);
            }
            self.arena.flags_mut(slot).insert(NodeFlags::STYLE_DIRTY);
        }
        // A replayed row checks its styles, not what they now resolve to
        if let Some(tree) = self.layout.as_deref_mut().and_then(|l| l.tree()) {
            tree.forget_memo();
        }
    }
}

#[cfg(test)]
#[path = "owner_tests.rs"]
mod tests;
