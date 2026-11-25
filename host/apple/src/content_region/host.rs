//! Region-aware layout and completion on the owning UI/runtime thread.
use super::*;
use crate::content_region::{ContentRegionRegistration, NativeArtifact, RegionState};
use exact_kernel::{RegionInputs, RegionTextRequest, TextMetrics};
use std::{any::Any, rc::Rc};
impl<D: DataSource> Host<D> {
    /// A captured node's block moved in or out of the region's content.
    pub(super) fn native_moved_linked(&mut self) {
        if let Some(native) =
            slot_region_mut(&mut self.content_region).and_then(|r| r.native.as_mut())
        {
            native.dirty = true;
        }
    }
    /// Boot one explicitly registered content region before any text layout.
    /// `facts` supplies the date, place and clock before settlement (LLP 1027.000.000 D2).
    pub fn boot_region(
        plan: &[u8],
        data: D,
        measurer: Box<dyn TextMeasurer>,
        width: f32,
        height: f32,
        registration: ContentRegionRegistration,
        facts: exact_runner::time::Launch,
    ) -> Result<(Self, String), HostError> {
        exact_runner::time::launching_with(facts, || {
            let (mut host, batch) = Self::boot_stored_after_decode(
                crate::host::PlanBytes::Copied(plan),
                data,
                measurer,
                crate::host::LaunchLayout::NONE,
                exact_runner::Viewport::sized(width as f64, height as f64),
                None,
                Vec::new(),
                None,
                None,
                None,
                None,
                "/",
                false,
                Some(registration),
                crate::link::Links::ALL,
                None,
                |_| Ok(()),
            )?;
            host.commit_boot();
            Ok((host, batch))
        })
    }
    /// The one current request. The source/ticket retains no Arena/Runtime.
    pub fn pending_region_request(&self) -> Option<(u64, &RegionTextRequest)> {
        let p = slot_region(&self.content_region)?.pending.as_ref()?;
        Some((p.id, &p.request))
    }
    /// Source/paint snapshot for a current request, copied only by its consumer.
    pub fn region_request_json(&self, id: u64) -> Result<String, String> {
        self.region_request_json_known(id, 0)
    }
    pub(crate) fn region_request_json_known(&self, id: u64, known: u64) -> Result<String, String> {
        slot_region(&self.content_region)
            .ok_or("no content region")?
            .request_json(
                self.runner.kernel(),
                id,
                known,
                self.runner.resolved_locale(),
            )
    }
    /// Selected immutable publication identity, including retained old content.
    pub fn region_publication_id(&self) -> Option<u64> {
        if let Some(native) = slot_region(&self.content_region)?.native.as_ref() {
            return native.selected.as_ref().map(|a| a.serial);
        }
        slot_region(&self.content_region)?
            .publication
            .as_ref()
            .map(|(id, _)| *id)
    }
    /// Takes ownership even when stale. No stale metric or completion changes
    /// clocks, current requests or the retained accepted native artifact.
    pub fn complete_region_text(
        &mut self,
        id: u64,
        metrics: TextMetrics,
        owner: Rc<dyn Any>,
    ) -> String {
        let Some(request) = self
            .pending_region_request()
            .filter(|(n, _)| *n == id)
            .map(|(_, r)| r.clone())
        else {
            return self.finish(Batch::new(), None);
        };
        let artifact = Rc::new(NativeArtifact { id, _owner: owner });
        match self
            .runner
            .kernel_mut()
            .resolve_region_text(&request, metrics, artifact)
        {
            Ok(true) => {
                let mut batch = Batch::new();
                let error = self.layout(&mut batch).err();
                self.finish(batch, error)
            }
            Ok(false) => self.finish(Batch::new(), None),
            Err(e) => self.finish(Batch::new(), Some(format!("region completion: {e:?}"))),
        }
    }
    /// Return a current worker request to native layout. A superseded answer
    /// cannot retire a successor registration or its accepted publication.
    pub fn fallback_region_text(&mut self, id: u64) -> String {
        if self
            .pending_region_request()
            .is_none_or(|(current, _)| current != id)
        {
            return self.finish(Batch::new(), None);
        }
        let mut batch = Batch::new();
        let error = self
            .retire_region(&mut batch, "incomplete text flow uses native layout")
            .and_then(|()| self.layout(&mut batch))
            .err();
        self.finish(batch, error)
    }
    fn retire_region(&mut self, batch: &mut Batch, reason: &str) -> Result<(), String> {
        self.runner
            .kernel_mut()
            .set_content_region(None)
            .map_err(|e| format!("region native fallback: {e:?}"))?;
        let region = *self
            .content_region
            .take()
            .and_then(|state| state.downcast::<RegionState>().ok())
            .unwrap();
        batch.region(&format!("{{\"op\":\"region\",\"disabled\":\"{reason}\"}}"));
        if let Some(native) = region.native {
            let headers = native.selected.map_or_else(Vec::new, |a| a.headers);
            for header in &headers {
                if self.runner.kernel().node_by_key(header.key).is_none() {
                    self.mirror.remove(&header.id);
                    if header.inline_owner.is_none() {
                        batch.destroy(header.id);
                    }
                }
            }
            // Native projection suppressed creates and edits while B was pending.
            // Replay the live subtree before ordinary layout publishes its flow.
            let order: Vec<_> = self
                .preorder()
                .into_iter()
                .filter(|id| {
                    self.runner.kernel().node(*id).is_some_and(|n| {
                        within(self.runner.kernel(), n.key, region.binding.content)
                            || headers.iter().any(|h| h.key == n.key)
                    })
                })
                .collect();
            let handlers = self.runner.handlers();
            for id in &order {
                if self.mirror.get(id).is_none_or(|m| m.style.is_empty()) {
                    self.create(*id, handlers.get(id).map_or(&[], Vec::as_slice), batch);
                } else {
                    self.update(*id, batch);
                }
            }
            for id in order {
                self.emit_children(id, batch);
            }
            if let Some(owner) = self.runner.kernel().node_by_key(region.binding.owner) {
                self.emit_children(owner.id, batch);
            }
            self.emit_paragraphs(batch);
            self.collections_json.clear();
            self.collection_heads = None;
        }
        Ok(())
    }
    fn unsupported_region_paint(&self) -> bool {
        use exact_kernel::{StyleId, WrapFlow};
        let Some(region) = slot_region(&self.content_region).filter(|r| r.native.is_none()) else {
            return false;
        };
        let kernel = self.runner.kernel();
        let mut stack: Vec<_> = kernel
            .roots()
            .into_iter()
            .filter_map(|id| kernel.node(id).map(|n| n.key))
            .collect();
        while let Some(node) = stack.pop().and_then(|key| kernel.node_by_key(key)) {
            let owner = node.key == region.binding.owner;
            let content = within(kernel, node.key, region.binding.content);
            let exclusion = node.layout_position() == exact_kernel::PositionType::Absolute
                && node.style.wrap_flow == WrapFlow::Both
                && (content
                    || node
                        .parent
                        .and_then(|id| kernel.node(id))
                        .is_some_and(|parent| within(kernel, region.binding.owner, parent.key)));
            if owner || content || exclusion {
                // The opaque surface draws only text over the owner/page ground.
                if !matches!(
                    node.node_type,
                    NodeType::View | NodeType::Text | NodeType::ScrollView
                ) || exclusion
                    && (node.node_type != NodeType::View || !node.children().is_empty())
                    || !node.looks().is_empty()
                    || node.props.str(PropId::BackgroundMaterial).is_some()
                    // The worker draws glyph fills only; a stroke is inherited.
                    || node.computed_row(StyleId::TextStrokeWidth, |s| s.text_stroke_width > 0.)
                    // It clips lines to the owner and the scroll port alone.
                    || content
                        && node.node_type != NodeType::ScrollView
                        && style::effective_overflow(&node) != (Overflow::Visible, Overflow::Visible)
                    || [
                        StyleId::BackgroundColor,
                        StyleId::BackgroundImage,
                        StyleId::BackgroundClip,
                        StyleId::BoxShadow,
                        StyleId::BackdropFilter,
                        StyleId::BorderStyleTop,
                        StyleId::BorderStyleRight,
                        StyleId::BorderStyleBottom,
                        StyleId::BorderStyleLeft,
                        StyleId::OutlineWidth,
                        StyleId::OutlineStyle,
                        StyleId::TextShadow,
                        StyleId::Opacity,
                        StyleId::Visibility,
                        StyleId::Translate,
                        StyleId::Scale,
                        StyleId::Rotate,
                        StyleId::Transform,
                        StyleId::ClipPath,
                        StyleId::Filter,
                    ]
                    .into_iter()
                    .any(|id| {
                        !(owner && id == StyleId::BackgroundColor)
                            && node.computed_row(id, |s| s.mask.has(id))
                    })
                {
                    return true;
                }
            }
            stack.extend(
                node.children()
                    .into_iter()
                    .filter_map(|id| kernel.node(id).map(|n| n.key)),
            );
        }
        false
    }
    pub(super) fn region_layout_linked(
        &mut self,
        root: ViewId,
        offer: Offer,
        batch: &mut Batch,
    ) -> Result<(), String> {
        self.native_prepare_candidate()?;
        if self.unsupported_region_paint() {
            return self.region_native_layout(
                root,
                offer,
                batch,
                "unsupported content paint uses native views",
            );
        }
        let Some(region) = slot_region_mut(&mut self.content_region) else {
            return Ok(());
        };
        if self.height_owner.is_some() {
            return Err("Height projection is unsupported with content region".into());
        }
        self.runner.count_kernel_layout();
        let receipt = self
            .runner
            .kernel_mut()
            .compute_region_layout(
                root,
                offer,
                RegionInputs {
                    catalog: region.incarnation,
                    consumer_revision: region.native.as_ref().map_or(0, |n| n.revision),
                },
            )
            .map_err(|e| format!("region layout: {e:?}"))?;
        self.runner.report_flow_skipped(&receipt.shell.flow_skipped);
        self.runner.moved(&receipt.shell.changed);
        self.pending_layout.extend(
            receipt
                .shell
                .updated
                .iter()
                .chain(&receipt.shell.flow_changed)
                .copied(),
        );
        if region.native.is_some()
            && receipt.shell.flow_changed.iter().any(|key| {
                self.runner
                    .kernel()
                    .node_by_key(*key)
                    .is_some_and(|n| !n.flow_shapes().is_empty())
            })
        {
            // Native projections do not retain flow geometry with selected A.
            return self.region_native_layout(
                root,
                offer,
                batch,
                "native flow uses ordinary views",
            );
        }
        // Final shell/native flow is published by emit_layout against its mirror.
        region.observe(self.runner.kernel(), receipt)?;
        if let Some(native) = &mut region.native {
            if let (Some(candidate), Some(pending)) = (&mut native.candidate, &region.pending) {
                candidate.identity.ticket = Some(pending.request.ticket().clone());
            }
        }
        // Even a recoverable staging refusal must revoke the client's previous
        // current authority. Selected A remains unchanged until full promotion.
        let result = self.native_select(batch);
        let region = slot_region(&self.content_region).unwrap();
        batch.region(&region.json(self.runner.kernel()));
        result
    }
    fn region_native_layout(
        &mut self,
        root: ViewId,
        offer: Offer,
        batch: &mut Batch,
        reason: &str,
    ) -> Result<(), String> {
        self.retire_region(batch, reason)?;
        self.runner.count_kernel_layout();
        let receipt = self
            .runner
            .kernel_mut()
            .compute_layout(root, offer)
            .map_err(|e| format!("region native layout: {e:?}"))?;
        self.runner.report_flow_skipped(&receipt.flow_skipped);
        self.runner.moved(&receipt.changed);
        self.record_layout(&receipt);
        Ok(())
    }
}

use crate::content_region::{
    projection_size, within, CandidateNative, CandidateNativeNode, NativeHeader, NativeIdentity,
    NativeProjectionLimits, SelectedNative,
};
use exact_kernel::RegionSelection;

impl<D: DataSource> Host<D> {
    /// Explicit native-subtree projection. Existing opaque registration and ABI
    /// entry points do not opt in. This is a Rust producer seam, not a Swift
    /// realization/retained-action implementation. `facts` supplies the date,
    /// place and clock before settlement (LLP 1027.000.000 D2).
    #[allow(clippy::too_many_arguments)]
    pub fn boot_native_region(
        plan: &[u8],
        data: D,
        measurer: Box<dyn TextMeasurer>,
        width: f32,
        height: f32,
        registration: ContentRegionRegistration,
        limits: NativeProjectionLimits,
        facts: exact_runner::time::Launch,
    ) -> Result<(Self, String), HostError> {
        exact_runner::time::launching_with(facts, || {
            let (mut host, batch) = Self::boot_stored_after_decode_mode(
                crate::host::PlanBytes::Copied(plan),
                data,
                measurer,
                crate::host::LaunchLayout::NONE,
                exact_runner::Viewport::sized(width as f64, height as f64),
                None,
                Vec::new(),
                None,
                None,
                None,
                None,
                "/",
                false,
                Some(registration),
                Some(limits),
                None,
                crate::link::Links::ALL,
                None,
                |_| Ok(()),
            )?;
            host.commit_boot();
            Ok((host, batch))
        })
    }
    pub(super) fn native_mode_linked(&self) -> bool {
        slot_region(&self.content_region).is_some_and(|r| r.native.is_some())
    }
    pub(super) fn native_current_linked(&self) -> bool {
        slot_region(&self.content_region).is_some_and(|r| r.selected_native_current())
    }
    pub(super) fn native_selected_id_linked(&self, id: ViewId) -> bool {
        slot_region(&self.content_region)
            .and_then(|r| r.native.as_ref())
            .and_then(|n| n.selected.as_ref())
            .is_some_and(|a| a.headers.iter().any(|h| h.id == id))
    }
    pub(super) fn native_captured_id_linked(&self, id: ViewId) -> bool {
        let Some(region) = slot_region(&self.content_region).filter(|r| r.native.is_some()) else {
            return false;
        };
        self.runner
            .kernel()
            .node(id)
            .is_some_and(|n| within(self.runner.kernel(), n.key, region.binding.content))
    }
    pub(super) fn native_retire_removed_owner_linked(&mut self, batch: &mut Batch) {
        let Some(region) = slot_region(&self.content_region).filter(|r| r.native.is_some()) else {
            return;
        };
        if self
            .runner
            .kernel()
            .node_by_key(region.binding.owner)
            .is_some()
            && self
                .runner
                .kernel()
                .node_by_key(region.binding.content)
                .is_some()
        {
            return;
        }
        let region = slot_region_mut(&mut self.content_region).unwrap();
        let native = region.native.as_mut().unwrap();
        native.candidate = None;
        if let Some(a) = native.selected.take() {
            for header in a.headers {
                self.keys.remove(&header.key);
                self.mirror.remove(&header.id);
                self.transform_drags.remove(header.id);
                if header.inline_owner.is_none() {
                    batch.destroy(header.id);
                }
            }
        }
        region.retire_native();
        batch.region(&region.json(self.runner.kernel()));
        // Retire kernel-owned artifact aliases too. This is terminal for this
        // explicit registration; no ViewId reuse can silently register a new owner.
        let _ = self.runner.kernel_mut().set_content_region(None);
    }
    pub(super) fn native_note_receipts_linked(&mut self, receipts: &[Timed]) {
        let Some(region) = slot_region(&self.content_region).filter(|r| r.native.is_some()) else {
            return;
        };
        let native = region.native.as_ref().unwrap();
        let dirty = receipts.iter().any(|t| {
            t.receipt
                .created
                .iter()
                .chain(t.receipt.touched.iter())
                .chain(t.receipt.destroyed.iter())
                .any(|key| {
                    within(self.runner.kernel(), *key, region.binding.content)
                        || native
                            .selected
                            .as_ref()
                            .is_some_and(|a| a.headers.iter().any(|h| h.key == *key))
                        || native
                            .candidate
                            .as_ref()
                            .is_some_and(|b| b.nodes.iter().any(|n| n.header.key == *key))
                })
        });
        if dirty {
            slot_region_mut(&mut self.content_region)
                .unwrap()
                .native
                .as_mut()
                .unwrap()
                .dirty = true;
        }
    }
    pub(super) fn native_prepare_candidate_linked(&mut self) -> Result<(), String> {
        let Some(region) = slot_region(&self.content_region).filter(|r| r.native.is_some()) else {
            return Ok(());
        };
        if let Some(why) = &region.refused {
            return Err(why.clone());
        }
        let native = region.native.as_ref().unwrap();
        let limits = native.limits;
        let content = region.binding.content;
        let incarnation = region.incarnation;
        // No owned source/style/child copy before this borrowed walk and wire
        // reservation. Collector reserves both bounded borrowed stacks first.
        let mut charge = projection_size(
            self.runner.kernel(),
            content,
            limits,
            self.runner.plan().handlers.len(),
        )?;
        let mut collections = self
            .runner
            .collections_bounded(
                limits.collections,
                limits.collection_rows,
                limits.traversal_entries,
                limits
                    .packet_wire_bound
                    .checked_sub(charge.wire_upper)
                    .ok_or("native wire capacity")?,
            )
            .map_err(str::to_owned)?;
        let collection_wire = collections
            .iter()
            .try_fold(2usize, |n, c| {
                let roots = c
                    .fixed
                    .iter()
                    .try_fold(0usize, |n, f| n.checked_add(f.roots.len()))?;
                c.rows
                    .len()
                    .checked_add(1)
                    .and_then(|rows| rows.checked_mul(1024))
                    .and_then(|bytes| roots.checked_mul(12).and_then(|r| bytes.checked_add(r)))
                    .and_then(|bytes| n.checked_add(bytes))
            })
            .ok_or("native collection wire overflow")?;
        charge.wire_upper = charge
            .wire_upper
            .checked_add(collection_wire)
            .ok_or("native wire overflow")?;
        collections.retain(|c| {
            self.runner
                .kernel()
                .node(c.view)
                .is_some_and(|n| within(self.runner.kernel(), n.key, content))
        });
        let old_collections = native
            .candidate
            .as_ref()
            .map(|b| b.collections.as_slice())
            .or_else(|| native.selected.as_ref().map(|a| a.collections.as_slice()));
        let changed_collections = old_collections.is_some_and(|old| old != collections);
        if !native.dirty && !changed_collections && native.candidate.is_some() {
            return Ok(());
        }
        let changed = native.dirty || changed_collections;
        let native = slot_region_mut(&mut self.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap();
        if changed {
            native.revision = native
                .revision
                .checked_add(1)
                .ok_or("native revision exhausted")?;
        }
        // Drop obsolete B before any replacement owned node snapshot. A lives
        // only in Host.mirror plus its immutable headers/collection/artifact pins.
        native.candidate = None;
        let revision = native.revision;
        let mut nodes = Vec::new();
        nodes
            .try_reserve_exact(charge.nodes)
            .map_err(|_| "native node allocation")?;
        let mut stack = Vec::new();
        stack
            .try_reserve_exact(charge.nodes)
            .map_err(|_| "native node stack allocation")?;
        stack.push(content.index);
        while let Some(slot) = stack.pop() {
            let kernel = self.runner.kernel();
            let arena = kernel.arena();
            if !within(kernel, arena.key(slot), content) {
                continue;
            }
            let children = arena
                .children(slot)
                .iter()
                .rev()
                .copied()
                .filter(|s| within(kernel, arena.key(*s), content));
            if children.clone().count() > charge.nodes.saturating_sub(stack.len()) {
                return Err("native stack capacity".into());
            }
            stack.extend(children);
            let node = self
                .runner
                .kernel()
                .node_by_key(arena.key(slot))
                .ok_or("native removed node")?;
            let props = props_for(&node);
            let (style, _) = style::style_json_for(&node, &self.runner.kernel().env_of(node.id));
            let children = arena
                .children(slot)
                .iter()
                .filter(|s| within(kernel, arena.key(**s), content))
                .map(|s| arena.local_id(*s))
                .collect();
            let handlers = self
                .runner
                .handlers_of(node.id)
                .into_iter()
                .filter_map(handler_name)
                .collect::<Vec<_>>()
                .into_boxed_slice();
            nodes.push(CandidateNativeNode {
                header: NativeHeader {
                    key: node.key,
                    id: node.id,
                    kind: kind_for(&node),
                    inline_owner: self.paragraph_owner(node.id).filter(|id| *id != node.id),
                    handlers,
                },
                mirror: Mirror {
                    props,
                    style: self.style_texts.share(style),
                    children,
                    ..Default::default()
                },
            });
        }
        if nodes.len() != charge.nodes {
            return Err("native capture membership changed".into());
        }
        let native = slot_region_mut(&mut self.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap();
        native.candidate = Some(CandidateNative {
            identity: NativeIdentity {
                incarnation,
                content,
                inputs: RegionInputs {
                    catalog: incarnation,
                    consumer_revision: revision,
                },
                ticket: None,
            },
            nodes,
            collections,
            charge,
        });
        native.dirty = false;
        Ok(())
    }
    pub(super) fn native_select(&mut self, batch: &mut Batch) -> Result<(), String> {
        if !self.native_mode() {
            return Ok(());
        }
        let region = slot_region(&self.content_region).unwrap();
        let native = region.native.as_ref().unwrap();
        let Some(receipt) = &region.receipt else {
            return Ok(());
        };
        if !receipt.current {
            return Ok(());
        }
        let RegionSelection::Accepted(publication) = &receipt.selection else {
            return Ok(());
        };
        let publication = publication.clone();
        let origin = receipt.origin;
        let Some(candidate) = &native.candidate else {
            return Ok(());
        };
        if candidate.identity.incarnation != region.incarnation
            || candidate.identity.content != region.binding.content
            || candidate.identity.inputs != publication.inputs()
            || candidate
                .identity
                .ticket
                .as_ref()
                .is_some_and(|t| t != publication.ticket())
        {
            return Err("native candidate publication mismatch".into());
        }
        if candidate.nodes.len() > native.limits.nodes
            || candidate.nodes.len() != publication.frames().len()
            || candidate
                .nodes
                .iter()
                .zip(publication.frames())
                .any(|(n, f)| f.node != n.header.key)
        {
            return Err("native publication membership mismatch".into());
        }
        let old_size = native.selected.as_ref().map_or(0, |a| a.charge.wire_upper);
        let wire_upper = old_size
            .checked_add(candidate.charge.wire_upper)
            .ok_or("native diff overflow")?;
        if wire_upper > native.limits.diff_wire_bound {
            return Err("native diff admission".into());
        }
        // One checked parent-first world projection, bounded before allocating.
        // The current receipt has already published these frames to the arena.
        let frames = publication
            .projected_frames(origin)
            .map_err(|e| format!("native frame projection: {e:?}"))?;
        let op_count = candidate
            .nodes
            .len()
            .checked_mul(10)
            .and_then(|n| n.checked_add(native.selected.as_ref().map_or(0, |a| a.headers.len())))
            .and_then(|n| n.checked_add(3))
            .ok_or("native op overflow")?;
        let mut staged = Batch::staging(op_count)?;
        // Use ordinary native f32 subtraction against the current arena parent.
        // This path never reprojects or relabels a retained/noncurrent A.
        let kernel = self.runner.kernel();
        let mut candidate = slot_region_mut(&mut self.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .candidate
            .take()
            .unwrap();
        for (n, projected) in candidate.nodes.iter_mut().zip(frames) {
            let live = kernel
                .node_by_key(n.header.key)
                .ok_or("native current node missing")?;
            let parent = live.parent.and_then(|id| kernel.node(id)).map(|p| p.frame);
            n.mirror.frame = Some(relative(projected.frame, parent));
            n.mirror.field_content = published_content_box(&live, kernel).map(Box::new);
            n.mirror.field_cell = live.is_cell_field();
            if style::effective_overflow(&live) != (Overflow::Visible, Overflow::Visible) {
                // Preserve the ordinary native overflow computation while current;
                // it cannot run against live B for an old selected A.
                n.mirror.content = Some(content_size(&live, kernel));
            }
        }
        let region = slot_region(&self.content_region).unwrap();
        let native = region.native.as_ref().unwrap();
        if let Some(old) = &native.selected {
            if Rc::ptr_eq(&old.publication, &publication)
                && old.origin.bits_eq(origin)
                && old.identity.inputs == candidate.identity.inputs
                && old.collections == candidate.collections
                && old.headers.len() == candidate.nodes.len()
                && old.headers.iter().zip(&candidate.nodes).all(|(h, n)| {
                    h.key == n.header.key
                        && h.id == n.header.id
                        && h.kind == n.header.kind
                        && h.inline_owner == n.header.inline_owner
                        && h.handlers == n.header.handlers
                        && self.mirror.get(&h.id).map(|m| &**m) == Some(&n.mirror)
                })
            {
                return Ok(());
            }
        }
        let returning: Vec<_> = native
            .selected
            .iter()
            .flat_map(|a| &a.headers)
            .filter(|h| {
                kernel.node_by_key(h.key).is_some()
                    && !within(kernel, h.key, region.binding.content)
            })
            .map(|h| h.id)
            .collect();
        // Presenters keep an open popover only if its home is known when
        // its former publication removes it from the parent's child list.
        let attachments: Vec<_> = returning
            .iter()
            .filter_map(|&id| {
                let node = kernel.node(id)?;
                let parent = self.props_of(&node).remove("popoverParent")?;
                (self.mirror.get(&id)?.props.get("popoverParent") != Some(&parent))
                    .then_some((id, parent))
            })
            .collect();
        for (id, parent) in &attachments {
            staged.props(*id, &[("popoverParent", parent.clone())], &[]);
        }
        if let Some(old) = &native.selected {
            for header in &old.headers {
                if header.inline_owner.is_none()
                    && !returning.contains(&header.id)
                    && !candidate.nodes.iter().any(|n| {
                        n.header.key == header.key
                            && n.header.id == header.id
                            && n.header.inline_owner == header.inline_owner
                    })
                {
                    staged.destroy(header.id);
                }
            }
        }
        // All creates precede final children; no Host.mirror mutation yet.
        // Layout may reserve an empty mirror before its native view exists.
        for node in candidate
            .nodes
            .iter()
            .filter(|n| n.header.inline_owner.is_none())
        {
            let old_header = native.selected.as_ref().and_then(|a| {
                a.headers.iter().find(|h| {
                    h.key == node.header.key && h.id == node.header.id && h.inline_owner.is_none()
                })
            });
            if let Some(h) = old_header {
                if h.kind != node.header.kind || h.handlers != node.header.handlers {
                    return Err("native listener/kind replacement needs binding handoff".into());
                }
            } else if self.native_selected_id(node.header.id)
                || self
                    .mirror
                    .get(&node.header.id)
                    .is_none_or(|m| m.style.is_empty())
            {
                let pairs: Vec<_> = node
                    .mirror
                    .props
                    .iter()
                    .map(|(k, v)| (*k, v.clone()))
                    .collect();
                staged.create(
                    node.header.id,
                    node.header.kind,
                    &pairs,
                    &node.mirror.style,
                    &node.header.handlers,
                );
            }
        }
        for node in candidate
            .nodes
            .iter()
            .filter(|n| n.header.inline_owner.is_none())
        {
            let old = self
                .mirror
                .get(&node.header.id)
                .filter(|_| {
                    !self.native_selected_id(node.header.id)
                        || native.selected.as_ref().is_some_and(|a| {
                            a.headers.iter().any(|h| {
                                h.id == node.header.id
                                    && h.key == node.header.key
                                    && h.inline_owner.is_none()
                            })
                        })
                })
                .filter(|m| !m.style.is_empty());
            if let Some(old) = old {
                if old.props != node.mirror.props {
                    let set: Vec<_> = node
                        .mirror
                        .props
                        .iter()
                        .filter(|(k, v)| old.props.get(*k) != Some(*v))
                        .map(|(k, v)| (*k, v.clone()))
                        .collect();
                    let clear: Vec<_> = old
                        .props
                        .keys()
                        .filter(|k| !node.mirror.props.contains_key(*k))
                        .copied()
                        .collect();
                    staged.props(node.header.id, &set, &clear);
                }
                if old.style != node.mirror.style {
                    staged.style(node.header.id, &node.mirror.style);
                }
            }
            if node.header.kind != "text" && old.is_none_or(|m| m.children != node.mirror.children)
            {
                staged.children(node.header.id, &node.mirror.children);
            }
            if old.is_none_or(|m| m.frame != node.mirror.frame) {
                let (x, y, w, h) = node.mirror.frame.ok_or("native frame not complete")?;
                staged.frame(node.header.id, x, y, w, h);
            }
            if self.layout_withheld
                || old.and_then(|m| m.field_content.as_deref())
                    != node.mirror.field_content.as_deref()
                || old.is_some_and(|m| m.field_cell != node.mirror.field_cell)
            {
                staged.field_content(
                    node.header.id,
                    node.mirror.field_content.as_deref().copied(),
                    node.mirror.field_cell,
                );
            }
            if self.layout_withheld || old.is_none_or(|m| m.content != node.mirror.content) {
                if let Some((w, h)) = node.mirror.content {
                    staged.content(node.header.id, w, h, None);
                }
            }
        }
        self.stage_native_paragraphs(&candidate.nodes, &mut staged);
        // Styles intentionally omit the four native presentation rows. Sample
        // this qualified current key at promotion so newly created controls get
        // their real opacity/transform too; do not reconstruct from an old ID.
        for node in candidate
            .nodes
            .iter()
            .filter(|n| n.header.inline_owner.is_none())
        {
            let live = self
                .runner
                .kernel()
                .node_by_key(node.header.key)
                .ok_or("native current key missing")?;
            for (property, target) in targets(live.style) {
                let value = self
                    .engine
                    .value(motion_node(node.header.key), property)
                    .unwrap_or(target);
                staged.present(
                    node.header.id,
                    property.name(),
                    value.x,
                    if property == Property::Translate {
                        value.y
                    } else {
                        0.0
                    },
                );
            }
        }
        // The global collection op contains outside live rows plus this exact B.
        // It is staged with node changes, never emitted before promotion.
        let collections_json = self.native_collections_with(&candidate.collections)?;
        if collections_json != self.collections_json {
            staged.collections("items", &collections_json);
        }
        let owner = self
            .runner
            .kernel()
            .node_by_key(region.binding.owner)
            .ok_or("native owner missing")?;
        let owner_id = owner.id;
        // Kernel registration requires the explicit direct content/placeholder
        // envelope; attach content only with its first complete node transaction.
        let owner_children = owner.children();
        if self
            .mirror
            .get(&owner_id)
            .is_none_or(|m| *m.children != *owner_children)
        {
            staged.children(owner_id, &owner_children);
        }
        let serial = crate::content_region::serial()?;
        let limits = native.limits;
        let mut headers = Vec::new();
        headers
            .try_reserve_exact(candidate.nodes.len())
            .map_err(|_| "native header allocation")?;
        // Last fallible step precedes any mutation of selected A.
        batch.append_checked(staged, limits.diff_wire_bound)?;
        for (id, parent) in attachments {
            self.mirror
                .get_mut(&id)
                .unwrap()
                .props
                .insert("popoverParent", parent);
        }
        let native = slot_region_mut(&mut self.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap();
        if let Some(old) = native.selected.take() {
            for h in old.headers {
                if !returning.contains(&h.id) {
                    self.mirror.remove(&h.id);
                }
            }
        }
        for node in candidate.nodes {
            headers.push(node.header);
            self.mirror
                .insert(headers.last().unwrap().id, Box::new(node.mirror));
        }
        self.mirror.entry(owner_id).or_default().children = owner_children.into_boxed_slice();
        self.collections_json = collections_json;
        self.collection_heads = None;
        native.selected = Some(SelectedNative {
            identity: candidate.identity,
            serial,
            publication,
            origin,
            headers,
            collections: candidate.collections,
            charge: candidate.charge,
        });
        // Selected views leaving the publication keep their native identity.
        // Replay their suppressed edits only after the new membership takes over.
        for id in returning {
            self.update(id, batch);
            self.emit_children(id, batch);
        }
        self.emit_paragraphs(batch);
        Ok(())
    }
    fn native_collections_with(
        &self,
        selected: &[exact_runner::CollectionSnapshot],
    ) -> Result<String, String> {
        let region = slot_region(&self.content_region).ok_or("native region missing")?;
        let limits = region
            .native
            .as_ref()
            .ok_or("native consumer missing")?
            .limits;
        let mut outside = self
            .runner
            .collections_bounded(
                limits.collections,
                limits.collection_rows,
                limits.traversal_entries,
                limits.packet_wire_bound,
            )
            .map_err(str::to_owned)?;
        outside.retain(|c| !self.native_protected_id(c.view));
        crate::content_region::native_collections_json(selected, &outside, limits)
    }
    pub(super) fn native_collections_json_linked(&self) -> Result<String, String> {
        let native = slot_region(&self.content_region)
            .and_then(|r| r.native.as_ref())
            .ok_or("native consumer missing")?;
        self.native_collections_with(
            native
                .selected
                .as_ref()
                .map_or(&[], |a| a.collections.as_slice()),
        )
    }
}

/// The region in a host's slot, which only [`LinkedRegions`] fills.
fn slot_region(slot: &Option<Box<dyn Any>>) -> Option<&RegionState> {
    slot.as_deref()?.downcast_ref()
}
fn slot_region_mut(slot: &mut Option<Box<dyn Any>>) -> Option<&mut RegionState> {
    slot.as_deref_mut()?.downcast_mut()
}

/// The content region, for a host whose entry links `content_regions`.
pub struct LinkedRegions;

impl<D: DataSource> super::region_seam::ContentRegions<D> for LinkedRegions {
    fn boot(
        &self,
        kernel: &mut exact_kernel::Kernel,
        registration: Option<ContentRegionRegistration>,
        native: Option<crate::content_region::NativeProjectionLimits>,
    ) -> Result<Option<Box<dyn Any>>, String> {
        let Some(registration) = registration else {
            return native.map_or(Ok(None), |_| Err("native registration missing".into()));
        };
        let mut region = RegionState::new(kernel, registration)?;
        if let Some(limits) = native {
            region.native = Some(crate::content_region::NativeProjection::new(limits)?);
        }
        Ok(Some(Box::new(region)))
    }
    fn region_layout(
        &self,
        h: &mut Host<D>,
        root: ViewId,
        offer: Offer,
        batch: &mut Batch,
    ) -> Result<(), String> {
        h.region_layout_linked(root, offer, batch)
    }
    fn native_mode(&self, h: &Host<D>) -> bool {
        h.native_mode_linked()
    }
    fn native_current(&self, h: &Host<D>) -> bool {
        h.native_current_linked()
    }
    fn native_selected_id(&self, h: &Host<D>, id: ViewId) -> bool {
        h.native_selected_id_linked(id)
    }
    fn native_captured_id(&self, h: &Host<D>, id: ViewId) -> bool {
        h.native_captured_id_linked(id)
    }
    fn native_moved(&self, h: &mut Host<D>) {
        h.native_moved_linked()
    }
    fn native_retire_removed_owner(&self, h: &mut Host<D>, batch: &mut Batch) {
        h.native_retire_removed_owner_linked(batch)
    }
    fn native_note_receipts(&self, h: &mut Host<D>, receipts: &[Timed]) {
        h.native_note_receipts_linked(receipts)
    }
    fn native_prepare_candidate(&self, h: &mut Host<D>) -> Result<(), String> {
        h.native_prepare_candidate_linked()
    }
    fn native_collections_json(&self, h: &Host<D>) -> Result<String, String> {
        h.native_collections_json_linked()
    }
}

#[cfg(test)]
mod selected_current_tests {
    use super::*;
    use exact_kernel::MonospaceMeasurer;
    use exact_runner::{DataError, Value};

    struct NoData;
    impl DataSource for NoData {
        fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(name.into()))
        }
    }
    fn settle(host: &mut Host<NoData>) {
        let mut count = 0;
        while let Some((id, request)) = host.pending_region_request() {
            let metrics = request.with_request(|r| MonospaceMeasurer::default().measure(r));
            let batch = host.complete_region_text(id, metrics, Rc::new(()));
            assert!(!batch.contains("\"error\":\""), "{batch}");
            count += 1;
            assert!(count <= 64);
        }
    }
    fn wire_current(host: &Host<NoData>) -> bool {
        slot_region(&host.content_region)
            .unwrap()
            .json(host.runner.kernel())
            .contains("\"current\":true")
    }
    #[test]
    fn native_publication_taken_candidate_does_not_make_old_selection_current() {
        let source = r#"component App
  view
    column width="100%" height="100%"
      view id="owner" width="100%" height=400 flex-shrink=0 overflow-x="hidden" overflow-y="hidden"
        view id="content" width="100%" height="100%"
          text "same source repeated words for complete A then complete B" testId="body"
        text "Preparing" id="pending"
      text "outside composer" testId="outside"
"#;
        let plan = contract::compile(source).unwrap().encode();
        let registration = ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        };
        let (mut host, _) = Host::boot_native_region(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            300.,
            600.,
            registration,
            Default::default(),
            exact_runner::time::Launch::NONE,
        )
        .unwrap();
        settle(&mut host);
        assert!(host.native_current() && wire_current(&host));
        let a_mirror = host.mirror.clone();
        let a = slot_region_mut(&mut host.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .selected
            .take()
            .unwrap();
        let body_key = host.runner.kernel().find_by_test_id("body")[0];
        let body = host.runner.kernel().node_by_key(body_key).unwrap().id;
        let outside_key = host.runner.kernel().find_by_test_id("outside")[0];
        let outside = host.runner.kernel().node_by_key(outside_key).unwrap().id;
        host.resize(330., 600.);
        settle(&mut host);
        let b_mirror = host.mirror.clone();
        let b = slot_region_mut(&mut host.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .selected
            .replace(a)
            .unwrap();
        host.mirror = a_mirror;
        let region = slot_region(&host.content_region).unwrap();
        assert!(region.receipt.as_ref().unwrap().current);
        assert!(region.native.as_ref().unwrap().candidate.is_none());
        assert!(!Rc::ptr_eq(
            &region
                .native
                .as_ref()
                .unwrap()
                .selected
                .as_ref()
                .unwrap()
                .publication,
            &b.publication
        ));
        // Real A/B publications, constructed failed-staging state: not an OOM
        // experiment. Candidate is absent but receipt/current still describes B.
        assert!(!host.native_current());
        assert!(!wire_current(&host));
        for key in [body_key, outside_key] {
            host.engine
                .observe(Change {
                    node: motion_node(key),
                    property: Property::Opacity,
                    value: exact_motion::Value::scalar(0.5),
                    velocity: None,
                })
                .unwrap();
        }
        let mut batch = Batch::new();
        host.present(&mut batch, false);
        let raw = batch.finish(None, false, 0., None);
        assert!(
            !raw.contains(&format!("\"op\":\"present\",\"id\":{body},")),
            "{raw}"
        );
        assert!(
            raw.contains(&format!("\"op\":\"present\",\"id\":{outside},")),
            "{raw}"
        );
        slot_region_mut(&mut host.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .selected = Some(b);
        host.mirror = b_mirror;
        assert!(host.native_current() && wire_current(&host));
        host.engine
            .observe(Change {
                node: motion_node(body_key),
                property: Property::Opacity,
                value: exact_motion::Value::scalar(0.75),
                velocity: None,
            })
            .unwrap();
        let mut batch = Batch::new();
        host.present(&mut batch, false);
        assert!(batch
            .finish(None, false, 0., None)
            .contains(&format!("\"op\":\"present\",\"id\":{body},")));
        // Origin and dirty-input negatives must qualify both consumers as well.
        slot_region_mut(&mut host.content_region)
            .unwrap()
            .receipt
            .as_mut()
            .unwrap()
            .origin
            .x += 1.;
        assert!(!host.native_current() && !wire_current(&host));
        slot_region_mut(&mut host.content_region)
            .unwrap()
            .receipt
            .as_mut()
            .unwrap()
            .origin
            .x -= 1.;
        slot_region_mut(&mut host.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .dirty = true;
        assert!(!host.native_current() && !wire_current(&host));
    }

    #[test]
    fn native_publication_diff_refusal_revokes_current_in_public_error_batch() {
        let source = r#"component App
  state corner = 4
  state draft = ""
  action edit(value: string)
    draft = value
  action revise
    corner = 8
  view
    column width="100%" height="100%"
      view id="owner" width="100%" height=400 flex-shrink=0 overflow-x="hidden" overflow-y="hidden"
        view id="content" width="100%" height="100%"
          view testId="body" width=100 height=40 border-radius=corner
        text "Preparing" id="pending"
      button press=revise testId="revise"
        text "Revise"
      input value=draft change=edit testId="input"
      text draft testId="echo"
"#;
        let plan = contract::compile(source).unwrap().encode();
        let boot = |limits| {
            Host::boot_native_region(
                &plan,
                NoData,
                Box::new(MonospaceMeasurer::default()),
                300.,
                600.,
                ContentRegionRegistration {
                    activate: None,
                    owner: "owner",
                    content: "content",
                    pending: "pending",
                },
                limits,
                exact_runner::time::Launch::NONE,
            )
            .unwrap()
        };
        // Calibrate a deterministic logical wire bound, not an allocator failure.
        // The fresh public boot below must actually admit A with that bound.
        let (calibration, _) = boot(NativeProjectionLimits::default());
        assert!(calibration.pending_region_request().is_none());
        let a_bound = slot_region(&calibration.content_region)
            .unwrap()
            .native
            .as_ref()
            .unwrap()
            .selected
            .as_ref()
            .unwrap()
            .charge
            .wire_upper;
        drop(calibration);
        let (mut host, initial) = boot(NativeProjectionLimits {
            diff_wire_bound: a_bound,
            ..Default::default()
        });
        assert!(!initial.contains("\"error\":\""), "{initial}");
        assert!(initial.contains("\"op\":\"native-region\""));
        assert!(initial.contains("\"current\":true"));
        assert!(host.pending_region_request().is_none());
        let region = slot_region(&host.content_region).unwrap();
        let selected = region.native.as_ref().unwrap().selected.as_ref().unwrap();
        let serial = selected.serial;
        let publication = selected.publication.clone();
        let revision = selected.identity.inputs.consumer_revision;
        let mirrors: Vec<_> = selected
            .headers
            .iter()
            .map(|h| (h.id, host.mirror.get(&h.id).unwrap().clone()))
            .collect();
        let collection_bytes = host.collections_json.clone();
        let key = host.runner.kernel().find_by_test_id("revise")[0];
        let id = host.runner.kernel().node_by_key(key).unwrap().id;
        let failed = host.dispatch_at(id, exact_runner::Event::Press, 10.);
        assert!(failed.contains("native diff admission"), "{failed}");
        assert!(
            host.pending_region_request().is_none(),
            "B is immediately ready"
        );
        let region = slot_region(&host.content_region).unwrap();
        let native = region.native.as_ref().unwrap();
        let selected = native.selected.as_ref().unwrap();
        let receipt = region.receipt.as_ref().unwrap();
        let RegionSelection::Accepted(b) = &receipt.selection else {
            panic!("the refusal must occur after complete B");
        };
        assert!(receipt.current);
        assert!(!Rc::ptr_eq(&publication, b));
        assert!(native.revision > revision);
        assert!(
            native.candidate.is_some(),
            "diff admission precedes candidate.take"
        );
        assert_eq!(selected.serial, serial);
        assert!(Rc::ptr_eq(&selected.publication, &publication));
        assert_eq!(host.collections_json, collection_bytes);
        for (id, before) in &mirrors {
            assert_eq!(host.mirror.get(id), Some(before));
        }
        assert!(!host.native_current());
        // The actual returned public error envelope must revoke authority. An
        // error string or a direct state query is not a replacement for this op.
        let state = region.json(host.runner.kernel());
        assert!(state.contains("\"current\":false"));
        assert!(state.contains(&format!("\"publication\":\"{serial}\"")));
        eprintln!(
            "diff bound={a_bound} retained={serial} current=false public_update={}",
            failed.contains(&state)
        );
        let revoked = failed.contains(&state);
        for (id, _) in mirrors {
            for op in [
                "create", "props", "style", "children", "frame", "content", "present", "destroy",
            ] {
                assert!(
                    !failed.contains(&format!("\"op\":\"{op}\",\"id\":{id},"))
                        && !failed.contains(&format!("\"op\":\"{op}\",\"id\":{id}}}")),
                    "partial B op: {failed}"
                );
            }
        }
        let input_key = host.runner.kernel().find_by_test_id("input")[0];
        let input = host.runner.kernel().node_by_key(input_key).unwrap().id;
        let outside = host.dispatch_at(
            input,
            exact_runner::Event::Change("outside progresses".into()),
            11.,
        );
        assert!(outside.contains("outside progresses"), "{outside}");
        assert!(outside.contains("native diff admission"), "{outside}");
        assert_eq!(host.region_publication_id(), Some(serial));
        let outside_state = slot_region(&host.content_region)
            .unwrap()
            .json(host.runner.kernel());
        assert!(outside_state.contains("\"current\":false"));
        let outside_revoked = outside.contains(&outside_state);
        // A bounded test-only limit restoration exercises recovery through the
        // ordinary public resize path, not direct promotion or fabricated rows.
        slot_region_mut(&mut host.content_region)
            .unwrap()
            .native
            .as_mut()
            .unwrap()
            .limits
            .diff_wire_bound = NativeProjectionLimits::default().diff_wire_bound;
        let recovered = host.resize(300., 600.);
        assert!(!recovered.contains("\"error\":\""), "{recovered}");
        assert!(host.native_current());
        assert_ne!(host.region_publication_id(), Some(serial));
        let region = slot_region(&host.content_region).unwrap();
        let selected = region.native.as_ref().unwrap().selected.as_ref().unwrap();
        let RegionSelection::Accepted(current_b) = &region.receipt.as_ref().unwrap().selection
        else {
            panic!("recovery must select complete B");
        };
        assert!(Rc::ptr_eq(&selected.publication, current_b));
        assert!(!Rc::ptr_eq(&selected.publication, &publication));
        let current = region.json(host.runner.kernel());
        assert!(current.contains("\"current\":true"));
        assert!(recovered.contains(&current), "{recovered}");
        assert!(
            revoked && outside_revoked,
            "public error batch omitted retained-state revocation: {failed}; outside: {outside}"
        );
    }

    #[test]
    fn region_runs_carry_their_boxes_lines_in_the_boxes_colours() {
        // CSS Text Decoration 3 §2.1: the paragraph's and the inline box's
        // lines cross the leaves below them, each in its own box's colour,
        // the nearest box's where both draw one.
        let source = r##"component App
  view
    column width="100%" height="100%"
      view id="owner" width="100%" height=400 flex-shrink=0 overflow-x="hidden" overflow-y="hidden"
        view id="content" width="100%" height="100%"
          text text-decoration="line-through" color="#ff0000" testId="body"
            text "plain "
            text text-decoration="underline line-through" color="#00ff00"
              text "under" color="#0000ff"
            text " own" text-decoration="line-through" color="#0000ff"
        text "Preparing" id="pending"
"##;
        let plan = contract::compile(source).unwrap().encode();
        let registration = ContentRegionRegistration {
            activate: None,
            owner: "owner",
            content: "content",
            pending: "pending",
        };
        let (host, _) = Host::boot_native_region(
            &plan,
            NoData,
            Box::new(MonospaceMeasurer::default()),
            300.,
            600.,
            registration,
            Default::default(),
            exact_runner::time::Launch::NONE,
        )
        .unwrap();
        let (id, _) = host.pending_region_request().unwrap();
        let wire: serde_json::Value =
            serde_json::from_str(&host.region_request_json(id).unwrap()).unwrap();
        let runs = wire["runs"].as_array().unwrap();
        let row = |text: &str| runs.iter().find(|r| r["text"] == text).unwrap();
        let red = serde_json::json!([255, 0, 0, 255]);
        let green = serde_json::json!([0, 255, 0, 255]);
        assert_eq!(row("plain ")["strikeColor"], red);
        assert!(row("plain ")["underlineColor"].is_null());
        assert_eq!(row("under")["strikeColor"], green);
        assert_eq!(row("under")["underlineColor"], green);
        assert_eq!(row("under")["decoration"], "");
        // Its own line-through is its own and is drawn in its own colour
        // (`RegionTextSource`); the wire still names its boxes'.
        assert_eq!(row(" own")["decoration"], "line-through");
        assert_eq!(row(" own")["strikeColor"], red);
    }
}
