//! A height handle shares vertical travel with its list when the list itself is the handle.
use super::contact::{Candidate, HeldKind, Hold};
use super::*;
use exact_kernel::NodeKey;
use exact_motion::{HoldEnd, Value, VelocityTracker};

#[derive(Clone, Copy, Default)]
pub(super) struct ListDrag {
    sheet_moved: bool,
    list_started: bool,
    list_last: bool,
    // Only finger travel contributes to velocity; a scroll handler can reposition the port.
    travel: Value,
}

impl<D: DataSource> Presenter<D> {
    fn list_handle(&self, node: &exact_kernel::NodeRef<'_>) -> bool {
        if !node.node_type.scrolls_by_default()
            || !matches!(
                crate::paint::effective_overflow(node).1,
                Overflow::Auto | Overflow::Scroll
            )
        {
            return false;
        }
        if node.node_type != NodeType::List {
            return true;
        }
        if let Some(list) = self.host.collection(node.id) {
            return list.axis == exact_runner::ListAxis::Vertical;
        }
        let range = self.scroll_range(node.id, &Default::default());
        range.0 <= 0.5 || range.1 > 0.5
    }

    pub(super) fn height_candidate(&self, hit: NodeKey) -> Option<Candidate> {
        if !self.input_live(hit) {
            return None;
        }
        let mut at = self.host.kernel().node_by_key(hit).map(|n| n.id);
        let mut pressed = false;
        let mut inner_port = false;
        while let Some(id) = at {
            let node = self.host.kernel().node(id)?;
            if node.props.str(PropId::HeightDragFor).is_some() {
                let shares = self.list_handle(&node);
                if (shares && inner_port)
                    || (!shares && (pressed || node.node_type.scrolls_by_default()))
                {
                    return None;
                }
                return self
                    .host
                    .height_drag_target(node.key)
                    .map(|target| Candidate::Height {
                        handle: node.key,
                        target,
                    });
            }
            if node.node_type == NodeType::TextInput {
                return None;
            }
            inner_port |= self.list_handle(&node);
            pressed |= self
                .host
                .runner()
                .handlers_of(id)
                .contains(&EventKind::Press);
            at = node.parent;
        }
        None
    }
    pub(super) fn begin_height_drag(
        &mut self,
        handle: NodeKey,
        target: NodeKey,
        now_ms: f64,
        at_ms: f64,
    ) -> Result<Option<Hold>, String> {
        let Some(node) = self.host.kernel().node_by_key(handle) else {
            return Ok(None);
        };
        let view = node.id;
        let list = self.list_handle(&node).then(ListDrag::default);
        let primary = if list.is_some() {
            self.stop_glide(view);
            None
        } else {
            let Some(start) = self.host.height_drag_begin(handle, target, now_ms)? else {
                return Ok(None);
            };
            Some(start)
        };
        self.set_collection_interaction(Some(view));
        self.dirty = true;
        let mut held = Hold {
            primary,
            velocity: VelocityTracker::new(),
            kind: HeldKind::Height {
                handle,
                target,
                list,
            },
            sampled: None,
            lifting: false,
        };
        held.sample(at_ms, Value::ZERO, primary.map_or(Value::ZERO, |p| p.value));
        Ok(Some(held))
    }
    pub(super) fn move_height_drag(
        &mut self,
        held: &mut Hold,
        input: Value,
        now_ms: f64,
        at_ms: f64,
    ) -> Result<bool, String> {
        let HeldKind::Height {
            handle,
            target,
            list,
        } = held.kind
        else {
            unreachable!()
        };
        if let Some(mut list) = list {
            let Some(port) = self.host.kernel().node_by_key(handle).map(|n| n.id) else {
                return Ok(false);
            };
            let Some(sheet) = self.host.kernel().node_by_key(target) else {
                return Ok(false);
            };
            let (sheet_id, before) = (sheet.id, sheet.frame.height);
            let fixed_limit = |d: &exact_kernel::Dimension| match d {
                exact_kernel::Dimension::Points(v) => Some(*v),
                _ => None,
            };
            let min = fixed_limit(&sheet.style.min_height);
            let max = fixed_limit(&sheet.style.max_height);
            let previous = held.sampled.map_or(Value::ZERO, |(_, point)| point);
            let dx = (input.x - previous.x) as f32;
            let dy = (input.y - previous.y) as f32;
            // Both transforms read the same finger sample. Translation drops
            // out, including the sheet's change of origin since the last sample.
            let mut local_y = |id| {
                self.box_of(id)
                    .and_then(|b| {
                        let transform = b.local_transform()?;
                        Some(transform.ky * dx + transform.sy * dy)
                    })
                    .unwrap_or(dy)
            };
            let travel = local_y(sheet_id);
            let list_travel = local_y(port);
            let offset = self.scroll_of(port);
            let offered = if travel < 0. && list_travel < 0. {
                (list_travel + offset.1.min(-list_travel)) * (travel / list_travel)
            } else {
                travel
            };
            if (offered > 0. && max.is_none_or(|h| before < h))
                || (offered < 0. && min.is_none_or(|h| before > h))
            {
                if held.primary.is_none() {
                    held.primary = self.host.height_drag_begin(handle, target, now_ms)?;
                }
                let Some(motion) = held.primary else {
                    return Ok(false);
                };
                if !self.height_update(motion.token, (before + offered).max(0.) as f64, now_ms)? {
                    return Ok(false);
                }
            }
            let Some(height) = self
                .host
                .kernel()
                .node_by_key(target)
                .map(|n| n.frame.height)
            else {
                return Ok(false);
            };
            let spent = height - before;
            let rest = if travel == 0. {
                list_travel
            } else {
                let unspent = travel - spent.clamp(travel.min(0.), travel.max(0.));
                unspent * (list_travel / travel)
            };
            let current = self.scroll_of(port);
            let mut scrolled = 0.;
            if rest != 0. {
                let max = self.scroll_range(port, &self.collection_scroll_limits()).1;
                let y = (current.1 + rest).clamp(0., max);
                if y != current.1 {
                    if !list.list_started {
                        list.list_started = true;
                        held.kind = HeldKind::Height {
                            handle,
                            target,
                            list: Some(list),
                        };
                        if let Some(error) = self.list_drag_began(port) {
                            return Err(error);
                        }
                    }
                    if self.host.height_drag_target(handle) != Some(target) {
                        return Ok(false);
                    }
                    let at = self.scroll_of(port);
                    if let Some((to, _)) = self.scroll_to_completion(
                        port,
                        (at.0, at.1 + rest),
                        super::scroll_end::Completion::Reader,
                        Some((0., rest)),
                    ) {
                        scrolled = to.1 - at.1;
                    }
                }
            }
            list.sheet_moved |= spent != 0.;
            // When both moved in one sample, a rise finishes on the list,
            // a descent on the sheet. A stationary lift keeps its last owner.
            if scrolled != 0. || spent != 0. {
                list.list_last = scrolled != 0. && (spent == 0. || travel > 0.);
            }
            list.travel.x += f64::from(spent);
            list.travel.y += f64::from(scrolled);
            held.kind = HeldKind::Height {
                handle,
                target,
                list: Some(list),
            };
            held.sample(at_ms, input, list.travel);
            return Ok(true);
        }
        let px = (held.motion().value.x + input.y).clamp(0., f32::MAX as f64);
        if !self.height_update(held.motion().token, px, now_ms)? {
            return Ok(false);
        }
        let Some(node) = self.host.kernel().node_by_key(target) else {
            return Ok(false);
        };
        // Record the actual constrained presentation, not finger displacement.
        held.sample(
            at_ms,
            Value::scalar(input.y),
            Value::scalar(node.frame.height as f64),
        );
        Ok(true)
    }
    pub(super) fn finish_height_drag(&mut self, held: &Hold, now_ms: f64) -> Result<bool, String> {
        let HeldKind::Height {
            handle,
            target,
            list,
        } = held.kind
        else {
            unreachable!()
        };
        let measured = held.release_velocity();
        let velocity = Value::scalar(if list.is_some_and(|d| d.list_last) {
            0.
        } else {
            measured.x
        });
        if let Some(list) = list {
            if list.list_started {
                if let Some(port) = self.host.kernel().node_by_key(handle).map(|n| n.id) {
                    if list.list_last && self.reader.lifting_touch {
                        self.reader.height_scroll_release = Some((handle, measured.y as f32));
                        self.reader.pending_height_end = Some(handle);
                    } else {
                        let velocity = if list.list_last { measured.y } else { 0. };
                        if list.list_last {
                            self.coast_list(port, velocity);
                        }
                        let rest = self.glides.target(port);
                        if let Some(error) =
                            self.list_drag_ended(port, (0., velocity), rest, rest.is_some())
                        {
                            self.host.log(error);
                        }
                        if rest.is_some() {
                            // The glide owns scrollend until it lands or another move interrupts it.
                            self.scroll_end_moved(port, super::scroll_end::Completion::Glide);
                            self.scroll_end_release(port);
                        }
                    }
                }
            }
            if !list.sheet_moved {
                return match held.primary {
                    Some(m) => self.height_end(m.token, HoldEnd::Cancel, now_ms),
                    None => Ok(true),
                };
            }
        }
        let height = self
            .host
            .kernel()
            .node_by_key(target)
            .map_or(0., |n| n.frame.height as f64);
        let dispatched =
            self.host
                .dispatch_height_held(held.motion().token, handle, height, velocity.x, now_ms);
        let after = self.after_commit();
        let end = if matches!(dispatched, Ok(true)) {
            HoldEnd::Release { velocity }
        } else {
            HoldEnd::Cancel
        };
        let ended = self.height_end(held.motion().token, end, now_ms);
        match dispatched {
            Err(e) => Err(e),
            Ok(accepted) => after.or(ended.err()).map_or(Ok(accepted), Err),
        }
    }
    pub(super) fn cancel_height_drag(
        &mut self,
        held: &Hold,
        end: HoldEnd,
        now_ms: f64,
    ) -> Result<(), String> {
        if let HeldKind::Height {
            handle,
            list: Some(list),
            ..
        } = held.kind
        {
            if list.list_started {
                if let Some(port) = self.host.kernel().node_by_key(handle).map(|n| n.id) {
                    if let Some(error) = self.list_drag_ended(port, (0., 0.), None, false) {
                        self.host.log(error);
                    }
                }
            }
        }
        if let Some(m) = held.primary {
            self.height_end(m.token, end, now_ms)?;
        }
        Ok(())
    }

    /// List id, offset velocity in points/s and the inverse transform’s y row for the reader’s own velocity.
    pub fn take_height_scroll_release(&mut self) -> Option<(ViewId, f32, [f32; 2])> {
        let (key, velocity) = self.reader.height_scroll_release.take()?;
        let id = self.host.kernel().node_by_key(key)?.id;
        let projection = self
            .box_of(id)
            .and_then(|b| b.local_transform())
            .map_or([0., 1.], |t| [t.ky, t.sy]);
        Some((id, velocity, projection))
    }
}
