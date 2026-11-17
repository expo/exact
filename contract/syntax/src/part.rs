//! A CSS part declaration as Contract carries it (LLP 1104 D3).

/// The attribute a part's declaration is carried as (LLP 1104 D3): CSS's
/// `::placeholder { color }` is `::placeholder color` beside the element's
/// own attributes, a name no author can write as one.
pub fn part_attr(part: &str, property: &str) -> String {
    format!("{part} {property}")
}

/// [`part_attr`]`("::placeholder", "color")`.
pub const PLACEHOLDER_COLOR: &str = "::placeholder color";

/// [`part_attr`]`("::backdrop", "background-color")` (LLP 1035.001 D11).
pub const BACKDROP_COLOR: &str = "::backdrop background-color";

/// A route's entrance and exit (LLP 1013.001 D1): CSS's
/// `::view-transition-new { animation }` and `::view-transition-old`'s.
pub const VIEW_TRANSITION_NEW: &str = "::view-transition-new animation";
/// [`VIEW_TRANSITION_NEW`]'s exit.
pub const VIEW_TRANSITION_OLD: &str = "::view-transition-old animation";
