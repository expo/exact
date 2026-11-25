// @ref LLP 1127.000 D2 and LLP 1123 D7 — residual keyboard and search-bar clearance.
#if os(iOS)
import ExactKit
import UIKit

final class SearchClearance {
    struct Occupancy {
        var keyboard: CGRect = .null
        var docked = false
        var bar: CGRect = .null

        func contentBlock(_ allocation: CGRect, policy: String, ignoringKeyboard: Bool) -> CGRect {
            guard policy == "resizes-content", !ignoringKeyboard, docked, keyboard.intersects(allocation) else { return allocation }
            var block = allocation
            block.size.height = max(0, keyboard.minY - block.minY)
            return block
        }

        func bottomDepth(in port: CGRect) -> CGFloat {
            guard !port.isEmpty else { return 0 }
            let keys = docked ? keyboard.intersection(port) : .null
            let search = bar.intersection(port)
            let reachesBottom = !search.isNull && search.maxY >= port.maxY - 1
            let aboveKeys = !search.isNull && !keys.isNull && bar.maxY <= keyboard.maxY && bar.maxY >= keyboard.minY - bar.height
            let obstruction = reachesBottom || aboveKeys ? keys.union(search) : keys
            return obstruction.isNull ? 0 : max(0, port.maxY - obstruction.minY)
        }

        func floatingKeyboard(in port: CGRect) -> CGRect? {
            let overlap = keyboard.intersection(port)
            return docked || overlap.isEmpty ? nil : overlap
        }
    }

    private final class Inset {
        weak var scroll: UIScrollView?
        let keyboard = KeyboardInset()

        init(_ scroll: UIScrollView) { self.scroll = scroll }
    }
    private var insets: [ObjectIdentifier: Inset] = [:]
    private var fitting = false
    private var bottom: CGFloat = 0

    func clear() {
        let previous = insets.values
        insets.removeAll()
        bottom = 0
        for inset in previous {
            if let scroll = inset.scroll { inset.keyboard.apply(0, to: scroll) }
        }
    }

    static func ignoresKeyboard(_ node: NodeView) -> Bool {
        Int(node.style["safe_area_ignore"]?.number ?? 0) & 2 != 0
    }

    /// Each scrolling branch clears its outermost observing port. The
    /// allocation has already taken UIKit's safe area and content resizing.
    func fit(_ root: NodeView, in view: UIView, allocation: CGRect, occupancy: Occupancy, policy: String,
             enabled: Bool, fallback: UIScrollView? = nil) -> CGFloat {
        guard !fitting else { return bottom } // Re-entry from the inset setters sees the same allocation and keyboard.
        fitting = true
        defer { fitting = false }
        guard enabled, root.style["display"]?.string != "none", view.window != nil, policy != "overlays-content" else {
            clear(); return 0
        }
        var residual = occupancy
        if root.props["keyboardAvoidance"] == "none" { residual.bar = .null }
        if Self.ignoresKeyboard(root), policy == "resizes-content" {
            clear(); return 0
        }
        let depth = residual.bottomDepth(in: allocation)
        guard depth > 0 else { clear(); return 0 }
        var ports: [UIScrollView] = []
        Self.scrollPorts(in: root, reaching: allocation, in: view, into: &ports)
        if ports.isEmpty, !Self.ignoresKeyboard(root), let fallback,
           Self.reaches(fallback, allocation: allocation, in: view) { ports.append(fallback) }
        if ports.isEmpty {
            clear()
            // Visual resizing can only reserve keyboard space in a scrollport.
            let keys = Occupancy(keyboard: residual.keyboard, docked: residual.docked).bottomDepth(in: allocation)
            bottom = policy == "resizes-visual" && depth == keys ? 0 : depth
            return bottom
        }
        let previous = insets
        insets = [:]
        bottom = 0
        for scroll in ports {
            let key = ObjectIdentifier(scroll)
            let inset = previous[key].flatMap { $0.scroll === scroll ? $0 : nil } ?? Inset(scroll)
            insets[key] = inset
            var system = scroll.adjustedContentInset
            system.bottom -= inset.keyboard.amount
            let port = scroll.convert(scroll.bounds.inset(by: system), to: view).intersection(allocation)
            let depth = PixelGrid.round(residual.bottomDepth(in: port), scale: view.traitCollection.displayScale)
            inset.keyboard.apply(depth, to: scroll)
        }
        for (key, inset) in previous where insets[key] == nil {
            if let scroll = inset.scroll { inset.keyboard.apply(0, to: scroll) }
        }
        return bottom
    }

    private static func scrollPorts(in node: NodeView, reaching allocation: CGRect, in view: UIView, into ports: inout [UIScrollView]) {
        guard !node.isHidden else { return }
        if node.scrollsVertically, !ignoresKeyboard(node), let scroll = node.scrollView,
           reaches(scroll, allocation: allocation, in: view) {
            ports.append(scroll)
            return
        }
        for case let child as NodeView in node.container.subviews {
            scrollPorts(in: child, reaching: allocation, in: view, into: &ports)
        }
    }

    private static func reaches(_ scroll: UIScrollView, allocation: CGRect, in view: UIView) -> Bool {
        scroll.window === view.window && scroll.convert(scroll.bounds, to: view).maxY >= allocation.maxY - 1
    }
}

extension NavigationHost {
    func searchOccupancy(in view: UIView) -> SearchClearance.Occupancy? {
        guard let split, let search = split.pairedSearch, search.controller.isActive, splitPrimaryExposed,
              let window = view.window, split.controller.viewIfLoaded?.window === window else { return nil }
        var occupancy = SearchClearance.Occupancy()
        // The window's sampler follows the keyboard as it moves, where a view's own guide moves only when that view lays out
        if let keys = KeyboardSampleIOS.install(in: window).sample, keys.frame.intersects(window.bounds) {
            occupancy.keyboard = view.convert(keys.frame, from: window)
            occupancy.docked = keys.docked
        }
        let bar = search.controller.searchBar
        // A search bar integrated in a navigation or toolbar already participates
        // in platform fitting. The floating UISearchBar publishes its own bounds.
        if bar.window === window, !bar.bounds.isEmpty, !sequence(first: bar as UIView, next: \.superview).contains(where: {
            $0 is UINavigationBar || $0 is UIToolbar || $0.isHidden || $0.alpha < 0.01
        }) {
            occupancy.bar = view.convert(bar.bounds, from: bar)
        }
        return occupancy
    }
}
#endif
