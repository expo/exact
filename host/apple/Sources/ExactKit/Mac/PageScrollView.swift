#if os(macOS)
import AppKit

/// The viewport: a click that reached it, on no node that takes the focus or
/// a press, leaves the first responder where it is, as a click on a window's
/// background does (LLP 1115.000 D7.1), and ends the reader's text selection.
package final class PageScrollView: NSScrollView {
    var visibleSizeChanged: (() -> Void)?
    var personScrolled: (() -> Void)?
    private var visibleSize = NSSize.zero
    private var styleObserver: NSObjectProtocol?

    package override init(frame: NSRect) {
        super.init(frame: frame)
        for name in [NSScrollView.willStartLiveScrollNotification, NSScrollView.didLiveScrollNotification] {
            NotificationCenter.default.addObserver(self, selector: #selector(liveScrolled), name: name, object: self)
        }
        followScrollerStyle()
        styleObserver = NotificationCenter.default.addObserver(forName: NSScroller.preferredScrollerStyleDidChangeNotification,
            object: nil, queue: .main) { [weak self] _ in self?.followScrollerStyle() }
    }
    required init?(coder: NSCoder) { nil }

    private func followScrollerStyle() {
        let style: NSScroller.Style = ExactEnv.agentMode ? .overlay : NSScroller.preferredScrollerStyle
        if scrollerStyle != style { invalidateDocumentFit(); scrollerStyle = style }
        tile()
    }

    private struct DocumentFit {
        let document: ObjectIdentifier
        let documentFrame: NSRect
        let clipSize: NSSize
        let frameSize: NSSize
        let magnification: CGFloat
        let backingScale: CGFloat?
        let style: NSScroller.Style
        let border: NSBorderType
        let vertical: Bool
        let horizontal: Bool
        let autohides: Bool
        /// The axes on which the document fit the clip at the completed fit.
        let fitsWidth: Bool
        let fitsHeight: Bool
    }
    // One viewport tracking area sees overflowing descendants and exits even
    // when no node's bounds contain the pointer. Nodes use the same hit test.
    // It is the key window's, as AppKit's own hover is (LLP 1115.000 D7.9):
    // a window behind another shows no hover, and `lostKey` ends what was
    // hovered when this one stops being key.
    var pointerTracking: ((NSEvent) -> Void)?
    var lostKey: (() -> Void)?
    private var keyObserver: NSObjectProtocol?
    deinit {
        NotificationCenter.default.removeObserver(self)
        if let keyObserver { NotificationCenter.default.removeObserver(keyObserver) }
        if let styleObserver { NotificationCenter.default.removeObserver(styleObserver) }
    }
    var modalAccessibilityChildren: (() -> [NSView]?)?
    /// The modal's elements, served as this view's children. A modal that
    /// is no element itself (an `aria-modal` box with no role) is served by
    /// its elements, as AppKit serves any view's ignored children: its
    /// field's parent is this view (the walk up passes the ignored box,
    /// whose parent is this view), so a client finds the field among this
    /// view's children. Served as itself, it was an `AXUnknown` holding the
    /// field whose parent named the scroll area, and VoiceOver's cursor
    /// stayed on the scroll area while the keyboard was in the field
    /// (realinput-1017k F-2, FN15).
    private var modalProjection: [Any]? { modalAccessibilityChildren?().map { NSAccessibility.unignoredChildren(from: $0) } }
    package override func accessibilityChildren() -> [Any]? {
        modalProjection ?? exposed(super.accessibilityChildren())
    }
    package override func accessibilityChildrenInNavigationOrder() -> [any NSAccessibilityElementProtocol]? { treeNavigationOrder }
    package override func accessibilityContents() -> [Any]? {
        modalProjection ?? exposed(super.accessibilityContents())
    }
    package override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .children || attribute == .contents, let projected = modalProjection { return projected }
        return super.accessibilityAttributeValue(attribute)
    }
    private var pointerArea: NSTrackingArea?
    package override func updateTrackingAreas() {
        super.updateTrackingAreas()
        if pointerArea == nil {
            let area = NSTrackingArea(rect: .zero, options: [.mouseEnteredAndExited, .mouseMoved, .activeInKeyWindow, .inVisibleRect], owner: self, userInfo: nil)
            addTrackingArea(area); pointerArea = area
        }
    }
    package override func mouseEntered(with event: NSEvent) { pointerTracking?(event) }
    package override func mouseMoved(with event: NSEvent) { pointerTracking?(event) }
    package override func mouseExited(with event: NSEvent) { pointerTracking?(event) }

    /// A live scroll's start and each of its moves, which AppKit posts on the
    /// main thread though it runs the scroll on its own.
    @objc private func liveScrolled() { personScrolled?() }
    /// Compatible: the page, and each scroller inside it, moves on AppKit's
    /// scrolling thread. AppKit withdraws responsive scrolling from every
    /// scroll view inside one that is not. The override reads nothing past a
    /// gesture's first event, which is all AppKit then sends here: the live
    /// scroll's notifications say the rest (`liveScrolled`).
    package override class var isCompatibleWithResponsiveScrolling: Bool { true }
    package override func scrollWheel(with event: NSEvent) {
        personScrolled?() // A mouse wheel's notches need not begin a live scroll.
        if event.cgEvent?.getIntegerValueField(.eventSourceUserData) == ChainingScrollView.driverMark { ChainingScrollView.handedDriverWheel(self) }
        super.scrollWheel(with: event)
    }

    private var documentFit: DocumentFit?
    private var prefittingDocument = false
    /// Inside the frame change being made: the document a root fit completed
    /// for (certified once the change ends), and whether the fit was
    /// invalidated since that change began.
    private var fittedInStep: ObjectIdentifier?, invalidatedInStep = false
    var permitsDocumentPrefit: (() -> Bool)?

    func invalidateDocumentFit() {
        documentFit = nil
        if prefittingDocument { fittedInStep = nil; invalidatedInStep = true }
    }

    /// Only a completed root fit may certify axes for the next outer shrink.
    func acceptDocumentFit() {
        guard let document = documentView else { return }
        if prefittingDocument { fittedInStep = ObjectIdentifier(document); return }
        let clip = contentView.bounds.size
        certify(document, fitsWidth: document.frame.width <= clip.width, fitsHeight: document.frame.height <= clip.height)
    }

    private func certify(_ document: NSView, fitsWidth: Bool, fitsHeight: Bool) {
        guard document.frame.origin == .zero, document.bounds.size == document.frame.size else { return }
        documentFit = DocumentFit(document: ObjectIdentifier(document), documentFrame: document.frame,
            clipSize: contentView.bounds.size, frameSize: frame.size, magnification: magnification,
            backingScale: window?.backingScaleFactor, style: scrollerStyle,
            border: borderType, vertical: hasVerticalScroller, horizontal: hasHorizontalScroller,
            autohides: autohidesScrollers, fitsWidth: fitsWidth, fitsHeight: fitsHeight)
    }

    /// A completed fit applies only to the document and viewport geometry it measured.
    var hasCurrentDocumentFit: Bool {
        guard let fit = documentFit, let document = documentView else { return false }
        return ObjectIdentifier(document) == fit.document && document.frame == fit.documentFrame
            && document.bounds.size == document.frame.size && frame.size == fit.frameSize
            && bounds.size == frame.size && contentView.bounds.size == fit.clipSize
            && magnification == fit.magnification && window?.backingScaleFactor == fit.backingScale
            && scrollerStyle == fit.style && borderType == fit.border
            && hasVerticalScroller == fit.vertical && hasHorizontalScroller == fit.horizontal
            && autohidesScrollers == fit.autohides && !automaticallyAdjustsContentInsets
            && contentInsets.top == 0 && contentInsets.left == 0
            && contentInsets.bottom == 0 && contentInsets.right == 0
    }

    package override func setFrameSize(_ newSize: NSSize) {
        guard newSize != frame.size, !prefittingDocument else { return super.setFrameSize(newSize) }
        prefittingDocument = true
        fittedInStep = nil; invalidatedInStep = false
        let fit = prefitDocument(for: newSize)
        super.setFrameSize(newSize)
        // Layout did not follow (`ExactView.fit` waits while main serves the owner, as in
        // an animated resize a module's call makes in a window that is not Exact's): what
        // fit at the last completed fit is the clip's size, as that fit left it, though a
        // scroller the other axis needs has narrowed the clip, and the fit stands for the
        // next step. Twice, since a scroller the old size needed goes and the clip grows.
        var unfollowed: Bool {
            fittedInStep == nil && !invalidatedInStep && documentView.map(ObjectIdentifier.init) == fit?.document
        }
        if let fit, let document = documentView {
            for _ in 0..<2 where unfollowed {
                let clip = contentView.bounds.size
                var size = document.frame.size
                if fit.fitsWidth { size.width = clip.width }
                if fit.fitsHeight { size.height = clip.height }
                if size == document.frame.size { break }
                document.setFrameSize(size)
                tile()
            }
        }
        let carried = unfollowed ? fit : nil
        prefittingDocument = false
        if let fit = carried, let document = documentView {
            certify(document, fitsWidth: fit.fitsWidth, fitsHeight: fit.fitsHeight)
        } else if let fitted = fittedInStep, let document = documentView, ObjectIdentifier(document) == fitted {
            acceptDocumentFit()
        }
    }

    /// An old fitting document must not manufacture gutters during tiling.
    /// Keep overflowing axes and authored root frames untouched; ordinary layout
    /// can still introduce real overflow and its necessary viewport correction.
    /// Returns the fit it applied.
    private func prefitDocument(for newSize: NSSize) -> DocumentFit? {
        let fit = hasCurrentDocumentFit ? documentFit : nil
        documentFit = nil
        guard permitsDocumentPrefit?() == true, let fit, let document = documentView,
              newSize.width.isFinite, newSize.height.isFinite,
              newSize.width > 0, newSize.height > 0 else { return nil }
        // The clip is in CSS px, the frame in points: the change over the page zoom.
        let width = fit.clipSize.width + (newSize.width - fit.frameSize.width) / magnification
        let height = fit.clipSize.height + (newSize.height - fit.frameSize.height) / magnification
        guard width.isFinite, height.isFinite, width > 0, height > 0 else { return nil }
        var size = document.frame.size
        if fit.fitsWidth, newSize.width < fit.frameSize.width {
            size.width = min(size.width, width)
        }
        if fit.fitsHeight, newSize.height < fit.frameSize.height {
            size.height = min(size.height, height)
        }
        if size != document.frame.size { document.setFrameSize(size) }
        return fit
    }

    /// A click that reached the page, on its empty ground: the field being
    /// edited keeps its editor and its caret, as a click on a window's
    /// background leaves them (LLP 1115.000 D7.1). It ends the reader's text
    /// selection, unless a node under it took the focus on the way up (its
    /// `mouseDown`: a `tabindex` box, LLP 1088 D7.3).
    var pressedGround: (() -> Void)?
    package override func mouseDown(with event: NSEvent) {
        let hit = window?.contentView?.hitTest(event.locationInWindow)
        if let focused = window?.firstResponder as? NodeView, let hit, hit.isDescendant(of: focused) {} else { pressedGround?() }
        if TitleBarAreaMac.doubleClicked(event, on: hit) { return }
        super.mouseDown(with: event)
    }
    /// AppKit turns automatic titlebar insets back on when this view
    /// becomes a window's content view, which leaves a black strip the
    /// height of the titlebar above the document (the night, the deck).
    package override func viewDidMoveToWindow() {
        super.viewDidMoveToWindow()
        pinInsets()
        if let keyObserver { NotificationCenter.default.removeObserver(keyObserver) }
        keyObserver = window.map { window in
            NotificationCenter.default.addObserver(forName: NSWindow.didResignKeyNotification, object: window, queue: .main) { [weak self] _ in self?.lostKey?() }
        }
    }
    package override func tile() {
        super.tile()
        pinInsets()
        insetScrollersFromChrome()
        syncElasticity()
        let size = contentView.bounds.size
        if visibleSize != size {
            visibleSize = size
            visibleSizeChanged?()
        }
    }
    /// A page that fits does not rubber-band.
    ///
    /// AppKit's `.automatic` elasticity bounces an axis the page cannot
    /// actually scroll, so an inner pane that reaches its end and chains the
    /// rest of the gesture up here (`ChainingScrollView`) drags the whole
    /// window's content — the app's own chrome with it — even though the
    /// document is exactly the viewport. The browser's rule is the one to
    /// match: an axis is elastic only while it has somewhere to go. Chaining
    /// itself is unchanged; a page that really does scroll still bounces.
    func syncElasticity() {
        let document = documentView?.frame.size ?? .zero
        let visible = contentView.bounds.size
        // Half a point of slack: a fractional layout must not read as scrollable.
        let x: NSScrollView.Elasticity = document.width - visible.width > 0.5 ? .automatic : .none
        let y: NSScrollView.Elasticity = document.height - visible.height > 0.5 ? .automatic : .none
        if horizontalScrollElasticity != x { horizontalScrollElasticity = x }
        if verticalScrollElasticity != y { verticalScrollElasticity = y }
    }
    func pinInsets() {
        if automaticallyAdjustsContentInsets { automaticallyAdjustsContentInsets = false }
        if contentInsets.top != 0 || contentInsets.left != 0 || contentInsets.bottom != 0 || contentInsets.right != 0 {
            contentInsets = NSEdgeInsetsZero
        }
    }
}

extension NSScrollView {
    /// The scrollers keep clear of the window's title bar and toolbar, as
    /// in a window with a full-size content view whose scroll view AppKit
    /// insets (`automaticallyAdjustsContentInsets`), while the content
    /// still scrolls beneath them: `viewport-fit=cover`'s page, or a
    /// `scroll` node in it. Only the scrollers move (`scrollerInsets`); the
    /// content insets stay zero, so the clip, the document and the kernel's
    /// viewport are as CSS lays them out. The inset on each side is how far
    /// this view reaches into the window outside `contentLayoutRect`.
    func insetScrollersFromChrome() {
        var insets = NSEdgeInsetsZero
        if let window, window.styleMask.contains(.fullSizeContentView), !isHiddenOrHasHiddenAncestor {
            let safe = convert(window.contentLayoutRect, from: nil), box = bounds
            let above = max(0, box.maxY - max(safe.maxY, box.minY)), below = max(0, min(safe.minY, box.maxY) - box.minY)
            insets.top = isFlipped ? below : above
            insets.bottom = isFlipped ? above : below
            insets.left = max(0, min(safe.minX, box.maxX) - box.minX)
            insets.right = max(0, box.maxX - max(safe.maxX, box.minX))
        }
        // A `scroll` node's padding edge masks its scroll view: the scrollers keep inside that too.
        if let clip = (self as? ChainingScrollView)?.clipInsets {
            insets = NSEdgeInsets(top: max(insets.top, clip.top), left: max(insets.left, clip.left),
                                  bottom: max(insets.bottom, clip.bottom), right: max(insets.right, clip.right))
        }
        let now = scrollerInsets
        // Setting the insets tiles again, which asks here again: equal, it stops.
        if now.top != insets.top || now.left != insets.left || now.bottom != insets.bottom || now.right != insets.right {
            scrollerInsets = insets
        }
    }
}

#endif
