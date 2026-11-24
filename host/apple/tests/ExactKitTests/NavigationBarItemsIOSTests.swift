#if os(iOS)
import ObjectiveC
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// What `setRightBarButtonItems(_:animated:)` was asked, in order: UIKit
/// shows no animation of the bar's items in its layers to test for.
private var trailingSets: [Bool] = []

extension UINavigationItem {
    /// Swapped with `setRightBarButtonItems(_:animated:)`, so this name calls UIKit's.
    @objc fileprivate func recordedRightBarButtonItems(_ items: [UIBarButtonItem]?, animated: Bool) {
        trailingSets.append(animated)
        recordedRightBarButtonItems(items, animated: animated)
    }
}

extension NavigationBarIOSTests {
    func assertBarLayout(_ agent: Agent, _ node: NodeView, _ view: UIView, region: CGRect? = nil,
                         file: StaticString = #filePath, line: UInt = #line) throws {
        let reply = agent.layout(["id": Int(node.id)])
        let target = try XCTUnwrap(reply["node"] as? [String: Any], file: file, line: line)
        let space = try XCTUnwrap(target["space"] as? [String: Any], file: file, line: line)
        let expected = agent.box(view, region: region)
        XCTAssertFalse(expected.isEmpty, file: file, line: line)
        let viewport = ["x": Agent.r2(expected.minX), "y": Agent.r2(expected.minY), "w": Agent.r2(expected.width), "h": Agent.r2(expected.height)]
        XCTAssertEqual(space["viewport"] as? [String: Double], viewport, file: file, line: line)
        let row = try XCTUnwrap((reply["nodes"] as? [[String: Any]])?.first { $0["id"] as? Int == Int(node.id) }, file: file, line: line)
        for (key, value) in viewport { XCTAssertEqual(row[key] as? Double, value, key, file: file, line: line) }
        XCTAssertEqual((target["visible"] as? [String: Any])?["hidden"] as? Bool, false, file: file, line: line)
        XCTAssertNil(target["radii"], file: file, line: line)
    }

    func assertInlineTitleCopies(_ agent: Agent, _ heading: NodeView, in bar: UINavigationBar, at frame: CGRect) throws {
        let copy = UIControl(frame: frame.offsetBy(dx: 1 / bar.traitCollection.displayScale, dy: 0))
        let label = UILabel(frame: copy.bounds)
        label.text = bar.topItem?.title
        copy.addSubview(label)
        bar.addSubview(copy)
        defer { copy.removeFromSuperview() }
        func target() throws -> [String: Any] {
            try XCTUnwrap(agent.layout(["id": Int(heading.id)])["node"] as? [String: Any])
        }
        XCTAssertEqual((try target()["visible"] as? [String: Any])?["hidden"] as? Bool, false, "glass copies at one frame")
        copy.frame.origin.y += 5
        XCTAssertEqual((try target()["visible"] as? [String: Any])?["hidden"] as? Bool, true, "two distinct title frames")
    }

    func testLayoutDoesNotGiveAnUnidentifiedTabAnotherTabsControl() throws {
        let agent = Agent(session: try fixture("tab-identity", module: false))
        let bar = UITabBar(frame: CGRect(x: 0, y: 0, width: 300, height: 50))
        let first = UITabBarItem(title: "Shared", image: nil, tag: 1)
        let second = UITabBarItem(title: "Shared", image: nil, tag: 2)
        first.accessibilityIdentifier = "first"
        second.accessibilityIdentifier = "second"
        bar.items = [first, second]
        let other = UIButton(frame: CGRect(x: 160, y: 2, width: 100, height: 44))
        other.accessibilityIdentifier = "second"
        other.accessibilityLabel = "Shared"
        other.setTitle("Shared", for: .normal)
        bar.addSubview(other)
        XCTAssertFalse(agent.drawnTabControl(first, in: bar) === other)
        first.title = "First"
        other.setTitle("First", for: .normal)
        XCTAssertFalse(agent.drawnTabControl(first, in: bar) === other)
    }

    func testLayoutFollowsTheShowingBarAndItsItems() throws {
        let session = try fixture("bar-layout", module: false)
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation), bar = nav.navigationBar
        let agent = Agent(session: session), header = try node(session, "header-home"), button = try node(session, "compose-home")
        let item = try XCTUnwrap(nav.topViewController?.navigationItem.rightBarButtonItems?.first)
        try assertBarLayout(agent, header, bar)
        try assertBarLayout(agent, button, bar, region: XCTUnwrap(item.frame(in: bar)))
        let tabs = try XCTUnwrap(session.presenter.navigationHost.tabController)
        let shown = try XCTUnwrap(tabs.shownTabBar)
        try assertBarLayout(agent, node(session, "tabs"), shown)
        if shown === tabs.tabBar, !shown.traitCollection.placesVerticalBar {
            let first = try XCTUnwrap(InspectHost.buttonBoxes(in: shown).first)
            let hit = try XCTUnwrap(shown.hitTest(CGPoint(x: first.midX, y: first.midY), with: nil))
            let control = try XCTUnwrap(sequence(first: hit, next: \.superview).first { $0 is UIControl })
            try assertBarLayout(agent, node(session, "tab-home"), control)
        }
        nav.setNavigationBarHidden(true, animated: false)
        for target in [header, button] {
            let detail = try XCTUnwrap(agent.layout(["id": Int(target.id)])["node"] as? [String: Any])
            let space = try XCTUnwrap(detail["space"] as? [String: Any])
            XCTAssertEqual((space["viewport"] as? [String: Double])?["w"], 0)
            XCTAssertEqual((detail["visible"] as? [String: Any])?["hidden"] as? Bool, true)
        }
    }

    /// UIKit gives a tab's button its identifier only once an accessibility client asks, so one it makes after that (a
    /// tab out of More, as the bar widens) has its name alone.
    func testATouchAimsAtATabsButtonThatHasNoIdentifier() throws {
        guard Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit names its tab bar's buttons only for an assistive client") }
        let session = try fixture("tab-aim", module: false)
        let controller = try XCTUnwrap(session.presenter.navigationHost.tabController)
        guard let bar = controller.shownTabBar as? UITabBar, let title = bar.items?.first?.title else { throw XCTSkip("no UITabBar shows the tabs") }
        let place = bar.convert(bar.bounds, to: nil)
        var buttons: [UIControl] = [], pending: [UIView] = [controller.view]
        while let view = pending.popLast() {
            let middle = view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
            if let button = view as? UIControl, button.accessibilityLabel == title, place.contains(middle) {
                button.accessibilityIdentifier = nil
                buttons.append(button)
            } else if !(view is NodeView) {
                pending += view.subviews
            }
        }
        XCTAssertFalse(buttons.isEmpty)
        let aim = Agent(session: session).tabItem(try node(session, "tab-home"))
        guard case .success(let aimed)? = aim else { return XCTFail(String(describing: aim)) }
        XCTAssertTrue(buttons.contains { $0 === aimed })
    }

    /// A press that changes the bar of a route that shows sets its items with
    /// UIKit's animation, as a hand-built bar's
    /// `setRightBarButtonItems(_:animated: true)` does, whether the bar held
    /// controls or only a readout.
    func testABarsItemsChangedOnARouteThatShowsAreSetWithUIKitsAnimation() throws {
        let session = try fixture("bar-items-animated", module: false, plan: "BAR_SHARE")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        nav.view.layoutIfNeeded()
        let uikit = try XCTUnwrap(class_getInstanceMethod(UINavigationItem.self, #selector(UINavigationItem.setRightBarButtonItems(_:animated:))))
        let recorded = try XCTUnwrap(class_getInstanceMethod(UINavigationItem.self, #selector(UINavigationItem.recordedRightBarButtonItems(_:animated:))))
        method_exchangeImplementations(uikit, recorded)
        defer { method_exchangeImplementations(uikit, recorded) }
        trailingSets = []
        let items = { nav.topViewController?.navigationItem.rightBarButtonItems ?? [] }
        let more = try XCTUnwrap(items().first { $0.accessibilityIdentifier == "more-home" })
        // UIKit's send; a test process with no connected scene (a parallel worker's bare xctest) refuses it (false), so
        // there the item's own target and action, which UIKit's send calls
        if !UIApplication.shared.sendAction(try XCTUnwrap(more.action), to: more.target, from: more, for: nil) {
            _ = (more.target as AnyObject?)?.perform(more.action)
        }
        until("the press adds its control") { items().contains { $0.accessibilityIdentifier == "star-home" } }
        XCTAssertEqual(trailingSets, [true], "the items are set once, animated")
        // A bar left only a readout, then given its controls back: each change animates
        let toggle = try node(session, "bare-toggle").id
        trailingSets = []
        session.presenter.onPress?(toggle)
        until("the controls go, the readout stays") { !items().contains { $0.accessibilityIdentifier == "share-home" } }
        session.presenter.onPress?(toggle)
        until("the controls come back") { items().contains { $0.accessibilityIdentifier == "share-home" } }
        XCTAssertEqual(trailingSets, [true, true], "the readout-only bar's changes animate too")
    }

    /// The bar leaves a touch on none of its own controls to an authored
    /// title that lies under it, and keeps every other touch.
    func testTheBarLeavesATouchOnAnAuthoredTitleUnderItToTheTitle() throws {
        let session = try fixture("authored-title-touch", module: false)
        let p = session.presenter
        p.press(try node(session, "chat").id)
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation)
        until("chat is pushed") { nav.transitionCoordinator == nil }
        let bar = try XCTUnwrap(nav.navigationBar as? RouteNavigationBar)
        let tap = try node(session, "title-group")
        p.apply(wireBatch([["op": "style", "id": Int(tap.id), "style": ["appearance": "none", "written": ["appearance"]]]]))
        nav.view.layoutIfNeeded()
        // Where a route drawn under its bar would place it: across the bar's middle, which shows no title now.
        let middle = CGPoint(x: bar.bounds.midX, y: bar.bounds.midY)
        tap.frame = try XCTUnwrap(tap.superview).convert(CGRect(x: middle.x - 40, y: middle.y - 10, width: 80, height: 20), from: bar)
        XCTAssertTrue(bar.authoredControl(at: middle) === tap)
        XCTAssertNil(bar.hitTest(middle, with: nil), "the touch goes on to the title")
        let beside = CGPoint(x: middle.x + 80, y: middle.y)
        XCTAssertNil(bar.authoredControl(at: beside))
        XCTAssertNotNil(bar.hitTest(beside, with: nil), "the bar keeps its empty areas")
        tap.isHidden = true
        XCTAssertNil(bar.authoredControl(at: middle), "a hidden control takes no touch")
        tap.isHidden = false
        p.apply(wireBatch([["op": "style", "id": Int(tap.id), "style": ["appearance": "auto", "written": ["appearance"]]]]))
        XCTAssertNil(bar.authoredControl(at: middle), "a projected title is the bar's own")
    }

    /// A header control the navigation bar draws as its item is hidden in
    /// the page, and its `aria-keyshortcuts` chord is still a key command
    /// that presses it, as the same button's in the page is (#501).
    func testABarItemsShortcutIsAKeyCommandThatPressesIt() throws {
        let session = try fixture("bar-item-shortcut", module: false, plan: "BAR_SHARE")
        let nav = try XCTUnwrap(session.presenter.navigationHost.primaryNavigation)
        nav.view.layoutIfNeeded()
        let more = try node(session, "more-home")
        session.presenter.apply(wireBatch([["op": "props", "id": Int(more.id), "set": ["accessibilityKeyShortcuts": "Meta+Z"]]]))
        XCTAssertFalse(more.accessibilityVisible, "the bar's item stands for the control")
        XCTAssertNotNil(session.presenter.navigationHost.tipItem(more), "the bar that shows draws it")
        let p = session.presenter, toggle = try node(session, "bare-toggle")
        p.apply(wireBatch([
            ["op": "props", "id": Int(more.id), "set": ["id": "more", "accessibilityActions": "toggle"]],
            ["op": "props", "id": Int(toggle.id), "set": ["id": "toggle", "accessibilityLabel": "Toggle bar", "accessibilityActions": "more"]]
        ]))
        let item = try XCTUnwrap(p.navigationHost.tipItem(more)?.0)
        XCTAssertEqual(item.accessibilityCustomActions?.map(\.name), ["Toggle bar"])
        let fromPage = try XCTUnwrap(toggle.accessibilityCustomActions?.first)
        XCTAssertEqual(fromPage.name, "More")
        var presses: [UInt32] = []
        let saved = p.onPress
        p.onPress = { presses.append($0) }
        defer { p.onPress = saved }
        XCTAssertEqual(fromPage.actionHandler?(fromPage), true)
        let fromBar = try XCTUnwrap(item.accessibilityCustomActions?.first)
        XCTAssertEqual(fromBar.actionHandler?(fromBar), true)
        XCTAssertEqual(presses, [more.id, toggle.id])
        let commands = session.presenter.shortcutCommands(#selector(UIResponder.becomeFirstResponder))
        XCTAssertEqual(commands.map { [$0.input, $0.title] }, [["z", "More"]], "one command, named for the button")
        XCTAssertTrue(session.presenter.shortcut(key: "z", held: "Meta+", focus: nil) === more)
    }
}
#endif
