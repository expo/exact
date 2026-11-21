//! `lower-state-dead` (LLP 1121 D8, stage 8 A; theme2's ruling (a)): a
//! node's own `:disabled` part where it writes no `disabled`, or its own
//! `:checked` where it writes none of `checked`, `aria-checked`,
//! `aria-selected` and `aria-pressed`, or its own `:active` where it has no
//! `press` and is no pressable link (`link` or `button` with an `href`),
//! which the host's press needs (theme2's 07:06 question): the state never
//! holds, so its rows never show. A bound expression counts as written. A
//! class's part is not judged: a shared style's `:disabled` on a node that
//! is never disabled is the ordinary case.

use crate::LowerError;
use contract_checking::View;
use contract_syntax::state::state_of;
use std::collections::BTreeSet;

/// What each state reads.
const READS: [(&str, &[&str]); 3] = [
    ("disabled", &["disabled"]),
    (
        "checked",
        &["checked", "aria-checked", "aria-selected", "aria-pressed"],
    ),
    ("active", &["press"]),
];

/// Every node's own state part that its node never meets.
pub(super) fn dead(view: &View<'_>, out: &mut Vec<LowerError>) {
    let mut candidates = Vec::new();
    let mut accounted = BTreeSet::new();
    for node in view.walk_reverse() {
        if let Some((tag, _, attrs)) = node.element() {
            for (state, reads) in READS {
                let Some(part) = attrs
                    .iter()
                    .find(|a| state_of(&a.name).is_some_and(|(s, _)| s == state))
                else {
                    continue;
                };
                // A pressable link (`link`, `button`) with an `href` (bound, or a literal URL)
                // is pressed as CSS's `:active` holds on it; a `text` link is never pressed.
                let link = state == "active"
                    && matches!(tag, "link" | "button")
                    && attrs.iter().any(|a| {
                        a.name == "href"
                            && !matches!(&a.value, contract_syntax::Expr::Str(u, _) if u.is_empty())
                    });
                let live = link || attrs.iter().any(|a| reads.contains(&a.name.as_str()));
                // A component's authored part is reachable if any expanded use can enter its state.
                if live {
                    accounted.insert((part.span, state));
                } else {
                    candidates.push((part.span, state, reads));
                }
            }
        }
    }
    // Keep diagnostic traversal order and report each unreachable source part once.
    for (span, state, reads) in candidates {
        if accounted.insert((span, state)) {
            out.push(LowerError {
                id: "lower-state-dead",
                message: format!(
                    "`:{state}` never holds here: the node writes no {}, so its rows never show (LLP 1121 D8)",
                    reads.iter().map(|r| format!("`{r}`")).collect::<Vec<_>>().join(" or ")
                ),
                span,
            });
        }
    }
}
