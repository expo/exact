//! A route's own entrance and exit (LLP 1013.001): the `::view-transition-new`
//! and `::view-transition-old` parts its author declared, played on the
//! stage in place of Material's shared axis, and on a shell with no bars,
//! which has no motion of its own for routes, because the author asked for
//! motion.
//!
//! The route a change adds or removes owns it: its keyframes move a picture
//! of it, about its centre, as CSS moves the part's image, while the route
//! beneath rests. On a push the route beneath stays drawn, and the arriving
//! route takes no input, until the entrance ends; on a pop the kept drawing
//! of the route that left is posed over the route it reveals. Predictive
//! Back seeks the exit, linearly in time, as Compose seeks a destination's
//! `popExitTransition`.
use super::*;
use exact_motion::Animations;

/// A part a crossing plays: the owner's keyframes over its box.
#[derive(Debug, Clone)]
pub(super) struct Authored {
    animations: Animations,
    /// The owner's border box, points: `translate`'s percentages are of it,
    /// and it turns and scales about its centre.
    size: (f32, f32),
}

impl Authored {
    /// How long the part plays, ms: until its last animation ends.
    pub(super) fn ms(&self) -> f64 {
        self.animations.end_time() * 1000.0
    }

    /// The owner's pose `ms` into the part, its picture about its centre.
    pub(super) fn pose(&self, ms: f64) -> Pose {
        let (w, h) = self.size;
        let picture = (self.animations).picture(ms.max(0.0) / 1000.0, (f64::from(w), f64::from(h)));
        Pose {
            dx: picture.translate.0 as f32,
            dy: picture.translate.1 as f32,
            scale_axes: (picture.scale.0 as f32, picture.scale.1 as f32),
            rotate: picture.rotate as f32,
            origin: (w / 2.0, h / 2.0),
            opacity: picture.opacity as f32,
            ..Pose::REST
        }
    }
}

/// What the stage keeps of the top route between syncs: a pop removes it
/// before the stage hears of the pop, and its exit is played from this.
#[derive(Debug, Clone)]
pub(super) struct Top {
    pub(super) key: NodeKey,
    /// Its `::view-transition-old`, where one is declared and may play.
    pub(super) exit: Option<Authored>,
    /// It was pushed, not presented full screen: in a sheet, if one is
    /// beneath it.
    pub(super) pushed: bool,
    /// Its ancestors' keys, nearest first.
    pub(super) ancestors: Vec<NodeKey>,
}

/// The part `row` of a route, where it is declared and the route may play
/// one (D2): not a sheet, and no zoom from a `navigationSource` it computes.
/// `None` is the platform's transition; an empty list is the author's cut.
pub(super) fn part(node: &exact_kernel::NodeRef<'_>, row: StyleId) -> Option<Authored> {
    if !node.style.mask.has(row) || Presented::of(node) == Presented::Sheet {
        return None;
    }
    if (node.props.str(PropId::NavigationSource)).is_some_and(|s| !s.is_empty()) {
        return None;
    }
    let animations = match row {
        StyleId::ViewTransitionNew => &node.style.rare.view_transition_new,
        _ => &node.style.rare.view_transition_old,
    };
    Some(Authored {
        animations: animations.clone(),
        size: (node.frame.width, node.frame.height),
    })
}

impl<D: DataSource> Presenter<D> {
    /// The entrance the route at `top`, just pushed or presented, declares.
    pub(super) fn entrance(&self, stack: &[ViewId], top: usize) -> Option<Authored> {
        let node = self.host.kernel().node(*stack.get(top)?)?;
        part(&node, StyleId::ViewTransitionNew)
    }

    /// The top route, kept for the pop that may remove it.
    pub(super) fn top_of(&self, stack: &[ViewId], kinds: &[Presented]) -> Option<Top> {
        let index = stack.len().checked_sub(1)?;
        let kernel = self.host.kernel();
        let node = kernel.node(stack[index])?;
        Some(Top {
            key: node.key,
            exit: part(&node, StyleId::ViewTransitionOld),
            pushed: kinds[index] == Presented::Pushed,
            ancestors: keys_above(kernel, node.id),
        })
    }

    /// Play `part` for the change from `old` to `new` on top, `forward` a
    /// push, its owner `pushed` (not presented full screen): from where
    /// predictive Back left it, cut while the person asks for less motion or
    /// when the part is `none`.
    pub(super) fn start_authored(
        &mut self,
        part: Authored,
        forward: bool,
        pushed: bool,
        (old, new): (NodeKey, NodeKey),
        kinds: &[Presented],
        now: f64,
    ) {
        let top = kinds.len() - 1;
        // The route beneath the owner, and the sheet the owner shows in.
        let beneath = match forward {
            true => top.checked_sub(1),
            false => Some(top),
        };
        let within = pushed.then(|| sheet_of(kinds, top)).flatten();
        let sheet = within.map(|i| self.stage.seen[i]);
        let alone = self.stage_presents()
            && within.is_none()
            && beneath.is_some_and(|i| sheet_of(kinds, i).is_some());
        let seek = (self.stage.preview.take()).and_then(|p| p.progress_for(old));
        let ms = part.ms();
        if self.motion_off() || ms <= 0.0 {
            self.stage.shift = None;
            return;
        }
        let start = now - f64::from(seek.unwrap_or(0.0)) * ms;
        let bar = (self.stage.bar.as_ref())
            .filter(|(route, ..)| *route == old && sheet.is_none())
            .map(|(_, tape, frame)| (tape.clone(), *frame));
        self.stage.shift = Some(Shift {
            forward,
            outgoing: old,
            incoming: new,
            sheet,
            progress: Glide::new(0.0, 1.0, start, ms, Curve::Standard),
            bar,
            from: None,
            authored: Some(part),
            alone,
            container: None,
            through: false,
            left: Vec::new(),
        });
    }

    /// The exit predictive Back seeks: the top route's, when the preview is
    /// of its pop and it declares one (D5).
    pub(super) fn previewed_exit(&self, preview: &touch::Preview) -> Option<&Authored> {
        let top = self.stage.top.as_ref()?;
        preview.progress_for(top.key)?;
        top.exit.as_ref()
    }

    /// An authored or container entrance takes no input until it ends,
    /// as UIKit ignores input to a controller in transition (D6).
    pub(super) fn hold_arriving(&mut self) {
        let arriving = (self.stage.shift.as_ref())
            .filter(|s| s.forward && (s.authored.is_some() || s.container.is_some()))
            .and_then(|s| self.host.kernel().node_by_key(s.incoming))
            .map(|n| n.id);
        if arriving != self.stage.arriving {
            self.stage.arriving = arriving;
            self.host.navigation.arriving = arriving;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_route_picture_keeps_its_scale_axes() {
        exact_motion::link_animations();
        let mut animations = Animations::parse("flip 1s linear both").unwrap();
        let keys = exact_motion::Keyframes::parse("from{scale:1 1}to{scale:-1 0}").unwrap();
        animations.resolve(|_| Some(&keys));
        let route = Authored {
            animations,
            size: (100., 80.),
        };
        let pose = route.pose(500.);
        let bounds = crate::paint::bbox(pose.transform((0., 0.)), (0., 0., 100., 80.));
        assert_eq!(bounds, (50., 20., 0., 40.));
        assert!(crate::paint::space::inverse_affine(pose.transform((0., 0.))).is_none());
        assert_eq!(route.pose(1000.).scale_axes, (-1., 0.));
    }
}
