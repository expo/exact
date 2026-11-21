//! Always-mounted authored children remain siblings of the windowed rows.
use super::*;

#[derive(Debug, Clone)]
pub(super) struct Fixed {
    pub(super) split: usize,
    pub(super) views: [ViewId; 2],
    pub(super) roots: [Vec<ViewId>; 2],
    pub(super) trail_hidden: bool,
    authored_exclusion: BTreeMap<ViewId, [Option<bool>; 2]>,
    published_exclusion: BTreeMap<ViewId, [Option<bool>; 2]>,
}

pub(super) fn create(
    u: &mut Update<'_>,
    node: NodesId,
    axis: ListAxis,
    frames: &[Frame],
    children: &mut Vec<Child>,
) -> Result<(RegionsId, Option<Fixed>), InstanceError> {
    let plan = u.env.plan;
    let sites = u.sites.children(Some(node), plan.node(node).arm);
    let mut each = sites
        .iter()
        .enumerate()
        .filter_map(|(i, (_, site))| match site {
            Site::Region(r) if plan.region(*r).kind == RegionKind::Each => Some((i, *r)),
            _ => None,
        });
    let (split, region) = each
        .next()
        .ok_or_else(|| invalid("a virtualized list needs one direct keyed `for`"))?;
    if each.next().is_some() {
        return Err(invalid("a virtualized list holds one direct `for`; wrap another in an element (all its rows mount)"));
    }
    if sites.len() == 1 {
        return Ok((region, None));
    }
    if axis != ListAxis::Vertical {
        return Err(invalid(
            "fixed content needs a vertical list; move it outside the horizontal list",
        ));
    }
    for b in plan.node(node).bindings.iter().map(|b| plan.binding(b)) {
        if b.kind == BindingKind::Prop
            && b.id == PropId::ListRows as u16
            && u.eval(b.expr, frames)? == Value::Bool(true)
        {
            return Err(invalid(
                "fixed content in a cells list needs appearance=\"none\"",
            ));
        }
    }
    for (i, (_, site)) in sites.iter().enumerate() {
        if i == split {
            continue;
        }
        // All arms are inspected: an inactive virtualized list is refused too.
        refuse_nested(plan, u.sites, *site)?;
        children.push(match site {
            Site::Node(n) => Child::Node(NodeInst::create(u, *n, frames)?),
            Site::Region(r) => Child::Region(RegionInst::create(u, *r, frames)?),
        });
    }
    Ok((
        region,
        Some(Fixed {
            split,
            views: [u.ids.fresh()?, u.ids.fresh()?],
            roots: [roots_of(&children[..split]), roots_of(&children[split..])],
            trail_hidden: true,
            authored_exclusion: authored_exclusion(u, &children[split..]),
            published_exclusion: BTreeMap::new(),
        }),
    ))
}

fn authored_exclusion(u: &Update<'_>, children: &[Child]) -> BTreeMap<ViewId, [Option<bool>; 2]> {
    roots_of(children)
        .into_iter()
        .map(|view| {
            let value = [PropId::AccessibilityElementsHidden, PropId::Inert].map(|prop| {
                super::super::find::locate(children, view, &mut Vec::new(), &mut 0)
                    .and_then(|n| n.bound_prop(u.env.plan, u.sites, prop))
                    .and_then(|v| v.as_bool())
            });
            (view, value)
        })
        .collect()
}

fn refuse_nested(plan: &Plan, sites: &SiteIndex, site: Site) -> Result<(), InstanceError> {
    match site {
        Site::Node(n) => {
            let node = plan.node(n);
            for b in node.bindings.iter().map(|b| plan.binding(b)) {
                if b.kind == BindingKind::Prop && b.id == PropId::Virtualized as u16 {
                    let code = plan.code(b.expr);
                    if code
                        != [
                            exact_plan::Opcode::Bool as u8,
                            0,
                            exact_plan::Opcode::Return as u8,
                        ]
                    {
                        return Err(invalid("a virtualized list cannot live in fixed content; move it into a data row, outside the list, or use virtualized=false"));
                    }
                }
            }
            for (_, s) in sites.children(Some(n), node.arm) {
                refuse_nested(plan, sites, *s)?;
            }
        }
        Site::Region(r) => {
            for arm in plan.region(r).arms.iter() {
                for (_, s) in sites.children(None, Some(arm)) {
                    refuse_nested(plan, sites, *s)?;
                }
            }
        }
    }
    Ok(())
}

impl Collection {
    pub(super) fn fixed_children(
        &mut self,
        u: &mut Update<'_>,
        children: &[Child],
    ) -> Result<(), InstanceError> {
        let Some(fixed) = &mut self.fixed else {
            return Ok(());
        };
        fixed.authored_exclusion = authored_exclusion(u, &children[fixed.split..]);
        let roots = [
            roots_of(&children[..fixed.split]),
            roots_of(&children[fixed.split..]),
        ];
        if roots != fixed.roots {
            fixed.roots = roots;
            advance(&mut self.revision)?;
        }
        self.emit_children(u)
    }

    pub(super) fn fixed_snapshot(&self) -> Vec<CollectionFixed> {
        self.fixed.as_ref().map_or_else(Vec::new, |fixed| {
            (0..2)
                .map(|side| CollectionFixed {
                    view: fixed.views[side],
                    roots: fixed.roots[side].clone(),
                    size: self.index.fixed_size(side),
                    hidden: side == 1 && fixed.trail_hidden,
                })
                .collect()
        })
    }

    pub(super) fn fixed_window(&mut self) -> Result<(), InstanceError> {
        let Some(fixed) = &mut self.fixed else {
            return Ok(());
        };
        fixed.trail_hidden = self.index.len() > 0
            && match &self.geometry {
                Some(g) if g.port_main > 0.0 => {
                    self.index
                        .window(g.offset, g.port_main, [None, None])
                        .map_err(index_error)?
                        .overscan
                        .end
                        < self.index.len()
                }
                _ => true,
            };
        Ok(())
    }

    pub(super) fn fixed_accessibility(&mut self, u: &mut Update<'_>) {
        let Some(fixed) = &mut self.fixed else { return };
        fixed
            .published_exclusion
            .retain(|view, _| fixed.roots[1].contains(view));
        for view in &fixed.roots[1] {
            let values = if fixed.trail_hidden {
                [Some(true); 2]
            } else {
                fixed
                    .authored_exclusion
                    .get(view)
                    .copied()
                    .unwrap_or([None; 2])
            };
            for (index, prop) in [PropId::AccessibilityElementsHidden, PropId::Inert]
                .into_iter()
                .enumerate()
            {
                let value = values[index];
                // An authored update can write either prop while the gate is closed.
                let written = u.ops.iter().any(|op| {
                    matches!(op,
                    Op::SetProp { id, prop: p, .. } | Op::ClearProp { id, prop: p }
                    if id == view && *p == prop)
                });
                if !written && fixed.published_exclusion.get(view).map(|v| v[index]) == Some(value)
                {
                    continue;
                }
                match value {
                    Some(value) => u.ops.push(Op::SetProp {
                        id: *view,
                        prop,
                        value: exact_kernel::PropValue::Bool(value),
                    }),
                    None => u.ops.push(Op::ClearProp { id: *view, prop }),
                }
            }
            fixed.published_exclusion.insert(*view, values);
        }
    }

    pub(in crate::instance) fn logical_fixed<'a>(
        &self,
        children: &'a [Child],
        trail: bool,
    ) -> &'a [Child] {
        self.fixed.as_ref().map_or(&[], |fixed| {
            if trail {
                &children[fixed.split..]
            } else {
                &children[..fixed.split]
            }
        })
    }

    pub(super) fn fixed_side(&self, view: ViewId) -> Option<usize> {
        self.fixed.as_ref()?.views.iter().position(|v| *v == view)
    }
}

impl Collection {
    /// Preflight the entire report before releasing any other collection's pins.
    pub(super) fn prepare_feedback(
        &self,
        feedback: &CollectionFeedback,
        fill: CollectionFill,
    ) -> Result<Option<BTreeMap<ViewId, usize>>, InstanceError> {
        feedback
            .validate()
            .map_err(|_| invalid("invalid collection feedback"))?;
        if feedback.view != self.view
            || feedback.revision != self.revision
            || self
                .geometry
                .as_ref()
                .is_some_and(|g| feedback.scroll_sequence < g.scroll_sequence)
        {
            return Ok(None);
        }
        if [feedback.focus_view, feedback.interaction_view]
            .into_iter()
            .flatten()
            .any(|view| self.pin(Some(view)).is_none())
        {
            return Ok(None);
        }
        // @ref LLP 1072 §5 — a build-only report keeps the port, width and
        // pins: a report that changes them may retire, so it is immediate.
        if fill.create_only
            && self.geometry.as_ref().is_none_or(|g| {
                (
                    g.port_cross,
                    g.port_main,
                    g.cross,
                    g.focus_view,
                    g.interaction_view,
                ) != (
                    feedback.port_cross,
                    feedback.port_main,
                    feedback.cross,
                    feedback.focus_view,
                    feedback.interaction_view,
                )
            })
        {
            return Err(InstanceError::InvalidCollectionFeedback);
        }
        // Reject batches mentioning retired/unpublished wrappers or epochs atomically.
        let mut by_view: BTreeMap<_, _> = self
            .mounted
            .iter()
            .enumerate()
            .map(|(i, r)| (r.wrapper, i))
            .collect();
        if let Some(fixed) = &self.fixed {
            for (side, view) in fixed.views.iter().enumerate() {
                if !feedback
                    .measurements
                    .iter()
                    .any(|m| m.view == *view && m.epoch == 1)
                {
                    return Err(invalid("this host has no fixed-item geometry; move fixed content outside the virtualized list"));
                }
                by_view.insert(*view, self.mounted.len() + side);
            }
        }
        let mut max_value = self
            .index
            .total_height()
            .max(feedback.padding[0])
            .max(feedback.padding[1]);
        for m in &feedback.measurements {
            if by_view.get(&m.view).is_none_or(|i| {
                if *i < self.mounted.len() {
                    self.mounted[*i].epoch != m.epoch
                } else {
                    m.epoch != 1
                }
            }) {
                return Ok(None);
            }
            max_value = max_value.max(m.size + self.sizing.gap);
        }
        let changed_width = self
            .geometry
            .as_ref()
            .is_none_or(|g| g.cross != feedback.cross);
        if changed_width && !self.zero_heights.is_empty() {
            max_value = max_value.max(self.estimated_height);
        }
        if self.index.padded_extent_definitely_fits(max_value * 2.0) {
            return Ok(Some(by_view));
        }
        let mut preview = self.index.preview_heights();
        if changed_width {
            // Width invalidation revives confirmed zero rows at the estimate;
            // measurements from the old width will be discarded.
            for key in &self.zero_heights {
                if let Some(position) = self.index.position(key) {
                    preview.set_height(position, self.estimated_height);
                }
            }
        } else {
            // Linear subtraction/addition can round below the committed binary
            // sum. Preview only changed leaves and their ancestors, without
            // cloning/scanning the N-row index or allocating for equal samples.
            for measurement in &feedback.measurements {
                let Some(row) = self.mounted.get(by_view[&measurement.view]) else {
                    continue;
                };
                let position = row.position;
                let extent = measurement.size + self.sizing.gap;
                preview.set_height(position, self.index.denoised(position, extent));
            }
        }
        let fixed = [0, 1].map(|side| {
            self.fixed
                .as_ref()
                .and_then(|f| {
                    feedback
                        .measurements
                        .iter()
                        .find(|m| m.view == f.views[side])
                })
                .map_or(self.index.fixed_size(side), |m| m.size)
        });
        let extent = preview.total_height() + fixed[0] + fixed[1];
        // Physical content size and relative end correction add padding in
        // different orders; both must fit native geometry before any mutation.
        let [leading, trailing] = feedback.padding;
        let padded_extent = (extent + leading + trailing).max(extent + trailing + leading);
        if !padded_extent.is_finite() || padded_extent > f32::MAX as f64 {
            return Err(InstanceError::InvalidCollectionFeedback);
        }
        Ok(Some(by_view))
    }
}
