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
