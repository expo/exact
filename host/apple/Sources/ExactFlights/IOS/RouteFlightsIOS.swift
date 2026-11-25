// @ref LLP 1013.004 D1, D3 — shared elements across a route change, alongside
// UIKit's own transition: its push, pop, edge swipe or presentation, not its zoom.
//
// The names each route holds once fly as in-page flights do (FlightsIOS):
// the arriver, live, lifted into a layer in the transition's container, the
// leaver hidden. The progress is the transition's own: a probe view whose
// opacity animates alongside it, read each frame from its presentation layer,
// so a flight takes UIKit's curve, follows a swipe's finger and reverses with
// its cancel. The destination is read from the model, which UIKit has already
// moved to where the transition ends. The flights land in its completion.
#if os(iOS)
import UIKit
import ExactKit

extension Presenter {
    private var holds: [ObjectIdentifier: (view: NodeView, source: FlightSource)] {
        get { (flightHost as? BuiltInFlights)?.holds ?? [:] }
        set { (flightHost as? BuiltInFlights)?.holds = newValue }
    }

    func holdFlights(leaving: UIView, arriving: UIView, zoom: String?) {
        guard !DisplayPreferences.reducedMotion, zoom == nil else { return }
        let pairs = Self.routePairs(leaving: named(in: leaving), arriving: named(in: arriving))
        for pair in pairs { holds[ObjectIdentifier(pair.from)] = (pair.from, flightSource(pair.from)) }
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

    func crossFlights(_ coordinator: UIViewControllerTransitionCoordinator, leaving: UIView, arriving: UIView, zoom: String?) -> Bool {
        // An unanimated change flies nothing, what it held included. Nor does a zoom: its card moves on its
        // own spring, and follows a drag to dismiss, where neither the coordinator's clock nor public API
        // reads it, so the names stay in their routes and the destination's ride the card, as in a
        // hand-built screen.
        guard coordinator.isAnimated, !DisplayPreferences.reducedMotion, zoom == nil else { holds = [:]; return false }
        let pairs = Self.routePairs(leaving: named(in: leaving), arriving: named(in: arriving))
            .filter { $0.from.window != nil && flights[$0.to.id] == nil }
        let sources = holds
        // A hold lasts until the transition that flies it, or until its route leaves the window.
        holds = holds.filter { held in held.value.view.window != nil && !pairs.contains { $0.from === held.value.view } }
        guard !pairs.isEmpty else { return false }
        // Hidden now, before a frozen route is pictured again without it.
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
                h.flight.fade = (picture, h.leaver.contentBox())
            }
        }
        UIView.performWithoutAnimation { for h in held { h.leaver.alpha = 0 } }
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
            var last: CGFloat?
            let tick = { [weak self, weak probe] (paced: Bool) in
                guard let self, let probe else { return }
                // Before the transaction commits there is no presentation yet: the start.
                let read = CGFloat(probe.layer.presentation()?.opacity ?? 0)
                var p = read
                // Unpinned, UIKit's own layers show a frame further on than the probe reads (they are paced on
                // the render server), so the flight takes the frame's step once more to keep with them.
                if paced, let l = last, !context.isInteractive { p = min(1, max(0, 2 * read - l)) }
                last = paced ? read : nil
                // Hidden each frame: a batch that applies the leaver's style shows it again.
                for h in held where flights[h.flight.id] === h.flight { h.leaver.alpha = 0; h.flight.progress = p; showFlight(h.flight) }
                // Text paints where it shows: what flies, as it flies.
                paintVisibleText()
            }
            UIView.performWithoutAnimation { tick(false) }
            // Each frame after UIKit moves its transition, which it does after the display link's callbacks:
            // read there, the probe is a frame behind the routes.
            #if !targetEnvironment(macCatalyst)
            if #available(iOS 18.0, *) {
                let link = UIUpdateLink(view: probe)
                link.addAction(to: .beforeCATransactionCommit) { _, _ in tick(true) }
                link.requiresContinuousUpdates = true
                link.preferredFrameRateRange = FrameClock.full(on: probe.window?.screen)
                link.isEnabled = true
                stop = { link.isEnabled = false }
                return
            }
            #endif
            FrameClock.shared.want(owner, .stage, rate: FrameClock.full(on: probe.window?.screen)) { _ in tick(false) }
        }, completion: { [weak self] _ in
            stop()
            probe.removeFromSuperview()
            for h in held {
                if let self, flights[h.flight.id] === h.flight { landFlight(h.flight) }
                h.leaver.alpha = h.alpha
            }
            self?.paintVisibleText()
        })
        guard queued else {
            for h in held { flights.removeValue(forKey: h.flight.id); h.leaver.alpha = h.alpha }
            return false
        }
        return true
    }
}
#endif
