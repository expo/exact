// Inspect mode on iOS (LLP 1122 D4, D5; T2c): the platform half of
// Inspect.swift. While it is on, the window (ExactWindow) gives every touch
// and press here instead of to the app, so none reaches it, the banner's
// close included (it is pressed here). A tap selects the plan node the
// window's own hit test lands on, the real tap path: from the hit view up to
// the nearest view that stands for a plan node, a walk that stops at a
// system bar (D5's chrome rule: a plan node the bar hosts, its title or an
// item, is selectable; UIKit's own parts select nothing, and say so).
#if EXACT_INSPECT && os(iOS)
import UIKit

package enum InspectHost {
    /// The overlay on each window inspect mode is on in.
    nonisolated(unsafe) private static var overlays: [ObjectIdentifier: InspectOverlay] = [:]
    nonisolated(unsafe) private(set) weak static var app: ExactApp?
    nonisolated(unsafe) private static var keyboard: NSObjectProtocol?
    /// The scroll views no presenter manages (a web frame's, a hatch's)
    /// whose status-bar tap inspect mode turned off, given back when it ends.
    nonisolated(unsafe) private static var toTop: [() -> UIScrollView?] = []

    /// The touches and presses inspect mode took, kept to the end of their
    /// sequence even when inspect mode ends meanwhile: none reaches the app
    /// halfway. (Weakly held: a finished touch frees its place.)
    nonisolated(unsafe) private static var owned = NSHashTable<AnyObject>.weakObjects()
    static var owning: Bool { owned.anyObject != nil }
    /// The touches down in the app's windows now, owned the moment inspect
    /// mode begins (one already down is its to its end, as a later one).
    nonisolated(unsafe) private static var live = NSHashTable<UITouch>.weakObjects()
    static func see(_ event: UIEvent) {
        guard event.type == .touches else { return }
        for touch in event.allTouches ?? [] { if touch.phase == .ended || touch.phase == .cancelled { live.remove(touch) } else { live.add(touch) } }
    }

    /// Inspect mode shown, or put away, on every window the app's sessions are in.
    static func show(_ app: ExactApp, on: Bool) {
        self.app = app
        let windows = Set(app.sessions.compactMap { $0.view?.window }.map(ObjectIdentifier.init))
        for (key, overlay) in overlays where !on || !windows.contains(key) || overlay.window == nil { overlay.removeFromSuperview(); overlays[key] = nil }
        // A status-bar tap (the scene's scroll to top, which comes as no
        // touch) scrolls nothing while inspect mode is on (ScrollsToTopIOS.swift).
        if !on { for scroll in toTop { scroll()?.scrollsToTop = true }; toTop = [] }
        for session in app.sessions where session.booted { session.presenter.syncScrollsToTop() }
        guard on else { return }
        for touch in live.allObjects { owned.add(touch) }
        // A field the app focuses later (an autofocus, a sheet) gets no keyboard either.
        keyboard = keyboard ?? NotificationCenter.default.addObserver(forName: UIResponder.keyboardWillShowNotification, object: nil, queue: .main) { _ in
            guard Inspect.on else { return }
            for window in Set((InspectHost.app?.sessions ?? []).compactMap { $0.view?.window }) { window.endEditing(true) }
        }
        for session in app.sessions { if let window = session.view?.window { _ = overlay(on: window) } }
    }

    /// The window's overlay, made on first use (a window inspect mode came
    /// on before, or one an overlay's window went from). On entry what the
    /// app was in the middle of ends: editing (the keyboard goes), each
    /// gesture recognizer (one still waiting, as a long press's, too), each
    /// control tracking a touch and each press a node holds, so none of
    /// them fires later.
    private static func overlay(on window: UIWindow) -> InspectOverlay {
        if let overlay = overlays[ObjectIdentifier(window)], overlay.window === window { return overlay }
        let overlay = InspectOverlay(frame: window.bounds)
        overlay.autoresizingMask = [.flexibleWidth, .flexibleHeight]
        window.addSubview(overlay)
        overlays[ObjectIdentifier(window)] = overlay
        window.endEditing(true)
        var pending: [UIView] = [window]
        while !pending.isEmpty {
            let v = pending.removeFirst()
            for g in v.gestureRecognizers ?? [] where g.isEnabled { g.isEnabled = false; g.isEnabled = true }
            if let control = v as? UIControl, control.isTracking { control.cancelTracking(with: nil) }
            (v as? NodeView)?.cancelPressContact()
            if let scroll = v as? UIScrollView, scroll.scrollsToTop { scroll.scrollsToTop = false; toTop.append { [weak scroll] in scroll } }
            pending += v.subviews
        }
        return overlay
    }

    /// An overlay put away (InspectOverlay.removeFromSuperview).
    static func forget(_ overlay: InspectOverlay) {
        for (key, o) in overlays where o === overlay { overlays[key] = nil }
    }

    /// Whether a session of the app still shows in `window`.
    static func hosts(_ window: UIWindow) -> Bool { app?.sessions.contains { $0.view?.window === window } ?? false }

    /// `outline {sites, plan}`: every copy of those sites outlined in the
    /// windows of the sessions running that plan, the first scrolled into
    /// view unless the frame answers this build's own tap; `{clear}`, or an
    /// empty list, clears them. A frame for another plan leaves a window's
    /// outlines as they are.
    static func outline(_ app: ExactApp, _ frame: [String: Any]) {
        guard Inspect.on else { return }
        if frame["clear"] as? Bool == true { overlays.values.forEach { $0.mark([]) }; return }
        guard let plan = frame["plan"] as? String, let wanted = (frame["sites"] as? [Any])?.compactMap({ ($0 as? NSNumber)?.intValue }) else { return }
        let want = Set(wanted)
        var marks: [ObjectIdentifier: (window: UIWindow, places: [Place])] = [:]
        var copies: [(session: ObjectIdentifier, id: UInt32, view: UIView, shown: Bool)] = [], running: [ObjectIdentifier: Int] = [:]
        for session in app.sessions where session.booted {
            let sites = Inspect.sites(session)
            guard sites.plan == plan else { continue }
            running[ObjectIdentifier(session)] = session.generation
            let nodes = sites.views.filter { want.contains($0.site) }.compactMap { session.presenter.views[$0.id] }.filter { $0.window != nil }
            // (shown: on screen, and not CSS `visibility: hidden`, whose box keeps its place but draws nothing)
            copies += nodes.map { (ObjectIdentifier(session), $0.id, $0, shown($0) && !$0.cssVisibilityHidden) }
            // Each in the window it is in (a sheet's or another scene's).
            for node in nodes { if let window = node.window, let place = place(of: node, in: session) { marks[ObjectIdentifier(window), default: (window, [])].places.append(place) } }
        }
        // The selection's own copy, or the first that shows (one rule, Inspect.revealing).
        if !Inspect.answersClick(frame), let chosen = Inspect.revealing(copies, sites: want, plan: plan, sessions: running) { reveal(chosen) }
        for (_, m) in marks { overlay(on: m.window).mark(m.places) }
    }

    /// Where a node shows, in window coordinates, asked each frame: its own
    /// view's box, or, for a node a native bar stands in for (hidden in the
    /// content), what UIKit draws for it: its bar button item, the bar's
    /// title for the route header or its heading, its tab's button. Nil for
    /// a node that shows nowhere (a grouped list's row is drawn by its cell,
    /// which is not looked for).
    typealias Place = () -> CGRect?
    static func place(of node: NodeView, in session: ExactSession) -> Place? {
        if shown(node) { return { [weak node] in node.flatMap { $0.window != nil && shown($0) ? $0.convert($0.bounds, to: nil) : nil } } }
        let presenter = session.presenter
        if let place = presenter.navigation.inspectPlace(of: node) { return place }
        if let root = presenter.navigation.container, let tabs = NavigationTabs.of(root, presenter),
           tabs.tabs.contains(where: { $0 === node }), presenter.navigation.tabController != nil {
            return { [weak session, weak node] in
                guard let session, let node, case .success(let view)? = session.agentInstance.tabItem(node) else { return nil }
                return view.convert(view.bounds, to: nil)
            }
        }
        return nil
    }

    /// The buttons a bar shows, one box per item in item order (leading to
    /// trailing, so right to left in a right-to-left layout; in the bar's
    /// coordinates; copies drawn for its glass are one box): what a tab bar
    /// shows for its items, which UIKit gives no frame.
    package static func buttonBoxes(in bar: UIView) -> [CGRect] {
        var boxes: [CGRect] = [], pending: [UIView] = bar.subviews
        while !pending.isEmpty {
            let v = pending.removeFirst()
            guard !v.isHidden, v.alpha > 0.01 else { continue }
            if v is UIControl {
                let box = v.convert(v.bounds, to: bar).integral
                if !boxes.contains(box) { boxes.append(box) }
                continue
            }
            pending += v.subviews
        }
        let rtl = bar.effectiveUserInterfaceLayoutDirection == .rightToLeft
        return boxes.sorted { ($0.minY, rtl ? -$0.maxX : $0.minX) < ($1.minY, rtl ? -$1.maxX : $1.minX) }
    }

    /// Whether a view shows: neither it nor an ancestor hidden (a node a
    /// native bar or control stands in for is hidden in the content).
    static func shown(_ view: UIView) -> Bool {
        var at: UIView? = view
        while let v = at { if v.isHidden || v.alpha < 0.01 || (v as? NodeView)?.drawnBySurface == true { return false }; at = v.superview }
        return true
    }

    /// The first copy into view: each scroll view around it, innermost first.
    private static func reveal(_ view: UIView) {
        var at = view.superview
        while let v = at {
            if let scroll = v as? UIScrollView { scroll.scrollRectToVisible(view.convert(view.bounds, to: scroll), animated: false) }
            at = v.superview
        }
    }

    /// An event while inspect mode is on, or one carrying a touch or press
    /// it took: taken here (true), of every kind. Nothing goes on to UIKit:
    /// the banner's close is pressed here too. Every touch and press is
    /// owned to its end: one begun in inspect mode, one already down when it
    /// began (its node's press was cancelled), and, after it ends, a finger
    /// put down beside an owned one. A hardware Escape leaves (its release
    /// is owned too). A tap (a release near where it began) selects.
    static func take(_ event: UIEvent, in window: UIWindow) -> Bool {
        guard let app else { return false }
        func ending(_ phase: UITouch.Phase) -> Bool { phase == .ended || phase == .cancelled }
        if event.type == .presses {
            guard let presses = (event as? UIPressesEvent)?.allPresses else { return Inspect.on }
            // As touches: after inspect mode, a press begun beside an owned one is owned too.
            let mine = Inspect.on || presses.contains { owned.contains($0) }
            if mine { for press in presses { if press.phase == .ended || press.phase == .cancelled { owned.remove(press) } else { owned.add(press) } } }
            if Inspect.on, presses.contains(where: { $0.key?.keyCode == .keyboardEscape && $0.phase == .began }) { app.leaveInspect() }
            return mine
        }
        guard event.type == .touches else { return Inspect.on }
        let touches = event.allTouches ?? []
        if !Inspect.on {
            guard touches.contains(where: { owned.contains($0) }) else { return false }
            for touch in touches { if ending(touch.phase) { owned.remove(touch) } else { owned.add(touch) } }
            return true
        }
        let overlay = overlay(on: window)
        for touch in touches {
            let p = touch.location(in: window)
            if ending(touch.phase) { owned.remove(touch) } else { owned.add(touch) }
            switch touch.phase {
            case .began:
                if overlay.onClose(p) { overlay.closing = touch; continue }
                overlay.pressed = (touch, p)
                overlay.press(where: target(at: p, in: window, app))
            case .moved where overlay.closing === touch:
                overlay.closeHighlighted(overlay.onClose(p))
            case .ended:
                if overlay.closing === touch {
                    overlay.closing = nil
                    if overlay.onClose(p) { app.leaveInspect() }
                    continue
                }
                defer { overlay.pressed = nil; overlay.press(where: nil) }
                guard let began = overlay.pressed, began.touch === touch, hypot(p.x - began.at.x, p.y - began.at.y) <= 10 else { continue }
                let hit = target(at: p, in: window, app)
                if let session = hit.session, let id = hit.id { overlay.say(nil); app.inspectTapped(session, id: id) }
                else { overlay.say(hit.why ?? "Nothing here is a view of the app.") }
            case .cancelled:
                if overlay.closing === touch { overlay.closing = nil }
                if overlay.pressed?.touch === touch { overlay.pressed = nil; overlay.press(where: nil) }
            default: break
            }
        }
        return true
    }

    /// What a tap at `p` selects: the session, the node and the view that
    /// stands for it, or why nothing.
    static func target(at p: CGPoint, in window: UIWindow, _ app: ExactApp) -> (session: ExactSession?, id: UInt32?, view: UIView?, why: String?) {
        // iPadOS's floating tab bar (a regular width): UIKit's own views beside the selected tab's, no UITabBar.
        if let floating = floatingTab(at: p, in: window, app) { return floating }
        guard let hit = window.hitTest(p, with: nil), let session = app.sessions.first(where: { $0.booted && Agent.shows($0, hit) }) else { return (nil, nil, nil, nil) }
        var at: UIView? = hit
        while let v = at {
            // A system bar: a plan node it hosts (its title, an item) is
            // selectable; its own parts are not, and the walk goes no higher.
            if let bar = v as? UINavigationBar {
                if let id = session.presenter.navigation.inspectNode(in: bar, at: window.convert(p, to: bar)) { return (session, id, session.presenter.views[id] ?? bar, nil) }
                return (session, nil, nil, "That is part of UIKit's navigation bar, not a view of the app.")
            }
            if let bar = v as? UITabBar {
                if let id = session.presenter.segments.tab(at: window.convert(p, to: bar), in: bar) ?? tabNode(session, matching: { $0.convert($0.bounds, to: window).contains(p) }) { return (session, id, session.presenter.views[id] ?? bar, nil) }
                return (session, nil, nil, "That is part of UIKit's tab bar, not a view of the app.")
            }
            // A segmented control standing in for a tablist: the tab under the point.
            if v is UISegmentedControl, let id = session.presenter.segments.tab(at: window.convert(p, to: v), in: v) { return (session, id, v, nil) }
            if v is UIToolbar { return (session, nil, nil, "That is part of a UIKit toolbar, not a view of the app.") }
            if let node = v as? NodeView, !node.isHidden { return (session, node.id, node, nil) }
            if let button = v as? NativeButtonIOS, let owner = button.owner { return (session, owner.id, owner, nil) }
            if let owned = v as? InspectionOwned, let id = owned.inspectionViewId { return (session, id, v, nil) }
            if let id = session.presenter.navigation.inspectTitle(v) { return (session, id, v, nil) }
            // A swipe action UIKit draws for a row: the action's node.
            if let button = v as? UIButton, !(v is NativeButtonIOS), let id = session.presenter.swipeActions.action(for: button) { return (session, id, button, nil) }
            // A grouped list's cell (its rows are no NodeViews): its switch's
            // or detail button's node when the point is on one, else its row.
            if v is UITableViewCell || v is UICollectionViewCell, let part = GroupedListsLink.part?(hit),
               let id = ((part["target"] ?? part["row"]) as? NSNumber)?.uint32Value { return (session, id, v, nil) }
            at = v.superview
        }
        return (session, nil, nil, nil)
    }

    /// What a tap at `p` on iPadOS's floating tab bar selects: the tab whose
    /// item is there, else nothing, said, on the bar's own space (its list, or
    /// a view around the list that holds no tab's content); nil anywhere else,
    /// for the walk to judge (`floatingTabList`).
    private static func floatingTab(at p: CGPoint, in window: UIWindow, _ app: ExactApp) -> (session: ExactSession?, id: UInt32?, view: UIView?, why: String?)? {
        guard let hit = window.hitTest(p, with: nil) else { return nil }
        for session in app.sessions where session.booted {
            let nav = session.presenter.navigation
            guard let tabs = nav.tabController, let list = tabs.floatingTabList, list.window === window,
                  let selected = tabs.selectedViewController?.viewIfLoaded else { continue }
            guard hit.isDescendant(of: tabs.view), !hit.isDescendant(of: selected) else { continue }
            if let id = tabNode(session, matching: { hit.isDescendant(of: $0) }) {
                return (session, id, session.presenter.views[id] ?? hit, nil)
            }
            guard hit.isDescendant(of: list) || (list.isDescendant(of: hit) && !selected.isDescendant(of: hit)) else { continue }
            return (session, nil, nil, "That is part of UIKit's tab bar, not a view of the app.")
        }
        return nil
    }

    /// The authored tab for the control UIKit presents.
    private static func tabNode(_ session: ExactSession, matching: (UIView) -> Bool) -> UInt32? {
        let nav = session.presenter.navigation
        guard let root = nav.container, let tabs = NavigationTabs.of(root, session.presenter) else { return nil }
        for tab in tabs.tabs {
            guard case .success(let control)? = session.agentInstance.tabItem(tab) else { continue }
            if matching(control) { return tab.id }
        }
        return nil
    }
}

extension UINavigationBar {
    /// The box of the visible label UIKit shows `title` in. Back shows the
    /// previous route's title, which can be the same words, so among the
    /// labels saying it the title is the lowest (a large title is under
    /// Back's row), then the nearest the bar's middle (an inline one is
    /// centred; Back's is at the leading edge).
    package func titleFrame(_ title: String?) -> CGRect? {
        guard let title, !title.isEmpty else { return nil }
        var found: [CGRect] = [], pending: [UIView] = subviews.filter { !$0.isHidden && $0.alpha > 0.01 }
        while !pending.isEmpty {
            let v = pending.removeFirst()
            if let label = v as? UILabel, label.text == title { found.append(label.convert(label.bounds, to: self)) }
            pending += v.subviews.filter { !$0.isHidden && $0.alpha > 0.01 }
        }
        return found.max { a, b in
            if abs(a.maxY - b.maxY) > 4 { return a.maxY < b.maxY }
            return abs(a.midX - bounds.midX) > abs(b.midX - bounds.midX)
        }
    }
}

/// The overlay over a window in inspect mode: the banner (with its close),
/// the press highlight and the outlines. It takes no touch itself:
/// InspectHost.take has them all, the close's included, and a tap on the
/// banner beside its close inspects what is under it (a bar's title).
final class InspectOverlay: UIView {
    private let banner = UIView(), label = UILabel(), close = UIButton(type: .close)
    private let pressBox = UIView()
    private var marks: [(place: InspectHost.Place, box: UIView)] = []
    private var link: CADisplayLink?
    var pressed: (touch: UITouch, at: CGPoint)?
    weak var closing: UITouch? { didSet { closeHighlighted(closing != nil) } }
    private var pressedPlace: InspectHost.Place?

    /// The display link's target, so the link holds no overlay.
    private final class Tick: NSObject {
        weak var overlay: InspectOverlay?
        init(_ overlay: InspectOverlay) { self.overlay = overlay }
        @objc func tick() { overlay?.follow() }
    }

    override init(frame: CGRect) {
        super.init(frame: frame)
        backgroundColor = .clear
        isUserInteractionEnabled = false
        pressBox.layer.borderWidth = 2
        pressBox.layer.borderColor = UIColor.systemBlue.cgColor
        pressBox.isHidden = true
        addSubview(pressBox)
        banner.backgroundColor = UIColor.systemBlue.withAlphaComponent(0.92)
        banner.layer.cornerRadius = 10
        label.textColor = .white
        label.font = .preferredFont(forTextStyle: .footnote)
        label.numberOfLines = 2
        say(nil)
        close.accessibilityLabel = "Leave inspect mode"
        banner.addSubview(label)
        banner.addSubview(close)
        addSubview(banner)
        link = CADisplayLink(target: Tick(self), selector: #selector(Tick.tick))
        link?.add(to: .main, forMode: .common)
    }
    required init?(coder: NSCoder) { fatalError() }

    override func removeFromSuperview() { link?.invalidate(); link = nil; marks = []; pressedPlace = nil; InspectHost.forget(self); super.removeFromSuperview() }

    override func hitTest(_ point: CGPoint, with event: UIEvent?) -> UIView? { nil }
    /// Whether `p` (in the window) is on the banner's close.
    func onClose(_ p: CGPoint) -> Bool { close.convert(close.bounds, to: window).insetBy(dx: -6, dy: -6).contains(p) }
    func closeHighlighted(_ on: Bool) { close.isHighlighted = on }

    override func layoutSubviews() {
        super.layoutSubviews()
        let inset = safeAreaInsets.top + 6, width = min(bounds.width - 24, 420)
        banner.frame = CGRect(x: (bounds.width - width) / 2, y: inset, width: width, height: 44)
        close.frame = CGRect(x: width - 40, y: 6, width: 32, height: 32)
        label.frame = CGRect(x: 12, y: 2, width: width - 56, height: 40)
        superview?.bringSubviewToFront(self)
    }

    /// The banner's line: inspect mode's own, or why a tap selected nothing.
    func say(_ why: String?) { label.text = why ?? "Inspecting: tap a view to select it" }

    /// The press highlight on what a finger down would select.
    func press(where hit: (session: ExactSession?, id: UInt32?, view: UIView?, why: String?)?) {
        if let hit, let session = hit.session, let id = hit.id, let node = session.presenter.views[id], let place = InspectHost.place(of: node, in: session) { pressedPlace = place }
        else if let view = hit?.view { pressedPlace = { [weak view] in view.flatMap { $0.window != nil ? $0.convert($0.bounds, to: nil) : nil } } }
        else { pressedPlace = nil }
        follow()
    }

    /// Every copy outlined, where each shows.
    func mark(_ places: [InspectHost.Place]) {
        for m in marks { m.box.removeFromSuperview() }
        marks = places.map { place in
            let box = UIView()
            box.isUserInteractionEnabled = false
            let dash = CAShapeLayer()
            dash.strokeColor = UIColor.systemBlue.cgColor
            dash.fillColor = nil
            dash.lineWidth = 2
            dash.lineDashPattern = [6, 4]
            box.layer.addSublayer(dash)
            insertSubview(box, belowSubview: banner)
            return (place, box)
        }
        follow()
    }

    /// Outlines kept on their copies as the app scrolls, lays out and
    /// animates; the overlay goes once no session of the app shows here.
    fileprivate func follow() {
        guard let window, InspectHost.hosts(window) else { if superview != nil { removeFromSuperview() }; return }
        superview?.bringSubviewToFront(self)
        for m in marks {
            let r = m.place().map { convert($0, from: nil) }
            m.box.isHidden = r == nil
            guard let r else { continue }
            m.box.frame = r
            (m.box.layer.sublayers?.first as? CAShapeLayer)?.path = UIBezierPath(rect: CGRect(origin: .zero, size: r.size)).cgPath
        }
        if let r = pressedPlace?() { pressBox.isHidden = false; pressBox.frame = convert(r, from: nil) } else { pressBox.isHidden = true }
    }
}
#endif
