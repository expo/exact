//! A scroller's words: where it opens, and whether Exact places it when its
//! page arrives (LLP 1010 §6.5, LLP 1153 D5.1, LLP 1070 §4.2).

/// The note for a scroller's attribute.
pub(super) fn note(name: &str) -> Option<&'static str> {
    Some(match name {
        "scroll-start" | "scrollFollowEnd" => "on a `list virtualized=true` (a grouped list never is): `scroll-start=\"end\"` opens it at its last row, `scrollFollowEnd=true` keeps it at the end as rows arrive and the port resizes, while the reader is there. A chat's transcript: `contract vocab chat-thread`. Any other scroller, a horizontal strip too, opens at the offset its `scrollLeft=` and `scrollTop=` give, and moves when the value changes. Each opens there again when its page arrives in the same visit (`scroll-restoration`)",
        "scroll-restoration" => "scroll-restoration, History's `auto` (the default) or `manual`: whether Exact places the scroller or leaves it to the app. Under `auto`, when a page arrives in the same visit (a `replace` to another path), each scroller in it starts over, as on a browser's new page: at the offset `scrollTop=` and `scrollLeft=` give, at the end of a list with `scroll-start=\"end\"`, else at its start; a list in a virtualized row gets its place back when the row returns. Under `manual` it stays where it was. A new query alone moves nothing, and a fragment still scrolls to its element",
        _ => return None,
    })
}
