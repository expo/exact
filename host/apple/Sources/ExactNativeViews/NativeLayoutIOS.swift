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
    /// Each held view's own layout margins, and whether it insets them from its safe area, from before it was held:
    /// meanwhile its margins also take what of it lies past its box.
    private let held = NSMapTable<UIView, Margins>.weakToStrongObjects()
    private final class Margins {
        let insets: UIEdgeInsets, fromSafeArea: Bool
        init(_ view: UIView) {
            fromSafeArea = view.insetsLayoutMarginsFromSafeArea
            // `layoutMargins` reads the safe area added where the view insets its margins from it
            let safe = fromSafeArea ? view.safeAreaInsets : .zero, m = view.layoutMargins
            insets = UIEdgeInsets(top: max(0, m.top - safe.top), left: max(0, m.left - safe.left),
                                  bottom: max(0, m.bottom - safe.bottom), right: max(0, m.right - safe.right))
        }
    }

    /// Whether `view` keeps its own size until its box's animation ends.
    func holds(_ view: UIView) -> Bool { held.object(forKey: view) != nil }

    /// `owner`'s box, which holds `view`, changes size in `resize`; `end` lays the view out in its content box once
    /// the box's animation is over. Meanwhile the view's layout margins take what of it lies past the box, so what
    /// it places by its margins (a map's logo and legal link, its compass) stands where the box shows it and moves
    /// with the box, as it would in a view that took the box's size.
    func resize(_ view: UIView, in owner: NodeView, _ resize: () -> Void, end: @escaping () -> Void) {
        let old = view.bounds.size, holding = held.object(forKey: view)
        view.autoresizingMask = []
        resize()
        let box = owner.contentBox(), own = holding ?? Margins(view)
        let size = CGSize(width: max(old.width, box.width), height: max(old.height, box.height))
        let safe = own.fromSafeArea ? view.safeAreaInsets : .zero
        func past(_ shown: CGSize) -> UIEdgeInsets {
            let x = max(0, size.width - shown.width) / 2, y = max(0, size.height - shown.height) / 2
            return UIEdgeInsets(top: own.insets.top + safe.top + y, left: own.insets.left + safe.left + x,
                                bottom: own.insets.bottom + safe.bottom + y, right: own.insets.right + safe.right + x)
        }
        UIView.performWithoutAnimation {
            view.bounds.size = size
            view.insetsLayoutMarginsFromSafeArea = false
            // Newly held, its margins start where its old box showed it
            if holding == nil { view.layoutMargins = past(old); view.layoutIfNeeded() }
        }
        view.center = CGPoint(x: box.midX, y: box.midY)
        view.layoutMargins = past(box.size)
        view.layoutIfNeeded()
        guard holding == nil else { return }
        held.setObject(own, forKey: view)
        // The box's animation ends on the first frame with nothing of its place or size in flight
        FrameClock.shared.want(view, .nativeResize) { [weak self, weak view, weak owner] _ in
            if let owner, owner.window != nil, Self.moving(owner.layer) { return }
            guard let self, let view, let own = held.object(forKey: view) else { return }
            FrameClock.shared.drop(view)
            held.removeObject(forKey: view)
            view.insetsLayoutMarginsFromSafeArea = own.fromSafeArea
            view.layoutMargins = own.insets
            end()
        }
    }

    /// Whether an animation of `layer`'s place or size is under way (`LayerMotion.inFlight`: not one held at
    /// speed 0 or kept past its end).
    private static func moving(_ layer: CALayer) -> Bool {
        let now = CACurrentMediaTime()
        LayerMotion.pruneUnbegun(now: now)
        return (layer.animationKeys() ?? []).contains { key in
            (key.hasPrefix("bounds") || key.hasPrefix("position"))
                && layer.animation(forKey: key).map { LayerMotion.inFlight($0, on: layer, key: key, now: now) } == true
        }
    }
}
#endif
