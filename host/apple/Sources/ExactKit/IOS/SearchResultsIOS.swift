#if os(iOS) || os(tvOS)
import UIKit

/// A header search's results as a level of the stack: the route that holds the element its field names by
/// `aria-controls`, the router's route like any other. UIKit shows it as it shows any route, pushed, a split's
/// detail included, but where the field's own route is on top of its navigation controller as the results arrive
/// (a closed iPhone Duo's list, a stack's root): there they are drawn in that route's controller, over its whole
/// view, and the field stays in its bar to type in. UIKit hides that bar while its search is open
/// (`hidesNavigationBarDuringPresentation`), so the results take its area too, as UIKit's own results controller
/// does.
extension NavigationHost {
    /// The route in `stack` that holds the element `field` names by `aria-controls`, other than the field's own.
    func resultsRoute(of field: NodeView?, in stack: [RouteController]) -> RouteController? {
        guard let field, let name = field.props["accessibilityControls"], !name.isEmpty else { return nil }
        return stack.first { $0.search?.field !== field && element(named: name, in: $0.node) != nil }
    }

    /// `stack` as UIKit is to show it in `nav`: less a results route drawn in its field's route's controller.
    /// Directly above that route, results already drawn there stay; results arriving while that route is alone in
    /// `nav` (a stack's root on top) are drawn there; anywhere else, and wherever `inPlace` is false (an expanded
    /// split, whose detail they are), they are UIKit's to push.
    func placingResults(_ stack: [RouteController], on nav: UINavigationController?, inPlace: Bool) -> [RouteController] {
        var placed: RouteController?
        if inPlace {
            for (index, list) in stack.enumerated() where index + 1 < stack.count {
                guard let results = resultsRoute(of: list.search?.field, in: stack), results === stack[index + 1] else { continue }
                if results.parent === list || (results.parent == nil && nav.map { $0.viewControllers.count == 1 && $0.viewControllers[0] === list } == true) { placed = results }
                break
            }
        }
        if let drawn = resultsInPlace, drawn !== placed { detach(drawn) }
        if let placed, let list = stack.first(where: { $0.search?.field.map { resultsRoute(of: $0, in: stack) === placed } == true }) {
            attach(placed, to: list)
        }
        resultsInPlace = placed
        return placed.map { placed in stack.filter { $0 !== placed } } ?? stack
    }

    /// The key of the route the person sees where UIKit shows `controller`: results drawn in it are on top of it.
    func shownKey(of controller: UIViewController?) -> String? {
        if let results = resultsInPlace, results.parent === controller { return results.key }
        return (controller as? RouteController)?.key
    }

    /// Whether the router's stack holds the results route `field` names.
    func showsResults(of field: NodeView) -> Bool {
        resultsRoute(of: field, in: controllers.values.filter { shownKeys.contains($0.key) }) != nil
    }

    /// Whether the route on top of the router's stack is the results route `field` names.
    func selectsResults(of field: NodeView) -> Bool {
        guard let top = controllers.values.first(where: { $0.key == selectedKey }) else { return false }
        return resultsRoute(of: field, in: [top]) === top
    }

    /// Settles every header search after the stacks changed (`HeaderSearch.settle`): one whose results a Back or a
    /// page took away closes, keeping its text.
    func settleSearches() {
        for search in controllers.values.compactMap(\.search) { search.settle() }
    }

    /// The search whose results route is `route`.
    func search(over route: RouteController) -> HeaderSearch? {
        controllers.values.lazy.compactMap(\.search).first { search in search.field.map { resultsRoute(of: $0, in: [route]) === route } == true }
    }

    /// The search whose results route is `route`, while its field is edited: the route takes the keys as safe area,
    /// as the field's own route does (`RouteController.fitContentViewport`).
    func searchEditing(over route: RouteController) -> HeaderSearch? {
        guard let search = searchEditingRoute?.search, let field = search.field, resultsRoute(of: field, in: [route]) === route else { return nil }
        return search
    }

    /// The route whose bar search field is being edited, as UIKit's first responder says.
    private var searchEditingRoute: RouteController? {
        controllers.values.first { $0.search?.editor.isFirstResponder == true }
    }

    /// The route whose bar search is in use: its field edits, or UIKit is moving its bar into search (when UIKit's own
    /// end of editing can leave the field for a moment, RFC §1 item 7).
    var searchInUseRoute: RouteController? {
        controllers.values.first { c in c.search.map { $0.presenting || $0.editor.isFirstResponder } == true }
    }

    /// The search whose field's editing a write of `stack` ends: UIKit makes a bar's search field the first responder
    /// again when its route shows after a push or a fold covered it while it edited, and ended first it stays as the
    /// person left it, the keyboard down. So the host ends it before UIKit moves anything over the field's route: a
    /// route written above it, its results apart; and on an expanded split, where nothing covers the list, a person's
    /// press that changes the detail (a result, a sidebar page), as a hand-built split puts the keys away. (The
    /// columns collapsing onto the detail is the third such move, `SplitIOS.listWillBeCovered`.) A location that comes
    /// with no press (a link, a test's `type`) moves no keyboard. `pressed`: the write is a person's press's work.
    func searchCovered(by stack: [RouteController], pressed: Bool) -> HeaderSearch? {
        guard let list = searchInUseRoute, let search = list.search else { return nil }
        let results = resultsRoute(of: search.field, in: stack)
        let rest = stack.filter { $0 !== results }
        guard let at = rest.firstIndex(where: { $0 === list }) else { return nil }
        if let split, split.settledExpanded {
            let detail = Array(rest.dropFirst()), shown = split.shown.dropFirst().filter { $0 !== results }
            return pressed && detail.map(ObjectIdentifier.init) != shown.map(ObjectIdentifier.init) ? search : nil
        }
        return at < rest.count - 1 ? search : nil
    }

    /// The platform's Back from a search's results ends its field's editing before the router hears it, as a push
    /// over the field's route does; the search then closes, keeping its text (`HeaderSearch.settle`). Every Back the
    /// host makes or hears goes through `goBack`, which asks this first.
    func endEditingLeavingResults() {
        guard let selected = selectedKey, let results = controllers.values.first(where: { $0.key == selected }) else { return }
        searchEditing(over: results)?.resign()
    }

    private func attach(_ results: RouteController, to list: RouteController) {
        guard results.parent !== list else { return }
        list.addChild(results)
        results.view.frame = list.view.bounds
        results.view.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        list.view.addSubview(results.view)
        // The view stands for the results' route, a positioned box after the list's in the tree: it takes the positioned
        // rank, as a navigation controller's view does for the routes it holds, so its later place paints it over the
        // list's route, whose node is ranked (at depth 0 it would paint under it)
        results.view.setPaintForeground(aboveAuthored: false)
        results.didMove(toParent: list)
    }

    private func detach(_ results: RouteController) {
        guard let parent = results.parent, !(parent is UINavigationController) else { return }
        results.willMove(toParent: nil)
        results.view.setPaintForeground(false)
        results.view.removeFromSuperview()
        results.removeFromParent()
    }
}

extension Presenter {
    /// Whether the bar search of the route `view` is in is open, or UIKit is moving its bar into search.
    package func barSearchOpen(around view: UIView) -> Bool {
        guard let search = ViewPresentation.controllers(for: view).lazy.compactMap({ ($0 as? RouteController)?.search }).first else { return false }
        return search.presenting || search.controller.isActive
    }
}
#endif
