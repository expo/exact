// @ref LLP 1055.001 — a box's CSS animations (its transform key paths and
// background colour) on its own layer. They are the core's, whatever SVG
// the app links (LLP 1047.001 D10).
import Foundation
import QuartzCore

/// A presenter's box animations, by view id: the `animations` ops, a box
/// hidden by `visibility`, a box flying scaled, the agent clock's re-seek,
/// and cleanup when a view goes.
final class BoxAnimations {
    private var boxSpecs: [UInt32: (layer: CALayer, specs: [CssAnimation])] = [:]
    private var boxInstalled: [UInt32: [String: Int]] = [:]
    private var hiddenBoxes: Set<UInt32> = []
    private var flyingBoxes: Set<UInt32> = []
    private var seeked: Double?

    func animations(_ id: UInt32, _ payload: [String: Any], layer: CALayer?, clock: Double?) {
        guard let layer else { return }
        let specs = CssAnimation.list(payload["specs"])
        boxSpecs[id] = specs.isEmpty ? nil : (layer, specs)
        CssAnimations.apply(visibleBoxSpecs(id, specs), to: layer, clock: clock, installed: &boxInstalled[id, default: [:]])
    }

    private func visibleBoxSpecs(_ id: UInt32, _ specs: [CssAnimation]) -> [CssAnimation] {
        let hidden = hiddenBoxes.contains(id), flying = flyingBoxes.contains(id)
        guard hidden || flying else { return specs }
        return specs.filter { !(hidden && $0.key == "backgroundColor") && !(flying && $0.key.hasPrefix("transform.")) }
    }

    func boxVisibility(_ id: UInt32, hidden: Bool, clock: Double?) {
        if hidden { hiddenBoxes.insert(id) } else { hiddenBoxes.remove(id) }
        guard let entry = boxSpecs[id] else { return }
        CssAnimations.apply(visibleBoxSpecs(id, entry.specs), to: entry.layer, clock: clock, installed: &boxInstalled[id, default: [:]])
    }

    /// An agent moved the clock: every held animation shows the new instant.
    func seek(clock: Double?) {
        guard clock != seeked else { return }
        seeked = clock
        for (id, entry) in boxSpecs { CssAnimations.apply(visibleBoxSpecs(id, entry.specs), to: entry.layer, clock: clock, installed: &boxInstalled[id, default: [:]]) }
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
            if let entry = boxSpecs[id] { CssAnimations.apply(visibleBoxSpecs(id, entry.specs), to: entry.layer, clock: clock, installed: &boxInstalled[id, default: [:]]) }
        }
        return !flying && boxSpecs[id]?.specs.contains { $0.key.hasPrefix("transform.") } == true
    }

    func forget(_ id: UInt32) {
        if let entry = boxSpecs.removeValue(forKey: id) { CssAnimations.apply([], to: entry.layer, clock: nil, installed: &boxInstalled[id, default: [:]]) }
        boxInstalled.removeValue(forKey: id)
        hiddenBoxes.remove(id)
        flyingBoxes.remove(id)
    }
}
