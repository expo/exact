#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// Observes a two-finger direct pinch alongside a scroll view's pan without
/// claiming or delaying single-finger scrolling (LLP 1057.001 §4.1).
///
/// Tracks every eligible direct touch for its full lifetime instead of ending
/// on UIKit's built-in `UIPinchGestureRecognizer` reset, so a surviving finger
/// or resting third finger can form a subsequent pair without lifting first.
final class ReportingPinch: UIGestureRecognizer, UIGestureRecognizerDelegate {
    private static let threshold: CGFloat = 4

    private weak var node: NodeView?
    private var active: [UITouch] = []
    private var pair: (UITouch, UITouch)?
    private var recognized = false
    private var startSpan: CGFloat = 0
    private var lastScale: CGFloat = 1
    private var focus: CGPoint?
    private var sharedScrolls: [UIScrollView] = []
    private var lift = PinchLift()
    private weak var survivor: UITouch?
    private var survivorOrigin: CGPoint = .zero
    private var survivorLift: PinchLift?

    var idle: Bool { active.isEmpty && pair == nil && !recognized && state == .possible }

    init(on node: NodeView) {
        self.node = node
        super.init(target: nil, action: nil)
        delegate = self
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }
    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

    static func allowsHit(_ hit: UIView, on node: NodeView, in presenter: Presenter) -> Bool {
        if !node.isTextControl && node.kind != "control",
           presenter.logicalAncestry(of: hit).prefix(while: { $0 !== node }).contains(where: {
               $0 is UIControl || $0 is UITextView
           }) { return false }
        return !presenter.authoredNodes(from: hit).prefix(while: { $0 !== node }).contains {
            $0.isTextControl || $0.kind == "control"
        }
    }

    func tracksPair(_ a: UITouch, _ b: UITouch) -> Bool {
        guard let p = pair else { return false }
        return (p.0 === a && p.1 === b) || (p.0 === b && p.1 === a)
    }

    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        guard touch.type == .direct, let hit = touch.view, let node, let presenter = node.presenter else { return false }
        return Self.allowsHit(hit, on: node, in: presenter)
    }

    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let p = pair, let node, let presenter = node.presenter else { return false }
        return canBeginPair(p, on: node, in: presenter)
    }

    private func canBeginPair(_ p: (UITouch, UITouch), on node: NodeView, in presenter: Presenter) -> Bool {
        guard SwipeInput.allows(node) else { return false }
        guard let hit = p.0.view else { return true }
        let other = p.1.view
        return !presenter.authoredNodes(from: hit).prefix(while: { $0 !== node }).contains { child in
            guard let childPinch = child.extras?.reportingPinch, SwipeInput.allows(child),
                  other === child || other?.isDescendant(of: child) == true else { return false }
            return childPinch.tracksPair(p.0, p.1)
        }
    }

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        pruneEnded()
        for t in touches where t.type == .direct {
            if !active.contains(t) { active.append(t) }
        }
        if pair == nil && active.count >= 2 { armPair() }
        super.touchesBegan(touches, with: event)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        if let s = survivor, touches.contains(s) {
            let p = s.location(in: nil)
            if hypot(p.x - survivorOrigin.x, p.y - survivorOrigin.y) > PinchLift.jitter {
                survivorLift?.noteMoved()
                survivor = nil
                survivorLift = nil
            }
        }
        if pair == nil && active.count >= 2 { armPair() }
        guard let p = pair, let node, let presenter = node.presenter else {
            super.touchesMoved(touches, with: event)
            return
        }
        if recognized && !SwipeInput.allows(node) {
            cancelActivePair(on: node, in: presenter)
            super.touchesMoved(touches, with: event)
            return
        }
        if startSpan == 0 { measurePair(p) }
        guard startSpan > 0 else {
            super.touchesMoved(touches, with: event)
            return
        }
        let a = p.0.location(in: nil), b = p.1.location(in: nil)
        let distance = hypot(a.x - b.x, a.y - b.y)
        if !recognized {
            guard abs(distance - startSpan) >= Self.threshold, canBeginPair(p, on: node, in: presenter) else {
                super.touchesMoved(touches, with: event)
                return
            }
            recognized = true
            lift = PinchLift()
            for scroll in sharedScrolls { PinchLift.attach(lift, to: scroll) }
        }
        let factor = distance / startSpan
        if factor.isFinite && factor > 0 { lastScale = factor }
        let point = presenter.client(CGPoint(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2))
        focus = point
        presenter.pinch(node, ended: false, scale: Double(lastScale), x: Double(point.x), y: Double(point.y))
        super.touchesMoved(touches, with: event)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        active.removeAll(where: touches.contains)
        if let s = survivor, touches.contains(s) {
            survivor = nil
            survivorLift = nil
        }
        if let p = pair, touches.contains(p.0) || touches.contains(p.1) {
            if recognized, let node, let presenter = node.presenter {
                let point = focus
                let factor = lastScale
                let stayed: UITouch? = !touches.contains(p.0) ? p.0 : (!touches.contains(p.1) ? p.1 : nil)
                let endedLift = lift
                endedLift.endPair(scrolls: sharedScrolls)
                clearPair()
                if let stayed {
                    survivor = stayed
                    survivorOrigin = stayed.location(in: nil)
                    survivorLift = endedLift
                }
                if let point {
                    presenter.pinch(node, ended: true, scale: Double(factor), x: Double(point.x), y: Double(point.y))
                }
            } else {
                clearPair()
            }
        }
        if active.isEmpty {
            clearPair()
        } else if pair == nil && active.count >= 2 {
            armPair()
        }
        super.touchesEnded(touches, with: event)
    }

    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) {
        active.removeAll(where: touches.contains)
        if let s = survivor, touches.contains(s) {
            survivor = nil
            survivorLift = nil
        }
        if let p = pair, touches.contains(p.0) || touches.contains(p.1) {
            if recognized, let node, let presenter = node.presenter {
                cancelActivePair(on: node, in: presenter)
            } else {
                clearPair()
            }
        }
        if active.isEmpty {
            clearPair()
        } else if pair == nil && active.count >= 2 {
            armPair()
        }
        super.touchesCancelled(touches, with: event)
    }

    private func pruneEnded() {
        let stale = Set(active.filter { $0.phase == .ended || $0.phase == .cancelled })
        guard !stale.isEmpty else { return }
        active.removeAll(where: stale.contains)
        if let s = survivor, stale.contains(s) { survivor = nil; survivorLift = nil }
        if let p = pair, stale.contains(p.0) || stale.contains(p.1) { clearPair() }
    }

    override func reset() {
        super.reset()
        active.removeAll()
        survivor = nil
        survivorLift = nil
        clearPair()
    }

    func resetForRecycle() {
        for scroll in sharedScrolls { PinchLift.clear(for: scroll) }
        active.removeAll()
        survivor = nil
        survivorLift = nil
        clearPair()
    }

    private func armPair() {
        guard active.count >= 2 else { return }
        survivor = nil
        survivorLift = nil
        let p = (active[0], active[1])
        pair = p
        recognized = false
        lastScale = 1
        focus = nil
        sharedScrolls = Self.commonScrolls(of: p.0.view, and: p.1.view)
        measurePair(p)
    }

    private func measurePair(_ p: (UITouch, UITouch)) {
        let a = p.0.location(in: nil), b = p.1.location(in: nil)
        let span = hypot(a.x - b.x, a.y - b.y)
        if span > 0 { startSpan = span }
    }

    private func clearPair() {
        pair = nil
        recognized = false
        startSpan = 0
        lastScale = 1
        focus = nil
        sharedScrolls.removeAll()
        lift = PinchLift()
    }

    private func cancelActivePair(on node: NodeView, in presenter: Presenter) {
        let point = focus
        lift.endPair(scrolls: sharedScrolls)
        clearPair()
        if let point {
            presenter.pinch(node, ended: true, scale: 1, x: Double(point.x), y: Double(point.y), cancelled: true)
        }
    }

    fileprivate static func commonScrolls(of a: UIView?, and b: UIView?) -> [UIScrollView] {
        var scrolls: [UIScrollView] = []
        var hit = a
        while let v = hit {
            if let scroll = v as? UIScrollView, b == nil || b?.isDescendant(of: scroll) == true {
                scrolls.append(scroll)
            }
            hit = v.superview
        }
        return scrolls
    }
}

/// Observes indirect (trackpad) pinches on iPadOS independently of direct
/// finger tracking, so a resting direct touch never swallows a trackpad pinch.
final class ReportingIndirectPinch: UIPinchGestureRecognizer, UIGestureRecognizerDelegate {
    private weak var node: NodeView?
    private var recognized = false
    private var lastScale: CGFloat = 1
    private var focus: CGPoint?
    private var sharedScrolls: [UIScrollView] = []
    private var lift = PinchLift()

    var idle: Bool { !recognized && state == .possible }

    init(on node: NodeView) {
        self.node = node
        super.init(target: nil, action: nil)
        allowedTouchTypes = []
        addTarget(self, action: #selector(report))
        delegate = self
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }
    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }
    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool { false }

    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let node, SwipeInput.allows(node), let presenter = node.presenter else { return false }
        guard let hit = node.window?.hitTest(location(in: node.window), with: nil) else { return true }
        guard ReportingPinch.allowsHit(hit, on: node, in: presenter) else { return false }
        return !presenter.authoredNodes(from: hit).prefix(while: { $0 !== node }).contains { child in
            child.extras?.reportingIndirectPinch != nil && SwipeInput.allows(child)
        }
    }

    @objc private func report() {
        guard let node, let presenter = node.presenter else { return }
        switch state {
        case .began, .changed:
            guard SwipeInput.allows(node) else {
                if recognized, let point = focus {
                    lift.endPair(scrolls: sharedScrolls)
                    clear()
                    presenter.pinch(node, ended: true, scale: 1, x: Double(point.x), y: Double(point.y), cancelled: true)
                }
                state = .cancelled
                return
            }
            if !recognized {
                recognized = true
                lift = PinchLift()
                sharedScrolls = ReportingPinch.commonScrolls(of: node, and: nil)
                for scroll in sharedScrolls { PinchLift.attach(lift, to: scroll) }
            }
            if scale.isFinite && scale > 0 { lastScale = scale }
            let point = presenter.client(location(in: node.window))
            focus = point
            presenter.pinch(node, ended: false, scale: Double(lastScale), x: Double(point.x), y: Double(point.y))
        case .ended, .cancelled, .failed:
            guard recognized, let point = focus else { clear(); return }
            let cancelled = state != .ended
            let factor: CGFloat = cancelled ? 1 : lastScale
            lift.endPair(scrolls: sharedScrolls)
            clear()
            presenter.pinch(node, ended: true, scale: Double(factor), x: Double(point.x), y: Double(point.y), cancelled: cancelled)
        default: break
        }
    }

    override func reset() {
        super.reset()
        clear()
    }

    func resetForRecycle() {
        for scroll in sharedScrolls { PinchLift.clear(for: scroll) }
        clear()
    }

    private func clear() {
        recognized = false
        lastScale = 1
        focus = nil
        sharedScrolls.removeAll()
        lift = PinchLift()
    }
}

/// Suppresses post-pinch scroll deceleration on any `UIScrollView` (plain,
/// grouped list, viewport, or paging) when a pair lifts cleanly or with a
/// stationary survivor. Stores only value snapshots (no `UITouch` references),
/// and restores normal flinging once the survivor or a scroll offset moves
/// past `jitter` after the pair ended.
final class PinchLift: NSObject {
    private static let lifts = NSMapTable<UIScrollView, PinchLift>.weakToStrongObjects()
    static let jitter: CGFloat = 2

    private var pairEnded = false
    private var moved = false
    private var releaseOffsets: [ObjectIdentifier: CGPoint] = [:]

    static func attach(_ lift: PinchLift, to scroll: UIScrollView) {
        if let existing = lifts.object(forKey: scroll) {
            if existing === lift { return }
            scroll.panGestureRecognizer.removeTarget(existing, action: #selector(panChanged(_:)))
        }
        scroll.panGestureRecognizer.addTarget(lift, action: #selector(panChanged(_:)))
        lifts.setObject(lift, forKey: scroll)
    }

    static func consume(for scroll: UIScrollView) -> Bool {
        guard let lift = lifts.object(forKey: scroll) else { return false }
        clear(for: scroll)
        return lift.endsPinch(for: scroll)
    }

    static func clear(for scroll: UIScrollView) {
        guard let lift = lifts.object(forKey: scroll) else { return }
        lifts.removeObject(forKey: scroll)
        scroll.panGestureRecognizer.removeTarget(lift, action: #selector(panChanged(_:)))
    }

    static func clearIfEnded(for scroll: UIScrollView) {
        guard let lift = lifts.object(forKey: scroll), lift.pairEnded else { return }
        clear(for: scroll)
    }

    func endPair(scrolls: [UIScrollView] = []) {
        pairEnded = true
        for scroll in scrolls { releaseOffsets[ObjectIdentifier(scroll)] = scroll.contentOffset }
    }

    func noteMoved() { moved = true }

    @objc func panChanged(_ pan: UIPanGestureRecognizer) {
        guard pairEnded, pan.state == .changed else { return }
        if let scroll = pan.view as? UIScrollView, let release = releaseOffsets[ObjectIdentifier(scroll)],
           hypot(scroll.contentOffset.x - release.x, scroll.contentOffset.y - release.y) > Self.jitter {
            moved = true
        }
    }

    func endsPinch(for scroll: UIScrollView? = nil) -> Bool {
        if moved { return false }
        if let scroll, let release = releaseOffsets[ObjectIdentifier(scroll)],
           hypot(scroll.contentOffset.x - release.x, scroll.contentOffset.y - release.y) > Self.jitter {
            return false
        }
        return true
    }
}

extension Presenter {
    func pinch(_ view: NodeView, ended: Bool, scale: Double, x: Double, y: Double, cancelled: Bool = false) {
        let kind: UInt32 = ended ? 68 : 67
        let name = ended ? "pinchrelease" : "pinch"
        let id = view.id
        guard views[id] === view, view.handlers.contains(name), cancelled || SwipeInput.allows(view) else { return }
        let payload = "\(scale),\(x),\(y)"
        send(id) { [weak self, weak view] in
            guard let self, let view, views[id] === view, view.handlers.contains(name), cancelled || SwipeInput.allows(view) else { return }
            onClipboard?(id, kind, payload)
        }
    }
}

extension NodeView {
    func updateReportingPinch() {
        let hears = handlers.contains("pinch") || handlers.contains("pinchrelease")
        if hears {
            if extras?.reportingPinch == nil {
                let pinch = ReportingPinch(on: self)
                more.reportingPinch = pinch
                addGestureRecognizer(pinch)
            }
            if extras?.reportingIndirectPinch == nil {
                let indirect = ReportingIndirectPinch(on: self)
                more.reportingIndirectPinch = indirect
                addGestureRecognizer(indirect)
            }
        } else {
            if let pinch = extras?.reportingPinch {
                pinch.resetForRecycle()
                removeGestureRecognizer(pinch)
                more.reportingPinch = nil
            }
            if let indirect = extras?.reportingIndirectPinch {
                indirect.resetForRecycle()
                removeGestureRecognizer(indirect)
                more.reportingIndirectPinch = nil
            }
        }
    }
}
#endif
