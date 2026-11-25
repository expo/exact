// @ref LLP 1013.004 D1, D3 — shared elements across a route change, alongside
// UIKit's own transition: its push, pop, edge swipe or presentation, not its zoom.
//
// The names each route holds once fly as in-page flights do (FlightsIOS):
// the arriver, live, lifted into a layer in the transition's container, the
// leaver hidden. The progress is the transition's own: a probe view whose
// opacity animates alongside it, read before each Core Animation commit, so a
// flight takes UIKit's curve, follows a swipe's finger and reverses with its
// cancel. The destination is where its slot is drawn in that frame, so a
// flight lands with its page. The flights land in the transition's completion.
#if os(iOS)
import UIKit
import ExactKit

extension Presenter {
    private var holds: [ObjectIdentifier: (view: NodeView, source: FlightSource, lines: CGRect)] {
        get { (flightHost as? BuiltInFlights)?.holds ?? [:] }
        set { (flightHost as? BuiltInFlights)?.holds = newValue }
    }

    func holdFlights(leaving: UIView, arriving: UIView, zoom: String?) {
        guard !DisplayPreferences.reducedMotion, zoom == nil else { return }
        let pairs = Self.routePairs(leaving: named(in: leaving), arriving: named(in: arriving))
        for pair in pairs { holds[ObjectIdentifier(pair.from)] = (pair.from, flightSource(pair.from), pair.from.flightLines) }
        // An unanimated pop takes its route out of the window at once and flies nothing: let its holds go.
        DispatchQueue.main.async { [weak self] in self?.holds = self?.holds.filter { $0.value.view.window != nil } ?? [:] }
    }

    /// The pairs a route change flies: each name `leaving` holds once and
    /// `arriving` holds once (LLP 1013.004 D1), in the order the nodes were made.
    static func routePairs(leaving: [NodeView], arriving: [NodeView]) -> [(from: NodeView, to: NodeView)] {
        func once(_ nodes: [NodeView]) -> [String: NodeView] {
            var seen: [String: NodeView?] = [:]
            for node in nodes { if let name = node.props["sharedElement"], !name.isEmpty { seen[name] = seen[name] == nil ? node : .some(nil) } }
            return seen.compactMapValues { $0 }
        }
        let to = once(arriving)
        return once(leaving).compactMap { name, from in to[name].map { (from, $0) } }.sorted { $0.from.id < $1.from.id }
    }

    /// The named nodes `route` draws: none hidden or transparent up to it, none empty. A route a Close or
    /// Back destroyed keeps only what `holdFlights` held.
    private func named(in route: UIView) -> [NodeView] {
        var candidates: [ObjectIdentifier: NodeView] = [:]
        for node in views.values where node.props["sharedElement"]?.isEmpty == false { candidates[ObjectIdentifier(node)] = node }
        for held in holds.values { candidates[ObjectIdentifier(held.view)] = held.view }
        return candidates.values.filter { node in
            guard node.isDescendant(of: route), !node.bounds.isEmpty else { return false }
            return !sequence(first: node as UIView, next: { $0 === route ? nil : $0.superview }).contains {
                $0 !== route && ($0.isHidden || $0.alpha < 0.01)
            }
        }
    }

    func crossFlights(_ coordinator: UIViewControllerTransitionCoordinator, leaving: UIView, arriving: UIView, zoom: String?,
                      picture: UIView? = nil) -> Bool {
        // An unanimated change flies nothing, what it held included. Nor does a zoom: its card moves on its
        // own spring, and follows a drag to dismiss, where neither the coordinator's clock nor public API
        // reads it, so the names stay in their routes and the destination's ride the card, as in a
        // hand-built screen.
        guard coordinator.isAnimated, !DisplayPreferences.reducedMotion, zoom == nil else { holds = [:]; return false }
        let pairs = Self.routePairs(leaving: named(in: leaving), arriving: named(in: arriving))
            .filter { $0.from.window != nil }
        let sources = holds
        // A hold lasts until the transition that flies it, or until its route leaves the window.
        holds = holds.filter { held in held.value.view.window != nil && !pairs.contains { $0.from === held.value.view } }
        guard !pairs.isEmpty else { return false }
        // An in-commit handoff of a name the routes pair yields to the pair: the name flies once (LLP 1013.004 D1).
        for pair in pairs { if let f = flights[pair.to.id] { landFlight(f) } }
        // Hidden now, and cut out of a picture of the route taken before: the flight draws it.
        let held = pairs.map {
            (flight: Flight(id: $0.to.id, source: sources[ObjectIdentifier($0.from)]?.source ?? flightSource($0.from)), leaver: $0.from, alpha: $0.from.alpha)
        }
        for (h, pair) in zip(held, pairs) {
            h.flight.crossing = true
            flights[h.flight.id] = h.flight
            if pair.to.kind != "image" {
                // A bitmap: a snapshot view goes on following the leaver as its route moves.
                let box = h.leaver.bounds
                let picture = UIImageView(image: UIGraphicsImageRenderer(bounds: box).image { _ in h.leaver.drawHierarchy(in: box, afterScreenUpdates: false) })
                h.flight.fade = (picture, sources[ObjectIdentifier(h.leaver)]?.lines ?? h.leaver.flightLines)
            }
        }
        UIView.performWithoutAnimation { for h in held { h.leaver.alpha = 0 } }
        if let picture {
            let cut = CAShapeLayer()
            // Their union: a name in another, or two that overlap, leave nothing in between.
            let holes = CGMutablePath()
            for h in held { holes.addRect(picture.convert(h.flight.source.rect, from: nil)) }
            cut.path = CGPath(rect: picture.bounds, transform: nil).subtracting(holes)
            picture.layer.mask = cut
        }
        // What the leaver shows when it lands: its own alpha, or one a batch set while it flew.
        var alphas = held.map(\.alpha)
        // Starts at 0, and the block animates it to 1.
        let probe = UIView()
        probe.isUserInteractionEnabled = false
        probe.alpha = 0
        let owner = NSObject()
        var stop = { FrameClock.shared.drop(owner) }
        let queued = coordinator.animate(alongsideTransition: { [weak self] context in
            guard let self else { return }
            UIView.performWithoutAnimation {
                context.containerView.addSubview(probe)
                for h in held where self.flights[h.flight.id] === h.flight { self.liftFlight(h.flight, in: context.containerView) }
                // One that could not lift shows where it is.
                for h in held where self.flights[h.flight.id] !== h.flight { h.leaver.alpha = h.alpha }
            }
            probe.alpha = 1
            let tick = { [weak self, weak probe] in
                guard let self, let probe else { return }
                // UIKit drives the probe's linear animation paused, setting its time each frame before this runs;
                // the presentation shows that time only after the frame commits, a frame behind the routes.
                // Before UIKit drives it, the presentation: the start.
                let driven = probe.layer.animationKeys()?.lazy.compactMap { probe.layer.animation(forKey: $0) as? CABasicAnimation }
                    .first { $0.keyPath == "opacity" && $0.speed == 0 }
                let p = min(1, max(0, driven.map { CGFloat($0.timeOffset / max($0.duration, 0.001)) } ?? CGFloat(probe.layer.presentation()?.opacity ?? 0)))
                // Hidden each frame: a batch that applies the leaver's style shows it again.
                for (i, h) in held.enumerated() where flights[h.flight.id] === h.flight {
                    if h.leaver.alpha != 0 { alphas[i] = h.leaver.alpha }
                    h.leaver.alpha = 0
                    h.flight.progress = p
                    showFlight(h.flight)
                }
                // Text paints where it shows: what flies, as it flies.
                paintVisibleText()
            }
            UIView.performWithoutAnimation(tick)
            // Each frame after UIKit moves its transition, which it does after the display link's callbacks:
            // read there, the probe is a frame behind the routes.
            #if !targetEnvironment(macCatalyst)
            if #available(iOS 18.0, *) {
                let link = UIUpdateLink(view: probe)
                link.addAction(to: .beforeCATransactionCommit) { _, _ in tick() }
                link.requiresContinuousUpdates = true
                link.preferredFrameRateRange = FrameClock.full(on: probe.window?.screen)
                link.isEnabled = true
                stop = { link.isEnabled = false }
                return
            }
            #endif
            FrameClock.shared.want(owner, .stage, rate: FrameClock.full(on: probe.window?.screen)) { _ in tick() }
        }, completion: { [weak self] _ in
            stop()
            probe.removeFromSuperview()
            for (h, alpha) in zip(held, alphas) {
                if let self, flights[h.flight.id] === h.flight { landFlight(h.flight) }
                // Not 0: a batch set it since the last frame.
                h.leaver.alpha = h.leaver.alpha != 0 ? h.leaver.alpha : alpha
            }
            self?.paintVisibleText()
        })
        guard queued else {
            picture?.layer.mask = nil
            for h in held { flights.removeValue(forKey: h.flight.id); h.leaver.alpha = h.alpha }
            return false
        }
        return true
    }
}
#endif
