// @ref LLP 1153 D5 — the runner's `scrollToFragment(visit, fragment,
// activation)`, sent once a route's data has arrived: the browser's "scroll
// to the fragment" on a native host. The node with that HTML `id` in the
// visit's own route (a route retained beneath may hold the same id; the id
// as written, else percent-decoded) is aligned as `scrollIntoView(id)` with
// `block="start"` aligns it, every scroller above it moving, held through a
// navigation's transition and while its pictures load. The accessibility
// cursor goes to it (VoiceOver on iOS, the layout change it names on the
// Mac), and subsequent Mac Tab navigation starts there unless a text
// control, another session or the containing app holds the focus.
// `top`, or no such node, takes the route's scrollers to their tops and the
// route's start into view. A scroll by the person since the router change
// that made the activation arrived (`fragmentArm` against `personScrolls`)
// cancels it, and an author's `scrollIntoView` takes the place of its held
// request (`fragmentHeld`).
#if os(macOS)
import AppKit
#else
import UIKit
#endif

final class FragmentHold {
    weak var route: NodeView?
    weak var target: NodeView?
    #if !os(macOS)
    let routeIncarnation: UInt64, incarnation: UInt64
    #endif
    let name: String, scrolls: Int
    var finishing = false, started = false

    init(route: NodeView, target: NodeView, name: String, scrolls: Int) {
        self.route = route; self.target = target; self.name = name; self.scrolls = scrolls
        #if !os(macOS)
        routeIncarnation = route.incarnation; incarnation = target.incarnation
        #endif
    }
}

extension Presenter {
    /// A router change's activation, armed at the person's scrolls by then;
    /// each once, though the change is seen again.
    func armFragment(_ activation: Any?) {
        guard let activation = (activation as? NSNumber)?.intValue else { return }
        if activation == 0 { cancelFragment(); fragmentArm = nil; return }
        guard activation > fragmentSeen else { return }
        cancelFragment()
        fragmentSeen = activation
        fragmentArm = (activation, personScrolls)
    }

    func cancelFragment() {
        #if !os(macOS)
        if let held = fragmentHeld { elementScrolls.drop(held.name) }
        #endif
        fragmentHeld = nil
    }

    func fragmentOffsetsWritten(_ op: BatchOp) {
        guard let target = fragmentHeld?.target, let scroller = views[op.id],
              op.props["scrollTop"] != nil || op.props["scrollLeft"] != nil,
              target.isDescendant(of: scroller) else { return }
        cancelFragment()
    }

    func resetFragments() {
        cancelFragment(); fragmentArm = nil; fragmentSeen = 0
    }

    private func holdFragment(_ target: NodeView, in route: NodeView, name: String) {
        cancelFragment()
        let hold = FragmentHold(route: route, target: target, name: name, scrolls: personScrolls)
        fragmentHeld = hold
        flushFragment()
        // The launch command can precede the window's mount.
        DispatchQueue.main.async { [weak self, weak hold] in
            guard let self, let hold, self.fragmentHeld === hold else { return }
            self.flushFragment()
        }
    }

    func flushFragment(finishing: Bool = false) {
        guard let hold = fragmentHeld else { return }
        if finishing { hold.finishing = false }
        guard let route = hold.route, let target = hold.target,
              views[route.id] === route, views[target.id] === target,
              target === route || target.isDescendant(of: route), !route.inert,
              target === route || target.props["id"] == hold.name,
              personScrolls == hold.scrolls else { cancelFragment(); return }
        #if !os(macOS)
        guard route.incarnation == hold.routeIncarnation, target.incarnation == hold.incarnation else { cancelFragment(); return }
        #endif
        guard !applying, target.window != nil else { return }
        if let session, session.fillInFlight {
            session.whenIdle { [weak self, weak hold] in
                guard let self, let hold, self.fragmentHeld === hold else { return }
                self.flushFragment()
            }
            return
        }
        #if !os(macOS)
        guard canScrollIntoView(target) else { return }
        #endif
        scrollElementIntoView([hold.name, "start", "nearest", "instant"], target: target)
        if !hold.started {
            hold.started = true
            if target !== route, !startAtFragment(target, hold) { return }
        }
        if session?.rasters.loading(inside: route) == true { return }
        #if !os(macOS)
        if settlesElementScroll { return }
        #endif
        if finishing { fragmentHeld = nil; return }
        guard !hold.finishing else { return }
        hold.finishing = true
        // A mount pass can have delivered the last raster with its intrinsic
        // report queued for the next turn. Release after that layout lands.
        DispatchQueue.main.async { [weak self, weak hold] in
            let finish = { [weak self, weak hold] in
                guard let self, let hold, self.fragmentHeld === hold else { return }
                self.flushFragment(finishing: true)
            }
            if let session = self?.session { session.whenIdle(finish) } else { finish() }
        }
    }

    /// False when what the focus change ran superseded `hold`.
    private func startAtFragment(_ target: NodeView, _ hold: FragmentHold) -> Bool {
        #if os(macOS)
        // Only the window's focus, or this session's off a text control, gives
        // way: another session's or the containing app's is theirs.
        if let window = target.window {
            let owner = keyTarget(window.firstResponder)
            if windowHoldsFocus(window) || (owner != nil && owner?.field == nil && owner?.textArea == nil) {
                if owner != nil {
                    window.makeFirstResponder(nil)
                    // Its `blur` handler ran, and may have navigated again.
                    guard fragmentHeld === hold else { return false }
                }
                tabStart = keyView(of: target)
                keyViewLoopStale = true
                session?.view?.focusesFirstKeyView = false
            }
        }
        NSAccessibility.post(element: target, notification: .layoutChanged, userInfo: [.uiElements: [target]])
        #elseif os(iOS)
        UIAccessibility.post(notification: .layoutChanged, argument: target)
        #endif
        return true
    }

    func scrollToFragment(_ args: [Any]) {
        let visit = (args.first as? NSNumber).map { String($0.int64Value) } ?? ""
        let fragment = args.count > 1 ? args[1] as? String ?? "" : ""
        let activation = args.count > 2 ? (args[2] as? NSNumber)?.intValue : nil
        let arm = fragmentArm.flatMap { $0.activation == activation ? $0 : nil }
        // One armed before and not now was taken or superseded: it leaves the current hold alone.
        if arm == nil, let activation, activation > 0, activation <= fragmentSeen { return }
        if arm != nil { fragmentArm = nil }
        if let arm, arm.scrolls != personScrolls {
            session?.log("fragment #\(fragment): the person scrolled first; the route stays where it is")
            return
        }
        let nodes = views.values.sorted { $0.id < $1.id }
        guard let route = nodes.first(where: { $0.props["navigationKey"] == visit && $0.props["navigationBack"] == nil }) else {
            session?.log("fragment #\(fragment): no route of visit \(visit) is on the screen")
            return
        }
        let id = fragment.removingPercentEncoding
        let target = [fragment, id ?? ""].lazy.filter { !$0.isEmpty }
            .compactMap { name in nodes.first { $0.props["id"] == name && $0.isDescendant(of: route) } }.first
        if let target, let name = target.props["id"] {
            holdFragment(target, in: route, name: name)
            return
        }
        if !fragment.isEmpty, id?.lowercased() != "top" {
            session?.log("fragment #\(fragment): no element in the route of visit \(visit) has that id; the route goes to its top")
        }
        for node in nodes where node === route || node.isDescendant(of: route) {
            #if os(macOS)
            guard let clip = node.scroll?.contentView, clip.bounds.origin.y != 0 else { continue }
            clip.scroll(to: NSPoint(x: clip.bounds.origin.x, y: 0))
            node.scroll?.reflectScrolledClipView(clip)
            #else
            guard let sv = node.scrollView else { continue }
            let top = CGPoint(x: sv.contentOffset.x, y: -sv.adjustedContentInset.top)
            if sv.contentOffset != top { sv.setContentOffset(top, animated: false) }
            #endif
        }
        // The scrollers that present the route, above it.
        let name = route.props["id"] ?? "route \(visit)"
        holdFragment(route, in: route, name: name)
    }
}
