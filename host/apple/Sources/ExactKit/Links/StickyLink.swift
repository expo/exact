// @ref LLP 1047.001 D10 — `position: sticky` (LLP 1083 D4) is a family the
// composition selects by the property a plan uses. The kernel lays a sticky
// box out unscrolled and sends its constraint; the family moves the box as
// its scroller scrolls, through `NodeView.stickyOffset`, which the core's
// `applyTransform` folds in. Without the family the presenter holds
// `NoStickies`, and no box moves. The slot's default is today's host.

/// What the core asks of a presenter's sticky boxes.
package protocol StickyHosting: AnyObject {
    /// A box's constraint, from its `sticky` op.
    func apply(_ id: UInt32, _ payload: [String: Any])
    func forget(_ id: UInt32)
    /// `scroller` scrolled; nil: the page, which moves the boxes stuck to
    /// the roots it scrolls.
    func scrolled(_ scroller: UInt32?)
    /// A box placed on the pixel grid again.
    func regridded(_ id: UInt32)
}

/// No sticky boxes.
package final class NoStickies: StickyHosting {
    package init() {}
    package func apply(_ id: UInt32, _ payload: [String: Any]) {}
    package func forget(_ id: UInt32) {}
    package func scrolled(_ scroller: UInt32?) {}
    package func regridded(_ id: UInt32) {}
}

/// The slots are set before any session is made, and read on the main thread.
package enum StickyLink {
    /// A presenter's sticky boxes.
    package static var make: ((Presenter) -> any StickyHosting)? = { StickyHost($0) }
}
