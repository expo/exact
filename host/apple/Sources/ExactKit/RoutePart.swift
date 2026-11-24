// @ref LLP 1013.001 D1 — a route's own entrance or exit, as Core Animation
// plays it: the part the route declares (`::view-transition-new`,
// `::view-transition-old`), posed on the Rust side for the route's box
// (`Host::route_part`) and played on the route's layer as one linear
// keyframe track on the wall clock, the clock UIKit's own push and AppKit's
// animations keep. Only the route the change adds or removes moves; the
// other rests.
import CExact
import ExactCore
import QuartzCore

/// A route's part for its box: how long it plays, and its poses at even
/// steps from its start to its end, each opacity, translation x and y
/// (points, y down), scale x and y, and rotation (degrees clockwise), about the box's
/// centre. No pose is the author's cut (`animation="none"`).
package struct RoutePart {
    package let seconds: Double
    let poses: [[Double]]

    init?(json: Data) {
        guard let object = try? JSONSerialization.jsonObject(with: json) as? [String: Any],
              let seconds = object["seconds"] as? Double,
              let poses = object["poses"] as? [[Double]], poses.allSatisfy({ $0.count == 6 }) else { return nil }
        self.seconds = seconds
        self.poses = poses
    }

    /// Whether it moves anything: a cut does not, nor does any part while
    /// the person asks for less motion (D6).
    package var plays: Bool { seconds > 0 && poses.count > 1 && !DisplayPreferences.reducedMotion }

    /// A pose as a layer's transform, turning and scaling about `centre`,
    /// the box's centre in the layer's own space: zero for a layer anchored
    /// there (UIKit's), the middle of its bounds for one anchored at its
    /// origin (AppKit's).
    static func transform(_ pose: [Double], about centre: CGPoint = .zero) -> CATransform3D {
        var t = CATransform3DMakeTranslation(pose[1] + centre.x, pose[2] + centre.y, 0)
        t = CATransform3DRotate(t, pose[5] * .pi / 180, 0, 0, 1)
        t = CATransform3DScale(t, pose[3], pose[4], 1)
        return CATransform3DTranslate(t, -centre.x, -centre.y, 0)
    }

    /// The part as one animation of a layer's opacity and transform, held at
    /// its end until removed.
    package func animation(about centre: CGPoint = .zero) -> CAAnimation {
        let steps = poses.count - 1
        let times = (0...steps).map { NSNumber(value: Double($0) / Double(steps)) }
        let opacity = CAKeyframeAnimation(keyPath: "opacity")
        opacity.values = poses.map { $0[0] }
        let transform = CAKeyframeAnimation(keyPath: "transform")
        transform.values = poses.map { NSValue(caTransform3D: Self.transform($0, about: centre)) }
        for track in [opacity, transform] {
            track.keyTimes = times
            track.calculationMode = .linear
        }
        let group = CAAnimationGroup()
        group.animations = [opacity, transform]
        group.duration = seconds
        group.fillMode = .both
        group.isRemovedOnCompletion = false
        return group
    }
}

extension ExactSession {
    /// The entrance (`exit` false) or exit `route` declares, posed for a box
    /// `size` large (`Runtime.routePart`). The runner is asked only for a
    /// part the route's style declares, so main does not wait on the owner
    /// to learn that a route declares none, as most do (LLP 1072 T2).
    package func routePart(_ route: NodeView, exit: Bool, size: CGSize) -> RoutePart? {
        guard route.style[exit ? "view_transition_old" : "view_transition_new"] != nil else { return nil }
        return runtime.routePart(route.id, exit: exit, size: size)
    }
}

extension Runtime {
    /// The entrance (`exit` false) or exit route `view` declares, posed for
    /// a box `size` large; nil where it declares none or may play none (a
    /// sheet, a zoom), and the platform's transition runs.
    package func routePart(_ view: UInt32, exit: Bool, size: CGSize) -> RoutePart? {
        on(busy: nil) {
            let len = exact_route_part(rt, view, exit ? 1 : 0, Double(size.width), Double(size.height))
            return RoutePart(json: Data(bytes: exact_out(rt), count: Int(len)))
        }
    }
}
