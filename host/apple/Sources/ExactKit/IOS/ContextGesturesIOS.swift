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
    var hasInlineContextMenu: Bool { inlineText.contains { $0.handlers.contains("contextmenu") || $0.props["contextPopover"]?.isEmpty == false } }
    var hasInlinePlainContextMenu: Bool { inlineText.contains { $0.handlers.contains("contextmenu") && $0.props["contextPopover"]?.isEmpty != false } }
    func inlineContextTarget(at point: CGPoint) -> InlineText? {
        inlineRun(at: point) { $0.handlers.contains("contextmenu") || $0.props["contextPopover"]?.isEmpty == false }
    }
    /// The innermost run under `point`, or one enclosing it, that `matches`; nil past an inert one.
    private func inlineRun(at point: CGPoint, where matches: (InlineText) -> Bool) -> InlineText? {
        var run = inlineTarget(at: point)
        while let current = run {
            guard presenter?.inlineInert(current.id) == false else { return nil }
            if matches(current) { return current }
            run = presenter?.inlineText(current.parent)
        }
        return nil
    }
    /// The link a long press here lifts with the host's own menu (LLP 1021
    /// §5.1, "A link's own menu"): the nearest run with an `href` under
    /// `point`, else the node's own, where nothing authored answers it.
    func peekedLink(at point: CGPoint) -> (href: String, run: InlineText?)? {
        guard !hasOwnContextMenu, inlineContextTarget(at: point) == nil else { return nil }
        if let run = inlineRun(at: point, where: { $0.props["href"].map(Self.hasLinkMenu) == true }), let href = run.props["href"] { return (href, run) }
        return props["href"].flatMap { Self.hasLinkMenu($0) ? ($0, nil) : nil }
    }
    /// Whether a link to `href` gets the host's own menu: a web link, `http` or
    /// `https`, or one with no scheme, which resolves in the app. A `mailto:`,
    /// `tel:` or `javascript:` link, or an app's own scheme, gets none.
    static func hasLinkMenu(_ href: String) -> Bool {
        guard !href.isEmpty else { return false }
        guard let scheme = URL(string: href)?.scheme?.lowercased() else { return true }
        return scheme == "http" || scheme == "https"
    }

    #endif

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
