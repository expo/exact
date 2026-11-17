// The agent's `reveal` on UIKit (ledger F7, shop F11; LLP 1116.003): before
// a tap or a type, a target out of what a person sees is scrolled into it, as
// a finger would scroll it, and the press then measures where it rests.
#if os(iOS) || os(tvOS)
import UIKit

extension Agent {
    /// A view whose middle is out of view is scrolled to the middle of each
    /// enclosing scroll view it is outside of, innermost first, then the
    /// page's — as the web's `scrollIntoView` does there (block centre,
    /// inline only as far as it takes) — unanimated, as far as each one's
    /// range allows; their delegates tell the app, as a finger's scroll does.
    /// In view is what a person sees (`visiblePort`). A row a grouped list
    /// draws, or a view in its custom row, scrolls its list by the row's cell
    /// first (LLP 1084 D8): with no cell, its views wait in the hidden
    /// authored scroll, which moves nothing on screen (r5 password generator).
    func reveal(_ req: [String: Any]) -> [String: Any] {
        settleForPress()
        guard let requested = req["id"] as? Int else { return miss(req) }
        typealias Target = (node: NodeView, view: UIView, rect: CGRect?)
        func locate() -> Target? {
            if let flat = flatBoxForRequest(req) { return (flat.host, flat.host.container, flat.rect) }
            return view(req).map { ($0, $0, nil) }
        }
        func seen(_ target: Target) -> CGRect? {
            if target.rect == nil, case .view(let drawn, _)? = presenter.groupedLists?.shown(target.node) { return box(drawn) }
            if target.node.window == nil || target.rect == nil && presenter.groupedLists?.draws(target.node.id) == true { return nil }
            return box(target.view, region: target.rect)
        }
        guard let initial = locate() else { return miss(req) }
        let id = initial.rect == nil ? initial.node.id : UInt32(requested), from = seen(initial)
        var scrolled = false
        // A second pass when the first one's scroll moved things on: a list's
        // cells sizing themselves, a large title collapsing.
        for _ in 0..<2 {
            guard let target = locate() else { return miss(req) }
            guard scrollIn(target.node, target: target.view, rect: target.rect,
                           logical: target.rect == nil ? nil : UInt32(requested)) else { break }
            scrolled = true
            settleBox(target.view, region: target.rect)
        }
        if scrolled { presenter.settlePump() }
        guard let final = locate(), final.node.window != nil else { return miss(req) }
        guard scrolled else { return ["revealed": Int(id), "scrolled": false] }
        let to = seen(final) ?? box(final.view, region: final.rect)
        var reply: [String: Any] = ["revealed": Int(id), "scrolled": from.map { to.origin != $0.origin } ?? true,
                                    "to": [Agent.r2(to.midX), Agent.r2(to.midY)]]
        if let from { reply["from"] = [Agent.r2(from.midX), Agent.r2(from.midY)] }
        return reply
    }

    /// One pass of `reveal`: the list first, then each enclosing scroller.
    /// Whether anything scrolled.
    private func scrollIn(_ v: NodeView, target: UIView, rect: CGRect?, logical: UInt32?) -> Bool {
        var scrolled = presenter.groupedLists?.reveal(v.id, port: { self.visiblePort($0) }) ?? false
        guard v.window != nil else { return scrolled }
        // A hidden scroller (the authored one under a grouped list) shows nothing where it scrolls.
        for case let sv as UIScrollView in sequence(first: rect == nil ? target.superview : target, next: { $0?.superview }).compactMap({ $0 }) where sv.isScrollEnabled && !sv.isHidden {
            let frame = target.convert(rect ?? target.bounds, to: sv), mid = CGPoint(x: frame.midX, y: frame.midY), i = sv.adjustedContentInset
            let port = visiblePort(sv, for: target, region: rect, logical: logical)
            // (No band left between the covers: nothing to scroll the middle into, so nothing moves.)
            if port.height <= 0 { continue }
            var o = sv.contentOffset
            if mid.y < port.minY || mid.y >= port.maxY { o.y += mid.y - port.midY }
            // Inline, only as far as it takes from the side its middle is on; a
            // target wider than the port is centred, so its middle comes in.
            if mid.x < port.minX || mid.x >= port.maxX {
                o.x += frame.width > port.width ? mid.x - port.midX : mid.x < port.minX ? frame.minX - port.minX : frame.maxX - port.maxX
            }
            o.y = min(max(o.y, -i.top), max(-i.top, sv.contentSize.height + i.bottom - sv.bounds.height))
            o.x = min(max(o.x, -i.left), max(-i.left, sv.contentSize.width + i.right - sv.bounds.width))
            if let owner = sv.superview as? NodeView, owner.scrollView === sv { o = owner.snappedOffset(o, in: sv, revealing: v) }
            o = PixelGrid.offset(o, in: sv)
            if o != sv.contentOffset { sv.contentOffset = o; scrolled = true }
        }
        return scrolled
    }

    /// The sticky (or fixed) box of `sv`'s content a finger at `at` (in the window) lands in, by the window's own hit
    /// test (as the tap's): none holding `v`, none of a scroller inside `sv`, none in a popover.
    private func stickyCover(at: CGPoint, over v: UIView, in sv: UIScrollView, logical: UInt32?) -> NodeView? {
        guard let hit = v.window?.hitTest(at, with: nil), hit !== v, logical != nil || !hit.isDescendant(of: v) else { return nil }
        var up: UIView? = hit, found: NodeView?
        while let u = up, u !== sv {
            if u is UIScrollView { return nil }
            if let n = u as? NodeView {
                if n.props["popover"] != nil { return nil }
                if found == nil, ["sticky", "fixed"].contains(n.layoutPosition) { found = n }
            }
            up = u.superview
        }
        guard up === sv, let found else { return nil }
        if let logical {
            if logicalContains(logical, found.id) || logicalContains(found.id, logical) { return nil }
        } else if v.isDescendant(of: found) { return nil }
        return found
    }

    /// After a scroll, the target's box at rest, for at most half a second:
    /// over the next turns a list's cells size themselves and a large title
    /// collapses, moving what the scroll brought in (a real touch's two aims
    /// disagreed).
    private func settleBox(_ v: UIView, region: CGRect?) {
        presenter.settlePump()
        let deadline = Date(timeIntervalSinceNow: 0.5)
        var last = box(v, region: region), still = 0
        while still < 2, Date() < deadline {
            RunLoop.main.run(until: Date(timeIntervalSinceNow: 0.016))
            v.window?.layoutIfNeeded()
            presenter.settlePump()
            let now = box(v, region: region)
            if now == last { still += 1 } else { still = 0; last = now }
        }
    }

    /// The part of `sv`'s bounds (its content space) a person sees: inside
    /// its adjusted insets — the bars, the safe area and the keyboard UIKit
    /// insets it by — above the software keyboard's top, and clear of a
    /// navigation bar, toolbar or tab bar over it (iOS 26's bars float over
    /// content: a scroller UIKit does not inset runs under them). The
    /// keyboard's top is the presenter's last, or its layout guide's, so the
    /// viewport's own end (`resizes-content`) or inset (`resizes-visual`)
    /// and `overlays-content`, which insets nothing, all count it
    /// (LLP 1086.000.000 D2).
    /// With a target (`v`, the agent's `reveal`), also clear of each sticky box of `sv`'s content that a finger at its
    /// middle lands on and that doesn't hold `v` (the design tool's edit dock; theme2's 14:49 ruling): a person scrolls
    /// a row out from under it. Never a popover or a top-layer box, which no scroll takes away.
    func visiblePort(_ sv: UIScrollView, for v: UIView? = nil, region: CGRect? = nil, logical: UInt32? = nil) -> CGRect {
        var port = sv.pageBounds.inset(by: sv.adjustedContentInset)
        guard let window = sv.window else { return port }
        var covers = bars(over: sv).map { $0.convert($0.bounds, to: nil) }
        if let top = keyboardTopInWindow() {
            covers.append(CGRect(x: window.bounds.minX, y: top, width: window.bounds.width, height: max(0, window.bounds.maxY - top)))
        }
        port = Self.port(port, clearOf: covers.map { sv.convert($0, from: nil) })
        // The target's covers: the sticky box at its middle, then each one at the middle of the band that leaves (a
        // header across from a dock), by the window's hit test, as the tap's.
        if let v {
            let bounds = region ?? v.bounds, middle = CGPoint(x: bounds.midX, y: bounds.midY)
            let x = v.convert(middle, to: sv).x, mid = port.midY
            var at = v.convert(middle, to: nil)
            for _ in 0..<4 {
                guard port.height > 0, let cover = stickyCover(at: at, over: v, in: sv, logical: logical) else { break }
                let r = cover.convert(cover.bounds, to: sv).intersection(port)
                guard !r.isNull, r.height > 0 else { break }
                if r.midY < mid { port = CGRect(x: port.minX, y: r.maxY, width: port.width, height: max(0, port.maxY - r.maxY)) }
                else { port.size.height = max(0, r.minY - port.minY) }
                at = sv.convert(CGPoint(x: x, y: port.midY), to: nil)
            }
        }
        return port
    }

    /// `port` less what `covers`, in its space, hide of it: one down its whole
    /// height (the iPhone Duo's strip of bar items) leaves the wider side of
    /// it; one in its upper half starts it below, and one lower ends it above.
    static func port(_ port: CGRect, clearOf covers: [CGRect]) -> CGRect {
        var port = port
        for cover in covers {
            let r = cover.intersection(port)
            guard !r.isNull, r.height > 0, r.width > 0 else { continue }
            if r.height >= port.height - 0.5, r.width < port.width {
                if r.midX < port.midX { port = CGRect(x: r.maxX, y: port.minY, width: max(0, port.maxX - r.maxX), height: port.height) }
                else { port.size.width = max(0, r.minX - port.minX) }
            } else if r.midY < port.midY { port = CGRect(x: port.minX, y: r.maxY, width: port.width, height: max(0, port.maxY - r.maxY)) }
            else { port.size.height = max(0, r.minY - port.minY) }
        }
        return port
    }

    /// The software keyboard's top in the window, or nil while it is down.
    func keyboardTopInWindow() -> CGFloat? {
        if let top = presenter.keyboardTop { return top }
        guard let container = presenter.keyboardContainer, let top = presenter.keyboardGuideTop(in: container) else { return nil }
        return container.convert(CGPoint(x: 0, y: top), to: nil).y
    }

    /// The bars UIKit shows over `sv`: the navigation bar and toolbar of each
    /// navigation controller holding it, and a tab bar controller's tab bar
    /// (on an iPad, its floating list of items at the top).
    func bars(over sv: UIScrollView) -> [UIView] {
        var out: [UIView] = []
        for responder in ViewPresentation.controllers(for: sv) {
            if let nav = responder as? UINavigationController {
                if !nav.isNavigationBarHidden { out.append(nav.navigationBar) }
                #if !os(tvOS)
                if !nav.isToolbarHidden { out.append(nav.toolbar) }
                #endif
            }
            if let tabs = responder as? UITabBarController {
                out.append(tabs.tabBar)
                #if os(iOS)
                if let list = tabs.floatingTabList { out.append(list) }
                #endif
            }
        }
        return out.filter { $0.window === sv.window && !$0.isHidden && $0.alpha > 0.01 }
    }

    /// Wait before measuring a press so keyboard motion cannot make its target
    /// read as covered, and settle the pump so newly inserted rows have final boxes.
    func settleForPress() {
        let until = min(KeyboardMotion.shared.until, Date(timeIntervalSinceNow: 1))
        while Date() < until { RunLoop.main.run(until: min(until, Date(timeIntervalSinceNow: 0.01))) }
        presenter.settlePump()
    }
}

/// When the software keyboard's last announced show, hide or resize ends:
/// its notification's time plus its animation's duration.
final class KeyboardMotion: NSObject {
    static let shared = KeyboardMotion()
    private(set) var until = Date.distantPast
    override init() {
        super.init()
        #if !os(tvOS)
        let c = NotificationCenter.default
        for name in [UIResponder.keyboardWillChangeFrameNotification, UIResponder.keyboardWillHideNotification, UIResponder.keyboardWillShowNotification] {
            c.addObserver(self, selector: #selector(moved(_:)), name: name, object: nil)
        }
        #endif
    }
    @objc private func moved(_ n: Notification) {
        #if !os(tvOS)
        let duration = n.userInfo?[UIResponder.keyboardAnimationDurationUserInfoKey] as? Double ?? 0
        until = max(until, Date(timeIntervalSinceNow: max(0, duration)))
        #endif
    }
}
#endif
