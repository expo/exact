//! Immutable plan indexing and each runner's evaluated binding values.
use super::*;

type SiteParent = (Option<NodesId>, Option<ArmsId>);

/// Child sites shared by runners of one immutable plan; evaluated values stay local.
#[derive(Debug)]
pub struct SiteIndex {
    pub(crate) data: std::sync::Arc<SiteData>,
    pub(super) keyframes: bridge::KeyframesTable,
    pub(super) resolve_keyframes: bridge::KeyframesLink,
    /// Values may contain runner-owned references; only indexing is shared.
    pub(super) constants: RefCell<BTreeMap<u32, Value>>,
}

#[derive(Debug)]
pub(crate) struct SiteData {
    groups: Vec<(SiteParent, std::ops::Range<usize>)>,
    sites: Vec<(u32, Site)>,
    pub(super) deps: Deps,
    pub(super) projections: Vec<vm::Prepared>,
}

impl SiteIndex {
    /// Index the exact parent/arm pair, preserving authored order and rank
    /// ties, with every capability linked.
    pub fn new(plan: &Plan) -> Self {
        Self::linked(plan, crate::RunnerLinks::ALL.keyframes)
    }

    /// [`SiteIndex::new`], resolving keyframes as `keyframes` links.
    pub fn linked(plan: &Plan, keyframes: bridge::KeyframesLink) -> Self {
        let mut entries = Vec::with_capacity(plan.nodes.len() + plan.regions.len());
        for (i, n) in plan.nodes.iter().enumerate() {
            entries.push(((n.parent, n.arm), n.order, Site::Node(NodesId(i as u32))));
        }
        for (i, r) in plan.regions.iter().enumerate() {
            entries.push((
                (r.parent, r.arm),
                r.order,
                Site::Region(RegionsId(i as u32)),
            ));
        }
        let key = |i: usize| (entries[i].0, entries[i].1, entries[i].2.rank());
        let order = stable_order(entries.len(), &|i, j| key(i) < key(j));
        let mut groups: Vec<(SiteParent, std::ops::Range<usize>)> = Vec::new();
        let mut sites = Vec::with_capacity(entries.len());
        for (parent, order, site) in order.into_iter().map(|i| entries[i]) {
            let end = sites.len() + 1;
            if let Some((previous, range)) = groups
                .last_mut()
                .filter(|(previous, _)| *previous == parent)
            {
                debug_assert_eq!(*previous, parent);
                range.end = end;
            } else {
                groups.push((parent, sites.len()..end));
            }
            sites.push((order, site));
        }
        groups.shrink_to_fit();
        let mut index = Self {
            data: std::sync::Arc::new(SiteData {
                groups,
                sites,
                deps: Deps::default(),
                projections: plan
                    .bindings
                    .iter()
                    .map(|b| vm::Prepared::new(plan.code(b.expr)))
                    .collect(),
            }),
            keyframes: bridge::keyframes(plan),
            resolve_keyframes: keyframes,
            constants: RefCell::default(),
        };
        let deps = Deps::new(plan, &index);
        std::sync::Arc::get_mut(&mut index.data).unwrap().deps = deps;
        index
    }

    pub(crate) fn sharing(
        plan: &Plan,
        data: std::sync::Arc<SiteData>,
        keyframes: bridge::KeyframesLink,
    ) -> Self {
        Self {
            data,
            keyframes: bridge::keyframes(plan),
            resolve_keyframes: keyframes,
            constants: RefCell::default(),
        }
    }

    /// What every binding, site, derive and resource argument reads.
    pub(crate) fn deps(&self) -> &Deps {
        &self.data.deps
    }

    /// What a region's row body reads outside its own item and position.
    pub fn row_dependencies(&self, region: RegionsId) -> RowDependencies {
        self.data.deps.row_dependencies(region.0 as usize)
    }

    pub(super) fn children(&self, parent: Option<NodesId>, arm: Option<ArmsId>) -> &[(u32, Site)] {
        self.data
            .groups
            .binary_search_by_key(&(parent, arm), |(key, _)| *key)
            .map_or(&[], |i| &self.data.sites[self.data.groups[i].1.clone()])
    }
}
