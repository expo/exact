// @ref LLP 1075.003 §9.16 — UIKit's mark in the panel More's Edit opens.
#if os(iOS)
import UIKit

/// UIKit's panel for More's Edit marks the tab a drag would replace with an
/// image view it adds to the bar, centered where the tab's button is in the
/// view that holds the buttons. That view can start far along the bar (an
/// iPhone Duo's platter), so the mark lies over another tab, or none. While
/// the panel is open, each update of the screen puts a mark placed so over its
/// tab. It moves no mark that is over a tab, as a UIKit that places it right
/// has it. Which tab a drag replaces stays UIKit's choice.
final class TabMark {
    private weak var tabs: UITabBarController?
    private var mayPlace: () -> Bool = { false }
    /// The images in the bar as the panel opened, none of them the mark.
    private let others = NSHashTable<UIImageView>.weakObjects()
    /// The mark as it was last judged, and where it was left.
    private weak var judged: UIImageView?
    private var left = CGPoint.zero
    private var stopUpdates: (() -> Void)?

    /// From when the panel opens, to when it is done or Exact lets the container go, while `mayPlace`.
    func watch(_ tabs: UITabBarController, while mayPlace: @escaping () -> Bool) {
        stop()
        #if !targetEnvironment(macCatalyst)
        guard #available(iOS 18.0, *), mayPlace() else { return }
        self.tabs = tabs
        self.mayPlace = mayPlace
        for image in images { others.add(image) }
        let link = UIUpdateLink(view: tabs.view)
        link.addAction(to: .beforeCATransactionCommit) { [weak self] _, _ in self?.place() }
        link.isEnabled = true
        stopUpdates = { link.isEnabled = false }
        #endif
    }

    var watching: Bool { stopUpdates != nil }

    func stop() {
        stopUpdates?()
        stopUpdates = nil
        others.removeAllObjects()
        judged = nil
    }

    /// Put UIKit's mark over its tab, after UIKit has handled the touches and before the frame that shows it. A mark is judged
    /// again only once UIKit has placed it anew: it leaves one where it is as the bar is laid out.
    private func place() {
        guard let tabs, tabs.tabBar.isCustomizing, mayPlace() else { return stop() }
        let marks = images.filter { !others.contains($0) && !$0.isUserInteractionEnabled }
        guard marks.count == 1, let mark = marks.first, mark !== judged || mark.center != left else { return }
        defer {
            judged = mark
            left = mark.center
        }
        let places = buttons.map { Self.place(of: mark.center, local: $0.local, over: $0.bar) }
        if let first = places.first ?? nil, places.allSatisfy({ $0.map { Self.near($0, first) } == true }) { mark.center = first }
    }

    /// Where a mark at `mark`, in the bar, belongs, if not there: UIKit centers it where the tab's button is in the view that
    /// holds the buttons (`local`), as though that view started where the bar does, and the tab is at `over`. Nil for a mark
    /// over a tab, as one UIKit places right is, one where no tab is, and one that could be either or two tabs'.
    static func place(of mark: CGPoint, local: [CGPoint], over: [CGPoint]) -> CGPoint? {
        guard !over.contains(where: { near(mark, $0) }), let tab = local.firstIndex(where: { near(mark, $0) }),
              local.lastIndex(where: { near(mark, $0) }) == tab else { return nil }
        return over[tab]
    }

    private static func near(_ a: CGPoint, _ b: CGPoint) -> Bool { abs(a.x - b.x) <= 1 && abs(a.y - b.y) <= 1 }

    private var images: [UIImageView] { tabs?.tabBar.subviews.compactMap { $0 as? UIImageView } ?? [] }

    /// Where the bar's buttons are, in each view that could hold them and in the bar: any outside the pages with one control
    /// for each of the bar's items. UIKit has one for the selected tab's looks and one for the others'; those of an iPhone
    /// Duo's vertical bar are not in the bar.
    private var buttons: [(local: [CGPoint], bar: [CGPoint])] {
        guard let tabs, let count = tabs.tabBar.items?.count, count > 1 else { return [] }
        let pages = Set(tabs.children.compactMap { $0.viewIfLoaded.map(ObjectIdentifier.init) })
        var found: [(local: [CGPoint], bar: [CGPoint])] = []
        func search(_ view: UIView) {
            guard !pages.contains(ObjectIdentifier(view)) else { return }
            let centers = view.subviews.compactMap { ($0 as? UIControl)?.center }
            if centers.count == count { found.append((centers, centers.map { view.convert($0, to: tabs.tabBar) })) }
            view.subviews.forEach(search)
        }
        search(tabs.view)
        return found
    }
}
#endif
