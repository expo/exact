#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

final class AgentBarPointerIOSTests: XCTestCase {
    private var session: ExactSession?
    private var window: UIWindow?
    private var animationsWereEnabled = true

    override func setUp() {
        super.setUp()
        animationsWereEnabled = UIView.areAnimationsEnabled
        UIView.setAnimationsEnabled(false)
    }

    override func tearDown() {
        session?.destroy(); session = nil
        window?.isHidden = true; window = nil
        UIView.setAnimationsEnabled(animationsWereEnabled)
        super.tearDown()
    }

    private func until(_ ready: () -> Bool) { waitFor("navigation reaches its observable ready state", ready) }

    private func fixture(_ key: String = "EXACT_FIXTURE_PLAN") throws -> ExactSession {
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(ProcessInfo.processInfo.environment[key])))
        let session = ExactApp.shared.makeSession(label: "bar-pointer")
        self.session = session
        let view = ExactView(session: session), host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = CGRect(x: 0, y: 0, width: 402, height: 874)
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        until {
            session.presenter.navigationHost.primaryNavigation?.topViewController is RouteController &&
                !Agent(session: session).nativeInFlight()
        }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, testId)
    }

    /// With UIKit's accessibility loaded, as under a real touch's runner:
    /// UIKit names a bar's controls.
    func testABarItemTheBarDoesNotShowIsRefused() throws {
        guard hostedWindowScene() != nil, Presenter.axRuntimeLoaded else { throw missing("accessibility", "UIKit names the control it shows for a bar item only with its accessibility loaded") }
        try checkABarItemIsPlacedOnlyAtItsOwnControl()
    }

    /// Without it, as a `tap` with no real touch finds the app: no control of
    /// the bar has a name.
    func testABarItemTheBarDoesNotShowIsRefusedWithoutAccessibility() throws {
        guard !Presenter.axRuntimeLoaded else { throw missing("bare process", "UIKit's accessibility is loaded here, and this covers a bar without it") }
        try checkABarItemIsPlacedOnlyAtItsOwnControl()
    }

    /// A bar item reporting the frame its test gives it, in its bar's space:
    /// the frames UIKit reports for an item it shows no control for.
    private final class PosedItem: UIBarButtonItem {
        var reports: () -> CGRect = { .zero }
        weak var bar: UIView?
        override func __frame(in referenceView: UIView) -> CGRect { bar?.convert(reports(), to: referenceView) ?? .zero }
    }

    /// A header of more buttons than the bar has room for, two of them
    /// without a test id (#738). A tap aims at and presses an item only at
    /// its own control. An item in the overflow menu is refused as there. An
    /// item the bar shows no control for is refused as not placed, whatever
    /// frame UIKit reports for it: one at the bar's leading edge, where a
    /// finger reaches Back (a hidden item's on iOS 27.0, as a dropped item's
    /// was reported on iOS 26.5), Back's own, or another item's.
    private func checkABarItemIsPlacedOnlyAtItsOwnControl() throws {
        let session = try fixture("EXACT_BAR_OVERFLOW_PLAN"), p = session.presenter, agent = Agent(session: session)
        p.press(try node(session, "open-board").id)
        until { p.navigationHost.primaryNavigation?.viewControllers.count == 2 && !agent.nativeInFlight() }
        try requireFront(window)
        let nav = try XCTUnwrap(p.navigationHost.primaryNavigation), bar = nav.navigationBar
        let navigationItem = try XCTUnwrap(nav.topViewController?.navigationItem)
        let appPress = p.onPress
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }
        typealias Entry = (item: UIBarButtonItem, node: NodeView, name: String)
        // The header's items by the node each presses; a space between groups presses none
        func entries() throws -> [Entry] {
            try (navigationItem.rightBarButtonItems ?? []).compactMap { item in
                guard let id = (item.target as? BarPress)?.id else { return nil }
                return (item, try XCTUnwrap(p.views[id]), item.accessibilityIdentifier ?? item.title ?? "#\(id)")
            }
        }
        func entry(_ name: String) throws -> Entry { try XCTUnwrap(entries().first { $0.name == name }, name) }
        func control(at point: CGPoint) -> UIControl? {
            sequence(first: window?.hitTest(point, with: nil), next: { $0?.superview }).lazy.compactMap { $0 as? UIControl }.first
        }
        // The title the control a finger at a window point reaches shows, which needs no accessibility to read
        func face(at point: CGPoint) -> String? {
            var pending: [UIView] = control(at: point).map { [$0] } ?? []
            while let v = pending.popLast() {
                if let text = (v as? UILabel)?.text { return text }
                pending.append(contentsOf: v.subviews)
            }
            return nil
        }
        func middle(_ item: UIBarButtonItem) -> CGPoint? { item.frame(in: bar).map { bar.convert(CGPoint(x: $0.midX, y: $0.midY), to: nil) } }
        func shows(_ item: UIBarButtonItem) -> Bool { middle(item).map { face(at: $0) == item.title } == true }
        func aim(_ node: NodeView) -> [String: Any] { agent.aim(["id": Int(node.id), "aim": ["press": true]]) }
        func assertPlaced(_ e: Entry) throws {
            let reply = aim(e.node)
            let at = try XCTUnwrap((reply["aim"] as? [String: Any])?["window"] as? [Double], "\(e.name): \(reply)"), point = CGPoint(x: at[0], y: at[1])
            if e.item.title?.isEmpty == false { XCTAssertEqual(face(at: point), e.item.title, "\(e.name): a finger there reaches the item's own control") }
            if Presenter.axRuntimeLoaded { XCTAssertEqual(control(at: point)?.accessibilityIdentifier, e.item.accessibilityIdentifier, "\(e.name): as UIKit names the control") }
            presses = []
            XCTAssertNil(agent.tap(["id": Int(e.node.id)])["error"], e.name)
            XCTAssertEqual(presses, [e.node.id], e.name)
        }
        func assertRefused(_ e: Entry, overflowed: Bool, _ why: String) {
            let error = aim(e.node)["error"] as? String ?? ""
            XCTAssertTrue(error.contains(overflowed ? "its bar item is in the navigation bar's overflow menu" : "its bar item is not placed in the navigation bar"), "\(e.name), \(why): \(error)")
            presses = []
            XCTAssertNotNil(agent.tap(["id": Int(e.node.id)])["error"], "\(e.name), \(why): the bar shows no control for it to activate")
            XCTAssertEqual(presses, [], "\(e.name), \(why)")
        }

        // More items than fit: UIKit shows the first and lists the rest under its overflow button. It lays the bar out
        // again some frames after a push: start once no item's answer has changed for a while
        bar.layoutIfNeeded()
        var answers: [String] = [], since = Date()
        until {
            let now = ((try? entries()) ?? []).map { aim($0.node)["error"] as? String ?? "" }
            if now != answers { (answers, since) = (now, Date()) }
            return Date().timeIntervalSince(since) > 0.5 && navigationItem.overflowPresentationSource != nil && !agent.nativeInFlight()
        }
        var placed: [String] = [], overflowed: [String] = []
        for e in try entries() {
            if aim(e.node)["error"] == nil { placed.append(e.name); try assertPlaced(e) } else { overflowed.append(e.name); assertRefused(e, overflowed: true, "crowded") }
        }
        XCTAssertTrue(placed.contains("item-add") && placed.contains("Plain"), "the bar shows its first items: \(placed)")
        XCTAssertTrue(overflowed.contains("item-31") && overflowed.contains("Spare"), "the bar has no room for its last: \(overflowed)")

        let omitted = try entry("item-31").node
        let reply = agent.layout(["id": Int(omitted.id)])
        let detail = try XCTUnwrap(reply["node"] as? [String: Any])
        let space = try XCTUnwrap(detail["space"] as? [String: Any])
        XCTAssertEqual((space["viewport"] as? [String: Double])?["w"], 0)
        let row = (reply["nodes"] as? [[String: Any]])?.first { $0["id"] as? Int == Int(omitted.id) }
        XCTAssertEqual(row?["w"] as? Double, 0)
        XCTAssertEqual((detail["visible"] as? [String: Any])?["hidden"] as? Bool, true, "More is not the item's box")

        // Every item fits
        p.onPress = appPress
        p.press(try node(session, "trim-items").id)
        p.onPress = { presses.append($0) }
        let kept = ["item-add", "item-31", "Spare"]
        // UIKit swaps the bar's controls over some frames after the items change
        let settled = { navigationItem.overflowPresentationSource == nil && !agent.nativeInFlight()
            && (try? entries())?.filter { $0.item.title?.isEmpty == false }.allSatisfy { shows($0.item) } == true }
        until { (try? entries()).map { Set($0.map(\.name)) } == Set(kept) && settled() }
        for e in try entries() { try assertPlaced(e) }

        // No control for an item, with and without a test id: UIKit reports a frame for it still
        for name in ["item-31", "Spare"] {
            let e = try entry(name)
            e.item.isHidden = true
            until { !agent.nativeInFlight() && !shows(e.item) }
            assertRefused(e, overflowed: false, "hidden, its frame \(String(describing: e.item.frame(in: bar)))")
            e.item.isHidden = false
            until(settled)
            try assertPlaced(e)
        }

        // The same, were UIKit to report another control's own frame for it
        let items = try XCTUnwrap(navigationItem.rightBarButtonItems)
        func assertRefused(_ name: String, reporting frame: @escaping () -> CGRect, _ why: String) throws {
            let e = try entry(name), posed = PosedItem(title: e.item.title, style: .plain, target: e.item.target, action: e.item.action)
            (posed.reports, posed.bar, posed.isHidden, posed.accessibilityIdentifier) = (frame, bar, true, e.item.accessibilityIdentifier)
            navigationItem.rightBarButtonItems = items.map { $0 === e.item ? posed : $0 }
            // The control there has the frame the item reports
            until { !agent.nativeInFlight() && middle(posed).flatMap(control(at:)).map { $0.convert($0.bounds, to: bar) } == frame() }
            assertRefused((posed, e.node, name), overflowed: false, why)
            navigationItem.rightBarButtonItems = items
            until(settled)
        }
        let back = {
            var controls: [UIView] = [], pending = bar.subviews
            while let v = pending.popLast() { if v is UIControl { controls.append(v) } else { pending.append(contentsOf: v.subviews) } }
            return controls.map { $0.convert($0.bounds, to: bar) }.min { $0.minX < $1.minX } ?? .zero
        }
        XCTAssertNotNil(bar.backItem, "the pushed route's bar shows Back")
        // A vertical bar holds Back, outside this one
        if !bar.traitCollection.placesVerticalBar {
            for name in ["item-31", "Spare"] { try assertRefused(name, reporting: back, "reporting Back's frame") }
        }
        let numbered = try entry("item-31").item
        try assertRefused("Spare", reporting: { numbered.frame(in: bar) ?? .zero }, "reporting another item's frame")

        // A control over an item with its box, which UIKit names as another's
        let spare = try entry("Spare"), other = UIControl(frame: try XCTUnwrap(spare.item.frame(in: bar)))
        other.accessibilityIdentifier = "BackButton"
        bar.addSubview(other)
        XCTAssertTrue((aim(spare.node)["error"] as? String)?.contains("covers its middle") == true)
        presses = []
        XCTAssertNotNil(agent.tap(["id": Int(spare.node.id)])["error"])
        XCTAssertTrue(presses.isEmpty)
        other.removeFromSuperview()

        for e in try entries() { try assertPlaced(e) }
    }

    private func assertVisibleSegmentPoint(_ session: ExactSession, _ testId: String, inBar: Bool = true) throws {
        let p = session.presenter, tab = try node(session, testId), agent = Agent(session: session)
        func control() -> UISegmentedControl? {
            let drawn = inBar ? p.navigationHost.barSegment(tab) : p.segments.drawn(tab)
            return drawn?.control as? UISegmentedControl
        }
        until { control()?.window != nil && !agent.nativeInFlight() }
        XCTAssertEqual(p.navigationHost.barSegment(tab) != nil, inBar)
        let segments = try XCTUnwrap(control())
        let index = try XCTUnwrap((0..<segments.numberOfSegments).first { segments.titleForSegment(at: $0) == tab.accessibleName })
        let views = segments.segmentViews
        XCTAssertEqual(views.count, segments.numberOfSegments)
        let view = try XCTUnwrap(views.indices.contains(index) ? views[index] : nil)
        let point = view.convert(CGPoint(x: view.bounds.midX, y: view.bounds.midY), to: nil)
        let expected = p.client(point)
        var samples: [PointerSample] = []
        p.onPointer = { _, _, sample in samples.append(sample) }
        let reply = agent.tap(["id": Int(tab.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(reply["native"] as? String, "segmented-control")
        XCTAssertEqual(samples.count, 2)
        for sample in samples {
            XCTAssertEqual(sample.clientX, Double(expected.x), accuracy: 0.5, testId)
            XCTAssertEqual(sample.clientY, Double(expected.y), accuracy: 0.5, testId)
        }
        p.onPointer = nil
    }

    func testATitleSegmentReportsItsVisibleMidpoint() throws {
        let session = try fixture()
        try node(session, "navigation").handlers.formUnion(["pointerdown", "pointerup"])
        try assertVisibleSegmentPoint(session, "segment-missed")
    }

    func testSearchScopeAndSubtitleSegmentsReportTheirVisibleMidpoints() throws {
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "search and subtitle segments are placed by a scene's bar") }
        let session = try fixture("EXACT_SEARCH_SCOPE_PLAN")
        try node(session, "app").handlers.formUnion(["pointerdown", "pointerup"])
        let inBar = UIDevice.current.userInterfaceIdiom == .phone
        try assertVisibleSegmentPoint(session, "filter-mine", inBar: inBar)
        session.presenter.press(try node(session, "pick-plain").id)
        try assertVisibleSegmentPoint(session, "filter-done", inBar: inBar)
    }

    /// LLP 1141.004 D3: a hosted item is the element's own view in the bar, so
    /// the agent's tap lands on it where the bar shows it, its pointer events
    /// bubble to its logical ancestors, and nothing presses it.
    func testAHostedItemBubblesPointersWithoutPressing() throws {
        let session = try fixture(), p = session.presenter, agent = Agent(session: session)
        p.press(try node(session, "open-plain").id)
        until { (p.navigationHost.primaryNavigation?.topViewController as? RouteController)?.node.props["testId"] == "route-plain" && !p.navigationHost.inTransition }
        try node(session, "navigation").handlers.formUnion(["pointerdown", "pointerup"])
        let readout = try node(session, "readout-plain")
        let route = try XCTUnwrap(p.navigationHost.primaryNavigation?.topViewController as? RouteController)
        let item = try XCTUnwrap(route.navigationItem.rightBarButtonItems?.first { ($0.customView as? HostedItemView)?.node === readout }, "the readout is a hosted item")
        var events: [String] = [], presses: [UInt32] = []
        p.onPointer = { _, kind, sample in
            events.append(kind == .down ? "down" : "up")
            let expected = p.client(readout.convert(CGPoint(x: readout.bounds.midX, y: readout.bounds.midY), to: nil))
            XCTAssertEqual(sample.clientX, Double(expected.x), accuracy: 0.5)
            XCTAssertEqual(sample.clientY, Double(expected.y), accuracy: 0.5)
        }
        p.onPress = { presses.append($0) }
        events = []
        let reply = agent.tap(["id": Int(readout.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(events, ["down", "up"])
        XCTAssertTrue(presses.isEmpty, "a readout has no press action")
        // A hosted item the bar does not show has no place for a pointer
        item.isHidden = true
        until { readout.window == nil }
        events = []
        XCTAssertNotNil(agent.tap(["id": Int(readout.id)])["error"], "the bar shows no view for the readout")
        XCTAssertEqual(events, [])
    }

    func testABarSegmentRechecksARIAAfterPointerHandlers() throws {
        let session = try fixture(), p = session.presenter
        let tab = try node(session, "segment-missed"), root = try node(session, "navigation")
        root.handlers.formUnion(["pointerdown", "pointerup"])
        var presses: [UInt32] = [], downs = 0
        p.onPointer = { _, kind, _ in
            if kind == .down {
                downs += 1
                p.apply(wireBatch([["op": "props", "id": tab.id, "set": ["accessibilityDisabled": "true"]]]))
            }
        }
        p.onPress = { presses.append($0) }
        XCTAssertNotNil(p.navigationHost.barSegment(tab))
        _ = Agent(session: session).tap(["id": Int(tab.id)])
        XCTAssertEqual(downs, 1)
        XCTAssertTrue(tab.ariaUnavailable)
        XCTAssertTrue(presses.isEmpty, "the agent applies the bar segment's eligibility after its observers")
    }

    func testARIAOnABarItemRemainsAnAccessibilityStateAfterPointers() throws {
        let session = try fixture(), p = session.presenter
        let item = try node(session, "compose-home"), root = try node(session, "navigation")
        root.handlers.formUnion(["pointerdown", "pointerup"])
        var presses: [UInt32] = [], downs = 0
        p.onPointer = { _, kind, _ in
            if kind == .down {
                downs += 1
                p.apply(wireBatch([["op": "props", "id": item.id, "set": ["accessibilityDisabled": "true"]]]))
            }
        }
        p.onPress = { presses.append($0) }
        let reply = Agent(session: session).tap(["id": Int(item.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(downs, 1)
        XCTAssertTrue(item.ariaUnavailable)
        XCTAssertEqual(presses, [item.id], "ARIA state leaves the native item pressable")
    }

    func testABarItemIsTappedWhereUIKitDrawsItWhereverItsHiddenBoxLies() throws {
        let session = try fixture(), p = session.presenter, agent = Agent(session: session)
        let item = try node(session, "compose-home"), header = try node(session, "header-home")
        guard case .success? = agent.barItem(item) else { return XCTFail("UIKit placed the item in the bar") }
        // The header the bar replaces is hidden and laid out as `display: none`,
        // so its box is no place on the screen; move it off the viewport
        XCTAssertTrue(header.isHidden)
        header.frame.origin = CGPoint(x: -500, y: -500)
        XCTAssertFalse(p.reach(of: item).contains(CGPoint(x: agent.box(item).midX, y: agent.box(item).midY)))
        var presses: [UInt32] = []
        p.onPress = { presses.append($0) }
        let reply = agent.tap(["id": Int(item.id)])
        XCTAssertNil(reply["error"], "\(reply)")
        XCTAssertEqual(reply["native"] as? String, "bar-button-item")
        XCTAssertEqual(presses, [item.id])
    }
}
#endif
