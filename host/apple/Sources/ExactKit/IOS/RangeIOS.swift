// @ref LLP 1069.001 D5 — `input type="range"` is a `UISlider`: HTML's
// `input` as the thumb moves, `change` as the finger lifts, each value
// clamped and snapped to `step` as HTML sanitizes it, and the thumb with it,
// as a browser's moves from step to step (`step="any"` glides) with no tick
// marks, as HTML draws none without a `list` and a stepped SwiftUI `Slider`
// none on iOS 27; VoiceOver moves it a step at a time. The thumb stays where
// the person put it until the bound value changes (D4, amended).
#if os(iOS)
import UIKit

extension ControlHost {
    func makeRange() -> UIControl {
        let slider = RangeSlider()
        slider.adjust = { [weak self] slider, direction in self?.adjustRange(slider, by: direction) }
        slider.isContinuous = true
        slider.addTarget(self, action: #selector(rangeMoved(_:)), for: .valueChanged)
        slider.addTarget(self, action: #selector(rangeReleased(_:)), for: [.touchUpInside, .touchUpOutside, .touchCancel])
        return slider
    }

    func configureRange(_ slider: RangeSlider, _ owner: NodeView) {
        let range = RangeSpec(owner.props)
        // Read before new bounds clamp the thumb's `Float`
        var value = slider.exact
        assign(slider, \.minimumValue, Float(range.min))
        assign(slider, \.maximumValue, Float(range.max))
        // The bound value is written when it changes, as the web build writes
        // an input's `value` (LLP 1069.001 D4, amended 2026-10-04); new bounds
        // or a new step sanitize the value again, as HTML's do.
        let bound = owner.props["value"] ?? ""
        if !slider.isTracking, appliedRange[owner.id] != bound {
            appliedRange[owner.id] = bound
            value = range.shown(owner.props)
        }
        slider.exact = range.sanitize(value)
        assign(slider, \.accessibilityValue, RangeSpec.format(slider.exact))
    }

    /// The value the slider reports: its position, snapped as HTML would.
    private func reported(_ slider: RangeSlider) -> (UInt32, String)? {
        let id = UInt32(slider.tag)
        guard let owner = presenter.views[id] else { return nil }
        return (id, RangeSpec.format(RangeSpec(owner.props).sanitize(slider.exact)))
    }

    @objc func rangeMoved(_ slider: RangeSlider) {
        let id = UInt32(slider.tag)
        guard let owner = presenter.views[id] else { return }
        // The person moved the thumb: its own value, snapped to the step
        slider.exact = RangeSpec(owner.props).sanitize(Double(slider.value))
        let value = RangeSpec.format(slider.exact)
        guard value != lastRange[id] else { return }
        lastRange[id] = value
        presenter.controlValue(id, value, input: true, change: false)
    }

    @objc func rangeReleased(_ slider: RangeSlider) {
        // Cleared before the liveness check, so a slider whose row went
        // mid-drag leaves nothing behind.
        lastRange[UInt32(slider.tag)] = nil
        guard let (id, value) = reported(slider) else { return }
        presenter.controlValue(id, value, input: false, change: true)
        if let owner = presenter.views[id] { configureRange(slider, owner) }
    }

    /// The agent's `type <range> <n>` (D9): the value a drag released
    /// there reports, `input` then `change`.
    func typeRange(_ slider: RangeSlider, _ node: NodeView, _ text: String) -> [String: Any] {
        guard let n = Double(text.trimmingCharacters(in: .whitespaces)), n.isFinite else { return ["error": "\"\(text)\" is not a number"] }
        commitRange(slider, node, n)
        return ["typed": Int(node.id), "value": RangeSpec.format(slider.exact), "delivery": "host-activation", "native": "control"]
    }

    /// VoiceOver's swipe up or down: the range's arrow key, whose default
    /// action is the slider's step. The author's `key` handler may take it
    /// with `preventDefault()`, as it takes a keyboard's on the web: the
    /// handler's change is then the move (LLP 1069.001, amended 2026-11-16).
    /// Else, as WebKit adjusts a range: a written `step` is one move; without
    /// one, or with `any`, a twentieth of the span, at least the default step.
    private func adjustRange(_ slider: RangeSlider, by direction: Double, phase: String? = nil, repeats: Bool = false) {
        guard let node = adjustable(slider) else { return }
        node.adjustKeys(up: direction > 0, phase: phase, repeats: repeats) {
            // The keydown's handler may have disabled, hidden or replaced the range.
            guard adjustable(slider) === node else { return }
            let range = RangeSpec(node.props), from = range.sanitize(slider.exact)
            let authored = node.props["step"]?.isEmpty == false ? range.step : nil
            let to = range.sanitize(from + direction * (authored ?? Swift.max((range.max - range.min) / 20, range.step ?? 0)))
            if to != from { commitRange(slider, node, to) }
        }
    }

    /// The agent's `type <range> key ArrowRight` or `ArrowLeft`: the slider's
    /// own accessibility increment or decrement, whichever sends that key in
    /// the range's `direction` (`adjustKeys`), so an authored test's arrow
    /// takes the path VoiceOver's swipe does. A held key is that step in
    /// halves: `phase` "down" its keydown and move, again for each repeat,
    /// and "up" its keyup. Nil for another key or node, and for a range that
    /// shows no slider (`appearance="none"`), whose `UISlider` is off the
    /// window and holds none of its value or bounds: the keyboard's route
    /// has those.
    func rangeKey(_ node: NodeView, _ key: String, phase: String? = nil, repeats: Bool = false) -> [String: Any]? {
        guard key == "ArrowRight" || key == "ArrowLeft", let slider = controls[node.id] as? RangeSlider, slider.window != nil else { return nil }
        let ltr = (node.style["direction"]?.string ?? presenter.documentDirection) != "rtl"
        let up = (key == "ArrowRight") == ltr
        if phase == "up" {
            node.adjustKeys(up: up, phase: phase)
        } else if phase != nil || repeats {
            adjustRange(slider, by: up ? 1 : -1, phase: phase, repeats: repeats)
        } else if up {
            slider.accessibilityIncrement()
        } else {
            slider.accessibilityDecrement()
        }
        var reply: [String: Any] = ["typed": Int(node.id), "key": key, "value": RangeSpec.format(slider.exact), "delivery": "host-activation", "native": "control"]
        if let phase { reply["phase"] = phase }
        return reply
    }

    /// The node whose range `slider` is, while VoiceOver may move it.
    private func adjustable(_ slider: RangeSlider) -> NodeView? {
        guard let node = presenter.views[UInt32(slider.tag)], controls[node.id] === slider, slider.isEnabled, !node.inert else { return nil }
        return node
    }

    /// Puts the thumb at `value`, sanitized, and reports it as a released
    /// drag does: `input` then `change`.
    private func commitRange(_ slider: RangeSlider, _ node: NodeView, _ value: Double) {
        slider.exact = RangeSpec(node.props).sanitize(value)
        presenter.controlValue(node.id, RangeSpec.format(slider.exact), input: true, change: true)
        if let owner = presenter.views[node.id] { configureRange(slider, owner) }
    }
}

/// UIKit's slider, whose VoiceOver adjustments the host makes (`adjustRange`)
final class RangeSlider: UISlider {
    /// Where the surface draws it, inside a placed canvas child (PlacedAccessibilityIOS.swift).
    override var accessibilityFrame: CGRect {
        get { placedAccessibilityFrame ?? super.accessibilityFrame }
        set { super.accessibilityFrame = newValue }
    }
    override var accessibilityActivationPoint: CGPoint {
        get { placedActivationPoint(from: super.accessibilityActivationPoint) ?? super.accessibilityActivationPoint }
        set { super.accessibilityActivationPoint = newValue }
    }
    var adjust: ((RangeSlider, Double) -> Void)?
    /// The value it shows, which its `Float` `value` may round (a step of
    /// 1e-6): kept while the thumb stays where it was put
    var exact: Double {
        get { put.flatMap { Float($0) == value ? $0 : nil } ?? Double(value) }
        set { put = newValue; if value != Float(newValue) { value = Float(newValue) } }
    }
    private var put: Double?
    override func accessibilityIncrement() { adjust?(self, 1) }
    override func accessibilityDecrement() { adjust?(self, -1) }
}
#endif
