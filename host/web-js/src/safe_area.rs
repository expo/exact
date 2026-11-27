//! @ref LLP 1123 D17 — what the JS compiler marks a node by for the web's safe-area executor: the classes hold its
//! static rows and enumerated arms, which the executor's inline-style selectors cannot see.

/// What the safe-area executor finds a node by in `css`, its style text (LLP 1123 D17): `reader` where it reads an
/// observation, a family's `var(--exact-…)`, and `scroller` where an axis's overflow is `auto` or `scroll`; `None` for
/// neither.
pub(crate) fn marks(css: &str) -> Option<String> {
    let reads = [
        "var(--exact-safe-area-inset-",
        "var(--exact-keyboard-inset-",
        "var(--exact-keyboard-undocked-inset-",
    ]
    .iter()
    .any(|family| css.contains(family));
    let scrolls = css.split(';').any(|declaration| {
        let Some((property, value)) = declaration.split_once(':') else {
            return false;
        };
        matches!(property.trim(), "overflow" | "overflow-x" | "overflow-y")
            && value
                .split_whitespace()
                .any(|v| v == "auto" || v == "scroll")
    });
    match (reads, scrolls) {
        (false, false) => None,
        (true, false) => Some("reader".into()),
        (false, true) => Some("scroller".into()),
        (true, true) => Some("reader scroller".into()),
    }
}
