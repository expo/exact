//! One line `contract vocab` says of an attribute or event beside its prop,
//! where the table says nothing of how it is written or what it does.

pub(super) fn note(name: &str) -> Option<&'static str> {
    Some(match name {
        "heightDragFor" => "names a strict ancestor by `id`; the target needs `box-sizing=\"border-box\"` and a finite nonnegative numeric point `height`. The handle and its ancestors must be displayed, enabled and non-inert. The compiler warns with `lower-height-drag-ineligible` when it can prove a requirement fails; computed values remain runtime checks",
        "src" => "an `image`'s source is its argument: `image url` (HTML's `img src`)",
        "min" | "max" | "step" => "on an `input` a number of either sign is written as its text: `min=-40`, `step=0.5`, `max=1e6` (LLP 1116.010 D3); a decimal pad (`inputmode=\"decimal\"`) with a negative `min` has the ± key",
        "role" => "`status` and `alert` on a `text` make a live region, which a screen reader reads as its text changes (`vocab live`)",
        "markup" => "\"markdown\" or \"none\" (the default): a `text` reads its string as Markdown, a `textarea` edits it (LLP 1045)",
        "resize" => "given an action, the element resize event: ResizeObserver's, after layout, with the content box's width and height and its `DOMRectReadOnly`",
        "focusin" | "focusout" => "bubbles from the focused child without making this node a Tab stop; an action may take `FocusEvent(target: string, relatedTarget: option<string>)` on every host (`focusout` also fires on a move between descendants and on the focused node's removal)",
        "swiperight" | "swipeleft" => "the node follows a sideways drag in that direction (a node may have both), 64 points freely and then a fifth of the finger's travel; its action runs once, when the finger lifts at or past 64 points. The direction is the screen's, in a right-to-left layout too. A finger reaches it only with `touch-action=\"pan-y\"`. A list row's Delete is `delete=` (the concept `swipe`). docs/contract-for-agents.md#swipe-to-reply",
        "swipe" | "swipe-direction" | "swipeDirection" => "`swipe=action swipe-direction=\"down\"`: the platform's discrete swipe in one direction, `left`, `right`, `up` or `down`. Nothing follows the finger; the action runs once when UIKit recognizes the swipe on the node or a view inside it, a field included. UIKit arbitrates it with the other gestures under the finger, as it does any swipe: a field whose text overflows pans instead. Only iOS has it; give the effect a control too. A node that follows the finger is `swiperight` or `swipeleft`; a list row's Delete is `delete=` (the concept `swipe`). LLP 1057.001 §8",
        _ => return None,
    })
}
