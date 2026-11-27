#if os(iOS)
import UIKit
import UIKit.UIGestureRecognizerSubclass
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// More's Edit (`TabMark`).
extension NavigationMoreIOSTests {
    /// A mark centered on a tab by the tab's place in the view that holds the tabs belongs over it. A mark over a tab, where no
    /// tab is either way, or that could be either or two tabs', belongs nowhere else.
    func testTheMarksPlaceIsOverTheTabUIKitMeans() {
        let local = stride(from: CGFloat(40), through: 340, by: 50).map { CGPoint(x: $0, y: 31) }
        let across = { (start: CGFloat) in local.map { CGPoint(x: $0.x + start, y: $0.y) } }
        let down = local.map { CGPoint(x: $0.y, y: $0.x) }
        XCTAssertEqual(TabMark.place(of: CGPoint(x: 90.4, y: 31), local: local, over: across(130)), CGPoint(x: 220, y: 31))
        XCTAssertEqual(TabMark.place(of: CGPoint(x: 31, y: 90), local: down, over: down.map { CGPoint(x: $0.x + 6, y: $0.y + 180) }), CGPoint(x: 37, y: 270))
        // An iPhone Duo's vertical bar on iOS 27.3, open and closed: where its tabs are in the bar, and UIKit's marks for them
        for start in [CGFloat(383), 392] {
            let rail = (0..<5).map { CGPoint(x: 24, y: 31 + 50 * CGFloat($0)) }, tabs = rail.map { CGPoint(x: $0.x - 3, y: $0.y + start) }
            // (The four tabs a drop can replace: More is none)
            for (tab, mark) in rail.prefix(4).enumerated() { XCTAssertEqual(TabMark.place(of: mark, local: rail, over: tabs), tabs[tab]) }
        }
        XCTAssertNil(TabMark.place(of: CGPoint(x: 220, y: 31), local: local, over: across(130)), "over a tab")
        XCTAssertNil(TabMark.place(of: CGPoint(x: 65, y: 31), local: local, over: across(130)), "between tabs")
        XCTAssertNil(TabMark.place(of: CGPoint(x: 140, y: 31), local: local, over: across(100)), "the third's misplaced, or the first's where it is")
        XCTAssertNil(TabMark.place(of: CGPoint(x: 90, y: 31), local: local, over: local), "over its tab already")
        XCTAssertNil(TabMark.place(of: CGPoint(x: 90, y: 31), local: local + [CGPoint(x: 90.5, y: 31)], over: across(130) + [CGPoint(x: 220.5, y: 31)]), "one of two tabs'")
    }

    #if !targetEnvironment(macCatalyst)

    /// A tab dragged in UIKit's panel over the bar and held there: UIKit's mark shows over the tab that the drop replaces.
    func testMoresEditPutsUIKitsMarkOverTheTabADropReplaces() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "a real drag reaches UIKit") }
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else { throw missing("touch", "a real drag over the bar") }
        guard #available(iOS 18.0, *) else { throw XCTSkip("no `UIUpdateLink`") }
        let touch = try TouchRunner(self)
        let (session, tabs, more) = try moreFixture("more-edit-drag")
        defer { touch.connection.cancel() }
        let window = try XCTUnwrap(tabs.view.window), bar = tabs.tabBar
        try XCTSkipUnless(bar.bounds.width > bar.bounds.height && tabs.view.effectiveUserInterfaceLayoutDirection == .leftToRight, "a bar across, from the left")
        openEdit(tabs, more)
        let updates = Updates(tabs.view)
        defer { updates.stop() }
        updates.wait(self, 3)
        let item = try XCTUnwrap(tabs.viewControllers?.last?.tabBarItem)
        let six = try XCTUnwrap(outside(tabs).first { ($0 as? UILabel)?.text == "Six" && !$0.isDescendant(of: bar) }, "the panel's Six")
        let place = { six.convert(CGPoint(x: six.bounds.midX, y: six.bounds.midY), to: nil) }
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil && place().y > 100 }
        let places = controls(tabs).map { $0.convert(CGPoint(x: $0.bounds.midX, y: $0.bounds.midY), to: nil) }.sorted { $0.x < $1.x }
        XCTAssertEqual(places.count, bar.items?.count)
        let before = bar.items ?? [], others = Set(bar.subviews.map(ObjectIdentifier.init)), to = try XCTUnwrap(places.dropFirst().first)
        // What shows while a finger is down at the end of its drag
        let finger = Finger()
        window.addGestureRecognizer(finger)
        defer { window.removeGestureRecognizer(finger) }
        var held: [(shows: Bool, center: CGPoint)] = []
        updates.each = {
            guard let at = finger.at, abs(at.x - to.x) < 1, abs(at.y - to.y) < 1 else { return }
            guard let image = bar.subviews.first(where: { $0 is UIImageView && !others.contains(ObjectIdentifier($0)) }) else { return held.append((false, .zero)) }
            let shown = image.layer.presentation() ?? image.layer
            held.append((!image.isHidden && shown.opacity > 0.9, image.convert(CGPoint(x: image.bounds.midX, y: image.bounds.midY), to: nil)))
        }
        // Only the screen's own updates from here, as a person's hold has
        updates.continuous = false
        var request = try TouchRunner.aim(place(), in: window)
        let end = try TouchRunner.aim(to, in: window)
        request.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "velocity": 200, "press": 0.6, "hold": 0.8]) { _, new in new }
        XCTAssertEqual(try touch.ask(request)["done"] as? Bool, true)
        waitFor("the drop") { (bar.items ?? []) != before }
        let replaced = try XCTUnwrap((bar.items ?? []).firstIndex { $0 === item }, "Six came to the bar")
        let last = try XCTUnwrap(held.last, "the finger held at the end of its drag")
        XCTAssertTrue(last.shows, "the mark shows")
        // From the first frame it shows, there
        for sample in [held.first { $0.shows }, last].compactMap({ $0 }) {
            XCTAssertEqual(sample.center.x, places[replaced].x, accuracy: 2, "over tab \(replaced), which the drop replaced: \(places)")
            XCTAssertEqual(sample.center.y, places[replaced].y, accuracy: 2)
        }
        try finishEdit(tabs)
        _ = session
    }

    /// Exact looks for UIKit's mark from when the panel opens until it is done, the container goes, no panel is open, or the app
    /// has native code. The one image that comes to the bar then is the mark, put over its tab each time it is placed, and left
    /// there; one there before and one of two stay.
    func testMoresEditIsWatchedWhileItsPanelIsOpen() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "the screen's updates, in which Exact looks") }
        guard #available(iOS 18.0, *) else { throw XCTSkip("no `UIUpdateLink`") }
        let (session, tabs, more) = try moreFixture("more-edit-watch")
        let mark = try XCTUnwrap(tabs as? TabController).mark, bar = tabs.tabBar
        let picture = UIImageView(image: UIImage(systemName: "star"))
        bar.addSubview(picture)
        XCTAssertFalse(mark.watching)
        openEdit(tabs, more)
        XCTAssertTrue(mark.watching)
        let updates = Updates(tabs.view)
        defer { updates.stop() }
        updates.wait(self, 3)
        let local = { self.controls(tabs).map(\.center) }
        let over = { self.controls(tabs).map { $0.superview!.convert($0.center, to: bar) } }
        func near(_ a: CGPoint, _ b: CGPoint) -> Bool { abs(a.x - b.x) <= 1 && abs(a.y - b.y) <= 1 }
        if let tab = local().indices.first(where: { i in !over().contains { near($0, local()[i]) } }) {
            picture.center = local()[tab]
            updates.wait(self, 3)
            XCTAssertTrue(near(picture.center, local()[tab]), "an image there before is no mark")
            let glow = UIImageView(image: UIImage(systemName: "circle.fill"))
            glow.center = local()[tab]
            bar.addSubview(glow)
            updates.wait(self, 3)
            XCTAssertTrue(near(glow.center, over()[tab]), "a mark goes over its tab")
            // Where the bar's layout makes of its place another's misplaced, UIKit has not placed it anew
            let stays = glow.center
            tabs.view.frame.size.width -= 60
            tabs.view.layoutIfNeeded()
            updates.wait(self, 3)
            XCTAssertEqual(glow.center, stays, "and stays there as the bar is laid out")
            glow.center = over()[0]
            updates.wait(self, 3)
            XCTAssertTrue(near(glow.center, over()[0]), "a mark UIKit has over a tab stays")
            let again = try XCTUnwrap(local().indices.first { i in !over().contains { near($0, local()[i]) } })
            glow.center = local()[again]
            updates.wait(self, 3)
            XCTAssertTrue(near(glow.center, over()[again]), "placed anew, it goes over its tab")
            let another = UIImageView(image: UIImage(systemName: "circle.fill"))
            bar.addSubview(another)
            glow.center = local()[again]
            updates.wait(self, 3)
            XCTAssertTrue(near(glow.center, local()[again]), "one of two is no mark")
            for view in [another, glow] { view.removeFromSuperview() }
        }
        picture.removeFromSuperview()
        try finishEdit(tabs)
        waitFor("the panel's end") { !mark.watching && !bar.isCustomizing }
        mark.watch(tabs) { true }
        waitFor("with no panel open there is nothing to watch") { !mark.watching }
        mark.watch(tabs) { false }
        XCTAssertFalse(mark.watching, "an app with native code")
        openEdit(tabs, more)
        XCTAssertTrue(mark.watching)
        session.view?.removeFromSuperview()
        XCTAssertFalse(mark.watching, "the container went")
    }

    /// The views outside the pages, and the bar's controls among them.
    private func outside(_ tabs: UITabBarController) -> [UIView] {
        let pages = Set(tabs.children.compactMap { $0.viewIfLoaded.map(ObjectIdentifier.init) })
        func all(_ view: UIView) -> [UIView] { pages.contains(ObjectIdentifier(view)) ? [] : [view] + view.subviews.flatMap(all) }
        return all(tabs.view)
    }

    private func controls(_ tabs: UITabBarController) -> [UIControl] {
        outside(tabs).map { $0.subviews.compactMap { $0 as? UIControl } }.first { $0.count == tabs.tabBar.items?.count } ?? []
    }

    /// Opens UIKit's panel from More's list.
    private func openEdit(_ tabs: UITabBarController, _ more: UINavigationController) {
        if tabs.selectedViewController !== more { tapTab(tabs, of: more) }
        waitFor("More's list") { more.topViewController === more.viewControllers.first && more.viewIfLoaded?.window != nil && !more.isNavigationBarHidden }
        let edit = more.viewControllers.first?.navigationItem.rightBarButtonItem
        edit?.target?.perform(edit?.action, with: edit)
        waitFor("UIKit's panel") { tabs.tabBar.isCustomizing }
    }

    /// Performs the Done of the panel's own bar, which no navigation controller owns.
    private func finishEdit(_ tabs: UITabBarController) throws {
        let own = outside(tabs).compactMap { $0 as? UINavigationBar }.filter { !($0.delegate is UINavigationController) && $0.topItem?.rightBarButtonItem?.action != nil }
        guard own.count == 1, let done = own.first?.topItem?.rightBarButtonItem, let target = done.target, let action = done.action else {
            throw XCTSkip("the panel's own bar, with Done, is not the one bar there is")
        }
        target.perform(action, with: done)
        waitFor("UIKit closes its panel") { !tabs.tabBar.isCustomizing }
    }
    #endif
}

#if !targetEnvironment(macCatalyst)
/// Where the first finger is, taking no touch.
private final class Finger: UIGestureRecognizer {
    private(set) var at: CGPoint?
    init() { super.init(target: nil, action: nil); cancelsTouchesInView = false; delaysTouchesEnded = false }
    override func touchesBegan(_ touches: Set<UITouch>, with event: UIEvent) { at = touches.first?.location(in: nil) }
    override func touchesMoved(_ touches: Set<UITouch>, with event: UIEvent) { at = touches.first?.location(in: nil) }
    override func touchesEnded(_ touches: Set<UITouch>, with event: UIEvent) { at = nil; state = .failed }
    override func touchesCancelled(_ touches: Set<UITouch>, with event: UIEvent) { at = nil; state = .failed }
}

/// Counts the screen's updates, and runs `each` after one.
@available(iOS 18.0, *)
private final class Updates {
    var each: () -> Void = {}
    private var count = 0
    private var link: UIUpdateLink?
    /// Whether updates come every frame, and not only as the screen asks for them.
    var continuous: Bool {
        get { link?.requiresContinuousUpdates == true }
        set { link?.requiresContinuousUpdates = newValue }
    }
    init(_ view: UIView) {
        let link = UIUpdateLink(view: view)
        link.addAction(to: .afterCATransactionCommit) { [weak self] _, _ in
            self?.count += 1
            self?.each()
        }
        link.requiresContinuousUpdates = true
        link.isEnabled = true
        self.link = link
    }
    func wait(_ test: XCTestCase, _ updates: Int) {
        let from = count
        test.waitFor("\(updates) updates") { count >= from + updates }
    }
    func stop() { link?.isEnabled = false }
}
#endif
#endif
