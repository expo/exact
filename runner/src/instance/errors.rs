//! Instance realization failures.

use super::*;

/// The journal line for a value a row refused.
pub(super) fn invalid(view: ViewId, row: &str, value: &Value) -> String {
    use exact_kernel::svg::length::{AttributeLength, ATTRIBUTE_LENGTH_DIAGNOSTIC};
    let mut shown = String::new();
    crate::agent::untyped_json(value, &mut shown);
    let code = exact_kernel::PropId::from_name(row)
        .and_then(AttributeLength::of)
        .map_or(String::new(), |_| {
            format!("{ATTRIBUTE_LENGTH_DIAGNOSTIC}: ")
        });
    format!("{code}view {view}: invalid {row} value {shown}; unset")
}

/// Why an instance could not be realized.
#[allow(missing_docs)]
#[derive(Clone, PartialEq)]
pub enum InstanceError {
    Trap(Trap),
    Bridge(bridge::BridgeError),
    UnknownNodeType(u8),
    List(&'static str),
    SubjectKind {
        region: RegionsId,
    },
    KeyKind {
        region: RegionsId,
    },
    NonFiniteValue {
        name: String,
    },
    SlotType {
        slot: String,
    },
    Collection(String),
    /// Host geometry rejected before changing any collection or kernel state.
    InvalidCollectionFeedback,

    /// A tree evaluation that trapped, with its nearest containing view.
    Evaluation {
        trap: Trap,
        evaluation: String,
        view: Option<(ViewId, NodesId)>,
    },
}

impl InstanceError {
    pub(crate) fn slot_type(
        plan: &exact_plan::Plan,
        value: &Value,
        ty: exact_plan::TypesId,
        name: &str,
    ) -> Self {
        if value.typed(plan, ty) {
            Self::NonFiniteValue { name: name.into() }
        } else {
            Self::SlotType { slot: name.into() }
        }
    }
}

impl From<Trap> for InstanceError {
    fn from(t: Trap) -> Self {
        InstanceError::Trap(t)
    }
}

impl std::fmt::Debug for InstanceError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Evaluation {
                trap,
                evaluation: label,
                view,
            } => {
                write!(f, "Trap({trap:?}) in ")?;
                evaluation(f, label, *view)?;
                f.write_str(trap.bound())
            }
            Self::Trap(value) => f.debug_tuple("Trap").field(value).finish(),
            Self::Bridge(value) => f.debug_tuple("Bridge").field(value).finish(),
            Self::UnknownNodeType(value) => f.debug_tuple("UnknownNodeType").field(value).finish(),
            Self::List(value) => f.debug_tuple("List").field(value).finish(),
            Self::SubjectKind { region } => f
                .debug_struct("SubjectKind")
                .field("region", region)
                .finish(),
            Self::KeyKind { region } => f.debug_struct("KeyKind").field("region", region).finish(),
            Self::NonFiniteValue { name } => f
                .debug_struct("NonFiniteValue")
                .field("name", name)
                .finish(),
            Self::SlotType { slot } => f.debug_struct("SlotType").field("slot", slot).finish(),
            Self::Collection(value) => f.debug_tuple("Collection").field(value).finish(),
            Self::InvalidCollectionFeedback => f.write_str("InvalidCollectionFeedback"),
        }
    }
}

pub(crate) fn evaluation(
    f: &mut std::fmt::Formatter<'_>,
    label: &str,
    view: Option<(ViewId, NodesId)>,
) -> std::fmt::Result {
    if let Some((view, site)) = view {
        write!(f, "view {view} (site {}) ", site.0)?;
    }
    f.write_str(label)
}
impl InstanceError {
    pub(super) fn at_view(mut self, view: ViewId, site: NodesId) -> Self {
        if let Self::Evaluation { view: at, .. } = &mut self {
            if at.is_none() {
                *at = Some((view, site));
            }
        }
        self
    }
}
