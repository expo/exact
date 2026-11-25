import ExactKit
import CExact
#if os(iOS) || os(tvOS)
import UIKit
import QuartzCore

extension NodeView {
    func updateHeightDragGesture() {
        if presenter?.heightBindings[id]?.target != nil, heightRecognizer == nil {
            let pan: TimedPan
            #if os(iOS)
            if kind == "list" || kind == "scroll" {
                let sheet = SheetScrollPan(target: self, action: #selector(heightDragging(_:)))
                sheet.owner = self
                pan = sheet
            } else {
                pan = TimedPan(target: self, action: #selector(heightDragging(_:)))
            }
            #else
            pan = TimedPan(target: self, action: #selector(heightDragging(_:)))
            #endif
            #if !os(tvOS)
            pan.maximumNumberOfTouches = 1
            #endif
            pan.delegate = self
            addGestureRecognizer(pan)
            heightRecognizer = pan
        } else if presenter?.heightBindings[id]?.target == nil, let pan = heightRecognizer {
            let previous = heightHold; heightHold = nil
            DispatchQueue.main.async { previous?.cancel() }
            removeGestureRecognizer(pan)
            heightRecognizer = nil
        }
    }
    /// Whether the height drag's recognizer begins; nil when `gesture` is another.
    func heightShouldBegin(_ gesture: UIGestureRecognizer) -> Bool? {
        guard gesture === heightRecognizer, let pan = gesture as? UIPanGestureRecognizer else { return nil }
        let velocity = pan.velocity(in: window), translation = pan.translation(in: window)
        #if os(iOS)
        if let sheet = pan as? SheetScrollPan, sheet.tracksScroll, !sheet.admits(velocity == .zero ? translation : velocity) { return false }
        #endif
        return SwipeInput.allows(self) && HeightDragDirection.accepts(
            velocityX: Double(velocity.x), velocityY: Double(velocity.y),
            translationX: Double(translation.x), translationY: Double(translation.y))
            && presenter?.heightBindings[id]?.target != nil
    }
    @objc func heightDragging(_ pan: UIPanGestureRecognizer) {
        #if os(iOS)
        if let sheet = pan as? SheetScrollPan, sheet.tracksScroll {
            sheet.track()
            return
        }
        #endif
        // UIKit's translation starts where it recognized the pan, past its
        // hysteresis. The drag is the finger's travel since it touched down:
        // the hold begins there and then, and steps to the finger in its
        // first sample, so the header stays under the finger and the release
        // velocity counts the travel at the finger's pace.
        let timed = pan as? TimedPan, down = timed?.down
        let translation = down.map { Double(pan.location(in: nil).y - $0.point.y) } ?? Double(pan.translation(in: window).y)
        let time = timed?.time(pan.state) ?? CACurrentMediaTime()
        switch pan.state {
        case .began:
            heightHold?.cancel()
            heightHold = HeightDragHold(self, time: down?.time ?? time)
            // UIKit may deliver a coalesced drag entirely in its first sample.
            // Catch the current presentation, then apply that sample too.
            if let hold = heightHold as? HeightDragHold, !hold.move(downward: translation, time: time) {
                hold.cancel(); heightHold = nil
            }
        case .changed:
            guard let hold = heightHold as? HeightDragHold else { return }
            if !hold.move(downward: translation, time: time) {
                hold.cancel(); heightHold = nil
            }
        case .ended, .cancelled, .failed:
            let previous = heightHold as? HeightDragHold; heightHold = nil
            previous?.finish(downward: translation, time: time, cancel: pan.state != .ended)
        default: break
        }
    }
}
#endif
