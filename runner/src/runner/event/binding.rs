//! Retained action bindings: a painted host's Press, Swiperight and Swipeleft
//! target captured at its publication and dispatched later without the tree.

use super::super::{DataSource, Runner, RunnerError};
use super::Event;
use exact_kernel::{CommitReceipt, NodeKey};
use exact_plan::{ActionsId, EventKind, HandlersId, NodesId, Opcode, TypeKind, Value};
use std::rc::Rc;

pub(super) const BINDING_ARGS: usize = 8;
pub(super) const BINDING_STRING: usize = 1024;
pub(super) const BINDING_STRING_TOTAL: usize = 4096;

/// A bounded retained Press/Swiperight/Swipeleft binding. Contains no frame, tree, plan,
/// resource or History owner. A fresh Runner (including reload) invalidates it.
#[derive(Debug, Clone)]
pub struct ActionBinding {
    origin: Rc<()>,
    key: NodeKey,
    node: NodesId,
    handler: HandlersId,
    event: EventKind,
    action: ActionsId,
    args: Vec<BindingScalar>,
}

impl ActionBinding {
    /// Retained scalar UTF-8 payload bytes, for aggregate host picture budgets.
    /// Walks at most eight scalars without allocation or exposing their values;
    /// excludes scalar metadata, allocator overhead and the transient capture.
    pub fn retained_utf8_bytes(&self) -> usize {
        self.args
            .iter()
            .map(|arg| match arg {
                BindingScalar::String(value) => value.len(),
                _ => 0,
            })
            .sum()
    }
}

/// Refusal before an action, clock or host effect. Generic dispatch is unaffected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActionBindingRefusal {
    /// No matching live node/handler, a different Runner, or changed arguments.
    Stale,
    /// The event, expression or action needs context outside this narrow grammar.
    Unsupported,
    /// A finite capture or code bound was exceeded.
    Limit,
    /// An earlier failed update invalidated this Runner.
    Poisoned,
}

/// Retained qualification can refuse without dispatch; a qualified ordinary
/// action can still fail with the same Runner error as generic dispatch.
#[derive(Debug)]
pub enum ActionBindingError {
    /// No action was dispatched.
    Refused(ActionBindingRefusal),
    /// The qualified action used ordinary Runner dispatch and failed there.
    Dispatch(RunnerError),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BindingScalar {
    Unit,
    Bool(bool),
    Number(u64),
    String(String),
}

impl BindingScalar {
    pub(super) fn capture(
        value: &Value,
        string_bytes: &mut usize,
    ) -> Result<Self, ActionBindingRefusal> {
        Ok(match value {
            Value::Unit => Self::Unit,
            Value::Bool(b) => Self::Bool(*b),
            Value::Number(n) if n.is_finite() => Self::Number(n.to_bits()),
            v @ exact_plan::str_value!() => return Self::string(v.text(), string_bytes),
            _ => return Err(ActionBindingRefusal::Unsupported),
        })
    }

    fn string(s: &str, string_bytes: &mut usize) -> Result<Self, ActionBindingRefusal> {
        // Both limits precede the copy; never retain the parent Record.
        let total = string_bytes
            .checked_add(s.len())
            .ok_or(ActionBindingRefusal::Limit)?;
        if s.len() > BINDING_STRING || total > BINDING_STRING_TOTAL {
            return Err(ActionBindingRefusal::Limit);
        }
        *string_bytes = total;
        Ok(Self::String(s.to_string()))
    }

    fn kind(&self) -> TypeKind {
        match self {
            Self::Unit => TypeKind::Unit,
            Self::Bool(_) => TypeKind::Bool,
            Self::Number(_) => TypeKind::Number,
            Self::String(_) => TypeKind::String,
        }
    }
}

/// The next instruction of a retained binding's code, through the one
/// decoder; running out or malformed code is outside the grammar.
pub(super) fn binding_step(
    code: &mut impl Iterator<Item = Result<crate::vm::Instruction, crate::vm::Trap>>,
) -> Result<crate::vm::Instruction, ActionBindingRefusal> {
    code.next()
        .and_then(Result::ok)
        .ok_or(ActionBindingRefusal::Unsupported)
}

pub(super) fn scalar_kind(kind: TypeKind) -> bool {
    matches!(
        kind,
        TypeKind::Unit | TypeKind::Bool | TypeKind::Number | TypeKind::String
    )
}

impl<D: DataSource> Runner<D> {
    /// Capture a retained Press/Swiperight/Swipeleft target at its painted publication.
    /// Admits only scalar literal/direct field projections and closed root-store
    /// actions. Tree lookup retains its existing mounted-tree traversal cost;
    /// accepted captures own at most eight scalars and 4096 UTF-8 string bytes.
    pub fn capture_action_binding(
        &self,
        key: NodeKey,
        event: EventKind,
    ) -> Result<ActionBinding, ActionBindingRefusal> {
        use ActionBindingRefusal::{Limit, Poisoned, Stale, Unsupported};
        if self.poisoned {
            return Err(Poisoned);
        }
        if !matches!(
            event,
            EventKind::Press | EventKind::Swiperight | EventKind::Swipeleft
        ) {
            return Err(Unsupported);
        }
        let view = self.kernel.node_by_key(key).ok_or(Stale)?.id;
        let (node, frames) = self.find(view).ok_or(Stale)?;
        if frames.len() > 32 {
            return Err(Limit);
        }
        let handler = self
            .plan
            .node(node)
            .handlers
            .iter()
            .find(|h| self.plan.handler(*h).event == event)
            .ok_or(Stale)?;
        let row = self.plan.handler(handler);
        if row.args.len as usize > BINDING_ARGS {
            return Err(Limit);
        }
        let mut string_bytes = 0;
        let mut args = Vec::with_capacity(row.args.len as usize);
        for arg in row.args.iter() {
            args.push(self.binding_projection(
                self.plan.arg(arg).expr,
                &frames,
                &mut string_bytes,
            )?);
        }
        self.binding_action(row.action, &args)?;
        Ok(ActionBinding {
            origin: self.action_binding_origin.clone(),
            key,
            node,
            handler,
            event,
            action: row.action,
            args,
        })
    }

    /// Revalidate BEFORE the host advances time or performs an action. This is
    /// not a host clock/geometry/eligibility certificate. Dispatch checks again,
    /// so intervening host work cannot turn a stale binding into a current one.
    pub fn validate_action_binding(
        &self,
        binding: &ActionBinding,
        event: EventKind,
    ) -> Result<(), ActionBindingRefusal> {
        if !Rc::ptr_eq(&binding.origin, &self.action_binding_origin) || binding.event != event {
            return Err(ActionBindingRefusal::Stale);
        }
        let current = self.capture_action_binding(binding.key, event)?;
        if current.node != binding.node
            || current.handler != binding.handler
            || current.action != binding.action
            || current.args != binding.args
        {
            return Err(ActionBindingRefusal::Stale);
        }
        Ok(())
    }

    /// Deliver a qualified retained event through ordinary dispatch. Refusals
    /// do not journal an action or touch state/effects/time. Hosts still own
    /// presented coordinates, live eligibility, and pre-clock qualification.
    pub fn dispatch_bound(
        &mut self,
        binding: &ActionBinding,
        event: Event,
    ) -> Result<CommitReceipt, ActionBindingError> {
        let kind = match event.bare() {
            Event::Press => EventKind::Press,
            Event::Swiperight => EventKind::Swiperight,
            Event::Swipeleft => EventKind::Swipeleft,
            _ => {
                return Err(ActionBindingError::Refused(
                    ActionBindingRefusal::Unsupported,
                ))
            }
        };
        self.validate_action_binding(binding, kind)
            .map_err(ActionBindingError::Refused)?;
        let view = self
            .kernel
            .node_by_key(binding.key)
            .ok_or(ActionBindingError::Refused(ActionBindingRefusal::Stale))?
            .id;
        self.dispatch(view, event)
            .map_err(ActionBindingError::Dispatch)
    }

    fn binding_action(
        &self,
        id: ActionsId,
        args: &[BindingScalar],
    ) -> Result<(), ActionBindingRefusal> {
        use ActionBindingRefusal::{Limit, Unsupported};
        let action = self.plan.action(id);
        if action.body.len > 1024
            || action.params.len as usize > BINDING_ARGS
            || action.writes.len > 8
        {
            return Err(Limit);
        }
        if action.params.len as usize != args.len() {
            return Err(Unsupported);
        }
        for (param, arg) in action.params.iter().zip(args) {
            if self.plan.type_(self.plan.param(param).ty).kind != arg.kind() {
                return Err(Unsupported);
            }
        }
        let writes: Vec<_> = action
            .writes
            .iter()
            .map(|w| self.plan.write(w).slot)
            .collect();
        for id in &writes {
            let slot = self.plan.slot(*id);
            if slot.owner.is_some() || !scalar_kind(self.plan.type_(slot.ty).kind) {
                return Err(Unsupported);
            }
        }
        let mut r = crate::vm::instructions(self.plan.code(action.body)).peekable();
        let mut stack = Vec::with_capacity(BINDING_ARGS);
        let (mut instructions, mut stores, mut literal_bytes) = (0, 0, 0usize);
        while r.peek().is_some() {
            instructions += 1;
            if instructions > 128 {
                return Err(Limit);
            }
            let step = binding_step(&mut r)?;
            let kind = match step.op {
                Opcode::Number => {
                    if !step.number.is_finite() {
                        return Err(Unsupported);
                    }
                    Some(TypeKind::Number)
                }
                Opcode::Bool => Some(TypeKind::Bool),
                Opcode::Unit => Some(TypeKind::Unit),
                Opcode::Str => {
                    let s = self.plan.str(exact_plan::StrId(step.args[0] as u32));
                    literal_bytes += s.len();
                    if s.len() > BINDING_STRING || literal_bytes > BINDING_STRING_TOTAL {
                        return Err(Limit);
                    }
                    Some(TypeKind::String)
                }
                Opcode::LoadParam => {
                    Some(args.get(step.args[0] as usize).ok_or(Unsupported)?.kind())
                }
                Opcode::StoreSlot => {
                    let id = exact_plan::SlotsId(step.args[0] as u32);
                    stores += 1;
                    if stores > 8 {
                        return Err(Limit);
                    }
                    if !writes.contains(&id) {
                        return Err(Unsupported);
                    }
                    let kind = self.plan.type_(self.plan.slot(id).ty).kind;
                    if stack.pop() != Some(kind) {
                        return Err(Unsupported);
                    }
                    None
                }
                Opcode::Return if r.peek().is_none() && stack.len() <= 1 => return Ok(()),
                _ => return Err(Unsupported),
            };
            if let Some(kind) = kind {
                if stack.len() == BINDING_ARGS {
                    return Err(Limit);
                }
                stack.push(kind);
            }
        }
        Err(Unsupported)
    }
}
