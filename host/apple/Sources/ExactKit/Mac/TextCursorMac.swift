// @ref LLP 1115.000 D7.9 — the I-beam over selectable text, from the page's
// one tracking area and the hit test a pointer move already makes.
#if os(macOS)
import AppKit

extension Presenter {
    /// The pointer moved, or what lies under it did, and `leaf` is the node
    /// the page's hit test found there (`hoverAt`). Over text a drag would
    /// select, the cursor is the I-beam, as over an `NSTextView`; `region`
    /// says the hit was a content region's surface, which is such text.
    ///
    /// It is set here and is no cursor rect: a rect on each paragraph is
    /// rebuilt by AppKit for every view whenever a scroll view moves, which
    /// on a long page is thousands of calls a frame (`syncHoverTracking`
    /// says what that cost). This adds no state to a view and nothing to a
    /// scroll: one walk up from the hit for each move the page already
    /// hit-tests.
    ///
    /// An authored `cursor` and a link's pointing hand are cursor rects and
    /// keep the point (`cursorRect`). Leaving the text for a place with
    /// neither brings the arrow back; elsewhere AppKit's rects decide, and
    /// only a cursor this set is ever taken away. As cursor rects are, it is
    /// the key window's. The agent's pointer is not the system's cursor,
    /// which stays the person's: under the agent only the answer is kept.
    func pointerCursor(over leaf: NodeView?, at point: NSPoint, region: Bool = false) {
        let text = region || leaf?.takesTextCursor(at: point) == true
        pointerOverText = text
        guard agentPointer == nil, !ExactEnv.agentMode else { return }
        if text, viewport.window?.isKeyWindow == true {
            if NSCursor.current !== NSCursor.iBeam { NSCursor.iBeam.set() }
            textCursorSet = true
        } else if textCursorSet {
            textCursorSet = false
            if NSCursor.current === NSCursor.iBeam { (leaf?.cursorRect(at: point) ?? .arrow).set() }
        }
    }
}

extension NodeView {
    var boxCursor: NSCursor? {
        if let name = style["cursor"]?.string, name != "auto", let cursor = CSSCursor.value(name) { return cursor }
        return props["accessibilityRole"] == "link" || !(props["href"] ?? "").isEmpty ? .pointingHand : nil
    }

    var hasCursorRects: Bool { boxCursor != nil || inlineText.contains { $0.isLink && !$0.hidden } }

    /// Whether a point over this node, in window points, shows the I-beam:
    /// it is text a drag from here selects (`selectsText`, `textSelectable`:
    /// a control's label and the chrome's are not), and no cursor rect has
    /// the point.
    func takesTextCursor(at windowPoint: NSPoint) -> Bool {
        isParagraph && selectsText && textSelectable && cursorRect(at: windowPoint) == nil
    }

    /// The box's cursor covers its bounds. Else an inline link uses its
    /// pointing hand, and the rest falls through to the nearest ancestor's cursor.
    func cursorRect(at windowPoint: NSPoint) -> NSCursor? {
        if let cursor = boxCursor { return cursor }
        if isParagraph, inlineText.contains(where: { $0.isLink && !$0.hidden }), inlineLink(at: local(windowPoint)) != nil { return .pointingHand }
        var view: NSView = self
        while let parent = view.superview {
            if let node = parent as? NodeView, let cursor = node.boxCursor { return cursor }
            view = parent
        }
        return nil
    }
}
#endif
