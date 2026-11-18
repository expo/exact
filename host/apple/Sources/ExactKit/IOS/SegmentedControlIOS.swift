// The segmented control a tablist, a radio group, a navigation bar's
// segments and a search scope show, and what an assistive client and the
// agent read of its segments: the core's, which navigation uses (the
// segment host is `ControlLink`'s, SegmentsIOS.swift).
#if os(iOS) || os(tvOS)
import UIKit

/// A segmented control whose segments carry their tabs' names and test ids
/// (LLP 1116 D2, D6): a title segment's `aria-label`, when it says more than
/// its words, is its accessibility label, and a tab's `testId` its
/// accessibility identifier, so XCUITest and Maestro address it as any
/// other node. UIKit has no API for either on a segment, and Exact names no
/// private class (LLP 1035.003), so they are written on the segments UIKit's
/// accessibility container declares, as a client reads them and after each
/// layout. A segment named by nothing is UIKit's (its title).
class LabelledSegmentedControl: UISegmentedControl {
    /// UIKit's parts stood in for at the frames the surface draws them at,
    /// inside a placed canvas child (PlacedAccessibilityIOS.swift).
    private let placedParts = PlacedStandIns()
    /// The stand-ins a placed control exposes, nil when it is not placed. A
    /// read of the whole container (`labelled`) labels UIKit's segments first;
    /// a read UIKit makes back into the control while it labels them gets
    /// UIKit's own answer.
    private func placedElements(labelled: Bool) -> [PlacedElement]? {
        guard !labelling else { return nil }
        if labelled { labelSegments() }
        return placedParts.elements(of: self) { uikitElements }
    }
    /// The elements UIKit's accessibility container declares, unlabelled by this class.
    private var uikitElements: [NSObject] {
        declaredElements(super.accessibilityElements, count: super.accessibilityElementCount(), at: { super.accessibilityElement(at: $0) })
    }
    override var accessibilityElements: [Any]? {
        get { placedElements(labelled: true) ?? super.accessibilityElements }
        set { super.accessibilityElements = newValue }
    }
    override func accessibilityElementCount() -> Int { placedElements(labelled: true)?.count ?? super.accessibilityElementCount() }
    override func accessibilityElement(at index: Int) -> Any? {
        guard let elements = placedElements(labelled: false) else { return super.accessibilityElement(at: index) }
        return elements.indices.contains(index) ? elements[index] : nil
    }
    override func index(ofAccessibilityElement element: Any) -> Int {
        guard let elements = placedElements(labelled: false) else { return super.index(ofAccessibilityElement: element) }
        return elements.firstIndex { $0 === element as AnyObject } ?? NSNotFound
    }
    var segmentLabels: [String?] = [] { didSet { if segmentLabels != oldValue { setNeedsLayout() } } }
    var segmentIDs: [String?] = [] { didSet { if segmentIDs != oldValue { setNeedsLayout() } } }
    var segmentOwners: [UInt32] = []
    var segmentUnavailable: [Bool] = [] { didSet { if segmentUnavailable != oldValue { setNeedsLayout() } } }
    /// Again after each layout, so a segment a client already holds shows its
    /// selection and availability as they change.
    override func layoutSubviews() {
        super.layoutSubviews()
        labelSegments()
    }
    /// UIKit's own answer is being read for labelling (`placedElements`).
    private var labelling = false
    private func labelSegments() {
        guard !labelling else { return }
        labelling = true
        defer { labelling = false }
        for (index, element) in segmentElements(uikitElements).enumerated() {
            let label = segmentLabels.indices.contains(index) ? segmentLabels[index] : nil
            let id = segmentIDs.indices.contains(index) ? segmentIDs[index] : nil
            if element.accessibilityLabel != label { element.accessibilityLabel = label }
            Self.identify(element, as: id)
            let unavailable = !isEnabled || !isEnabledForSegment(at: index) ||
                (segmentUnavailable.indices.contains(index) && segmentUnavailable[index])
            var traits = element.accessibilityTraits
            if unavailable { traits.insert(.notEnabled) } else { traits.remove(.notEnabled) }
            if index == selectedSegmentIndex { traits.insert(.selected) } else { traits.remove(.selected) }
            // Write only changed traits so UIKit's computed selection is not frozen by an unchanged override
            if element.accessibilityTraits != traits { element.accessibilityTraits = traits }
        }
    }
}

extension UISegmentedControl {
    /// Its segments as an assistive client reads them, in segment order
    /// (`segmentElements`).
    var accessibilitySegments: [NSObject] {
        segmentElements(declaredElements(accessibilityElements, count: accessibilityElementCount(), at: accessibilityElement(at:)))
    }

    /// The segments among the elements UIKit's accessibility container
    /// declares, in segment order: from the leading edge of the control's own
    /// space, whatever turns it on screen, and in declared order where two
    /// share a place or the control has no window. Empty while no assistive
    /// client reads the control, since UIKit makes them only then (measured on
    /// iOS 27.1).
    func segmentElements(_ declared: [NSObject]) -> [NSObject] {
        guard declared.count == numberOfSegments, !declared.isEmpty else { return [] }
        guard let screen = window?.windowScene?.screen.coordinateSpace else { return declared }
        // An element's frame is in the screen's space
        let x = declared.map { screen.convert($0.accessibilityFrame, to: self).midX }
        let rightToLeft = effectiveUserInterfaceLayoutDirection == .rightToLeft
        let before = { (a: Int, b: Int) -> Bool in x[a] == x[b] ? a < b : (x[a] < x[b]) != rightToLeft }
        return declared.indices.sorted(by: before).map { declared[$0] }
    }

    /// Writes `id` as `element`'s accessibility identifier when it differs: a
    /// view's or an element's, since `UIView` has the property without
    /// conforming to `UIAccessibilityIdentification` at runtime (iOS 27.1).
    static func identify(_ element: NSObject, as id: String?) {
        if let view = element as? UIView {
            if view.accessibilityIdentifier != id { view.accessibilityIdentifier = id }
        } else if let element = element as? UIAccessibilityElement, element.accessibilityIdentifier != id {
            element.accessibilityIdentifier = id
        }
    }

    #if EXACT_INSPECT
    func segmentOwner(of view: UIView, owners: [UInt32]) -> UInt32? {
        guard let index = segmentViews.firstIndex(where: { view === $0 || view.isDescendant(of: $0) }),
              owners.indices.contains(index) else { return nil }
        return owners[index]
    }

    /// The segment views (`UISegment`, two containers down on iOS 27), in
    /// segment order: left to right, reversed right to left.
    var segmentViews: [UIView] {
        var found: [UIView] = []
        func walk(_ view: UIView, _ depth: Int) {
            for child in view.subviews {
                if String(describing: type(of: child)) == "UISegment" { found.append(child) } else if depth < 3 { walk(child, depth + 1) }
            }
        }
        walk(self, 0)
        let views = found.enumerated().sorted { a, b in
            let x = (a.element.convert(a.element.bounds, to: self).minX, b.element.convert(b.element.bounds, to: self).minX)
            return x.0 == x.1 ? a.offset < b.offset : x.0 < x.1
        }.map(\.element)
        return effectiveUserInterfaceLayoutDirection == .rightToLeft ? views.reversed() : views
    }
    #endif

    /// The segment under `point`, in this control's coordinates, by equal
    /// shares of the width. Segments sized by content
    /// (`apportionsSegmentWidthsByContent`) are unequal, and nothing public
    /// gives their boxes, so near a boundary there the share may name the
    /// neighbour (LLP 1035.003).
    func segment(at point: CGPoint) -> Int? {
        guard bounds.contains(point), numberOfSegments > 0 else { return nil }
        var index = Int((point.x - bounds.minX) / bounds.width * CGFloat(numberOfSegments))
        if effectiveUserInterfaceLayoutDirection == .rightToLeft { index = numberOfSegments - 1 - index }
        return (0..<numberOfSegments).contains(index) ? index : nil
    }
}

extension LabelledSegmentedControl {
    /// A title segment's accessibility label: its tab's name where that is
    /// not its words (an image segment's is its image's, `content`).
    static func segmentLabel(_ tab: NodeView) -> String? {
        guard case .title(let title)? = tab.segmentFace, tab.accessibleName != title else { return nil }
        return tab.accessibleName
    }
}
#endif
