#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass

/// This recognizer observes alongside the scroll view's pan (LLP 1057.001 §4.1).
final class ReportingPinch: UIPinchGestureRecognizer, UIGestureRecognizerDelegate {
    private weak var node: NodeView?
    private var recognized = false
    private var focus: CGPoint?
    private var fingers: [UITouch] = []
    private var startSpan: CGFloat = 0
    private var lastScale: CGFloat = 1
    private var sharedScrolls: [ScrollView] = []
    private var lift = PinchLift()

    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) {
        for t in touches where startSpan == 0 && t.type == .direct && fingers.count < 2 { fingers.append(t) }
        if startSpan == 0, fingers.count == 2 {
            let a = fingers[0].location(in: nil), b = fingers[1].location(in: nil)
            startSpan = hypot(a.x - b.x, a.y - b.y)
            samplePair()
            lift.enroll(fingers)
            var hit = fingers[0].view
            while let v = hit {
                if let scroll = v as? ScrollView, fingers[1].view?.isDescendant(of: scroll) == true { sharedScrolls.append(scroll) }
                hit = v.superview
            }
        }
        super.touchesBegan(touches, with: event)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        samplePair()
        super.touchesMoved(touches, with: event)
    }

    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) {
        let endsPair = fingers.count == 2 && fingers.contains(where: touches.contains)
        if endsPair {
            if let survivor = fingers.first(where: { !touches.contains($0) }) { lift.surviving(survivor) }
            fingers.removeAll()
        }
        super.touchesEnded(touches, with: event)
        if endsPair && (state == .began || state == .changed) { state = .ended }
    }

    private func samplePair() {
        guard let presenter = node?.presenter, fingers.count == 2, startSpan > 0 else { return }
        guard fingers.allSatisfy({ $0.phase != .ended && $0.phase != .cancelled }) else { return }
        let a = fingers[0].location(in: nil), b = fingers[1].location(in: nil)
        let distance = hypot(a.x - b.x, a.y - b.y)
        let factor = distance / startSpan
        if factor.isFinite && factor > 0 { lastScale = factor }
        focus = presenter.client(CGPoint(x: (a.x + b.x) / 2, y: (a.y + b.y) / 2))
    }

    override func reset() {
        super.reset()
        fingers.removeAll(); sharedScrolls.removeAll()
        recognized = false; startSpan = 0; lastScale = 1; focus = nil; lift = PinchLift()
    }

    init(on node: NodeView) {
        self.node = node
        super.init(target: nil, action: nil)
        addTarget(self, action: #selector(report))
        delegate = self
        cancelsTouchesInView = false
        delaysTouchesBegan = false
        delaysTouchesEnded = false
    }

    override func canPrevent(_ other: UIGestureRecognizer) -> Bool { false }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool { false }

    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldRecognizeSimultaneouslyWith other: UIGestureRecognizer) -> Bool { true }

    func gestureRecognizer(_ gesture: UIGestureRecognizer, shouldReceive touch: UITouch) -> Bool {
        if numberOfTouches >= 2 { return false }
        guard let hit = touch.view, let node, let presenter = node.presenter else { return false }
        if !node.isTextControl && node.kind != "control",
           presenter.logicalAncestry(of: hit).prefix(while: { $0 !== node }).contains(where: {
               $0 is UIControl || $0 is UITextView
           }) { return false }
        return !presenter.authoredNodes(from: hit).prefix(while: { $0 !== node }).contains {
            $0.isTextControl || $0.kind == "control"
        }
    }

    func gestureRecognizerShouldBegin(_ gesture: UIGestureRecognizer) -> Bool {
        guard let node, SwipeInput.allows(node), let presenter = node.presenter else { return false }
        let hit = fingers.first?.view ?? node.window?.hitTest(location(in: node.window), with: nil)
        guard let hit else { return true }
        let other = fingers.count == 2 ? fingers[1].view : hit
        return !presenter.authoredNodes(from: hit).prefix(while: { $0 !== node }).contains { child in
            child.extras?.reportingPinch != nil && SwipeInput.allows(child)
                && (other === child || other?.isDescendant(of: child) == true)
        }
    }

    @objc private func report() {
        guard let node, let presenter = node.presenter, let session = presenter.session else { return }
        let ended: Bool
        switch state {
        case .began, .changed:
            recognized = true
            if startSpan == 0 {
                guard fingers.isEmpty else { return }
                focus = presenter.client(location(in: node.window))
                if scale.isFinite && scale > 0 { lastScale = scale }
            } else if fingers.count != 2 { return }
            for scroll in sharedScrolls where scroll.pinchLift !== lift { scroll.pinchLift = lift }
            ended = false
        case .ended, .cancelled, .failed:
            guard recognized else { return }
            recognized = false
            ended = true
        default: return
        }
        guard let point = focus else { return }
        if ended { focus = nil }
        guard node.handlers.contains(ended ? "pinchrelease" : "pinch") else { return }
        let factor = state == .cancelled || state == .failed ? 1 : lastScale
        let payload = "\(factor),\(point.x),\(point.y)"
        session.applyInput(session.runtime.clipboard(node.id, ended ? 68 : 67, payload, now: session.now()), at: node.id)
    }
}

/// A stationary survivor is the pinch's staggered lift. A survivor that
/// moves again can throw the scroll in its own right.
final class PinchLift: NSObject {
    private weak var first: UITouch?
    private weak var second: UITouch?
    private var moved = false

    func enroll(_ fingers: [UITouch]) { first = fingers[0]; second = fingers[1] }

    var hasContact: Bool {
        [first, second].contains { touch in
            guard let touch else { return false }
            return touch.phase == .began || touch.phase == .moved || touch.phase == .stationary
        }
    }

    private weak var finger: UITouch?
    private var at: CGPoint = .zero

    func surviving(_ touch: UITouch) {
        finger = touch
        at = touch.location(in: nil)
    }

    @objc func panChanged() {
        if let finger, finger.phase == .moved, finger.location(in: nil) != at { moved = true }
    }

    var endsPinch: Bool { !moved && (finger.map { $0.location(in: nil) == at } ?? true) }
}

extension NodeView {
    func updateReportingPinch() {
        let hears = handlers.contains("pinch") || handlers.contains("pinchrelease")
        if hears, extras?.reportingPinch == nil {
            let pinch = ReportingPinch(on: self)
            more.reportingPinch = pinch
            addGestureRecognizer(pinch)
        } else if !hears, let pinch = extras?.reportingPinch {
            removeGestureRecognizer(pinch)
            more.reportingPinch = nil
        }
    }
}
#endif
