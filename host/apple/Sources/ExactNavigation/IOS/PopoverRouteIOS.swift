// A route presented as a popover (LLP 1123 D7, `navigationPresentation="popover"`): UIKit's popover,
// anchored to the element its `navigationPopoverAnchor` names and shaped by its other choices, each
// UIKit's property while the route says it and UIKit's default once it stops; in a compact-width
// window a sheet, as UIKit adapts it, unless the route says `none`. Its content is laid out beside
// the arrow, not under it, so the arrow takes none of its safe area (the evidence row "A popover's
// arrow").
#if os(iOS)
import ExactKit
import UIKit

final class PopoverRoute: NSObject, UIPopoverPresentationControllerDelegate {
    private weak var host: ModalHost?
    private weak var route: NodeView?
    /// Whether its anchor has left since it was presented, so it closes once.
    private var lost = false
    /// UIKit's margins, as its popover had them before the route's, which it keeps once the route stops saying its own.
    private var platformMargins: UIEdgeInsets?

    init(host: ModalHost, route: NodeView) {
        self.host = host
        self.route = route
    }

    /// The element `route`'s `navigationPopoverAnchor` names, in a window and outside the popover:
    /// of several with that id, the first made.
    static func anchor(of route: NodeView, in presenter: Presenter) -> NodeView? {
        guard let name = route.props["navigationPopoverAnchor"], !name.isEmpty else { return nil }
        return presenter.views.values.filter { $0.props["id"] == name && $0.window != nil && !$0.isDescendant(of: route) }
            .min { $0.id < $1.id }
    }

    /// Give `source` the anchor and the route's choices, UIKit's defaults where it says none: its
    /// arrows any, its margins UIKit's, no overlap of the anchor, and the size UIKit gives a popover
    /// unless the route's box says its width and height in points.
    func configure(_ source: UIPopoverPresentationController) {
        guard let route, let presenter = host?.presenter else { return }
        if let anchor = Self.anchor(of: route, in: presenter), source.sourceView !== anchor {
            source.sourceView = anchor
            source.sourceRect = .null
        }
        let arrows = Self.arrows(route.props["navigationPopoverArrows"])
        if source.permittedArrowDirections != arrows { source.permittedArrowDirections = arrows }
        if platformMargins == nil { platformMargins = source.popoverLayoutMargins }
        if let margins = Self.margins(route.props["navigationPopoverMargins"]) ?? platformMargins, source.popoverLayoutMargins != margins {
            source.popoverLayoutMargins = margins
        }
        let overlaps = route.props["navigationPopoverOverlapsSource"] == "true"
        if source.canOverlapSourceViewRect != overlaps { source.canOverlapSourceViewRect = overlaps }
        let size = Self.size(of: route)
        if source.presentedViewController.preferredContentSize != size { source.presentedViewController.preferredContentSize = size }
    }

    /// Whether its anchor has left its window since it was presented, said once.
    func anchorLost(in presenter: Presenter) -> Bool {
        guard !lost, let route, Self.anchor(of: route, in: presenter) == nil else { return false }
        lost = true
        return true
    }

    /// `navigationPopoverArrows`' words as UIKit's directions; unsaid or `any`, any.
    static func arrows(_ words: String?) -> UIPopoverArrowDirection {
        guard let words, words != "any" else { return .any }
        var arrows: UIPopoverArrowDirection = []
        for word in words.split(separator: " ") {
            switch word {
            case "up": arrows.insert(.up)
            case "down": arrows.insert(.down)
            case "left": arrows.insert(.left)
            case "right": arrows.insert(.right)
            default: break
            }
        }
        return arrows.isEmpty ? .any : arrows
    }

    /// `navigationPopoverMargins`' lengths as `padding` expands one to four; `nil` unsaid or malformed.
    static func margins(_ value: String?) -> UIEdgeInsets? {
        guard let value else { return nil }
        let l = value.split(separator: " ").compactMap { Double($0) }.map { CGFloat($0) }
        switch l.count {
        case 1: return UIEdgeInsets(top: l[0], left: l[0], bottom: l[0], right: l[0])
        case 2: return UIEdgeInsets(top: l[0], left: l[1], bottom: l[0], right: l[1])
        case 3: return UIEdgeInsets(top: l[0], left: l[1], bottom: l[2], right: l[1])
        case 4: return UIEdgeInsets(top: l[0], left: l[3], bottom: l[2], right: l[1])
        default: return nil
        }
    }

    /// The popover's size: the route's box's width and height where it says them in points, and
    /// otherwise none, UIKit's size.
    static func size(of route: NodeView) -> CGSize {
        let points = { (row: String) -> CGFloat? in
            guard route.wrote(row), let value = route.style[row], value.string == nil else { return nil }
            return value.length(basis: 0).flatMap { $0 > 0 ? $0 : nil }
        }
        guard let width = points("width"), let height = points("height") else { return .zero }
        return CGSize(width: width, height: height)
    }

    // MARK: UIPopoverPresentationControllerDelegate

    /// A popover a compact-width window shows as UIKit adapts it, a sheet, unless the route says
    /// `none`: `automatic` adapts as UIKit does with no delegate (the evidence row "A popover's
    /// adaptation").
    func adaptivePresentationStyle(for controller: UIPresentationController, traitCollection: UITraitCollection) -> UIModalPresentationStyle {
        route?.props["navigationPopoverAdaptation"] == "none" ? .none : .automatic
    }

    func presentationControllerShouldDismiss(_ presentationController: UIPresentationController) -> Bool {
        host?.presentationControllerShouldDismiss(presentationController) ?? true
    }
    func presentationControllerDidAttemptToDismiss(_ presentationController: UIPresentationController) {
        host?.presentationControllerDidAttemptToDismiss(presentationController)
    }
    func presentationControllerWillDismiss(_ presentationController: UIPresentationController) {
        host?.presentationControllerWillDismiss(presentationController)
    }
    func presentationControllerDidDismiss(_ presentationController: UIPresentationController) {
        host?.presentationControllerDidDismiss(presentationController)
    }
}
#endif
