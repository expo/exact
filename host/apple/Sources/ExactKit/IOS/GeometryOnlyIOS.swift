// PROTO (UIKit-first measurement; not landing code). The live path's geometry-only batch
// (notes `design/safe-area-design.md` §6) applies frames, scroll content sizes, field text boxes,
// owner block rects and the routes' reach. Controls, accessibility, menus, swipe actions,
// row cells and popovers' anchors wait for the first frame without such a batch, after the edge
// stops, or join the next full batch.
#if os(iOS) || os(tvOS)
import UIKit

/// PROTO (UIKit-first measurement): the deferred views, quiet-frame clock, last root attributes,
/// active run and top-reaching view IDs retained between geometry-only batches
final class GeometryOnlyState {
    var deferredViews = Set<UInt32>()
    var quiet: GeometryQuiet?
    var dataset = ""
    var lastRun: String?
    var run: EdgeRun?
    var topReachers: [UInt32: [UInt32]] = [:]
}

extension Presenter {
    /// PROTO (UIKit-first measurement): whether `batch` contains only frames, scroll content sizes,
    /// field text boxes or owner block rects, with no controls, under the `geometry` mode.
    /// Finish the full projection before newly mounted views have their first frame.
    func geometryOnly(_ batch: Batch) -> Bool {
        ExactSession.protoMode == .geometry && awaitingFrame.isEmpty && !batch.controls && !batch.ops.isEmpty
            && batch.ops.allSatisfy { op in
                switch op.op {
                case .frame, .content, .fieldContent: true
                case .unknown: op.payload["op"] as? String == "block"
                default: false
                }
            }
    }

    /// PROTO (UIKit-first measurement): the full reach walk after a moving edge's own, under
    /// `ExactProtoVerify`, and the number of native frames it corrects and their largest change.
    /// Both are 0 when the moving edge's reach was right.
    func verifyMovingTops() {
        guard Self.protoVerify else { return }
        let before = views.mapValues { $0.frame }
        reachSafeEdges()
        var changed = 0, most: CGFloat = 0
        for (id, frame) in before {
            guard let now = views[id]?.frame, now != frame else { continue }
            changed += 1
            most = max(most, abs(now.minX - frame.minX), abs(now.minY - frame.minY), abs(now.width - frame.width), abs(now.height - frame.height))
            if most > 1 { NSLog("PROTO verify view %u %@ %@ → %@", id, views[id]?.props["testId"] ?? views[id]?.kind ?? "?", NSCoder.string(for: frame), NSCoder.string(for: now)) }
        }
        NSLog("PROTO verify changed %d frames, at most %.2f", changed, most)
    }
    /// PROTO (UIKit-first measurement): whether `ExactProtoVerify` asks for the full reach check
    static let protoVerify = UserDefaults.standard.bool(forKey: "ExactProtoVerify")

    /// PROTO (UIKit-first measurement): the boxes a geometry-only batch moved, scheduled for
    /// synchronization once a frame passes without another such batch
    func deferGeometry(_ ids: [UInt32]) {
        geometry.deferredViews.formUnion(ids)
        if let quiet = geometry.quiet { quiet.moved = true } else { geometry.quiet = GeometryQuiet(self) }
    }

    /// PROTO (UIKit-first measurement): the deferred view IDs for the next full batch or final
    /// synchronization, removed from the pending set with their quiet-frame clock stopped
    func takeDeferredGeometry() -> [UInt32] {
        geometry.quiet?.stop()
        geometry.quiet = nil
        defer { geometry.deferredViews.removeAll(keepingCapacity: true) }
        return Array(geometry.deferredViews)
    }

    /// PROTO (UIKit-first measurement): the synchronization a batch runs after its operations,
    /// for every box moved by geometry-only batches once the edge has stopped
    func finishDeferredGeometry() {
        let ids = takeDeferredGeometry()
        guard !ids.isEmpty else { return }
        let changed = touchedAndAbove(ids)
        swipeActions.sync(changed: changed, reshaped: []); rowCells.sync(changed: changed)
        groupedLists?.sync(changed: changed); reachSafeEdges(); reachPageEdges()
        segments.sync()
        controls.sync(contents: false, changed: changed)
        menus.sync()
        glassGroups.reconcile()
        positionContexts()
        navigation.followAuthoredTablist()
        syncAccessibility(changed: changed)
    }

    /// PROTO (UIKit-first measurement): the root's words for Duo Lab's Edges screen.
    /// `data-safe-area-mode` picks the mode and reports the routes' covers again to lay them out
    /// at their live or resting top; a new `data-safe-area-run` moves the shown route's top edge
    /// as a bar would (`EdgeRun`).
    func applyProtoMode(_ root: NodeView?) {
        let dataset = root?.props["dataset"] ?? ""
        guard dataset != geometry.dataset else { return }
        geometry.dataset = dataset
        guard let words = try? JSONSerialization.jsonObject(with: Data(dataset.utf8)) as? [String: Any] else { return }
        if let word = words["safe-area-mode"] as? String, let mode = ExactSession.ProtoMode(rawValue: word), mode != ExactSession.protoMode {
            ExactSession.protoMode = mode
            // PROTO (rule 4): discard interpolated frames when live layout moves the boxes
            if mode != .resting { edgeFrames = [:]; for c in navigation.controllers.values { c.placedEdge = nil } }
            NSLog("PROTO mode %@", mode.rawValue)
            afterBatch { [weak self] in self?.navigation.reportCovers() }
        }
        let run = words["safe-area-run"] as? String ?? ""
        if run != geometry.lastRun {
            let previous = geometry.lastRun
            geometry.lastRun = run
            guard previous != nil, let route = navigation.controllers.values.first(where: {
                $0.viewIfLoaded?.window != nil && $0.navigationController?.topViewController === $0
            }) else { return }
            geometry.run?.finish()
            geometry.run = EdgeRun(route) { [weak self] in self?.geometry.run = nil }
        }
    }
}

/// PROTO (UIKit-first measurement): the scheduled motion of a route's top edge through its
/// `additionalSafeAreaInsets.top`, from its own inset plus 106 points to its own inset and back,
/// over a second each way, twice. UIKit changes the safe area and calls
/// `viewSafeAreaInsetsDidChange` on every frame, as for a bar; a programmatic scroll does not
/// collapse a large title, and a drag cannot keep a schedule on a loaded machine. The run holds
/// the edge 106 points low for half a second first. In the resting mode that held inset counts
/// in the resting top, so the sweep moves the edge up from its resting place as a collapsing
/// title does. Completion or cancellation restores the route's own inset; start and end delimit its log.
final class EdgeRun: NSObject {
    private weak var route: UIViewController?
    private let originalTop: CGFloat
    private let done: () -> Void
    private var link: CADisplayLink?
    private var started: CFTimeInterval = 0
    private static let hold: CFTimeInterval = 0.5, leg: CFTimeInterval = 1, travel: CGFloat = 106
    private static let legs: [CGFloat] = [0, travel, 0, travel]

    init(_ route: UIViewController, done: @escaping () -> Void) {
        self.route = route
        originalTop = route.additionalSafeAreaInsets.top
        self.done = done
        super.init()
        NSLog("PROTO run start %@", ExactSession.protoMode.rawValue)
        route.additionalSafeAreaInsets.top = originalTop + Self.travel
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    /// PROTO (UIKit-first measurement): this route's held inset while the run is active in the
    /// resting mode, or 0 for another route, an ended run or a live mode
    func restingInset(for route: UIViewController) -> CGFloat {
        link != nil && self.route === route && ExactSession.protoMode == .resting ? originalTop + Self.travel : 0
    }

    @objc private func tick(_ link: CADisplayLink) {
        guard let route else { return finish() }
        if started == 0 { started = link.timestamp }
        let elapsed = link.timestamp - started - Self.hold
        guard elapsed >= 0 else { return }
        let index = Int(elapsed / Self.leg)
        guard index < Self.legs.count else { return finish() }
        let from = index == 0 ? Self.travel : Self.legs[index - 1], to = Self.legs[index]
        let progress = CGFloat((elapsed - Double(index) * Self.leg) / Self.leg)
        let eased = progress * progress * (3 - 2 * progress)
        route.additionalSafeAreaInsets.top = originalTop + from + (to - from) * eased
    }

    func finish() {
        guard let activeLink = link else { return }
        link = nil
        activeLink.invalidate()
        route?.additionalSafeAreaInsets.top = originalTop
        NSLog("PROTO run end %@", ExactSession.protoMode.rawValue)
        done()
    }
}

/// PROTO (UIKit-first measurement): the frame clock for deferred work, stopped on the first
/// frame without a geometry-only batch (`Presenter.deferGeometry`)
final class GeometryQuiet: NSObject {
    private weak var presenter: Presenter?
    /// PROTO (UIKit-first measurement): whether a geometry-only batch arrived since the last tick
    var moved = false
    private var link: CADisplayLink?

    init(_ presenter: Presenter) {
        self.presenter = presenter
        super.init()
        let link = CADisplayLink(target: self, selector: #selector(tick))
        link.add(to: .main, forMode: .common)
        self.link = link
    }

    @objc private func tick() {
        if moved { moved = false; return }
        guard let presenter else { stop(); return }
        presenter.finishDeferredGeometry()
    }

    func stop() {
        link?.invalidate()
        link = nil
    }
}
#endif
