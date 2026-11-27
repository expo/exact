//! @ref LLP 1123 D17 — what the JS compiler marks a node by for the web's safe-area executor: the classes hold its
//! static rows and enumerated arms, which the executor's inline-style selectors cannot see. A node is marked once, for
//! all its rows (`marks`), the static and the enumerated together.

/// Whether a style text reads an observation, a family's `var(--exact-…)`.
pub(crate) fn reads(css: &str) -> bool {
    [
        "var(--exact-safe-area-inset-",
        "var(--exact-keyboard-inset-",
        "var(--exact-keyboard-undocked-inset-",
    ]
    .iter()
    .any(|family| css.contains(family))
}

/// Whether a style text lets an axis scroll: an `overflow` longhand or shorthand of `auto` or `scroll`.
pub(crate) fn scrolls(css: &str) -> bool {
    css.split(';').any(|declaration| {
        declaration
            .split_once(':')
            .is_some_and(|(property, value)| {
                matches!(property.trim(), "overflow" | "overflow-x" | "overflow-y")
                    && value
                        .split_whitespace()
                        .any(|v| v == "auto" || v == "scroll")
            })
    })
}

/// The `data-exact-safe-area` value of a node that reads (`reader`), scrolls or may scroll (`scroller`), or both; `None`
/// for neither.
pub(crate) fn marks(reads: bool, scrolls: bool) -> Option<String> {
    match (reads, scrolls) {
        (false, false) => None,
        (true, false) => Some("reader".into()),
        (false, true) => Some("scroller".into()),
        (true, true) => Some("reader scroller".into()),
    }
}

/// The `data-exact-safe-area` attribute of a node whose static rows are `css`, where they read or scroll.
pub(crate) fn attribute(css: &str) -> Option<(String, String)> {
    marks(reads(css), scrolls(css)).map(|m| ("data-exact-safe-area".into(), m))
}
