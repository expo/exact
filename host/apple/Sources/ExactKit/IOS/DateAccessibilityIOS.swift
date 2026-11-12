#if os(iOS)
import UIKit

/// What VoiceOver reads for an empty compact picker in place of one of
/// UIKit's own elements for it. UIKit's element computes its value from the
/// picker's date, which an empty picker still holds, and ignores an assigned
/// one; so while the picker is empty it exposes these instead
/// (`UIAccessibilityContainer`, `BoundDatePicker.accessibilityElements`).
/// Each says what its UIKit element says, but for the value, which is the
/// author's placeholder or none, and activating it activates that element,
/// which opens the calendar or wheel; where that element leaves activation
/// to VoiceOver, VoiceOver taps its activation point, on UIKit's content.
final class EmptyDateElement: UIAccessibilityElement, AgentOwned {
    private(set) weak var native: NSObject?
    private weak var picker: BoundDatePicker?
    init(standingFor native: NSObject, in picker: BoundDatePicker) {
        self.native = native
        self.picker = picker
        super.init(accessibilityContainer: picker)
    }
    var agentViewId: UInt32? { picker.map { UInt32($0.tag) } }
    override var accessibilityLabel: String? { get { native?.accessibilityLabel } set {} }
    override var accessibilityHint: String? { get { native?.accessibilityHint } set {} }
    override var accessibilityValue: String? {
        get { picker.flatMap { $0.placeholder.isEmpty ? nil : $0.placeholder } }
        set {}
    }
    override var accessibilityTraits: UIAccessibilityTraits { get { native?.accessibilityTraits ?? .button } set {} }
    /// UIKit's element's frame, where the surface draws it inside a placed canvas child.
    override var accessibilityFrame: CGRect {
        get { native.map { picker?.placedScreenRect($0.accessibilityFrame) ?? $0.accessibilityFrame } ?? .zero }
        set {}
    }
    override var accessibilityActivationPoint: CGPoint {
        get {
            guard let native else { return CGPoint(x: accessibilityFrame.midX, y: accessibilityFrame.midY) }
            return picker?.placedScreenPoint(native.accessibilityActivationPoint) ?? native.accessibilityActivationPoint
        }
        set {}
    }
    override var accessibilityIdentifier: String? { get { (native as? UIAccessibilityIdentification)?.accessibilityIdentifier } set {} }
    override var accessibilityCustomActions: [UIAccessibilityCustomAction]? { get { native?.accessibilityCustomActions } set {} }
    override func accessibilityActivate() -> Bool { native?.accessibilityActivate() ?? false }
}

#endif
