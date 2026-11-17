//! Source expressions behind executable rows for translation validation.
use super::*;

/// One checked instance expression before lowering replaces its reads.
pub struct SharedExpression {
    /// The generated derive-row name.
    pub name: String,
    /// The Lean embedding of the checked substituted expression.
    pub expr: String,
    /// The source arm that owns the expression. `None` is the root instance.
    pub owner: Option<(u32, u8)>,
}

/// One plan region and the source node recorded when lowering emitted it.
pub struct SharedRegion {
    /// The expanded source node's region tag.
    pub tag: u32,
    kind: exact_plan::RegionKind,
    parent: Option<u32>,
    arm: Option<u32>,
    order: u32,
    subject: Vec<u8>,
    key: Vec<u8>,
    arms: (u32, u32),
}

impl SharedRegion {
    /// Whether `plan` still has this emitted region at `index`.
    pub fn matches(&self, plan: &exact_plan::Plan, index: usize) -> bool {
        let Some(region) = plan.regions.get(index) else {
            return false;
        };
        region.kind == self.kind
            && region.parent.map(|id| id.0) == self.parent
            && region.arm.map(|id| id.0) == self.arm
            && region.order == self.order
            && plan.code(region.subject) == self.subject
            && plan.code(region.key) == self.key
            && (region.arms.start, region.arms.len) == self.arms
    }
}

/// Source identities needed to validate the executable rows in a plan.
pub struct SharingContext {
    /// Checked expressions joined to their source owners.
    pub expressions: Vec<SharedExpression>,
    /// Plan regions paired with the source node recorded at emission.
    pub regions: Vec<SharedRegion>,
}

/// Embed the checked instance expressions and their source owner paths.
pub fn sharing_context(src: &str, path: Option<&Path>) -> Result<SharingContext, CompileError> {
    let mut file = match path {
        Some(path) => load(path, src)?,
        None => contract_syntax::parse(src)?,
    };
    contract_syntax::resolve_clock_timelines(&mut file)?;
    crate::params(&mut file)?;
    crate::vars(&mut file)?;
    let strings = path.map(strings_beside).transpose()?.flatten();
    let checked = contract_types::check_all(&file, true, contract_lower::tags::style, strings)
        .map_err(|mut all| CompileError::from(all.swap_remove(0)))?;
    let e = Emitter {
        out: String::new(),
        root: checked.expanded.root.clone(),
        checked: &checked,
        format_used: Default::default(),
        records: Default::default(),
    };
    let expressions = checked
        .expanded
        .thunks
        .iter()
        .map(|t| {
            let owner = match t.owner {
                contract_syntax::Owner::Arm { tag, arm } => Some((tag, arm)),
                contract_syntax::Owner::Root | contract_syntax::Owner::Instance => None,
            };
            Ok(SharedExpression {
                name: t.name.clone(),
                expr: e.expr(&t.expr)?,
                owner,
            })
        })
        .collect::<Result<Vec<_>, CompileError>>()?;
    let analysis = contract_analyze::check_all(&checked)
        .map_err(|mut all| CompileError::from(all.swap_remove(0)))?;
    let asset_root = path.map(app_root).transpose()?;
    let (plan, sites) = contract_lower::lower_all(
        &checked,
        &analysis,
        asset_root.as_deref(),
        true,
        contract_lower::Profile::Web,
        None,
    )
    .map_err(|mut all| CompileError::from(all.swap_remove(0)))?;
    let sites = sites.ok_or_else(|| {
        refuse(
            "mapped lowering returned no source sites",
            checked.expanded.root.span,
        )
    })?;
    let regions = sites
        .region_tags
        .into_iter()
        .zip(&plan.regions)
        .map(|(tag, region)| SharedRegion {
            tag,
            kind: region.kind,
            parent: region.parent.map(|id| id.0),
            arm: region.arm.map(|id| id.0),
            order: region.order,
            subject: plan.code(region.subject).to_vec(),
            key: plan.code(region.key).to_vec(),
            arms: (region.arms.start, region.arms.len),
        })
        .collect();
    Ok(SharingContext {
        expressions,
        regions,
    })
}
