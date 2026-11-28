// The session's update on iOS (LLP 1123 D2): what the session view reports in the document's space, which the boxes
// outside every owner read, sampled from the same UIKit values as an owner's update.
#if os(iOS)
import UIKit

extension Presenter {
    /// The session's update for the session view `view`, in the document's space as the kernel reads each field.
    ///
    /// The band is how far the session view's unsafe area reaches into the document on each edge (`band(document:safe:)`).
    /// The targets are the chain of the controller holding the session view, its own entry that controller's view, and
    /// the relations beside it, the session view among them; with no controller the chain is empty. The keyboard and
    /// the reserved regions are where a person sees them in the document (`documentRect`), as an owner's are in its
    /// view. No window map is reported: the runner applies the session's map to the keyboard and the regions, where a
    /// person sees them, and to laid-out frames alike, which a scrolled page tells apart, so a map waits for the runner
    /// to take the page's scroll off laid-out frames. The policy is always sent, `resizes-content` where the page names
    /// none, which is UIKit's own behavior. Out of a window there is no keyboard, configuration or regions, and the
    /// chain's targets have no frame; with no controller holding the session view the chain is empty, the session
    /// view still a relation.
    package func sessionUpdate(of view: UIView) -> SessionUpdateWire {
        var update = SessionUpdateWire()
        let safe = viewport.convert(view.bounds.inset(by: view.safeAreaInsets), from: view)
        update.band = Self.band(document: viewport.bounds.inset(by: pageMargins), safe: safe)
        var responder = view.next
        while let next = responder, !(next is UIViewController) { responder = next.next }
        let controller = responder as? UIViewController
        let targets = OwnerUpdateIOS.targets(controller: controller, view: controller?.viewIfLoaded ?? view, session: view)
        update.chain = targets.chain.map { (OwnerUpdateIOS.wire($0.target), $0.kinds) }
        update.relations = targets.relations.map { ($0.relation, OwnerUpdateIOS.wire($0.target)) }
        let policy: SessionUpdateWire.KeyboardPolicy = switch keyboardPolicy {
        case "resizes-visual": .resizesVisual
        case "overlays-content": .overlaysContent
        default: .resizesContent
        }
        update.keyboardPolicy = policy
        guard let window = view.window else { return update }
        if let sampler = KeyboardSampleIOS.existing(in: window) {
            update.keyboard = sampler.sample.map { .shown(frame: documentRect($0.frame, from: window), docked: $0.docked) } ?? .dismissed
        }
        if let controller {
            let tabs = sequence(first: controller, next: \.parent).lazy.compactMap { $0 as? UITabBarController }.first
            update.configuration = OwnerUpdateIOS.configuration(of: controller, view: view, tabs: tabs)
        }
        update.regions = ReservedRegions.all(of: view)?.map { documentRegion($0, of: view) }
        return update
    }

    /// `region`, as `ReservedRegions.all(of: view)` gives it, where a person sees it in the document: its margins
    /// converted with its frame, so they scale with it under the agent's explicit size.
    package func documentRegion(_ region: ReservedRegions.Region, of view: UIView) -> OwnerUpdateWire.Region {
        let frame = region.frame.offsetBy(dx: view.bounds.minX, dy: view.bounds.minY)
        let whole = documentRect(frame, from: view), inner = documentRect(frame.inset(by: region.margins), from: view)
        let margins = OwnerUpdateWire.Edges(top: inner.minY - whole.minY, right: whole.maxX - inner.maxX, bottom: whole.maxY - inner.maxY, left: inner.minX - whole.minX)
        return .init(id: region.id, kind: region.kind, frame: whole, margins: margins, active: region.active)
    }

    /// `rect` in `view`'s coordinates (the window's for `nil`) where a person sees it in the document: in the viewport,
    /// less the page's origin, its scroll and margins (`viewportPoints`), scaled as the viewport is.
    package func documentRect(_ rect: CGRect, from view: UIView?) -> CGRect {
        viewportPoints(viewport.convert(rect, from: view))
    }

    /// How far the unsafe area reaches into `document` on each edge, both in one space: `safe` is the safe area's
    /// rectangle. The safe area where the document takes the whole view, none where the document is already inside it.
    package static func band(document: CGRect, safe: CGRect) -> OwnerUpdateWire.Edges {
        let reach = { (depth: CGFloat, extent: CGFloat) in min(max(0, depth), extent) }
        return .init(top: reach(safe.minY - document.minY, document.height), right: reach(document.maxX - safe.maxX, document.width),
                     bottom: reach(document.maxY - safe.maxY, document.height), left: reach(safe.minX - document.minX, document.width))
    }
}
#endif
