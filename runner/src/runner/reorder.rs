//! Collection-certified Arrange. No host clock, motion executor or data preview.
//! A list with a `reorderGroup` adds a target and a hold (`reorder_group.rs`,
//! LLP 1094); every call here keeps the source's token.
use super::reorder_group::Session;
use super::*;
use crate::instance::collection::{
    Collection, ReorderBinding, ReorderFrame, ReorderGeometry, ReorderPhase, ReorderProgress,
    ReorderStart, ReorderToken,
};
use exact_kernel::{NodeKey, Op};
use std::sync::atomic::{AtomicU64, Ordering};
static SERIAL: AtomicU64 = AtomicU64::new(1);

impl<D: DataSource> Runner<D> {
    pub(super) fn begin_session(
        &mut self,
        binding: ReorderBinding,
        geometry: ReorderGeometry,
        group: Option<String>,
        ghost: bool,
    ) -> Result<Option<ReorderStart>, RunnerError> {
        if self.reorder_owner.is_some()
            || self.reorder_binding(binding.handle) != Some(binding)
            || self.reorder_geometry(binding.list).as_ref() != Some(&geometry)
        {
            return Ok(None);
        }
        let serial = SERIAL
            .try_update(Ordering::Relaxed, Ordering::Relaxed, |v| v.checked_add(1))
            .map_err(|_| reorder_error("reorder serial exhausted"))?;
        let token = ReorderToken {
            list: binding.list,
            serial,
        };
        let handle = self.kernel.node_by_key(binding.handle).unwrap().id;
        let grouped = group.is_some();
        let (section, item, key, ops) = if binding.unwrapped() {
            let Some((mut section, key)) =
                super::mounted_reorder::MountedSection::new(self, binding)
            else {
                return Ok(None);
            };
            let ops = section.paint(&self.kernel, true, ghost);
            (Some(Box::new(section)), format!("s:{key}"), key, ops)
        } else {
            let (accepted, ops) = self.edit_reorder(binding.list, |c, u, _| {
                if !c.begin_preview(u, binding, token, handle)? {
                    return Ok(None);
                }
                let item = c.preview_item();
                if grouped {
                    c.set_grouped();
                    if ghost {
                        c.set_hidden(u, item.as_ref().map(|(ident, _)| ident.clone()))?;
                    }
                }
                Ok(item)
            })?;
            let Some(Some((item, key))) = accepted else {
                return Ok(None);
            };
            (None, item, key, ops)
        };
        self.reorder_owner = Some(binding.list);
        self.reorder_session = Some(Box::new(Session {
            token,
            target: binding.list,
            group,
            section,
            phase: ReorderPhase::Active,
            ending: None,
            deadline_ms: None,
            item,
            key,
            before: None,
            place: None,
            ghost,
            touched: vec![binding.list],
            warned: false,
        }));
        let receipt = self.apply_reorder_ops(ops)?;
        Ok(Some(ReorderStart { token, receipt }))
    }
    /// True only while the original source binding, pin and action capability live.
    pub(super) fn has_reorder_linked(&self, token: ReorderToken) -> bool {
        if let Some(s) = self
            .session()
            .filter(|s| s.token == token && s.section.is_some())
        {
            return s.phase == ReorderPhase::Active && s.section.as_ref().unwrap().current(self);
        }
        let Some(c) = self.reorder_collection(token.list) else {
            return false;
        };
        c.preview_active(token)
            && c.preview_binding()
                .is_some_and(|b| self.reorder_binding(b.handle) == Some(b))
    }
    /// Measured gap selection in unpreviewed content coordinates. No queries,
    /// all-key serialization or destination pin; stale precedes numeric validation.
    pub(super) fn preview_reorder_linked(
        &mut self,
        token: ReorderToken,
        geometry: ReorderGeometry,
        content_y: f64,
    ) -> Result<ReorderProgress, RunnerError> {
        if self.is_grouped(token) {
            return self.preview_reorder_into(token, token.list, geometry, content_y);
        }
        if !self.has_reorder(token) || self.reorder_geometry(token.list).as_ref() != Some(&geometry)
        {
            return Ok(ReorderProgress::Stale);
        }
        if !content_y.is_finite() || content_y.abs() > f32::MAX as f64 {
            return Err(reorder_error("invalid reorder coordinate"));
        }
        let (accepted, ops) = self.edit_reorder(token.list, |c, u, _| {
            c.move_preview(u, geometry.clone(), content_y)
        })?;
        if accepted != Some(true) {
            return Ok(ReorderProgress::NeedsMeasurement);
        }
        Ok(ReorderProgress::Accepted {
            receipt: self.apply_reorder_ops(ops)?,
        })
    }

    pub(super) fn drop_reorder_now(
        &mut self,
        token: ReorderToken,
        geometry: ReorderGeometry,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        if self.is_grouped(token) {
            return self.drop_group(token, geometry);
        }
        if !self.has_reorder(token) || self.reorder_geometry(token.list).as_ref() != Some(&geometry)
        {
            return Ok(None);
        }
        let (event, ops) = self.edit_reorder(token.list, |c, u, _| c.preview_drop(u, &geometry))?;
        let Some(Some((item, before))) = event else {
            return Ok(None);
        };
        // Reset targets join the action's one instance commit. The descriptor is
        // already terminal, so reentry cannot dispatch twice or reacquire it.
        self.reorder_ops.extend(ops);
        let queued = &self.reorder_ops;
        self.reorder_kept = exact_kernel::style::wide::Keep::of(|m| {
            exact_kernel::style::wide::Colors::colors(&queued[..], m)
        });
        self.set_phase(token, ReorderPhase::Settling);
        let view = self.kernel.node_by_key(token.list).unwrap().id;
        let result = self.dispatch(view, Event::ReorderDrop { item, before });
        self.edit_reorder(token.list, |c, _, _| {
            c.settle_drop_anchors(result.is_err().then_some(token));
            Ok(())
        })?;
        if result.is_err() {
            // Action validation/data refusal may precede apply. Do not leak its
            // queued style operations into a later unrelated user action.
            let (ops, _kept) = self.take_reorder_ops();
            self.apply_reorder_ops(ops)?;
        }
        result.map(Some)
    }
    /// Enter the no-action terminal phase; retain the pin until host hold cleanup.
    /// A hold is past cancelling: its send is out (LLP 1094 D8).
    pub(super) fn cancel_reorder_linked(
        &mut self,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        if let Some(s) = self
            .session()
            .filter(|s| s.token == token && s.section.is_some())
        {
            if s.phase != ReorderPhase::Active {
                return Ok(None);
            }
            let ops = self.end_session(None, false)?;
            return self.apply_reorder_ops(ops);
        }
        let Some(c) = self.reorder_collection(token.list) else {
            return Ok(None);
        };
        let active = c.preview_active(token);
        self.edit_reorder(token.list, |c, _, _| {
            c.cancel_drop_anchor(token);
            Ok(())
        })?;
        if !active {
            return Ok(None);
        }
        if self.is_grouped(token) {
            let ops = self.end_session(None, false)?;
            return self.apply_reorder_ops(ops);
        }
        let (_, ops) = self.edit_reorder(token.list, |c, u, _| c.end_preview(u))?;
        self.set_phase(token, ReorderPhase::Cancelling);
        self.apply_reorder_ops(ops)
    }
    /// Bounded live wrapper identities/targets for host before/after C0 rebasing.
    /// Current viewport presentation and mapping remain the host's responsibility.
    /// It says where the session stands, its target, and which mounted
    /// wrapper holds the dragged row now (LLP 1094 D4, D8).
    pub(super) fn reorder_frame_linked(&self, token: ReorderToken) -> Option<ReorderFrame> {
        if let Some(frame) = self
            .session()
            .filter(|s| s.token == token)
            .and_then(|s| self.mounted_frame(s))
        {
            return Some(frame);
        }
        let mut frame = self
            .reorder_collection(token.list)?
            .preview_frame(&self.kernel, token)?;
        let Some(s) = self.session().filter(|s| s.token == token) else {
            return Some(frame);
        };
        frame.phase = s.phase;
        frame.ending = s.ending;
        frame.target = Some(s.target);
        let mut lists = vec![s.target, token.list];
        if let Some(group) = s.group.as_deref() {
            lists.extend(self.group_lists(group));
        }
        frame.row = lists.into_iter().find_map(|list| {
            self.reorder_collection(list)?
                .wrapper_of(&self.kernel, &s.item)
        });
        Some(frame)
    }
    /// Whether `token` is a live session on a grouped list.
    pub(super) fn is_grouped(&self, token: ReorderToken) -> bool {
        self.session()
            .is_some_and(|s| s.token == token && (s.group.is_some() || s.section.is_some()))
    }
    fn set_phase(&mut self, token: ReorderToken, phase: ReorderPhase) {
        if let Some(s) = self.session_mut().filter(|s| s.token == token) {
            s.phase = phase;
        }
    }
    /// Retire the terminal descriptor and only its still-owned interaction pin.
    /// Hosts may defer this through the source return spring; finish only when
    /// presentation no longer needs the pin, or before explicitly replacing it.
    /// A stale token cannot clear a successor; deletion needs no source to survive.
    pub(super) fn finish_reorder_linked(
        &mut self,
        token: ReorderToken,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        if self.reorder_owner != Some(token.list) {
            return Ok(None);
        }
        if let Some(s) = self
            .session()
            .filter(|s| s.token == token && s.section.is_some())
        {
            if !matches!(s.phase, ReorderPhase::Cancelling | ReorderPhase::Settling) {
                return Ok(None);
            }
            let ops = self
                .edit_section(|section, kernel| section.restore(kernel))
                .unwrap();
            self.reorder_session = None;
            self.reorder_owner = None;
            return self.apply(ops).map(Some);
        }
        let Some(c) = self.reorder_collection(token.list) else {
            self.reorder_owner = None;
            return Ok(None);
        };
        if c.preview_token() != Some(token) {
            return Ok(None);
        }
        let (finished, mut ops) = self.edit_reorder(token.list, |c, u, frames| {
            c.finish_preview(u, frames, token)
        })?;
        if finished != Some(true) {
            return Ok(None);
        }
        self.reorder_owner = None;
        // Every row a grouped session hid shows again, and any gap left
        // open closes (LLP 1094 D6).
        if let Some(s) = self
            .reorder_session
            .take()
            .and_then(|s| s.downcast::<Session>().ok())
        {
            for list in s.touched {
                let (_, o) = self.edit_reorder(list, |c, u, _| {
                    c.close_incoming(u, false)?;
                    c.set_hidden(u, None)
                })?;
                ops.extend(o);
            }
        }
        self.sync_reorder_target();
        self.apply(ops).map(Some)
    }
    pub(super) fn reorder_collection(&self, key: NodeKey) -> Option<&Collection> {
        let id = self.kernel.node_by_key(key)?.id;
        self.tree.as_ref()?.reorder_collection(id)
    }
    pub(super) fn edit_reorder<T>(
        &mut self,
        key: NodeKey,
        edit: impl FnMut(&mut Collection, &mut Update<'_>, &[Frame]) -> Result<T, InstanceError>,
    ) -> Result<(Option<T>, Vec<Op>), RunnerError> {
        if self.poisoned {
            return Err(RunnerError::Poisoned);
        }
        let Some(id) = self.kernel.node_by_key(key).map(|n| n.id) else {
            return Ok((None, Vec::new()));
        };
        let mut tree = self.tree.take().expect("booted");
        let mut ids = std::mem::take(&mut self.ids);
        let result = {
            let mut u = self.instance_update(&mut ids);
            u.reuse = self.reuse;
            tree.edit_collection(id, &mut u, edit)
                .map(|r| (r, u.ops, u.notes, u.renewed, u.shown))
        };
        self.tree = Some(tree);
        self.ids = ids;
        let result = result.map(|(r, ops, notes, renewed, shown)| {
            self.notes.extend(notes);
            self.renewed.extend(renewed);
            self.shown.extend(shown);
            (r, ops)
        });
        result.map_err(Into::into)
    }
    pub(super) fn apply_reorder_ops(
        &mut self,
        ops: Vec<Op>,
    ) -> Result<Option<CommitReceipt>, RunnerError> {
        self.sync_reorder_target();
        if ops.is_empty() {
            Ok(None)
        } else {
            self.apply(ops).map(Some)
        }
    }
    pub(super) fn reconcile_reorder_linked(&mut self) -> Result<Vec<Op>, RunnerError> {
        let Some(owner) = self.reorder_owner else {
            return Ok(Vec::new());
        };
        if self.mounted_section().is_some() {
            return self.reconcile_group();
        }
        let Some(c) = self.reorder_collection(owner) else {
            self.reorder_owner = None;
            self.reorder_session = None;
            return Ok(Vec::new());
        };
        let Some(token) = c.preview_token() else {
            self.reorder_owner = None;
            self.reorder_session = None;
            return Ok(Vec::new());
        };
        if self.is_grouped(token) {
            return self.reconcile_group();
        }
        if !c.preview_active(token) || self.has_reorder(token) {
            return Ok(Vec::new());
        }
        self.edit_reorder(owner, |c, u, _| c.end_preview(u))
            .map(|(_, ops)| ops)
    }
}
fn reorder_error(message: &str) -> RunnerError {
    InstanceError::Collection(message.into()).into()
}
