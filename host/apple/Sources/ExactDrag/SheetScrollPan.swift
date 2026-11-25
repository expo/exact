#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass
import ExactKit

/// UIKit's scroll pan keeps its velocity and deceleration. During a mixed
/// drag the list receives only the travel left over from the displayed height.
final class SheetScrollPan: TimedPan, SheetScrollGesture {
    weak var owner: NodeView?
    private weak var port: UIScrollView?
    private var finger: UITouch?
    private var binding: HeightDragBinding?
    private var previousTranslation: CGFloat = 0
    private var previousPortTranslation: CGFloat = 0
    private var previousTime: TimeInterval = 0
    private var startOffset: CGFloat = 0
    private var offset: CGFloat = 0
    private var hold: HeightDragHold?
    private var movedSheet = false
    private var sampling = false
    private var writingOffset = false
    private var releasedAtStart: Bool?
    var tracksScroll: Bool { finger != nil }

    func receive(_ touch: UITouch, in scroll: UIScrollView) -> Bool {
        guard finger == nil, let owner, owner.scrollView === scroll, scroll.isScrollEnabled,
              owner.scrollsVertically, let binding = owner.presenter?.heightBindings[owner.id], binding.target != nil else { return false }
        self.binding = binding
        port = scroll
        finger = touch
        previousTranslation = 0
        previousPortTranslation = 0
        previousTime = touch.timestamp
        startOffset = scroll.contentOffset.y
        offset = max(0, scroll.contentOffset.y + scroll.adjustedContentInset.top)
        releasedAtStart = nil
        return true
    }

    func admits(_ direction: CGPoint) -> Bool {
        guard let owner, let port, port === owner.scrollView, owner.scrollsVertically,
              owner.allowsTouchPan(direction) else { return false }
        if let scroll = port as? ScrollView, let down,
           !scroll.admitsPan(velocity: direction, translation: .zero, start: scroll.convert(down.point, from: nil), chain: false) { return false }
        if let scroll = port as? ScrollView, scroll.chains(direction), let target = liveTarget,
           !canResize(target, expanding: direction.y < 0) { return false }
        if let snapshot = owner.presenter?.collections.snapshot(owner.id) { return !snapshot.horizontal }
        let inset = port.adjustedContentInset
        let horizontal = port.contentSize.width + inset.left + inset.right > port.bounds.width + 0.5
        let vertical = port.contentSize.height + inset.top + inset.bottom > port.bounds.height + 0.5
        return owner.kind != "list" || !horizontal || vertical
    }

    private var liveTarget: NodeView? {
        guard let owner, let binding, let presenter = owner.presenter, presenter.heightBindings[owner.id] == binding,
              let id = binding.target else { return nil }
        return presenter.views[id]
    }

    private func limit(_ row: String, of target: NodeView) -> CGFloat? {
        guard let value = target.style[row] else { return nil }
        if let points = value.number { return CGFloat(points) }
        guard let presenter = target.presenter else { return nil }
        let absolute = target.layoutPosition == "absolute"
        var parent = presenter.logicalParent(of: target.id)
        while let id = parent {
            parent = presenter.logicalParent(of: id)
            if let box = presenter.views[id] {
                if absolute && box.layoutPosition == "static" && parent != nil { continue }
                let basis = absolute ? box.layoutBounds.height - box.number("border_width_top") - box.number("border_width_bottom") : box.contentBox().height
                return value.length(basis: basis)
            }
            if let flat = presenter.flats.leaves[id] {
                if absolute && NodeView.layoutPosition(flat.style, popover: false) == "static" && parent != nil { continue }
                return value.length(basis: flat.layout.height)
            }
        }
        return value.length(basis: presenter.viewportScroll.bounds.height)
    }

    func takesEdge(in scroll: UIScrollView, direction: CGPoint) -> Bool {
        guard tracksScroll, port === scroll, abs(direction.y) > abs(direction.x),
              let target = liveTarget, admits(direction) else { return false }
        return canResize(target, expanding: direction.y < 0)
    }

    private func canResize(_ target: NodeView, expanding: Bool) -> Bool {
        let height = target.bounds.height
        if expanding { return height < (limit("max_height", of: target) ?? .infinity) }
        return height > (limit("min_height", of: target) ?? 0)
    }

    override func canPrevent(_ other: UIGestureRecognizer) -> Bool {
        if tracksScroll, other === port?.panGestureRecognizer { return false }
        return super.canPrevent(other)
    }
    override func canBePrevented(by other: UIGestureRecognizer) -> Bool {
        if tracksScroll, other === port?.panGestureRecognizer { return false }
        return super.canBePrevented(by: other)
    }

    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) {
        super.touchesMoved(touches, with: event)
        if state == .began || state == .changed { sample() }
        else if state == .possible { previousTime = event.timestamp }
    }

    func track() {
        switch state {
        case .began, .changed: sample()
        case .ended, .cancelled, .failed:
            if state == .ended { sample() }
            guard let owner, let hold, owner.heightHold === hold else { return }
            if movedSheet && state == .ended {
                releasedAtStart = atStart
                // UIKit asks for its rest in the same event turn, on either
                // side of the recognizer action. A later drag owns its own rest.
                DispatchQueue.main.async { [weak self] in self?.releasedAtStart = nil }
            }
            owner.heightHold = nil
            hold.finish(time: time(state), cancel: state != .ended || !movedSheet)
        default: break
        }
    }

    override func reset() {
        finger = nil
        hold = nil
        movedSheet = false
        super.reset()
    }

    private var atStart: Bool { offset <= 1 }

    private func sample() {
        guard !sampling, let finger, let port, let owner, owner.scrollView === port,
              owner.presenter?.session?.isApplyingPresentation == false,
              let target = liveTarget else { return }
        // A sheet reads the recognized pan in its own coordinates. Translation
        // excludes UIKit's recognition slop and follows the target's scale.
        let translation = translation(in: target).y, time = finger.timestamp
        let portTranslation = self.translation(in: port).y
        let travel = previousTranslation - translation, listTravel = previousPortTranslation - portTranslation
        guard time >= previousTime, travel != 0 || (finger.phase != .ended && time > previousTime) else { return }
        sampling = true
        defer {
            previousTranslation = translation
            previousPortTranslation = portTranslation
            previousTime = time
            sampling = false
            _ = correctOffset(in: port)
        }
        // Downward motion reaches the sheet only after consuming the list offset.
        var offered = travel
        if travel < 0, listTravel < 0 {
            if atStart { offset = 0 }
            offered *= 1 - min(1, offset / -listTravel)
        }
        if offered != 0, !canResize(target, expanding: offered > 0) { offered = 0 }
        var spent: CGFloat = 0
        if offered != 0 || hold != nil {
            if hold == nil {
                hold = HeightDragHold(owner, time: previousTime)
                owner.heightHold = hold
            }
            guard let hold else { return }
            let before = target.bounds.height
            let displacement = hold.mapping.base - Double(before + offered)
            guard hold.move(downward: displacement, time: time) else {
                hold.cancel()
                if owner.heightHold === hold { owner.heightHold = nil }
                self.binding = nil
                return
            }
            spent = target.bounds.height - before
            if abs(spent) > 0.001 {
                movedSheet = true
                if let scroll = port as? ScrollView, scroll.snapPages { scroll.isPagingEnabled = false }
            }
        }
        // Both translations describe the same finger sample in their own coordinates.
        let remaining = travel == 0 ? listTravel : listTravel * (1 - spent / travel)
        offset = max(0, offset + remaining)
        if !port.bounces { offset = min(offset, endOffset(in: port)) }
    }

    private func endOffset(in scroll: UIScrollView) -> CGFloat {
        let inset = scroll.adjustedContentInset
        return max(0, scroll.contentSize.height + inset.top + inset.bottom - scroll.bounds.height)
    }

    func correctOffset(in scroll: UIScrollView) -> Bool {
        guard movedSheet, port === scroll, !writingOffset, !sampling, liveTarget != nil,
              state == .began || state == .changed || state == .ended else { return false }
        if scroll.bounces, offset > endOffset(in: scroll) { return false }
        let wanted = offset - scroll.adjustedContentInset.top
        guard abs(scroll.contentOffset.y - wanted) > 0.01 else { return false }
        writingOffset = true
        scroll.contentOffset.y = wanted
        writingOffset = false
        return true
    }

    func listDrag(in scroll: UIScrollView) -> (start: CGFloat, atStart: Bool)? {
        if finger?.phase == .ended { sample() }
        guard port === scroll, liveTarget != nil, movedSheet || releasedAtStart != nil else { return nil }
        return (startOffset, releasedAtStart ?? atStart)
    }
}
#endif
