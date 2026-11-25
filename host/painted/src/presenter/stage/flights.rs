//! Shared elements across a route change on the stage (LLP 1013.004 D4).
use super::*;

/// Compose's default `boundsTransform` for a shared element, `spring()`
/// with damping ratio 1 and stiffness 400 (LLP 1013.004 D4).
pub(super) fn bounds_spring() -> Transition {
    Transition {
        property: exact_motion::TransitionProperty::All,
        duration: 0.0,
        delay: 0.0,
        timing: exact_motion::TimingFunction::Spring(exact_motion::SpringConfig {
            stiffness: 400.0,
            damping: 40.0,
            mass: 1.0,
        }),
    }
}

impl<D: DataSource> Presenter<D> {
    /// The names a push from `old` to `new`, or a pop of `old`, pairs.
    pub(super) fn pairs(
        &self,
        forward: bool,
        (old, new): (NodeKey, NodeKey),
    ) -> Vec<(NodeKey, NodeKey)> {
        match forward {
            true => pair_routes(self.host.kernel(), old, new),
            false if self.stage.pops.1 == Some(old) => self.stage.pops.2.clone(),
            false => Vec::new(),
        }
    }

    /// Fly the names a push from `old` to `new`, or a pop of `old`, pairs.
    pub(super) fn fly_routes(
        &mut self,
        forward: bool,
        (old, new): (NodeKey, NodeKey),
        curve: &Transition,
    ) {
        for (from, to) in self.pairs(forward, (old, new)) {
            self.host.fly(from, to, curve, true);
        }
    }

    /// Take the crossing's pose off each flight's destination, so the
    /// reader aims at its rest, as Compose aims at the lookahead bounds
    /// (LLP 1013.004 D4). On a container's clock, from the source to the
    /// rest, they move with its box as one transform: aimed at their pose
    /// in it, they would move with it twice.
    pub(crate) fn unpose_flights(&mut self) {
        let crossing = |s: &&Shift| !s.through;
        let Some(shift) = self.stage.shift.as_ref().filter(crossing) else {
            return;
        };
        let poses = self.shift_poses(self.host.now(), shift.sheet);
        let kernel = self.host.kernel();
        for f in &mut self.brush.shared.frames {
            let mut route = kernel.node_by_key(f.to);
            while let Some(node) = route
                .as_ref()
                .filter(|n| ![shift.incoming, shift.outgoing].contains(&n.key))
            {
                route = node.parent.and_then(|id| kernel.node(id));
            }
            let Some(route) = route else {
                continue;
            };
            let pose = if route.key == shift.incoming {
                poses.1
            } else {
                poses.0
            };
            let mut at = (0.0, 0.0);
            let mut next = Some(route);
            while let Some(node) = next {
                at = (at.0 + node.frame.x, at.1 + node.frame.y);
                next = node.parent.and_then(|id| kernel.node(id));
            }
            // posed = at + d + o + s(p - o) for p in the route; rest = at + p.
            let (o, d, s) = (pose.origin, (pose.dx, pose.dy), pose.scale);
            // A route scaled to nothing has no rest to aim at yet.
            if s.abs() < 1e-3 {
                continue;
            }
            f.unpose = [
                1.0 / s,
                at.0 + o.0 - (at.0 + d.0 + o.0) / s,
                at.1 + o.1 - (at.1 + d.1 + o.1) / s,
            ];
        }
    }
}
