// @ref LLP 1055.001 — a box's CSS animations (its transform key paths and
// background colour) on its own layer.
import ExactKit
import Foundation
import QuartzCore

/// A presenter's box animations, by view id: the `animations` ops, a box
/// hidden by `visibility`, a box flying scaled, the agent clock's re-seek,
/// and cleanup when a view goes.
final class BoxAnimations: BoxAnimating {
    private var boxSpecs: [UInt32: (layer: CALayer, specs: [CssAnimation])] = [:]
    private var boxInstalled: [UInt32: [String: Int]] = [:]
    /// The layer each box paints its background colour on, where not its own (`setFill`).
    private var fills: [UInt32: Redirect] = [:]
    /// The layer each box plays its translate and scale on, where not its own (`setTransformProxy`).
    private var proxies: [UInt32: Redirect] = [:]
    private var hiddenBoxes: Set<UInt32> = []
    private var flyingBoxes: Set<UInt32> = []
    private var seeked: Double?

    func animations(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, clock: Double?) {
        guard let layer else { return }
        let specs = CssAnimation.list(payload["specs"])
        boxSpecs[id] = specs.isEmpty ? nil : (layer, specs)
        play(id, visibleBoxSpecs(id, specs), layer: layer, clock: clock)
    }

    /// `specs` on the box's layer, its background colour's on its fill's (`setFill`), its translate and scale on its
    /// proxy (`setTransformProxy`).
    private func play(_ id: UInt32, _ specs: [CssAnimation], layer: CALayer, clock: Double?) {
        let filled = fills[id] != nil
        // Keep all transform animations together on the box's layer when any unproxied transform (rotation, negative scale) runs.
        let proxied = proxies[id] != nil && Self.canProxy(specs)
        let own = specs.filter { !(filled && Self.filled($0.key)) && !(proxied && Self.proxied($0.key)) }
        CssAnimations.apply(own, to: layer, clock: clock, installed: &boxInstalled[id, default: [:]])
        fills[id]?.play(specs.filter { Self.filled($0.key) }, clock: clock)
        proxies[id]?.play(proxied ? specs.filter { Self.proxied($0.key) } : [], clock: clock)
    }

    private static func filled(_ key: String) -> Bool { key == "backgroundColor" }
    private static func proxied(_ key: String) -> Bool {
        key.hasPrefix("transform.translation.") || key.hasPrefix("transform.scale.")
    }
    private static func canProxy(_ specs: [CssAnimation]) -> Bool {
        !specs.contains { spec in
            guard spec.key.hasPrefix("transform.") else { return false }
            if !proxied(spec.key) { return true }
            if spec.key.hasPrefix("transform.scale.") {
                return (spec.values as? [NSNumber])?.contains { $0.doubleValue < 0 } == true
            }
            return false
        }
    }

    func setFill(_ id: UInt32, _ fill: CALayer?, clock: Double?) { redirect(id, \.fills, to: fill, clock: clock) }
    func setTransformProxy(_ id: UInt32, _ proxy: CALayer?, clock: Double?) { redirect(id, \.proxies, to: proxy, clock: clock) }

    /// Points `id`'s redirect in `table` at `layer` (nil for the box's own), and moves what played there.
    private func redirect(_ id: UInt32, _ table: ReferenceWritableKeyPath<BoxAnimations, [UInt32: Redirect]>, to layer: CALayer?, clock: Double?) {
        guard self[keyPath: table][id]?.layer !== layer else { return }
        self[keyPath: table][id]?.clear()
        self[keyPath: table][id] = layer.map { Redirect(layer: $0) }
        if let entry = boxSpecs[id] { play(id, visibleBoxSpecs(id, entry.specs), layer: entry.layer, clock: clock) }
    }

    private func visibleBoxSpecs(_ id: UInt32, _ specs: [CssAnimation]) -> [CssAnimation] {
        let hidden = hiddenBoxes.contains(id), flying = flyingBoxes.contains(id)
        guard hidden || flying else { return specs }
        return specs.filter { !(hidden && $0.key == "backgroundColor") && !(flying && $0.key.hasPrefix("transform.")) }
    }

    func boxVisibility(_ id: UInt32, hidden: Bool, clock: Double?) {
        if hidden { hiddenBoxes.insert(id) } else { hiddenBoxes.remove(id) }
        guard let entry = boxSpecs[id] else { return }
        play(id, visibleBoxSpecs(id, entry.specs), layer: entry.layer, clock: clock)
    }

    /// An agent moved the clock: every held animation shows the new instant.
    func seek(clock: Double?) {
        guard clock != seeked else { return }
        seeked = clock
        for (id, entry) in boxSpecs { play(id, visibleBoxSpecs(id, entry.specs), layer: entry.layer, clock: clock) }
    }

    /// Whether `id`'s view holds CSS animations, which are its layer's.
    func animatesBox(_ id: UInt32) -> Bool { boxSpecs[id] != nil }

    /// Whether `id`'s layer plays transform animations, which replace its
    /// translate, rotate and scale (`NodeView.applyTransform`). A box flying
    /// scaled in its clip (LLP 1013.000 D4.4) plays none: the flight's scale
    /// is its whole transform, and they go back on, in phase, when it lands.
    func playsTransform(_ id: UInt32, flying: Bool, clock: Double?) -> Bool {
        if flying != flyingBoxes.contains(id) {
            if flying { flyingBoxes.insert(id) } else { flyingBoxes.remove(id) }
            if let entry = boxSpecs[id] { play(id, visibleBoxSpecs(id, entry.specs), layer: entry.layer, clock: clock) }
        }
        let specs = boxSpecs[id]?.specs ?? []
        let proxied = proxies[id] != nil && Self.canProxy(specs)
        return !flying && specs.contains { $0.key.hasPrefix("transform.") && !(proxied && Self.proxied($0.key)) }
    }

    func forget(_ id: UInt32) {
        if let entry = boxSpecs.removeValue(forKey: id) { CssAnimations.apply([], to: entry.layer, clock: nil, installed: &boxInstalled[id, default: [:]]) }
        fills.removeValue(forKey: id)?.clear()
        proxies.removeValue(forKey: id)?.clear()
        boxInstalled.removeValue(forKey: id)
        hiddenBoxes.remove(id)
        flyingBoxes.remove(id)
    }
}

/// A layer other than the box's own that some of its animations play on, and what is installed there.
private struct Redirect {
    let layer: CALayer
    var installed: [String: Int] = [:]

    mutating func play(_ specs: [CssAnimation], clock: Double?) {
        CssAnimations.apply(specs, to: layer, clock: clock, installed: &installed)
    }

    /// Takes off what plays on the layer.
    func clear() {
        var installed = installed
        CssAnimations.apply([], to: layer, clock: nil, installed: &installed)
    }
}
