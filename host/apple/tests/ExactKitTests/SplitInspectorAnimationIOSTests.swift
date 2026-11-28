#if os(iOS)
import UIKit
import XCTest
import ExactCore
@testable import ExactKit
@testable import ExactNavigation
@testable import ExactGroupedLists

extension SplitIOSTests {
    /// LLP 1127 D14, as UIKit's own inspector comes and goes, on an iPhone
    /// Duo's width, where it hides the list: the column slides in over
    /// UIKit's animation with its route laid out where it ends, still, while
    /// the detail's content moves to its new width inside that animation and
    /// the list slides away in it, keeping its sidebar's look. Closed, the
    /// column slides out with the route's last pixels, the detail moving back
    /// with it.
    func testTheInspectorsColumnMovesWithTheDetailAndKeepsItsRoute() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        let session = try boot(size: CGSize(width: 951, height: 669))
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("a split") { navigation.split != nil && !agent.nativeInFlight() }
        let split = try XCTUnwrap(navigation.split)
        split.controller.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        let books = try node(session, "books"), notes = try node(session, "notes-a")
        let wide = notes.frame
        let list = try XCTUnwrap(split.list.view), whole = try XCTUnwrap(split.controller.view)
        XCTAssertNotEqual(split.controller.displayMode, .secondaryOnly, "the list shows beside the detail")
        let shown = Self.showing(list, in: whole)
        XCTAssertNil(agent.tap(["id": Int(try node(session, "info-a").id)])["error"])
        // UIKit puts the column in place as it lays the columns out, inside its animation (`SplitIOS.moveAlong`)
        until("its info is in the inspector's column") { split.inspecting?.navigationController?.view.superview != nil }
        let route = try XCTUnwrap(split.inspecting), info = try node(session, "info-page")
        let column = try XCTUnwrap(route.navigationController?.view.superview)
        let slide = try XCTUnwrap(Self.longest(column.layer), "the column slides in")
        let leaving = try XCTUnwrap(Self.longest(from: list.layer, upTo: whole.layer), "the list's column slides out")
        XCTAssertEqual(leaving.duration, slide.duration, accuracy: 0.01, "inside the animation that slides the inspector's column in")
        // Where the screen shows the list while the inspector's column still moves: part of the way out, not gone at once
        var partWay = false
        until("the list is seen sliding, or the inspector's column has stopped") {
            let now = Self.showing(list, in: whole)
            partWay = now.minX < shown.minX - 1 && now.maxX > shown.minX + 1
            return partWay || Self.longest(column.layer) == nil
        }
        XCTAssertTrue(partWay, "the list slides out as the inspector's column slides in")
        XCTAssertEqual(info.frame.width, route.view.bounds.width, "its route is laid out at the column's width")
        for view in [info] + info.subviews { XCTAssertNil(Self.longest(view.layer), "its route is laid out still: \(view)") }
        let moving = try XCTUnwrap(Self.longest(notes.layer), "the detail's content moves inside the column's animation")
        XCTAssertEqual(moving.duration, slide.duration, accuracy: 0.01)
        XCTAssertNotEqual(notes.frame.width, wide.width, "to its width beside the inspector")
        XCTAssertEqual(split.controller.displayMode, .secondaryOnly, "the inspector hides the list")
        XCTAssertTrue(navigation.showsAsSidebar(books), "the list keeps its sidebar's look as it slides away")

        until("the column is in") { split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "info-close").id)])["error"])
        until("the info's route is gone") { !session.presenter.views.values.contains { $0.props["testId"] == "info-page" } }
        // UIKit opens the close's animation as it lays the columns out (`SplitIOS.moveAlong`)
        until("the detail's content moves back inside the column's animation") { Self.longest(notes.layer) != nil }
        XCTAssertTrue(route.view.window != nil && !route.view.subviews.isEmpty, "the column goes with the route's last pixels")
        until("closed") { split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        XCTAssertEqual(notes.frame.width, wide.width, accuracy: 0.5)
    }

    /// LLP 1127 D7: a route UIKit shows in the inspector's column, its author having written no ground, is on the
    /// column's glass, as a hand-built inspector whose views paint nothing is: its controller and its box paint
    /// nothing, nor does its grouped list's box, and the list's collection view keeps the clear ground UIKit gives a
    /// list there. Collapsed, the route is a sheet on the grouped ground its grouped scroll gives it (LLP 1116.007 D5),
    /// and UIKit's grey is the list's; expanded again, the column's glass.
    func testARouteInTheInspectorsColumnIsOnTheColumnsGlassAndASheetsGroundFolded() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        guard hostedWindowScene() != nil else { throw missing("scene", "folded, UIKit presents the info's sheet in its window scene") }
        let (session, agent, split) = try inspectedNotes()
        let info = try node(session, "info-page"), band = try node(session, "info-band"), owner = try node(session, "info-facts")
        let lists = session.presenter.groupedLists as! GroupedListHost
        let list = try XCTUnwrap(lists.lists[owner.id], "the info's grouped list")
        func controller() -> RouteController? { sequence(first: info as UIResponder, next: \.next).first { $0 is RouteController } as? RouteController }
        func alpha(_ colour: UIColor?, in view: UIView) -> CGFloat {
            var a: CGFloat = 0
            colour?.resolvedColor(with: view.traitCollection).getWhite(nil, alpha: &a)
            return a
        }
        func grounds() -> [String: CGFloat] {
            list.collection.layoutIfNeeded()
            let route = controller()
            return ["route's controller": route.map { alpha($0.view.backgroundColor, in: $0.view) } ?? -1,
                    "route's box": info.layer.backgroundColor?.alpha ?? 0, "band's box": band.layer.backgroundColor?.alpha ?? 0,
                    "list's box": owner.layer.backgroundColor?.alpha ?? 0,
                    "list's collection view": alpha(list.collection.backgroundColor, in: list.collection)]
        }
        func onGlass() -> Bool { grounds().values.allSatisfy { $0 == 0 } }
        func transitionFacts() -> String {
            let c = split.controller.transitionCoordinator
            let from = c?.viewController(forKey: .from).map { "\(type(of: $0))" } ?? "none", to = c?.viewController(forKey: .to).map { "\(type(of: $0))" } ?? "none"
            let root = split.controller.view.window?.rootViewController
            var presented: [String] = []
            var next = root?.presentedViewController
            while let p = next { presented.append("\(type(of: p)) beingPresented \(p.isBeingPresented) in a window \(p.viewIfLoaded?.window != nil)"); next = p.presentedViewController }
            return "transition from \(from) to \(to) style \(c?.presentationStyle.rawValue ?? -9) animated \(c?.isAnimated ?? false) cancelled \(c?.isCancelled ?? false), "
                + "split's own \(split.controller.isBeingPresented), presented \(presented), window \(split.controller.view.window?.frame ?? .zero), "
                + "split's view \(split.controller.view.frame), mode \(split.controller.displayMode.rawValue), inspector shown \(split.controller.isShowing(.inspector))"
        }
        // As a fold or an unfold of the Duo: the width class and the window's size change together
        let window = try XCTUnwrap(split.controller.view.window)
        func fold(open: Bool) {
            split.controller.traitOverrides.horizontalSizeClass = open ? .regular : .compact
            window.traitOverrides.horizontalSizeClass = open ? .regular : .compact
            window.updateTraitsIfNeeded()
            window.frame = CGRect(origin: .zero, size: open ? CGSize(width: 951, height: 669) : CGSize(width: 466, height: 678))
            window.rootViewController?.view.subviews.first?.frame = window.bounds
        }
        func facts() -> String {
            let route = controller()
            let chain = sequence(first: list.collection as UIView, next: \.superview).map { "\(type(of: $0))" }.suffix(12)
            return "collapsed \(split.controller.isCollapsed), inspecting \(split.inspecting != nil), route \(route.map { "\($0)" } ?? "none"), "
                + "parent \(route?.parent.map { "\(type(of: $0))" } ?? "none"), presenting \(route?.presentingViewController.map { "\(type(of: $0))" } ?? "none"), "
                + "in a window \(info.window != nil), transition \(split.controller.transitionCoordinator != nil), native \(agent.nativeInFlight()), "
                + "grounds \(grounds()), list in \(chain.joined(separator: " < ")), " + transitionFacts()
        }
        until("UIKit has laid the list out in the column") { !list.collection.visibleCells.isEmpty && info.window != nil }
        info.layer.displayIfNeeded(); owner.layer.displayIfNeeded()
        XCTAssertTrue(onGlass(), "in the column, nothing paints over its glass: \(grounds())")

        fold(open: false)
        waitFor("collapsed, the info is a sheet", cap: 20, facts: facts) {
            split.controller.isCollapsed && controller()?.presentingViewController != nil
                && info.window != nil && split.controller.transitionCoordinator == nil && !agent.nativeInFlight()
        }
        info.layer.displayIfNeeded(); owner.layer.displayIfNeeded()
        let folded = grounds()
        XCTAssertEqual(folded["route's box"], 1, "the sheet is on its route's grouped ground: \(folded)")
        XCTAssertEqual(folded["list's collection view"], 1, "UIKit's grey is the list's in a sheet: \(folded)")

        fold(open: true)
        waitFor("expanded, the info is in the column again", cap: 20, facts: facts) {
            !split.controller.isCollapsed && split.inspecting != nil && info.window != nil
                && split.controller.transitionCoordinator == nil && !agent.nativeInFlight()
        }
        info.layer.displayIfNeeded(); owner.layer.displayIfNeeded()
        XCTAssertTrue(onGlass(), "back in the column, on its glass again: \(grounds())")
    }

    private static func longest(_ layer: CALayer) -> CAAnimation? {
        (layer.animationKeys() ?? []).compactMap { layer.animation(forKey: $0) }.max { $0.duration < $1.duration }
    }

    /// The longest animation on `layer` or on a layer above it, below `root`. UIKit slides the list's column on a
    /// second layer of the column's view, above the view's own `layer`: no view's `layer` shows the slide (iOS 27.1).
    private static func longest(from layer: CALayer, upTo root: CALayer) -> CAAnimation? {
        sequence(first: layer, next: \.superlayer).prefix { $0 !== root }.compactMap(longest).max { $0.duration < $1.duration }
    }

    /// Where the screen shows `view` now, in `root`, with the animations above it under way.
    private static func showing(_ view: UIView, in root: UIView) -> CGRect {
        guard let shown = view.layer.presentation(), let within = root.layer.presentation() else { return view.convert(view.bounds, to: root) }
        return shown.convert(shown.bounds, to: within)
    }

    /// LLP 1127 D14: Exact's close of the inspector's column empties the
    /// column once UIKit's transition is over. An inspector shown again
    /// before then supersedes the close: it keeps its route, and the column's
    /// hiding is no person's close.
    func testAnInspectorShownAgainWhileItClosesKeepsItsRoute() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        let session = try boot(size: CGSize(width: 1100, height: 800))
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("a split") { navigation.split != nil && !agent.nativeInFlight() }
        let split = try XCTUnwrap(navigation.split)
        split.controller.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "info-a").id)])["error"])
        until("its info is in the inspector's column") { split.inspecting != nil && split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        let route = try XCTUnwrap(split.inspecting), nav = try XCTUnwrap(split.inspector)
        var reported = 0
        let report = split.inspectorClosed
        split.inspectorClosed = { reported += 1; report() }
        split.inspect(nil, in: nil)
        let closing = try XCTUnwrap(split.controller.transitionCoordinator, "the column closes in a transition")
        // Registered after the close's own completion, so run after it.
        var closed = false
        closing.animate(alongsideTransition: nil) { _ in closed = true }
        split.inspect(route, in: nav)
        until("the close's transition is over") { closed && split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.05)) // the delegate's reports, queued
        XCTAssertTrue(nav.viewControllers.first === route, "the inspector keeps its route: \(nav.viewControllers)")
        XCTAssertTrue(split.inspector === nav)
        XCTAssertTrue(split.controller.isShowing(.inspector), "the column shows again")
        XCTAssertEqual(reported, 0, "the superseded close is no person's")
    }

    func testTheAgentsBackUnderTheInspectorEndsWhereUIKitsBackDoes() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let (session, agent, split) = try inspectedNotes()
        let book = try XCTUnwrap(split.detail.viewControllers.first)
        XCTAssertNil(agent.back()["error"])
        until("the inspector and notes close") { split.detail.topViewController === book && split.inspecting == nil }
    }

    /// UIKit's pop of the detail's top, the Back button's, while the inspector's column shows the visit above it goes
    /// back past both, and stays there: the pop's meaning is read as it began, though UIKit has emptied the column by
    /// its end (the inspector, LLP 1127 §7)
    func testUIKitsPopUnderTheInspectorGoesBackPastBoth() throws {
        guard #available(iOS 26.0, *) else { throw XCTSkip("the inspector's column is iOS 26's") }
        guard UIApplication.shared.connectedScenes.first is UIWindowScene else { throw missing("scene", "UIKit animates a pop only in a window a scene holds") }
        let (session, agent, split) = try inspectedNotes()
        let navigation = session.presenter.navigationHost, visits = navigation.shownKeys
        let book = try XCTUnwrap(split.detail.viewControllers.first)
        split.detail.popViewController(animated: true)
        until("the notes and their info are gone") { split.detail.topViewController === book && split.inspecting == nil && !agent.nativeInFlight() }
        waitForMainQueue("a sync after the pop")
        until("the router follows and the columns stay") { navigation.shownKeys == Array(visits.prefix(2)) && split.detail.topViewController === book && !agent.nativeInFlight() }
        XCTAssertNil(split.inspecting)
    }

    /// The expanded split at the Duo's open width: the book in the detail,
    /// its notes over it, and the notes' info in the inspector's column.
    @available(iOS 26.0, *)
    func inspectedNotes() throws -> (ExactSession, Agent, SplitIOS) {
        let session = try boot(size: CGSize(width: 951, height: 669))
        let navigation = session.presenter.navigationHost, agent = Agent(session: session)
        until("a split") { navigation.split != nil && !agent.nativeInFlight() }
        let split = try XCTUnwrap(navigation.split)
        split.controller.traitOverrides.horizontalSizeClass = .regular
        until("expanded") { !split.controller.isCollapsed && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "row-a").id)])["error"])
        until("the book is in the detail") { split.shown.count == 2 && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "notes-a").id)])["error"])
        until("the notes are pushed") { split.shown.count == 3 && !agent.nativeInFlight() }
        XCTAssertNil(agent.tap(["id": Int(try node(session, "notes-info-a").id)])["error"])
        until("the notes' info is in the inspector's column") { split.inspecting != nil && split.controller.transitionCoordinator == nil && !agent.nativeInFlight() }
        return (session, agent, split)
    }
}
#endif
