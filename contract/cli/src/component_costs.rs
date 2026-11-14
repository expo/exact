//! Expanded body bytes by declaring component, separate from the interned pool.
use super::SourceMap;
use exact_plan::{Code, Plan};
use std::collections::{BTreeMap, BTreeSet};

impl SourceMap {
    /// Own code referenced by each component's instances, largest first.
    /// Shared bodies count at every use, as web emission can repeat them;
    /// child components are counted separately from their parents. Runtime
    /// rows of one `for` are one compiled instance, not repeated plan code.
    pub fn component_code_costs(&self, plan: &Plan) -> Vec<(String, usize, usize)> {
        let mut costs = BTreeMap::<&str, (usize, usize)>::new();
        for instance in &self.sites.instances {
            costs.entry(&instance.component).or_default().0 += 1;
        }
        let mut seen = BTreeSet::new();
        let mut add = |instance: u32, code: Code| {
            seen.insert((code.offset, code.len));
            let name = self.sites.instances[instance as usize].component.as_str();
            costs.entry(name).or_default().1 += code.len as usize;
        };
        for (i, node) in plan.nodes.iter().enumerate() {
            let owner = self.sites.nodes[i].instance;
            for b in node.bindings.iter() {
                add(owner, plan.binding(b).expr);
            }
            for h in node.handlers.iter() {
                for a in plan.handler(h).args.iter() {
                    add(owner, plan.arg(a).expr);
                }
            }
            if let Some(surface) = node.surface {
                for a in plan.surface(surface).args.iter() {
                    add(owner, plan.surface_arg(a).expr);
                }
            }
        }
        for look in &plan.looks {
            let owner = self.sites.nodes[look.node.0 as usize].instance;
            for b in look.bindings.iter() {
                add(owner, plan.binding(b).expr);
            }
        }
        for (i, slot) in plan.slots.iter().enumerate() {
            add(self.sites.slots.get(i).map_or(0, |s| s.instance), slot.init);
        }
        for (i, action) in plan.actions.iter().enumerate() {
            add(
                self.sites.actions.get(i).map_or(0, |s| s.instance),
                action.body,
            );
        }
        for (i, derive) in plan.derives.iter().enumerate() {
            add(
                self.sites.derives.get(i).map_or(0, |d| d.instance),
                derive.body,
            );
        }
        for resource in &plan.resources {
            for a in resource
                .args
                .iter()
                .chain(resource.each.iter())
                .chain(resource.gate.iter())
            {
                add(0, plan.arg(a).expr);
            }
        }
        for (i, region) in plan.regions.iter().enumerate() {
            let owner = self.sites.regions[i];
            add(owner, region.subject);
            add(owner, region.key);
        }
        // Code not referenced by these rows belongs to root-only declarations
        // such as tasks and head. Count each remaining pool byte once.
        let covered: usize = seen.into_iter().map(|(_, len)| len as usize).sum();
        if let Some(root) = self.sites.instances.first() {
            costs.entry(&root.component).or_default().1 += plan.code.len().saturating_sub(covered);
        }
        let mut out: Vec<_> = costs
            .into_iter()
            .map(|(name, (instances, bytes))| (name.to_string(), instances, bytes))
            .collect();
        out.sort_by(|a, b| b.2.cmp(&a.2).then_with(|| a.0.cmp(&b.0)));
        out
    }
}
