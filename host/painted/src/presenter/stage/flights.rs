//! Shared elements across a route change on the stage (LLP 1013.004 D4).
use super::*;
use exact_kernel::handoff::pair_routes;

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
            // A node's frame is absolute.
            let at = (route.frame.x, route.frame.y);
            // posed = at + d + o + s(p - o) for p in the route, s each
            // axis's scale; rest = at + p.
            let (o, d) = (pose.origin, (pose.dx, pose.dy));
            let s = (
                pose.scale * pose.scale_axes.0,
                pose.scale * pose.scale_axes.1,
            );
            // A route scaled to nothing has no rest to aim at yet, and a
            // turned one's is no box's scale and offset (LLP 1013.004).
            if s.0.abs() < 1e-3 || s.1.abs() < 1e-3 || pose.rotate != 0.0 {
                continue;
            }
            f.unpose = [
                1.0 / s.0,
                1.0 / s.1,
                at.0 + o.0 - (at.0 + d.0 + o.0) / s.0,
                at.1 + o.1 - (at.1 + d.1 + o.1) / s.1,
            ];
        }
    }

    /// The pairs a pop of the top route would fly, kept: a pop has removed
    /// the route by the time its crossing starts (LLP 1013.004 D1).
    pub(super) fn remember_pops(&mut self, keys: &[NodeKey], kinds: &[Presented]) {
        let epoch = self.host.kernel().epoch();
        if (self.stage.pops.0, self.stage.pops.1) != (epoch, keys.last().copied()) {
            let pairs = match keys.len() {
                n if n > 1 && kinds[n - 1] != Presented::Sheet => {
                    pair_routes(self.host.kernel(), keys[n - 1], keys[n - 2])
                }
                _ => Vec::new(),
            };
            self.stage.pops = (epoch, keys.last().copied(), pairs);
        }
    }

    /// A Back preview ends, and a pop that flies nothing lets its flights go.
    pub(super) fn settle_preview(&mut self, keys: &[NodeKey], now: f64) {
        // A cancelled preview shows the route beneath until the flights it
        // sent back have landed.
        let (host, pairs) = (&self.host, &self.stage.pops.2);
        self.stage.preview.take_if(|p| {
            !p.live(keys) || p.returned(now) && !pairs.iter().any(|(_, to)| host.in_flight(*to))
        });
        // A pop whose crossing flies nothing (a fade through) ends a
        // preview's flights where Back went; a container lets its names go
        // when it lands.
        let carries = (self.stage.shift.as_ref()).is_some_and(|s| s.names.is_some());
        if self.stage.preview.is_none() && !carries {
            self.host.release_flights(&bounds_spring(), true);
        }
    }

    /// A crossing's names under motion turned off, on a container's clock,
    /// and as the crossing ends.
    pub(super) fn settle_shift(&mut self, now: f64) {
        if self.motion_off() {
            if self.stage.shift.take().is_some_and(|s| s.names.is_some()) {
                self.host.land_seeking();
            }
            self.stage.preview = None;
        }
        if let Some((names, seeked)) = (self.stage.shift.as_ref()).and_then(|s| {
            let (names, from) = s.names.as_ref()?;
            Some((
                names.clone(),
                from + (1.0 - from) * s.progress.at(now).clamp(0.0, 1.0),
            ))
        }) {
            self.host.seek_flights(&names, seeked);
        }
        // A route that flies in as a whole covers the route beneath only
        // when it lands, so that one is drawn until then. A container's own
        // names land with it, below.
        let flies_in = |s: &Shift| {
            s.forward
                && !(s.names.as_ref())
                    .is_some_and(|(n, _)| n.iter().any(|&(_, to)| to == s.incoming))
                && (self.host.kernel().node_by_key(s.incoming))
                    .is_some_and(|n| self.host.flying(n.id))
        };
        if (self.stage.shift.as_ref()).is_some_and(|s| !s.progress.moving(now) && !flies_in(s)) {
            if self.stage.shift.take().is_some_and(|s| s.names.is_some()) {
                self.host.land_seeking();
            }
        }
    }

    /// Fly the names a crossing from `old` to `new` pairs; `kind`: whether
    /// it is a fade through, and a container.
    pub(super) fn fly_shift(
        &mut self,
        forward: bool,
        (old, new): (NodeKey, NodeKey),
        kind: (bool, bool),
        seeked: f32,
    ) {
        // The names both routes hold fly on the crossing's own timing,
        // whatever their authored curve (LLP 1013.004 D2, D4).
        match kind {
            (true, _) => {}
            (false, true) => {
                let pairs = self.pairs(forward, (old, new));
                self.host.seek_flights(&pairs, seeked);
                if let Some(s) = self.stage.shift.as_mut() {
                    s.names = Some((pairs, seeked));
                }
            }
            (false, false) => self.fly_routes(forward, (old, new), &bounds_spring()),
        }
    }
}
