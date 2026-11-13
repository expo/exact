//! Runner failures and conversions from callees.

use super::{DataError, ViewId};
use crate::instance::InstanceError;
use crate::vm::Trap;
use exact_kernel::KernelError;
use exact_plan::{Plan, PlanError, TypesId, Value};

impl From<Trap> for RunnerError {
    fn from(t: Trap) -> Self {
        RunnerError::Trap(t)
    }
}

impl From<InstanceError> for RunnerError {
    fn from(e: InstanceError) -> Self {
        match e {
            InstanceError::SlotType { slot } => RunnerError::SlotType { slot },
            InstanceError::NonFiniteValue { name } => RunnerError::NonFiniteValue { name },
            other => RunnerError::Instance(other),
        }
    }
}

impl From<KernelError> for RunnerError {
    fn from(e: KernelError) -> Self {
        RunnerError::Kernel(e)
    }
}

/// Why the runner refused. The kernel is unchanged.
#[allow(missing_docs)]
pub enum RunnerError {
    KernelSchemaMismatch {
        plan: u64,
        kernel: u64,
    },
    /// The plan belongs to another app (LLP 1023 D5): its header names one identity, this binary's data crate another.
    /// `fix` says which declaration to change, since hosts print this error as it debug-formats.
    AppMismatch {
        plan: String,
        host: String,
        fix: String,
    },
    Plan(PlanError),
    NotOneRoot(usize),
    Trap(Trap),
    Instance(InstanceError),
    Kernel(KernelError),
    Data {
        resource: String,
        error: DataError,
    },
    Shape {
        resource: String,
        /// Where the answer first differs from the declared shape.
        why: String,
    },
    UnknownView(ViewId),
    /// A typed host event carries invalid numeric values. No action ran.
    InvalidEvent {
        event: &'static str,
    },
    /// A control's `input` or `change` carries a value it could never report (LLP 1069.001 D4): a select's value no enabled option has.
    InvalidValue {
        event: &'static str,
        reason: String,
    },
    NoHandler {
        view: ViewId,
        event: &'static str,
    },
    Arity {
        action: String,
        expected: usize,
        actual: usize,
    },
    /// Derives and resources depend on each other in a cycle; nothing settles.
    Cycle,
    /// An earlier update failed after the instance tree had begun to change; the runner no longer matches its kernel and must be restarted (D5).
    Poisoned,
    /// `advance` was given a non-finite time.
    NonFiniteClock,
    /// A viewport dimension is non-finite or non-positive.
    InvalidViewport,
    /// A date fact is non-finite, negative, or its offset past ±18 hours.
    InvalidTime,
    /// A locale or time zone is empty, over-long, or not in its form.
    InvalidPlace,
    /// The declared router shapes, table, launch fallback or value is invalid.
    Router(String),
    /// A clock value exceeds the exact integer-millisecond domain.
    ClockOutOfRange,
    /// Adding a timer interval did not advance its next due time.
    ClockDidNotAdvance {
        timer: usize,
    },
    /// One seek reached the bounded number of timer commits it may perform.
    TimerFireLimit {
        limit: usize,
    },
    /// A send would be the 65th waiting for a `queue` mutation (LLP 1092 D4).
    QueueFull {
        mutation: String,
    },
    /// A gated task's `key=` is no key (a non-finite number; LLP 1092 D8).
    TaskKey {
        task: String,
    },
    /// A resource over a list keyed an element with no key (a non-finite
    /// number; LLP 1017.000 P4c).
    ResourceKey {
        resource: String,
    },
    /// A region sits at the plan root; v1 requires one root node.
    RootRegion,
    /// Curried argument evaluation refused before this handler's action ran.
    HandlerArguments {
        action: String,
        error: Box<RunnerError>,
    },
    /// A slot initializer or write does not conform to the slot's declared type.
    SlotType {
        slot: String,
    },
    /// A derive's value does not conform to its declared type.
    DeriveType {
        derive: String,
    },
    /// An action argument does not conform to the parameter's declared type.
    ArgumentType {
        action: String,
        param: String,
    },
    /// A slot initializer, write, derive or argument contains a non-finite number.
    NonFiniteValue {
        name: String,
    },
    /// An action argument or a slot write is a string longer than [`crate::vm::MAX_STRING`] bytes.
    StringTooLong {
        name: String,
    },

    /// A trap with the evaluation that refused; constructed only on failure.
    Evaluation {
        error: Box<RunnerError>,
        evaluation: String,
    },
}

impl RunnerError {
    pub(super) fn slot_type(plan: &Plan, value: &Value, ty: TypesId, name: &str) -> Self {
        if value.typed(plan, ty) {
            Self::NonFiniteValue { name: name.into() }
        } else {
            Self::SlotType { slot: name.into() }
        }
    }
}

impl std::fmt::Debug for RunnerError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluation { error, evaluation } => write!(f, "{error:?} in {evaluation}"),
            Self::KernelSchemaMismatch { plan, kernel } => f
                .debug_struct("KernelSchemaMismatch")
                .field("plan", plan)
                .field("kernel", kernel)
                .finish(),
            Self::AppMismatch { plan, host, fix } => f
                .debug_struct("AppMismatch")
                .field("plan", plan)
                .field("host", host)
                .field("fix", fix)
                .finish(),
            Self::Plan(value) => f.debug_tuple("Plan").field(value).finish(),
            Self::NotOneRoot(value) => f.debug_tuple("NotOneRoot").field(value).finish(),
            Self::Trap(value) => f.debug_tuple("Trap").field(value).finish(),
            Self::Instance(crate::instance::InstanceError::Evaluation {
                trap,
                evaluation,
                view,
            }) => {
                write!(f, "Instance(Trap({trap:?})) in ")?;
                crate::instance::errors::evaluation(f, evaluation, *view)?;
                f.write_str(trap.bound())
            }
            Self::Instance(value) => f.debug_tuple("Instance").field(value).finish(),
            Self::Kernel(value) => f.debug_tuple("Kernel").field(value).finish(),
            Self::Data { resource, error } => f
                .debug_struct("Data")
                .field("resource", resource)
                .field("error", error)
                .finish(),
            Self::Shape { resource, why } => f
                .debug_struct("Shape")
                .field("resource", resource)
                .field("why", why)
                .finish(),
            Self::UnknownView(value) => f.debug_tuple("UnknownView").field(value).finish(),
            Self::InvalidEvent { event } => f
                .debug_struct("InvalidEvent")
                .field("event", event)
                .finish(),
            Self::InvalidValue { event, reason } => f
                .debug_struct("InvalidValue")
                .field("event", event)
                .field("reason", reason)
                .finish(),
            Self::NoHandler { view, event } => f
                .debug_struct("NoHandler")
                .field("view", view)
                .field("event", event)
                .finish(),
            Self::Arity {
                action,
                expected,
                actual,
            } => f
                .debug_struct("Arity")
                .field("action", action)
                .field("expected", expected)
                .field("actual", actual)
                .finish(),
            Self::Cycle => f.write_str("Cycle"),
            Self::Poisoned => f.write_str("Poisoned"),
            Self::NonFiniteClock => f.write_str("NonFiniteClock"),
            Self::InvalidViewport => f.write_str("InvalidViewport"),
            Self::InvalidTime => f.write_str("InvalidTime"),
            Self::InvalidPlace => f.write_str("InvalidPlace"),
            Self::Router(value) => f.debug_tuple("Router").field(value).finish(),
            Self::ClockOutOfRange => f.write_str("ClockOutOfRange"),
            Self::ClockDidNotAdvance { timer } => f
                .debug_struct("ClockDidNotAdvance")
                .field("timer", timer)
                .finish(),
            Self::TimerFireLimit { limit } => f
                .debug_struct("TimerFireLimit")
                .field("limit", limit)
                .finish(),
            Self::QueueFull { mutation } => f
                .debug_struct("QueueFull")
                .field("mutation", mutation)
                .finish(),
            Self::TaskKey { task } => f.debug_struct("TaskKey").field("task", task).finish(),
            Self::ResourceKey { resource } => f
                .debug_struct("ResourceKey")
                .field("resource", resource)
                .finish(),
            Self::RootRegion => f.write_str("RootRegion"),
            Self::HandlerArguments { action, error } => f
                .debug_struct("HandlerArguments")
                .field("action", action)
                .field("error", error)
                .finish(),
            Self::SlotType { slot } => f.debug_struct("SlotType").field("slot", slot).finish(),
            Self::DeriveType { derive } => f
                .debug_struct("DeriveType")
                .field("derive", derive)
                .finish(),
            Self::ArgumentType { action, param } => f
                .debug_struct("ArgumentType")
                .field("action", action)
                .field("param", param)
                .finish(),
            Self::NonFiniteValue { name } => f
                .debug_struct("NonFiniteValue")
                .field("name", name)
                .finish(),
            Self::StringTooLong { name } => {
                f.debug_struct("StringTooLong").field("name", name).finish()
            }
        }
    }
}

impl RunnerError {
    pub(crate) fn evaluated(self, label: impl FnOnce() -> String) -> Self {
        match self {
            Self::Trap(trap @ Trap::Pending { .. }) => Self::Trap(trap),
            Self::Trap(trap) => {
                let evaluation = format!("{}{}", label(), trap.bound());
                Self::Evaluation {
                    error: Box::new(Self::Trap(trap)),
                    evaluation,
                }
            }
            other => other,
        }
    }
}

impl RunnerError {
    pub(super) fn argument(
        self,
        view: ViewId,
        site: exact_plan::NodesId,
        event: &str,
        k: usize,
    ) -> Self {
        self.evaluated(|| format!("view {view} (site {}) {event} argument {}", site.0, k + 1))
    }
}

impl<D: super::DataSource> super::Runner<D> {
    pub(super) fn eval_argument(
        &self,
        code: exact_plan::Code,
        frames: &[crate::vm::Frame],
        context: (ViewId, exact_plan::NodesId, &str, usize),
    ) -> Result<Value, RunnerError> {
        self.eval(code, &[], frames)
            .map_err(|e| e.argument(context.0, context.1, context.2, context.3))
    }
}
