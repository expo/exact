//! Each custom press names its owner in the published tree and its target by live view id.
use super::*;

impl<D: DataSource> Presenter<D> {
    fn action_targets<'a>(
        &'a self,
        owner: &NodeRef<'a>,
    ) -> impl Iterator<Item = (ViewId, String)> + 'a {
        let kernel = self.host.kernel();
        let source = owner.id;
        let refs = owner
            .props
            .str(PropId::AccessibilityActions)
            .unwrap_or_default();
        let mut seen = BTreeSet::new();
        refs.split_ascii_whitespace()
            .filter(move |name| seen.insert(*name))
            .filter_map(move |name| {
                let target = kernel
                    .find_by_id(name)
                    .into_iter()
                    .filter_map(|key| kernel.node_by_key(key))
                    .min_by_key(|node| node.id)?;
                let view = target.id;
                if view == source
                    || !self.action_available(view)
                    || !(self
                        .host
                        .runner()
                        .handlers_of(view)
                        .contains(&EventKind::Press)
                        || target.props.link().is_some())
                {
                    return None;
                }
                let name = self.access_name(&target);
                (!name.is_empty()).then_some((view, name))
            })
    }

    fn action_available(&self, view: ViewId) -> bool {
        let kernel = self.host.kernel();
        let (hidden, inert) = self.host.authored_visibility(view);
        if hidden
            || inert
            || !self.alert_allows(view)
            || self.placement_hidden(view)
            || !self.display.allows(kernel, view)
        {
            return false;
        }
        if let Some(surface) = self.surface() {
            if !surface.targets.iter().any(|row| row.node == view) && surface.node != view {
                return false;
            }
        }
        let Some(target) = kernel.node(view) else {
            return false;
        };
        if target.computed_row(exact_kernel::StyleId::Visibility, |s| s.visibility)
            != exact_kernel::Visibility::Visible
        {
            return false;
        }
        let mut at = Some(target);
        let mut disabled = None;
        while let Some(n) = at {
            if n.props.bool(PropId::Disabled) == Some(true)
                || n.style.display == Display::None
                || n.props.bool(PropId::AccessibilityElementsHidden) == Some(true)
            {
                return false;
            }
            disabled = disabled.or_else(|| n.props.bool(PropId::AccessibilityDisabled));
            at = n.parent.and_then(|parent| kernel.node(parent));
        }
        disabled != Some(true)
    }

    pub(super) fn access_actions(&self, node: &NodeRef<'_>) -> Vec<(ViewId, String)> {
        if node.props.str(PropId::AccessibilityActions).is_none() || !self.action_available(node.id)
        {
            return Vec::new();
        }
        self.action_targets(node).collect()
    }

    pub(super) fn action_group(&self, node: &NodeRef<'_>) -> bool {
        node.props
            .str(PropId::AccessibilityActions)
            .is_some_and(|refs| {
                refs.split_ascii_whitespace().next().is_some()
                    && (!self.access_actions(node).is_empty()
                        || self.access.nodes.iter().any(|prior| {
                            prior.role == role::GROUP && prior.action_owner == node.id
                        }))
            })
    }

    /// Press a target offered on this node of the last published accessibility tree.
    pub fn access_custom_action(&mut self, id: i32, view: ViewId, now: f64) -> bool {
        let Some(source) = self.access.nodes.iter().find(|node| node.id == id) else {
            return false;
        };
        if !source.actions.iter().any(|(target, _)| *target == view) {
            return false;
        }
        let kernel = self.host.kernel();
        let Some(owner) = [source.action_owner, source.view].into_iter().find(|&id| {
            kernel.node(id).is_some_and(|node| {
                self.action_available(id) && self.action_targets(&node).any(|(id, _)| id == view)
            })
        }) else {
            return false;
        };
        let href = kernel
            .node(view)
            .and_then(|n| n.props.link().map(str::to_owned));
        let prior_focus = self.focus;
        if let Some(done) = self.surface_tap(view) {
            if done.is_err() {
                return false;
            }
        } else if !self.click_control(view, now, Some(view), false) {
            self.activate_with(view, now, false);
            if let Some(href) = href {
                self.follow(view, &href, now);
            }
        }
        if prior_focus != Some(view)
            && self.focus == Some(view)
            && self.host.kernel().node(owner).is_some()
        {
            self.set_focus(Some(owner), now);
        }
        true
    }
}
