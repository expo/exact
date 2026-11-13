// A canvas child its surface places (LLP 1014 D5) stays at its kernel frame
// in the view tree, and the surface draws it elsewhere. Exact's own node
// views map their accessibility frame through the placement
// (`NodeView.accessibilityFrame`); a UIKit view inside one does not, since
// UIKit computes a view's frame from the view tree alone. So the controls
// and fields Exact makes answer from here: one that is an element itself
// maps its own frame, and one whose parts are UIKit's elements (a stepper's
// two buttons, a segmented control's segments, a compact date picker's date
// and time) stands an element of Exact's in for each part while it is
// placed (`PlacedElement`), as an empty date picker does (`EmptyDateElement`).
// VoiceOver's cursor and the frames an XCUITest client reads are then where
// the person sees the control.
#if os(iOS) || os(tvOS)
import UIKit

extension UIView {
    /// The canvas child this view is placed under, when the surface places
    /// one above it, and the node whose coordinates map through it.
    private var placedOwner: (owner: NodeView, placed: NodeView)? {
        guard NodeView.placedViews.count > 0,
              let owner = sequence(first: superview, next: { $0?.superview }).lazy.compactMap({ $0 as? NodeView }).first,
              let placed = owner.placedAncestor else { return nil }
        return (owner, placed)
    }

    /// Whether this view is inside a canvas child its surface places.
    var isPlacedBySurface: Bool { placedOwner != nil }

    /// This view's box on screen where the surface draws it, when it is
    /// inside a placed canvas child: `.zero` while the surface hides that
    /// child, nil when it is not inside one (UIKit's own frame is right).
    var placedAccessibilityFrame: CGRect? {
        guard let (owner, placed) = placedOwner else { return nil }
        if placed.placementHidden { return .zero }
        guard placed.placement != nil, let window else { return nil }
        return UIAccessibility.convertToScreenCoordinates(owner.drawnRect(owner.convert(bounds, from: self), in: window), in: window)
    }

    /// `point`, a point on screen as UIKit's view tree has it (an element's
    /// activation point UIKit computed), where the surface draws it; nil when
    /// this view is not inside a placed canvas child.
    func placedScreenPoint(_ point: CGPoint) -> CGPoint? { placedScreenRect(CGRect(origin: point, size: .zero))?.origin }

    /// Where VoiceOver taps this view, inside a placed canvas child: UIKit's
    /// own point (`native`), mapped through the placement unless UIKit took
    /// it from the frame this view already maps (its default, the frame's
    /// midpoint). VoiceOver's synthesized tap there is mapped back to the view
    /// by the canvas's hit test.
    func placedActivationPoint(from native: CGPoint) -> CGPoint? {
        guard let frame = placedAccessibilityFrame else { return nil }
        return frame.contains(native) ? native : placedScreenPoint(native)
    }

    /// `rect`, a box on screen as UIKit's view tree has it (an element's
    /// frame UIKit computed), where the surface draws it; nil when this view
    /// is not inside a placed canvas child.
    func placedScreenRect(_ rect: CGRect) -> CGRect? {
        guard let (owner, placed) = placedOwner else { return nil }
        if placed.placementHidden { return .zero }
        guard placed.placement != nil, let window else { return nil }
        // Back into the window as UIKit took it to the screen.
        let screen = UIAccessibility.convertToScreenCoordinates(window.bounds, in: window)
        let inWindow = rect.offsetBy(dx: window.bounds.minX - screen.minX, dy: window.bounds.minY - screen.minY)
        return UIAccessibility.convertToScreenCoordinates(owner.drawnRect(owner.convert(inWindow, from: window), in: window), in: window)
    }

    /// The elements UIKit exposes for this view, found as VoiceOver finds
    /// them: what it declares (`declared`, UIKit's own answer before any
    /// stand-in's), else its subviews, down to each element.
    func nativeAccessibilityElements(declared: [NSObject]) -> [NSObject] {
        var found: [NSObject] = [], seen = Set<ObjectIdentifier>()
        func visit(_ object: NSObject) {
            guard seen.insert(ObjectIdentifier(object)).inserted else { return }
            if object.isAccessibilityElement { found.append(object); return }
            if let elements = object.accessibilityElements, !elements.isEmpty {
                for case let element as NSObject in elements { visit(element) }
                return
            }
            let count = object.accessibilityElementCount()
            if count != NSNotFound, count > 0 {
                for i in 0..<count { if let element = object.accessibilityElement(at: i) as? NSObject { visit(element) } }
            } else if let view = object as? UIView {
                for child in view.subviews { visit(child) }
            }
        }
        for object in declared.isEmpty ? subviews : declared { visit(object) }
        return found
    }
}

/// Exact's element for one of UIKit's inside a placed control: everything
/// UIKit's says, at the frame the surface draws it at. Activating it, or
/// adjusting it, is done by UIKit's; where UIKit's leaves activation to
/// VoiceOver, VoiceOver taps the activation point here, which the canvas
/// maps back through the placement to the control.
final class PlacedElement: UIAccessibilityElement, AgentOwned {
    private(set) weak var native: NSObject?
    private weak var control: UIView?
    init(standingFor native: NSObject, in control: UIView) {
        self.native = native
        self.control = control
        super.init(accessibilityContainer: control)
    }
    var agentViewId: UInt32? { control.map { UInt32($0.tag) } }
    override var accessibilityLabel: String? { get { native?.accessibilityLabel } set {} }
    override var accessibilityHint: String? { get { native?.accessibilityHint } set {} }
    override var accessibilityValue: String? { get { native?.accessibilityValue } set {} }
    override var accessibilityTraits: UIAccessibilityTraits { get { native?.accessibilityTraits ?? .none } set {} }
    override var accessibilityFrame: CGRect {
        get {
            guard let native else { return .zero }
            return control?.placedScreenRect(native.accessibilityFrame) ?? native.accessibilityFrame
        }
        set {}
    }
    override var accessibilityActivationPoint: CGPoint {
        get {
            guard let native else { return .zero }
            return control?.placedScreenPoint(native.accessibilityActivationPoint) ?? native.accessibilityActivationPoint
        }
        set {}
    }
    override var accessibilityIdentifier: String? { get { (native as? UIAccessibilityIdentification)?.accessibilityIdentifier } set {} }
    override var accessibilityCustomActions: [UIAccessibilityCustomAction]? { get { native?.accessibilityCustomActions } set {} }
    override func accessibilityActivate() -> Bool { native?.accessibilityActivate() ?? false }
    override func accessibilityIncrement() { native?.accessibilityIncrement() }
    override func accessibilityDecrement() { native?.accessibilityDecrement() }
}

/// A placed control's stand-ins, one for each of UIKit's elements, kept
/// while that element lives so VoiceOver's cursor stays on one.
final class PlacedStandIns {
    private var kept: [PlacedElement] = []
    /// UIKit's own answer is being read, which may ask the control again.
    private var reading = false
    /// The stand-ins `control` exposes while it is placed, nil when it is not
    /// (UIKit's own elements are right). `declared` reads UIKit's own answer.
    func elements(of control: UIView, declared: () -> [NSObject]) -> [PlacedElement]? {
        guard !reading, control.isPlacedBySurface else { if !reading { kept = [] }; return nil }
        reading = true
        let natives = control.nativeAccessibilityElements(declared: declared())
        reading = false
        kept = natives.map { native in kept.first { $0.native === native } ?? PlacedElement(standingFor: native, in: control) }
        return kept
    }
}

/// What a UIKit control declares as its elements before any stand-in: its
/// `accessibilityElements`, else its counted elements (`super`'s answers).
func declaredElements(_ elements: [Any]?, count: Int, at: (Int) -> Any?) -> [NSObject] {
    if let elements, !elements.isEmpty { return elements.compactMap { $0 as? NSObject } }
    guard count != NSNotFound, count > 0 else { return [] }
    return (0..<count).compactMap { at($0) as? NSObject }
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
#endif
