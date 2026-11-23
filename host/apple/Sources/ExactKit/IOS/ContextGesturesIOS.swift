#if os(iOS) || os(tvOS)
import UIKit

extension NodeView {
    private var needsContextRecognizer: Bool {
        #if os(iOS)
        handlers.contains("contextmenu") || hasInlineContextMenu
        #else
        handlers.contains("contextmenu")
        #endif
    }
    func updateContextGestures() {
        if needsContextRecognizer, contextRecognizer == nil {
            let g = UILongPressGestureRecognizer(target: self, action: #selector(openContext(_:)))
            g.delegate = self
            // Recognition and scroll arbitration remain UIKit's.
            g.delaysTouchesEnded = false
            addGestureRecognizer(g)
            contextRecognizer = g
        }
        if !needsContextRecognizer, let g = contextRecognizer {
            removeGestureRecognizer(g)
            contextRecognizer = nil
        }
        if handlers.contains("dblclick"), doubleRecognizer == nil {
            let g = UITapGestureRecognizer(target: self, action: #selector(doubleClicked(_:)))
            g.delegate = self
            // The web's order (LLP 1057.001 §1 rule 5): the second tap still presses.
            g.numberOfTapsRequired = 2; g.delaysTouchesEnded = false; g.cancelsTouchesInView = false
            addGestureRecognizer(g)
            doubleRecognizer = g
        }
        if !handlers.contains("dblclick"), let g = doubleRecognizer {
            removeGestureRecognizer(g)
            doubleRecognizer = nil
        }
        syncPointerRecognizer()
    }
    @objc func openContext(_ gesture: UILongPressGestureRecognizer) {
        guard gesture.state == .began, !formDisabled else { return }
        let point = gesture.location(in: self)
        #if os(iOS)
        let run = inlineContextTarget(at: point)
        guard run != nil || hasOwnContextMenu else { return }
        deliverContext(at: point, inline: run?.id)
        presenter?.menus.agentContext(self, inline: run?.id)
        #else
        deliverContext(at: point)
        presenter?.menus.agentContext(self)
        #endif
    }

    @objc func doubleClicked(_ gesture: UITapGestureRecognizer) {
        guard gesture.state == .ended, !disabled else { return } // then after this touch's own press
        DispatchQueue.main.async { [weak self, token = incarnation] in if let self, self.incarnation == token, !self.disabled, self.presenter?.views[self.id] === self { self.presenter?.dblclick(self.id) } }
    }

    #if os(iOS)
    var hasOwnContextMenu: Bool { handlers.contains("contextmenu") || props["contextPopover"]?.isEmpty == false }
    package var hasInlineContextMenu: Bool { inlineText.contains { $0.handlers.contains("contextmenu") || $0.props["contextPopover"]?.isEmpty == false } }
    var hasInlinePlainContextMenu: Bool { inlineText.contains { $0.handlers.contains("contextmenu") && $0.props["contextPopover"]?.isEmpty != false } }
    func inlineContextTarget(at point: CGPoint) -> InlineText? {
        inlineMatch(at: point) { run in
            run.handlers.contains("contextmenu") || run.props["contextPopover"]?.isEmpty == false ? run : nil
        }
    }
    /// The first match from the innermost run under `point` through its enclosing runs, stopping at an inert run
    private func inlineMatch<Match>(at point: CGPoint, _ match: (InlineText) -> Match?) -> Match? {
        var run = inlineTarget(at: point)
        while let current = run {
            guard presenter?.inlineInert(current.id) == false else { return nil }
            if let found = match(current) { return found }
            run = presenter?.inlineText(current.parent)
        }
        return nil
    }
    /// Where no authored menu answers, a long press lifts the nearest run under `point`
    /// whose `href` gets the host menu, or the node's own link
    func peekedLink(at point: CGPoint, relativeTo base: URL?) -> (href: String, url: URL, run: InlineText?)? {
        guard !hasOwnContextMenu, inlineContextTarget(at: point) == nil else { return nil }
        func link(_ href: String?, run: InlineText?) -> (href: String, url: URL, run: InlineText?)? {
            guard let href, let url = Self.linkURL(href, relativeTo: base) else { return nil }
            return (href, url.absoluteURL, run)
        }
        return inlineMatch(at: point) { link($0.props["href"], run: $0) } ?? link(props["href"], run: nil)
    }
    /// A row's menu wins a long press over selectable text; an authored
    /// menu on the paragraph itself is the nearest menu. Disabled controls
    /// or inert ancestry admit neither one.
    package var contextMenuSource: NodeView? { contextMenuSource(at: nil) }

    package func contextMenuSource(at point: CGPoint?) -> NodeView? {
        guard let presenter, presenter.menus.eligible(self, contextMenu: true) else { return nil }
        var next: UIView? = self
        while let view = next {
            if let node = view as? NodeView,
               node.hasOwnContextMenu || point.map({ node.inlineContextTarget(at: node.convert($0, from: self)) != nil }) == true { return node }
            next = presenter.listenerParent(of: view)
        }
        return nil
    }
    /// Whether this node's own long press (its `contextmenu` recognizer) takes a press at `point` that lands on
    /// `touched`: it listens there, gives no menu there that its interaction opens, and no node nearer the touch
    /// gives a menu or hears a long press, counted from the node that holds a native view the touch lands on.
    func hearsLongPress(on touched: UIView?, at point: CGPoint) -> Bool {
        guard presenter?.menus.eligible(self, contextMenu: true) == true else { return false }
        let run = inlineContextTarget(at: point)
        guard hasOwnContextMenu || run != nil else { return false }
        if !ExactEnv.agentMode, (run?.props["contextPopover"] ?? (run == nil ? props["contextPopover"] : nil))?.isEmpty == false { return false }
        guard let hit = touched.flatMap(NodeView.innermost), let source = hit.contextMenuSource(at: convert(point, to: hit)) else { return true }
        return source === self
    }
    /// Dynamic failure ordering follows the current row, including a menu
    /// added or removed after this paragraph took its text interaction.
    package func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldBeRequiredToFailBy other: UIGestureRecognizer) -> Bool {
        guard gesture === contextRecognizer, let paragraph = other.view as? NodeView,
              paragraph.contextMenuSource(at: other.location(in: paragraph)) === self else { return false }
        return presenter?.textSelection.interactionHolds(other, on: paragraph) == true
    }

    #endif

    private static let linkMenuRoot = linkMenuBase(visit: "/")

    /// Whether `href` qualifies for a host interaction. The visit changes the URL, while eligibility
    /// depends on the `href` and the app's declared origin.
    static func hasLinkMenu(_ href: String) -> Bool {
        linkURL(href, relativeTo: linkMenuRoot) != nil
    }

    static func linkMenuBase(origin: String? = ExactEnv.appMetadata["ExactOrigin"] as? String, visit: String) -> URL? {
        origin.flatMap { URL(string: $0 + visit) }
    }

    /// The URL the menu copies and shares (LLP 1021 §5.1). HTTP and HTTPS URLs need a host; any other scheme gets nil.
    /// An `href` with no scheme resolves against the current visit on the app's declared origin
    /// and has no URL to take out of the app when it lacks that base.
    static func linkURL(_ href: String, relativeTo base: URL?) -> URL? {
        guard !href.isEmpty, let url = URL(string: href, relativeTo: base),
              let scheme = url.scheme?.lowercased(), scheme == "http" || scheme == "https",
              url.host?.isEmpty == false else { return nil }
        return url
    }

    func deliverContext(at point: CGPoint, inline inlineID: UInt32? = nil) {
        guard let presenter else { return }
        if let inlineID {
            guard presenter.textHost(inlineID) === self, !presenter.inlineInert(inlineID) else { return }
        }
        inlinePressed = nil; linkPressed = nil; pressed = false
        let target = inlineID ?? id
        let run = inlineID.flatMap(presenter.inlineText)
        guard run.map({ $0.handlers.contains("contextmenu") }) ?? handlers.contains("contextmenu") else { return }
        let box = run.flatMap { inlineRects($0).first } ?? contentBox()
        let client = presenter.client(convert(point, to: nil))
        let sample = PointerSample(x: Double(point.x - box.minX), y: Double(point.y - box.minY), buttons: 1, pressure: 0.5, type: "touch", id: 2,
                                   clientX: Double(client.x), clientY: Double(client.y))
        presenter.contextmenu(target, line: sample.line)
    }
}
#endif
