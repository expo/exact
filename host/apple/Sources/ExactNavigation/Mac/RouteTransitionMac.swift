// @ref LLP 1013.001 D1, D2, D6 — a route's own entrance and exit on AppKit,
// whose route changes otherwise cut (the hide-and-inert rule, LLP 1038 D6).
// On a push of one route that declares an entrance, the route beneath stays
// shown and both stay inert while the arriving route plays it on its layer;
// on a pop of one that declares an exit, the route that left stays in the
// window as an exit animation's view does (LLP 1063, `PresenceMac.swift`),
// over the route it reveals, and plays it. When the part ends, or a second
// change comes, the held route takes its hiding and the leaving route goes.
#if os(macOS)
import AppKit
import ExactKit

final class RouteTransitions {
    unowned let presenter: Presenter
    init(presenter: Presenter) { self.presenter = presenter }

    /// The route a push holds shown beneath the route it brings in, and
    /// that route, until its entrance ends.
    private(set) var held: (beneath: UInt32, arriving: UInt32)?
    /// The first root's top route as last shown, and the exit it declares.
    private var top: (id: UInt32, exit: RoutePart?)?
    /// The top route a batch destroyed, kept for its exit.
    private var leaving: (id: UInt32, exit: RoutePart)?
    /// The kept route playing its exit, which the stack still holds and
    /// the gate leaves shown.
    private(set) var exiting: UInt32?
    /// The playing part's view and what ends it, told whether it ended on
    /// its own (and not in a sync, which reads `held` itself).
    private var playing: (view: NSView, end: (Bool) -> Void)?
    private var generation = 0

    /// Whether the destroyed route `id` stays for its exit: the top route,
    /// when it declares one that plays. `changed` plays it or, for a change
    /// that is not a pop, lets it go.
    func keeps(_ id: UInt32) -> Bool {
        // Inside `startViewTransition()`'s cross-fade no route plays a part of its own (LLP 1013.003 D3).
        guard !presenter.navigation.crossFading, let top, top.id == id, let exit = top.exit, exit.plays else { return false }
        finish()
        leaving = (id, exit)
        return true
    }

    /// In a sync of the first root, before it gates its routes: play the
    /// part the change from `before` to `after` (the stack's keys through
    /// the route it shows) asks for, and keep the exit of the route shown.
    func changed(from before: [String], to after: [String], stack: [NodeView], at: Int) {
        let kept = leaving
        leaving = nil
        if before != after { finish() }
        if let kept {
            let pop = before.count == after.count + 1 && before.starts(with: after)
            if pop { exiting = kept.id }
            play(pop ? kept.exit : nil, on: presenter.leaving[kept.id]?.view) { [weak self] _ in
                if self?.exiting == kept.id { self?.exiting = nil }
                _ = self?.presenter.endExit(kept.id)
            }
        }
        let route = stack.indices.contains(at) ? stack[at] : nil
        if !presenter.navigation.crossFading, after.count == before.count + 1, after.starts(with: before), at > 0, let route,
           let part = presenter.session?.runtime.routePart(route.id, exit: false, size: route.bounds.size), part.plays {
            held = (stack[at - 1].id, route.id)
            play(part, on: route) { [weak self] alone in
                self?.held = nil
                if alone { self?.presenter.navigationHost.resync() }
            }
        }
        top = route.map { ($0.id, at > 0 ? presenter.session?.runtime.routePart($0.id, exit: true, size: $0.bounds.size) : nil) }
    }

    private func play(_ part: RoutePart?, on view: NSView?, end: @escaping (Bool) -> Void) {
        guard let part, let view, let layer = view.layer else { return end(false) }
        generation &+= 1
        let mine = generation
        playing = (view, end)
        CATransaction.begin()
        CATransaction.setCompletionBlock { [weak self, weak view] in
            guard let self, generation == mine else { return }
            playing = nil
            view?.layer?.removeAnimation(forKey: Self.key)
            end(true)
        }
        layer.add(part.animation(about: CGPoint(x: view.bounds.midX, y: view.bounds.midY)), forKey: Self.key)
        CATransaction.commit()
    }

    /// End the playing part at its end (D6).
    private func finish() {
        guard let playing else { return }
        generation &+= 1
        self.playing = nil
        playing.view.layer?.removeAnimation(forKey: Self.key)
        playing.end(false)
    }

    private static let key = "exact.route-transition"
}
#endif
