//! Host state changes feed presentation targets without changing layout or authored style.
use super::{motion_node, node_key, MotionSync};
use crate::{Kernel, NodeKey, NodeRef, PropId, StyleId, LOOK_FOCUS_VISIBLE, LOOK_HOVER};
use exact_motion::{Change, Engine, EngineError, Property, Value};
use std::collections::BTreeMap;

/// Host-known state bits whose scale participates in ordinary CSS transitions.
#[derive(Debug, Default)]
pub struct LookMotion(BTreeMap<NodeKey, u8>);

impl LookMotion {
    /// Record hover/focus and observe the resulting scale, on the engine's clock.
    pub fn observe(
        &mut self,
        node: &NodeRef<'_>,
        states: u8,
        engine: &mut Engine,
    ) -> Result<(), EngineError> {
        if states == 0 {
            self.0.remove(&node.key);
        } else {
            self.0.insert(node.key, states);
        }
        engine.observe(Change {
            node: motion_node(node.key),
            property: Property::Scale,
            value: Value::scalar(scale(node, states) as f64),
            velocity: None,
        })
    }

    /// Keep active looks above the base when a commit changes the node's style.
    pub fn apply(&mut self, kernel: &Kernel, sync: &mut MotionSync) {
        for removed in &sync.removed {
            self.0.remove(&node_key(*removed));
        }
        for change in &mut sync.changes {
            if change.property != Property::Scale {
                continue;
            }
            let key = node_key(change.node);
            if let Some(states) = self.0.get(&key) {
                if let Some(node) = kernel.node_by_key(key) {
                    change.value = Value::scalar(scale(&node, *states) as f64);
                }
            }
        }
    }
}

fn scale(node: &NodeRef<'_>, states: u8) -> f32 {
    let mut value = node.style.scale;
    if node.props.bool(PropId::Disabled) == Some(true) {
        return value;
    }
    for (state, look) in node.looks() {
        let bit = match *state {
            LOOK_HOVER => 1,
            LOOK_FOCUS_VISIBLE => 2,
            _ => 0,
        };
        if states & bit != 0 && look.mask.union(look.written).has(StyleId::Scale) {
            value = if look.mask.has(StyleId::Scale) {
                look.scale
            } else {
                1.
            };
        }
    }
    value
}
