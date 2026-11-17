import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

// Keep the detail button while UIKit refreshes the cell for accessibility.
final class GroupedCell: UICollectionViewListCell {
    private var activeBounds = CGRect.null
    var refreshActiveOnLayout = false
    override func layoutSubviews() {
        super.layoutSubviews()
        guard bounds != activeBounds else { return }
        activeBounds = bounds
        if refreshActiveOnLayout { setNeedsUpdateConfiguration() }
    }
    var forcedPressed = false { didSet { if forcedPressed != oldValue { setNeedsUpdateConfiguration() } } }
    override var configurationState: UICellConfigurationState {
        var state = super.configurationState
        state.isHighlighted = state.isHighlighted || forcedPressed
        return state
    }
    #if os(iOS)
    weak var menuSource: NodeView?
    private var detailButton: UIButton?
    private var detailAction: (() -> Void)?

    func detailAccessory(enabled: Bool, action: @escaping () -> Void) -> UICellAccessory {
        detailAction = action
        let button = detailButton ?? {
            let button = UIButton(type: .detailDisclosure)
            button.addAction(UIAction { [weak self] _ in self?.detailAction?() }, for: .touchUpInside)
            button.sizeToFit()
            detailButton = button
            return button
        }()
        button.isEnabled = enabled
        button.removeFromSuperview()
        return .customView(configuration: .init(customView: button, placement: .trailing(displayed: .always)))
    }
    /// Whether its detail button takes a press (`GroupedListView.detailTarget`).
    var detailEnabled: Bool {
        get { detailButton?.isEnabled ?? false }
        set { if let detailButton, detailButton.isEnabled != newValue { detailButton.isEnabled = newValue } }
    }

    /// Whether the cell speaks for its row: UIKit draws the row's accessory, so the cell is the
    /// row's one element, as each row of Settings is (`GroupedAccessoriesIOS.swift`). UIKit makes
    /// a list cell an element only while it has content or a label of its own, and a cell's custom
    /// accessory is not part of it on every iOS (17's VoiceOver read a switch after its row, with
    /// no name), so this cell says so itself.
    var speaksForRow = false
    /// Its row's switch: the cell has its value and traits, and activating the cell flips it.
    /// One its author hid (`aria-hidden`) adds nothing, and the cell is activated as a touch on
    /// the row is. UIKit hides only the switch's own element: it still reads the switch's value
    /// and traits into the cell and activates the cell at the switch (iOS 27).
    weak var toggle: UISwitch?
    /// Checks its row's radio, where the row has one in the accessibility tree.
    var checkRadio: (() -> Bool)?
    private var hidesToggle: Bool { toggle?.accessibilityElementsHidden == true }
    private var accessibleToggle: UISwitch? { hidesToggle ? nil : toggle }
    override var isAccessibilityElement: Bool {
        get { speaksForRow || super.isAccessibilityElement }
        set { super.isAccessibilityElement = newValue }
    }
    override var accessibilityValue: String? {
        get { hidesToggle ? nil : toggle?.accessibilityValue ?? super.accessibilityValue }
        set { super.accessibilityValue = newValue }
    }
    override var accessibilityActivationPoint: CGPoint {
        get {
            guard hidesToggle else { return super.accessibilityActivationPoint }
            let frame = accessibilityFrame
            return CGPoint(x: frame.midX, y: frame.midY)
        }
        set { super.accessibilityActivationPoint = newValue }
    }
    override func accessibilityActivate() -> Bool {
        guard let toggle = accessibleToggle else { return checkRadio?() ?? super.accessibilityActivate() }
        if toggle.isEnabled {
            toggle.setOn(!toggle.isOn, animated: true)
            toggle.sendActions(for: .valueChanged)
        }
        return true
    }
    private var switchTraits: UIAccessibilityTraits { accessibleToggle?.accessibilityTraits ?? [] }
    #else
    private var switchTraits: UIAccessibilityTraits { [] }
    /// Whether its detail accessory is drawn as one that takes a press (`GroupedListView.detailTarget`).
    var detailEnabled = true
    #endif

    /// Its traits, which the host sets (`GroupedListView.accessibility`, `interact`), with its
    /// switch's. UIKit's accessibility answers a list cell's traits from its subviews: it reads a
    /// checkmark-like symbol (`checkmark.circle`) as `.selected`, and a switch's traits in, so an
    /// insert or remove would write them back, and they would outlast the row.
    private var own: UIAccessibilityTraits = []
    override var accessibilityTraits: UIAccessibilityTraits {
        get { own.union(switchTraits) }
        // An insert or remove reads the union first: what only the switch says stays the switch's,
        // except the `.notEnabled` that `interact` marks the cell with itself.
        set {
            var switchOnly = switchTraits.subtracting(own)
            if markedNotEnabled { switchOnly.remove(.notEnabled) }
            own = newValue.subtracting(switchOnly)
        }
    }

    var row: UInt32?
    /// Whether `interact` put `.notEnabled` in its traits, set before it does.
    var markedNotEnabled = false
    /// The node of its row's switch or detail button.
    var target: UInt32?
    var height: CGFloat?
    /// An authored `min-height` for a cell UIKit's configuration sizes.
    var minimumHeight: CGFloat?
    /// Its standard row's svg-only icon path token, drawn again at a new
    /// Dynamic Type size (stage 9 A2); nil for an SF Symbol or none.
    var pathSymbol: String? {
        didSet {
            guard pathSymbol != nil, !watchesSize else { return }
            watchesSize = true
            registerForTraitChanges([UITraitPreferredContentSizeCategory.self]) { (cell: GroupedCell, _: UITraitCollection) in cell.redrawPath() }
        }
    }
    private var watchesSize = false
    /// A path's side: the body text's size under these traits, as a symbol
    /// with no configuration in a list cell takes.
    static func pathPoints(_ traits: UITraitCollection) -> CGFloat {
        UIFont.preferredFont(forTextStyle: .body, compatibleWith: traits).pointSize
    }
    func redrawPath() {
        guard let pathSymbol, var c = contentConfiguration as? UIListContentConfiguration else { return }
        c.image = inked(FaceSymbol.image(pathSymbol, points: Self.pathPoints(traitCollection)))
        contentConfiguration = c
    }
    /// The colour of its own its standard row's image is drawn in, nil for the cell's tint, which
    /// UIKit draws a cell's image in and greys behind a sheet or an alert
    var imageInk: UIColor?
    /// `image` drawn in `imageInk`, which UIKit draws as it is (`.alwaysOriginal`), dynamic colours
    /// per trait; else as it is, for the cell's tint
    func inked(_ image: UIImage?) -> UIImage? {
        imageInk.flatMap { ink in image?.withTintColor(ink, renderingMode: .alwaysOriginal) } ?? image
    }
    /// Its standard row's content. UIKit's list content view hides a label
    /// a new configuration drops but keeps its text, which UIKit's
    /// accessibility still reads: a reused cell's Privacy Policy row read
    /// the Font row's "Georgia" to VoiceOver (iOS 27). A cell that drops a
    /// text starts from a new content view.
    func show(_ c: UIListContentConfiguration) {
        if let old = contentConfiguration as? UIListContentConfiguration,
           old.secondaryText != nil && c.secondaryText == nil || old.text != nil && c.text == nil {
            contentConfiguration = nil
        }
        contentConfiguration = c
    }
    #if os(iOS)
    /// A touch its row leaves to no control goes to the row's one field: the
    /// cell is the field (LLP 1116.005 D2).
    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? {
        NodeView.cellField(in: self, hit: super.hitTest(point, with: event))
    }
    #endif
    override func preferredLayoutAttributesFitting(_ attributes: UICollectionViewLayoutAttributes) -> UICollectionViewLayoutAttributes {
        if let height {
            let fitted = attributes.copy() as! UICollectionViewLayoutAttributes
            fitted.size.height = height
            return fitted
        }
        let fitted = super.preferredLayoutAttributesFitting(attributes)
        guard let least = minimumHeight, fitted.size.height < least else { return fitted }
        let raised = fitted.copy() as! UICollectionViewLayoutAttributes
        raised.size.height = least
        return raised
    }
}

extension UICollectionViewListCell {
    /// A row's or header's margins are the list layout's, never the safe
    /// area's: a cell under the home indicator (a fling past the end)
    /// otherwise grew by the inset, moved, and shrank a pixel a layout pass
    /// until UIKit's feedback-loop check stopped the app. The layout's
    /// sections already keep clear of the sides' safe area (its
    /// `contentInsetsReference`). Again after each configuration, which may
    /// replace the content view.
    func marginsIgnoreSafeArea() {
        insetsLayoutMarginsFromSafeArea = false
        contentView.insetsLayoutMarginsFromSafeArea = false
    }
}
#endif

#if os(iOS)
extension GroupedCell: GroupedMenuSurface {}

extension GroupedCell: InspectionOwned {
    /// A toggle row's element is its switch's: `tree --ax` joins it to the control.
    package var inspectionViewId: UInt32? { accessibleToggle == nil ? nil : target }
}
#endif
