#if os(iOS)
import UIKit
import XCTest
@testable import ExactKit
@testable import ExactNavigation

/// LLP 1075.003 §9.11: `navigationDetent`'s words as UIKit detents,
/// `fit-content` resolving to the route's content height within the sheet's
/// maximum, and a sheet presented over the native fixture following its rows.
final class ModalDetentIOSTests: XCTestCase {
    private var window: UIWindow?
    private var host: UIViewController?
    private var sessions: [ExactSession] = []

    override func tearDown() {
        for session in sessions { session.destroy() }
        sessions = []
        window?.isHidden = true
        window = nil
        host = nil
        super.tearDown()
    }

    private final class Context: NSObject, UISheetPresentationControllerDetentResolutionContext {
        let containerTraitCollection = UITraitCollection()
        let maximumDetentValue: CGFloat
        init(_ maximum: CGFloat) { maximumDetentValue = maximum }
    }

    func testTheWordsBecomeDetentsInOrder() {
        let detents = SheetDetents.detents("300 500 large nonsense") { 120 }
        XCTAssertEqual(detents.map(\.identifier.rawValue), ["authored-0", "authored-1", UISheetPresentationController.Detent.Identifier.large.rawValue])
        XCTAssertEqual(detents[0].resolvedValue(in: Context(700)), 300)
        XCTAssertEqual(SheetDetents.detents("fit-content large") { 120 }.map(\.identifier.rawValue), ["fit-content-0", UISheetPresentationController.Detent.Identifier.large.rawValue])
        XCTAssertTrue(SheetDetents.detents(nil) { nil }.isEmpty)
    }

    /// LLP 1075.003 §9.13: a percentage is that share of UIKit's maximum
    /// detent value, as RN's `sheetAllowedDetents: [0.45]` is.
    func testAPercentageIsAShareOfTheMaximum() {
        let large = UISheetPresentationController.Detent.Identifier.large.rawValue
        let detents = SheetDetents.detents("45% 70%") { nil }
        XCTAssertEqual(detents.map(\.identifier.rawValue), ["authored-0", "authored-1"])
        XCTAssertEqual(detents[0].resolvedValue(in: Context(700)), 315)
        XCTAssertEqual(detents[0].resolvedValue(in: Context(600)), 270, "read at each resolution, as the maximum changes")
        XCTAssertEqual(detents[1].resolvedValue(in: Context(700)), 490)
        // All of the maximum is `large` itself (Grok's review): one rest, not
        // two at one height, and the status bar's covering sheet (LLP 1105 D2).
        XCTAssertEqual(SheetDetents.detents("100%") { nil }.map(\.identifier.rawValue), [large])
        var refused: [Substring] = []
        XCTAssertEqual(SheetDetents.detents("45% 100% large", refused: { word, _ in refused.append(word) }) { nil }.map(\.identifier.rawValue), ["authored-0", large])
        XCTAssertEqual(refused, ["large"])
    }

    /// A point height stops at the maximum. Where a taller detent follows, a
    /// height the maximum reaches is inactive (UIKit's resolver answers nil),
    /// so "900 large" on a phone is `large` alone, not two rests at one
    /// height; on a taller screen it is 900 then `large`.
    func testAPointHeightNeverTiesTheDetentAfterIt() {
        let followed = SheetDetents.detents("900 large") { nil }[0]
        XCTAssertNil(followed.resolvedValue(in: Context(800)))
        XCTAssertNil(followed.resolvedValue(in: Context(900)))
        XCTAssertEqual(followed.resolvedValue(in: Context(1000)), 900)
        XCTAssertEqual(SheetDetents.detents("300 900") { nil }[0].resolvedValue(in: Context(800)), 300)
        XCTAssertEqual(SheetDetents.detents("900") { nil }[0].resolvedValue(in: Context(800)), 800, "alone, it stops at the maximum")
    }

    /// A bound value can carry what the compiler refuses in a literal; the
    /// words that name no detent are handed back for the journal, and the
    /// line names the value, the route and what the sheet uses.
    func testWordsThatNameNoDetentAreRefusedAloud() {
        var refused: [Substring] = []
        let detents = SheetDetents.detents("0 -4 NaN nan inf 0% 150% x% 300 large", refused: { word, reason in
            XCTAssertEqual(reason, .unknown)
            refused.append(word)
        }) { nil }
        XCTAssertEqual(refused, ["0", "-4", "NaN", "nan", "inf", "0%", "150%", "x%"])
        XCTAssertEqual(detents.count, 2)
        XCTAssertEqual(
            SheetDetents.refusal(detent: "-12", words: [("-12", .unknown)], owner: "route \"3\"", kept: false),
            "navigationDetent \"-12\" on route \"3\": \"-12\" is not large, medium, fit-content, a positive point height or a percentage above 0% and at most 100%; the sheet uses the platform's large detent")
    }

    /// LLP 1075.003 §9.13: UIKit takes detents shortest to tallest. "300 45%"
    /// is in order on an 800-point maximum (300, 360) and not on a 600 one
    /// (300, 270), so what a bound value keeps is one kind, ascending, with
    /// `large` last, which is in order on any screen; the rest is journaled.
    func testWhatIsKeptAscendsOnEveryScreen() {
        var refused: [(Substring, SheetDetents.DetentRefusal)] = []
        let refuse = { (word: Substring, reason: SheetDetents.DetentRefusal) in refused.append((word, reason)) }
        let large = UISheetPresentationController.Detent.Identifier.large.rawValue
        let kept = SheetDetents.detents("300 45% large", refused: refuse) { nil }
        XCTAssertEqual(kept.map(\.identifier.rawValue), ["authored-0", large])
        for value in ["medium 300", "500 300", "60% medium", "fit-content 300", "large medium", "large large"] {
            refused = []
            XCTAssertEqual(SheetDetents.detents(value, refused: refuse) { 120 }.count, 1, value)
            XCTAssertEqual(refused.map { "\($0.0)" }, [String(value.split(separator: " ")[1])], value)
            XCTAssertTrue(refused.allSatisfy { $0.1 == .order }, value)
        }
        let shares = SheetDetents.detents("30% medium 80% large") { nil }
        for maximum: CGFloat in [500, 600, 800] {
            let custom = [shares[0], shares[2]].compactMap { $0.resolvedValue(in: Context(maximum)) }
            XCTAssertEqual(custom, custom.sorted(), "at \(maximum)")
        }
        XCTAssertTrue(
            SheetDetents.refusal(detent: "300 45%", words: [("45%", .order)], owner: "route \"3\"", kept: true)
                .hasSuffix("\"45%\" is not taller than the detent before it on every screen (UIKit takes them shortest to tallest; a point height, a share of the maximum and the content's height do not mix, and large goes last); the sheet uses its other detents"))
    }

    func testFitContentIsTheContentHeightWithinTheMaximum() {
        var content: CGFloat? = 180.4
        let detent = SheetDetents.detents("fit-content") { content }[0]
        XCTAssertEqual(detent.resolvedValue(in: Context(700)), 181, "rounded up to a whole point")
        content = 900
        XCTAssertEqual(detent.resolvedValue(in: Context(700)), 700, "clamped to the sheet's maximum")
        // Read at each resolution: a grown route resolves anew.
        content = 260
        XCTAssertEqual(detent.resolvedValue(in: Context(700)), 260)
        // Not measured yet: the sheet's maximum, as the default large sheet.
        content = nil
        XCTAssertEqual(detent.resolvedValue(in: Context(700)), 700)
        content = 0
        XCTAssertEqual(detent.resolvedValue(in: Context(700)), 700)
    }

    private func spin(_ seconds: Double) { RunLoop.main.run(until: Date().addingTimeInterval(seconds)) }

    private func until(_ what: String, _ seconds: Double = 5, _ done: () -> Bool) {
        let deadline = Date().addingTimeInterval(seconds)
        while !done(), Date() < deadline { RunLoop.main.run(mode: .default, before: Date().addingTimeInterval(0.02)) }
        XCTAssertTrue(done(), what)
    }

    /// The native fixture booted in a window of the test host's scene: UIKit
    /// presents a sheet only from a window a scene holds.
    private func fixture(_ label: String, planKey: String = "EXACT_FIXTURE_PLAN") throws -> ExactSession {
        let env = ProcessInfo.processInfo.environment
        let plan = try Data(contentsOf: URL(fileURLWithPath: try XCTUnwrap(env[planKey], "build.mjs --test --ios compiles the fixture's plan")))
        let session = ExactApp.shared.makeSession(label: label)
        sessions.append(session)
        let view = ExactView(session: session)
        let host = UIViewController()
        let scene = UIApplication.shared.connectedScenes.first as? UIWindowScene
        let window = scene.map { UIWindow(windowScene: $0) } ?? UIWindow(frame: CGRect(x: 0, y: 0, width: 402, height: 874))
        window.frame = window.screen.bounds
        window.rootViewController = host
        window.makeKeyAndVisible()
        host.view.addSubview(view)
        self.window = window
        self.host = host
        XCTAssertNil(session.boot(plan: plan, size: window.bounds.size).error)
        view.frame = host.view.bounds
        view.layoutIfNeeded()
        spin(0.3)
        // A sheet waits for the first drawn frame, which this window never draws.
        if session.firstDrawMs == nil { session.firstDrawMs = ExactEnv.wall() }
        return session
    }

    private func node(_ session: ExactSession, _ testId: String) throws -> NodeView {
        try XCTUnwrap(session.presenter.views.values.first { $0.props["testId"] == testId }, "no \(testId)")
    }

    /// Pressed directly: the sheet's view is never laid out in this window,
    /// so a tap finds nothing to hit there.
    private func tap(_ session: ExactSession, _ testId: String) throws {
        session.presenter.press(try node(session, testId).id)
    }

    /// The sheet over the fixture, once presented.
    private func sheet() -> UIViewController? {
        var top = host?.presentedViewController
        while let next = top?.presentedViewController { top = next }
        return top
    }

    /// The fixture's menu: 16 points of padding around More, Fewer, its
    /// rows, Close and Close both, 44 points each.
    private func rows(_ n: Int) -> CGFloat { 16 + 44 * CGFloat(4 + n) + 16 }

    /// `invalidateDetents()` calls on any sheet while `body` runs. This
    /// window finishes no presentation transition (the sheet's view stays
    /// zero-sized), so the sheet's own height cannot be read here; what
    /// resizes it is the invalidation, counted by exchanging UIKit's method
    /// for the test's, which calls it.
    private func countingInvalidations(_ body: () throws -> Void) rethrows {
        let cls: AnyClass = UISheetPresentationController.self
        let original = class_getInstanceMethod(cls, #selector(UISheetPresentationController.invalidateDetents))!
        let counting = class_getInstanceMethod(cls, #selector(UISheetPresentationController.exactTestInvalidateDetents))!
        method_exchangeImplementations(original, counting)
        defer { method_exchangeImplementations(original, counting) }
        UISheetPresentationController.exactTestInvalidations = 0
        try body()
    }

    /// LLP 1115 D5: a sheet that declares no Back control still swipes down,
    /// and its dismissal goes back by the root's `navigate`, once.
    func testASheetWithNoBackControlStillSwipesDownAndGoesBackByNavigate() throws {
        try sheetSwipesDown(navigate: true)
    }

    /// The same with no `navigate` handler: the runner's own back, once.
    func testASheetWithNoBackControlAndNoNavigateHandlerIsTheRunnersOwnBack() throws {
        try sheetSwipesDown(navigate: false)
    }

    private func sheetSwipesDown(navigate: Bool) throws {
        let session = try fixture("sheet-back-always")
        let root = try node(session, "navigation")
        if !navigate { root.handlers.remove("navigate") }
        try tap(session, "open-note")
        until("the note is presented as a sheet") { sheet()?.sheetPresentationController != nil }
        let presented = try XCTUnwrap(sheet())
        let controller = try XCTUnwrap(presented.presentationController)
        let modals = session.presenter.modalHost
        XCTAssertTrue(modals.presentationControllerShouldDismiss(controller), "the swipe may dismiss it")
        // A finished swipe: UIKit dismisses the sheet, then tells its
        // delegate (this window finishes no transition, so it is told here).
        presented.presentingViewController?.dismiss(animated: false)
        modals.presentationControllerDidDismiss(controller)
        until("the router went back") {
            let text = session.agent(#"{"op":"state"}"#)
            let json = try? JSONSerialization.jsonObject(with: Data(text.utf8)) as? [String: Any]
            let nav = (json?["slots"] as? [String: Any])?["nav"] as? [String: Any]
            return ((nav?["tabs"] as? [[String: Any]])?.first?["stack"] as? [Any])?.count == 1
        }
        let journal = session.agent(#"{"op":"logs","since":0}"#)
        XCTAssertEqual(journal.components(separatedBy: "(follow)").count - 1, navigate ? 1 : 0, "navigate once, or never: \(journal)")
        XCTAssertEqual(journal.components(separatedBy: "host back").count - 1, navigate ? 0 : 1, "the runner's own back without a handler: \(journal)")
        XCTAssertFalse(journal.contains("modal dismissal refused"))
    }

    /// Through the real presenter: a row added or taken away reaches the
    /// presented sheet, whose detents are invalidated (the `content` op
    /// reaching `ModalHost.contentChanged`), and its detent resolves to the
    /// new height; it is the same sheet throughout.
    func testAFitContentSheetFollowsItsRows() throws {
        let session = try fixture("detent-follows")
        try countingInvalidations {
            try tap(session, "menu")
            until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
            let presented = try XCTUnwrap(sheet())
            let controller = try XCTUnwrap(presented.sheetPresentationController)
            XCTAssertEqual(controller.detents.map(\.identifier.rawValue), ["fit-content-0"])
            XCTAssertFalse(controller.prefersGrabberVisible, "one detent: no grabber")
            let detent = { controller.detents.first?.resolvedValue(in: Context(800)) }
            until("at its rows' height") { detent() == rows(1) }
            var seen = UISheetPresentationController.exactTestInvalidations
            for (press, n) in [("menu-more", 2), ("menu-more", 3), ("menu-fewer", 2)] {
                try tap(session, press)
                until("\(press): \(n) rows") { detent() == rows(n) }
                until("\(press) invalidates the detents") { UISheetPresentationController.exactTestInvalidations > seen }
                seen = UISheetPresentationController.exactTestInvalidations
            }
            XCTAssertTrue(sheet() === presented, "the same sheet, never presented again")
            // Taller than the sheet's maximum: the maximum.
            XCTAssertEqual(controller.detents.first?.resolvedValue(in: Context(100)), 100)
        }
    }

    func testAScrollingFitContentSheetMeasuresRowsOutsideItsCurrentBounds() throws {
        guard hostedWindowScene() != nil else {
            throw missing("scene", "the sheet's height follows its detent only once UIKit finishes presenting it, in a scene's window")
        }
        let session = try fixture("detent-scrolling", planKey: "EXACT_KEYBOARD_SHEET_PLAN")
        try tap(session, "open-menu")
        until("the scrolling menu is presented") { sheet()?.sheetPresentationController != nil }
        let presented = try XCTUnwrap(sheet())
        let controller = try XCTUnwrap(presented.sheetPresentationController)
        let route = try node(session, "scrolling-menu")
        let scroll = try XCTUnwrap(route.scroll)
        let routeView = try XCTUnwrap(session.presenter.navigationHost.controllers[route.id]?.view)
        let detent = try XCTUnwrap(controller.detents.first)
        until("the menu is fitted and settled") { route.fitContentHeight == 208 && !Agent(session: session).nativeInFlight() }
        let short = routeView.bounds.height
        controller.animateChanges { controller.selectedDetentIdentifier = .large }
        until("the menu expands to large") { routeView.bounds.height > short && !Agent(session: session).nativeInFlight() }
        XCTAssertEqual(detent.resolvedValue(in: Context(800)), 208)
        controller.animateChanges { controller.selectedDetentIdentifier = detent.identifier }
        until("the menu returns to its fitted height") { abs(routeView.bounds.height - short) < 1 && !Agent(session: session).nativeInFlight() }
        try countingInvalidations {
            for (press, natural) in [("more", 252), ("more", 296), ("fewer", 252)] {
                let seen = UISheetPresentationController.exactTestInvalidations
                try tap(session, press)
                until("the detent follows the rows") { detent.resolvedValue(in: Context(800)) == CGFloat(natural) }
                until("the sheet invalidates its detents") { UISheetPresentationController.exactTestInvalidations > seen }
                XCTAssertEqual(controller.selectedDetentIdentifier, detent.identifier)
            }
        }
        controller.animateChanges { controller.selectedDetentIdentifier = .large }
        var natural: CGFloat = 252
        for _ in 0..<20 {
            try tap(session, "more")
            natural += 44
            until("the natural height follows another row") { route.fitContentHeight == natural }
            XCTAssertEqual(controller.selectedDetentIdentifier, .large)
        }
        until("the large sheet settles") { !Agent(session: session).nativeInFlight() }
        XCTAssertGreaterThan(natural, routeView.bounds.height)
        XCTAssertLessThan(try node(session, "row-0").bounds.height, 44, "the real rows still shrink")
        XCTAssertLessThan(route.content.height, natural, "the natural height does not inflate the scroll extent")
        XCTAssertEqual(scroll.contentSize.height, max(route.content.height, route.scrollFloor(scroll)), accuracy: 1)
        for maximum: CGFloat in [100, 800] {
            XCTAssertEqual(detent.resolvedValue(in: Context(maximum)), maximum)
        }
        XCTAssertTrue(sheet() === presented)
    }

    /// `navigationGrabber` is the author's word over the platform's: a sheet
    /// of one height shows the grabber it says `visible`, and one of several
    /// hides it when it says `hidden`; `auto` is the platform's again.
    func testASheetsGrabberIsWhatItsRouteSays() throws {
        let session = try fixture("detent-grabber")
        try tap(session, "menu")
        until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
        let controller = try XCTUnwrap(sheet()?.sheetPresentationController)
        let route = try node(session, "route-menu")
        XCTAssertFalse(controller.prefersGrabberVisible, "one detent, unsaid: none")
        for (word, shown) in [("visible", true), ("hidden", false), ("auto", false)] {
            route.props["navigationGrabber"] = word
            session.presenter.modalHost.updatePermissions()
            XCTAssertEqual(controller.prefersGrabberVisible, shown, word)
        }
        route.props["navigationDetent"] = "medium large"
        session.presenter.modalHost.updatePermissions()
        XCTAssertTrue(controller.prefersGrabberVisible, "several detents, auto: the platform's grabber")
        // Dragged to large (a drag sets the selection): a grabber change
        // leaves the sheet where the person put it.
        controller.selectedDetentIdentifier = .large
        route.props["navigationGrabber"] = "hidden"
        session.presenter.modalHost.updatePermissions()
        XCTAssertFalse(controller.prefersGrabberVisible, "several detents, hidden")
        XCTAssertEqual(controller.selectedDetentIdentifier, .large, "still large")
    }

    /// `navigationInitialDetent` names the detent the sheet opens at among
    /// its `navigationDetent`'s, read when the detents are set. Unsaid, or
    /// naming none the sheet kept (a bound value, journaled), it is the
    /// first, as UIKit opens a sheet at its smallest; a change to it alone
    /// leaves the sheet where it is.
    func testASheetOpensAtTheDetentItsRouteNames() throws {
        let session = try fixture("detent-initial")
        try tap(session, "menu")
        until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
        let controller = try XCTUnwrap(sheet()?.sheetPresentationController)
        let route = try node(session, "route-menu")
        let set = { (detents: String, initial: String?) in
            route.props["navigationInitialDetent"] = initial
            route.props["navigationDetent"] = detents
            session.presenter.modalHost.updatePermissions()
        }
        for (detents, initial, opened) in [
            ("medium large", "large", UISheetPresentationController.Detent.Identifier.large),
            ("300 500 large", "500", .init("authored-1")),
            ("fit-content large", "fit-content", .init("fit-content-0")),
            ("medium 100%", "100%", .large),
            ("medium large", nil, .medium),
            ("medium large", "", .medium),
            ("", "large", .large),
        ] {
            set(detents, initial)
            XCTAssertEqual(controller.selectedDetentIdentifier, opened, "\(detents), \(initial ?? "unsaid")")
        }
        set("300 500", "45%")
        XCTAssertEqual(controller.selectedDetentIdentifier, .init("authored-0"), "none of its detents: the first")
        let journal = session.agent(#"{"op":"logs","since":0}"#)
        XCTAssertTrue(journal.contains(#"navigationInitialDetent \"45%\" on route"#) && journal.contains("the sheet opens at its first"), journal)
        XCTAssertFalse(journal.contains(#"navigationInitialDetent \"large\""#), "the platform's large is a sheet's one detent unsaid")
        XCTAssertFalse(journal.contains(#"navigationInitialDetent \"\""#), "empty is unsaid, as `navigationDetent`'s is")
        route.props["navigationInitialDetent"] = "500"
        session.presenter.modalHost.updatePermissions()
        XCTAssertEqual(controller.selectedDetentIdentifier, .init("authored-0"), "the initial detent alone changed: still the first")
        // A route that says no `navigationDetent`, as its sheet is made
        // (UIKit's `large`, nothing selected): a bound initial detent that
        // names none is journaled there too.
        route.props["navigationDetent"] = nil
        route.props["navigationInitialDetent"] = nil
        session.presenter.modalHost.updatePermissions()
        route.props["navigationInitialDetent"] = "medum"
        controller.selectedDetentIdentifier = nil
        session.presenter.modalHost.updatePermissions()
        XCTAssertEqual(controller.selectedDetentIdentifier, .large)
        XCTAssertTrue(session.agent(#"{"op":"logs","since":0}"#).contains(#"navigationInitialDetent \"medum\""#), "journaled when the sheet is made")
    }

    /// LLP 1123 D7: the detent a route holds its sheet at moves the sheet as it changes and again over new
    /// detents, while an unrelated update leaves the height the person dragged it to; let go, the sheet returns to
    /// the one it opens at. Its edge attachment and scroll expansion are UIKit's properties while it says them, and
    /// UIKit's defaults once it stops; its sizing is the presentation style it is made with.
    func testASheetRouteHoldsItsSelectedDetentAndSaysItsSheetsChoices() throws {
        let session = try fixture("detent-selected")
        try tap(session, "menu")
        until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
        let presented = try XCTUnwrap(sheet())
        let controller = try XCTUnwrap(presented.sheetPresentationController)
        XCTAssertEqual(presented.modalPresentationStyle, .pageSheet, "unsaid, a page sheet")
        let route = try node(session, "route-menu")
        func update(_ props: [String: String?]) {
            for (name, value) in props { route.props[name] = value }
            session.presenter.modalHost.updatePermissions()
        }
        update(["navigationDetent": "medium large", "navigationSelectedDetent": "large"])
        XCTAssertEqual(controller.selectedDetentIdentifier, .large)
        controller.selectedDetentIdentifier = .medium
        update(["navigationGrabber": "visible"])
        XCTAssertEqual(controller.selectedDetentIdentifier, .medium, "an unrelated update leaves the height the person dragged it to")
        update(["navigationSelectedDetent": "medium"])
        update(["navigationSelectedDetent": "large"])
        XCTAssertEqual(controller.selectedDetentIdentifier, .large, "a new value moves it")
        update(["navigationDetent": "300 large"])
        XCTAssertEqual(controller.selectedDetentIdentifier, .large, "held over new detents")
        update(["navigationSelectedDetent": "45%"])
        XCTAssertEqual(controller.selectedDetentIdentifier, .large, "a word that names none of its detents leaves it")
        XCTAssertTrue(session.agent(#"{"op":"logs","since":0}"#).contains(#"navigationSelectedDetent \"45%\" on route"#))
        update(["navigationSelectedDetent": nil])
        XCTAssertEqual(controller.selectedDetentIdentifier, .init("authored-0"), "let go: where it opens, its first")
        XCTAssertFalse(controller.prefersEdgeAttachedInCompactHeight)
        XCTAssertTrue(controller.prefersScrollingExpandsWhenScrolledToEdge, "unsaid over two detents: the platform's")
        XCTAssertFalse(controller.widthFollowsPreferredContentSizeWhenEdgeAttached)
        update(["navigationEdgeAttachedInCompactHeight": "true", "navigationWidthFollowsPreferredContentSize": "true", "navigationScrollExpands": "false"])
        XCTAssertTrue(controller.prefersEdgeAttachedInCompactHeight)
        XCTAssertTrue(controller.widthFollowsPreferredContentSizeWhenEdgeAttached)
        XCTAssertFalse(controller.prefersScrollingExpandsWhenScrolledToEdge)
        update(["navigationEdgeAttachedInCompactHeight": nil, "navigationWidthFollowsPreferredContentSize": nil, "navigationScrollExpands": nil])
        XCTAssertFalse(controller.prefersEdgeAttachedInCompactHeight, "UIKit's default once it stops")
        XCTAssertFalse(controller.widthFollowsPreferredContentSizeWhenEdgeAttached)
        XCTAssertTrue(controller.prefersScrollingExpandsWhenScrolledToEdge)
        // UIKit reads a style back as it resolves it for the device (on an iPhone a form sheet reads as a page
        // sheet), so the sheet is made as a controller given the same style is
        for (sizing, style) in [("form", UIModalPresentationStyle.formSheet), ("page", .pageSheet), (nil, .pageSheet)] {
            XCTAssertEqual(ModalController.sheetStyle(sizing), style, sizing ?? "unsaid")
            route.props["navigationSizing"] = sizing
            let made = ModalController(host: session.presenter.modalHost, route: route, kind: "modal", detent: nil, grabber: nil)
            let plain = UIViewController()
            plain.modalPresentationStyle = style
            XCTAssertEqual(made.modalPresentationStyle, plain.modalPresentationStyle, sizing ?? "unsaid")
            XCTAssertNotNil(made.sheetPresentationController, "\(sizing ?? "unsaid"): a sheet to configure")
        }
    }

    /// LLP 1123 D7: a sheet resting at or below its undimmed detent dims nothing and is no modal dialog: the screen
    /// under it takes a person's touch, its views are live to reads, and the sheet begins no modal boundary. Above
    /// it, or with no undimmed detent, the sheet is modal again.
    func testASheetAtOrBelowItsUndimmedDetentLeavesTheScreenUnderItLive() throws {
        guard ProcessInfo.processInfo.environment["EXACT_TOUCH_PORT"] != nil else {
            throw missing("touch", "a person's touch reaches the screen under an undimmed sheet")
        }
        let touch = try TouchRunner(self)
        defer { touch.connection.cancel() }
        let session = try fixture("detent-undimmed")
        let window = try XCTUnwrap(self.window), modals = session.presenter.modalHost
        try tap(session, "menu")
        until("the menu is presented as a sheet") { self.sheet()?.sheetPresentationController != nil && self.sheet()?.view.window != nil }
        let presented = try XCTUnwrap(sheet()), controller = try XCTUnwrap(presented.sheetPresentationController)
        let route = try node(session, "route-menu"), bump = try node(session, "bump")
        func update(_ props: [String: String?]) {
            for (name, value) in props { route.props[name] = value }
            modals.updatePermissions()
        }
        func count() -> Double? {
            let reply = try? JSONSerialization.jsonObject(with: Data(session.agent(#"{"op":"state"}"#).utf8)) as? [String: Any]
            return (reply?["slots"] as? [String: Any])?["count"] as? Double
        }
        // A short detent, so the sheet leaves Bump uncovered
        let short = UISheetPresentationController.Detent.Identifier("authored-0")
        update(["navigationDetent": "200 large", "navigationSelectedDetent": "200", "navigationUndimmedDetent": "200"])
        XCTAssertEqual(controller.largestUndimmedDetentIdentifier, short)
        until("the sheet rests at its short detent") { controller.selectedDetentIdentifier == short && presented.transitionCoordinator == nil }
        XCTAssertFalse(modals.inertBehind(bump), "the screen under it is live")
        XCTAssertNil(session.presenter.openTopLayer.1, "the sheet begins no modal boundary")
        XCTAssertNil(modals.topNavigation)
        // Bump takes touches where every view from it to the window does
        let takesTouches = { sequence(first: bump as UIView, next: \.superview).allSatisfy(\.isUserInteractionEnabled) }
        XCTAssertTrue(takesTouches())
        let center = bump.convert(CGPoint(x: bump.bounds.midX, y: bump.bounds.midY), to: window)
        XCTAssertLessThan(center.y, presented.view.convert(presented.view.bounds, to: window).minY, "Bump lies above the sheet")
        let before = try XCTUnwrap(count())
        waitFor("the window receives touches") { Agent.frontRefusal(window) == nil }
        XCTAssertEqual(try touch.ask(TouchRunner.aim(center, in: window).merging(["op": "tap"]) { _, new in new })["done"] as? Bool, true)
        until("a person's tap presses Bump under the sheet") { count() == before + 1 }
        // A person's drag takes it above its undimmed detent, and it is modal again
        let top = presented.view.convert(presented.view.bounds, to: window).minY
        var drag = try TouchRunner.aim(CGPoint(x: window.bounds.midX, y: top + 10), in: window)
        let end = try TouchRunner.aim(CGPoint(x: window.bounds.midX, y: 120), in: window)
        drag.merge(["op": "drag", "to": end["point"]!, "screenTo": end["screen"]!, "velocity": 900, "press": 0.05, "hold": 0.1]) { _, new in new }
        XCTAssertEqual(try touch.ask(drag)["done"] as? Bool, true)
        until("the person's drag takes the sheet to large") { controller.selectedDetentIdentifier == .large && presented.transitionCoordinator == nil }
        XCTAssertTrue(modals.inertBehind(bump), "above its undimmed detent the sheet is modal")
        XCTAssertFalse(takesTouches(), "the screen under it takes no touch")
        XCTAssertNotNil(session.presenter.openTopLayer.1)
        update(["navigationSelectedDetent": "large", "navigationUndimmedDetent": nil])
        XCTAssertNil(controller.largestUndimmedDetentIdentifier)
        XCTAssertTrue(modals.inertBehind(bump), "with no undimmed detent every height is modal")
    }

    func testADialogUsesItsAuthoredDetentsAndKeepsTheSelectionWhenItsGrabberChanges() throws {
        guard hostedWindowScene() != nil else {
            throw missing("scene", "UIKit presents and dismisses the dialog in its window")
        }
        let session = try fixture("dialog-detents", planKey: "EXACT_DIALOG_SHEET_READS_PLAN")
        let p = session.presenter
        p.dialogCommand("showModal", "resizable")
        until("dialog presented") { self.sheet() != nil && !p.menus.inTransition }
        let presented = try XCTUnwrap(sheet()), sheet = try XCTUnwrap(presented.sheetPresentationController)
        let dialog = try XCTUnwrap(p.views.values.first { $0.props["id"] == "resizable" })
        let footer = try node(session, "dialog-footer")
        let bottom = { footer.convert(footer.bounds, to: presented.view).maxY }
        let port = { presented.view.safeAreaLayoutGuide.layoutFrame.maxY }
        until("the footer reaches the large sheet's safe area") { abs(bottom() - port()) < 1 }
        let large = port()
        XCTAssertEqual(sheet.detents.map(\.identifier), [.medium, .large])
        XCTAssertEqual(sheet.selectedDetentIdentifier, .large)
        XCTAssertFalse(sheet.prefersGrabberVisible)
        XCTAssertEqual(presented.view.backgroundColor, .systemBackground, "at its large detent the sheet's ground is the system background")
        sheet.animateChanges { sheet.selectedDetentIdentifier = .medium }
        until("the footer follows the sheet to medium") { port() < large - 100 && abs(bottom() - port()) < 1 }
        // UIKit tells the delegate only of a detent a person dragged to.
        sheet.delegate?.sheetPresentationControllerDidChangeSelectedDetentIdentifier?(sheet)
        // In regular width the ground stays the system background, as the route's does.
        let below: UIColor = presented.traitCollection.horizontalSizeClass == .regular ? .systemBackground : .clear
        XCTAssertEqual(presented.view.backgroundColor, below, "below it a compact sheet's ground is left to UIKit's glass (LLP 1141.001 D3)")
        dialog.props["navigationGrabber"] = "visible"
        p.menus.sync()
        XCTAssertTrue(sheet.prefersGrabberVisible)
        XCTAssertEqual(sheet.selectedDetentIdentifier, .medium)
        p.dialogCommand("close", "resizable")
        until("dialog closed") { self.sheet() == nil && !p.menus.inTransition }
    }

    func testADialogsFitContentDetentFollowsItsOwnContents() throws {
        guard hostedWindowScene() != nil else { throw missing("scene", "UIKit resolves the dialog's resting heights") }
        let session = try fixture("fitted-dialog", planKey: "EXACT_DIALOG_SHEET_READS_PLAN"), p = session.presenter
        p.dialogCommand("showModal", "fitted-rests")
        until("fitted dialog presented") { self.sheet() != nil && !p.menus.inTransition }
        let sheet = try XCTUnwrap(sheet()?.sheetPresentationController)
        let fitted = try XCTUnwrap(sheet.detents.first)
        XCTAssertEqual(sheet.selectedDetentIdentifier, fitted.identifier)
        let below: UIColor = self.sheet()?.traitCollection.horizontalSizeClass == .regular ? .systemBackground : .clear
        XCTAssertEqual(self.sheet()?.view.backgroundColor, below, "a compact sheet that opens below its large detent opens on UIKit's glass")
        XCTAssertEqual(sheet.largestUndimmedDetentIdentifier, .large, "the authored backdrop replaces UIKit's dimming at every height")
        until("initial content height") { fitted.resolvedValue(in: Context(800)) == 100 }
        try tap(session, "grow-fitted")
        until("the content detent grows with its contents") { fitted.resolvedValue(in: Context(800)) == 300 }
        p.dialogCommand("close", "fitted-rests")
        until("fitted dialog closed") { self.sheet() == nil && !p.menus.inTransition }
    }

    /// Mounted, its route in its block (LLP 1123 D2), the sheet still
    /// follows its rows: its content is measured at the block's width,
    /// whatever height the block has.
    func testAMountedFitContentSheetFollowsItsRows() throws {
        let session = try fixture("detent-mounted")
        try countingInvalidations {
            try tap(session, "menu")
            until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
            let presented = try XCTUnwrap(sheet())
            let controller = try XCTUnwrap(presented.sheetPresentationController)
            // Put where UIKit's transition, which this window never finishes, puts it
            if presented.view.window == nil { window?.addSubview(presented.view) }
            presented.view.frame = CGRect(x: 0, y: 400, width: 402, height: 474)
            presented.view.layoutIfNeeded()
            let route = try node(session, "route-menu")
            let routeView = try XCTUnwrap(session.presenter.navigationHost.controllers[route.id]?.view)
            until("the route has its block") { session.presenter.owners.told[route.id]?.block == .size(routeView.bounds.size) && !routeView.bounds.isEmpty }
            let detent = { controller.detents.first?.resolvedValue(in: Context(800)) }
            until("at its rows' height") { detent() == rows(1) }
            var seen = UISheetPresentationController.exactTestInvalidations
            for (press, n) in [("menu-more", 2), ("menu-more", 3), ("menu-fewer", 2)] {
                try tap(session, press)
                until("\(press): \(n) rows") { detent() == rows(n) }
                until("\(press) invalidates the detents") { UISheetPresentationController.exactTestInvalidations > seen }
                seen = UISheetPresentationController.exactTestInvalidations
            }
        }
    }

    /// `"fit-content large"`: the sheet starts at its rows, with the grabber;
    /// a content change invalidates the detents and keeps whichever is
    /// selected.
    func testFitContentLargeKeepsTheSelectedDetent() throws {
        let session = try fixture("detent-large")
        try tap(session, "menu-large")
        try countingInvalidations {
            try tap(session, "menu")
            until("the menu is presented as a sheet") { sheet()?.sheetPresentationController != nil }
            let controller = try XCTUnwrap(sheet()?.sheetPresentationController)
            XCTAssertEqual(controller.detents.map(\.identifier), [.init("fit-content-0"), .large])
            XCTAssertTrue(controller.prefersGrabberVisible)
            XCTAssertEqual(controller.selectedDetentIdentifier, .init("fit-content-0"), "it starts at its rows")
            let detent = { controller.detents.first?.resolvedValue(in: Context(800)) }
            until("at its rows' height") { detent() == rows(1) }
            var seen = UISheetPresentationController.exactTestInvalidations
            try tap(session, "menu-more")
            until("a row added") { detent() == rows(2) && UISheetPresentationController.exactTestInvalidations > seen }
            XCTAssertEqual(controller.selectedDetentIdentifier, .init("fit-content-0"))
            // Dragged to large (a drag sets the selection), a row added keeps it.
            controller.selectedDetentIdentifier = .large
            seen = UISheetPresentationController.exactTestInvalidations
            try tap(session, "menu-more")
            until("another row") { detent() == rows(3) && UISheetPresentationController.exactTestInvalidations > seen }
            XCTAssertEqual(controller.selectedDetentIdentifier, .large, "still large")
        }
    }
}

private extension UISheetPresentationController {
    static var exactTestInvalidations = 0
    /// Exchanged with `invalidateDetents()`: after the exchange this name
    /// runs UIKit's own.
    @objc func exactTestInvalidateDetents() {
        Self.exactTestInvalidations += 1
        exactTestInvalidateDetents()
    }
}
#endif
