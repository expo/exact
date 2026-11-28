import ExactKit
#if os(iOS) || os(tvOS)
import UIKit

// Keep the detail button while UIKit refreshes the cell for accessibility.
final class GroupedCell: UICollectionViewListCell {
    /// The row's content margins and background insets (LLP 1123 D15), given to the content and
    /// background configurations the cell holds after each of its configuration updates, Exact's
    /// and UIKit's for its state; `nil` keeps UIKit's.
    var listInsets: (margins: NSDirectionalEdgeInsets?, background: NSDirectionalEdgeInsets?) = (nil, nil) {
        didSet { if listInsets != oldValue { setNeedsUpdateConfiguration() } }
    }
    /// How far a plain list's cell paints past its frame at its left and right, over the side safe area UIKit does not
    /// give the list (`GroupedCollectionView.sideBand`): its pressed and selected ground goes there, as a plain list's
    /// cell runs under the safe area UIKit gives it, while its content, its accessories and its touches stay where the
    /// list lays them out (LLP 1123 D13, D15). UIKit draws a background configuration only inside the cell, a negative
    /// inset included, so the ground past the frame is a strip of the cell's own at each side (`paintGroundReach`).
    /// Interim, until the kernel's reach (stage 5) lays the collection across the band, as `GroupedListView.sideReach`
    /// and `SideReachIOS.swift` are.
    var sideReach: (left: CGFloat, right: CGFloat) = (0, 0) {
        didSet { if sideReach != oldValue { paintGroundReach() } }
    }
    private var groundReach: (left: UIView?, right: UIView?) = (nil, nil)
    /// The cell's background configuration, which Exact sets only through here so the strips past its frame follow it
    /// (`sideReach`): UIKit gives no hook on `backgroundConfiguration`, and a configuration update handler sets it after
    /// `updateConfiguration(using:)` returns. UIKit's own update of the ground for the state, where no handler sets it,
    /// is in place as `updateConfiguration(using:)` runs, which paints the strips after it.
    var ground: UIBackgroundConfiguration? {
        get { backgroundConfiguration }
        set {
            backgroundConfiguration = newValue
            paintGroundReach()
        }
    }
    override func tintColorDidChange() {
        super.tintColorDidChange()
        paintGroundReach()
    }
    override func updateConfiguration(using state: UICellConfigurationState) {
        super.updateConfiguration(using: state)
        if let margins = listInsets.margins, var c = contentConfiguration as? UIListContentConfiguration, c.directionalLayoutMargins != margins {
            c.directionalLayoutMargins = margins
            contentConfiguration = c
        }
        if let insets = listInsets.background, var b = backgroundConfiguration, b.backgroundInsets != insets {
            b.backgroundInsets = insets
            ground = b
        }
        paintGroundReach()
    }
    /// Continues the ground past the frame at each side the list's band reaches (`sideReach`) where the ground meets that
    /// edge, with no inset there and square corners, in the color it resolves to now (clear at rest). A strip's color
    /// changes with any animation the ground's change runs in; its frame is placed without one. A ground inset from that
    /// edge stays inside the cell, measured from the cell's edge rather than the band's, until the kernel's reach lays
    /// the cell across the band (LLP 1123 D13).
    private func paintGroundReach() {
        let ground = backgroundConfiguration
        let color = ground.flatMap { $0.cornerRadius > 0 ? nil : $0.resolvedBackgroundColor(for: tintColor) }
        let insets = ground?.backgroundInsets ?? .zero
        let rtl = effectiveUserInterfaceLayoutDirection == .rightToLeft
        let meets = (left: (rtl ? insets.trailing : insets.leading) <= 0, right: (rtl ? insets.leading : insets.trailing) <= 0)
        groundReach.left = strip(groundReach.left, color: sideReach.left > 0 && meets.left ? color : nil)
        groundReach.right = strip(groundReach.right, color: sideReach.right > 0 && meets.right ? color : nil)
        UIView.performWithoutAnimation(layoutGroundReach)
    }
    private func strip(_ view: UIView?, color: UIColor?) -> UIView? {
        guard let color else {
            view?.removeFromSuperview()
            return nil
        }
        let strip = view ?? UIView()
        if view == nil {
            strip.isUserInteractionEnabled = false
            strip.accessibilityElementsHidden = true
            insertSubview(strip, at: 0)
        }
        strip.backgroundColor = color
        return strip
    }
    /// Where the cell's ground may show, in its own coordinates: its bounds and the strips past them (`sideReach`).
    var reachedBounds: CGRect {
        [groundReach.left, groundReach.right].compactMap { $0?.frame }.reduce(bounds) { $0.union($1) }
    }
    /// The outline a context menu lifts the row in while a strip shows a ground past the cell, as UIKit lifts a cell
    /// that runs under the safe area: the cell's bounds and each strip with a color; nil while none has one.
    var groundPath: UIBezierPath? {
        let shown = [groundReach.left, groundReach.right].compactMap { $0 }
            .filter { !$0.frame.isEmpty && ($0.backgroundColor?.resolvedColor(with: traitCollection).cgColor.alpha ?? 0) > 0 }
        guard !shown.isEmpty else { return nil }
        let path = UIBezierPath(rect: bounds)
        for strip in shown { path.append(UIBezierPath(rect: strip.frame)) }
        return path
    }
    private func layoutGroundReach() {
        let insets = backgroundConfiguration?.backgroundInsets ?? .zero
        // The ground's span down the cell, none where its insets meet or cross (a negative height would draw)
        let top = max(0, insets.top), height = max(0, bounds.height - top - max(0, insets.bottom))
        groundReach.left?.frame = CGRect(x: bounds.minX - sideReach.left, y: bounds.minY + top, width: sideReach.left, height: height)
        groundReach.right?.frame = CGRect(x: bounds.maxX, y: bounds.minY + top, width: sideReach.right, height: height)
    }
    private var activeBounds = CGRect.null
    var refreshActiveOnLayout = false
    override func layoutSubviews() {
        super.layoutSubviews()
        layoutGroundReach()
        #if os(iOS)
        updateGripActions()
        #endif
        guard bounds != activeBounds else { return }
        activeBounds = bounds
        if refreshActiveOnLayout { setNeedsUpdateConfiguration() }
    }
    override func layoutMarginsDidChange() {
        super.layoutMarginsDidChange()
        (superview as? GroupedCollectionView)?.marginsChanged()
    }
    var forcedPressed = false { didSet { if forcedPressed != oldValue { setNeedsUpdateConfiguration() } } }
    override var configurationState: UICellConfigurationState {
        var state = super.configurationState
        state.isHighlighted = state.isHighlighted || forcedPressed
        return state
    }
    #if os(iOS)
    weak var grip: NodeView?
    private var takesReorderKeys: Bool { grip.map { $0.presenter?.reorders.takesKeys($0) == true } ?? false }
    override var canBecomeFocused: Bool { takesReorderKeys || super.canBecomeFocused }
    override var canBecomeFirstResponder: Bool { takesReorderKeys || super.canBecomeFirstResponder }
    private weak var actionGrip: NodeView?
    private(set) weak var reorderControl: UIView?
    private var moveActions: [UIAccessibilityCustomAction] = []

    /// UIKit may replace its reorder button on layout. The content, switch and detail
    /// button have their own actions. Only the remaining accessory button moves the row.
    private func updateGripActions() {
        guard grip != nil || reorderControl != nil else { return }
        let control: UIView? = grip == nil ? nil : firstInTree({ view in
            view !== self && view.isAccessibilityElement && view.accessibilityTraits.contains(.button) ? view : nil
        }, prune: { $0 === self.contentView || $0 === self.toggle || $0 === self.detailButton || $0.isHidden })
        guard control !== reorderControl || grip !== actionGrip else { return }
        if let old = reorderControl {
            NodeView.clearReferencedActions(from: old)
            old.accessibilityCustomActions = old.accessibilityCustomActions?.filter { action in !moveActions.contains { $0 === action } }
        }
        actionGrip = nil
        reorderControl = nil
        moveActions = []
        guard let control, let grip, let actions = grip.presenter?.reorders.actions(grip) else { return }
        actionGrip = grip
        reorderControl = control
        moveActions = actions
        control.accessibilityCustomActions = (control.accessibilityCustomActions ?? []) + actions
        grip.syncProjectedActions(control)
    }
    override func pressesBegan(_ presses: Set<UIPress>, with event: UIPressesEvent?) {
        guard let grip, isFocused || isFirstResponder, let reorders = grip.presenter?.reorders else {
            return super.pressesBegan(presses, with: event)
        }
        let rest = presses.filter { press in
            guard let key = press.key, key.modifierFlags.intersection([.shift, .control, .alternate, .command]).isEmpty else { return true }
            let name: String
            switch key.keyCode {
            case .keyboardSpacebar: name = " "
            case .keyboardUpArrow: name = "ArrowUp"
            case .keyboardDownArrow: name = "ArrowDown"
            case .keyboardReturnOrEnter: name = "Enter"
            case .keyboardEscape: name = "Escape"
            default: return true
            }
            return !reorders.key(grip, name)
        }
        if !rest.isEmpty { super.pressesBegan(Set(rest), with: event) }
    }
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
    /// The color of its own its standard row's image is drawn in, nil for the cell's tint, which
    /// UIKit draws a cell's image in and greys behind a sheet or an alert
    var imageInk: UIColor?
    /// `image` drawn in `imageInk`, which UIKit draws as it is (`.alwaysOriginal`), dynamic colors
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
extension GroupedCell: GroupedMenuSurface {
    var liftOutline: UIBezierPath? { groundPath }
}

extension GroupedCell: InspectionOwned {
    /// A toggle row's element is its switch's: `tree --ax` joins it to the control.
    package var inspectionViewId: UInt32? { accessibleToggle == nil ? nil : target }
}
#endif
