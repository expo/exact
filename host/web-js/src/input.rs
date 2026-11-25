//! Which of the input piece's behaviours (host/web/input-glue.js) a plan can
//! use (LLP 1047.002 D4.2).

use super::answers::{left_out, property, role_can};
use super::Em;
use exact_kernel::PropId;
use exact_plan::EventKind;

/// The constants of input-glue.js, each a behaviour of the piece.
pub(crate) const ALL: [&str; 6] = [
    "PRESS_PAINT",
    "STATE_LOOKS",
    "PANS",
    "ROVING",
    "RADIO_GROUPS",
    "SHORTCUTS",
];

/// The constants the plan cannot use, which the compiler writes to
/// `input.flag`, one a line.
pub(super) fn unused(em: &Em) -> Vec<&'static str> {
    let plan = em.plan;
    let any = |f: &dyn Fn(u32) -> bool| (0..plan.nodes.len() as u32).any(f);
    let used = [
        // Set where emit.rs and rows.rs write a pressed paint, which the piece reads from the node's own style.
        em.press_paint,
        !em.looks.is_empty(),
        plan.handlers.iter().any(|h| h.event == EventKind::Pan),
        // A chooser is an `alertdialog` a menu's row opens (navigation/popover.js).
        any(&|n| role_can(plan, n, &["tablist", "menu", "alertdialog"])),
        any(&|n| role_can(plan, n, &["radiogroup"])),
        any(&|n| property(plan, n, PropId::AccessibilityKeyShortcuts).is_some()),
    ];
    left_out(ALL, used)
}

pub(super) fn text_field(parts: &exact_web::host::template::Parts) -> bool {
    parts.tag == "textarea"
        || parts.tag == "input"
            && matches!(
                parts
                    .props
                    .get("type")
                    .map(String::as_str)
                    .unwrap_or("text"),
                "text" | "password" | "search" | "url" | "tel" | "email"
            )
}

impl Em<'_> {
    pub(super) fn field_value(&mut self, i: u32, expr: exact_plan::Code, e: &str, f: &str) -> bool {
        use std::fmt::Write as _;
        let plan = self.plan;
        let input = plan
            .node(exact_plan::NodesId(i))
            .handlers
            .iter()
            .map(|h| plan.handler(h))
            .find(|h| h.event == EventKind::Input && plan.action(h.action).writes.len != 0);
        let echo = input.is_some_and(|h| echoes_input(plan, expr, h));
        let filters = input.is_some() && !echo;
        let set = if filters {
            format!("{e}.$field=")
        } else if echo {
            format!("{e}.$echo=true;")
        } else {
            String::new()
        };
        let field = self.uses.rt("fieldValue");
        let _ = write!(self.out, "{set}{field}({e},{f});");
        filters
    }
}

// A single assignment of the event value to the slot the field displays
// needs only its ordinary binding, including while composing.
fn echoes_input(
    plan: &exact_plan::Plan,
    expr: exact_plan::Code,
    handler: &exact_plan::HandlersRow,
) -> bool {
    use exact_plan::Opcode::{LoadParam, LoadSlot, Return, StoreSlot, Unit};
    use exact_runner::vm::instructions;
    let code = plan.read_code(plan.code(expr)).0;
    let mut expr = instructions(code).flatten();
    let Some(slot) = expr.next().filter(|i| i.op == LoadSlot) else {
        return false;
    };
    if !expr.next().is_some_and(|i| i.op == Return) {
        return false;
    }
    let mut body = instructions(plan.code(plan.action(handler.action).body)).flatten();
    body.next()
        .is_some_and(|i| i.op == LoadParam && i.args[0] == u64::from(handler.args.len))
        && body
            .next()
            .is_some_and(|i| i.op == StoreSlot && i.args[0] == slot.args[0])
        && body.next().is_some_and(|i| {
            i.op == Return || i.op == Unit && body.next().is_some_and(|i| i.op == Return)
        })
}
