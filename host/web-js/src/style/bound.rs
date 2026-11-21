//! What data can give the plan's elements (LLP 1047.003 D1).

use super::{decides_tag, literal};
use exact_kernel::{NodeType, PropId};
use exact_plan::{BindingKind, Plan};
use exact_web::host::template::{attributes_by_data, tags_by_data};
use std::collections::BTreeSet;

/// Page-style tokens for every tag a bound prop can select and every attribute
/// the web host can derive from a bound row. The build keeps their CSS whether
/// the Bun renderer or the Rust renderer writes the page (LLP 1047.003 D1).
pub fn bound_tokens(plan: &Plan) -> BTreeSet<String> {
    let mut tokens = BTreeSet::new();
    for node in plan.nodes.iter() {
        let Some(node_type) = NodeType::from_wire(node.node_type) else {
            continue;
        };
        let mut bound = false;
        for row in node.bindings.iter().map(|b| plan.binding(b)) {
            if !matches!(row.kind, BindingKind::Prop | BindingKind::Style)
                || literal(plan, plan.code(row.expr)).is_some()
            {
                continue;
            }
            bound = true;
            if row.kind != BindingKind::Prop {
                continue;
            }
            if let Some(prop) = PropId::from_wire(row.id).filter(|p| {
                decides_tag(plan, &node, *p)
                    || matches!(
                        p,
                        PropId::AccessibilityRole | PropId::AccessibilityHeadingLevel
                    )
            }) {
                tokens.extend(
                    tags_by_data(node_type, prop)
                        .iter()
                        .map(|t| format!("t:{t}")),
                );
            }
        }
        if bound {
            tokens.extend(
                attributes_by_data(node_type)
                    .into_iter()
                    .map(|a| format!("a:{}", a.to_lowercase())),
            );
        }
    }
    tokens
}
