//! LLP 1084 D1: a reorder session whose source section already has every row.
use super::reorder_group::Session;
use super::*;
use crate::instance::collection::Offsets;
use crate::list_api::{ReorderBinding, ReorderEnding, ReorderFrame, ReorderGeometry, ReorderPhase};
use exact_kernel::{NodeKey, Op, PropId, StyleId, StyleProps, ViewId};

pub(super) struct MountedSection {
    pub binding: ReorderBinding,
    card: ViewId,
    rows: Vec<Row>,
    gap: Option<usize>,
    measured_height: Option<f64>,
    measured_offset: Option<f64>,
    source: usize,
}
struct Row {
    view: ViewId,
    node: NodeKey,
    key: String,
    original: StyleProps,
    overlay: StyleProps,
    applied: Option<(f64, bool)>,
}

impl Row {
    /// Restore only the properties the preview owned. An authored change made
    /// since the last preview becomes the value to restore.
    fn apply_overlay(&mut self, kernel: &Kernel, next: StyleProps, ops: &mut Vec<Op>) {
        let Some(node) = kernel.node_by_key(self.node) else {
            return;
        };
        let live = node.style;
        let previous = std::mem::replace(&mut self.overlay, next);
        let changed = live.changed_mask(&previous);
        let capture = self
            .overlay
            .mask
            .minus(previous.mask)
            .union(previous.mask.intersect(changed));
        self.original.copy_rows(live, capture);
        self.original.mask = self
            .original
            .mask
            .minus(capture)
            .union(live.mask.intersect(capture));
        self.original.written = self
            .original
            .written
            .minus(capture)
            .union(live.written.intersect(capture));
        let release = previous.mask.minus(self.overlay.mask);
        let mut patch = self.overlay.clone();
        patch.copy_rows(&self.original, release.intersect(self.original.mask));
        patch.written = patch
            .written
            .union(release.intersect(self.original.written));
        let clear = release.minus(self.original.mask);
        for (mask, written) in [
            (clear.minus(self.original.written), false),
            (clear.intersect(self.original.written), true),
        ] {
            if !mask.is_empty() {
                ops.push(Op::ClearStyle {
                    id: self.view,
                    mask,
                    written,
                });
            }
        }
        if !patch.mask.is_empty() {
            ops.push(Op::SetStyle {
                id: self.view,
                patch: Box::new(patch),
            });
        }
    }
}

impl MountedSection {
    pub fn new<D: DataSource>(
        runner: &Runner<D>,
        binding: ReorderBinding,
    ) -> Option<(Self, String)> {
        let list = runner.kernel.node_by_key(binding.list)?.id;
        let source = runner.kernel.node_by_key(binding.root)?;
        let card = source.parent?;
        let mut rows = Vec::new();
        let mut seen = std::collections::HashSet::new();
        for (view, key, _) in runner.grouped_reorder_rows(list, card) {
            if !seen.insert(key.clone()) {
                return None;
            }
            let node = runner.kernel.node(view)?;
            rows.push(Row {
                view,
                node: node.key,
                key,
                original: StyleProps::default(),
                overlay: StyleProps::default(),
                applied: None,
            });
        }
        let at = rows.iter().position(|r| r.view == source.id)?;
        let key = rows[at].key.clone();
        Some((
            Self {
                binding,
                card,
                rows,
                source: at,
                gap: None,
                measured_height: None,
                measured_offset: None,
            },
            key,
        ))
    }

    pub fn before(&self, certified: bool) -> Option<Option<String>> {
        let gap = if certified {
            self.gap?
        } else {
            self.gap.unwrap_or(self.source + 1)
        };
        Some(self.rows.get(gap).map(|r| r.key.clone()))
    }
    fn select_before(&mut self, kernel: &Kernel, before: Option<ViewId>) -> Option<()> {
        let gap = match before {
            Some(view) => {
                if view == self.rows[self.source].view
                    || kernel.node(view)?.parent != Some(self.card)
                {
                    return None;
                }
                if let Some(at) = self.rows.iter().position(|r| r.view == view) {
                    at
                } else {
                    let children = kernel.node(self.card)?.children();
                    let at = children.iter().position(|&id| id == view)?;
                    children[at + 1..]
                        .iter()
                        .find_map(|&id| {
                            self.rows
                                .iter()
                                .position(|r| r.view == id && r.view != self.rows[self.source].view)
                        })
                        .unwrap_or(self.rows.len())
                }
            }
            None => self.rows.len(),
        };
        self.gap = Some(gap);
        Some(())
    }
    /// Keep each row's preview while resolving a fresh step in the current order.
    /// A previously chosen gap follows its successor, including the end of the section.
    pub fn step(&mut self, mut current: Self, by: isize, kernel: &Kernel, ghost: bool) -> Vec<Op> {
        let mut ops = Vec::new();
        let at = match self.gap {
            None => current.source + 1,
            Some(gap) => match self.rows.get(gap) {
                Some(before) => {
                    let Some(index) = current.rows.iter().position(|row| row.node == before.node)
                    else {
                        return ops;
                    };
                    index
                }
                None => current.rows.len(),
            },
        };
        let mut previous: std::collections::HashMap<_, _> =
            self.rows.drain(..).map(|row| (row.node, row)).collect();
        for row in &mut current.rows {
            if let Some(old) = previous.remove(&row.node) {
                *row = old;
            }
        }
        for mut gone in previous.into_values().filter(|row| row.applied.is_some()) {
            gone.apply_overlay(kernel, StyleProps::default(), &mut ops);
        }
        let source = current.source;
        let slot = (at - usize::from(at > source))
            .saturating_add_signed(by)
            .min(current.rows.len() - 1);
        current.gap = Some(slot + usize::from(slot >= source));
        *self = current;
        ops.extend(self.paint(kernel, true, ghost));
        ops
    }
    pub fn preview(&mut self, kernel: &Kernel, y: f64) -> bool {
        let Some(list) = kernel.node_by_key(self.binding.list) else {
            return false;
        };
        let source = self.source;
        let gap = self.rows.iter().enumerate().find_map(|(i, r)| {
            let f = kernel.node(r.view)?.frame;
            (i != source && y < f64::from(f.y - list.frame.y + f.height / 2.)).then_some(i)
        });
        self.gap = Some(gap.unwrap_or(self.rows.len()));
        true
    }

    pub fn paint(&mut self, kernel: &Kernel, active: bool, ghost: bool) -> Vec<Op> {
        let source = self.source;
        let Some(frame) = kernel.node(self.rows[source].view).map(|n| n.frame) else {
            return self.restore(kernel);
        };
        let before = self.gap.unwrap_or(source + 1);
        let target_y = self
            .rows
            .get(before)
            .and_then(|r| kernel.node(r.view))
            .map(|n| n.frame.y)
            .or_else(|| {
                self.rows
                    .last()
                    .and_then(|r| kernel.node(r.view))
                    .map(|n| n.frame.y + n.frame.height)
            })
            .unwrap_or(frame.y);
        let offsets = Offsets {
            source,
            before,
            height: self.measured_height.unwrap_or(frame.height.into()),
            source_offset: self.measured_offset.unwrap_or_else(|| {
                f64::from(target_y - frame.y - if before > source { frame.height } else { 0. })
            }),
        };
        let mut ops = Vec::new();
        for (i, row) in self.rows.iter_mut().enumerate() {
            let offset = if active { offsets.at(i) } else { 0. };
            let hide = ghost && i == source;
            if row.applied.unwrap_or((0., false)) == (offset, hide)
                || kernel.node_by_key(row.node).is_none()
            {
                continue;
            }
            let mut patch = StyleProps::default();
            if offset != 0. {
                crate::bridge::set_style(
                    &mut patch,
                    StyleId::Translate as u16,
                    &Value::str(&format!("0px {}px", exact_num::Shortest(offset))),
                    0,
                )
                .expect("finite row offset");
                crate::bridge::set_style(
                    &mut patch,
                    StyleId::Transition as u16,
                    &Value::str("translate -exact-spring(300,30,1)"),
                    0,
                )
                .expect("row transition");
            }
            if hide {
                crate::bridge::set_style(
                    &mut patch,
                    StyleId::Visibility as u16,
                    &Value::str("hidden"),
                    0,
                )
                .expect("visibility");
            }
            row.applied = Some((offset, hide));
            row.apply_overlay(kernel, patch, &mut ops);
        }
        ops
    }
    pub fn restore(&mut self, kernel: &Kernel) -> Vec<Op> {
        let mut ops = Vec::new();
        for row in &mut self.rows {
            if row.applied.take().is_some() {
                row.apply_overlay(kernel, StyleProps::default(), &mut ops);
            }
        }
        ops
    }

    pub fn place<D: DataSource>(
        &self,
        r: &Runner<D>,
        key: &str,
    ) -> Option<(NodeKey, Option<String>, Option<String>)> {
        let list = r.kernel.node_by_key(self.binding.list)?.id;
        let rows = r.grouped_reorder_rows(list, self.card);
        let at = rows.iter().position(|(_, k, _)| k == key)?;
        let ident = |n: usize| rows.get(n).map(|(_, k, _)| format!("s:{k}"));
        Some((
            self.binding.list,
            at.checked_sub(1).and_then(ident),
            ident(at + 1),
        ))
    }
    pub fn current<D: DataSource>(&self, r: &Runner<D>) -> bool {
        let Some(list) = r.kernel.node_by_key(self.binding.list) else {
            return false;
        };
        let rows = r.grouped_reorder_rows(list.id, self.card);
        let live = |row: &Row| {
            let mut matches = rows.iter().filter(|(_, key, _)| *key == row.key);
            matches.next().is_some_and(|(id, _, _)| *id == row.view)
                && matches.next().is_none()
                && r.kernel.node_by_key(row.node).is_some()
        };
        live(&self.rows[self.source])
            && self.gap.and_then(|gap| self.rows.get(gap)).is_none_or(live)
            && r.reorder_binding(self.binding.handle) == Some(self.binding)
    }
}

impl<D: DataSource> Runner<D> {
    /// The browser measures this row to place the keyboard preview. At the end of
    /// a section it measures the bottom of the last keyed row.
    pub fn section_step_anchor(&self, token: crate::ReorderToken) -> Option<(ViewId, bool)> {
        let s = self
            .session()
            .filter(|s| s.token == token)?
            .section
            .as_ref()?;
        let gap = s.gap?;
        Some(match s.rows.get(gap) {
            Some(r) => (r.view, false),
            None => (s.rows.last()?.view, true),
        })
    }
    /// UIKit has already placed the row in its section. Certify the successor
    /// against the same lifted rows without drawing another preview.
    pub fn native_section_gap(
        &mut self,
        token: crate::ReorderToken,
        before: Option<ViewId>,
    ) -> bool {
        self.has_reorder(token)
            && self
                .edit_section(|section, k| section.select_before(k, before))
                .flatten()
                .is_some()
    }
    /// The browser owns layout. It names the mounted row under the gap, which
    /// the session resolves against the lifted row's section before painting.
    pub fn preview_section_before(
        &mut self,
        token: crate::ReorderToken,
        before: Option<ViewId>,
        height: f64,
        offset: f64,
    ) -> Result<crate::ReorderProgress, RunnerError> {
        let Some(s) = self
            .session()
            .filter(|s| s.token == token && s.section.is_some())
        else {
            return Ok(crate::ReorderProgress::Stale);
        };
        if !offset.is_finite()
            || offset.abs() > f32::MAX as f64
            || !height.is_finite()
            || height <= 0.
            || height > f32::MAX as f64
            || !self.has_reorder(token)
        {
            return Ok(crate::ReorderProgress::Stale);
        }
        let ghost = s.ghost;
        let ops = self
            .edit_section(|s, k| {
                s.select_before(k, before)?;
                s.measured_offset = Some(offset);
                s.measured_height = Some(height);
                Some(s.paint(k, true, ghost))
            })
            .flatten();
        match ops {
            Some(ops) => Ok(crate::ReorderProgress::Accepted {
                receipt: self.apply_reorder_ops(ops)?,
            }),
            None => Ok(crate::ReorderProgress::Stale),
        }
    }
    pub(super) fn mounted_section(&self) -> Option<&MountedSection> {
        self.session()?.section.as_deref()
    }
    pub(super) fn edit_section<T>(
        &mut self,
        f: impl FnOnce(&mut MountedSection, &Kernel) -> T,
    ) -> Option<T> {
        let s = self
            .reorder_session
            .as_deref_mut()?
            .downcast_mut::<Session>()?;
        Some(f(s.section.as_deref_mut()?, &self.kernel))
    }
    pub(super) fn mounted_binding(&self, handle: NodeKey, list: ViewId) -> Option<ReorderBinding> {
        let list = self.kernel.node(list)?;
        list.props.str(PropId::ListStyle)?;
        let mut node = self.kernel.node_by_key(handle)?;
        while node.id != list.id {
            let parent = node.parent?;
            if self
                .kernel
                .grouped_card_section(parent)
                .is_some_and(|s| self.kernel.node(s).and_then(|n| n.parent) == Some(list.id))
            {
                self.grouped_reorder_rows(list.id, parent)
                    .into_iter()
                    .find(|(r, _, _)| *r == node.id)?;
                return Some(ReorderBinding {
                    handle,
                    list: list.key,
                    wrapper: node.key,
                    root: node.key,
                    row_epoch: 0,
                });
            }
            node = self.kernel.node(parent)?;
        }
        None
    }
    pub(super) fn mounted_geometry(&self, list: NodeKey) -> Option<ReorderGeometry> {
        let node = self.kernel.node_by_key(list)?;
        node.props.str(PropId::ListStyle)?;
        Some(ReorderGeometry {
            list,
            revision: 0,
            scroll_sequence: 0,
            scroll_top: self.scroll_offset(Some(node.id)).1,
            port_width: node.frame.width.into(),
            port_height: node.frame.height.into(),
            row_width: node.frame.width.into(),
            total_extent: node.content.1.into(),
        })
    }
    pub(super) fn mounted_frame(&self, s: &Session) -> Option<ReorderFrame> {
        let section = s.section.as_ref()?;
        Some(ReorderFrame {
            terminal: s.phase != ReorderPhase::Active,
            wrappers: Vec::new(),
            phase: s.phase,
            ending: s.ending,
            target: Some(s.target),
            row: self.kernel.node_by_key(section.binding.root).map(|n| n.key),
        })
    }
    pub(super) fn mounted_outcome(&self, s: &Session) -> Option<ReorderEnding> {
        let place = s.section.as_ref()?.place(self, &s.key);
        match place {
            None => Some(ReorderEnding::Gone),
            Some(p) if p.2 == s.before || Some(&p) != s.place.as_ref() => {
                Some(ReorderEnding::Landed)
            }
            _ => None,
        }
    }
}
