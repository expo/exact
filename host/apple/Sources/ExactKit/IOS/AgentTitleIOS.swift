// @ref LLP 1021 §5.1 — a long press on a bar's title that stands for its
// header's pressable group opens the group's context popover; the agent's
// `contextmenu` tap on the group lands on that title, where a finger would.
#if os(iOS)
import UIKit

extension Agent {
    /// A `contextmenu` tap on the group the bar's title shows (the page hides
    /// it): the title's middle, or `at` in the title, gets the group's
    /// `contextmenu`, then the painted popover. Nil for any other tap.
    func titleContext(_ req: [String: Any]) -> [String: Any]? {
        guard req["contextmenu"] as? Bool == true, req["phase"] == nil,
              ["x", "y", "wheel", "hover", "dblclick", "pinch"].allSatisfy({ req[$0] == nil }),
              let rawID = req["id"] as? Int, let id = UInt32(exactly: rawID), let source = presenter.views[id],
              let title = presenter.navigation.titleSurface(for: source) else { return nil }
        guard presenter.menus.eligible(source, contextMenu: true, inBar: true), source.props["contextPopover"]?.isEmpty == false,
              let window = title.window else {
            return ["error": "title #\(id) has no context popover it can open now"]
        }
        var point = CGPoint(x: title.bounds.midX, y: title.bounds.midY)
        if let at = req["at"] {
            guard let pair = at as? [Double], pair.count == 2, pair.allSatisfy(\.isFinite) else { return ["error": "tap at needs two finite numbers"] }
            point = CGPoint(x: pair[0], y: pair[1])
        }
        guard title.bounds.contains(point), let hit = window.hitTest(title.convert(point, to: window), with: nil),
              hit === title || hit.isDescendant(of: title) else { return ["error": "title #\(id) is hidden or covered at that point"] }
        guard ExactEnv.agentPaints else { return ["error": "title #\(id): UIKit's context menu opens from a real long press, `tap <group> drag 0 0 press 1000`; the agent opens it painted"] }
        source.deliverContext(at: source.convert(point, from: title))
        // Its handler may navigate, replace the group, or replace the title
        let opened = presenter.views[id] === source
            && presenter.navigation.titleSurface(for: source) === title && presenter.menus.agentContext(source, inBar: true)
        // A handler that ran opens nothing by itself: the menu is the popover's
        guard opened else {
            return ["error": "title #\(id) opened nothing: no popover has the id its `contextPopover` names, or the group is no longer the title's"]
        }
        let at = presenter.client(title.convert(point, to: nil))
        return ["tapped": Int(id), "event": "contextmenu", "injected": true, "at": [Agent.r2(at.x), Agent.r2(at.y)]]
    }
}
#endif
