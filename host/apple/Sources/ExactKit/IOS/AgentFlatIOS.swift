// Layout-only nodes are addressed through the kernel's layout, never a placeholder view.
#if os(iOS) || os(tvOS)
import UIKit

struct FlatBox {
    let id: UInt32
    let host: NodeView
    let rect: CGRect
}

extension Agent {
    func flatBox(_ node: [String: Any]) -> FlatBox? {
        guard let id = node["id"] as? UInt32, let flat = node["layoutOnly"] as? [String: Any],
              let parent = flat["parent"] as? UInt32, let host = presenter.views[parent],
              let x = flat["x"] as? Double, let y = flat["y"] as? Double,
              let w = flat["w"] as? Double, let h = flat["h"] as? Double else { return nil }
        let origin = host.grid?.origin ?? .zero
        let place = PixelGrid.place(CGRect(x: x, y: y, width: w, height: h),
                                    in: origin, scale: presenter.gridScale(under: origin))
        return FlatBox(id: id, host: host, rect: place.drawn)
    }

    func flatBoxForRequest(_ req: [String: Any]) -> FlatBox? {
        guard let id = req["id"] as? Int, presenter.textHost(UInt32(id)) == nil,
              let data = session.agent("{\"op\":\"node\",\"id\":\(id)}").data(using: .utf8),
              let node = try? JSONSerialization.jsonObject(with: data) as? [String: Any] else { return nil }
        return flatBox(node)
    }

    func flatLayout(_ node: [String: Any]) -> [String: Any]? {
        guard let flat = flatBox(node) else { return nil }
        var node = node
        let holder = flat.host.container, frame = flat.rect
        let shift = PresentedScrolls(presenter.viewport).shift(of: holder)
        let b = box(holder, region: frame).offsetBy(dx: shift.x, dy: shift.y)
        func rect(_ r: CGRect) -> [String: Double] {
            ["x": Agent.r2(r.minX), "y": Agent.r2(r.minY), "w": Agent.r2(r.width), "h": Agent.r2(r.height)]
        }
        var space: [String: Any] = ["viewport": rect(b), "local": ["w": Agent.r2(frame.width), "h": Agent.r2(frame.height)],
                                  "capture": ["scale": Agent.r2(holder.window?.screen.scale ?? holder.traitCollection.displayScale)]]
        if let window = holder.window {
            let f = holder.convert(frame, to: window)
            space["window"] = rect(f)
            space["screen"] = rect(window.convert(f, to: window.screen.coordinateSpace))
        }
        node["space"] = space
        node["native"] = ["layoutOnly": true, "parent": "#\(flat.host.id)"]
        let visibility = visibility(of: flat.host, region: flat.host.convert(frame, from: holder), box: b, inside: true)
        node.merge(visibility.facts) { _, new in new }
        node["observed"] = ["clock": session.now(), "wall": Date().timeIntervalSince1970 * 1000]
        return node
    }

    /// Native visibility and clip facts for a view or a viewless region inside it.
    func visibility(of host: NodeView, region: CGRect, box b: CGRect, inside: Bool = false, hidden: Bool = false)
        -> (facts: [String: Any], canvas: NodeView?) {
        visibility(of: host as UIView, region: region, box: b, inside: inside,
                   hidden: hidden || !host.accessibilityExposed, placed: host.placement != nil)
    }

    func visibility(of host: UIView, region: CGRect, box b: CGRect, inside: Bool = false, hidden: Bool = false, placed: Bool = false)
        -> (facts: [String: Any], canvas: NodeView?) {
        let vp = presenter.viewport
        let describe = { (view: UIView) -> String in (view as? NodeView).map { "#\($0.id)" } ?? String(describing: Swift.type(of: view)) }
        var hiddenBy: String? = host.isHidden || (host.alpha == 0 && !placed) ? describe(host) : nil
        var inertBy: String? = host.isUserInteractionEnabled ? nil : describe(host)
        var clipped = b.isEmpty
        var chain: [[String: Any]] = []
        var clippers: [(UIView, UInt32, String)] = []
        var canvas: NodeView? = nil
        var above: UIView? = inside ? host : host.superview
        while let s = above {
            // A canvas child's overlay is transparent on purpose: the child
            // is painted through the canvas's capture (LLP 1014 D5) and only
            // hit here. That is not hidden; it is named.
            if let o = s as? PlainView, let c = o.canvasOverlayOwner {
                canvas = c
                above = s.superview
                continue
            }
            if hiddenBy == nil, s.isHidden || (s.alpha == 0 && (s as? NodeView)?.placement == nil) { hiddenBy = describe(s) }
            if inertBy == nil, !s.isUserInteractionEnabled { inertBy = describe(s) }
            if let sv = s as? ScrollView, let route = sv.superview?.next as? any RouteSurface, route.contentViewport === sv {
                let port = box(sv, region: sv.bounds.inset(by: sv.adjustedContentInset))
                chain.append(["route": Int(route.node.id), "port": [port.minX, port.minY, port.width, port.height].map(Agent.r2)]
                    .merging(Agent.offset(of: sv)) { a, _ in a }.merging(Agent.range(of: sv)) { a, _ in a })
                clippers.append((sv, route.node.id, "viewport"))
            }
            if let n = s as? NodeView {
                if let sv = n.scrollView { chain.append(["id": Int(n.id), "port": port(of: n, sv)].merging(Agent.offset(of: sv, left: n.scrollLeftInset(sv), top: n.scrollTopInset(sv))) { a, _ in a }.merging(Agent.range(of: sv)) { a, _ in a }) } // CSS scrollLeft and scrollTop, as `tree` reads them
                else if n.scrollDormant { chain.append(["id": Int(n.id), "sx": 0.0, "sy": 0.0, "rx": 0.0, "ry": 0.0]) }
                if n.overflowClips || n.clipBox != nil { clippers.append((n, n.id, "overflow")) }
                if n.clipPath != nil { clippers.append((n, n.id, "clip-path")) }
            }
            above = s.superview
        }
        // The viewport's scrolling, or none: a presentation's mount does not scroll (LLP 1123 D2)
        let popoverLayer = PopoverLayer.containing(host)
        let inViewport = popoverLayer.map { presenter.inDocument($0) } ?? (presenter.port(of: host) === vp)
        var scroll: [[String: Any]] = [inViewport ? ["viewport": true].merging(Agent.offset(of: vp)) { a, _ in a }.merging(Agent.range(of: vp)) { a, _ in a }
                                                  : ["presentation": true, "sx": 0.0, "sy": 0.0, "rx": 0.0, "ry": 0.0]]
        scroll.append(contentsOf: chain.reversed())
        var clip: [[String: Any]] = []
        for (n, id, kind) in clippers.reversed() {
            clip.append(["id": Int(id), "kind": kind])
            if let w = host.window {
                let intersects = kind == "overflow" && n is NodeView
                    ? (n as! NodeView).overflowClipIntersects(host.convert(region, to: n))
                    : host.convert(region, to: w).intersects(n.convert(n.bounds, to: w))
                if !intersects { clipped = true }
            }
        }
        var facts: [String: Any] = ["scroll": scroll, "clip": clip]
        // CSS `visibility: hidden` with nothing of it showing, or a hidden run (e28279b3b keeps the view).
        if hiddenBy == nil, hidden { hiddenBy = "visibility" }
        let reach = popoverLayer.map { box($0) } ?? presenter.reach(of: host)
        var visible: [String: Any] = ["hidden": hiddenBy != nil, "inert": inertBy != nil, "inViewport": b.intersects(reach), "clipped": clipped]
        if let hiddenBy { visible["hiddenBy"] = hiddenBy }
        if let inertBy { visible["inertBy"] = inertBy }
        facts["visible"] = visible
        return (facts, canvas)
    }

    func logicalContains(_ ancestor: UInt32, _ descendant: UInt32) -> Bool {
        var at: UInt32? = descendant
        while let id = at {
            if id == ancestor { return true }
            guard let bytes = session.agent("{\"op\":\"node\",\"id\":\(id)}").data(using: .utf8),
                  let node = try? JSONSerialization.jsonObject(with: bytes) as? [String: Any] else { return false }
            at = (node["parent"] as? NSNumber)?.uint32Value
        }
        return false
    }

    /// `point` is in window coordinates.
    func flatObscured(_ flat: FlatBox, at point: CGPoint, hit: UIView) -> String? {
        guard let node = Agent.enclosing(hit).first else { return Agent.covers(hit) }
        if logicalContains(flat.id, node.id) { return nil }
        if logicalContains(node.id, flat.id), hit === node || hit === node.container {
            // The shared ancestor answers for every omitted box in it: the
            // box the point lands in must be this one or one inside it.
            guard let token = LayoutHits.touched(node, at: point) else { return Agent.covers(hit) }
            return logicalContains(flat.id, token.id) ? nil : "layout box #\(token.id) covers its middle"
        }
        return Agent.covers(hit)
    }

    /// A plain addressed tap on a layout-only wrapper cannot activate a control inside it.
    func flatAddressed(_ flat: FlatBox, at point: CGPoint, in window: UIWindow) -> [String: Any]? {
        guard let reach = pressReach(flat.host, at: point, in: window) else { return nil }
        if case .blocked = reach { return nil }
        if let target = reach.node, logicalContains(flat.id, target.id) {
            return ["error": "tap on a layout-only box would press \(reach.described); name that control or use tap at"]
        }
        return nil
    }
}
#endif
