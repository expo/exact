//! Which of navigation/routes.js's behaviours a plan can use (LLP 1047.002 D2.1).

use super::answers::{left_out, may_say, property, role_can};
use super::Em;
use exact_kernel::PropId;

/// The constants of navigation/routes.js, each a behaviour of the projection.
pub(crate) const ALL: [&str; 5] = ["TABS", "SPLITS", "SHEETS", "BARS", "CLEAR_BARS"];

/// The constants the plan cannot use, which the compiler writes to
/// `navigation.flag`: all of them without a navigation root (`roots`), since
/// nothing is projected; else each one no node can make what it acts on for.
pub(super) fn unused(em: &Em, roots: bool) -> Vec<&'static str> {
    if !roots {
        return ALL.to_vec();
    }
    let plan = em.plan;
    let any = |f: &dyn Fn(u32) -> bool| (0..plan.nodes.len() as u32).any(f);
    // A route's header (routes.js `headerOf`): a node that may be a `header` whose element
    // is a route's child.
    let bars = any(&|n| {
        may_say(plan, n, PropId::SemanticTag, &["header"])
            && em
                .parent_of(n)
                .is_some_and(|p| property(plan, p, PropId::NavigationKey).is_some())
    });
    let used = [
        any(&|n| role_can(plan, n, &["tablist"])),
        // A split is asked by `navigationSplit` (LLP 1127 D1); a plan with a list keeps the code,
        // though only an ask runs it on the web.
        super::commands::lists(plan)
            || any(&|n| property(plan, n, PropId::NavigationSplit).is_some()),
        any(&|n| {
            may_say(
                plan,
                n,
                PropId::NavigationPresentation,
                &["modal", "fullscreen"],
            )
        }),
        bars,
        bars && any(&|n| may_say(plan, n, PropId::NavigationBar, &["transparent"])),
    ];
    left_out(ALL, used)
}
