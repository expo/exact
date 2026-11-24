// @ref LLP 1013.004 D1, D3 — shared elements across a route change, alongside
// UIKit's own transition: its push, pop, edge swipe, zoom or presentation.
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
    private var holds: [ObjectIdentifier: NodeView] {
        get { (flightHost as? BuiltInFlights)?.holds ?? [:] }
        set { (flightHost as? BuiltInFlights)?.holds = newValue }
    }

    func holdFlights(leaving: UIView, arriving: UIView, zoom: String?) {
        guard !DisplayPreferences.reducedMotion else { return }
        let pairs = Self.routePairs(leaving: named(in: leaving, excluding: zoom), arriving: named(in: arriving, excluding: zoom))
        for pair in pairs { holds[ObjectIdentifier(pair.from)] = pair.from }
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

    /// The named nodes `route` draws: none hidden or transparent up to it, none empty, none the zoom's
    /// own source or landing. A route a Close or Back destroyed keeps only what `holdFlights` held.
    private func named(in route: UIView, excluding zoom: String?) -> [NodeView] {
        var candidates: [ObjectIdentifier: NodeView] = [:]
        for node in views.values where node.props["sharedElement"]?.isEmpty == false || zoom != nil && node.props["id"] == zoom {
            candidates[ObjectIdentifier(node)] = node
        }
        for held in holds.values { candidates[ObjectIdentifier(held)] = held }
        let inside = candidates.values.filter { $0.isDescendant(of: route) }
        let zoomed = inside.filter { zoom != nil && $0.props["id"] == zoom }
        return inside.filter { node in
            guard node.props["sharedElement"]?.isEmpty == false, !node.bounds.isEmpty,
                  !zoomed.contains(where: { node.isDescendant(of: $0) }) else { return false }
            // The route's own view aside: a zoomed one is transparent until the zoom takes it (ModalIOS).
            return !sequence(first: node as UIView, next: { $0 === route ? nil : $0.superview }).contains {
                $0 !== route && ($0.isHidden || $0.alpha < 0.01)
            }
        }
    }

    func crossFlights(_ coordinator: UIViewControllerTransitionCoordinator, leaving: UIView, arriving: UIView, zoom: String?) -> Bool {
        // An unanimated change flies nothing, what it held included.
        guard coordinator.isAnimated, !DisplayPreferences.reducedMotion else { holds = [:]; return false }
        let pairs = Self.routePairs(leaving: named(in: leaving, excluding: zoom), arriving: named(in: arriving, excluding: zoom))
            .filter { $0.from.window != nil && flights[$0.to.id] == nil }
        // A hold lasts until the transition that flies it, or until its route leaves the window.
        holds = holds.filter { held in held.value.window != nil && !pairs.contains { $0.from === held.value } }
        guard !pairs.isEmpty else { return false }
        // Hidden now, before UIKit pictures the routes for a zoom.
        let held = pairs.map { (flight: Flight(id: $0.to.id, source: flightSource($0.from)), leaver: $0.from, alpha: $0.from.alpha) }
        for h in held { h.flight.crossing = true; flights[h.flight.id] = h.flight }
        UIView.performWithoutAnimation { for h in held { h.leaver.alpha = 0 } }
        // Starts at 0, and the block animates it to 1.
        let probe = UIView()
        probe.isUserInteractionEnabled = false
        probe.alpha = 0
        let owner = NSObject()
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
                // Before the transaction commits there is no presentation yet: the start.
                let p = CGFloat(probe.layer.presentation()?.opacity ?? 0)
                // Hidden each frame: a batch that applies the leaver's style shows it again.
                for h in held where flights[h.flight.id] === h.flight { h.leaver.alpha = 0; h.flight.progress = p; showFlight(h.flight) }
                // Not shown beside a leaver still on screen: a zoom's first frame is the route as last drawn.
                held.lazy.compactMap { $0.flight.container }.first?.isHidden = held.contains {
                    self.flights[$0.flight.id] === $0.flight && $0.leaver.window != nil && ($0.leaver.layer.presentation()?.opacity ?? 0) > 0.01
                }
                // Text paints where it shows: what flies, as it flies.
                paintVisibleText()
            }
            UIView.performWithoutAnimation(tick)
            FrameClock.shared.want(owner, .stage, rate: FrameClock.full(on: probe.window?.screen)) { _ in tick() }
        }, completion: { [weak self] _ in
            FrameClock.shared.drop(owner)
            probe.removeFromSuperview()
            held.lazy.compactMap { $0.flight.container }.first?.isHidden = false
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
