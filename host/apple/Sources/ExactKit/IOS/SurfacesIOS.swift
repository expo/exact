#if os(iOS) || os(tvOS)
import UIKit

package struct SessionSurface {
    package enum Kind: Equatable { case document, route, dialog, popover }
    package let view: UIView
    package let label: String
    package let kind: Kind
    package var owned: [UIView] = []
    package init(view: UIView, label: String, kind: Kind, owned: [UIView] = []) { (self.view, self.label, self.kind, self.owned) = (view, label, kind, owned) }
}

extension Presenter {
    /// The session's view, viewport and root (nested trees occur only once),
    /// route mounts, dialog sheets and popover layers outside those trees. Live by default;
    /// inspection, editing and motion also need presentations still retiring.
    /// Roots are session-owned, never a window or another session's container.
    func surfaces(includingRetiring: Bool = false) -> [SessionSurface] {
        let candidates: [(UIView?, String)] = [(session?.view, "ExactView"), (viewport, "viewport"), (root, "document")]
        let own: [(UIView, String)] = candidates.compactMap { view, label in view.map { ($0, label) } }
        let documents = own.filter { candidate in
            !own.contains { other in candidate.0 !== other.0 && candidate.0.isDescendant(of: other.0) }
        }.map { SessionSurface(view: $0.0, label: $0.1, kind: .document) }
        let results = navigation.searchResultsSurface.map { [SessionSurface(view: $0, label: "search results", kind: .route)] } ?? []
        return documents + results + modals.surfaces(includingRetiring: includingRetiring)
            + dialogs.surfaces(includingRetiring: includingRetiring)
            + menus.sheetLayers.map { SessionSurface(view: $0, label: "popover over a sheet", kind: .popover) }
    }

    /// The views of the surfaces beside the document.
    func presentations(includingRetiring: Bool = false) -> [UIView] {
        surfaces(includingRetiring: includingRetiring).filter { $0.kind != .document }.map(\.view)
    }

    /// The innermost surface containing a view; the document's scrolling
    /// port is still `viewport`, even when its root is the session's view.
    package func surface(containing view: UIView, includingRetiring: Bool = true) -> SessionSurface? {
        surfaces(includingRetiring: includingRetiring).last { view === $0.view || view.isDescendant(of: $0.view) }
    }

    /// UIKit's accessibility hiding rule applies across presentation-container
    /// siblings. Include those wrappers for AX only, without entering a window
    /// or enclosing the session's own view. The AX walker deduplicates objects.
    var accessibilitySurfaces: [UIView] {
        let all = surfaces(includingRetiring: true)
        return all.map { surface in
            guard surface.kind == .route else { return surface.view }
            var view = surface.view
            while let parent = view.superview, !(parent is UIWindow),
                  !all.contains(where: { $0.kind == .document && $0.view.isDescendant(of: parent) }) { view = parent }
            return view
        }
    }
}

/// A lifted popover belongs to its sheet even though UIKit places its layer
/// beside the sheet's controller view, outside that controller's responders.
package enum ViewPresentation {
    package static func isPresented(_ controller: UIViewController, above ancestor: UIViewController) -> Bool {
        sequence(first: controller.presentingViewController, next: { $0?.presentingViewController })
            .contains { $0 === ancestor }
    }
    package static func controller(for view: UIView, fallbackToWindow: Bool = false, layout: Bool = false) -> UIViewController? {
        if let sheet = PopoverLayer.containing(view)?.sheet { return sheet }
        var responder: UIResponder? = view
        while let current = responder {
            if !layout, let node = current as? NodeView { node.presenter?.navigation.finishBootOwner() }
            if !layout, let view = current as? ExactView { view.session.presenter.navigation.finishBootOwner() }
            // Ask only a controller for the navigation protocols, which only a controller conforms to: the first
            // protocol cast on a class scans every loaded image's conformances, and a fold puts new UIKit views here
            if let controller = current as? UIViewController {
                if let holder = controller as? any BootHolding, holder.bootContent, let route = holder.routeSurface {
                    if !layout { route.node.presenter?.navigation.finishBootOwner(); return Self.controller(for: view, fallbackToWindow: fallbackToWindow, layout: true) }
                    return route
                }
                if !layout, let route = controller as? any RouteSurface, route.bootSurface != nil { route.node.presenter?.navigation.finishBootOwner() }
                return controller
            }
            responder = current.next
        }
        return fallbackToWindow ? view.window?.rootViewController : nil
    }

    /// The controllers on the responder chain from the owner, nearest first:
    /// its containment ancestors, then those presenting it.
    package static func controllers(for view: UIView, layout: Bool = false) -> [UIViewController] {
        guard let controller = controller(for: view, layout: layout) else { return [] }
        return sequence(first: controller as UIResponder, next: { responder in
            if let controller = responder as? UIViewController, let route = controller as? any RouteSurface, let surface = route.bootSurface { return surface.next }
            return responder.next
        }).compactMap { $0 as? UIViewController }
    }

    /// The owner, followed through presentations that are not being dismissed;
    /// nil out of a window, where UIKit refuses to present and calls back nothing.
    package static func presenter(for view: UIView) -> UIViewController? {
        var owner = controller(for: view, fallbackToWindow: true, layout: false)
        guard view.window != nil else { return nil }
        while let presented = owner?.presentedViewController, !presented.isBeingDismissed { owner = presented }
        return owner?.viewIfLoaded?.window == nil ? nil : owner
    }

    /// A flight stays in its presentation's paint plane. Above a sheet that
    /// is the popover layer, not the clipped card or the window around it.
    package static func root(of view: UIView) -> UIView? {
        if let layer = PopoverLayer.containing(view), layer.sheet != nil { return layer }
        return controllers(for: view, layout: true).first {
            $0.parent == nil || $0.parent is UINavigationController || $0.parent is BootHolding || $0.parent is UITabBarController
        }.map { ($0 as? any RouteSurface)?.surface ?? $0.view } ?? view.window
    }
}
#endif
