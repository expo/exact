// @ref LLP 1069.001 D5 — `input type="range"` is an `NSSlider`: HTML's
// `input` as the knob moves, `change` on release or a discrete step, each value
// clamped and snapped to `step` as HTML sanitizes it, and the knob with it. The bound
// value is written into it when it changes, as the web build writes an
// input's `value`: an action that has not written it leaves the knob where
// the person put it (LLP 1069.001 D4, amended 2026-10-04).
#if os(macOS)
import AppKit
import ExactKit

extension ControlHost {
    func makeRange() -> NSControl {
        let slider = SliderMac(value: 0, minValue: 0, maxValue: 100, target: nil, action: nil)
        slider.isContinuous = true
        return slider
    }

    func configureRange(_ slider: NSSlider, _ owner: NodeView, accent: NSColor?) {
        let range = RangeSpec(owner.props)
        slider.minValue = range.min
        slider.maxValue = range.max
        let bound = owner.props["value"] ?? ""
        if appliedRange[owner.id] != bound, NSApp.currentEvent?.type != .leftMouseDragged {
            appliedRange[owner.id] = bound
            let shown = range.shown(owner.props)
            if slider.doubleValue != shown { slider.doubleValue = shown }
        }
        if slider.trackFillColor != accent { slider.trackFillColor = accent }
        // A value set inside the knob's tracking draws only when asked to.
        slider.needsDisplay = true
        slider.setAccessibilityValue(RangeSpec.format(range.sanitize(slider.doubleValue)))
    }

    /// Mouse tracking defers `change` until release; keyboard and accessibility steps commit at once.
    func rangeChanged(_ slider: NSSlider) {
        let id = UInt32(slider.tag)
        guard let owner = presenter.views[id] else { return }
        let value = RangeSpec.format(RangeSpec(owner.props).sanitize(slider.doubleValue))
        // Snap the knob here: AppKit's own stepping draws a tick mark for every step
        if let snapped = Double(value), slider.doubleValue != snapped { slider.doubleValue = snapped }
        // Accessibility actions have no event of their own: currentEvent may be unrelated.
        let tracking = NSApp.currentEvent.map { $0.type == .leftMouseDown || $0.type == .leftMouseDragged } ?? false
        if value != lastRange[id] {
            lastRange[id] = value
            presenter.controlValue(id, value, input: true, change: false)
        }
        if !tracking {
            lastRange[id] = nil
            presenter.controlValue(id, value, input: false, change: true)
            if let owner = presenter.views[id] {
                configureRange(slider, owner, accent: owner.channels("accent_color").map { TextEngine.color($0) })
            }
        }
    }

    /// The agent's `type <range> <n>` (D9): the value a drag released
    /// there reports, `input` then `change`.
    func typeRange(_ slider: NSSlider, _ node: NodeView, _ text: String) -> [String: Any] {
        guard let n = Double(text.trimmingCharacters(in: .whitespaces)), n.isFinite else { return ["error": "\"\(text)\" is not a number"] }
        let value = RangeSpec.format(RangeSpec(node.props).sanitize(n))
        slider.doubleValue = Double(value) ?? slider.doubleValue
        presenter.controlValue(node.id, value, input: true, change: true)
        if let owner = presenter.views[node.id] {
            configureRange(slider, owner, accent: owner.channels("accent_color").map { TextEngine.color($0) })
        }
        // What it shows, as the web's reply says.
        return ["typed": Int(node.id), "value": RangeSpec.format(RangeSpec(node.props).sanitize(slider.doubleValue)), "delivery": "host-activation", "native": "control"]
    }
}

/// A range's slider, AppKit's own, but for a Control-click its node's
/// `contextmenu` answers (X85, `contextClicked`).
final class SliderMac: NSSlider {
    override class var cellClass: AnyClass? { get { SliderCellMac.self } set {} }
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, ariaUnavailableFromNode { return false }
        return super.accessibilityAttributeValue(attribute)
    }
    override func mouseDown(with event: NSEvent) { if !contextClicked(event) { super.mouseDown(with: event) } }
}
#endif
