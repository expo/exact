// How a platform view is laid out as its box or its window moves: re-armed once per changed window geometry
// (`NativeWorldLayout`), and keeping a size of its own while UIKit animates the size of a box that clips it
// (`NativeCrop`).
#if os(iOS) || os(tvOS)
import ExactKit
import UIKit

/// A fixed-size native child need not get UIKit layout when only an ancestor
/// moves. Re-arm it after a geometry batch, once per changed window geometry.
struct NativeWorldLayout {
    private struct Geometry: Equatable {
        let frame: CGRect
        let scale: CGFloat
        let window: ObjectIdentifier
    }
    private var previous: Geometry?

    mutating func refresh(_ view: UIView) {
        guard let window = view.window else { previous = nil; return }
        let next = Geometry(frame: view.convert(view.bounds, to: window),
                            scale: window.screen.scale, window: ObjectIdentifier(window))
        guard next != previous else { return }
        previous = next
        view.setNeedsLayout()
    }
}

/// The platform views that keep a size of their own while UIKit animates the size of the box that clips them
/// (`NativeViews.resize`). A platform view lays its content out for its bounds once, not at each size between, and a
/// map keeps its centre and scale as they change, showing more or less around them. In a box that clips its content
/// (`overflow`), the view takes the larger of its old and new sizes at once, centred in the box, and only its centre
/// moves with the box: the clip UIKit moves shows more of the new layout as the box grows, as SwiftUI's map does, and
/// less of the old one as it shrinks, never a layout smaller than the box around it. Once the box's animation ends,
/// the view takes its content box again.
final class NativeCrop {
    private let held = NSHashTable<UIView>.weakObjects()

    /// Whether `view` keeps its own size until its box's animation ends.
    func holds(_ view: UIView) -> Bool { held.contains(view) }

    /// `owner`'s box, which holds `view`, changes size in `resize`; `end` lays the view out in its content box once
    /// the box's animation is over.
    func resize(_ view: UIView, in owner: NodeView, _ resize: () -> Void, end: @escaping () -> Void) {
        let old = view.bounds.size
        view.autoresizingMask = []
        resize()
        let box = owner.contentBox()
        UIView.performWithoutAnimation { view.bounds.size = CGSize(width: max(old.width, box.width), height: max(old.height, box.height)) }
        view.center = CGPoint(x: box.midX, y: box.midY)
        guard !held.contains(view) else { return }
        held.add(view)
        // The box's animation ends on the first frame with nothing of its place or size in flight
        FrameClock.shared.want(view, .nativeResize) { [weak self, weak view, weak owner] _ in
            if let owner, owner.window != nil, Self.moving(owner.layer) { return }
            guard let self, let view, held.contains(view) else { return }
            FrameClock.shared.drop(view)
            held.remove(view)
            end()
        }
    }

    /// Whether an animation of `layer`'s place or size is under way (`Agent.inFlight`: not one held at speed 0
    /// or kept past its end).
    private static func moving(_ layer: CALayer) -> Bool {
        let now = CACurrentMediaTime()
        return (layer.animationKeys() ?? []).contains { key in
            (key.hasPrefix("bounds") || key.hasPrefix("position"))
                && layer.animation(forKey: key).map { Agent.inFlight($0, on: layer, key: key, now: now) } == true
        }
    }
}
#endif
