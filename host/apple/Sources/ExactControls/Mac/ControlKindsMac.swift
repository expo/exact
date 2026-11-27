// @ref LLP 1069.001 D5 — `input type="checkbox"` is AppKit's checkbox
// (`NSButton`), and `switch` an `NSSwitch`, projected into the node's box as
// a tablist's segmented control is. Contract owns the value: the control
// flips at once, reports, and shows the committed `checked` after the action
// (D4). `appearance: none` draws nothing native.
#if os(macOS)
import AppKit
import ExactKit

class AriaButtonCellMac: NSButtonCell {
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, (controlView as? NSControl)?.ariaUnavailableFromNode == true { return false }
        return super.accessibilityAttributeValue(attribute)
    }
    // An assistive app's press clicks once AppKit's request has returned (`NativeButtonCellMac`).
    override func accessibilityPerformPress() -> Bool {
        guard isEnabled else { return false }
        DispatchQueue.main.async { _ = super.accessibilityPerformPress() }
        return true
    }
}

protocol AccentControl: AnyObject {
    func applyAccent(_ color: NSColor?)
}

final class ControlAccent {
    private var authored: NSColor?
    var nativeFill: NSColor?

    // The control's public setter records hatch writes, even one equal to the authored colour.
    // Exact writes through the superclass setter so clearing restores the last native fill.
    func apply(_ color: NSColor?, current: NSColor?, write: (NSColor?) -> Void) {
        guard color != authored else { return }
        if authored == nil { nativeFill = current }
        authored = color
        let fill = color ?? nativeFill
        if current != fill { write(fill) }
    }
}

class AccentToggleMac: NSButton, AccentControl {
    private let accent = ControlAccent()
    override var contentTintColor: NSColor? { didSet { accent.nativeFill = contentTintColor } }
    func applyAccent(_ color: NSColor?) {
        accent.apply(color, current: contentTintColor) { super.contentTintColor = $0 }
    }
}

final class CheckboxMac: AccentToggleMac {
    override class var cellClass: AnyClass? { get { AriaButtonCellMac.self } set {} }
    override func mouseDown(with event: NSEvent) { if !contextClicked(event) { super.mouseDown(with: event) } }
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, ariaUnavailableFromNode { return false }
        return super.accessibilityAttributeValue(attribute)
    }
}

final class SwitchMac: NSSwitch {
    override func mouseDown(with event: NSEvent) { if !contextClicked(event) { super.mouseDown(with: event) } }
    // An assistive app's press flips it once AppKit's request has returned (`NativeButtonCellMac`).
    override func accessibilityPerformPress() -> Bool {
        guard isEnabled else { return false }
        DispatchQueue.main.async { _ = super.accessibilityPerformPress() }
        return true
    }
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, ariaUnavailableFromNode { return false }
        return super.accessibilityAttributeValue(attribute)
    }
}

final class PopupCellMac: NSPopUpButtonCell {
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, (controlView as? NSControl)?.ariaUnavailableFromNode == true { return false }
        return super.accessibilityAttributeValue(attribute)
    }
}

final class SliderCellMac: NSSliderCell {
    override func accessibilityAttributeValue(_ attribute: NSAccessibility.Attribute) -> Any? {
        if attribute == .enabled, (controlView as? NSControl)?.ariaUnavailableFromNode == true { return false }
        return super.accessibilityAttributeValue(attribute)
    }
    // An assistive app's increment or decrement is the range's arrow key,
    // whose default action is AppKit's own step: the author's `key` handler
    // may take it with `preventDefault()` (LLP 1069.001, amended 2026-11-16).
    // The cell is the slider's accessibility element, and the control's own
    // methods call these.
    override func accessibilityPerformIncrement() -> Bool { adjust(up: true) { super.accessibilityPerformIncrement() } }
    override func accessibilityPerformDecrement() -> Bool { adjust(up: false) { super.accessibilityPerformDecrement() } }
    private func adjust(up: Bool, _ step: () -> Bool) -> Bool {
        guard let node = (controlView as? NSControl)?.ownerNode else { return step() }
        // Asked again after the keydown, whose handler may have disabled,
        // hidden or replaced the range.
        let adjustable = { self.isEnabled && !node.inert && node.presenter?.views[node.id] === node }
        guard adjustable() else { return false }
        // The answer is whether the value moved, by a handler or by AppKit's step.
        let from = doubleValue
        node.adjustKeys(up: up) { if adjustable() { _ = step() } }
        return doubleValue != from
    }
}

/// The built-in kinds behind `ControlLink.make`: each forwards to the host's
/// extensions in this family's files.
final class BuiltInControlKinds: ControlKinding {
    unowned let host: ControlHost
    init(_ host: ControlHost) { self.host = host }

    func make(_ kind: String, for node: NodeView) -> NSControl {
        let made: NSControl
        switch kind {
        case "switch": made = SwitchMac()
        case "checkbox":
            let box = CheckboxMac(checkboxWithTitle: "", target: nil, action: nil)
            box.imagePosition = .imageOnly
            made = box
        case "radio": made = RadioButtonMac() // x2apps survey #2
        default: made = host.makeValueControl(kind)
        }
        made.target = host
        made.action = kind == "radio" ? #selector(ControlHost.radioClicked(_:))
            : kind == "switch" || kind == "checkbox" ? #selector(ControlHost.changed(_:)) : #selector(ControlHost.valueChanged(_:))
        return made
    }

    func applyAccent(_ accent: NSColor?, to control: NSControl) {
        // NSSwitch exposes no custom on-state fill (LLP 1069.001 D6).
        (control as? AccentControl)?.applyAccent(accent)
    }

    func configure(_ control: NSControl, _ owner: NodeView, checked on: Bool?) {
        guard control is NSSwitch || ["checkbox", "radio"].contains(host.kinds[owner.id]) else {
            host.configureValue(control, owner)
            return
        }
        // @ref LLP 1115.002 D6 — a grouped form's switch is the size its
        // sheet names (`-exact-control-size`, a metric: AppKit's mini).
        if let s = control as? NSSwitch {
            let named: [String: NSControl.ControlSize] = ["mini": .mini, "small": .small, "large": .large]
            let size = named[owner.style["control_size"]?.string ?? ""] ?? .regular
            // AppKit takes the new size's metrics at its next layout: now, so the size reported below is the one drawn.
            if s.controlSize != size { s.controlSize = size; s.layoutSubtreeIfNeeded() }
        }
        if let on { host.setOn(control, on) }
    }

    func naturalSize(_ control: NSControl, _ owner: NodeView) -> CGSize { host.naturalSize(control) }

    func activate(_ control: NSControl, _ node: NodeView) -> Bool {
        // A click at a slider's middle moves its knob there, as a click
        // does; AppKit's own tracking loop would wait for a mouse-up the
        // agent's synthesized click never hands it.
        // A date field takes the focus, as a click into it does.
        if let picker = control as? NSDatePicker { return picker.window?.makeFirstResponder(picker) ?? false }
        if let slider = control as? NSSlider {
            _ = host.typeRange(slider, node, String((slider.minValue + slider.maxValue) / 2))
            return true
        }
        if !["checkbox", "switch", "radio"].contains(host.kinds[node.id]) { return host.openValue(control) }
        if let b = control as? NSButton { b.performClick(nil) } else {
            host.setOn(control, !host.isOn(control))
            host.changed(control)
        }
        return true
    }

    func observation(_ control: NSControl) -> [String: Any] {
        let size = [InspectionValue.r2(control.frame.width), InspectionValue.r2(control.frame.height)]
        if let value = host.valueObservation(control) { return value.merging(["size": size]) { a, _ in a } }
        return ["view": control is NSSwitch ? "NSSwitch" : control is RadioButtonMac ? "NSButton(radio)" : "NSButton(checkbox)", "on": host.isOn(control), "size": size]
    }

    func syncProgress() { host.syncProgress() }
    func progressObservation(_ node: NodeView) -> [String: Any]? { host.progressObservation(node) }
    func radioOn(_ control: NSControl) -> Bool { host.radioOn(control) }
    func showRadio(_ id: UInt32, _ on: Bool) { host.showRadio(id, on) }
    #if EXACT_AGENT
    func type(_ node: NodeView, _ value: String) -> [String: Any]? { host.type(node, value) }
    #endif

    /// The agent reads AppKit's own checkboxes and radios.
    static func drawnToggle(_ object: AnyObject) -> (radio: Bool, on: Bool)? { nil }
}

extension ControlHost {
    @objc func changed(_ sender: NSControl) {
        let id = UInt32(sender.tag)
        guard let node = presenter.views[id] else { return }
        // Read first: the press's batch syncs a bound control back to `checked`.
        let on = isOn(sender)
        // A click on a checkbox is heard before its `input` and `change` (HTML
        // dispatches `click`, then fires those as its activation behaviour).
        if node.handlers.contains("press") { presenter.press(id) }
        presenter.checked(id, on)
        // The committed state is authoritative: an action that refused the
        // toggle snaps the control back (D4).
        if let committed = presenter.views[id]?.props["checked"].map({ $0 == "true" }) { setOn(sender, committed) }
    }
}
#endif
