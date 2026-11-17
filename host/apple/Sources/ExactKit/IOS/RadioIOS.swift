// `input type="radio"` on UIKit (x2apps survey #2), which has no radio
// control: the SF Symbols pair, `circle` and, checked, `circle.inset.filled`
// in `accent-color`, scaled with the body text, the way the checkbox is
// (`ExactCheckbox`), with its 44-point target (LLP 1115.000 D13.2). A standard
// grouped row shows the platform's checkmark (`Accessory::Radio`,
// GroupedListIOS.swift); a custom row carries the control.
// tvOS keeps the drawn circle.
// A tap never moves its own state: the group is Exact's (`Radios.swift`).
#if os(iOS) || os(tvOS)
import UIKit

final class ExactRadio: ExactCheckbox {
    weak var owner: NodeView?

    /// A custom choice keeps its row's name unless the radio has its own.
    /// Read the row live: a sibling's text can change without a control update.
    override var accessibilityLabel: String? {
        get {
            if let label = super.accessibilityLabel { return label }
            guard let owner, let row = owner.presenter?.groupedLists?.radioRow(for: owner) else { return nil }
            let name = row.accessibleName
            return name.isEmpty ? nil : name
        }
        set { super.accessibilityLabel = newValue }
    }

    #if os(tvOS)
    override var intrinsicContentSize: CGSize { CGSize(width: 16, height: 16) }
    #else
    /// The radio's symbol at the checkbox's `side`: an empty circle, and
    /// checked a circle with its centre filled, in one colour.
    override func mark(on: Bool, tint: UIColor) -> UIImage? {
        UIImage(systemName: on ? "circle.inset.filled" : "circle", withConfiguration: UIImage.SymbolConfiguration(pointSize: side))?
            .withTintColor(tint, renderingMode: .alwaysOriginal)
    }
    #endif

    override func accessibilityActivate() -> Bool {
        guard isEnabled, !ariaUnavailable, let owner, !owner.inert, !owner.formDisabled,
              let presenter = owner.presenter else { return false }
        if !isOn { presenter.checkRadio(owner.id) }
        return true
    }

    /// A tap moves nothing here: the host hears it (`radioTapped`), and
    /// checks it and unchecks the rest of its group.
    override func activated() {}

    /// UIKit has no public radio trait: VoiceOver reads it as Safari's ARIA
    /// radios read here (`setAccessibilityChecked`), a button that is
    /// selected while checked, with no checkbox value.
    override func updateAccessibility() {
        var traits: UIAccessibilityTraits = .button
        if isOn { traits.insert(.selected) }
        if !isEnabled || ariaUnavailable { traits.insert(.notEnabled) }
        accessibilityTraits = traits
        accessibilityValue = nil
    }

    #if os(tvOS)
    override func draw(_ rect: CGRect) {
        let side = min(bounds.width, bounds.height)
        let box = CGRect(x: bounds.midX - side / 2, y: bounds.midY - side / 2, width: side, height: side)
        let alpha: CGFloat = isEnabled ? 1 : 0.4
        if isOn {
            (accent ?? tintColor ?? .systemBlue).withAlphaComponent(alpha).setFill()
            UIBezierPath(ovalIn: box).fill()
            UIColor.white.withAlphaComponent(alpha).setFill()
            UIBezierPath(ovalIn: box.insetBy(dx: side * 0.3, dy: side * 0.3)).fill()
        } else {
            let ring = UIBezierPath(ovalIn: box.insetBy(dx: 0.5, dy: 0.5))
            UIColor.clear.setFill()
            ring.fill()
            ring.lineWidth = 1
            UIColor.systemGray.withAlphaComponent(alpha).setStroke()
            ring.stroke()
        }
    }
    #endif
}

extension Presenter {
    /// A grouped list's row checks its radio as a click on the radio does
    /// (`ControlHost.checkRadio`, LLP 1115.000 D13.2).
    @discardableResult
    package func checkRadio(_ id: UInt32) -> Bool { controls.checkRadio(id) }
}

extension ControlHost {
    func radioOn(_ control: UIControl) -> Bool { (control as? ExactCheckbox)?.isOn ?? false }

    func showRadio(_ id: UInt32, _ on: Bool) { (controls[id] as? ExactRadio)?.isOn = on }

    /// The arrows move the focus with the check, as on the web; a tap does
    /// not focus a radio, as Safari iOS does not. The ring shows for a
    /// keyboard's move (`showFocusRing`).
    func focusRadio(_ node: NodeView, keyboard: Bool = false) {
        guard !node.isFirstResponder, node.canBecomeFirstResponder, node.becomeFirstResponder() else { return }
        if keyboard { node.showFocusRing(true) }
    }

    @objc func radioTapped(_ sender: UIControl) { checkRadio(UInt32(sender.tag)) }
}
#endif
