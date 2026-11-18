// @ref LLP 1069.001 D5 — `input type="checkbox"` is projected onto UIKit as
// a tablist is onto a segmented control: `switch` is a `UISwitch`; a plain
// checkbox is the platform's checklist mark (LLP 1116 D2: iOS has no square
// checkbox, and Safari's read as a web form), an empty circle that fills
// with a checkmark in `accent-color`, as Reminders and Notes draw one. On
// tvOS, where it stands in for a switch too, it is Safari's rounded square.
// Contract owns the value: the control flips at once, reports, and shows the
// committed `checked` after the action (D4). `appearance: none` draws
// nothing native. VoiceOver reads it as UIKit reads a toggle (LLP 1115.000
// D13.3).
#if os(iOS) || os(tvOS)
import UIKit

/// The checklist mark: an SF Symbol circle scaled with the body text, filled
/// and checked when on (tvOS: a 16×16 rounded square). A radio is drawn on
/// it, with its own symbols (`ExactRadio`, RadioIOS.swift). VoiceOver reads
/// a checkbox as it reads UIKit's own toggles (`updateAccessibility`).
class ExactCheckbox: UIControl {
    /// Where the surface draws it, inside a placed canvas child (PlacedAccessibilityIOS.swift).
    override var accessibilityFrame: CGRect {
        get { placedAccessibilityFrame ?? super.accessibilityFrame }
        set { super.accessibilityFrame = newValue }
    }
    override var accessibilityActivationPoint: CGPoint {
        get { placedActivationPoint(from: super.accessibilityActivationPoint) ?? super.accessibilityActivationPoint }
        set { super.accessibilityActivationPoint = newValue }
    }
    var isOn = false { didSet { if isOn != oldValue { setNeedsDisplay(); updateAccessibility() } } }
    var ariaUnavailable = false { didSet { if ariaUnavailable != oldValue { updateAccessibility() } } }
    var accent: UIColor? { didSet { setNeedsDisplay() } }
    override var isEnabled: Bool { didSet { setNeedsDisplay(); updateAccessibility() } }
    override init(frame: CGRect) {
        super.init(frame: frame)
        isOpaque = false
        backgroundColor = .clear
        contentMode = .redraw
        isAccessibilityElement = true
        addTarget(self, action: #selector(activated), for: .touchUpInside)
        updateAccessibility()
    }
    required init?(coder: NSCoder) { nil }
    #if os(tvOS)
    override var intrinsicContentSize: CGSize { CGSize(width: 16, height: 16) }
    #else
    /// The mark's side: UIKit's 22-point circle at the default text size,
    /// scaled with the body style by `UIFontMetrics` to 44 at most, about
    /// where UIKit's own list checkmark stops (48 by 43 at the largest
    /// size). At the body font's `.large` symbol scale it had grown to 80
    /// points there and outweighed its row (LLP 1116.019 D1).
    var side: CGFloat {
        min(44, ceil(UIFontMetrics(forTextStyle: .body).scaledValue(for: 22, compatibleWith: traitCollection)))
    }
    /// The mark's symbol, drawn fitted to `side`; on, its checkmark white
    /// over the accent.
    func mark(on: Bool, tint: UIColor) -> UIImage? {
        let size = UIImage.SymbolConfiguration(pointSize: side)
        guard on else { return UIImage(systemName: "circle", withConfiguration: size)?.withTintColor(tint, renderingMode: .alwaysOriginal) }
        return UIImage(systemName: "checkmark.circle.fill", withConfiguration: size.applying(UIImage.SymbolConfiguration(paletteColors: [.white, tint])))
    }
    override var intrinsicContentSize: CGSize { CGSize(width: side, height: side) }
    /// A finger's target is 44 points each way at least (Apple's minimum),
    /// whatever the mark's size: the touch reaches past the mark's box,
    /// which stays the mark's for layout (LLP 1115.000 D13.2). Where two
    /// marks' margins meet (radios side by side), the nearer mark has it.
    override func point(inside point: CGPoint, with event: UIEvent?) -> Bool {
        if bounds.contains(point) { return true }
        guard bounds.insetBy(dx: min(0, (bounds.width - 44) / 2), dy: min(0, (bounds.height - 44) / 2)).contains(point) else { return false }
        let mine = hypot(point.x - bounds.midX, point.y - bounds.midY)
        let marks = superview?.superview?.subviews.flatMap { $0.subviews.compactMap { $0 as? ExactCheckbox } } ?? []
        return !marks.contains { other in
            guard other !== self, !other.isHidden else { return false }
            let there = convert(point, to: other)
            return hypot(there.x - other.bounds.midX, there.y - other.bounds.midY) < mine
        }
    }
    #endif
    @objc func activated() {
        isOn.toggle()
        sendActions(for: .valueChanged)
    }
    /// A toggle, as UIKit has one (LLP 1115.000 D13.3): the `toggleButton`
    /// trait and UIKit's own value, "1" on and "0" off, which is what a
    /// `UISwitch` reports and what VoiceOver speaks in the reader's language.
    /// It had been Safari's English words, "checked" and "unchecked", read
    /// out as written in every language.
    func updateAccessibility() {
        accessibilityTraits = isEnabled && !ariaUnavailable ? [.button, .toggleButton] : [.button, .toggleButton, .notEnabled]
        accessibilityValue = isOn ? "1" : "0"
    }
    /// Drawn again when its tint changes: it draws in its tint.
    override func tintColorDidChange() { super.tintColorDidChange(); setNeedsDisplay() }
    override func draw(_ rect: CGRect) {
        let side = min(bounds.width, bounds.height)
        let alpha: CGFloat = isEnabled ? 1 : 0.4
        // At its own alpha (a share of the tint's), dimmed while disabled
        let resolved = (accent ?? tintColor ?? .systemBlue).resolvedColor(with: traitCollection)
        let fill = resolved.withAlphaComponent(resolved.cgColor.alpha * alpha)
        #if os(tvOS)
        let box = CGRect(x: bounds.midX - side / 2, y: bounds.midY - side / 2, width: side, height: side)
        if isOn {
            fill.setFill()
            UIBezierPath(roundedRect: box, cornerRadius: side * 0.25).fill()
            let check = UIBezierPath()
            check.move(to: CGPoint(x: box.minX + side * 0.25, y: box.minY + side * 0.52))
            check.addLine(to: CGPoint(x: box.minX + side * 0.43, y: box.minY + side * 0.7))
            check.addLine(to: CGPoint(x: box.minX + side * 0.76, y: box.minY + side * 0.32))
            check.lineWidth = max(1.5, side * 0.12)
            check.lineCapStyle = .round
            check.lineJoinStyle = .round
            UIColor.white.withAlphaComponent(alpha).setStroke()
            check.stroke()
        } else {
            // tvOS has no systemBackground; the ring shows what is behind it.
            let ring = UIBezierPath(roundedRect: box.insetBy(dx: 0.5, dy: 0.5), cornerRadius: side * 0.25)
            ring.lineWidth = 1
            UIColor.systemGray.withAlphaComponent(alpha).setStroke()
            ring.stroke()
        }
        #else
        // The ring in the tertiary label colour, the fill in the accent;
        // dimmed while disabled. Fitted to the box when the author made it
        // smaller than the mark.
        let tint = isOn ? fill : (isEnabled ? UIColor.tertiaryLabel : .quaternaryLabel)
        guard let image = mark(on: isOn, tint: tint) else { return }
        let scale = min(side, self.side) / max(image.size.width, image.size.height)
        let size = CGSize(width: image.size.width * scale, height: image.size.height * scale)
        image.draw(in: CGRect(x: bounds.midX - size.width / 2, y: bounds.midY - size.height / 2, width: size.width, height: size.height))
        #endif
    }
}

#if os(iOS)
/// UIKit's switch, whose frame follows a placement.
final class ExactSwitch: UISwitch {
    override var accessibilityFrame: CGRect {
        get { placedAccessibilityFrame ?? super.accessibilityFrame }
        set { super.accessibilityFrame = newValue }
    }
    override var accessibilityActivationPoint: CGPoint {
        get { placedActivationPoint(from: super.accessibilityActivationPoint) ?? super.accessibilityActivationPoint }
        set { super.accessibilityActivationPoint = newValue }
    }
}
#endif

/// The built-in kinds behind `ControlLink.make`: each forwards to the host's
/// extensions in this family's files.
final class BuiltInControlKinds: ControlKinding {
    unowned let host: ControlHost
    init(_ host: ControlHost) { self.host = host }

    func make(_ kind: String, for node: NodeView) -> UIControl {
        let made: UIControl
        switch kind {
        #if os(tvOS)
        // tvOS has no UISwitch; a checkbox stands in.
        case "switch": made = ExactCheckbox(frame: .zero)
        #else
        case "switch": made = ExactSwitch()
        #endif
        case "checkbox": made = ExactCheckbox(frame: .zero)
        case "radio":
            let radio = ExactRadio(frame: .zero)
            radio.owner = node
            made = radio
        default: made = host.makeValueControl(kind, node.id)
        }
        if kind == "switch" || kind == "checkbox" { made.addTarget(host, action: #selector(ControlHost.changed(_:)), for: .valueChanged) }
        if kind == "radio" { made.addTarget(host, action: #selector(ControlHost.radioTapped(_:)), for: .touchUpInside) }
        return made
    }

    /// The accent is UIKit's tint (`accentUIColor`) or the mark's (`accentShare`).
    func configure(_ control: UIControl, _ owner: NodeView, checked on: Bool?) {
        #if !os(tvOS)
        if let s = control as? UISwitch {
            if let on, s.isOn != on { s.setOn(on, animated: s.window != nil) }
            assign(s, \.onTintColor, owner.accentUIColor())
            return
        }
        #endif
        if let c = control as? ExactCheckbox {
            if let on { c.isOn = on }
            assign(c, \.accent, owner.accentShare)
            c.ariaUnavailable = owner.ariaUnavailable
        } else {
            host.configureValue(control, owner)
        }
    }

    func naturalSize(_ control: UIControl, _ owner: NodeView) -> CGSize { host.naturalSize(control, owner) }
    #if os(iOS)
    func selectHeight(_ button: UIButton, width: CGFloat) -> CGFloat { host.selectHeight(button, width: width) }
    func valueAlignment(_ owner: NodeView) -> UIControl.ContentHorizontalAlignment { host.valueAlignment(owner) }
    #endif

    func activate(_ control: UIControl, _ node: NodeView) -> Bool {
        // A radio's tap, the host's own (x2apps survey #2).
        if control is ExactRadio { host.radioTapped(control); return true }
        #if os(tvOS)
        if control is ExactCheckbox { control.sendActions(for: .touchUpInside) }
        else { return host.openValue(control) }
        #else
        if let s = control as? UISwitch { s.setOn(!s.isOn, animated: false); s.sendActions(for: .valueChanged) }
        else if control is ExactCheckbox { control.sendActions(for: .touchUpInside) }
        else { return host.openValue(control) }
        #endif
        return true
    }

    func observation(_ control: UIControl) -> [String: Any] {
        let size = [InspectionValue.r2(control.bounds.width), InspectionValue.r2(control.bounds.height)]
        if let value = host.valueObservation(control) { return value.merging(["size": size]) { a, _ in a } }
        #if os(tvOS)
        let on = (control as? ExactCheckbox)?.isOn ?? false
        return ["view": "checkbox", "on": on, "size": size]
        #else
        let on = (control as? UISwitch)?.isOn ?? (control as? ExactCheckbox)?.isOn ?? false
        return ["view": control is UISwitch ? "UISwitch" : control is ExactRadio ? "radio" : "checkbox", "on": on, "size": size]
        #endif
    }

    func syncProgress() { host.syncProgress() }
    func progressObservation(_ node: NodeView) -> [String: Any]? { host.progressObservation(node) }
    func radioOn(_ control: UIControl) -> Bool { host.radioOn(control) }
    func showRadio(_ id: UInt32, _ on: Bool) { host.showRadio(id, on) }
    func type(_ node: NodeView, _ value: String) -> [String: Any]? { host.type(node, value) }
    func unopened(_ node: NodeView) -> [String: Any]? { host.unopened(node) }
    #if os(iOS)
    func rangeKey(_ node: NodeView, _ key: String, phase: String?, repeats: Bool) -> [String: Any]? { host.rangeKey(node, key, phase: phase, repeats: repeats) }
    #endif

    static func drawnToggle(_ object: AnyObject) -> (radio: Bool, on: Bool)? {
        guard let box = object as? ExactCheckbox else { return nil }
        return (box is ExactRadio, box.isOn)
    }
}

extension ControlHost {
    @objc func changed(_ sender: UIControl) {
        let id = UInt32(sender.tag)
        guard let node = presenter.views[id] else { return }
        // Read first: the press's batch syncs a bound control back to `checked`.
        #if os(tvOS)
        let on = (sender as? ExactCheckbox)?.isOn ?? false
        #else
        let on = (sender as? UISwitch)?.isOn ?? (sender as? ExactCheckbox)?.isOn ?? false
        #endif
        // A click on a checkbox is heard before its `input` and `change` (HTML
        // dispatches `click`, then fires those as its activation behaviour).
        if node.handlers.contains("press") { presenter.press(id) }
        presenter.checked(id, on)
        // The committed state is authoritative: an action that refused the
        // toggle snaps the control back (D4).
        if let committed = presenter.views[id]?.props["checked"].map({ $0 == "true" }) {
            #if !os(tvOS)
            if let s = sender as? UISwitch, s.isOn != committed { s.setOn(committed, animated: true) }
            #endif
            if let c = sender as? ExactCheckbox, c.isOn != committed { c.isOn = committed }
        }
    }
}
#endif
