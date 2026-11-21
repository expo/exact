//! The native table indexes a section's header and each body row separately.
//! Only structure and the styles that remove a row from flow affect its index.
use super::*;
use crate::instance::region::{init_owned, owns_slots};
use crate::list_api::{SectionRun, SectionTable};
use exact_kernel::StyleId;

fn tag(plan: &Plan, node: NodesId) -> Option<&str> {
    crate::instance::bound::literal_str(plan, node, PropId::SemanticTag)
}

fn flow_binding(kind: BindingKind, id: u16) -> bool {
    kind == BindingKind::Style
        && (id == StyleId::Display as u16 || id == StyleId::PositionType as u16)
}

fn excluded(id: u16, value: &Value) -> bool {
    if id == StyleId::Display as u16 {
        value.as_str() == Some("none")
    } else {
        matches!(value.as_str(), Some("absolute" | "fixed"))
    }
}

fn in_flow(u: &mut Update<'_>, node: NodesId, frames: &[Frame]) -> Result<bool, InstanceError> {
    let plan = u.env.plan;
    for b in plan.node(node).bindings.iter().map(|b| plan.binding(b)) {
        if flow_binding(b.kind, b.id) && excluded(b.id, &u.eval(b.expr, frames)?) {
            return Ok(false);
        }
    }
    Ok(true)
}

/// Whether every item has the same row kinds regardless of data or local state.
fn fixed(plan: &Plan, sites: &SiteIndex, section: NodesId) -> bool {
    let constant = |n| {
        plan.node(n).bindings.iter().all(|id| {
            let b = plan.binding(id);
            !flow_binding(b.kind, b.id) || sites.data.deps.bindings[id.0 as usize].is_constant()
        })
    };
    constant(section) && sites.children(Some(section), plan.node(section).arm).iter().all(|(_, s)| {
        matches!(s, Site::Node(n) if tag(plan, *n) == Some("footer") || constant(*n))
    })
}

/// Evaluate an unrealized arm in the scope it will have when it mounts.
fn enter(
    u: &mut Update<'_>,
    region: RegionsId,
    arm: ArmsId,
    mut frame: Frame,
    frames: &mut Vec<Frame>,
) -> Result<(), InstanceError> {
    frame.region = Some(region.0);
    frame.row = owns_slots(u.env.plan, arm).then(crate::vm::Owned::shared);
    let slots = frame.row.clone();
    frames.push(frame);
    if let Some(slots) = slots {
        init_owned(u, arm, &slots, frames)?;
    }
    Ok(())
}

fn project(
    u: &mut Update<'_>,
    sites: &[(u32, Site)],
    frames: &mut Vec<Frame>,
    headers: &mut Vec<bool>,
) -> Result<(), InstanceError> {
    let plan = u.env.plan;
    for &(_, site) in sites {
        match site {
            Site::Node(n) => {
                let tag = tag(plan, n);
                if tag != Some("footer") && in_flow(u, n, frames)? {
                    headers.push(tag == Some("header"));
                }
            }
            Site::Region(region) => {
                let r = plan.region(region);
                let value = u.eval(r.subject, frames)?;
                let mut visit = |i: usize, frame: Frame| -> Result<(), InstanceError> {
                    if let Some(arm) = r.arms.iter().nth(i) {
                        enter(u, region, arm, frame, frames)?;
                        project(u, u.sites.children(None, Some(arm)), frames, headers)?;
                        frames.pop();
                    }
                    Ok(())
                };
                match (r.kind, value) {
                    (RegionKind::When, Value::Bool(shown)) => {
                        visit(usize::from(!shown), Frame::default())?
                    }
                    (RegionKind::Match, Value::Option(bound)) => {
                        visit(
                            usize::from(bound.is_none()),
                            Frame {
                                bound: bound.map(|v| (*v).clone()),
                                ..Default::default()
                            },
                        )?;
                    }
                    (RegionKind::Each, Value::List(items)) => {
                        for (index, item) in items.iter().enumerate() {
                            visit(
                                0,
                                Frame {
                                    item: Some(item.clone()),
                                    index: Some(index),
                                    ..Default::default()
                                },
                            )?;
                        }
                    }
                    _ => return Err(InstanceError::SubjectKind { region }),
                }
            }
        }
    }
    Ok(())
}

fn mounted_flow(n: &NodeInst, u: &Update<'_>) -> bool {
    [StyleId::Display, StyleId::PositionType].iter().all(|id| {
        n.bound_style(u.env.plan, u.sites, *id)
            .is_none_or(|v| !excluded(*id as u16, &v))
    })
}

fn mounted(u: &Update<'_>, children: &[Child], out: &mut Vec<(ViewId, bool)>) {
    for child in children {
        match child {
            Child::Node(n) => {
                let tag = tag(u.env.plan, n.node);
                if tag != Some("footer") && mounted_flow(n, u) {
                    out.push((n.view, tag == Some("header")));
                }
            }
            Child::Region(r) => match &r.active {
                Active::Arm { roots, .. } => mounted(u, roots, out),
                Active::Rows { rows } => {
                    for row in rows {
                        mounted(u, &row.roots, out);
                    }
                }
            },
        }
    }
}

fn append(runs: &mut Vec<SectionRun>, sections: usize, headers: &[bool]) {
    if sections == 0 {
        return;
    }
    let end = runs.last().map_or(0, |run| run.end) + sections;
    if let Some(last) = runs.last_mut().filter(|run| run.headers == headers) {
        last.end = end;
    } else {
        runs.push(SectionRun {
            end,
            headers: headers.to_vec(),
        });
    }
}

impl Collection {
    /// The structure an item will have when it mounts. A scroll reuses this
    /// index. Local state in a mounted row is overlaid below and expires with it.
    fn section_runs(
        &self,
        u: &mut Update<'_>,
        section: NodesId,
        arm: ArmsId,
        frames: &mut Vec<Frame>,
    ) -> Result<Rc<[SectionRun]>, InstanceError> {
        if let Some(runs) = &*self.section_rows.borrow() {
            return Ok(runs.clone());
        }
        let children = u
            .sites
            .children(Some(section), u.env.plan.node(section).arm);
        let mut headers = Vec::new();
        let mut runs = Vec::new();
        if fixed(u.env.plan, u.sites, section) {
            if in_flow(u, section, &[])? {
                project(u, children, frames, &mut headers)?;
            }
            append(&mut runs, self.logical_len(), &headers);
        } else {
            for (index, item) in self.items.iter().enumerate() {
                headers.clear();
                enter(
                    u,
                    self.region,
                    arm,
                    Frame {
                        item: Some(item.clone()),
                        index: Some(index),
                        ..Default::default()
                    },
                    frames,
                )?;
                if in_flow(u, section, frames)? {
                    project(u, children, frames, &mut headers)?;
                }
                frames.pop();
                append(&mut runs, 1, &headers);
            }
        }
        let runs: Rc<[_]> = runs.into();
        self.section_rows.replace(Some(runs.clone()));
        Ok(runs)
    }
}

impl Tree {
    pub(crate) fn list_sections(
        &self,
        u: &mut Update<'_>,
        view: ViewId,
    ) -> Result<Option<SectionTable>, InstanceError> {
        let mut frames = Vec::new();
        let Some((list, _, _)) = text::list_region(
            None,
            &self.children,
            view,
            &mut frames,
            ListTextPolicy::MacOS,
        ) else {
            return Ok(None);
        };
        let plan = u.env.plan;
        let Some(arm) = plan.region(list.region).arms.iter().next() else {
            return Ok(None);
        };
        let [(_, Site::Node(section))] = u.sites.children(None, Some(arm)) else {
            return Ok(None);
        };
        if tag(plan, *section) != Some("section") || list.axis != ListAxis::Vertical {
            return Ok(None);
        }
        let base = list.section_runs(u, *section, arm, &mut frames)?;
        let mut mounted_views = Vec::new();
        let mut changes = Vec::new();
        for (index, row) in list.mounted_rows() {
            let mut cells = Vec::new();
            if let [Child::Node(n)] = row.roots.as_slice() {
                if mounted_flow(n, u) {
                    mounted(u, &n.children, &mut cells);
                }
            }
            let run = &base[base.partition_point(|r| r.end <= index)];
            if !cells.iter().map(|(_, header)| header).eq(&run.headers) {
                changes.push((
                    index,
                    cells.iter().map(|(_, header)| *header).collect::<Vec<_>>(),
                ));
            }
            mounted_views.push((index, cells.into_iter().map(|(id, _)| id).collect()));
        }
        let runs = if changes.is_empty() {
            base
        } else {
            let mut result = Vec::new();
            let mut start = 0;
            let mut changes = changes.into_iter().peekable();
            for run in base.iter() {
                let end = run.end;
                while let Some((index, headers)) = changes.next_if(|(i, _)| *i < end) {
                    append(&mut result, index - start, &run.headers);
                    append(&mut result, 1, &headers);
                    start = index + 1;
                }
                append(&mut result, end - start, &run.headers);
                start = end;
            }
            result.into()
        };
        Ok(Some(SectionTable {
            runs,
            mounted: mounted_views,
        }))
    }
}
