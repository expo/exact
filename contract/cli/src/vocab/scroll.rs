//! A scroller's words: where it opens, whether Exact places it when its page
//! arrives (LLP 1010 §6.5, LLP 1153 D5.1, LLP 1070 §4.2), and the grammar of
//! the reach, extension and edge-effect forms (LLP 1123 D13, D14).

/// The note for a scroller's attribute.
pub(super) fn note(name: &str) -> Option<&'static str> {
    Some(match name {
        "scroll-start" | "scrollFollowEnd" => "on a `list virtualized=true` (a grouped list never is): `scroll-start=\"end\"` opens it at its last row, `scrollFollowEnd=true` keeps it at the end as rows arrive and the port resizes, while the reader is there. A chat's transcript: `contract vocab chat-thread`. Any other scroller, a horizontal strip too, opens at the offset its `scrollLeft=` and `scrollTop=` give, and moves when the value changes. Each opens there again when its page arrives in the same visit (`scroll-restoration`)",
        // @ref LLP 1123 D13, D14 — the grammar each form's rows do not show, printed under them
        "-exact-safe-area-reach" => "`auto` (the initial value: a scroller and a box with a plain background colour reach), `none`, or the edges a box reaches on, each once: `top bottom`. `start` and `end` follow the box's `direction`. Reach changes where a box paints and takes touches past a safe edge, never its layout",
        "-exact-safe-area-extend" => "`none` (the initial value), or the edges, each once, then `manual` if wanted: `top bottom manual`. `start` and `end` follow the box's `direction`. Mirrors the box's content into the band (iOS 26 and macOS 26); `manual` places the box's content at the box's frame, where the extension view otherwise places it",
        "-exact-scroll-edge-effect-style" => "a list of edges, each named once across it, each item ending in `automatic`, `soft` or `hard`: `top soft, bottom hard`. An edge the list does not name is automatic, `start` and `end` follow the box's `direction`, and an edge named by `start` or `end` takes that item's style over `left` or `right`",
        "-exact-scroll-edge-effect-hidden" => "`none` (the initial value), or the edges whose scroll edge effect is hidden, each once: `top bottom`; `start` and `end` follow the box's `direction`",
        "scroll-restoration" => "scroll-restoration, History's `auto` (the default) or `manual`: whether Exact places the scroller or leaves it to the app. Under `auto`, when a page arrives in the same visit (a `replace` to another path), each scroller in it starts over, as on a browser's new page: at the offset `scrollTop=` and `scrollLeft=` give, at the end of a list with `scroll-start=\"end\"`, else at its start; a list in a virtualized row gets its place back when the row returns. Under `manual` it stays where it was. A new query alone moves nothing, and a fragment still scrolls to its element",
        _ => return None,
    })
}
