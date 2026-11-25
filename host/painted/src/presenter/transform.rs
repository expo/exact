//! A transform binding’s paired translate and scale hold (LLP 1057.001 §4).
use super::contact::{Candidate, HeldKind, Hold};
use super::*;
use exact_kernel::{NodeKey, TransformDragBinding};
use exact_motion::{TransformHold, Value, VelocityTracker};

impl<D: DataSource> Presenter<D> {
    pub(super) fn transform_candidate(&self, hit: NodeKey) -> Option<Candidate> {
        if !self.input_live(hit) {
            return None;
        }
        let mut at = self.host.kernel().node_by_key(hit).map(|n| n.id);
        while let Some(id) = at {
            let node = self.host.kernel().node(id)?;
            if node.props.str(PropId::TransformDragFor).is_some() {
                let binding = self.host.transform_drag_binding(node.key)?;
                self.transform_revision(binding)?;
                return Some(Candidate::Transform(binding));
            }
            if node.node_type == NodeType::TextInput
                || self
                    .host
                    .runner()
                    .handlers_of(id)
                    .contains(&EventKind::Press)
            {
                return None;
            }
            at = node.parent;
        }
        None
    }
    pub(super) fn begin_transform_drag(
        &mut self,
        binding: TransformDragBinding,
        now_ms: f64,
        at_ms: f64,
    ) -> Result<Option<Hold>, String> {
        let Some(revision) = self.transform_revision(binding) else {
            return Ok(None);
        };
        let Some(pair) = self.host.transform_drag_begin(binding, now_ms)? else {
            return Ok(None);
        };
        let primary = pair.translate();
        let view = self
            .host
            .kernel()
            .node_by_key(binding.handle)
            .expect("live handle")
            .id;
        self.set_collection_interaction(Some(view));
        self.dirty = true;
        let mut held = Hold {
            primary: Some(primary),
            velocity: VelocityTracker::new(),
            kind: HeldKind::Transform {
                time_offset: now_ms - at_ms,
                binding,
                pair,
                revision,
            },
            sampled: None,
            lifting: false,
        };
        held.sample(at_ms, Value::ZERO, primary.value);
        Ok(Some(held))
    }
    pub(super) fn move_transform_drag(
        &mut self,
        held: &mut Hold,
        delta: Value,
        pinched: Option<[Value; 2]>,
        now_ms: f64,
        at_ms: f64,
    ) -> Result<bool, String> {
        let HeldKind::Transform {
            binding,
            pair,
            time_offset,
            revision,
        } = held.kind
        else {
            unreachable!()
        };
        if self.transform_revision(binding) != Some(revision) {
            return Ok(false);
        }
        let [translate, scale] = pinched.unwrap_or_else(|| {
            [
                Value::new(
                    pair.translate().value.x + delta.x,
                    pair.translate().value.y + delta.y,
                ),
                pair.scale().value,
            ]
        });
        let sample = Value::four(translate.x, translate.y, scale.x, 0.);
        if let Some((at, was)) = held.sampled.filter(|_| held.lifting && pinched.is_some()) {
            if !exact_motion::gesture::lift_samples(sample != was, (at_ms - at) / 1000.) {
                return Ok(true);
            }
        }
        let accepted = self.host.transform_drag_update_at(
            pair,
            binding,
            [translate, scale],
            now_ms,
            at_ms + time_offset,
        )?;
        if accepted {
            if pinched.is_some() {
                held.sampled = Some((at_ms, sample));
            } else {
                held.sample(at_ms, delta, translate);
            }
            self.dirty = true;
        }
        Ok(accepted)
    }
    pub(super) fn finish_transform_drag(
        &mut self,
        held: &Hold,
        pinched: bool,
        now_ms: f64,
    ) -> Result<bool, String> {
        let HeldKind::Transform {
            binding,
            pair,
            time_offset,
            revision,
        } = held.kind
        else {
            unreachable!()
        };
        if self.transform_revision(binding) != Some(revision)
            || !self.host.transform_hold_live(pair, binding)
        {
            self.end_transform_drag(pair, None, now_ms)?;
            return Ok(false);
        }
        let values = [
            self.host
                .engine()
                .value(
                    pair.translate().token.node(),
                    exact_motion::Property::Translate,
                )
                .expect("live translate"),
            self.host
                .engine()
                .value(pair.scale().token.node(), exact_motion::Property::Scale)
                .expect("live scale"),
        ];
        let velocities = if pinched {
            let at = held.sampled.unwrap().0 + time_offset;
            [pair.translate(), pair.scale()].map(|s| {
                self.host
                    .engine()
                    .hold_velocity(s.token, at / 1000.)
                    .unwrap_or(Value::ZERO)
            })
        } else {
            [held.release_velocity(), Value::ZERO]
        };
        let dispatched = self
            .host
            .dispatch_transform_held(pair, binding, values, velocities, now_ms);
        let after = self.after_commit();
        let end = self.end_transform_drag(
            pair,
            matches!(dispatched, Ok(true)).then_some(velocities),
            now_ms,
        );
        match dispatched {
            Err(e) => Err(e),
            Ok(accepted) => after.or(end.err()).map_or(Ok(accepted), Err),
        }
    }
    pub(super) fn end_transform_drag(
        &mut self,
        pair: TransformHold,
        velocities: Option<[Value; 2]>,
        now_ms: f64,
    ) -> Result<(), String> {
        let result = self.host.transform_drag_end(pair, velocities, now_ms);
        self.dirty = true;
        result
    }
}
