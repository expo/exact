//! A plan and the immutable work shared by its runners.
use crate::instance::{SiteData, SiteIndex};
use crate::vm;
use exact_plan::Plan;
use std::sync::{Arc, OnceLock};

/// An immutable plan shared across launches, with one site index and decoded
/// operand pool. A runner keeps its own plan buffers for theme edits. Stores,
/// evaluated bindings, themes and VM stacks belong to each runner. The shared
/// work is released with its last program or runner.
#[derive(Clone)]
pub struct Program(Arc<Inner>);

struct Inner {
    plan: Plan,
    sites: OnceLock<Arc<SiteData>>,
    constants: OnceLock<Arc<vm::Constants>>,
}

impl Program {
    /// Keep `plan` for any number of independent runners, including on other threads.
    pub fn new(plan: Plan) -> Self {
        Self(Arc::new(Inner {
            plan,
            sites: OnceLock::new(),
            constants: OnceLock::new(),
        }))
    }

    pub(crate) fn into_plan(self) -> Plan {
        match Arc::try_unwrap(self.0) {
            Ok(inner) => inner.plan,
            Err(shared) => shared.plan.clone(),
        }
    }

    pub(crate) fn sites(&self, keyframes: crate::bridge::KeyframesLink) -> SiteIndex {
        let data = self.0.sites.get_or_init(|| SiteIndex::new(self).data);
        SiteIndex::sharing(self, Arc::clone(data), keyframes)
    }

    pub(crate) fn strings(&self) -> vm::Strings {
        let constants = self
            .0
            .constants
            .get_or_init(|| Arc::new(vm::Constants::new(self)));
        vm::Strings::with_constants(self, Arc::clone(constants))
    }
}

impl std::ops::Deref for Program {
    type Target = Plan;

    fn deref(&self) -> &Plan {
        &self.0.plan
    }
}

impl From<Plan> for Program {
    fn from(plan: Plan) -> Self {
        Self::new(plan)
    }
}

impl From<&Plan> for Program {
    fn from(plan: &Plan) -> Self {
        Self::new(plan.clone())
    }
}

impl From<&Program> for Program {
    fn from(program: &Program) -> Self {
        program.clone()
    }
}
